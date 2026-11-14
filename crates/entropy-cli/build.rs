//! Compiles the runtime into a static library for `entropy build` to link into executables.
//!
//! The runtime has no dependencies, so plain `rustc` can build it from its source, which
//! `entropy-lang-runtime` passes on through its `links` metadata. This needs no nested cargo and
//! works the same from a crates.io install (spec 6).

use std::path::Path;
use std::process::Command;

fn main() {
    let src = env("DEP_ENTROPY_RUNTIME_SRC");
    let out = env("OUT_DIR");
    let out = Path::new(&out);
    for entry in std::fs::read_dir(&src).unwrap_or_else(|e| panic!("can't list {src}: {e}")) {
        let path = entry.map(|e| e.path()).unwrap_or_default();
        println!("cargo::rerun-if-changed={}", path.display());
    }
    let status = Command::new(env("RUSTC"))
        .args(["--edition", "2024", "--crate-name", "entropy_runtime"])
        .args(["--crate-type", "staticlib", "--cfg", "feature=\"capi\""])
        .args(["-C", "opt-level=3", "-C", "panic=abort"])
        .args([
            "-C",
            "lto=fat",
            "-C",
            "codegen-units=1",
            "-C",
            "debuginfo=0",
        ])
        .args(["--cap-lints", "allow", "--target", &env("TARGET")])
        .arg(format!(
            "--print=native-static-libs={}",
            out.join("native-libs.txt").display()
        ))
        .arg("-o")
        .arg(out.join("libentropy_runtime.a"))
        .arg(Path::new(&src).join("lib.rs"))
        .status()
        .unwrap_or_else(|e| panic!("can't run rustc: {e}"));
    assert!(status.success(), "compiling the runtime failed");
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}
