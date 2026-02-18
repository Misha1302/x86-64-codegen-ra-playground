use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use ir::{term_uses, uses, defs, Function, VReg};

#[derive(Debug, Clone)]
pub struct InterferenceGraph {
    pub nodes: IndexSet<VReg>,
    pub edges: IndexMap<VReg, IndexSet<VReg>>,
}

impl InterferenceGraph {
    pub fn neighbors(&self, v: VReg) -> impl Iterator<Item=VReg> + '_ {
        self.edges.get(&v).into_iter().flat_map(|s| s.iter().copied())
    }
    pub fn has_edge(&self, a: VReg, b: VReg) -> bool {
        self.edges.get(&a).map(|s| s.contains(&b)).unwrap_or(false)
    }
}

pub fn build_interference_graph(f: &Function) -> Result<InterferenceGraph> {
    // Standard backwards walk per block, using a conservative live set.
    // MVP: no special phi edge handling.
    let mut nodes = IndexSet::new();
    let mut edges: IndexMap<VReg, IndexSet<VReg>> = IndexMap::new();

    for b in &f.blocks {
        let mut live: IndexSet<VReg> = IndexSet::new();
        for u in term_uses(&b.term) {
            live.insert(u);
            nodes.insert(u);
        }

        for inst in b.insts.iter().rev() {
            for u in uses(inst) {
                live.insert(u);
                nodes.insert(u);
            }
            if let Some(d) = defs(inst) {
                nodes.insert(d);
                // d interferes with everything live now
                for &v in live.iter() {
                    if v != d {
                        edges.entry(d).or_default().insert(v);
                        edges.entry(v).or_default().insert(d);
                    }
                }
                live.shift_remove(&d);
            }
        }
    }

    Ok(InterferenceGraph { nodes, edges })
}
