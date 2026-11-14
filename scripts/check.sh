#!/bin/sh
# Every static check CI runs. Usage: scripts/check.sh
# Tools: cargo install cargo-deny zizmor --locked; brew install actionlint (or go install).
set -eu
cd "$(dirname "$0")/.."

for tool in cargo-deny actionlint zizmor; do
    command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

# zizmor's online audits (impostor commits, known-vulnerable actions) need a token.
if [ -z "${GH_TOKEN:-}" ] && command -v gh >/dev/null; then
    GH_TOKEN=$(gh auth token 2>/dev/null || true)
    export GH_TOKEN
fi

run() { echo "==> $*"; "$@"; }

run cargo fmt --all --check
run cargo clippy --workspace --all-targets --locked -- -D warnings
run cargo deny check
run actionlint
run zizmor --quiet .github/workflows
