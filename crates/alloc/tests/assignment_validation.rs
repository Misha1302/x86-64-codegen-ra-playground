use alloc::{
    verify_assignment, Assignment, AssignmentError, Location, PhysReg, PhysRegSet, StackSlot,
};
use analysis::{LiveInterval, LiveIntervals, Pos};
use indexmap::IndexMap;
use ir::VReg;

fn intervals(first: (u32, u32), second: (u32, u32)) -> LiveIntervals {
    LiveIntervals {
        intervals: vec![
            LiveInterval {
                v: VReg(0),
                start: Pos(first.0),
                end: Pos(first.1),
            },
            LiveInterval {
                v: VReg(1),
                start: Pos(second.0),
                end: Pos(second.1),
            },
        ],
        positions: IndexMap::new(),
    }
}

fn assignment(first: Location, second: Location, stack_slots: u32, spills: u32) -> Assignment {
    Assignment {
        map: IndexMap::from([(VReg(0), first), (VReg(1), second)]),
        stack_slots,
        spills,
    }
}

#[test]
fn overlapping_values_cannot_share_a_register() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        1,
        &assignment(
            Location::Reg(PhysReg::Rax),
            Location::Reg(PhysReg::Rax),
            0,
            0,
        ),
    );

    assert_eq!(
        result,
        Err(AssignmentError::RegisterConflict {
            left: VReg(0),
            right: VReg(1),
            register: PhysReg::Rax,
        })
    );
}

#[test]
fn overlapping_spills_cannot_alias_the_same_slot() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        0,
        &assignment(
            Location::Stack(StackSlot { index: 0 }),
            Location::Stack(StackSlot { index: 0 }),
            1,
            2,
        ),
    );

    assert_eq!(
        result,
        Err(AssignmentError::StackSlotConflict {
            left: VReg(0),
            right: VReg(1),
            slot: 0,
        })
    );
}

#[test]
fn a_dead_spill_slot_can_be_reused() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 1), (2, 5)),
        &regs,
        0,
        &assignment(
            Location::Stack(StackSlot { index: 0 }),
            Location::Stack(StackSlot { index: 0 }),
            1,
            2,
        ),
    );

    assert_eq!(result, Ok(()));
}

#[test]
fn scratch_registers_are_not_part_of_the_allocator_budget() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        5,
        &assignment(
            Location::Reg(PhysReg::R10),
            Location::Stack(StackSlot { index: 0 }),
            1,
            1,
        ),
    );

    assert_eq!(
        result,
        Err(AssignmentError::ReservedScratch {
            value: VReg(0),
            register: PhysReg::R10,
        })
    );
}

#[test]
fn spill_metadata_matches_the_assignment_map() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        1,
        &assignment(
            Location::Reg(PhysReg::Rax),
            Location::Stack(StackSlot { index: 0 }),
            1,
            0,
        ),
    );

    assert_eq!(
        result,
        Err(AssignmentError::SpillCountMismatch {
            declared: 0,
            actual: 1,
        })
    );
}

#[test]
fn stack_slot_metadata_covers_the_highest_slot() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        1,
        &assignment(
            Location::Reg(PhysReg::Rax),
            Location::Stack(StackSlot { index: 2 }),
            2,
            1,
        ),
    );

    assert_eq!(
        result,
        Err(AssignmentError::StackSlotCountMismatch {
            declared: 2,
            expected: 3,
        })
    );
}

#[test]
fn register_and_stack_locations_may_overlap() {
    let regs = PhysRegSet::default_gp_with_scratch();
    let result = verify_assignment(
        &intervals((0, 3), (2, 5)),
        &regs,
        1,
        &assignment(
            Location::Reg(PhysReg::Rax),
            Location::Stack(StackSlot { index: 0 }),
            1,
            1,
        ),
    );

    assert_eq!(result, Ok(()));
}
