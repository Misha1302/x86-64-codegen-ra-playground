use alloc::Allocator;
use alloc::PhysRegSet;
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use anyhow::Result;
use ir::examples;

#[test]
fn linear_scan_is_deterministic() -> Result<()> {
    let f = examples::basicblock()?;
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;

    let a1 = LinearScan.allocate(&li, &regs, 4)?;
    let a2 = LinearScan.allocate(&li, &regs, 4)?;

    assert_eq!(a1.spills, a2.spills);
    assert_eq!(a1.stack_slots, a2.stack_slots);
    assert_eq!(a1.map, a2.map);
    Ok(())
}
