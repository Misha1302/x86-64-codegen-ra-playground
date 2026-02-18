use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use ir::{successors, term_uses, uses, defs, BlockId, Function, VReg};

#[derive(Debug, Clone)]
pub struct BlockLiveness {
    pub live_in: IndexSet<VReg>,
    pub live_out: IndexSet<VReg>,
    pub use_set: IndexSet<VReg>,
    pub def_set: IndexSet<VReg>,
}

#[derive(Debug, Clone)]
pub struct Liveness {
    pub per_block: IndexMap<BlockId, BlockLiveness>,
}

pub fn compute_liveness(f: &Function) -> Result<Liveness> {
    let mut per_block: IndexMap<BlockId, BlockLiveness> = IndexMap::new();

    for b in f.blocks_in_order() {
        let mut use_set = IndexSet::new();
        let mut def_set = IndexSet::new();

        // Phi uses are conceptually on incoming edges; for MVP we treat them as normal uses here,
        // which is conservative enough for examples.
        for inst in &b.insts {
            for u in uses(inst) {
                if !def_set.contains(&u) {
                    use_set.insert(u);
                }
            }
            if let Some(d) = defs(inst) {
                def_set.insert(d);
            }
        }
        for u in term_uses(&b.term) {
            if !def_set.contains(&u) {
                use_set.insert(u);
            }
        }

        per_block.insert(b.id, BlockLiveness {
            live_in: IndexSet::new(),
            live_out: IndexSet::new(),
            use_set,
            def_set,
        });
    }

    // Iterative dataflow
    let mut changed = true;
    while changed {
        changed = false;
        for b in f.blocks.iter().rev() {
            let succs = successors(&b.term);
            let mut live_out = IndexSet::new();
            for s in succs {
                if let Some(slv) = per_block.get(&s) {
                    for v in slv.live_in.iter() { live_out.insert(*v); }
                }
            }

            let mut live_in = per_block[&b.id].use_set.clone();
            for v in live_out.iter() {
                if !per_block[&b.id].def_set.contains(v) {
                    live_in.insert(*v);
                }
            }

            let bl = per_block.get_mut(&b.id).unwrap();
            if bl.live_in != live_in || bl.live_out != live_out {
                bl.live_in = live_in;
                bl.live_out = live_out;
                changed = true;
            }
        }
    }

    Ok(Liveness { per_block })
}
