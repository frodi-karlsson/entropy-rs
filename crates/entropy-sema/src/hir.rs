//! The typed program that codegen and the reference evaluator run.
//!
//! Every expression that is evaluated is consumed exactly once, by its parent: an operator, a
//! print, an assignment or a call argument (spec 4.3). So consumption needs no node of its own.
//! Conversions are implied too: they happen when the value's type differs from the target's.

pub use entropy_syntax::ast::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VarId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// One entry per declaration in the source, indexed by [`VarId`].
    pub vars: Vec<Var>,
    /// One entry per literal occurrence in the source, indexed by [`LitId`]. Each is a hidden
    /// slot or object created at program start.
    pub literals: Vec<Literal>,
    /// Indexed by [`ExternId`].
    pub externs: Vec<Extern>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Var {
    pub name: String,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Real(f32),
    Char(char),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extern {
    /// The C symbol, exactly as written.
    pub symbol: String,
    pub params: Vec<Type>,
    pub returns: Option<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Variables declared directly in this block, released when it ends.
    pub vars: Vec<VarId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// Starts the variable unassigned, releasing what a char or string slot held.
    Declare(VarId),
    Assign {
        var: VarId,
        value: Expr,
    },
    Print(Expr),
    If {
        cond: Cond,
        body: Block,
    },
    While {
        cond: Cond,
        body: Block,
    },
    /// A call whose result, if any, is discarded.
    Call(Call),
}

/// A comparison. Only `if` and `while` conditions have one.
#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    pub op: CmpOp,
    pub lhs: Expr,
    pub rhs: Expr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Var(VarId),
    Lit(LitId),
    /// Numeric operands. The result is a `char` if either operand is one, else a `real`.
    Arith {
        op: ArithOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// At least one operand is a `string`.
    Concat {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// A call with a return value.
    Call(Call),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub callee: ExternId,
    /// Each argument converts to its parameter type as in assignment.
    pub args: Vec<Expr>,
}
