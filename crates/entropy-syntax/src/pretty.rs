//! Prints an AST back as source. `parse(pretty(ast))` gives the same AST, ignoring spans.

use std::fmt::Write;

use crate::ast::*;

const INDENT: &str = "    ";

pub fn pretty(program: &Program) -> String {
    let mut out = format!(
        "Program {} {} [\n",
        program.namespace.name, program.name.name
    );
    for item in &program.items {
        match item {
            Item::Extern(e) => extern_decl(&mut out, e),
            Item::Stmt(s) => stmt(&mut out, s, 1),
        }
    }
    out.push_str("]\n");
    out
}

fn type_name(t: TypeRef) -> &'static str {
    match t.ty {
        Type::Real => "real",
        Type::Char => "char",
        Type::String => "string",
    }
}

fn extern_decl(out: &mut String, e: &Extern) {
    let params: Vec<_> = e.params.iter().map(|&p| type_name(p)).collect();
    let _ = write!(out, "{INDENT}extern {}({})", e.symbol, params.join(", "));
    if let Some(r) = e.returns {
        let _ = write!(out, " returns {}", type_name(r));
    }
    out.push_str(";\n");
}

fn stmt(out: &mut String, s: &Stmt, level: usize) {
    out.push_str(&INDENT.repeat(level));
    match &s.kind {
        StmtKind::Empty => out.push(';'),
        StmtKind::Declare { name, ty } => {
            let _ = write!(out, "declare {} {};", name.name, type_name(*ty));
        }
        StmtKind::DeclareInit { ty, name, value } => {
            let _ = write!(out, "{} {} = {};", type_name(*ty), name.name, expr(value));
        }
        StmtKind::Assign { name, op, value } => {
            let op = match op {
                AssignOp::Set => "=",
                AssignOp::Add => "+=",
                AssignOp::Sub => "-=",
                AssignOp::Mul => "*=",
                AssignOp::Div => "/=",
                AssignOp::Rem => "%=",
            };
            let _ = write!(out, "let {} {op} {};", name.name, expr(value));
        }
        StmtKind::Print(e) => {
            let _ = write!(out, "print {};", expr(e));
        }
        StmtKind::If { cond, body } => block(out, "if", cond, body, level),
        StmtKind::While { cond, body } => block(out, "while", cond, body, level),
        StmtKind::Call(c) => {
            let _ = write!(out, "{};", call(c));
        }
    }
    out.push('\n');
}

fn block(out: &mut String, keyword: &str, cond: &Expr, body: &[Stmt], level: usize) {
    let _ = write!(out, "{keyword} {} [", expr(cond));
    if body.is_empty() {
        out.push_str(" ]");
        return;
    }
    out.push('\n');
    for s in body {
        stmt(out, s, level + 1);
    }
    out.push_str(&INDENT.repeat(level));
    out.push(']');
}

/// Binding strength: comparisons 0, `+ -` 1, `* / %` 2, atoms 3.
fn precedence(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::Binary { op, .. } => match op {
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 0,
            BinOp::Add | BinOp::Sub => 1,
            BinOp::Mul | BinOp::Div | BinOp::Rem => 2,
        },
        _ => 3,
    }
}

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Number(v) => v.to_string(),
        ExprKind::Str(s) => quote(s, '"'),
        ExprKind::Char(c) => quote(&c.to_string(), '\''),
        ExprKind::Var(id) => id.name.clone(),
        ExprKind::Call(c) => call(c),
        ExprKind::Binary { op, lhs, rhs } => {
            let p = precedence(e);
            // Operators are left-associative and comparisons don't chain.
            let left_parens = precedence(lhs) < p || (p == 0 && precedence(lhs) == 0);
            let right_parens = precedence(rhs) <= p;
            format!(
                "{} {} {}",
                parenthesize(lhs, left_parens),
                op.symbol(),
                parenthesize(rhs, right_parens)
            )
        }
    }
}

fn parenthesize(e: &Expr, parens: bool) -> String {
    if parens {
        format!("({})", expr(e))
    } else {
        expr(e)
    }
}

fn call(c: &Call) -> String {
    let args: Vec<_> = c.args.iter().map(expr).collect();
    format!("{}({})", c.name.name, args.join(", "))
}

fn quote(text: &str, delimiter: char) -> String {
    let mut out = String::from(delimiter);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c if c == delimiter => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() && (c as u32) <= 0xffff => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(delimiter);
    out
}

#[cfg(test)]
mod tests;
