# x86-64 Codegen & Register Allocation Playground

[![CI](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml/badge.svg)](https://github.com/Misha1302/x86-64-codegen-ra-playground/actions/workflows/ci.yml)

Это небольшой учебный backend компилятора. Я сделал его, чтобы отдельно разобраться с теми частями генерации кода, которые обычно теряются внутри большого проекта: живостью значений, `phi`-узлами, spills, назначением регистров и проверкой результата аллокатора.

Рабочий путь выглядит так:

```text
IR -> проверка SSA/CFG -> анализ живости -> распределение регистров
   -> проверка раскладки -> генерация x86-64 SysV-кода
   -> запуск в отдельном процессе -> сравнение с интерпретатором
```

Проект не претендует на production-ready backend. Его цель — сделать инварианты видимыми и дать место для экспериментов с разными аллокаторами.

## Что уже работает

- компактная `i64` IR с `arg`, `const`, `mov`, `add`, `mul`, `cmpgt` и `phi`;
- переходы `jmp`, `br` и `ret`;
- parser и эталонный интерпретатор;
- проверка достижимости, единственности определений, dominance и корректности `phi`;
- учёт `phi`-входов на конкретных рёбрах CFG;
- live intervals и граф конфликтов;
- детерминированный linear scan;
- экспериментальный simulated annealing с фиксируемым seed;
- verifier раскладки, который запрещает конфликты и разрешает переиспользовать stack slot только после смерти значения;
- x86-64 SysV emitter на `iced-x86`;
- lowering `phi` на рёбрах CFG и разрыв циклов параллельных перемещений;
- дифференциальная проверка нативного кода против интерпретатора;
- отдельная scalar/SSE демонстрация суммирования восьми `f32`.

## Быстрый запуск

Нужны Linux x86-64 и Rust stable. Graphviz требуется только для визуализации CFG.

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

Примеры: `basicblock`, `trace`, `loop-sum`, `phi-swap-loop`.

Аллокаторы: `linear-scan`, `sim-anneal`.

`--regs 0` принудительно отправляет все значения в stack slots. Это простой способ проверить путь со spills без искусственно подобранного примера.

## Как проверяется корректность

У каждого слоя свой oracle:

- validator возвращает типизированную ошибку для конкретного нарушения IR;
- verifier раскладки запрещает пропущенные значения, недоступные регистры и конфликты live intervals;
- codegen-тесты проверяют аргументы, spills, ветвления и parallel `phi` moves;
- CLI исполняет одни и те же входы в интерпретаторе и в сгенерированном x86-64 коде;
- тесты runner отдельно проверяют большой вывод и остановку зацикленного машинного кода по лимиту времени.

Полная локальная проверка совпадает с CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --all-targets --release
```

## Структура репозитория

```text
crates/ir/                  IR, parser, examples, interpreter
crates/analysis/            validation, dominators, liveness, intervals
crates/alloc/               контракт аллокатора и verifier раскладки
crates/alloc_linear_scan/   linear scan
crates/alloc_sim_anneal/    simulated annealing и conflict-free repair
crates/codegen/             x86-64 emitter, spills, phi lowering, disassembly
crates/runner/              выполнение сгенерированного кода в дочернем процессе
crates/cli/                 сборка pipeline и сравнение с интерпретатором
tools/viz/                  экспорт CFG в Graphviz
```

Новый аллокатор должен реализовать `alloc::Allocator`, вернуть `Assignment` для всех live intervals и пройти общий `verify_assignment`. После этого его нужно добавить в `crates/cli/src/allocators.rs` и включить в дифференциальные тесты на ветвлениях, циклах и разном давлении на регистры.

## Почему код запускается в отдельном процессе

Ошибка в emitter может привести к падению или бесконечному циклу. Поэтому CLI передаёт машинный код маленькому runner-процессу, читает его `stdout` и `stderr` параллельно и останавливает процесс по лимиту времени.

Runner применяет `no_new_privs`, ограничения CPU, памяти и файловых дескрипторов, а mapping переводится из `RW` в `RX`. Это уменьшает последствия ошибок, но не делает runner песочницей: здесь нет seccomp, namespaces и защиты от враждебного машинного кода.

Не используйте его для запуска непроверенного ввода на рабочей машине.

## Ограничения

Пока нет calls, memory IR, register classes, callee-saved register allocation, live-range splitting, coalescing, spill-cost model, graph coloring, PBQP, exact allocation и AVX/AVX2 backend.

SIMD-команда показывает только путь генерации и запуска кода. Это не воспроизводимое сравнение производительности, поэтому её результаты нельзя переносить на другие программы без отдельного измерения.
