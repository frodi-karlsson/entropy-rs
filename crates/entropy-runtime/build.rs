//! Tells crates that depend on this one directly where the source is, so `entropy-lang` can
//! compile it into the static library that built executables link (spec 6).

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    println!("cargo::metadata=src={dir}/src");
}
