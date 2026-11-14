use super::*;
use crate::diagnostic::render;

/// Compact S-expression form of an expression, for readable assertions.
fn sexpr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Number(v) => v.to_string(),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Char(c) => format!("{c:?}"),
        ExprKind::Var(id) => id.name.clone(),
        ExprKind::Call(call) => call_sexpr(call),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", op.symbol(), sexpr(lhs), sexpr(rhs))
        }
    }
}

fn call_sexpr(call: &Call) -> String {
    let mut s = format!("(call {}", call.name.name);
    for arg in &call.args {
        s.push(' ');
        s.push_str(&sexpr(arg));
    }
    s + ")"
}

fn ty(t: TypeRef) -> &'static str {
    match t.ty {
        Type::Real => "real",
        Type::Char => "char",
        Type::String => "string",
    }
}

fn stmt_sexpr(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Empty => "(empty)".into(),
        StmtKind::Declare { name, ty: t } => format!("(declare {} {})", name.name, ty(*t)),
        StmtKind::DeclareInit { ty: t, name, value } => {
            format!("(declare {} {} {})", name.name, ty(*t), sexpr(value))
        }
        StmtKind::Assign { name, op, value } => {
            format!("(let {} {op:?} {})", name.name, sexpr(value))
        }
        StmtKind::Print(e) => format!("(print {})", sexpr(e)),
        StmtKind::If { cond, body } => format!("(if {} {})", sexpr(cond), block_sexpr(body)),
        StmtKind::While { cond, body } => format!("(while {} {})", sexpr(cond), block_sexpr(body)),
        StmtKind::Call(call) => call_sexpr(call),
    }
}

fn block_sexpr(body: &[Stmt]) -> String {
    let parts: Vec<_> = body.iter().map(stmt_sexpr).collect();
    format!("[{}]", parts.join(" "))
}

fn item_sexpr(item: &Item) -> String {
    match item {
        Item::Stmt(s) => stmt_sexpr(s),
        Item::Extern(e) => {
            let params: Vec<_> = e.params.iter().map(|&p| ty(p)).collect();
            let returns = e
                .returns
                .map(|r| format!(" -> {}", ty(r)))
                .unwrap_or_default();
            format!(
                "(extern {} \"{}\" ({}){returns})",
                e.name.name,
                e.symbol,
                params.join(" ")
            )
        }
    }
}

fn wrap(body: &str) -> String {
    format!("Program t t [ {body} ]")
}

/// Parses a program body that must be valid, returning its items in S-expression form.
fn items(body: &str) -> String {
    let (program, diagnostics) = parse(&wrap(body));
    assert_eq!(diagnostics, [], "unexpected diagnostics for {body:?}");
    let parts: Vec<_> = program.items.iter().map(item_sexpr).collect();
    parts.join(" ")
}

fn expr(e: &str) -> String {
    let s = items(&format!("print {e};"));
    s.strip_prefix("(print ")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("not a print: {s}"))
        .to_string()
}

/// Error codes, plus the items that survived recovery.
fn errors(src: &str) -> (Vec<&'static str>, String) {
    let (program, diagnostics) = parse(src);
    let codes = diagnostics.iter().map(|d| d.code.0).collect();
    let parts: Vec<_> = program.items.iter().map(item_sexpr).collect();
    (codes, parts.join(" "))
}

#[test]
fn precedence() {
    assert_eq!(expr("1 + 2 * 3"), "(+ 1 (* 2 3))");
    assert_eq!(expr("1 * 2 + 3"), "(+ (* 1 2) 3)");
    assert_eq!(expr("1 - 2 - 3"), "(- (- 1 2) 3)");
    assert_eq!(expr("a * b % c / d"), "(/ (% (* a b) c) d)");
    assert_eq!(expr("a + b < c * d"), "(< (+ a b) (* c d))");
    assert_eq!(expr("a <= b"), "(<= a b)");
    assert_eq!(expr("a >= b - 1"), "(>= a (- b 1))");
}

