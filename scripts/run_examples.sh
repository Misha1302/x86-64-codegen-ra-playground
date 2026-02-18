#!/usr/bin/env bash
set -euo pipefail
cargo test
cargo run -p cli -- run --example basicblock --regs 4 --dump-disasm
cargo run -p cli -- run --example trace --regs 3 --dump-disasm
cargo run -p cli -- simd-bench --target auto
