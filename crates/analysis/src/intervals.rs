use anyhow::Result;
use indexmap::IndexMap;
use ir::{term_uses, uses, defs, Function, VReg};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Pos(pub u32);

#[derive(Debug, Clone)]
pub struct LiveInterval {
    pub v: VReg,
    pub start: Pos,
    pub end: Pos, // inclusive
}

#[derive(Debug, Clone)]
pub struct LiveIntervals {
    pub intervals: Vec<LiveInterval>,
    pub positions: IndexMap<(ir::BlockId, usize), Pos>, // (block, inst_idx_or_term) -> pos
}

pub fn compute_live_intervals(f: &Function) -> Result<LiveIntervals> {
    // Simple linear positions by block order; each inst increments, terminator increments.
    let mut pos: u32 = 0;
    let mut positions = IndexMap::new();
    for b in &f.blocks {
        for (i, _) in b.insts.iter().enumerate() {
            positions.insert((b.id, i), Pos(pos));
            pos += 1;
        }
        positions.insert((b.id, b.insts.len()), Pos(pos)); // terminator
        pos += 1;
    }

    let mut first: IndexMap<VReg, Pos> = IndexMap::new();
    let mut last: IndexMap<VReg, Pos> = IndexMap::new();

    for b in &f.blocks {
        for (i, inst) in b.insts.iter().enumerate() {
            let p = positions[&(b.id, i)];
            if let Some(d) = defs(inst) {
                first.entry(d).or_insert(p);
                last.insert(d, p);
            }
            for u in uses(inst) {
                first.entry(u).or_insert(p);
                last.insert(u, p);
            }
        }
        let tp = positions[&(b.id, b.insts.len())];
        for u in term_uses(&b.term) {
            first.entry(u).or_insert(tp);
            last.insert(u, tp);
        }
    }

    let mut intervals = Vec::new();
    for (v, s) in first {
        let e = *last.get(&v).unwrap();
        intervals.push(LiveInterval { v, start: s, end: e });
    }
    intervals.sort_by_key(|i| i.start);

    Ok(LiveIntervals { intervals, positions })
}
