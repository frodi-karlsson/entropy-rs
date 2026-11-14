use crate::{jit, lower};

fn clif(src: &str) -> String {
    let (hir, diagnostics) = entropy_sema::analyze(src);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let mut module = jit::module().unwrap();
    let clif = lower(&mut module, &hir.unwrap()).unwrap().clif;
    // The calling convention depends on the host. The rest is the same on every 64-bit host.
    clif.replace(" apple_aarch64", "").replace(" system_v", "")
}

#[test]
fn arithmetic() {
    insta::assert_snapshot!(clif(
        "Program a b [ declare x real; let x = 1; let x = 1 + 2 * x; print x % 3; ]"
    ));
}

#[test]
fn concat() {
    insta::assert_snapshot!(clif(
        "Program a b [ declare s string; let s = \"hi\" + 'c'; print s + 1; ]"
    ));
}

#[test]
fn loop_with_scoped_string() {
    insta::assert_snapshot!(clif(
        "Program a b [ declare n real; let n = 0; while n < 3 [ declare s string; let s = \"x\"; print s; let n = n + 1; ] ]"
    ));
}

#[test]
fn externs_are_rejected() {
    let (hir, _) = entropy_sema::analyze("Program a b [ extern f(real); f(1); ]");
    let mut module = jit::module().unwrap();
    assert!(matches!(
        lower(&mut module, &hir.unwrap()),
        Err(crate::Error::Externs)
    ));
}
