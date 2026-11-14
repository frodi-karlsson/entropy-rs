//! Random well-typed programs must give the same stdout, stderr and exit code on the reference
//! evaluator and `entropy run`. Every 10th also goes through `entropy build` (spec 9.3).
//!
//! `PROPTEST_CASES` sets the number of cases (default 64).

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const REALS: &[&str] = &["r0", "r1", "r2"];
const CHARS: &[&str] = &["c0", "c1"];
const STRINGS: &[&str] = &["s0", "s1"];

/// An expression, rendered fully parenthesised.
#[derive(Clone, Debug)]
enum Expr {
    Leaf(String),
    Arith(&'static str, Box<Expr>, Box<Expr>),
    Concat(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn render(&self) -> String {
        match self {
            Expr::Leaf(s) => s.clone(),
            Expr::Arith(op, a, b) => format!("({} {op} {})", a.render(), b.render()),
            Expr::Concat(a, b) => format!("({} + {})", a.render(), b.render()),
        }
    }
}

fn leaf(options: &'static [&'static str]) -> BoxedStrategy<Expr> {
    prop::sample::select(options)
        .prop_map(|s| Expr::Leaf(s.to_string()))
        .boxed()
}

/// A real or char expression.
fn number() -> BoxedStrategy<Expr> {
    let base = prop_oneof![
        leaf(REALS),
        leaf(CHARS),
        leaf(&["0", "1", "2", "3", "0.5", "10", "65", "100"]),
        leaf(&["'a'", "'Z'", "' '", "'0'"]),
    ];
    base.prop_recursive(3, 12, 2, |inner| {
        (
            prop::sample::select(&["+", "-", "*", "/", "%"][..]),
            inner.clone(),
            inner,
        )
            .prop_map(|(op, a, b)| Expr::Arith(op, Box::new(a), Box::new(b)))
    })
    .boxed()
}

fn string() -> BoxedStrategy<Expr> {
    let base = prop_oneof![
        leaf(STRINGS),
        leaf(&["\"\"", "\"hi\"", "\"x y\"", "\"\\n\"", "\"é\""]),
    ];
    base.prop_recursive(2, 8, 2, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone())
                .prop_map(|(a, b)| Expr::Concat(Box::new(a), Box::new(b))),
            (inner.clone(), number()).prop_map(|(a, b)| Expr::Concat(Box::new(a), Box::new(b))),
            (number(), inner).prop_map(|(a, b)| Expr::Concat(Box::new(a), Box::new(b))),
        ]
    })
    .boxed()
}

fn any_expr() -> BoxedStrategy<Expr> {
    prop_oneof![number(), string()].boxed()
}

