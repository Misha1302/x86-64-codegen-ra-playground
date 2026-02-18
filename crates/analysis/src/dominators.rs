use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use ir::{successors, BlockId, Function};

/// Simple dominator computation (MVP).
#[derive(Debug, Clone)]
pub struct Dominators {
    pub dom: IndexMap<BlockId, IndexSet<BlockId>>,
}

pub fn compute_dominators(f: &Function) -> Result<Dominators> {
    let blocks: Vec<BlockId> = f.blocks.iter().map(|b| b.id).collect();
    let all: IndexSet<BlockId> = blocks.iter().copied().collect();

    let mut dom: IndexMap<BlockId, IndexSet<BlockId>> = IndexMap::new();
    for &b in &blocks {
        if b == f.entry {
            let mut s = IndexSet::new();
            s.insert(b);
            dom.insert(b, s);
        } else {
            dom.insert(b, all.clone());
        }
    }

    let preds = build_preds(f);

    let mut changed = true;
    while changed {
        changed = false;
        for &b in &blocks {
            if b == f.entry { continue; }
            let mut new = all.clone();
            for p in preds.get(&b).into_iter().flat_map(|s| s.iter()) {
                let pd = dom.get(p).unwrap();
                new = new.intersection(pd).copied().collect();
            }
            new.insert(b);
            if new != dom[&b] {
                dom.insert(b, new);
                changed = true;
            }
        }
    }

    Ok(Dominators { dom })
}

fn build_preds(f: &Function) -> IndexMap<BlockId, IndexSet<BlockId>> {
    let mut preds: IndexMap<BlockId, IndexSet<BlockId>> = IndexMap::new();
    for b in &f.blocks {
        for s in successors(&b.term) {
            preds.entry(s).or_default().insert(b.id);
        }
    }
    preds
}
