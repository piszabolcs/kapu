#!/usr/bin/env sh
# Local equivalent of the CI quality gates (NFR-03). POSIX sh.
set -eu
cd "$(dirname "$0")/.."
run() { echo "==> $1"; shift; "$@"; }
run fmt    cargo fmt --all -- --check
run clippy cargo clippy --workspace --all-targets -- -D warnings
run test   cargo test --workspace
run deny   cargo deny check
run audit  cargo audit
echo "All checks passed."
