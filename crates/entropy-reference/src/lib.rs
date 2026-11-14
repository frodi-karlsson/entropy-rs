//! A small evaluator over HIR, used as the oracle for codegen in tests. It has no FFI.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use entropy_runtime::{CharObj, Halt, Runtime, StrObj};
use entropy_sema::hir::{
    ArithOp, Block, CmpOp, Cond, Expr, ExprKind, Literal, Program, Stmt, Type,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Halt(Halt),
    /// The program declares an extern, which the evaluator can't call.
    Externs,
    /// A bug in sema or here: the HIR broke an invariant.
    Internal(&'static str),
}

impl From<Halt> for Error {
    fn from(halt: Halt) -> Self {
        Error::Halt(halt)
    }
}

type Char = Rc<RefCell<CharObj>>;
type Str = Rc<RefCell<StrObj>>;

/// A variable or literal slot.
enum Slot {
    Real(f32),
    Char(Option<Char>),
    Str(Option<Str>),
}

/// A value after its consumption step.
enum Consumed {
    Real(f32),
    Char(Char, f32),
    Str(Str),
}

impl Consumed {
    fn number(&self) -> Result<f32, Error> {
        match self {
            Consumed::Real(v) | Consumed::Char(_, v) => Ok(*v),
            Consumed::Str(_) => Err(Error::Internal("string used as a number")),
        }
    }

    /// The value fixed for concatenation, as display code points (spec 4.1).
    fn snapshot(&self) -> StrObj {
        match self {
            Consumed::Real(v) => StrObj::from_real(*v),
            Consumed::Char(_, v) => StrObj::from_char(*v),
            Consumed::Str(s) => s.borrow().snapshot(),
        }
    }
}

/// Runs a program. Output goes through the runtime, which the caller flushes.
pub fn run<W: Write>(program: &Program, rt: &mut Runtime<W>) -> Result<(), Error> {
    if !program.externs.is_empty() {
        return Err(Error::Externs);
    }
    let slot = |ty| match ty {
        Type::Real => Slot::Real(0.0),
        Type::Char => Slot::Char(None),
        Type::String => Slot::Str(None),
    };
    let mut eval = Eval {
        rt,
        vars: program.vars.iter().map(|v| slot(v.ty)).collect(),
        literals: program
            .literals
            .iter()
            .map(|l| match l {
                Literal::Real(v) => Slot::Real(*v),
                Literal::Char(c) => {
                    Slot::Char(Some(Rc::new(RefCell::new(CharObj(*c as u32 as f32)))))
                }
                Literal::Str(s) => Slot::Str(Some(Rc::new(RefCell::new(StrObj::from_text(s))))),
            })
            .collect(),
    };
    eval.block(&program.body)
}

struct Eval<'a, W: Write> {
    rt: &'a mut Runtime<W>,
    vars: Vec<Slot>,
    literals: Vec<Slot>,
}

impl<W: Write> Eval<'_, W> {
    fn block(&mut self, block: &Block) -> Result<(), Error> {
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        for id in &block.vars {
            self.reset(id.0);
        }
        Ok(())
    }

    /// Starts a variable unassigned, releasing what it held.
    fn reset(&mut self, var: usize) {
        if let Some(slot) = self.vars.get_mut(var) {
            match slot {
                Slot::Real(v) => *v = 0.0,
                Slot::Char(c) => *c = None,
                Slot::Str(s) => *s = None,
            }
        }
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), Error> {
        match stmt {
            Stmt::Declare(id) => self.reset(id.0),
            Stmt::Assign { var, value } => {
                let value = self.consume(value)?;
                let slot = self
                    .vars
                    .get_mut(var.0)
                    .ok_or(Error::Internal("unknown variable"))?;
                match (slot, value) {
                    (Slot::Real(v), Consumed::Real(x) | Consumed::Char(_, x)) => *v = x,
                    (Slot::Char(c), Consumed::Real(x)) => {
                        *c = Some(Rc::new(RefCell::new(CharObj(x))));
                    }
                    (Slot::Char(c), Consumed::Char(obj, _)) => *c = Some(obj),
                    (Slot::Str(s), Consumed::Str(obj)) => *s = Some(obj),
                    _ => return Err(Error::Internal("assignment type mismatch")),
                }
            }
            Stmt::Print(e) => match self.consume(e)? {
                Consumed::Real(v) => self.rt.print_real(v)?,
                Consumed::Char(_, v) => self.rt.print_char(v)?,
                Consumed::Str(s) => self.rt.print_str(&s.borrow())?,
            },
            Stmt::If { cond, body } => {
                if self.test(cond)? {
                    self.block(body)?;
                }
            }
            Stmt::While { cond, body } => {
                while self.test(cond)? {
                    self.block(body)?;
                }
            }
            Stmt::Call(_) => return Err(Error::Externs),
        }
        Ok(())
    }

    fn test(&mut self, cond: &Cond) -> Result<bool, Error> {
        let a = self.consume(&cond.lhs)?.number()?;
        let b = self.consume(&cond.rhs)?.number()?;
        // Rust's float comparisons are IEEE ordered: anything with NaN is false.
        Ok(match cond.op {
            CmpOp::Lt => a < b,
            CmpOp::Gt => a > b,
            CmpOp::Le => a <= b,
            CmpOp::Ge => a >= b,
        })
    }

    /// Evaluates an expression and applies its consumption step (spec 4.3).
    fn consume(&mut self, e: &Expr) -> Result<Consumed, Error> {
        let slot = match &e.kind {
            ExprKind::Var(id) => self.vars.get_mut(id.0),
            ExprKind::Lit(id) => self.literals.get_mut(id.0),
            ExprKind::Arith { op, lhs, rhs } => {
                let a = self.consume(lhs)?.number()?;
                let b = self.consume(rhs)?.number()?;
                let mut v = match op {
                    ArithOp::Add => a + b,
                    ArithOp::Sub => a - b,
                    ArithOp::Mul => a * b,
                    ArithOp::Div => a / b,
                    ArithOp::Rem => a % b,
                };
                if e.ty == Type::Char {
                    let obj = Rc::new(RefCell::new(CharObj(v)));
                    let v = self.rt.consume_char(&mut obj.borrow_mut())?;
                    return Ok(Consumed::Char(obj, v));
                }
                return Ok(Consumed::Real(self.rt.consume_real(&mut v)?));
            }
            ExprKind::Concat { lhs, rhs } => {
                let a = self.consume(lhs)?.snapshot();
                let b = self.consume(rhs)?.snapshot();
                let obj = Rc::new(RefCell::new(a.concat(&b)));
                self.rt.consume_str(&mut obj.borrow_mut())?;
                return Ok(Consumed::Str(obj));
            }
            ExprKind::Call(_) => return Err(Error::Externs),
        };
        match slot.ok_or(Error::Internal("unknown slot"))? {
            Slot::Real(v) => Ok(Consumed::Real(self.rt.consume_real(v)?)),
            Slot::Char(Some(obj)) => {
                let obj = obj.clone();
                let v = self.rt.consume_char(&mut obj.borrow_mut())?;
                Ok(Consumed::Char(obj, v))
            }
            Slot::Str(Some(obj)) => {
                let obj = obj.clone();
                self.rt.consume_str(&mut obj.borrow_mut())?;
                Ok(Consumed::Str(obj))
            }
            Slot::Char(None) | Slot::Str(None) => Err(Error::Internal("unassigned read")),
        }
    }
}

#[cfg(test)]
mod tests;
