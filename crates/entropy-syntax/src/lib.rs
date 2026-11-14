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

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod pretty;
