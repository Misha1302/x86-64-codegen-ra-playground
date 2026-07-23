use anyhow::{Context, Result};
use indexmap::IndexMap;
use ir::{defs, term_uses, uses, Function, Inst, VReg};

use crate::compute_liveness;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Pos(pub u32);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LiveInterval {
    pub v: VReg,
    pub start: Pos,
    pub end: Pos,
}

impl LiveInterval {
    pub fn overlaps(&self, other: &Self) -> bool {
        !(self.end < other.start || other.end < self.start)
    }
}

#[derive(Debug, Clone)]
pub struct LiveIntervals {
    pub intervals: Vec<LiveInterval>,
    pub positions: IndexMap<(ir::BlockId, usize), Pos>,
}

fn touch(
    first: &mut IndexMap<VReg, Pos>,
    last: &mut IndexMap<VReg, Pos>,
    value: VReg,
    pos: Pos,
) {
    first
        .entry(value)
        .and_modify(|current| *current = (*current).min(pos))
        .or_insert(pos);
    last.entry(value)
        .and_modify(|current| *current = (*current).max(pos))
        .or_insert(pos);
}

pub fn compute_live_intervals(f: &Function) -> Result<LiveIntervals> {
    let liveness = compute_liveness(f)?;
    let mut position = 0_u32;
    let mut positions = IndexMap::new();
    let mut block_bounds = IndexMap::new();

    for block in &f.blocks {
        let start = Pos(position);
        for (index, _) in block.insts.iter().enumerate() {
            positions.insert((block.id, index), Pos(position));
            position = position
                .checked_add(1)
                .context("instruction position overflow")?;
        }
        positions.insert((block.id, block.insts.len()), Pos(position));
        let end = Pos(position);
        position = position
            .checked_add(1)
            .context("terminator position overflow")?;
        block_bounds.insert(block.id, (start, end));
    }

    let mut first = IndexMap::new();
    let mut last = IndexMap::new();

    for block in &f.blocks {
        for (index, inst) in block.insts.iter().enumerate() {
            let pos = positions[&(block.id, index)];
            if let Some(defined) = defs(inst) {
                touch(&mut first, &mut last, defined, pos);
            }
            if !matches!(inst, Inst::PhiI64 { .. }) {
                for used in uses(inst) {
                    touch(&mut first, &mut last, used, pos);
                }
            }
        }

        let term_pos = positions[&(block.id, block.insts.len())];
        for used in term_uses(&block.term) {
            touch(&mut first, &mut last, used, term_pos);
        }
    }

    for ((predecessor, _successor), values) in &liveness.phi_uses {
        let predecessor_block = f
            .try_block(*predecessor)
            .with_context(|| format!("unknown phi predecessor {:?}", predecessor))?;
        let edge_pos = positions[&(predecessor_block.id, predecessor_block.insts.len())];
        for value in values {
            touch(&mut first, &mut last, *value, edge_pos);
        }
    }

    for (block_id, block_liveness) in &liveness.per_block {
        let (start, end) = block_bounds
            .get(block_id)
            .copied()
            .with_context(|| format!("missing bounds for block {:?}", block_id))?;
        for value in &block_liveness.live_in {
            touch(&mut first, &mut last, *value, start);
        }
        for value in &block_liveness.live_out {
            touch(&mut first, &mut last, *value, end);
        }
    }

    let mut intervals = Vec::with_capacity(first.len());
    for (value, start) in first {
        let end = *last
            .get(&value)
            .with_context(|| format!("missing interval end for {:?}", value))?;
        intervals.push(LiveInterval {
            v: value,
            start,
            end,
        });
    }
    intervals.sort_by_key(|interval| (interval.start, interval.end, interval.v.0));

    Ok(LiveIntervals {
        intervals,
        positions,
    })
}
