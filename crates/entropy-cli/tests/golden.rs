//! Golden cases in `tests/cases/`. Each `.en` file has header directives and sibling `.stdout`,
//! `.stderr` and `.exit` files (a missing file means empty output or exit 0).
//!
//! Run cases go through the reference evaluator, `entropy run` and a built executable, and all
//! three must match.
//!
//! Directives, one per line at the top of the file:
//! - `// mode: check` runs `entropy check` only.
//! - `// seed: N`, `// rate: R` and `// fuel: N` set up a run (fuel defaults to 1000000).
//! - `// ffi: foo.c` marks an FFI case (not run yet).
//!
//! Files in `spec/` and `upstream/` are copied verbatim and have no directives, so they run at the
//! defaults: seed 0, rate 2 and fuel 1000000.
//!
//! `UPDATE_GOLDEN=1` rewrites the expected files, except under `by-hand/`, whose expected files
//! are written by hand.

use std::path::{Path, PathBuf};
use std::process::Command;

use libtest_mimic::{Arguments, Failed, Trial};

const DEFAULT_FUEL: u64 = 1_000_000;

#[derive(Default)]
struct Directives {
    check_only: bool,
    seed: u64,
    rate: Option<f32>,
    fuel: Option<u64>,
    ffi: bool,
}

fn directives(src: &str) -> Result<Directives, String> {
    let mut d = Directives::default();
    for line in src.trim_start_matches('\u{feff}').lines() {
        let Some(rest) = line.trim().strip_prefix("//") else {
            break;
        };
        let Some((key, value)) = rest.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let bad = || format!("bad directive `{}`", line.trim());
        match key.trim() {
            "mode" if value == "check" => d.check_only = true,
            "seed" => d.seed = value.parse().map_err(|_| bad())?,
            "rate" => d.rate = Some(value.parse().map_err(|_| bad())?),
            "fuel" => d.fuel = Some(value.parse().map_err(|_| bad())?),
            "ffi" => d.ffi = true,
            _ => return Err(bad()),
        }
    }
    Ok(d)
}

/// What a run produced.
#[derive(PartialEq)]
struct Outcome {
    stdout: Vec<u8>,
    stderr: String,
    exit: u8,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn entropy(args: &[&str]) -> Result<Outcome, Failed> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_entropy"));
    cmd.args(args);
    outcome(&mut cmd)
}

fn outcome(cmd: &mut Command) -> Result<Outcome, Failed> {
    let out = cmd.current_dir(root()).env("NO_COLOR", "1").output()?;
    Ok(Outcome {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        exit: out
            .status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(255),
    })
}

fn check(rel: &str) -> Result<Outcome, Failed> {
    entropy(&["check", rel])
}

fn jit(rel: &str, d: &Directives) -> Result<Outcome, Failed> {
    entropy(&[
        "run",
        rel,
        "--seed",
        &d.seed.to_string(),
        "--mutation-rate",
        &d.rate.unwrap_or(2.0).to_string(),
        "--fuel",
        &d.fuel.unwrap_or(DEFAULT_FUEL).to_string(),
    ])
}

/// Builds an executable and runs it. If the build fails, that is the outcome instead.
fn aot(rel: &str, d: &Directives) -> Result<Outcome, Failed> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    std::fs::create_dir_all(&dir)?;
    let exe = dir.join(rel.trim_start_matches("tests/cases/").replace('/', "_"));
    let exe_arg = exe.to_string_lossy();
    let rate = d.rate.unwrap_or(2.0).to_string();
    let build = entropy(&["build", rel, "-o", &exe_arg, "--mutation-rate", &rate])?;
    if build.exit != 0 {
        return Ok(build);
    }
    if !build.stderr.is_empty() || !build.stdout.is_empty() {
        return Err(format!("{rel}: build printed output\n{}", describe(&build)).into());
    }
    outcome(
        Command::new(&exe)
            .env("ENTROPY_SEED", d.seed.to_string())
            .env("ENTROPY_FUEL", d.fuel.unwrap_or(DEFAULT_FUEL).to_string())
            .env_remove("ENTROPY_DEBUG_RC"),
    )
}

