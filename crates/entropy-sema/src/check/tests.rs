use crate::analyze;
use crate::hir::*;

fn wrap(body: &str) -> String {
    format!("Program t t [ {body} ]")
}

/// Checks a program body that must be valid.
fn ok(body: &str) -> Program {
    let (hir, diagnostics) = analyze(&wrap(body));
    assert_eq!(diagnostics, [], "unexpected diagnostics for {body:?}");
    hir.unwrap_or_else(|| panic!("no HIR for {body:?}"))
}

/// The diagnostic codes for a program body.
fn codes(body: &str) -> Vec<&'static str> {
    let (_, diagnostics) = analyze(&wrap(body));
    diagnostics.iter().map(|d| d.code.0).collect()
}

/// The type of `e`, found by printing it.
fn type_of(decls: &str, e: &str) -> Type {
    let program = ok(&format!("{decls} print {e};"));
    match program.body.stmts.last() {
        Some(Stmt::Print(e)) => e.ty,
        other => panic!("not a print: {other:?}"),
    }
}

const VARS: &str = "real r = 1; char c = 'a'; string s = \"s\";";

#[test]
fn operator_types() {
    assert_eq!(type_of(VARS, "r + r"), Type::Real);
    assert_eq!(type_of(VARS, "r % r"), Type::Real);
    assert_eq!(type_of(VARS, "r - c"), Type::Char);
    assert_eq!(type_of(VARS, "c * r"), Type::Char);
    assert_eq!(type_of(VARS, "c / c"), Type::Char);
    assert_eq!(type_of(VARS, "s + s"), Type::String);
    assert_eq!(type_of(VARS, "s + r"), Type::String);
    assert_eq!(type_of(VARS, "r + s"), Type::String);
    assert_eq!(type_of(VARS, "c + s"), Type::String);
    assert_eq!(type_of(VARS, "r + c + s"), Type::String);
}

#[test]
fn string_operators() {
    assert_eq!(codes(&format!("{VARS} print s - r;")), ["E0017"]);
    assert_eq!(codes(&format!("{VARS} print r * s;")), ["E0017"]);
    assert_eq!(codes(&format!("{VARS} if s < s [ ]")), ["E0017"]);
    assert_eq!(codes(&format!("{VARS} if r < s [ ]")), ["E0017"]);
}

#[test]
fn comparisons_only_in_conditions() {
    ok(&format!(
        "{VARS} if r < c [ ] while c >= 1 [ ] if r <= r [ ] if r > r [ ]"
    ));
    assert_eq!(codes(&format!("{VARS} print r < r;")), ["E0018"]);
    assert_eq!(codes(&format!("{VARS} let r = r < r;")), ["E0018"]);
    assert_eq!(codes(&format!("{VARS} print (r < r) + 1;")), ["E0018"]);
    assert_eq!(codes(&format!("{VARS} if r [ ]")), ["E0019"]);
    assert_eq!(codes(&format!("{VARS} while r + 1 [ ]")), ["E0019"]);
}

#[test]
fn assignment_types() {
    ok(&format!(
        "{VARS} let r = c; let c = r; let s = s + r; let r = 'x'; let c = 65;"
    ));
    assert_eq!(codes(&format!("{VARS} let s = r;")), ["E0020"]);
    assert_eq!(codes(&format!("{VARS} let s = c;")), ["E0020"]);
    assert_eq!(codes(&format!("{VARS} let r = s;")), ["E0020"]);
    assert_eq!(codes(&format!("{VARS} let c = \"x\";")), ["E0020"]);
    assert_eq!(codes("string s = 1;"), ["E0020"]);
}

#[test]
fn string_conversion_hint() {
    let (_, diagnostics) = analyze(&wrap("real r = 1; string s = r;"));
    assert!(diagnostics[0].message.contains("\"\" + x"));
}

#[test]
fn compound_assignment() {
    ok(&format!(
        "{VARS} let r += 1; let r %= c; let c -= 1; let c *= r; let s += r; let s += s;"
    ));
    assert_eq!(codes(&format!("{VARS} let s -= s;")), ["E0017"]);
    assert_eq!(codes(&format!("{VARS} let r += s;")), ["E0020"]);
}

#[test]
fn compound_assignment_desugars() {
    let program = ok("real x = 1; let x -= 2;");
    let Some(Stmt::Assign { var, value }) = program.body.stmts.last() else {
        panic!("not an assignment")
    };
    let ExprKind::Arith { op, lhs, rhs } = &value.kind else {
        panic!("not arithmetic")
    };
    assert_eq!(*op, ArithOp::Sub);
    assert_eq!(lhs.kind, ExprKind::Var(*var));
    assert_eq!(rhs.kind, ExprKind::Lit(LitId(1)));
}

#[test]
fn every_literal_occurrence_gets_a_slot() {
    let program = ok("print 1; print 1; print 'a' + \"b\";");
    assert_eq!(
        program.literals,
        [
            Literal::Real(1.0),
            Literal::Real(1.0),
            Literal::Char('a'),
            Literal::Str("b".into())
        ]
    );
}

