use alloc::{Allocator, PhysRegSet};
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use codegen::{emit_function_i64, disasm};
use ir::examples;

#[test]
fn emit_disasm_nonempty() {
    let f = examples::basicblock().unwrap();
    let li = compute_live_intervals(&f).unwrap();
    let regs = PhysRegSet::default_gp_with_scratch();
    let asg = LinearScan.allocate(&li, &regs, 3).unwrap();
    let code = emit_function_i64(&f, &asg, &regs).unwrap();
    let text = disasm(&code.bytes, 0);
    assert!(!text.is_empty());
}
