use anyhow::{Context, Result};
use indexmap::{IndexMap, IndexSet};
use ir::{defs, term_uses, uses, Function, Inst, VReg};

use crate::compute_liveness;

#[derive(Debug, Clone)]
pub struct InterferenceGraph {
    pub nodes: IndexSet<VReg>,
    pub edges: IndexMap<VReg, IndexSet<VReg>>,
}

impl InterferenceGraph {
    pub fn neighbors(&self, value: VReg) -> impl Iterator<Item = VReg> + '_ {
        self.edges
            .get(&value)
            .into_iter()
            .flat_map(|neighbors| neighbors.iter().copied())
    }

    pub fn has_edge(&self, left: VReg, right: VReg) -> bool {
        self.edges
            .get(&left)
            .is_some_and(|neighbors| neighbors.contains(&right))
    }

    fn add_node(&mut self, value: VReg) {
        self.nodes.insert(value);
        self.edges.entry(value).or_default();
    }

    fn add_edge(&mut self, left: VReg, right: VReg) {
        if left == right {
            return;
        }
        self.add_node(left);
        self.add_node(right);
        self.edges.entry(left).or_default().insert(right);
        self.edges.entry(right).or_default().insert(left);
    }
}

pub fn build_interference_graph(f: &Function) -> Result<InterferenceGraph> {
    let liveness = compute_liveness(f)?;
    let mut graph = InterferenceGraph {
        nodes: IndexSet::new(),
        edges: IndexMap::new(),
    };

    for block in &f.blocks {
        let block_liveness = liveness
            .per_block
            .get(&block.id)
            .with_context(|| format!("missing liveness for block {:?}", block.id))?;
        let mut live = block_liveness.live_out.clone();
        live.extend(term_uses(&block.term));

        for value in &live {
            graph.add_node(*value);
        }

        for inst in block.insts.iter().rev() {
            if let Some(defined) = defs(inst) {
                graph.add_node(defined);
                for value in &live {
                    graph.add_edge(defined, *value);
                }
                live.shift_remove(&defined);
            }

            if !matches!(inst, Inst::PhiI64 { .. }) {
                for used in uses(inst) {
                    graph.add_node(used);
                    live.insert(used);
                }
            }
        }
    }

    Ok(graph)
}