#[test]
fn definite_assignment() {
    ok("declare x real; let x = 1; print x;");
    ok("declare x real; let x = 1; if x < 1 [ print x; ]");
    ok("declare x real; if 1 < 2 [ let x = 1; print x; ]");
    assert_eq!(codes("declare x real; print x;"), ["E0021"]);
    assert_eq!(
        codes("declare x real; if 1 < 2 [ let x = 1; ] print x;"),
        ["E0021"]
    );
    assert_eq!(
        codes("declare x real; while 1 < 2 [ let x = 1; ] print x;"),
        ["E0021"]
    );
    assert_eq!(codes("real x = x;"), ["E0021"]);
    assert_eq!(codes("declare x real; let x += 1;"), ["E0021"]);
    assert_eq!(codes("declare x real; if x < 1 [ ]"), ["E0021"]);
}

#[test]
fn scopes() {
    ok("if 1 < 2 [ real x = 1; ] if 1 < 2 [ real x = 2; ]");
    ok("while 1 < 2 [ real x = 1; print x; ]");
    assert_eq!(codes("if 1 < 2 [ real x = 1; ] print x;"), ["E0015"]);
    assert_eq!(codes("print y;"), ["E0015"]);
    assert_eq!(codes("let y = 1;"), ["E0015"]);
    assert_eq!(codes("real x = 1; declare x char;"), ["E0016"]);
    assert_eq!(codes("real x = 1; if 1 < 2 [ real x = 2; ]"), ["E0016"]);
    assert_eq!(codes("real Count = 1; declare COUNT real;"), ["E0016"]);
}

#[test]
fn block_records_its_variables() {
    let program = ok("real a = 1; if 1 < 2 [ char b = 'b'; ]");
    assert_eq!(program.body.vars, [VarId(0)]);
    let Some(Stmt::If { body, .. }) = program.body.stmts.last() else {
        panic!("not an if")
    };
    assert_eq!(body.vars, [VarId(1)]);
}

#[test]
fn externs() {
    let program = ok("extern Plot(real, char, string); extern f() returns real; \
                      Plot(1, 2, \"x\"); real x = f(); f();");
    assert_eq!(program.externs[0].symbol, "Plot");
    assert_eq!(program.externs[1].returns, Some(Type::Real));
}

#[test]
fn extern_arguments_convert_as_in_assignment() {
    ok("extern f(real, char); f('a', 66);");
    assert_eq!(codes("extern f(real); f(\"x\");"), ["E0024"]);
    assert_eq!(codes("extern f(string); f(1);"), ["E0024"]);
    assert_eq!(codes("extern f(real); f(1, 2);"), ["E0023"]);
    assert_eq!(codes("extern f(real); f();"), ["E0023"]);
}

#[test]
fn extern_errors() {
    assert_eq!(codes("f(); extern f();"), ["E0022"]);
    assert_eq!(codes("real f = 1; f();"), ["E0022"]);
    assert_eq!(codes("extern f(); print f();"), ["E0025"]);
    assert_eq!(codes("extern f(); real x = f();"), ["E0025"]);
    assert_eq!(codes("extern f() returns string;"), ["E0026"]);
    assert_eq!(codes("extern f() returns real; print f;"), ["E0028"]);
    assert_eq!(codes("extern f() returns real; let f = 1;"), ["E0028"]);
}

#[test]
fn reserved_extern_names() {
    for name in [
        "main",
        "Main",
        "entropy_main",
        "ent_print",
        "extern",
        "returns",
    ] {
        assert_eq!(codes(&format!("extern {name}();")), ["E0027"], "{name}");
    }
}

#[test]
fn externs_and_variables_share_a_namespace() {
    assert_eq!(codes("extern f(); real f = 1;"), ["E0016"]);
    assert_eq!(codes("real f = 1; extern f();"), ["E0016"]);
    assert_eq!(codes("extern f(); extern F();"), ["E0016"]);
}

#[test]
fn variadic_externs_warn() {
    let (hir, diagnostics) = analyze(&wrap("extern printf(string);"));
    assert!(hir.is_some());
    assert_eq!(
        diagnostics.iter().map(|d| d.code.0).collect::<Vec<_>>(),
        ["W0001"]
    );
}

#[test]
fn errors_in_both_operands_are_reported() {
    assert_eq!(codes("print a + b;"), ["E0015", "E0015"]);
}

#[test]
fn sema_is_skipped_after_parse_errors() {
    assert_eq!(codes("print undeclared; print ;"), ["E0009"]);
}

#[test]
fn diagnostics_point_at_the_problem() {
    let src = wrap("real x = 1; print y;");
    let (_, diagnostics) = analyze(&src);
    let span = diagnostics[0].span;
    assert_eq!(&src[span.start..span.end], "y");
}