fn reference(rel: &str, src: &str, d: &Directives) -> Outcome {
    let mut stdout = Vec::new();
    let (exit, stderr) = entropy_reference::run_source(
        src,
        rel,
        d.rate.unwrap_or(2.0),
        Some(d.seed),
        Some(d.fuel.unwrap_or(DEFAULT_FUEL)),
        false,
        &mut stdout,
    );
    Outcome {
        stdout,
        stderr,
        exit,
    }
}

fn read_expected(path: &Path) -> Outcome {
    let read = |ext| std::fs::read(path.with_extension(ext)).unwrap_or_default();
    Outcome {
        stdout: read("stdout"),
        stderr: String::from_utf8_lossy(&read("stderr")).into_owned(),
        exit: String::from_utf8_lossy(&read("exit"))
            .trim()
            .parse()
            .unwrap_or(0),
    }
}

/// Writes each expected file, removing it when it would be empty (or exit 0).
fn bless(path: &Path, actual: &Outcome) -> std::io::Result<()> {
    let exit = if actual.exit == 0 {
        String::new()
    } else {
        format!("{}\n", actual.exit)
    };
    for (ext, bytes) in [
        ("stdout", actual.stdout.as_slice()),
        ("stderr", actual.stderr.as_bytes()),
        ("exit", exit.as_bytes()),
    ] {
        let file = path.with_extension(ext);
        if bytes.is_empty() {
            if file.exists() {
                std::fs::remove_file(file)?;
            }
        } else {
            std::fs::write(file, bytes)?;
        }
    }
    Ok(())
}

fn describe(o: &Outcome) -> String {
    format!(
        "exit {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        o.exit,
        String::from_utf8_lossy(&o.stdout),
        o.stderr
    )
}

fn run_case(path: &Path, rel: &str) -> Result<(), Failed> {
    let src = std::fs::read_to_string(path)?;
    let d = directives(&src)?;
    let first = if d.check_only {
        check(rel)?
    } else {
        reference(rel, &src, &d)
    };
    let by_hand = rel.contains("/by-hand/");
    if std::env::var_os("UPDATE_GOLDEN").is_some() && !by_hand {
        bless(path, &first)?;
    }
    let expected = read_expected(path);
    let first_name = if d.check_only { "check" } else { "reference" };
    compare(rel, first_name, &expected, &first)?;
    if !d.check_only {
        compare(rel, "jit", &expected, &jit(rel, &d)?)?;
        compare(rel, "aot", &expected, &aot(rel, &d)?)?;
    }
    Ok(())
}

fn compare(rel: &str, path: &str, expected: &Outcome, actual: &Outcome) -> Result<(), Failed> {
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "{rel}: {path} output differs\n=== expected ===\n{}\n=== actual ===\n{}",
        describe(expected),
        describe(actual)
    )
    .into())
}

fn cases(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            cases(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "en") {
            out.push(path);
        }
    }
    Ok(())
}

fn main() {
    let args = Arguments::from_args();
    let base = root().join("tests/cases");
    let mut paths = Vec::new();
    if let Err(e) = cases(&base, &mut paths) {
        eprintln!("can't list {}: {e}", base.display());
        std::process::exit(1);
    }
    paths.sort();
    let trials = paths
        .into_iter()
        .map(|path| {
            let rel = format!(
                "tests/cases/{}",
                path.strip_prefix(&base).unwrap_or(&path).display()
            );
            let ffi = std::fs::read_to_string(&path)
                .ok()
                .and_then(|src| directives(&src).ok())
                .is_some_and(|d| d.ffi);
            let name = rel.trim_start_matches("tests/cases/").to_string();
            Trial::test(name, move || run_case(&path, &rel)).with_ignored_flag(ffi)
        })
        .collect();
    libtest_mimic::run(&args, trials).exit();
}
