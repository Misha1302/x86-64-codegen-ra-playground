use anyhow::Result;
use ir::{Block, BlockId, Function, Inst, Terminator, VReg};
use smallvec::smallvec;

#[test]
fn build_phi_cycle_function_parses_and_interps() -> Result<()> {
    // Build a tiny CFG:
    // entry:
    //   v0 = arg0
    //   v1 = arg1
    //   cond = (v0 > v1)
    //   br cond then else
    // then: jmp join
    // else: jmp join
    // join:
    //   x = phi(then: v1, else: v1)   // x=b
    //   y = phi(then: v0, else: v0)   // y=a
    //   ret y - x  (=> a-b)
    //
    // This isn't a true swap-cycle, but it forces φ moves for both values on edges.
    // (Cycle tests are harder without explicit parallel move; this still stresses edge moves.)
    let entry = BlockId(0);
    let then_bb = BlockId(1);
    let else_bb = BlockId(2);
    let join = BlockId(3);

    let v0 = VReg(0);
    let v1 = VReg(1);
    let c = VReg(2);
    let x = VReg(3);
    let y = VReg(4);
    let negx = VReg(5);
    let res = VReg(6);

    let b0 = Block {
        id: entry,
        insts: vec![
            Inst::ArgI64 { dst: v0, idx: ir::ArgId(0) },
	    Inst::ArgI64 { dst: v1, idx: ir::ArgId(1) },

            Inst::CmpGtI64 { dst: c, a: v0, b: v1 },
        ],
        term: Terminator::Br { cond: c, then_bb, else_bb },
    };
    let b1 = Block { id: then_bb, insts: vec![], term: Terminator::Jmp { target: join } };
    let b2 = Block { id: else_bb, insts: vec![], term: Terminator::Jmp { target: join } };
    let b3 = Block {
        id: join,
        insts: vec![
            Inst::PhiI64 { dst: x, incoming: smallvec![(then_bb, v1), (else_bb, v1)] },
            Inst::PhiI64 { dst: y, incoming: smallvec![(then_bb, v0), (else_bb, v0)] },
            Inst::ConstI64 { dst: negx, imm: -1 },
            Inst::MulI64 { dst: negx, a: negx, b: x },
            Inst::AddI64 { dst: res, a: y, b: negx },
        ],
        term: Terminator::Ret { value: res },
    };

    let f = Function { name: "phi_edge_moves".into(), args: 2, entry, blocks: vec![b0, b1, b2, b3] };

    let interp = ir::interp::Interpreter::default();
    let r = interp.eval_i64(&f, &[10, 7])?;
    assert_eq!(r, 3);
    let r = interp.eval_i64(&f, &[7, 10])?;
    assert_eq!(r, -3);
    Ok(())
}