#[derive(Clone, Debug)]
enum Stmt {
    Assign(&'static str, Expr),
    Compound(&'static str, &'static str, Expr),
    Print(Expr),
    If(String, Vec<Stmt>),
    While(String, Vec<Stmt>),
    /// A string declared in the current block: `declare t; let t = e; print t + u;`.
    Scoped(Expr, Expr),
}

/// Mostly plain operands, which are often equal at rate 0, so `<` and `<=` behave differently.
fn operand() -> BoxedStrategy<Expr> {
    prop_oneof![
        3 => leaf(REALS),
        2 => leaf(CHARS),
        2 => leaf(&["1", "'m'"]),
        2 => number(),
    ]
    .boxed()
}

fn cond() -> impl Strategy<Value = String> {
    (
        operand(),
        prop::sample::select(&["<", ">", "<=", ">="][..]),
        operand(),
    )
        .prop_map(|(a, op, b)| format!("{} {op} {}", a.render(), b.render()))
}

fn stmt() -> impl Strategy<Value = Stmt> {
    let simple = prop_oneof![
        (prop::sample::select(REALS), number()).prop_map(|(v, e)| Stmt::Assign(v, e)),
        (prop::sample::select(CHARS), number()).prop_map(|(v, e)| Stmt::Assign(v, e)),
        (prop::sample::select(STRINGS), string()).prop_map(|(v, e)| Stmt::Assign(v, e)),
        (
            prop::sample::select(REALS),
            prop::sample::select(&["+=", "-=", "*=", "/=", "%="][..]),
            number()
        )
            .prop_map(|(v, op, e)| Stmt::Compound(v, op, e)),
        (prop::sample::select(STRINGS), any_expr()).prop_map(|(v, e)| Stmt::Compound(v, "+=", e)),
        any_expr().prop_map(Stmt::Print),
        (string(), any_expr()).prop_map(|(a, b)| Stmt::Scoped(a, b)),
    ];
    simple.prop_recursive(3, 24, 4, |inner| {
        let body = prop::collection::vec(inner, 0..4);
        prop_oneof![
            (cond(), body.clone()).prop_map(|(c, b)| Stmt::If(c, b)),
            (cond(), body).prop_map(|(c, b)| Stmt::While(c, b)),
        ]
    })
}

fn render_block(stmts: &[Stmt], out: &mut String, scoped: &mut usize) {
    for s in stmts {
        match s {
            Stmt::Assign(v, e) => out.push_str(&format!("let {v} = {};\n", e.render())),
            Stmt::Compound(v, op, e) => out.push_str(&format!("let {v} {op} {};\n", e.render())),
            Stmt::Print(e) => out.push_str(&format!("print {};\n", e.render())),
            Stmt::If(c, body) | Stmt::While(c, body) => {
                let keyword = if matches!(s, Stmt::If(..)) {
                    "if"
                } else {
                    "while"
                };
                out.push_str(&format!("{keyword} {c} [\n"));
                render_block(body, out, scoped);
                out.push_str("]\n");
            }
            Stmt::Scoped(a, b) => {
                // Names are never reused, since Entropy has no shadowing.
                let t = format!("t{scoped}");
                *scoped += 1;
                out.push_str(&format!(
                    "declare {t} string;\nlet {t} = {};\nprint {t} + {};\n",
                    a.render(),
                    b.render()
                ));
            }
        }
    }
}

fn program() -> impl Strategy<Value = String> {
    prop::collection::vec(stmt(), 1..8).prop_map(|stmts| {
        let mut out = String::from("Program diff case [\n");
        // Every variable is assigned right away, so definite assignment always holds.
        for (vars, ty, init) in [
            (REALS, "real", "1"),
            (CHARS, "char", "'m'"),
            (STRINGS, "string", "\"ab\""),
        ] {
            for v in vars {
                out.push_str(&format!("declare {v} {ty};\nlet {v} = {init};\n"));
            }
        }
        render_block(&stmts, &mut out, &mut 0);
        out.push_str("]\n");
        out
    })
}

#[derive(Debug, PartialEq)]
struct Outcome {
    stdout: Vec<u8>,
    stderr: String,
    exit: i32,
}

fn outcome(cmd: &mut Command) -> Outcome {
    let out = cmd.env_remove("ENTROPY_DEBUG_RC").output().unwrap();
    Outcome {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out.status.code().unwrap_or(-1),
    }
}

fn differential(src: &str, seed: u64, rate: f32, fuel: u64) -> Result<(), TestCaseError> {
    let (_, diagnostics) = entropy_sema::analyze(src);
    prop_assert!(
        diagnostics.is_empty(),
        "generator bug: {diagnostics:?}\n{src}"
    );

    let mut stdout = Vec::new();
    let (exit, stderr) = entropy_reference::run_source(
        src,
        "t.en",
        rate,
        Some(seed),
        Some(fuel),
        false,
        &mut stdout,
    );
    let reference = Outcome {
        stdout,
        stderr,
        exit: i32::from(exit),
    };

    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("differential");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{n}.en"));
    std::fs::write(&file, src).unwrap();
    let (seed, rate, fuel) = (seed.to_string(), rate.to_string(), fuel.to_string());

    let jit = outcome(
        Command::new(env!("CARGO_BIN_EXE_entropy"))
            .args([
                "run",
                "--seed",
                &seed,
                "--mutation-rate",
                &rate,
                "--fuel",
                &fuel,
            ])
            .arg(&file),
    );
    prop_assert_eq!(&reference, &jit, "jit differs on {}", file.display());

    if n.is_multiple_of(10) {
        let exe = dir.join(n.to_string());
        let build = outcome(
            Command::new(env!("CARGO_BIN_EXE_entropy"))
                .args(["build", "--mutation-rate", &rate, "-o"])
                .arg(&exe)
                .arg(&file),
        );
        prop_assert_eq!(build.exit, 0, "build failed: {}", build.stderr);
        let aot = outcome(
            Command::new(&exe)
                .env("ENTROPY_SEED", &seed)
                .env("ENTROPY_FUEL", &fuel),
        );
        prop_assert_eq!(&reference, &aot, "aot differs on {}", file.display());
    }
    Ok(())
}

#[test]
fn reference_jit_and_aot_agree() {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(64);
    let mut runner = TestRunner::new(Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    });
    let strategy = (
        program(),
        any::<u64>(),
        prop::sample::select(&[0.0f32, 0.5, 2.0, 10.0][..]),
        100u64..5000,
    );
    let result = runner.run(&strategy, |(src, seed, rate, fuel)| {
        differential(&src, seed, rate, fuel)
    });
    if let Err(e) = result {
        panic!("{e}");
    }
}
