# x86-64 Codegen & Register Allocation Playground

[![CI](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml/badge.svg)](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml)

Учебная лаборатория compiler backend’а: небольшая SSA-подобная IR проходит структурную проверку, анализ живости, аллокацию регистров, генерацию x86-64 SysV-кода и differential-проверку против интерпретатора.

```text
IR
 └─ validate SSA / CFG invariants
     └─ edge-sensitive liveness and live intervals
         └─ register allocator
             └─ assignment verifier
                 └─ x86-64 lowering and emission
                     └─ isolated runner
                         └─ interpreter/native differential oracle
```

Проект предназначен для изучения инвариантов backend’а и экспериментов с allocator’ами. Это не production compiler backend и не средство безопасного запуска недоверенного машинного кода.

## Реализовано

### IR и анализ

- `i64`-инструкции: `arg`, `const`, `mov`, `add`, `mul`, `cmpgt`, `phi`;
- terminators: `jmp`, `br`, `ret`;
- parser и эталонный интерпретатор;
- validator для уникальности определений, CFG reachability, dominance, порядка φ и состава φ-входов;
- edge-sensitive φ-liveness;
- консервативные live intervals для ветвлений и backedges;
- CFG-aware interference graph.

### Register allocation

- deterministic linear scan;
- experimental deterministic simulated annealing;
- режим `--regs 0`, принудительно размещающий значения в stack slots;
- единый verifier результата allocator’а:
  - назначение для каждого live interval;
  - только разрешённые allocatable registers;
  - запрет scratch registers;
  - согласованные `spills` и `stack_slots`;
  - отсутствие register conflicts;
  - отсутствие aliasing одного stack slot у пересекающихся live intervals;
  - reuse stack slot разрешён для непересекающихся интервалов.

### Code generation и execution

- x86-64 SysV emitter через `iced-x86`;
- stack spills и argument shadow slots;
- edge-specific φ-lowering;
- parallel-copy cycle breaking;
- scalar/SSE демонстрация `sum8(f32)`;
- запуск нативного кода только на **Linux x86-64** в отдельном процессе;
- `RW → RX` mapping, `no_new_privs`, resource limits и parent-side timeout;
- concurrent draining stdout/stderr, чтобы большой корректный вывод не блокировал runner.

## Быстрый запуск

Требуются Linux x86-64 и Rust stable. Graphviz нужен только для `.dot`-визуализации. Версии внешних crates закреплены в `Cargo.lock`.

```bash
git clone https://github.com/Misha1302/x86-64-codegen-ra-playground.git
cd x86-64-codegen-ra-playground

cargo test --workspace --all-targets
cargo run -p cli -- run --example basicblock --regs 4 --dump-disasm
cargo run -p cli -- run --example trace --alloc sim-anneal --regs 2
cargo run -p cli -- run --example loop-sum --regs 1
cargo run -p cli -- run --example phi-swap-loop --regs 0 --dump-disasm
cargo run -p cli -- simd-bench --target auto
```

Доступные примеры: `basicblock`, `trace`, `loop-sum`, `phi-swap-loop`. Allocator’ы: `linear-scan`, `sim-anneal`. Допустимое число регистров: `0..=5`.

## Что проверяет `cli run`

По умолчанию CLI:

1. валидирует IR;
2. строит live intervals;
3. запускает allocator;
4. проверяет `Assignment`;
5. генерирует машинный код;
6. выполняет boundary и deterministic-random cases в runner;
7. выполняет те же cases в интерпретаторе;
8. сравнивает результаты целиком.

Успешный вывод содержит `N differential cases passed`. Отключение через `--validate=false` предназначено только для локальной диагностики, а не для CI или ревью.

## Тестовая стратегия

Тесты организованы вокруг контрактов, а не вокруг числа файлов.

| Уровень | Защищаемый контракт | Примеры |
|---|---|---|
| IR/parser | вход разбирается однозначно либо отклоняется | terminator rules, malformed input |
| Validator | только корректная SSA/CFG достигает анализов и codegen | duplicate defs, reachability, dominance, malformed φ, invalid args |
| Liveness | φ-use принадлежит конкретному CFG edge | branches, backedges, loop-carried values |
| Assignment verifier | location assignment не разрушает одновременно живые значения | register conflicts, scratch misuse, stack-slot aliasing, metadata |
| Codegen | lowering сохраняет семантику при pressure и spills | arguments, comparisons, branches, parallel φ moves |
| Runner | отдельный процесс завершается и отдаёт полный output | timeout, 4096 cases, output larger than a typical pipe buffer |
| End-to-end | native result совпадает с reference interpreter | 4 CFG examples × 2 allocator’а × pressure `0/1/2/5` |

Полная локальная проверка:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --all-targets --release
```

CI дополнительно запускает differential smoke matrix с all-spill, branch, loop и φ-cycle cases.

## Архитектура

```text
crates/
  ir/                  IR, parser, examples, reference interpreter
  analysis/            validator, dominators, liveness, intervals, interference
  alloc/               allocator contract, locations, assignment verifier
  alloc_linear_scan/   deterministic linear scan
  alloc_sim_anneal/    deterministic annealing + conflict-free repair
  codegen/             x86-64 SysV emitter, frame/spill handling, φ edge moves
  runner/              separate-process native execution with bounded resources
  cli/                 orchestration, reports and differential validation

tools/viz/             Graphviz export
docs/exercises/        optional follow-up exercises
```

## Добавление allocator’а

1. Создайте crate, зависящий от `alloc`, `analysis` и `ir`.
2. Реализуйте `alloc::Allocator`.
3. Возвращайте полный `Assignment` для каждого interval.
4. Пропускайте результат через `verify_assignment`.
5. Зарегистрируйте allocator в `crates/cli/src/allocators.rs`.
6. Добавьте deterministic test, pressure matrix, verifier-negative test и interpreter/native differential cases для branch и loop CFG.

`Assignment` contract:

- `spills` — число virtual values, назначенных в stack;
- `stack_slots` — размер адресуемого пространства slots (`max(index) + 1`);
- один slot может переиспользоваться только непересекающимися live intervals.

## Граница безопасности runner’а

Runner уменьшает последствия ошибок generated code, но не образует security boundary для hostile input.

Есть: отдельный процесс, W^X `RW → RX`, `PR_SET_NO_NEW_PRIVS`, `RLIMIT_CPU`, `RLIMIT_AS`, `RLIMIT_NOFILE`, ограничения размера кода/cases и внешний timeout.

Нет: seccomp, namespaces/container isolation, syscall filtering и защиты от kernel-level exploits.

**Не запускайте непроверенный IR или машинный код на рабочей или боевой машине.**

## Ограничения и non-goals

Пока отсутствуют register classes, полноценные callee-saved registers, calls, memory IR, live-range splitting, coalescing, spill-cost model, graph coloring, PBQP, exact allocation, seccomp/namespaces, IR-driven vectorization и AVX/AVX2 backend.

Scalar/SSE micro-benchmark является демонстрацией code emission и runner path. Он не доказывает универсальное ускорение SIMD и не должен цитироваться как производительный benchmark без отдельной воспроизводимой методики.
