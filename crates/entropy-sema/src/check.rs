use std::collections::HashSet;

use entropy_syntax::ast::{self, AssignOp, BinOp};
use entropy_syntax::diagnostic::{Code, Diagnostic, Severity, Span, code};

use crate::hir::{self, ExternId, LitId, Type, VarId};

/// Type-checks a parsed program. The HIR is returned only when there are no errors.
pub fn check(program: &ast::Program) -> (Option<hir::Program>, Vec<Diagnostic>) {
    let mut checker = Checker {
        vars: Vec::new(),
        literals: Vec::new(),
        externs: Vec::new(),
        scopes: Vec::new(),
        assigned: HashSet::new(),
        diagnostics: Vec::new(),
    };
    checker.scopes.push(Vec::new());
    let mut body = hir::Block {
        stmts: Vec::new(),
        vars: Vec::new(),
    };
    for item in &program.items {
        match item {
            ast::Item::Extern(e) => checker.extern_decl(e),
            ast::Item::Stmt(s) => checker.stmt(s, &mut body),
        }
    }
    let failed = checker
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    let hir = if failed {
        None
    } else {
        Some(hir::Program {
            vars: checker.vars,
            literals: checker.literals,
            externs: checker.externs,
            body,
        })
    };
    (hir, checker.diagnostics)
}

#[derive(Clone, Copy)]
enum Binding {
    Var(VarId),
    Extern(ExternId),
}

struct Scoped {
    name: String,
    binding: Binding,
    span: Span,
}

/// A checked expression. Comparisons are kept apart because only conditions may use them.
enum Checked {
    Value(hir::Expr),
    Comparison(hir::Cond),
}

const VARIADIC: [&str; 6] = [
    "printf", "fprintf", "sprintf", "snprintf", "scanf", "fscanf",
];

fn type_name(ty: Type) -> &'static str {
    match ty {
        Type::Real => "real",
        Type::Char => "char",
        Type::String => "string",
    }
}

/// Whether a value of type `value` may be stored in, or passed as, `target` (spec 4.2).
fn assignable(target: Type, value: Type) -> bool {
    match target {
        Type::String => value == Type::String,
        Type::Real | Type::Char => value != Type::String,
    }
}

fn is_reserved_extern(name: &str) -> bool {
    matches!(name, "main" | "entropy_main" | "extern" | "returns") || name.starts_with("ent_")
}

struct Checker {
    vars: Vec<hir::Var>,
    literals: Vec<hir::Literal>,
    externs: Vec<hir::Extern>,
    scopes: Vec<Vec<Scoped>>,
    assigned: HashSet<VarId>,
    diagnostics: Vec<Diagnostic>,
}

impl Checker {
    fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    fn lookup(&self, name: &str) -> Option<&Scoped> {
        self.scopes.iter().rev().flatten().find(|s| s.name == name)
    }

    /// Adds a name to the current scope. Fails if the name is visible already.
    fn bind(&mut self, ident: &ast::Ident, binding: Binding) -> bool {
        let in_current = self
            .scopes
            .last()
            .is_some_and(|scope| scope.iter().any(|s| s.name == ident.name));
        if let Some(previous) = self.lookup(&ident.name) {
            let message = if in_current {
                format!("`{}` is already declared", ident.name)
            } else {
                format!("`{}` is already declared in an outer scope", ident.name)
            };
            let previous = previous.span;
            self.diagnostics.push(
                Diagnostic::error(code::ALREADY_DECLARED, ident.span, message)
                    .with_label(previous, "first declared here")
                    .with_label(ident.span, "declared again here"),
            );
            return false;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(Scoped {
                name: ident.name.clone(),
                binding,
                span: ident.span,
            });
        }
        true
    }

