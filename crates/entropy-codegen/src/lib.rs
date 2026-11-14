//! Cranelift code generation for Entropy (spec 6).

pub mod aot;
pub mod jit;
mod lower;

pub use lower::{Lowered, Rt, extern_symbols, lower};

#[derive(Debug)]
pub enum Error {
    /// An extern the JIT can't find.
    MissingSymbol(String),
    /// A `--dylib` that can't be opened.
    Dylib(std::path::PathBuf, String),
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
            Error::MissingSymbol(name) => write!(
                f,
                "can't find extern `{name}`. Pass the library that defines it with --dylib."
            ),
            Error::Dylib(path, e) => write!(f, "can't open {}: {e}", path.display()),
            Error::Module(e) => write!(f, "code generation failed: {e}"),
            Error::Host(e) => write!(f, "can't compile for this machine: {e}"),
            Error::Internal(what) => write!(f, "internal error: {what}"),
        }
    }
}

#[cfg(test)]
mod tests;
