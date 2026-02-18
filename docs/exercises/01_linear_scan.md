# Exercise 01 — Linear Scan (усиление)

## Цель
Улучшить референсный linear scan:
- splitting,
- better spill heuristics,
- move coalescing.

## Checklist
- [ ] Реализовать splitting: если интервал не помещается, разрезать на куски
- [ ] Спилл-эвристика: выбрать интервал с максимальной "стоимостью" (веса использования)
- [ ] Предварительное coalescing для mov
- [ ] Метрики: spills, reloads, stack bytes

## What to measure
- В `examples/basicblock` уменьшить количество spills при `--regs 3..5`.
