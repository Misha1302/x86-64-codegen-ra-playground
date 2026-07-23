use anyhow::{Context, Result};
use indexmap::{IndexMap, IndexSet};
use ir::{BlockId, Function};

use crate::build_predecessors;

#[derive(Debug, Clone)]
pub struct Dominators {
    pub dom: IndexMap<BlockId, IndexSet<BlockId>>,
}

impl Dominators {
    pub fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        self.dom
            .get(&block)
            .is_some_and(|set| set.contains(&dominator))
    }
}

pub fn compute_dominators(f: &Function) -> Result<Dominators> {
    let blocks: Vec<BlockId> = f.blocks.iter().map(|block| block.id).collect();
    let all: IndexSet<BlockId> = blocks.iter().copied().collect();
    anyhow::ensure!(
        all.contains(&f.entry),
        "entry block {:?} is missing",
        f.entry
    );

    let predecessors = build_predecessors(f)?;
    let mut dom = IndexMap::new();

    for block in &blocks {
        if *block == f.entry {
            dom.insert(*block, IndexSet::from_iter([*block]));
        } else {
            dom.insert(*block, all.clone());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in &blocks {
            if *block == f.entry {
                continue;
            }

            let preds = predecessors
                .get(block)
                .with_context(|| format!("missing predecessor set for {:?}", block))?;
            let mut pred_iter = preds.iter();
            let mut next = if let Some(first_pred) = pred_iter.next() {
                dom.get(first_pred)
                    .with_context(|| format!("missing dominators for {:?}", first_pred))?
                    .clone()
            } else {
                IndexSet::new()
            };

            for predecessor in pred_iter {
                let predecessor_dom = dom
                    .get(predecessor)
                    .with_context(|| format!("missing dominators for {:?}", predecessor))?;
                next.retain(|candidate| predecessor_dom.contains(candidate));
            }
            next.insert(*block);

            if dom.get(block) != Some(&next) {
                dom.insert(*block, next);
                changed = true;
            }
        }
    }

    Ok(Dominators { dom })
}