    fn extern_decl(&mut self, e: &ast::Extern) {
        if is_reserved_extern(&e.name.name) {
            self.error(
                code::RESERVED_EXTERN_NAME,
                e.name.span,
                format!("`{}` is reserved and can't be an extern", e.symbol),
            );
        }
        if let Some(r) = e.returns.filter(|r| r.ty == Type::String) {
            self.error(
                code::STRING_RETURN,
                r.span,
                "an extern can't return a string",
            );
        }
        if VARIADIC.contains(&e.name.name.as_str()) {
            self.diagnostics.push(Diagnostic::warning(
                code::VARIADIC_EXTERN,
                e.name.span,
                format!(
                    "`{}` is variadic in C, and calling a variadic function through an extern is \
                     undefined behaviour",
                    e.symbol
                ),
            ));
        }
        let id = ExternId(self.externs.len());
        self.externs.push(hir::Extern {
            symbol: e.symbol.clone(),
            params: e.params.iter().map(|p| p.ty).collect(),
            returns: e.returns.map(|r| r.ty),
        });
        self.bind(&e.name, Binding::Extern(id));
    }

    fn declare(&mut self, name: &ast::Ident, ty: Type, block: &mut hir::Block) -> VarId {
        let id = VarId(self.vars.len());
        self.vars.push(hir::Var {
            name: name.name.clone(),
            ty,
        });
        self.bind(name, Binding::Var(id));
        self.assigned.remove(&id);
        block.vars.push(id);
        block.stmts.push(hir::Stmt::Declare(id));
        id
    }

    /// Resolves a name used as a variable.
    fn var(&mut self, ident: &ast::Ident) -> Option<VarId> {
        match self.lookup(&ident.name).map(|s| s.binding) {
            Some(Binding::Var(id)) => Some(id),
            Some(Binding::Extern(_)) => {
                self.error(
                    code::EXTERN_AS_VARIABLE,
                    ident.span,
                    format!("`{}` is an extern, not a variable", ident.name),
                );
                None
            }
            None => {
                self.error(
                    code::UNDECLARED,
                    ident.span,
                    format!("`{}` is not declared", ident.name),
                );
                None
            }
        }
    }

    fn var_type(&self, id: VarId) -> Type {
        self.vars.get(id.0).map_or(Type::Real, |v| v.ty)
    }

    fn stmt(&mut self, s: &ast::Stmt, block: &mut hir::Block) {
        match &s.kind {
            ast::StmtKind::Empty => {}
            ast::StmtKind::Declare { name, ty } => {
                self.declare(name, ty.ty, block);
            }
            ast::StmtKind::DeclareInit { ty, name, value } => {
                let id = self.declare(name, ty.ty, block);
                self.assign(id, name, value, block);
            }
            ast::StmtKind::Assign { name, op, value } => {
                let Some(id) = self.var(name) else {
                    // Still check the value, for its own errors.
                    let _ = self.value(value);
                    return;
                };
                match binop_of(*op) {
                    None => self.assign(id, name, value, block),
                    Some(op) => {
                        // `let x op= e` means exactly `let x = x op e`.
                        let desugared = ast::Expr {
                            kind: ast::ExprKind::Binary {
                                op,
                                lhs: Box::new(ast::Expr {
                                    kind: ast::ExprKind::Var(name.clone()),
                                    span: name.span,
                                }),
                                rhs: Box::new(value.clone()),
                            },
                            span: s.span,
                        };
                        self.assign(id, name, &desugared, block);
                    }
                }
            }
            ast::StmtKind::Print(e) => {
                if let Some(value) = self.value(e) {
                    block.stmts.push(hir::Stmt::Print(value));
                }
            }
            ast::StmtKind::If { cond, body } => {
                if let Some((cond, body)) = self.conditional(cond, body) {
                    block.stmts.push(hir::Stmt::If { cond, body });
                }
            }
            ast::StmtKind::While { cond, body } => {
                if let Some((cond, body)) = self.conditional(cond, body) {
                    block.stmts.push(hir::Stmt::While { cond, body });
                }
            }
            ast::StmtKind::Call(c) => {
                if let Some((call, _)) = self.call(c) {
                    block.stmts.push(hir::Stmt::Call(call));
                }
            }
        }
    }

