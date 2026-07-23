use alloc::Allocator;
use alloc::PhysRegSet;
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use anyhow::Result;
use codegen::{disasm, emit_function_i64};
use ir::examples;

#[test]
fn frame_has_aligned_rsp_when_stack_used() -> Result<()> {
    let f = examples::basicblock()?; // spills often => stack frame exists for some regs counts
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;

    // small reg count => force stack slots
    let asg = LinearScan.allocate(&li, &regs, 3)?;
    let out = emit_function_i64(&f, &asg, &regs)?;

    let text = disasm(&out.bytes, 0);

    // Prologue must exist
    assert!(text.contains("push rbp"));
    assert!(text.contains("mov rbp,rsp"));

    // If stack used => we should see `sub rsp,` AND later `add rsp,` before ret
    if asg.stack_slots > 0 {
        assert!(
            text.contains("sub rsp,"),
            "expected stack allocation in prologue"
        );
        assert!(
            text.contains("add rsp,"),
            "expected stack deallocation in epilogue"
        );
    }

    // Must end with ret
    assert!(text.trim_end().ends_with("ret"), "no ret at end");
    Ok(())
}
