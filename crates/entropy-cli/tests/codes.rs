//! Every diagnostic code in the registry has at least one golden case that produces it.

#[test]
fn every_code_has_a_golden_case() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry =
        std::fs::read_to_string(root.join("crates/entropy-syntax/src/diagnostic.rs")).unwrap();
    let codes: Vec<&str> = registry
        .split("Code(\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|code| !code.starts_with("E9"))
        .collect();
    assert!(codes.len() > 20, "found only {} codes", codes.len());

    let mut expected = String::new();
    let mut stack = vec![root.join("tests/cases")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "stderr") {
                expected.push_str(&std::fs::read_to_string(path).unwrap());
            }
        }
    }
    let missing: Vec<_> = codes
        .iter()
        .filter(|code| !expected.contains(&format!("[{code}]")))
        .collect();
    assert!(missing.is_empty(), "codes with no golden case: {missing:?}");
}
