use anyhow::Result;
use ir::{examples, interp::Interpreter, parser};

#[test]
fn interpreter_matches_built_in_examples() -> Result<()> {
    let interpreter = Interpreter;
    assert_eq!(
        interpreter.eval_i64(&examples::basicblock()?, &[2, 3, 4])?,
        47
    );
    assert_eq!(interpreter.eval_i64(&examples::trace()?, &[10, 7])?, 10);
    assert_eq!(interpreter.eval_i64(&examples::trace()?, &[3, 9])?, 9);
    assert_eq!(interpreter.eval_i64(&examples::loop_sum()?, &[0])?, 0);
    assert_eq!(interpreter.eval_i64(&examples::loop_sum()?, &[5])?, 15);
    for iterations in 0..=8 {
        let expected = if iterations % 2 == 0 { 11 } else { 29 };
        assert_eq!(
            interpreter.eval_i64(&examples::phi_swap_loop()?, &[11, 29, iterations])?,
            expected
        );
    }
    Ok(())
}

#[test]
fn parser_rejects_implicit_or_multiple_terminators() {
    assert!(parser::parse("func f args=0\nblock b0:\nv0 = const 1").is_err());
    assert!(parser::parse("func f args=0\nblock b0:\nv0 = const 1\nret v0\nret v0").is_err());
}

#[test]
fn interpreter_enforces_step_limit() -> Result<()> {
    let function = parser::parse(
        r#"
        func forever args=0
        block b0:
          jmp b0
        "#,
    )?;
    assert!(Interpreter.eval_i64_with_limit(&function, &[], 10).is_err());
    Ok(())
}
