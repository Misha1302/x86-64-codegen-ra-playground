# x86-64 Codegen & Register Allocation Lab

[![CI](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml/badge.svg)](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml)

An experimental compiler backend for making code-generation invariants explicit and testable: SSA/CFG validity, liveness, `phi` semantics, spills, register assignment, and generated-code correctness.

The project is intentionally compact. It does not try to imitate a production backend end-to-end; it isolates hard backend mechanisms so they can be checked by independent oracles and compared under the same correctness boundary. Architectural decisions, invariants, failure modes, and extension rules are documented in [`DESIGN.md`](DESIGN.md).

```text
IR -> SSA/CFG validation -> liveness -> register allocation
   -> assignment verification -> SysV x86-64 emission
   -> isolated native execution -> comparison with reference interpreter
```

## What it demonstrates

- a compact `i64` IR with `arg`, `const`, `mov`, `add`, `mul`, `cmpgt`, and `phi`;
- `jmp`, `br`, and `ret` control flow;
- a parser and reference interpreter;
- reachability, single-definition, dominance, and `phi` validation;
- edge-specific liveness for `phi` operands;
- live intervals and an interference graph;
- deterministic linear scan allocation;
- experimental simulated annealing with a fixed seed;
- allocator-independent assignment verification;
- spill support and safe stack-slot reuse after values die;
- a SysV x86-64 emitter built with `iced-x86`;
- edge-based `phi` lowering with explicit parallel-copy cycle breaking;
- native-vs-interpreter differential execution;
- a separate scalar/SSE demonstration for summing eight `f32` values.

## Quick start

Requires Linux x86-64 and stable Rust. Graphviz is needed only for CFG visualization.

```bash
git clone https://github.com/Misha1302/x86-64-codegen-ra-playground.git
cd x86-64-codegen-ra-playground

cargo test --workspace --all-targets

cargo run -p cli -- run --example basicblock --regs 4 --dump-disasm
cargo run -p cli -- run --example trace --alloc sim-anneal --regs 2
cargo run -p cli -- run --example loop-sum --regs 1
cargo run -p cli -- run --example phi-swap-loop --regs 0

cargo run -p viz -- --example basicblock --out /tmp/basic.dot
dot -Tpng /tmp/basic.dot -o /tmp/basic.png
```

Examples: `basicblock`, `trace`, `loop-sum`, `phi-swap-loop`.

Allocators: `linear-scan`, `sim-anneal`.

`--regs 0` forces all values into stack slots, which is a simple way to exercise the spill path without constructing a special input.

## Correctness model

Each layer has its own oracle:

- IR validation returns a typed error for the violated invariant;
- assignment verification rejects missing values, unavailable registers, overlapping live ranges, and unsafe stack-slot reuse;
- codegen tests cover arguments, spills, branches, and parallel `phi` moves;
- the CLI executes the same IR in the reference interpreter and in generated native code and compares observable results;
- runner tests cover large output and termination of non-returning generated code under a timeout.

The full local check mirrors CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --all-targets --release
```

## Repository map

```text
crates/ir/                  IR, parser, examples, interpreter
crates/analysis/            validation, dominators, liveness, intervals
crates/alloc/               allocator contract and assignment verifier
crates/alloc_linear_scan/   linear-scan allocator
crates/alloc_sim_anneal/    simulated annealing and conflict-free repair
crates/codegen/             x86-64 emitter, spills, phi lowering, disassembly
crates/runner/              generated-code execution in a child process
crates/cli/                 pipeline assembly and interpreter comparison
tools/viz/                  CFG export to Graphviz
```

A new allocator should implement `alloc::Allocator`, return an `Assignment` for every live interval, pass the shared `verify_assignment`, and then be exercised by the existing differential tests under branches, loops, and different levels of register pressure.

## Why native code runs out of process

A code-emitter bug can crash or loop forever. The CLI therefore hands generated machine code to a small runner process, reads `stdout` and `stderr` concurrently, and enforces a timeout.

The runner applies `no_new_privs`, resource limits for CPU, memory, and file descriptors, and changes executable memory from `RW` to `RX`. This is fault containment for development, not a security sandbox: it does not use seccomp or namespaces and is not designed for hostile machine code.

Do not use it to execute untrusted input on a production machine.

## Deliberate limitations

The current model does not cover calls, memory IR, register classes, callee-saved register allocation, live-range splitting, coalescing, a spill-cost model, graph coloring, PBQP, exact allocation, or a complete AVX/AVX2 backend.

The SIMD command demonstrates code generation and execution only. It is not a reproducible performance benchmark, so its results should not be generalized without a controlled measurement setup.