#[test]
fn parentheses_group_as_written() {
    assert_eq!(expr("(1 + 2) * 3"), "(* (+ 1 2) 3)");
    assert_eq!(expr("1 - (2 - 3)"), "(- 1 (- 2 3))");
    assert_eq!(expr("((a))"), "a");
    assert_eq!(expr("(a < b)"), "(< a b)");
}

#[test]
fn atoms() {
    assert_eq!(expr("1.5"), "1.5");
    assert_eq!(expr("\"hi\""), "\"hi\"");
    assert_eq!(expr("'x'"), "'x'");
    assert_eq!(expr("Count"), "count");
    assert_eq!(expr("f()"), "(call f)");
    assert_eq!(expr("f(a, b + 1, g(c))"), "(call f a (+ b 1) (call g c))");
}

#[test]
fn statements() {
    assert_eq!(items(";"), "(empty)");
    assert_eq!(items("declare x real;"), "(declare x real)");
    assert_eq!(items("Declare s STRING;"), "(declare s string)");
    assert_eq!(items("real x = 1;"), "(declare x real 1)");
    assert_eq!(items("char c = 'a';"), "(declare c char 'a')");
    assert_eq!(
        items("string s = \"a\" + 1;"),
        "(declare s string (+ \"a\" 1))"
    );
    assert_eq!(items("let x = 1;"), "(let x Set 1)");
    assert_eq!(
        items("let x += 1; let x -= 1; let x *= 1; let x /= 1; let x %= 1;"),
        "(let x Add 1) (let x Sub 1) (let x Mul 1) (let x Div 1) (let x Rem 1)"
    );
    assert_eq!(items("print x;"), "(print x)");
    assert_eq!(items("plot(1, 2);"), "(call plot 1 2)");
}

#[test]
fn blocks() {
    assert_eq!(items("if a < b [ ]"), "(if (< a b) [])");
    assert_eq!(
        items("while x > 0 [ print x; if x < 5 [ let x -= 1; ] ]"),
        "(while (> x 0) [(print x) (if (< x 5) [(let x Sub 1)])])"
    );
}

#[test]
fn externs() {
    assert_eq!(
        items("extern plot(real, char, string); extern Sqrt(real) returns real; extern f();"),
        "(extern plot \"plot\" (real char string)) \
         (extern sqrt \"Sqrt\" (real) -> real) \
         (extern f \"f\" ())"
    );
}

#[test]
fn contextual_keywords_are_identifiers_elsewhere() {
    assert_eq!(
        items("declare extern real; let extern = returns; print extern + 1;"),
        "(declare extern real) (let extern Set returns) (print (+ extern 1))"
    );
    assert_eq!(items("extern(1);"), "(call extern 1)");
}

#[test]
fn spans() {
    let src = wrap("print (1 + 2) * x;");
    let (program, _) = parse(&src);
    let Some(Item::Stmt(stmt)) = program.items.first() else {
        panic!("no statement")
    };
    assert_eq!(&src[stmt.span.start..stmt.span.end], "print (1 + 2) * x;");
    let StmtKind::Print(e) = &stmt.kind else {
        panic!("not a print")
    };
    assert_eq!(&src[e.span.start..e.span.end], "(1 + 2) * x");
    let ExprKind::Binary { lhs, .. } = &e.kind else {
        panic!("not binary")
    };
    assert_eq!(&src[lhs.span.start..lhs.span.end], "(1 + 2)");
    assert_eq!((program.span.start, program.span.end), (0, src.len()));
}

#[test]
fn header() {
    let (program, diagnostics) = parse("program Ns Name [ ]");
    assert_eq!(diagnostics, []);
    assert_eq!(
        (program.namespace.name.as_str(), program.name.name.as_str()),
        ("ns", "name")
    );
    assert_eq!(errors("print 1;").0, ["E0009"]);
    assert_eq!(errors("Program a [ ]").0, ["E0009"]);
    assert_eq!(errors("Program print b [ ]").0, ["E0009"]);
    assert_eq!(errors("Program a b").0, ["E0009"]);
    assert_eq!(errors("Program a b [ print 1;").0, ["E0009"]);
    assert_eq!(errors("Program a b [ ] x").0, ["E0009"]);
}

