use anyhow::Result;
use alloc::{Allocator, PhysRegSet};
use alloc_linear_scan::LinearScan;
use analysis::compute_live_intervals;
use codegen::emit_function_i64;
use ir::{Block, BlockId, Function, Inst, Terminator, VReg};
use smallvec::smallvec;

/// CFG where join needs to assign two values with potential cycles in locations.
/// We validate via interpreter equivalence indirectly by ensuring codegen succeeds and bytes non-empty.
/// Full e2e semantic equality is covered by cli tests.
#[test]
fn phi_parallel_move_cycle_codegen_smoke() -> Result<()> {
    let entry = BlockId(0);
    let then_bb = BlockId(1);
    let else_bb = BlockId(2);
    let join = BlockId(3);

    let a0 = VReg(0);
    let a1 = VReg(1);
    let c = VReg(2);

    let x = VReg(3);
    let y = VReg(4);
    let sum = VReg(5);

    let b0 = Block {
        id: entry,
        insts: vec![
            Inst::ArgI64 { dst: a0, idx: ir::ArgId(0) },
            Inst::ArgI64 { dst: a1, idx: ir::ArgId(1) },
            Inst::CmpGtI64 { dst: c, a: a0, b: a1 },
        ],
        term: Terminator::Br { cond: c, then_bb, else_bb },
    };
    let b1 = Block { id: then_bb, insts: vec![], term: Terminator::Jmp { target: join } };
    let b2 = Block { id: else_bb, insts: vec![], term: Terminator::Jmp { target: join } };

    // join:
    // x = phi(then: a1, else: a0)
    // y = phi(then: a0, else: a1)
    // return x + y (== a0 + a1 always)
    let b3 = Block {
        id: join,
        insts: vec![
            Inst::PhiI64 { dst: x, incoming: smallvec![(then_bb, a1), (else_bb, a0)] },
            Inst::PhiI64 { dst: y, incoming: smallvec![(then_bb, a0), (else_bb, a1)] },
            Inst::AddI64 { dst: sum, a: x, b: y },
        ],
        term: Terminator::Ret { value: sum },
    };

    let f = Function { name: "phi_cycle_swap".into(), args: 2, entry, blocks: vec![b0, b1, b2, b3] };

    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;
    let asg = LinearScan.allocate(&li, &regs, 2)?; // force tight reg pressure
    let code = emit_function_i64(&f, &asg, &regs)?;
    assert!(!code.bytes.is_empty());
    Ok(())
}
