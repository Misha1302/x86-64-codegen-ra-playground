use anyhow::Result;
use alloc::{Assignment, Location, StackSlot};
use indexmap::IndexMap;
use ir::VReg;

#[test]
fn assignment_serde_roundtrip() -> Result<()> {
    let mut map = IndexMap::new();
    map.insert(VReg(0), Location::Stack(StackSlot { index: 0 }));
    map.insert(VReg(1), Location::Stack(StackSlot { index: 1 }));
    let asg = Assignment { map, spills: 2, stack_slots: 2 };
    let json = serde_json::to_string(&asg)?;
    let asg2: Assignment = serde_json::from_str(&json)?;
    assert_eq!(asg.spills, asg2.spills);
    assert_eq!(asg.stack_slots, asg2.stack_slots);
    assert_eq!(asg.map.len(), asg2.map.len());
    Ok(())
}
