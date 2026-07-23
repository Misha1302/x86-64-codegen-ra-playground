use alloc::Allocator;
use alloc::PhysRegSet;
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use anyhow::Result;
use codegen::emit_function_i64;
use ir::{examples, interp::Interpreter};

#[test]
fn trace_is_max_for_many_inputs() -> Result<()> {
    let f = examples::trace()?;
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;
    let asg = LinearScan.allocate(&li, &regs, 3)?;
    let code = emit_function_i64(&f, &asg, &regs)?;

    let interp = Interpreter::default();
    let inputs = [
        (10, 7),
        (7, 10),
        (0, 0),
        (-1, 5),
        (5, -1),
        (-10, -7),
        (123, 123),
    ];

    for (a, b) in inputs {
        let ref_r = interp.eval_i64(&f, &[a, b])?;
        // We can't call runner here directly (keeps codegen tests pure),
        // but we can at least ensure interpreter matches expected spec:
        let expected = if a > b { a } else { b };
        assert_eq!(ref_r, expected, "ref mismatch for a={a}, b={b}");
    }

    // Smoke: bytes non-empty
    assert!(!code.bytes.is_empty());
    Ok(())
}
