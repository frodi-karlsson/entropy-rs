//! Lexing, parsing and checking never panic. A program that parses also survives a round trip
//! through the pretty-printer.

#![no_main]

use entropy_syntax::diagnostic::Severity;
use entropy_syntax::parser::parse;
use entropy_syntax::pretty::pretty;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let _ = entropy_sema::analyze(src);

    let (program, diagnostics) = parse(src);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return;
    }
    let printed = pretty(&program);
    let (reparsed, diagnostics) = parse(&printed);
    assert!(
        diagnostics.is_empty(),
        "pretty output doesn't parse:\n{printed}"
    );
    assert_eq!(pretty(&reparsed), printed);
});
