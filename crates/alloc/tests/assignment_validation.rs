use alloc::{verify_assignment, Assignment, Location, PhysReg, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals, Pos};
use indexmap::IndexMap;
use ir::VReg;

fn overlapping_intervals() -> LiveIntervals {
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

fn non_overlapping_intervals() -> LiveIntervals {
    LiveIntervals {
        intervals: vec![
            LiveInterval {
                v: VReg(0),
                start: Pos(0),
                end: Pos(1),
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

fn assert_error_contains(result: anyhow::Result<()>, expected: &str) {
    let error = result.expect_err("assignment must be rejected");
    let message = format!("{error:#}");
    assert!(
        message.contains(expected),
        "expected error containing {expected:?}, got {message:?}"
    );
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
    assert_error_contains(
        verify_assignment(&overlapping_intervals(), &regs, 1, &assignment),
        "share register",
    );
}

#[test]
fn rejects_overlapping_values_in_same_stack_slot() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Stack(StackSlot { index: 0 })),
            (VReg(1), Location::Stack(StackSlot { index: 0 })),
        ]),
        stack_slots: 1,
        spills: 2,
    };
    assert_error_contains(
        verify_assignment(&overlapping_intervals(), &regs, 0, &assignment),
        "share stack slot 0",
    );
}

#[test]
fn permits_stack_slot_reuse_for_disjoint_intervals() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Stack(StackSlot { index: 0 })),
            (VReg(1), Location::Stack(StackSlot { index: 0 })),
        ]),
        stack_slots: 1,
        spills: 2,
    };
    verify_assignment(&non_overlapping_intervals(), &regs, 0, &assignment).unwrap();
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
    assert_error_contains(
        verify_assignment(&overlapping_intervals(), &regs, 5, &assignment),
        "unavailable register R10",
    );
}

#[test]
fn rejects_incorrect_spill_metadata() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Reg(PhysReg::Rax)),
            (VReg(1), Location::Stack(StackSlot { index: 0 })),
        ]),
        stack_slots: 1,
        spills: 0,
    };
    assert_error_contains(
        verify_assignment(&overlapping_intervals(), &regs, 1, &assignment),
        "spill count does not match",
    );
}

#[test]
fn rejects_incorrect_stack_slot_metadata() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let assignment = Assignment {
        map: IndexMap::from([
            (VReg(0), Location::Reg(PhysReg::Rax)),
            (VReg(1), Location::Stack(StackSlot { index: 2 })),
        ]),
        stack_slots: 2,
        spills: 1,
    };
    assert_error_contains(
        verify_assignment(&overlapping_intervals(), &regs, 1, &assignment),
        "does not match highest used slot 3",
    );
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
    verify_assignment(&overlapping_intervals(), &regs, 1, &assignment).unwrap();
}
