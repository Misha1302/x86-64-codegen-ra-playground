# Exercise 03 — Exact allocator (скелет)

Для маленького блока:
- ILP / CP-SAT (OR-Tools) модель:
  - переменные: vreg->reg/stack
  - ограничения: interference, callee/caller-saved, class constraints
  - objective: minimize spills + moves + cost
- Самописный backtracking + pruning.

Интеграция: crate `alloc_exact` + флаг CLI.
