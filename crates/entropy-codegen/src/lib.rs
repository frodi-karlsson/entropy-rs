//! Cranelift code generation for Entropy (spec 6).

pub mod aot;
pub mod jit;
mod lower;

pub use lower::{Lowered, Rt, lower};

#[derive(Debug)]
pub enum Error {
    /// Externs arrive in milestone 3.
    Externs,
    Module(Box<cranelift_module::ModuleError>),
    /// The host can't be targeted.
    Host(String),
    /// A bug in sema or here: the HIR broke an invariant.
    Internal(&'static str),
}

impl From<cranelift_module::ModuleError> for Error {
    fn from(e: cranelift_module::ModuleError) -> Self {
        Error::Module(Box::new(e))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Externs => write!(f, "externs are not supported yet"),
            Error::Module(e) => write!(f, "code generation failed: {e}"),
            Error::Host(e) => write!(f, "can't compile for this machine: {e}"),
            Error::Internal(what) => write!(f, "internal error: {what}"),
        }
    }
}

#[cfg(test)]
mod tests;