    fn assign(&mut self, id: VarId, name: &ast::Ident, value: &ast::Expr, block: &mut hir::Block) {
        let checked = self.value(value);
        // Count it as assigned even after an error, to avoid follow-on errors.
        self.assigned.insert(id);
        let Some(value_hir) = checked else { return };
        let target = self.var_type(id);
        if !assignable(target, value_hir.ty) {
            let hint = if target == Type::String {
                " (write `\"\" + x` to convert)"
            } else {
                ""
            };
            self.error(
                code::ASSIGNMENT_TYPE,
                value.span,
                format!(
                    "can't assign a {} to `{}`, which is a {}{hint}",
                    type_name(value_hir.ty),
                    name.name,
                    type_name(target)
                ),
            );
            return;
        }
        block.stmts.push(hir::Stmt::Assign {
            var: id,
            value: value_hir,
        });
    }

    /// Checks an `if` or `while`. Assignments in the body don't count after it.
    fn conditional(
        &mut self,
        cond: &ast::Expr,
        body: &[ast::Stmt],
    ) -> Option<(hir::Cond, hir::Block)> {
        let cond = self.cond(cond);
        let assigned_before = self.assigned.clone();
        self.scopes.push(Vec::new());
        let mut block = hir::Block {
            stmts: Vec::new(),
            vars: Vec::new(),
        };
        for s in body {
            self.stmt(s, &mut block);
        }
        self.scopes.pop();
        self.assigned = assigned_before;
        Some((cond?, block))
    }

    fn cond(&mut self, e: &ast::Expr) -> Option<hir::Cond> {
        match self.expr(e)? {
            Checked::Comparison(cond) => Some(cond),
            Checked::Value(_) => {
                self.error(
                    code::CONDITION_NOT_COMPARISON,
                    e.span,
                    "a condition must be a comparison (`<`, `>`, `<=` or `>=`)",
                );
                None
            }
        }
    }

    fn value(&mut self, e: &ast::Expr) -> Option<hir::Expr> {
        match self.expr(e)? {
            Checked::Value(value) => Some(value),
            Checked::Comparison(_) => {
                self.error(
                    code::COMPARISON_AS_VALUE,
                    e.span,
                    "a comparison can only be an `if` or `while` condition",
                );
                None
            }
        }
    }

    fn literal(&mut self, literal: hir::Literal, ty: Type) -> hir::Expr {
        let id = LitId(self.literals.len());
        self.literals.push(literal);
        hir::Expr {
            kind: hir::ExprKind::Lit(id),
            ty,
        }
    }

    fn expr(&mut self, e: &ast::Expr) -> Option<Checked> {
        let value = match &e.kind {
            ast::ExprKind::Number(v) => self.literal(hir::Literal::Real(*v), Type::Real),
            ast::ExprKind::Char(c) => self.literal(hir::Literal::Char(*c), Type::Char),
            ast::ExprKind::Str(s) => self.literal(hir::Literal::Str(s.clone()), Type::String),
            ast::ExprKind::Var(ident) => {
                let id = self.var(ident)?;
                if !self.assigned.contains(&id) {
                    self.error(
                        code::UNASSIGNED,
                        ident.span,
                        format!("`{}` is read before it is assigned", ident.name),
                    );
                }
                hir::Expr {
                    kind: hir::ExprKind::Var(id),
                    ty: self.var_type(id),
                }
            }
            ast::ExprKind::Call(c) => {
                let (call, returns) = self.call(c)?;
                let Some(ty) = returns else {
                    self.error(
                        code::VOID_AS_VALUE,
                        c.span,
                        format!(
                            "`{}` returns nothing, so its result can't be used",
                            c.name.name
                        ),
                    );
                    return None;
                };
                hir::Expr {
                    kind: hir::ExprKind::Call(call),
                    ty,
                }
            }
            ast::ExprKind::Binary { op, lhs, rhs } => return self.binary(*op, lhs, rhs, e.span),
        };
        Some(Checked::Value(value))
    }

