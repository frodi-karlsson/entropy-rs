#!/bin/sh
# Proves the crates work as published: packages them, installs `entropy` from the packages alone,
# and runs and builds 99 Bottles with it. Slow, so not part of check.sh. Usage:
# scripts/package-check.sh
set -eu
cd "$(dirname "$0")/.."

version=$(cargo pkgid -p entropy-lang | sed 's/.*[#@]//')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Builds each crate from its packaged form. The runtime source comes from a registry copy.
cargo package --workspace --exclude entropy-reference --locked

# Installs from the packaged copies only. The internal crates aren't on crates.io yet, so each is
# patched to its packaged copy.
set --
for crate in syntax sema runtime codegen; do
    dir="$PWD/target/package/entropy-lang-$crate-$version"
    # The value is TOML, so the path is quoted inside the argument.
    set -- "$@" --config "patch.crates-io.entropy-lang-$crate.path=\"$dir\""
done
cargo install --locked --path "target/package/entropy-lang-$version" --root "$tmp" "$@"

case_file=tests/cases/upstream/99-bottles.en
expected=tests/cases/upstream/99-bottles.stdout
"$tmp/bin/entropy" run "$case_file" --seed 0 --fuel 1000000 > "$tmp/run.txt"
cmp "$tmp/run.txt" "$expected"
"$tmp/bin/entropy" build "$case_file" -o "$tmp/bottles"
ENTROPY_SEED=0 ENTROPY_FUEL=1000000 "$tmp/bottles" > "$tmp/build.txt"
cmp "$tmp/build.txt" "$expected"
echo "package check passed"