#[test]
fn rejected_syntax() {
    assert_eq!(errors(&wrap("print a < b < c;")).0, ["E0010"]);
    assert_eq!(errors(&wrap("let x &= 1;")).0, ["E0011"]);
    assert_eq!(errors(&wrap("let x <<= 1;")).0, ["E0011"]);
    assert_eq!(errors(&wrap("if a < b [ extern f(); ]")).0, ["E0013"]);
    assert_eq!(errors(&wrap("print -1;")).0, ["E0014"]);
    assert_eq!(errors(&wrap("print 1 - -1;")).0, ["E0014"]);
}

#[test]
fn recovery_skips_to_the_next_statement() {
    assert_eq!(
        errors(&wrap(
            "print ; print 1; if 1 < 2 [ print ); print 2; ] print 3;"
        )),
        (
            vec!["E0009", "E0009"],
            "(print 1) (if (< 1 2) [(print 2)]) (print 3)".to_string()
        )
    );
    // A missing `;` costs the next statement too, but nothing after it.
    assert_eq!(
        errors(&wrap("print 1 print 2; print 3;")),
        (vec!["E0009"], "(print 3)".to_string())
    );
    // A broken `if` header skips its whole block.
    assert_eq!(
        errors(&wrap("if [ print 1; ] print 2;")),
        (vec!["E0009"], "(print 2)".to_string())
    );
}

#[test]
fn lexer_errors_are_reported_too() {
    assert_eq!(errors(&wrap("print 1. ;")).0, ["E0007"]);
}

#[test]
fn rendered_errors() {
    let src = wrap("print a < b < c; let x &= 1; print -1;");
    let (_, diagnostics) = parse(&src);
    insta::assert_snapshot!(render(&diagnostics, "t.en", &src, false));
}

fn nested(open: &str, close: &str, depth: usize, inner: &str) -> String {
    format!("{}{inner}{}", open.repeat(depth), close.repeat(depth))
}

#[test]
fn nesting_up_to_the_limit_is_fine() {
    assert!(
        parse(&wrap(&format!("print {};", nested("(", ")", 200, "1"))))
            .1
            .is_empty()
    );
    let ifs = nested("if 1 < 2 [ ", " ]", 200, "print 1;");
    assert!(parse(&wrap(&ifs)).1.is_empty());
    let chain = format!("print 1{};", " + 1".repeat(200));
    assert!(parse(&wrap(&chain)).1.is_empty());
}

#[test]
fn nesting_past_the_limit_is_one_error() {
    let cases = [
        format!("print {};", nested("(", ")", 300, "1")),
        nested("if 1 < 2 [ ", " ]", 300, "print 1;"),
        format!("print 1{};", " + 1".repeat(300)),
        format!("print 1{};", " * 1".repeat(300)),
        // Far too deep to recurse through: must not overflow the stack.
        format!("print {};", nested("(", ")", 1_000_000, "1")),
        format!("print 1{};", " - 1".repeat(1_000_000)),
    ];
    for case in cases {
        assert_eq!(errors(&wrap(&case)).0, ["E0012"]);
    }
}

/// Programs made of random tokens.
fn token_soup() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let token = prop::sample::select(vec![
        "Program", "declare", "let", "print", "if", "while", "real", "char", "string", "extern",
        "returns", "x", "f", "1", "\"s\"", "'c'", "[", "]", "(", ")", ";", ",", "=", "+=", "&=",
        "+", "-", "*", "/", "%", "<", ">", "<=", ">=",
    ]);
    prop::collection::vec(token, 0..60).prop_map(|t| t.join(" "))
}

proptest::proptest! {
    #[test]
    fn never_panics_and_spans_are_valid(body in token_soup()) {
        for src in [body.clone(), wrap(&body)] {
            let (program, diagnostics) = parse(&src);
            for d in &diagnostics {
                proptest::prop_assert!(d.span.start <= d.span.end && d.span.end <= src.len());
            }
            proptest::prop_assert!(program.span.end <= src.len());
        }
    }
}