    fn binary(
        &mut self,
        op: BinOp,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Option<Checked> {
        // Check both sides before giving up, so errors in each are reported.
        let (l, r) = (self.value(lhs), self.value(rhs));
        let (l, r) = (l?, r?);
        let has_string = l.ty == Type::String || r.ty == Type::String;
        let cmp = match op {
            BinOp::Lt => Some(hir::CmpOp::Lt),
            BinOp::Gt => Some(hir::CmpOp::Gt),
            BinOp::Le => Some(hir::CmpOp::Le),
            BinOp::Ge => Some(hir::CmpOp::Ge),
            _ => None,
        };
        if let Some(op) = cmp {
            if has_string {
                self.error(code::OPERAND_TYPE, span, "strings can't be compared");
                return None;
            }
            return Some(Checked::Comparison(hir::Cond { op, lhs: l, rhs: r }));
        }
        if has_string {
            if op != BinOp::Add {
                self.error(
                    code::OPERAND_TYPE,
                    span,
                    format!(
                        "`{}` can't be used with a string (only `+` can)",
                        op.symbol()
                    ),
                );
                return None;
            }
            return Some(Checked::Value(hir::Expr {
                kind: hir::ExprKind::Concat {
                    lhs: Box::new(l),
                    rhs: Box::new(r),
                },
                ty: Type::String,
            }));
        }
        let ty = if l.ty == Type::Char || r.ty == Type::Char {
            Type::Char
        } else {
            Type::Real
        };
        let op = match op {
            BinOp::Add => hir::ArithOp::Add,
            BinOp::Sub => hir::ArithOp::Sub,
            BinOp::Mul => hir::ArithOp::Mul,
            BinOp::Div => hir::ArithOp::Div,
            _ => hir::ArithOp::Rem,
        };
        Some(Checked::Value(hir::Expr {
            kind: hir::ExprKind::Arith {
                op,
                lhs: Box::new(l),
                rhs: Box::new(r),
            },
            ty,
        }))
    }

    /// Checks a call, returning it with its return type.
    fn call(&mut self, c: &ast::Call) -> Option<(hir::Call, Option<Type>)> {
        let args: Vec<_> = c.args.iter().map(|a| self.value(a)).collect();
        let callee = match self.lookup(&c.name.name).map(|s| s.binding) {
            Some(Binding::Extern(id)) => id,
            Some(Binding::Var(_)) => {
                self.error(
                    code::NOT_AN_EXTERN,
                    c.name.span,
                    format!("`{}` is a variable, not an extern", c.name.name),
                );
                return None;
            }
            None => {
                self.error(
                    code::NOT_AN_EXTERN,
                    c.name.span,
                    format!(
                        "no extern named `{}` is declared (externs must be declared before use)",
                        c.name.name
                    ),
                );
                return None;
            }
        };
        let signature = self.externs.get(callee.0).cloned()?;
        if args.len() != signature.params.len() {
            self.error(
                code::ARGUMENT_COUNT,
                c.span,
                format!(
                    "`{}` takes {}, but the call passes {}",
                    c.name.name,
                    count(signature.params.len(), "argument"),
                    args.len()
                ),
            );
            return None;
        }
        let mut checked = Vec::new();
        let mut ok = true;
        for ((arg, param), node) in args.into_iter().zip(&signature.params).zip(&c.args) {
            match arg {
                Some(arg) if assignable(*param, arg.ty) => checked.push(arg),
                Some(arg) => {
                    ok = false;
                    self.error(
                        code::ARGUMENT_TYPE,
                        node.span,
                        format!(
                            "expected a {} argument, found a {}",
                            type_name(*param),
                            type_name(arg.ty)
                        ),
                    );
                }
                None => ok = false,
            }
        }
        ok.then_some((
            hir::Call {
                callee,
                args: checked,
            },
            signature.returns,
        ))
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn binop_of(op: AssignOp) -> Option<BinOp> {
    match op {
        AssignOp::Set => None,
        AssignOp::Add => Some(BinOp::Add),
        AssignOp::Sub => Some(BinOp::Sub),
        AssignOp::Mul => Some(BinOp::Mul),
        AssignOp::Div => Some(BinOp::Div),
        AssignOp::Rem => Some(BinOp::Rem),
    }
}

#[cfg(test)]
mod tests;
