# Backend design notes

This document explains the engineering model behind the playground: what must remain true, which component owns each invariant, and how failures are detected.

## 1. Goal and non-goals

The project isolates a narrow compiler-backend pipeline so register allocation and code generation can be reasoned about independently of a large frontend or optimizer.

The goal is **not** to reproduce LLVM or a production x86 backend. The goal is to make backend contracts explicit enough that alternative allocators and lowering strategies can be compared against the same correctness boundary.

Current pipeline:

```text
IR
 -> SSA/CFG validation
 -> liveness + interference
 -> register allocation
 -> assignment verification
 -> phi lowering
 -> SysV x86-64 emission
 -> isolated native execution
 -> comparison with reference interpreter
```

## 2. Core invariants

### IR / CFG

- every value has one definition;
- uses are dominated by their definitions;
- `phi` inputs correspond to real predecessor edges;
- unreachable or malformed control flow is rejected before backend work starts.

**Owner:** `crates/analysis`.

### Liveness

A value may share a physical location with another value only when their live ranges do not overlap. `phi` operands are edge uses rather than ordinary block-local uses.

**Owner:** liveness/interference analysis in `crates/analysis`.

### Allocation

An allocator is allowed to choose registers or stack slots, but it does not define correctness. Every returned assignment is checked by a separate verifier.

The verifier rejects:

- missing assignments;
- unavailable registers;
- overlapping live values mapped to the same register;
- unsafe stack-slot reuse while both values are live.

**Owner:** allocator-independent contract in `crates/alloc`.

This separation is deliberate: a bug in an allocator should not be able to validate itself.

## 3. Phi lowering

SSA `phi` nodes describe simultaneous edge assignments. Lowering them as sequential moves is incorrect when moves form a cycle, for example `a <- b, b <- a`.

The backend therefore treats phi lowering as a parallel-copy problem and breaks cycles explicitly when necessary.

Validation includes branch, loop, spill, and parallel-phi cases.

## 4. Native execution as a differential oracle

Generated code is not considered correct merely because it assembles or executes. The same IR is executed by a reference interpreter and by generated machine code; observable results must agree.

This catches bugs that structural tests alone can miss: wrong moves, incorrect branch targets, spill/reload mistakes, and emitter errors.

Native code runs in a child process because an emitter bug may crash or loop forever. The runner applies resource limits and converts executable memory from RW to RX. This is fault containment for development, not a security sandbox.

## 5. Why two allocators

`linear-scan` is the deterministic baseline. `sim-anneal` is an experimental search-based allocator with a fixed seed and a repair step.

The purpose of keeping both behind one `Allocator` interface is not to claim that one is universally better. It makes allocator policy replaceable while preserving the same verifier and differential-execution oracle.

A meaningful future comparison should measure at least:

- spill count;
- stack slots;
- generated instruction count / code size;
- allocator runtime;
- correctness against the same corpus.

Any performance claim should be made only from a reproducible benchmark with controlled inputs and repeated runs.

## 6. Deliberate limitations

The current model does not yet cover calls, memory IR, register classes, callee-saved allocation, live-range splitting, coalescing, graph coloring/PBQP, or a complete SIMD backend.

These omissions are explicit boundaries, not hidden assumptions. Extending the backend should first identify which existing invariant changes and which verifier/test must move with it.

## 7. Evidence map

The main architectural claims are intentionally traceable to concrete implementation boundaries:

| Claim | Implementation | Primary check |
| --- | --- | --- |
| SSA/CFG and dominance invariants | `crates/analysis` | validation tests reject malformed IR before allocation/codegen |
| Liveness and interference | `crates/analysis` | branch/loop/`phi` cases exercise edge-sensitive liveness |
| Allocator-independent correctness | `crates/alloc` | `verify_assignment` checks every allocator result |
| Replaceable allocation policy | `crates/alloc_linear_scan`, `crates/alloc_sim_anneal` | both implementations pass the same verifier and pipeline tests |
| Spill and `phi` lowering | `crates/codegen` | codegen tests cover spills and parallel-copy cycles |
| Semantic equivalence of generated code | `crates/cli` + reference interpreter in `crates/ir` | native-vs-interpreter differential execution |
| Fault containment for emitter failures | `crates/runner` | timeout and large-output runner tests |

This map is not a substitute for the tests; it is a fast path from a public claim to the code boundary responsible for it.

## 8. Extension rule

A new allocator should need to know only the allocation contract, not codegen internals:

1. implement `alloc::Allocator`;
2. return an assignment for every live interval;
3. pass `verify_assignment`;
4. run through existing differential tests under branch/loop/register-pressure cases.

That boundary is the main architectural experiment of the repository: **policy is replaceable; correctness checks are shared and independent.**
