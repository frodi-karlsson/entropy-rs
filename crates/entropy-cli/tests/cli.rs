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

const SQRT: &str = "Program a b [ extern sqrt(real) returns real; print sqrt(16); ]";
const MISSING: &str =
    "Program a b [ extern no_such_function_anywhere(); no_such_function_anywhere(); ]";

#[test]
fn run_finds_libm_with_no_flags() {
    let out = entropy(&["run", "--mutation-rate", "0"], SQRT);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"4");
}

#[test]
fn run_reports_a_missing_extern() {
    let out = entropy(&["run"], MISSING);
    assert_eq!(code(&out), 3);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "entropy: can't find extern `no_such_function_anywhere`. Pass the library that defines \
         it with --dylib.\n"
    );
}

#[test]
fn run_ignores_externs_that_are_never_called() {
    let out = entropy(
        &["run"],
        "Program a b [ extern no_such_function_anywhere(); ]",
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn run_reports_a_bad_dylib() {
    let out = entropy(&["run", "--dylib", "/nonexistent/libx.so"], SQRT);
    assert_eq!(code(&out), 3);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("entropy: can't open /nonexistent/libx.so"),
        "{stderr}"
    );
}

/// A fresh output path in the test directory.
fn out_path(ext: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("out{}{ext}", NEXT.fetch_add(1, Ordering::Relaxed)));
    path.to_string_lossy().into_owned()
}

fn build_good() -> String {
    let exe = out_path("");
    let out = entropy(&["build", "--mutation-rate", "0", "-o", &exe], GOOD);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    exe
}

#[test]
fn build_writes_an_executable() {
    let out = Command::new(build_good()).output().unwrap();
    assert_eq!(code(&out), 0);
    assert_eq!(out.stdout, b"1");
    assert!(out.stderr.is_empty());
}

#[test]
fn built_executables_check_their_environment() {
    let exe = build_good();
    for var in ["ENTROPY_SEED", "ENTROPY_FUEL"] {
        let out = Command::new(&exe).env(var, "-1").output().unwrap();
        assert_eq!(code(&out), 2, "{var}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!(
                "entropy: {var} must be a whole number from 0 to {}, got `-1`\n",
                u64::MAX
            )
        );
    }
}

#[test]
fn build_emits_objects_and_ir() {
    let obj = out_path(".o");
    assert_eq!(
        code(&entropy(&["build", "--emit", "obj", "-o", &obj], GOOD)),
        0
    );
    assert!(std::fs::metadata(&obj).unwrap().len() > 0);
    let clif = out_path(".clif");
    assert_eq!(
        code(&entropy(&["build", "--emit", "clif", "-o", &clif], GOOD)),
        0
    );
    let text = std::fs::read_to_string(&clif).unwrap();
    assert!(text.contains("call") && text.contains("return"), "{text}");
}

#[test]
fn build_links_libm_with_no_flags() {
    let exe = out_path("");
    let out = entropy(&["build", "--mutation-rate", "0", "-o", &exe], SQRT);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(Command::new(exe).output().unwrap().stdout, b"4");
}

#[test]
fn build_reports_a_missing_extern() {
    let out = entropy(&["build", "-o", &out_path("")], MISSING);
    assert_eq!(code(&out), 3);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("entropy: linking failed")
            && stderr.contains("no_such_function_anywhere"),
        "{stderr}"
    );
}

#[test]
fn build_without_a_linker_explains() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("no-linker.en");
    std::fs::write(&file, GOOD).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_entropy"))
        .args(["build", "-o", &out_path("")])
        .arg(&file)
        .env("CC", "/nonexistent/cc")
        .output()
        .unwrap();
    assert_eq!(code(&out), 3);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("can't find the C linker"), "{stderr}");
}

/// Prints about 6 MiB, more than any pipe buffer holds.
const LOUD: &str = "Program a b [ real n = 0; while n < 100000 [
    print \"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\\n\";
    let n = n + 1;
] ]";

/// Reads one byte, closes the pipe, and expects a quiet exit 0.
fn exits_quietly_when_stdout_closes(mut cmd: Command) {
    use std::io::Read;
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut byte = [0u8];
    child.stdout.take().unwrap().read_exact(&mut byte).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{:?}", out.status);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_exits_quietly_when_stdout_closes() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("loud.en");
    std::fs::write(&file, LOUD).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_entropy"));
    cmd.args(["run", "--mutation-rate", "0"]).arg(file);
    exits_quietly_when_stdout_closes(cmd);
}

#[test]
fn executables_exit_quietly_when_stdout_closes() {
    let exe = out_path("");
    let out = entropy(&["build", "--mutation-rate", "0", "-o", &exe], LOUD);
    assert_eq!(code(&out), 0);
    exits_quietly_when_stdout_closes(Command::new(exe));
}

/// `show2(s, s)` must consume `s` and fix the first argument's text before evaluating the second.
/// `print s; print s;` makes the same draws on the reference, so the texts must match it.
#[test]
fn extern_arguments_are_fixed_in_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dylib = out_path(".so");
    let cc = Command::new("cc")
        .args(["-shared", "-fPIC", "-o", &dylib])
        .arg(root.join("tests/cases/ffi/helpers.c"))
        .output()
        .unwrap();
    assert!(
        cc.status.success(),
        "{}",
        String::from_utf8_lossy(&cc.stderr)
    );

    let decl = "declare s string; let s = \"abcdefgh\";";
    let ffi = format!("Program a b [ extern show2(string, string); {decl} show2(s, s); ]");
    let plain = format!("Program a b [ {decl} print s; print s; ]");
    let mut told_apart = false;
    for seed in ["1", "2", "3", "4"] {
        let args = [
            "run",
            "--seed",
            seed,
            "--mutation-rate",
            "50",
            "--dylib",
            &dylib,
        ];
        let jit = String::from_utf8(entropy(&args, &ffi).stdout).unwrap();
        let mut out = Vec::new();
        let (exit, _) = entropy_reference::run_source(
            &plain,
            "t.en",
            50.0,
            Some(seed.parse().unwrap()),
            None,
            false,
            &mut out,
        );
        assert_eq!(exit, 0);
        let texts: Vec<char> = String::from_utf8(out).unwrap().chars().collect();
        let (first, second) = texts.split_at(8);
        let expected = format!(
            "[{}|{}]",
            first.iter().collect::<String>(),
            second.iter().collect::<String>()
        );
        assert_eq!(jit, expected, "seed {seed}");
        told_apart |= first != second;
    }
    assert!(told_apart, "no seed tells the order apart");
}
