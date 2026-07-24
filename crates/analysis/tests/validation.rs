use analysis::validate_function;
use anyhow::Result;
use ir::{parser, Block, BlockId, Function, Inst, Terminator, VReg};
use smallvec::smallvec;

fn assert_validation_error(function: &Function, expected: &str) {
    let error = validate_function(function).expect_err("function must be rejected");
    let message = format!("{error:#}");
    assert!(
        message.contains(expected),
        "expected error containing {expected:?}, got {message:?}"
    );
}

#[test]
fn all_built_in_examples_validate() -> Result<()> {
    for function in [
        ir::examples::basicblock()?,
        ir::examples::trace()?,
        ir::examples::loop_sum()?,
        ir::examples::phi_swap_loop()?,
    ] {
        validate_function(&function)?;
    }
    Ok(())
}

#[test]
fn rejects_duplicate_definitions_with_specific_error() -> Result<()> {
    let function = parser::parse(
        r#"
        func duplicate args=0
        block b0:
          v0 = const 1
          v0 = const 2
          ret v0
        "#,
    )?;
    assert_validation_error(&function, "has multiple definitions");
    Ok(())
}

#[test]
fn rejects_missing_phi_predecessor() {
    let entry = BlockId(0);
    let left = BlockId(1);
    let right = BlockId(2);
    let join = BlockId(3);
    let function = Function {
        name: "bad_phi".into(),
        args: 0,
        entry,
        blocks: vec![
            Block {
                id: entry,
                insts: vec![Inst::ConstI64 {
                    dst: VReg(0),
                    imm: 1,
                }],
                term: Terminator::Br {
                    cond: VReg(0),
                    then_bb: left,
                    else_bb: right,
                },
            },
            Block {
                id: left,
                insts: vec![],
                term: Terminator::Jmp { target: join },
            },
            Block {
                id: right,
                insts: vec![],
                term: Terminator::Jmp { target: join },
            },
            Block {
                id: join,
                insts: vec![Inst::PhiI64 {
                    dst: VReg(1),
                    incoming: smallvec![(left, VReg(0))],
                }],
                term: Terminator::Ret { value: VReg(1) },
            },
        ],
    };
    assert_validation_error(&function, "exactly one input for every predecessor");
}

#[test]
fn rejects_use_not_dominated_by_definition() -> Result<()> {
    let function = parser::parse(
        r#"
        func nondom args=1
        block b0:
          v0 = arg 0
          br v0 b1 b2
        block b1:
          v1 = const 7
          jmp b3
        block b2:
          jmp b3
        block b3:
          ret v1
        "#,
    )?;
    assert_validation_error(&function, "does not dominate use");
    Ok(())
}

#[test]
fn rejects_use_before_definition_in_same_block() -> Result<()> {
    let function = parser::parse(
        r#"
        func use_before_def args=0
        block b0:
          v1 = mov v0
          v0 = const 7
          ret v1
        "#,
    )?;
    assert_validation_error(&function, "used before its definition");
    Ok(())
}

#[test]
fn rejects_unreachable_blocks() -> Result<()> {
    let function = parser::parse(
        r#"
        func unreachable args=0
        block b0:
          v0 = const 1
          ret v0
        block b1:
          v1 = const 2
          ret v1
        "#,
    )?;
    assert_validation_error(&function, "contains unreachable blocks");
    Ok(())
}

#[test]
fn rejects_phi_after_non_phi_instruction() -> Result<()> {
    let function = parser::parse(
        r#"
        func misplaced_phi args=0
        block b0:
          v0 = const 1
          jmp b1
        block b1:
          v1 = const 2
          v2 = phi b0 v0
          ret v2
        "#,
    )?;
    assert_validation_error(&function, "phi nodes must be contiguous at the start");
    Ok(())
}

#[test]
fn rejects_duplicate_phi_predecessors() -> Result<()> {
    let function = parser::parse(
        r#"
        func duplicate_phi_pred args=0
        block b0:
          v0 = const 1
          jmp b1
        block b1:
          v1 = phi b0 v0, b0 v0
          ret v1
        "#,
    )?;
    assert_validation_error(&function, "contains duplicate predecessor");
    Ok(())
}

#[test]
fn rejects_out_of_range_argument_index() -> Result<()> {
    let function = parser::parse(
        r#"
        func bad_arg args=1
        block b0:
          v0 = arg 1
          ret v0
        "#,
    )?;
    assert_validation_error(&function, "argument index 1 is out of range");
    Ok(())
}
