# x86-64 Codegen & Register Allocation Playground

Учебный репозиторий для практики **SSA-подобной IR**, **анализа живости**, **аллокации регистров** и **генерации x86-64** (включая минимальный SIMD пример).

> ⚠️ **Безопасность:** проект генерирует нативный машинный код. Запуск происходит **только** в отдельном процессе (`runner`) с базовыми ограничениями (rlimit + `no_new_privs`). **Не** запускайте непроверенные входные IR/байткод-файлы на боевой машине.

## Цели

- Потренироваться в аллокации регистров: *linear scan* (в MVP), дальше — графовая раскраска, PBQP, exact NP (план/скелет).
- Потренироваться в оптимизациях: live-range splitting, rematerialization, coalescing (план/упражнения).
- Генерировать и дисассемблировать x86-64 через **iced-x86**.
- Валидировать семантику: интерпретатор IR vs сгенерированный код.
- Считать метрики: spills, loads/stores, code size, simulated cost, micro-bench (SIMD vs scalar).

---

## Quickstart

Требуется: **Rust (stable)**.

```bash
git clone <this-repo>
cd x86-64-codegen-ra-playground
cargo test
cargo run -p cli -- run --example basicblock --regs 4 --dump-disasm
cargo run -p cli -- run --example trace --regs 3 --dump-disasm
cargo run -p cli -- simd-bench --target auto
cargo run -p viz -- --example basicblock --out /tmp/basic.dot
dot -Tpng /tmp/basic.dot -o /tmp/basic.png
```

### Что вы увидите

- Таблицу назначений VReg → PhysReg/Stack
- Кол-во spills и вставленных loads/stores
- Размер кода (байты)
- Дисассемблинг
- Для SIMD: сравнение scalar vs SIMD по времени и по инструкциям (микробенч в `runner`)

---

## Learning path (этапы обучения)

1. **IR и интерпретатор**
   - Добавьте инструкции, типы, простые оптимизации (CSE, const-fold).
2. **Liveness → live intervals**
   - Убедитесь, что интервалы совпадают с ожиданиями на примерах.
3. **Linear Scan**
   - Добавьте live-range splitting, heuristics выбора spill-кандидата.
4. **Interference Graph**
   - Экспорт `.dot`, визуально проверяйте coalescing.
5. **Graph Coloring**
   - Реализуйте Greedy Chaitin/Briggs + coalescing (см. `docs/exercises/02_graph_coloring.md`).
6. **PBQP**
   - Подключите внешний solver (или реализуйте малый PBQP).
7. **Exact allocator**
   - ILP/CP-SAT (OR-Tools) для маленьких блоков + самописный backtracking+pruning.
8. **SIMD / Vectorization**
   - Добавьте шаблоны и авто-векторизацию для простых паттернов.
9. **Валидация/инварианты**
   - SSA invariants, dominator tree, корректность φ.
10. **Metrics & Bench**
   - Реальные замеры, perf counters (опционально), отчёты.

---

## Архитектура

```
crates/
  ir/                  SSA-подобная IR, парсер, генераторы примеров, интерпретатор
  analysis/             liveness, live intervals, interference graph, dominators (частично)
  alloc/                API аллокатора (плагин-интерфейс), общие типы Locations
  alloc_linear_scan/    референс: linear scan allocator (GP regs)
  codegen/              lowering + emitter на iced-x86, disasm, метрики codegen
  runner/               отдельный процесс: mmap+exec байткода, sandbox-ish, bench
  cli/                  CLI: generate/run/report, вызывает runner
tools/
  viz/                  генерация Graphviz .dot (interference graph + intervals)
docs/
  exercises/            задания и чек-листы
.github/workflows/ci.yml
```

### Extensibility / плагины

Аллокаторы подключаются через trait `Allocator` (crate `alloc`). Для расширений (PBQP/ILP/CP-SAT) — добавляйте crates, реализующие этот trait, и регистрируйте их в CLI.

---

## CPU target (SSE2/AVX/AVX2)

- `cli simd-bench --target auto` — выберет лучшую доступную цель.
- `--target sse2|avx|avx2` — принудительно.
- Если CPU не поддерживает, будет fallback на scalar.

---

## Acceptance criteria (MVP)

- `cargo test` проходит.
- `cargo run -p cli -- run --example basicblock` показывает assignment/метрики и дисассембл.
- Запуск сгенерированного кода происходит **в отдельном процессе** (`runner`).
- Есть 2 примера: `basicblock` и `trace`, плюс SIMD micro-bench.
- Есть экспорт `.dot` графа интерференции.

---

## Добавление своего аллокатора

1. Создайте crate `crates/alloc_my_allocator`.
2. Зависимости: `alloc`, `analysis`, `ir`.
3. Реализуйте `alloc::Allocator`:
   - вход: `analysis::LiveIntervals`, `alloc::PhysRegSet`, политика (кол-во регов)
   - выход: `alloc::Assignment` (VReg → Location, + spill slots)
4. Зарегистрируйте его в `crates/cli/src/allocators.rs`.

Смотрите пример: `crates/alloc_linear_scan`.

---

## Документация / упражнения

См. `docs/exercises/`:
- `01_linear_scan.md`
- `02_graph_coloring.md` (скелет)
- `03_exact_allocator.md` (скелет)
- `04_simd.md` (скелет)
