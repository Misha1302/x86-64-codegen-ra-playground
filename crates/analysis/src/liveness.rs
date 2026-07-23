use anyhow::{Context, Result};
use indexmap::{IndexMap, IndexSet};
use ir::{defs, successors, term_uses, uses, BlockId, Function, Inst, VReg};

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
    pub phi_defs: IndexMap<BlockId, IndexSet<VReg>>,
    pub phi_uses: IndexMap<(BlockId, BlockId), IndexSet<VReg>>,
}

pub fn build_predecessors(f: &Function) -> Result<IndexMap<BlockId, IndexSet<BlockId>>> {
    let block_ids: IndexSet<BlockId> = f.blocks.iter().map(|block| block.id).collect();
    let mut predecessors: IndexMap<BlockId, IndexSet<BlockId>> = f
        .blocks
        .iter()
        .map(|block| (block.id, IndexSet::new()))
        .collect();

    for block in &f.blocks {
        for successor in successors(&block.term) {
            if !block_ids.contains(&successor) {
                anyhow::bail!("block {:?} has unknown successor {:?}", block.id, successor);
            }
            predecessors
                .get_mut(&successor)
                .context("successor predecessor set")?
                .insert(block.id);
        }
    }

    Ok(predecessors)
}

fn collect_phi_data(
    f: &Function,
) -> (
    IndexMap<BlockId, IndexSet<VReg>>,
    IndexMap<(BlockId, BlockId), IndexSet<VReg>>,
) {
    let mut phi_defs: IndexMap<BlockId, IndexSet<VReg>> = IndexMap::new();
    let mut phi_uses: IndexMap<(BlockId, BlockId), IndexSet<VReg>> = IndexMap::new();

    for block in &f.blocks {
        for inst in &block.insts {
            if let Inst::PhiI64 { dst, incoming } = inst {
                phi_defs.entry(block.id).or_default().insert(*dst);
                for (pred, value) in incoming {
                    phi_uses
                        .entry((*pred, block.id))
                        .or_default()
                        .insert(*value);
                }
            }
        }
    }

    (phi_defs, phi_uses)
}

pub fn compute_liveness(f: &Function) -> Result<Liveness> {
    let (phi_defs, phi_uses) = collect_phi_data(f);
    let mut per_block: IndexMap<BlockId, BlockLiveness> = IndexMap::new();

    for block in f.blocks_in_order() {
        let mut use_set = IndexSet::new();
        let mut def_set = IndexSet::new();

        for inst in &block.insts {
            match inst {
                Inst::PhiI64 { dst, .. } => {
                    def_set.insert(*dst);
                }
                _ => {
                    for used in uses(inst) {
                        if !def_set.contains(&used) {
                            use_set.insert(used);
                        }
                    }
                    if let Some(defined) = defs(inst) {
                        def_set.insert(defined);
                    }
                }
            }
        }

        for used in term_uses(&block.term) {
            if !def_set.contains(&used) {
                use_set.insert(used);
            }
        }

        per_block.insert(
            block.id,
            BlockLiveness {
                live_in: IndexSet::new(),
                live_out: IndexSet::new(),
                use_set,
                def_set,
            },
        );
    }

    let mut changed = true;
    while changed {
        changed = false;

        for block in f.blocks.iter().rev() {
            let mut live_out = IndexSet::new();

            for successor in successors(&block.term) {
                let successor_liveness = per_block
                    .get(&successor)
                    .with_context(|| format!("missing liveness for successor {:?}", successor))?;
                let successor_phi_defs = phi_defs.get(&successor);

                for value in &successor_liveness.live_in {
                    if successor_phi_defs.is_none_or(|defs| !defs.contains(value)) {
                        live_out.insert(*value);
                    }
                }

                if let Some(edge_uses) = phi_uses.get(&(block.id, successor)) {
                    live_out.extend(edge_uses.iter().copied());
                }
            }

            let current = per_block
                .get(&block.id)
                .with_context(|| format!("missing liveness for block {:?}", block.id))?;
            let mut live_in = current.use_set.clone();
            for value in &live_out {
                if !current.def_set.contains(value) {
                    live_in.insert(*value);
                }
            }

            let current = per_block
                .get_mut(&block.id)
                .with_context(|| format!("missing mutable liveness for block {:?}", block.id))?;
            if current.live_in != live_in || current.live_out != live_out {
                current.live_in = live_in;
                current.live_out = live_out;
                changed = true;
            }
        }
    }

    Ok(Liveness {
        per_block,
        phi_defs,
        phi_uses,
    })
}
