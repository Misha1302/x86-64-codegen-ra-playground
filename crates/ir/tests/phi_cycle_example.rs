use anyhow::Result;
use ir::{Block, BlockId, Function, Inst, Terminator, VReg};
use smallvec::smallvec;

#[test]
fn build_phi_edge_function_interprets() -> Result<()> {
    let entry = BlockId(0);
    let then_bb = BlockId(1);
    let else_bb = BlockId(2);
    let join = BlockId(3);

    let left = VReg(0);
    let right = VReg(1);
    let condition = VReg(2);
    let phi_right = VReg(3);
    let phi_left = VReg(4);
    let negative_one = VReg(5);
    let negative_right = VReg(6);
    let result = VReg(7);

    let entry_block = Block {
        id: entry,
        insts: vec![
            Inst::ArgI64 {
                dst: left,
                idx: ir::ArgId(0),
            },
            Inst::ArgI64 {
                dst: right,
                idx: ir::ArgId(1),
            },
            Inst::CmpGtI64 {
                dst: condition,
                a: left,
                b: right,
            },
        ],
        term: Terminator::Br {
            cond: condition,
            then_bb,
            else_bb,
        },
    };
    let then_block = Block {
        id: then_bb,
        insts: vec![],
        term: Terminator::Jmp { target: join },
    };
    let else_block = Block {
        id: else_bb,
        insts: vec![],
        term: Terminator::Jmp { target: join },
    };
    let join_block = Block {
        id: join,
        insts: vec![
            Inst::PhiI64 {
                dst: phi_right,
                incoming: smallvec![(then_bb, right), (else_bb, right)],
            },
            Inst::PhiI64 {
                dst: phi_left,
                incoming: smallvec![(then_bb, left), (else_bb, left)],
            },
            Inst::ConstI64 {
                dst: negative_one,
                imm: -1,
            },
            Inst::MulI64 {
                dst: negative_right,
                a: negative_one,
                b: phi_right,
            },
            Inst::AddI64 {
                dst: result,
                a: phi_left,
                b: negative_right,
            },
        ],
        term: Terminator::Ret { value: result },
    };

    let function = Function {
        name: "phi_edge_moves".into(),
        args: 2,
        entry,
        blocks: vec![entry_block, then_block, else_block, join_block],
    };

    let interpreter = ir::interp::Interpreter;
    assert_eq!(interpreter.eval_i64(&function, &[10, 7])?, 3);
    assert_eq!(interpreter.eval_i64(&function, &[7, 10])?, -3);
    Ok(())
}
