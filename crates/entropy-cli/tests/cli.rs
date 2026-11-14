use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn entropy(args: &[&str], src: &str) -> Output {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    // Tests run in parallel, so each gets its own file.
    let file = dir.join(format!("{}.en", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&file, src).unwrap();
    Command::new(env!("CARGO_BIN_EXE_entropy"))
        .args(args)
        .arg(&file)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

const GOOD: &str = "Program a b [ real x = 1; print x; ]";

#[test]
fn check_accepts_a_valid_program() {
    let out = entropy(&["check"], GOOD);
    assert_eq!(code(&out), 0);
    assert!(out.stderr.is_empty());
}

#[test]
fn check_reports_errors() {
    let out = entropy(&["check"], "Program a b [ print y; ]");
    assert_eq!(code(&out), 1);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[E0015]") && stderr.contains("print y"),
        "{stderr}"
    );
    assert!(!stderr.contains('\u{1b}'), "colour codes with NO_COLOR set");
}

#[test]
fn warnings_still_pass() {
    let out = entropy(&["check"], "Program a b [ extern printf(string); ]");
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stderr).contains("[W0001]"));
}

#[test]
fn usage_errors_exit_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_entropy"))
        .arg("check")
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
    let out = Command::new(env!("CARGO_BIN_EXE_entropy"))
        .args(["check", "/no/such/file.en"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn dump() {
    let ast = entropy(&["dump", "ast"], GOOD);
    assert_eq!(code(&ast), 0);
    assert!(String::from_utf8_lossy(&ast.stdout).contains("DeclareInit"));
    let hir = entropy(&["dump", "hir"], GOOD);
    assert_eq!(code(&hir), 0);
    assert!(String::from_utf8_lossy(&hir.stdout).contains("literals"));
    assert_eq!(
        code(&entropy(&["dump", "hir"], "Program a b [ print y; ]")),
        1
    );
}

#[test]
fn run_prints_output() {
    let out = entropy(&["run", "--mutation-rate", "0"], GOOD);
    assert_eq!(code(&out), 0);
    assert_eq!(out.stdout, b"1");
}

#[test]
fn run_rejects_a_bad_rate() {
    for rate in ["-1", "0.0001", "101", "x"] {
        let out = entropy(&["run", "--mutation-rate", rate], GOOD);
        assert_eq!(code(&out), 2, "rate {rate}");
    }
}

#[test]
fn run_reports_errors() {
    let out = entropy(&["run"], "Program a b [ print y; ]");
    assert_eq!(code(&out), 1);
    assert!(out.stdout.is_empty());
}

#[test]
fn run_rejects_externs_for_now() {
    let out = entropy(&["run"], "Program a b [ extern f(real); f(1); ]");
    assert_eq!(code(&out), 3);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "entropy: externs are not supported yet\n"
    );
}
