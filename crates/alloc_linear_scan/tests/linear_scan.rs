use alloc::{verify_assignment, Allocator, PhysRegSet};
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use anyhow::Result;
use ir::examples;

#[test]
fn handles_zero_to_all_registers() -> Result<()> {
    let regs = PhysRegSet::default_gp_with_scratch();
    for function in [examples::basicblock()?, examples::phi_swap_loop()?] {
        let intervals = compute_live_intervals(&function)?;
        for register_count in 0..=regs.regs.len() {
            let assignment = LinearScan.allocate(&intervals, &regs, register_count)?;
            verify_assignment(&intervals, &regs, register_count, &assignment)?;
            if register_count == 0 {
                assert_eq!(assignment.spills as usize, intervals.intervals.len());
            }
        }
    }
    Ok(())
}
