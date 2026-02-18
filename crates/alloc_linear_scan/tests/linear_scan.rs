use alloc::{PhysRegSet, Allocator};
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use ir::examples;

#[test]
fn linear_scan_allocates() {
    let f = examples::basicblock().unwrap();
    let li = compute_live_intervals(&f).unwrap();
    let regs = PhysRegSet::default_gp_with_scratch();
    let a = LinearScan.allocate(&li, &regs, 3).unwrap();
    assert!(!a.map.is_empty());
}
