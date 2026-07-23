use analysis::{
    build_interference_graph, compute_live_intervals, compute_liveness, validate_function,
};
use anyhow::Result;
use ir::{examples, BlockId, VReg};

#[test]
fn phi_inputs_are_live_only_on_matching_edges() -> Result<()> {
    let function = examples::trace()?;
    validate_function(&function)?;
    let liveness = compute_liveness(&function)?;

    assert!(liveness.phi_uses[&(BlockId(1), BlockId(3))].contains(&VReg(3)));
    assert!(!liveness.phi_uses[&(BlockId(1), BlockId(3))].contains(&VReg(4)));
    assert!(liveness.phi_uses[&(BlockId(2), BlockId(3))].contains(&VReg(4)));
    assert!(!liveness.per_block[&BlockId(3)].live_in.contains(&VReg(3)));
    assert!(!liveness.per_block[&BlockId(3)].live_in.contains(&VReg(4)));
    Ok(())
}

#[test]
fn loop_backedge_extends_live_intervals() -> Result<()> {
    let function = examples::phi_swap_loop()?;
    validate_function(&function)?;
    let intervals = compute_live_intervals(&function)?;
    let px = intervals
        .intervals
        .iter()
        .find(|interval| interval.v == VReg(5))
        .expect("px interval");
    let py = intervals
        .intervals
        .iter()
        .find(|interval| interval.v == VReg(6))
        .expect("py interval");
    assert!(px.overlaps(py), "loop-carried phi values must overlap");
    Ok(())
}

#[test]
fn interference_graph_contains_cross_block_conflicts() -> Result<()> {
    let function = examples::phi_swap_loop()?;
    let graph = build_interference_graph(&function)?;
    assert!(graph.has_edge(VReg(5), VReg(6)));
    assert!(graph.has_edge(VReg(5), VReg(7)));
    Ok(())
}

#[test]
fn intervals_are_stably_sorted() -> Result<()> {
    let function = examples::basicblock()?;
    let intervals = compute_live_intervals(&function)?;
    for pair in intervals.intervals.windows(2) {
        assert!(
            (pair[0].start, pair[0].end, pair[0].v.0)
                <= (pair[1].start, pair[1].end, pair[1].v.0)
        );
    }
    Ok(())
}
