use crate::parser;
use crate::Function;
use anyhow::Result;

pub fn basicblock() -> Result<Function> {
    let text = r#"
    func basicblock args=3
    block b0:
      v0 = arg 0
      v1 = arg 1
      v2 = arg 2
      v3 = add v0 v1
      v4 = mul v3 v2
      v5 = const 7
      v6 = add v4 v5
      v7 = mul v0 v2
      v8 = add v6 v7
      v9 = mul v1 v2
      v10 = add v8 v9
      ret v10
    "#;
    parser::parse(text)
}

pub fn trace() -> Result<Function> {
    let text = r#"
    func trace args=2
    block b0:
      v0 = arg 0
      v1 = arg 1
      v2 = cmpgt v0 v1
      br v2 b1 b2
    block b1:
      v3 = mov v0
      jmp b3
    block b2:
      v4 = mov v1
      jmp b3
    block b3:
      v5 = phi b1 v3, b2 v4
      ret v5
    "#;
    parser::parse(text)
}

pub fn loop_sum() -> Result<Function> {
    let text = r#"
    func loop_sum args=1
    block b0:
      v0 = arg 0
      v1 = const 0
      v2 = const 1
      jmp b1
    block b1:
      v3 = phi b0 v2, b2 v7
      v4 = phi b0 v1, b2 v6
      v5 = cmpgt v3 v0
      br v5 b3 b2
    block b2:
      v6 = add v4 v3
      v7 = add v3 v2
      jmp b1
    block b3:
      ret v4
    "#;
    parser::parse(text)
}

pub fn phi_swap_loop() -> Result<Function> {
    let text = r#"
    func phi_swap_loop args=3
    block b0:
      v0 = arg 0
      v1 = arg 1
      v2 = arg 2
      v3 = const 0
      v4 = const -1
      jmp b1
    block b1:
      v5 = phi b0 v0, b2 v6
      v6 = phi b0 v1, b2 v5
      v7 = phi b0 v2, b2 v9
      v8 = cmpgt v7 v3
      br v8 b2 b3
    block b2:
      v9 = add v7 v4
      jmp b1
    block b3:
      ret v5
    "#;
    parser::parse(text)
}
