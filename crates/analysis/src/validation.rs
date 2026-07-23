use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Result};
use ir::{defs, successors, term_uses, uses, BlockId, Function, Inst, VReg};

use crate::{build_predecessors, compute_dominators};

#[derive(Debug, Copy, Clone)]
struct Definition {
    block: BlockId,
    inst_index: usize,
}

fn ensure_use_dominated(
    value: VReg,
    use_block: BlockId,
    use_index: usize,
    definitions: &HashMap<VReg, Definition>,
    dominators: &crate::Dominators,
) -> Result<()> {
    let definition = definitions
        .get(&value)
        .with_context(|| format!("use of undefined value {:?}", value))?;

    if definition.block == use_block {
        anyhow::ensure!(
            definition.inst_index < use_index,
            "value {:?} is used before its definition in block {:?}",
            value,
            use_block
        );
    } else {
        anyhow::ensure!(
            dominators.dominates(definition.block, use_block),
            "definition of {:?} in {:?} does not dominate use in {:?}",
            value,
            definition.block,
            use_block
        );
    }

    Ok(())
}

pub fn validate_function(f: &Function) -> Result<()> {
    anyhow::ensure!(!f.blocks.is_empty(), "function has no blocks");

    let mut block_ids = HashSet::new();
    for block in &f.blocks {
        anyhow::ensure!(
            block_ids.insert(block.id),
            "duplicate block id {:?}",
            block.id
        );
    }
    anyhow::ensure!(block_ids.contains(&f.entry), "entry block is missing");

    for block in &f.blocks {
        for successor in successors(&block.term) {
            anyhow::ensure!(
                block_ids.contains(&successor),
                "block {:?} targets unknown block {:?}",
                block.id,
                successor
            );
        }
    }

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([f.entry]);
    while let Some(block_id) = queue.pop_front() {
        if !reachable.insert(block_id) {
            continue;
        }
        let block = f
            .try_block(block_id)
            .with_context(|| format!("reachable block {:?} is missing", block_id))?;
        queue.extend(successors(&block.term));
    }
    anyhow::ensure!(
        reachable.len() == f.blocks.len(),
        "function contains unreachable blocks"
    );

    let predecessors = build_predecessors(f)?;
    let dominators = compute_dominators(f)?;
    let mut definitions = HashMap::new();

    for block in &f.blocks {
        let mut saw_non_phi = false;
        for (index, inst) in block.insts.iter().enumerate() {
            match inst {
                Inst::PhiI64 { .. } if saw_non_phi => {
                    anyhow::bail!(
                        "phi nodes must be contiguous at the start of block {:?}",
                        block.id
                    )
                }
                Inst::PhiI64 { .. } => {}
                _ => saw_non_phi = true,
            }

            if let Inst::ArgI64 { idx, .. } = inst {
                anyhow::ensure!(
                    idx.0 < f.args,
                    "argument index {} is out of range for {} arguments",
                    idx.0,
                    f.args
                );
            }

            if let Some(defined) = defs(inst) {
                anyhow::ensure!(
                    definitions
                        .insert(
                            defined,
                            Definition {
                                block: block.id,
                                inst_index: index,
                            },
                        )
                        .is_none(),
                    "value {:?} has multiple definitions",
                    defined
                );
            }
        }
    }

    for block in &f.blocks {
        let expected_predecessors = predecessors
            .get(&block.id)
            .with_context(|| format!("missing predecessors for block {:?}", block.id))?;

        for (index, inst) in block.insts.iter().enumerate() {
            if let Inst::PhiI64 { incoming, .. } = inst {
                anyhow::ensure!(
                    block.id != f.entry,
                    "entry block {:?} must not contain phi nodes",
                    block.id
                );
                let mut incoming_blocks = HashSet::new();
                for (predecessor, value) in incoming {
                    anyhow::ensure!(
                        incoming_blocks.insert(*predecessor),
                        "phi in {:?} contains duplicate predecessor {:?}",
                        block.id,
                        predecessor
                    );
                    anyhow::ensure!(
                        expected_predecessors.contains(predecessor),
                        "phi in {:?} references non-predecessor {:?}",
                        block.id,
                        predecessor
                    );

                    let definition = definitions
                        .get(value)
                        .with_context(|| format!("phi uses undefined value {:?}", value))?;
                    if definition.block != *predecessor {
                        anyhow::ensure!(
                            dominators.dominates(definition.block, *predecessor),
                            "definition of phi input {:?} in {:?} does not dominate edge {:?} -> {:?}",
                            value,
                            definition.block,
                            predecessor,
                            block.id
                        );
                    }
                }
                anyhow::ensure!(
                    incoming_blocks.len() == expected_predecessors.len(),
                    "phi in {:?} must have exactly one input for every predecessor",
                    block.id
                );
            } else {
                for used in uses(inst) {
                    ensure_use_dominated(used, block.id, index, &definitions, &dominators)?;
                }
            }
        }

        for used in term_uses(&block.term) {
            ensure_use_dominated(used, block.id, block.insts.len(), &definitions, &dominators)?;
        }
    }

    Ok(())
}
