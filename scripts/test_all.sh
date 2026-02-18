#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "== Toolchain =="
rustc --version
cargo --version
echo

echo "== Build runner (needed for e2e) =="
cargo build -p runner
echo

echo "== Run all tests (workspace) =="
cargo test --workspace
echo

echo "== Optional: run with backtraces for easier debugging =="
# Uncomment if you want noisy failures:
# RUST_BACKTRACE=1 cargo test --workspace

echo
echo "✅ ALL TESTS PASSED"
