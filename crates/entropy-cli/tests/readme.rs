//! Every code block tagged `entropy` in the README must pass `entropy check`.

use std::process::Command;

#[test]
fn readme_programs_check() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("readme");
    std::fs::create_dir_all(&dir).unwrap();
    let mut blocks = 0;
    let mut lines = readme.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "```entropy" {
            continue;
        }
        let program: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "```").collect();
        let file = dir.join(format!("{blocks}.en"));
        std::fs::write(&file, program.join("\n")).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_entropy"))
            .arg("check")
            .arg(&file)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "README block {blocks} fails check:\n{}\n{}",
            program.join("\n"),
            String::from_utf8_lossy(&out.stderr)
        );
        blocks += 1;
    }
    assert!(blocks >= 3, "found only {blocks} entropy blocks");
}
