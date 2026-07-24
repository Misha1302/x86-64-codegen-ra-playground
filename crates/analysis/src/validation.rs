use std::collections::{HashMap, HashSet, VecDeque};

use ir::{defs, successors, term_uses, uses, BlockId, Function, Inst, VReg};
use thiserror::Error;

use crate::{build_predecessors, compute_dominators};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("function has no blocks")]
    EmptyFunction,
    #[error("duplicate block id {0:?}")]
    DuplicateBlock(BlockId),
    #[error("entry block {0:?} is missing")]
    MissingEntry(BlockId),
    #[error("block {block:?} targets unknown block {target:?}")]
    UnknownSuccessor { block: BlockId, target: BlockId },
    #[error("reachable block {0:?} is missing")]
    MissingReachableBlock(BlockId),
    #[error("function contains unreachable blocks")]
    UnreachableBlocks,
    #[error("failed to analyse the control-flow graph: {0}")]
    Analysis(String),
    #[error("phi nodes must be contiguous at the start of block {0:?}")]
    PhiAfterInstruction(BlockId),
    #[error("argument index {index} is out of range for {args} arguments")]
    ArgumentOutOfRange { index: u32, args: u32 },
    #[error("value {0:?} has multiple definitions")]
    MultipleDefinitions(VReg),
    #[error("predecessor information for block {0:?} is missing")]
    MissingPredecessors(BlockId),
    #[error("entry block {0:?} must not contain phi nodes")]
    PhiInEntry(BlockId),
    #[error("phi in {block:?} contains duplicate predecessor {predecessor:?}")]
    DuplicatePhiPredecessor {
        block: BlockId,
        predecessor: BlockId,
    },
    #[error("phi in {block:?} references non-predecessor {predecessor:?}")]
    PhiReferencesNonPredecessor {
        block: BlockId,
        predecessor: BlockId,
    },
    #[error("phi uses undefined value {0:?}")]
    UndefinedPhiInput(VReg),
    #[error(
        "definition of phi input {value:?} in {definition:?} does not dominate edge {predecessor:?} -> {block:?}"
    )]
    PhiInputNotDominated {
        value: VReg,
        definition: BlockId,
        predecessor: BlockId,
        block: BlockId,
    },
    #[error("phi in {0:?} must have exactly one input for every predecessor")]
    IncompletePhi(BlockId),
    #[error("use of undefined value {0:?}")]
    UndefinedValue(VReg),
    #[error("value {value:?} is used before its definition in block {block:?}")]
    UseBeforeDefinition { value: VReg, block: BlockId },
    #[error("definition of {value:?} in {definition:?} does not dominate use in {use_block:?}")]
    UseNotDominated {
        value: VReg,
        definition: BlockId,
        use_block: BlockId,
    },
}

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
) -> Result<(), ValidationError> {
    let definition = definitions
        .get(&value)
        .ok_or(ValidationError::UndefinedValue(value))?;

    if definition.block == use_block {
        if definition.inst_index >= use_index {
            return Err(ValidationError::UseBeforeDefinition {
                value,
                block: use_block,
            });
        }
    } else if !dominators.dominates(definition.block, use_block) {
        return Err(ValidationError::UseNotDominated {
            value,
            definition: definition.block,
            use_block,
        });
    }

    Ok(())
}

pub fn validate_function(f: &Function) -> Result<(), ValidationError> {
    if f.blocks.is_empty() {
        return Err(ValidationError::EmptyFunction);
    }

    let mut block_ids = HashSet::new();
    for block in &f.blocks {
        if !block_ids.insert(block.id) {
            return Err(ValidationError::DuplicateBlock(block.id));
        }
    }
    if !block_ids.contains(&f.entry) {
        return Err(ValidationError::MissingEntry(f.entry));
    }

    for block in &f.blocks {
        for target in successors(&block.term) {
            if !block_ids.contains(&target) {
                return Err(ValidationError::UnknownSuccessor {
                    block: block.id,
                    target,
                });
            }
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
            .ok_or(ValidationError::MissingReachableBlock(block_id))?;
        queue.extend(successors(&block.term));
    }
    if reachable.len() != f.blocks.len() {
        return Err(ValidationError::UnreachableBlocks);
    }

    let predecessors =
        build_predecessors(f).map_err(|error| ValidationError::Analysis(error.to_string()))?;
    let dominators =
        compute_dominators(f).map_err(|error| ValidationError::Analysis(error.to_string()))?;
    let mut definitions = HashMap::new();

    for block in &f.blocks {
        let mut saw_non_phi = false;
        for (index, inst) in block.insts.iter().enumerate() {
            match inst {
                Inst::PhiI64 { .. } if saw_non_phi => {
                    return Err(ValidationError::PhiAfterInstruction(block.id));
                }
                Inst::PhiI64 { .. } => {}
                _ => saw_non_phi = true,
            }

            if let Inst::ArgI64 { idx, .. } = inst {
                if idx.0 >= f.args {
                    return Err(ValidationError::ArgumentOutOfRange {
                        index: idx.0,
                        args: f.args,
                    });
                }
            }

            if let Some(value) = defs(inst) {
                if definitions
                    .insert(
                        value,
                        Definition {
                            block: block.id,
                            inst_index: index,
                        },
                    )
                    .is_some()
                {
                    return Err(ValidationError::MultipleDefinitions(value));
                }
            }
        }
    }

    for block in &f.blocks {
        let expected_predecessors = predecessors
            .get(&block.id)
            .ok_or(ValidationError::MissingPredecessors(block.id))?;

        for (index, inst) in block.insts.iter().enumerate() {
            if let Inst::PhiI64 { incoming, .. } = inst {
                if block.id == f.entry {
                    return Err(ValidationError::PhiInEntry(block.id));
                }

                let mut incoming_blocks = HashSet::new();
                for (predecessor, value) in incoming {
                    if !incoming_blocks.insert(*predecessor) {
                        return Err(ValidationError::DuplicatePhiPredecessor {
                            block: block.id,
                            predecessor: *predecessor,
                        });
                    }
                    if !expected_predecessors.contains(predecessor) {
                        return Err(ValidationError::PhiReferencesNonPredecessor {
                            block: block.id,
                            predecessor: *predecessor,
                        });
                    }

                    let definition = definitions
                        .get(value)
                        .ok_or(ValidationError::UndefinedPhiInput(*value))?;
                    if definition.block != *predecessor
                        && !dominators.dominates(definition.block, *predecessor)
                    {
                        return Err(ValidationError::PhiInputNotDominated {
                            value: *value,
                            definition: definition.block,
                            predecessor: *predecessor,
                            block: block.id,
                        });
                    }
                }

                if incoming_blocks.len() != expected_predecessors.len() {
                    return Err(ValidationError::IncompletePhi(block.id));
                }
            } else {
                for value in uses(inst) {
                    ensure_use_dominated(value, block.id, index, &definitions, &dominators)?;
                }
            }
        }

        for value in term_uses(&block.term) {
            ensure_use_dominated(
                value,
                block.id,
                block.insts.len(),
                &definitions,
                &dominators,
            )?;
        }
    }

    Ok(())
}
