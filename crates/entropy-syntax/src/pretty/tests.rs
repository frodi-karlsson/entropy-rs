use proptest::prelude::*;

use super::*;
use crate::diagnostic::Span;
use crate::lexer::normalize_ident;
use crate::parser::parse;

#[test]
fn prints_a_program() {
    let src = "program NS Demo [ extern Plot(real, char) returns real; extern f(); declare x real; \
               real y = (1 + 2) * 3 - (4 - 5); ; let x += y % 2; \
               while x > 0 [ print \"a\\\"b\\n\" + 'c' + '\\''; if (x) < 1 [ ] ] f(); ]";
    let (program, diagnostics) = parse(src);
    assert_eq!(diagnostics, []);
    assert_eq!(
        pretty(&program),
        r#"Program ns demo [
    extern Plot(real, char) returns real;
    extern f();
    declare x real;
    real y = (1 + 2) * 3 - (4 - 5);
    ;
    let x += y % 2;
    while x > 0 [
        print "a\"b\n" + 'c' + '\'';
        if x < 1 [ ]
    ]
    f();
]
"#
    );
}

#[test]
fn numbers_print_without_exponents() {
    assert_eq!(expr_text("1e-7"), "0.0000001");
    assert_eq!(expr_text("2e3"), "2000");
    assert_eq!(expr_text("1.50"), "1.5");
}

#[test]
fn control_characters_are_escaped() {
    assert_eq!(expr_text("\"\\u0001\\u007f\""), "\"\\u0001\\u007f\"");
}

fn expr_text(e: &str) -> String {
    let (program, diagnostics) = parse(&format!("Program a b [ print {e}; ]"));
    assert_eq!(diagnostics, []);
    match program.items.first() {
        Some(Item::Stmt(Stmt {
            kind: StmtKind::Print(e),
            ..
        })) => expr(e),
        other => panic!("not a print: {other:?}"),
    }
}

// Round trip: generate an AST, print it, parse it back.

const NO_SPAN: Span = Span { start: 0, end: 0 };

fn ident() -> impl Strategy<Value = Ident> {
    prop_oneof![
        "[a-z_][a-z0-9_]{0,4}",
        Just("café".to_string()),
        Just("extern".to_string()),
        Just("returns".to_string()),
    ]
    .prop_filter("keyword", |n| {
        !matches!(
            n.as_str(),
            "program" | "declare" | "let" | "print" | "if" | "while" | "real" | "char" | "string"
        )
    })
    .prop_map(|name| Ident {
        name,
        span: NO_SPAN,
    })
}

fn type_ref() -> impl Strategy<Value = TypeRef> {
    prop_oneof![Just(Type::Real), Just(Type::Char), Just(Type::String)]
        .prop_map(|ty| TypeRef { ty, span: NO_SPAN })
}

fn number() -> impl Strategy<Value = f32> {
    prop_oneof![
        (0u32..1000).prop_map(|n| n as f32),
        0.0f32..f32::MAX,
        Just(f32::MIN_POSITIVE),
        Just(1e-45),
    ]
}

fn binop() -> impl Strategy<Value = BinOp> {
    prop::sample::select(vec![
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::Rem,
        BinOp::Lt,
        BinOp::Gt,
        BinOp::Le,
        BinOp::Ge,
    ])
}

fn node(kind: ExprKind) -> Expr {
    Expr {
        kind,
        span: NO_SPAN,
    }
}

fn call_of(args: impl Strategy<Value = Vec<Expr>>) -> impl Strategy<Value = Call> {
    (ident(), args).prop_map(|(name, args)| Call {
        name,
        args,
        span: NO_SPAN,
    })
}

fn expr_strategy() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        number().prop_map(|v| node(ExprKind::Number(v))),
        any::<String>().prop_map(|s| node(ExprKind::Str(s))),
        any::<char>().prop_map(|c| node(ExprKind::Char(c))),
        ident().prop_map(|id| node(ExprKind::Var(id))),
    ];
    leaf.prop_recursive(6, 40, 3, |inner| {
        prop_oneof![
            (binop(), inner.clone(), inner.clone()).prop_map(|(op, lhs, rhs)| node(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            )),
            call_of(prop::collection::vec(inner, 0..3)).prop_map(|c| node(ExprKind::Call(c))),
        ]
    })
}

fn stmt_node(kind: StmtKind) -> Stmt {
    Stmt {
        kind,
        span: NO_SPAN,
    }
}

