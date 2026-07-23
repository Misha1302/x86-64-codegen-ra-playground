# x86-64 Codegen & Register Allocation Playground

Учебная лаборатория для практики SSA-подобной IR, CFG-aware liveness, live intervals, register allocation и генерации x86-64 SysV-кода через `iced-x86`.

Проект намеренно небольшой, но путь исполнения настоящий:

`IR → validation → liveness/intervals → allocator → x86-64 emission → isolated runner → interpreter/native differential check`

> **Граница безопасности.** Сгенерированный код выполняется только в отдельном Linux-процессе. Runner применяет `no_new_privs`, `RLIMIT_CPU`, `RLIMIT_AS`, `RLIMIT_NOFILE`, W^X-переход `RW → RX`, а родительский CLI ограничивает время процесса. Это снижает последствия ошибок, но **не является полноценной песочницей**: seccomp и namespaces пока не реализованы. Не запускайте непроверенный машинный код на рабочей машине.

## Что реализовано

- SSA-подобная `i64` IR: `arg`, `const`, `mov`, `add`, `mul`, `cmpgt`, `phi`, `jmp`, `br`, `ret`.
- Структурный validator: уникальность определений, достижимость блоков, dominance uses, корректность φ-входов и аргументов.
- Edge-sensitive φ-liveness и CFG-aware interference graph.
- Консервативные live intervals, включая ветвления и backedges.
- Linear scan с корректным all-spill режимом при `--regs 0`.
- Детерминированный experimental simulated-annealing allocator с обязательным conflict-free repair и финальной проверкой assignment.
- x86-64 SysV emitter, stack spills, edge-specific φ lowering и parallel-move cycle breaking.
- Эталонный интерпретатор с параллельной семантикой φ и step limit.
- Scalar/SSE демонстрация суммирования восьми `f32`. AVX/AVX2 и автоматическая векторизация не заявляются.
- Differential validation native code против интерпретатора на boundary, randomized, branching и loop cases.

## Quickstart

Требуются Linux x86-64, Rust stable и, для визуализации, Graphviz.

```bash
git clone https://github.com/Misha1302/x86-64-codegen-ra-playground.git
cd x86-64-codegen-ra-playground

cargo test --workspace --all-targets
cargo run -p cli -- run --example basicblock --regs 4 --dump-disasm
cargo run -p cli -- run --example trace --alloc sim-anneal --regs 2
cargo run -p cli -- run --example loop-sum --regs 1
cargo run -p cli -- run --example phi-swap-loop --regs 0 --dump-disasm
cargo run -p cli -- simd-bench --target auto

cargo run -p viz -- --example basicblock --out /tmp/basic.dot
dot -Tpng /tmp/basic.dot -o /tmp/basic.png
```

Доступные примеры: `basicblock`, `trace`, `loop-sum`, `phi-swap-loop`.

Аллокаторы: `linear-scan` и `sim-anneal`. Допустимое число регистров: `0..=5`; ноль принудительно размещает все значения в stack slots.

## Архитектура

```text
crates/
  ir/                  IR, parser, examples, interpreter
  analysis/            validation, dominators, φ-aware liveness, intervals, interference
  alloc/               allocator contract, locations, assignment verifier
  alloc_linear_scan/   deterministic linear scan
  alloc_sim_anneal/    deterministic experimental annealing + safe repair
  codegen/             x86-64 SysV emitter, φ edge moves, disassembly, SIMD demo
  runner/              separate-process native execution with bounded resources
  cli/                 compile, report, differential validation, SIMD bench

tools/viz/             Graphviz export
docs/exercises/        follow-up exercises
```

Аллокаторы реализуют trait `alloc::Allocator`. Результат каждого allocator проходит общий `verify_assignment`: полнота, допустимые регистры, stack metadata и отсутствие register conflicts у пересекающихся intervals.

## Проверки

CI выполняет:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --all-targets --release
```

Ключевые регрессии охватывают:

- φ-use на конкретных CFG edges;
- cross-block и loop-carried liveness;
- dominance и malformed SSA;
- zero-register/all-spill allocation;
- allocator validity по нескольким seeds и уровням register pressure;
- аргументы, spills, branches, loops и parallel φ cycles;
- десятки interpreter/native differential cases для каждого встроенного примера.

## Ограничения

Это не production compiler backend. Пока отсутствуют register classes, callee-saved register support, calls, memory IR, live-range splitting, coalescing, graph coloring, PBQP, exact allocation, seccomp/namespaces и IR-driven vectorization. Соответствующие направления находятся в `docs/exercises/` и не выдаются за реализованные возможности.
