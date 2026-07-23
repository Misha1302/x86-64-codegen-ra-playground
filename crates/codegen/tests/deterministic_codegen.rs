use alloc::{Allocator, PhysRegSet};
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use anyhow::Result;
use codegen::emit_function_i64;
use ir::examples;

#[test]
fn emit_is_deterministic_for_basicblock() -> Result<()> {
    let f = examples::basicblock()?;
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;
    let asg = LinearScan.allocate(&li, &regs, 4)?;

    let e1 = emit_function_i64(&f, &asg, &regs)?;
    let e2 = emit_function_i64(&f, &asg, &regs)?;

    assert_eq!(e1.bytes, e2.bytes, "codegen produced different bytes");
    assert_eq!(e1.metrics.code_size, e2.metrics.code_size);
    assert_eq!(e1.metrics.loads, e2.metrics.loads);
    assert_eq!(e1.metrics.stores, e2.metrics.stores);
    Ok(())
}