fn stmt_strategy() -> impl Strategy<Value = Stmt> {
    let assign_op = prop::sample::select(vec![
        AssignOp::Set,
        AssignOp::Add,
        AssignOp::Sub,
        AssignOp::Mul,
        AssignOp::Div,
        AssignOp::Rem,
    ]);
    let leaf = prop_oneof![
        Just(stmt_node(StmtKind::Empty)),
        (ident(), type_ref()).prop_map(|(name, ty)| stmt_node(StmtKind::Declare { name, ty })),
        (type_ref(), ident(), expr_strategy())
            .prop_map(|(ty, name, value)| stmt_node(StmtKind::DeclareInit { ty, name, value })),
        (ident(), assign_op, expr_strategy())
            .prop_map(|(name, op, value)| stmt_node(StmtKind::Assign { name, op, value })),
        expr_strategy().prop_map(|e| stmt_node(StmtKind::Print(e))),
        call_of(prop::collection::vec(expr_strategy(), 0..3))
            .prop_map(|c| stmt_node(StmtKind::Call(c))),
    ];
    leaf.prop_recursive(3, 30, 4, |inner| {
        let body = prop::collection::vec(inner, 0..4);
        prop_oneof![
            (expr_strategy(), body.clone())
                .prop_map(|(cond, body)| stmt_node(StmtKind::If { cond, body })),
            (expr_strategy(), body)
                .prop_map(|(cond, body)| stmt_node(StmtKind::While { cond, body })),
        ]
    })
}

fn extern_strategy() -> impl Strategy<Value = Extern> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,4}",
        prop::collection::vec(type_ref(), 0..3),
        prop::option::of(type_ref()),
    )
        .prop_filter("keyword or contextual keyword", |(symbol, _, _)| {
            let name = normalize_ident(symbol);
            !matches!(
                name.as_str(),
                "program"
                    | "declare"
                    | "let"
                    | "print"
                    | "if"
                    | "while"
                    | "real"
                    | "char"
                    | "string"
                    | "extern"
                    | "returns"
            )
        })
        .prop_map(|(symbol, params, returns)| Extern {
            name: Ident {
                name: normalize_ident(&symbol),
                span: NO_SPAN,
            },
            symbol,
            params,
            returns,
            span: NO_SPAN,
        })
}

fn program_strategy() -> impl Strategy<Value = Program> {
    let item = prop_oneof![
        1 => extern_strategy().prop_map(Item::Extern),
        4 => stmt_strategy().prop_map(Item::Stmt),
    ];
    (ident(), ident(), prop::collection::vec(item, 0..6)).prop_map(|(namespace, name, items)| {
        Program {
            namespace,
            name,
            items,
            span: NO_SPAN,
        }
    })
}

// Clearing spans so two ASTs compare by structure only.

fn clear_expr(e: &mut Expr) {
    e.span = NO_SPAN;
    match &mut e.kind {
        ExprKind::Var(id) => id.span = NO_SPAN,
        ExprKind::Call(c) => clear_call(c),
        ExprKind::Binary { lhs, rhs, .. } => {
            clear_expr(lhs);
            clear_expr(rhs);
        }
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Char(_) => {}
    }
}

fn clear_call(c: &mut Call) {
    c.span = NO_SPAN;
    c.name.span = NO_SPAN;
    c.args.iter_mut().for_each(clear_expr);
}

fn clear_stmt(s: &mut Stmt) {
    s.span = NO_SPAN;
    match &mut s.kind {
        StmtKind::Empty => {}
        StmtKind::Declare { name, ty } => {
            name.span = NO_SPAN;
            ty.span = NO_SPAN;
        }
        StmtKind::DeclareInit { ty, name, value } => {
            ty.span = NO_SPAN;
            name.span = NO_SPAN;
            clear_expr(value);
        }
        StmtKind::Assign { name, value, .. } => {
            name.span = NO_SPAN;
            clear_expr(value);
        }
        StmtKind::Print(e) => clear_expr(e),
        StmtKind::If { cond, body } | StmtKind::While { cond, body } => {
            clear_expr(cond);
            body.iter_mut().for_each(clear_stmt);
        }
        StmtKind::Call(c) => clear_call(c),
    }
}

fn clear_program(p: &mut Program) {
    p.span = NO_SPAN;
    p.namespace.span = NO_SPAN;
    p.name.span = NO_SPAN;
    for item in &mut p.items {
        match item {
            Item::Stmt(s) => clear_stmt(s),
            Item::Extern(e) => {
                e.span = NO_SPAN;
                e.name.span = NO_SPAN;
                e.params.iter_mut().for_each(|t| t.span = NO_SPAN);
                if let Some(r) = &mut e.returns {
                    r.span = NO_SPAN;
                }
            }
        }
    }
}

proptest! {
    #[test]
    fn round_trip(program in program_strategy()) {
        let text = pretty(&program);
        let (mut parsed, diagnostics) = parse(&text);
        prop_assert_eq!(diagnostics, [], "{}", text);
        clear_program(&mut parsed);
        prop_assert_eq!(parsed, program, "{}", text);
    }
}
