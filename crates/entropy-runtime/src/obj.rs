//! Reference-counted char and string objects for generated code (spec 5).
//!
//! This is plain Rust so it can be tested. The C ABI wraps it and aborts on an [`RcError`], which
//! only a codegen bug can cause.

use crate::{CharObj, StrObj};

#[derive(Debug)]
pub enum Value {
    Char(CharObj),
    Str(StrObj),
    /// What a freed object holds. In debug mode freed objects are kept, so a later use is caught.
    Freed,
}

#[derive(Debug)]
pub struct Obj {
    rc: usize,
    value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcError {
    UseAfterFree,
    WrongKind,
}

impl Obj {
    /// A new object with one reference.
    pub fn new(value: Value) -> Self {
        Obj { rc: 1, value }
    }

    pub fn retain(&mut self) -> Result<(), RcError> {
        self.check()?;
        self.rc += 1;
        Ok(())
    }

    /// Drops one reference. Returns whether it was the last, in which case the object is poisoned.
    pub fn release(&mut self) -> Result<bool, RcError> {
        self.check()?;
        self.rc -= 1;
        if self.rc > 0 {
            return Ok(false);
        }
        self.value = Value::Freed;
        Ok(true)
    }

    fn check(&self) -> Result<(), RcError> {
        match self.value {
            Value::Freed => Err(RcError::UseAfterFree),
            _ => Ok(()),
        }
    }

    pub fn char_mut(&mut self) -> Result<&mut CharObj, RcError> {
        match &mut self.value {
            Value::Char(c) => Ok(c),
            Value::Freed => Err(RcError::UseAfterFree),
            Value::Str(_) => Err(RcError::WrongKind),
        }
    }

    pub fn str_mut(&mut self) -> Result<&mut StrObj, RcError> {
        match &mut self.value {
            Value::Str(s) => Ok(s),
            Value::Freed => Err(RcError::UseAfterFree),
            Value::Char(_) => Err(RcError::WrongKind),
        }
    }

    pub fn str_ref(&self) -> Result<&StrObj, RcError> {
        match &self.value {
            Value::Str(s) => Ok(s),
            Value::Freed => Err(RcError::UseAfterFree),
            Value::Char(_) => Err(RcError::WrongKind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string() -> Obj {
        Obj::new(Value::Str(StrObj::from_text("hi")))
    }

    #[test]
    fn last_release_frees() {
        let mut o = string();
        o.retain().unwrap();
        assert_eq!(o.release(), Ok(false));
        assert_eq!(o.release(), Ok(true));
    }

    #[test]
    fn double_release_is_caught() {
        let mut o = string();
        assert_eq!(o.release(), Ok(true));
        assert_eq!(o.release(), Err(RcError::UseAfterFree));
        assert_eq!(o.retain(), Err(RcError::UseAfterFree));
    }

    #[test]
    fn use_after_free_is_caught() {
        let mut o = Obj::new(Value::Char(CharObj(65.0)));
        assert_eq!(o.release(), Ok(true));
        assert_eq!(o.char_mut().map(|c| c.0), Err(RcError::UseAfterFree));
    }

    #[test]
    fn wrong_kind_is_caught() {
        let mut o = Obj::new(Value::Char(CharObj(65.0)));
        assert_eq!(o.str_ref().map(|_| ()), Err(RcError::WrongKind));
        assert_eq!(o.str_mut().map(|_| ()), Err(RcError::WrongKind));
        let mut s = string();
        assert_eq!(s.char_mut().map(|_| ()), Err(RcError::WrongKind));
    }
}
