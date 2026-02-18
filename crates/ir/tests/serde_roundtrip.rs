use anyhow::Result;
use ir::examples;

#[test]
fn ir_serde_roundtrip_basicblock() -> Result<()> {
    let f = examples::basicblock()?;
    let json = serde_json::to_string(&f)?;
    let f2: ir::Function = serde_json::from_str(&json)?;
    assert_eq!(f.name, f2.name);
    assert_eq!(f.args, f2.args);
    assert_eq!(f.blocks.len(), f2.blocks.len());
    Ok(())
}
