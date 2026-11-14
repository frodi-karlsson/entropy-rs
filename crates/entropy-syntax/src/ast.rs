//! The syntax tree. Parentheses leave no node: grouping is the tree's shape.

use crate::diagnostic::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub namespace: Ident,
    pub name: Ident,
    pub items: Vec<Item>,
    pub span: Span,
}

/// A name, normalised for comparison (see [`crate::lexer::normalize_ident`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Extern(Extern),
    Stmt(Stmt),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extern {
    pub name: Ident,
    /// The name exactly as written, used as the C symbol.
    pub symbol: String,
    pub params: Vec<TypeRef>,
    pub returns: Option<TypeRef>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Real,
    Char,
    String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeRef {
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Empty,
    /// `declare x real;`
    Declare {
        name: Ident,
        ty: TypeRef,
    },
    /// `real x = e;`
    DeclareInit {
        ty: TypeRef,
        name: Ident,
        value: Expr,
    },
    /// `let x op e;`
    Assign {
        name: Ident,
        op: AssignOp,
        value: Expr,
    },
    Print(Expr),
    If {
        cond: Expr,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Call(Call),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Number(f32),
    Str(String),
    Char(char),
    Var(Ident),
    Call(Call),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Gt,
    Le,
    Ge,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub name: Ident,
    pub args: Vec<Expr>,
    pub span: Span,
}
