use entropy_runtime::{Halt, Runtime, display_char, format_real};
use entropy_sema::analyze;

use crate::{Error, run};

/// Runs a program body, returning its output and result.
fn eval(body: &str, rate: f32, seed: u64, fuel: Option<u64>) -> (String, Result<(), Error>) {
    let src = format!("Program t t [ {body} ]");
    let (hir, diagnostics) = analyze(&src);
    let hir = hir.unwrap_or_else(|| panic!("{body:?} doesn't check: {diagnostics:?}"));
    let mut rt = Runtime::new(rate, Some(seed), fuel, Vec::new());
    let result = run(&hir, &mut rt);
    let _ = rt.flush();
    let out = String::from_utf8(rt.into_output()).unwrap_or_default();
    (out, result)
}

/// Output with decay off.
fn exact(body: &str) -> String {
    let (out, result) = eval(body, 0.0, 1, None);
    assert_eq!(result, Ok(()), "{body:?}");
    out
}

#[test]
fn arithmetic_without_decay() {
    assert_eq!(exact("print 1 + 2;"), "3");
    assert_eq!(exact("print 1 + 2 * 3;"), "7");
    assert_eq!(exact("print (1 + 2) * 3;"), "9");
    assert_eq!(exact("print 10 - 4 - 3;"), "3");
    assert_eq!(exact("print 7 % 3;"), "1");
    assert_eq!(exact("print 1 / 4;"), "0.25");
    assert_eq!(exact("print 0.1 + 0.2;"), "0.3");
}

#[test]
fn non_finite_values() {
    assert_eq!(exact("print 1 / 0;"), "inf");
    assert_eq!(exact("print 0 - 1 / 0;"), "-inf");
    assert_eq!(exact("print 0 / 0;"), "NaN");
    assert_eq!(exact("print 0 * (0 - 1);"), "-0");
}

#[test]
fn nan_comparisons_are_false() {
    let body = "real n = 0 / 0; \
                if n < 1 [ print 1; ] if n > 1 [ print 2; ] \
                if n <= 1 [ print 3; ] if n >= 1 [ print 4; ] \
                if 1 <= n [ print 5; ] if 1 >= n [ print 6; ] print 0;";
    assert_eq!(exact(body), "0");
}

#[test]
fn chars() {
    assert_eq!(exact("print 'A' + 1;"), "B");
    assert_eq!(exact("char c = 72; print c;"), "H");
    assert_eq!(exact("real r = 'a'; print r;"), "97");
    assert_eq!(exact("print 'A' * 0 - 1;"), "\u{fffd}");
}

#[test]
fn strings() {
    assert_eq!(exact("print \"a\" + 1.5 + 'b';"), "a1.5b");
    assert_eq!(exact("print 1 + \"a\";"), "1a");
    assert_eq!(exact("string s = \"ab\"; let s += s; print s;"), "abab");
    assert_eq!(exact("string s = \"x\"; let s += 'A' + 1; print s;"), "xB");
    assert_eq!(exact("print \"\" + 0 / 0;"), "NaN");
}

#[test]
fn control_flow() {
    assert_eq!(
        exact("real n = 3; while n > 0 [ print n; let n -= 1; ]"),
        "321"
    );
    assert_eq!(exact("if 1 < 2 [ print 1; ] if 2 < 1 [ print 2; ]"), "1");
    assert_eq!(
        exact("real i = 0; while i < 2 [ string s = \"i\"; let s += i; print s; let i += 1; ]"),
        "i0i1"
    );
}

/// The exact number of steps a program takes: it finishes with `n` but not `n - 1`.
fn steps(body: &str) -> u64 {
    let mut n = 0;
    loop {
        if eval(body, 0.0, 1, Some(n)).1 == Ok(()) {
            return n;
        }
        assert_eq!(
            eval(body, 0.0, 1, Some(n)).1,
            Err(Error::Halt(Halt::OutOfFuel))
        );
        n += 1;
    }
}

#[test]
fn step_counts() {
    // Each operand, temporary and assignment value is consumed once (spec 4.3).
    assert_eq!(steps("print 1;"), 1);
    assert_eq!(steps("print 1 + 2;"), 3);
    assert_eq!(steps("real a = 1;"), 1);
    assert_eq!(steps("real a = 1; real b = a;"), 2);
    assert_eq!(steps("real a = 1; real b = a + 1;"), 4);
    assert_eq!(steps("real x = 1; print x - x - x;"), 1 + 5);
    assert_eq!(steps("real x = 1; let x += 2;"), 1 + 3);
    assert_eq!(steps("declare x real;"), 0);
    assert_eq!(steps("print \"abc\";"), 3);
    assert_eq!(steps("string s = \"abc\"; print s;"), 6);
    // Both snapshots are 1 cell, then the 2-cell result is consumed by print.
    assert_eq!(steps("print 'a' + \"b\";"), 1 + 1 + 2);
    assert_eq!(steps("if 1 < 2 [ ]"), 2);
    assert_eq!(
        steps("real i = 0; while i < 2 [ let i += 1; ]"),
        1 + 3 * 2 + 2 * 3
    );
}

/// Applies `n` steps to `v` with the same RNG stream as a program run.
fn stepped(rt: &mut Runtime<Vec<u8>>, v: &mut f32, n: usize) -> f32 {
    for _ in 0..n {
        let _ = rt.step(v);
    }
    *v
}

#[test]
fn chars_are_shared_by_assignment() {
    let (out, _) = eval("char a = 'A'; char b = a; print b; print a;", 50.0, 3, None);
    // One object takes all four steps: the literal, `a`, `print b` and `print a`.
    let mut rt = Runtime::new(50.0, Some(3), None, Vec::new());
    let mut v = 65.0;
    let first = stepped(&mut rt, &mut v, 3);
    let second = stepped(&mut rt, &mut v, 1);
    assert_eq!(
        out,
        format!("{}{}", display_char(first), display_char(second))
    );
}

#[test]
fn reals_are_copied_by_assignment() {
    let (out, _) = eval("real a = 65; real b = a; print b; print a;", 2.0, 3, None);
    let mut rt = Runtime::new(2.0, Some(3), None, Vec::new());
    let mut literal = 65.0;
    let mut a = stepped(&mut rt, &mut literal, 1);
    let mut b = stepped(&mut rt, &mut a, 1);
    let printed_b = stepped(&mut rt, &mut b, 1);
    let printed_a = stepped(&mut rt, &mut a, 1);
    assert_eq!(
        out,
        format!("{}{}", format_real(printed_b), format_real(printed_a))
    );
}

#[test]
fn literals_drift_across_iterations() {
    let (out, _) = eval(
        "real i = 0; while i < 3 [ print 5; print \" \"; let i += 1; ]",
        2.0,
        8,
        None,
    );
    let values: Vec<&str> = out.split_whitespace().collect();
    // The loop count drifts too, so only check the values that were printed.
    assert!(values.len() >= 2, "{out}");
    assert!(values.windows(2).all(|w| w[0] != w[1]), "{out}");
}

#[test]
fn same_seed_same_output() {
    let body = "real x = 1; while x < 5 [ print x; let x += 1; ]";
    let a = eval(body, 2.0, 77, Some(10_000));
    assert_eq!(a, eval(body, 2.0, 77, Some(10_000)));
    assert_ne!(a.0, eval(body, 2.0, 78, Some(10_000)).0);
}

#[test]
fn externs_are_rejected() {
    assert_eq!(eval("extern f();", 0.0, 1, None).1, Err(Error::Externs));
}
