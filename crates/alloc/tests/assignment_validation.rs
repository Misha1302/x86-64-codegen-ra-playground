use alloc::{verify_assignment, Assignment, Location, PhysReg, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals, Pos};
use indexmap::IndexMap;
use ir::VReg;

fn intervals() -> LiveIntervals {
    LiveIntervals {
        intervals: vec![
            LiveInterval {
                v: VReg(0),
                start: Pos(0),
                end: Pos(3),
            },
            LiveInterval {
                v: VReg(1),
                start: Pos(2),
                end: Pos(5),
            },
        ],
        positions: IndexMap::new(),
    }
}

#[test]
fn rejects_register_conflict() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Reg(PhysReg::Rax)),
            (VReg(1), Location::Reg(PhysReg::Rax)),
        ]),
        stack_slots: 0,
        spills: 0,
    };
    assert!(verify_assignment(&intervals(), &regs, 1, &assignment).is_err());
}

#[test]
fn rejects_reserved_scratch_register() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Reg(PhysReg::R10)),
            (VReg(1), Location::Stack(StackSlot { index: 0 })),
        ]),
        stack_slots: 1,
        spills: 1,
    };
    assert!(verify_assignment(&intervals(), &regs, 5, &assignment).is_err());
}

#[test]
fn accepts_complete_conflict_free_assignment() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Reg(PhysReg::Rax)),
            (VReg(1), Location::Stack(StackSlot { index: 0 })),
        ]),
        stack_slots: 1,
        spills: 1,
    };
    verify_assignment(&intervals(), &regs, 1, &assignment).unwrap();
}
