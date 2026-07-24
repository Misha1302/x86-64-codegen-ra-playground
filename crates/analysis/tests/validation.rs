use analysis::{validate_function, ValidationError};
use anyhow::Result;
use ir::{parser, Block, BlockId, Function, Inst, Terminator, VReg};
use smallvec::smallvec;

#[test]
fn built_in_examples_satisfy_the_ir_contract() -> Result<()> {
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
fn a_value_is_defined_once() -> Result<()> {
    let function = parser::parse(
        r#"
        func duplicate args=0
        block b0:
          v0 = const 1
          v0 = const 2
          ret v0
        "#,
    )?;

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::MultipleDefinitions(VReg(0)))
    );
    Ok(())
}

#[test]
fn phi_has_one_input_for_each_cfg_predecessor() {
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

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::IncompletePhi(join))
    );
}

#[test]
fn a_branch_local_definition_cannot_escape_without_a_phi() -> Result<()> {
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

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::UseNotDominated {
            value: VReg(1),
            definition: BlockId(1),
            use_block: BlockId(3),
        })
    );
    Ok(())
}

#[test]
fn an_instruction_cannot_read_a_later_definition() -> Result<()> {
    let function = parser::parse(
        r#"
        func use_before_def args=0
        block b0:
          v1 = mov v0
          v0 = const 7
          ret v1
        "#,
    )?;

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::UseBeforeDefinition {
            value: VReg(0),
            block: BlockId(0),
        })
    );
    Ok(())
}

#[test]
fn dead_blocks_are_rejected_before_dataflow_analysis() -> Result<()> {
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

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::UnreachableBlocks)
    );
    Ok(())
}

#[test]
fn phi_nodes_stay_at_the_start_of_a_block() -> Result<()> {
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

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::PhiAfterInstruction(BlockId(1)))
    );
    Ok(())
}

#[test]
fn a_phi_cannot_name_the_same_edge_twice() -> Result<()> {
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

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::DuplicatePhiPredecessor {
            block: BlockId(1),
            predecessor: BlockId(0),
        })
    );
    Ok(())
}

#[test]
fn argument_indices_follow_the_declared_signature() -> Result<()> {
    let function = parser::parse(
        r#"
        func bad_arg args=1
        block b0:
          v0 = arg 1
          ret v0
        "#,
    )?;

    assert_eq!(
        validate_function(&function),
        Err(ValidationError::ArgumentOutOfRange { index: 1, args: 1 })
    );
    Ok(())
}
