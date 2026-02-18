use crate::parser;
use crate::Function;
use anyhow::Result;

pub fn basicblock() -> Result<Function> {
    // Many temps to stress reg pressure.
    // f(a,b,c) = (a+b)*c + 7 + (a*c) + (b*c)
    let txt = r#"
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
    parser::parse(txt)
}

pub fn trace() -> Result<Function> {
    // max(a,b)
    let txt = r#"
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
    parser::parse(txt)
}
