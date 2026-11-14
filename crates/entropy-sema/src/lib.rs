// User input must never cause a panic.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod check;
pub mod hir;

pub use check::check;

use entropy_syntax::diagnostic::{Diagnostic, Severity};

/// Parses and checks source. Sema is skipped when parsing reports an error, to avoid follow-on
/// errors. The HIR is returned only when there are no errors at all.
pub fn analyze(src: &str) -> (Option<hir::Program>, Vec<Diagnostic>) {
    let (program, mut diagnostics) = entropy_syntax::parser::parse(src);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return (None, diagnostics);
    }
    let (hir, more) = check(&program);
    diagnostics.extend(more);
    (hir, diagnostics)
}
