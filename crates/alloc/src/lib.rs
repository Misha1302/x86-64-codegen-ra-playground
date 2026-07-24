use std::collections::HashSet;

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use analysis::LiveIntervals;
use ir::VReg;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PhysReg {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
}

impl PhysReg {
    pub fn name(&self) -> &'static str {
        match self {
            PhysReg::Rax => "rax",
            PhysReg::Rcx => "rcx",
            PhysReg::Rdx => "rdx",
            PhysReg::Rbx => "rbx",
            PhysReg::Rsi => "rsi",
            PhysReg::Rdi => "rdi",
            PhysReg::R8 => "r8",
            PhysReg::R9 => "r9",
            PhysReg::R10 => "r10",
            PhysReg::R11 => "r11",
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct StackSlot {
    pub index: u32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Location {
    Reg(PhysReg),
    Stack(StackSlot),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub map: IndexMap<VReg, Location>,
    pub stack_slots: u32,
    pub spills: u32,
}

#[derive(Debug, Clone)]
pub struct PhysRegSet {
    pub regs: Vec<PhysReg>,
    pub scratch: Vec<PhysReg>,
}

impl PhysRegSet {
    pub fn default_gp_with_scratch() -> Self {
        Self {
            regs: vec![
                PhysReg::Rax,
                PhysReg::Rcx,
                PhysReg::Rdx,
                PhysReg::R8,
                PhysReg::R9,
            ],
            scratch: vec![PhysReg::R10, PhysReg::R11],
        }
    }

    pub fn validate_for_codegen(&self) -> Result<()> {
        let mut all = HashSet::new();
        for register in &self.regs {
            anyhow::ensure!(
                all.insert(*register),
                "duplicate allocatable register {register:?}"
            );
            anyhow::ensure!(
                *register != PhysReg::Rbx,
                "RBX is callee-saved and is not supported by the MVP prologue"
            );
        }
        anyhow::ensure!(
            self.scratch.len() >= 2,
            "codegen requires two dedicated scratch registers"
        );
        for register in &self.scratch {
            anyhow::ensure!(
                all.insert(*register),
                "scratch register overlaps allocatable set"
            );
            anyhow::ensure!(
                *register != PhysReg::Rbx,
                "RBX is callee-saved and cannot be used as scratch"
            );
        }
        anyhow::ensure!(
            self.scratch[0] != self.scratch[1],
            "scratch registers must be distinct"
        );
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssignmentError {
    #[error("assignment contains {actual} entries for {expected} intervals")]
    WrongEntryCount { actual: usize, expected: usize },
    #[error("assignment is missing {0:?}")]
    MissingValue(VReg),
    #[error("assignment contains unknown value {0:?}")]
    UnknownValue(VReg),
    #[error("value {value:?} uses unavailable register {register:?}")]
    UnavailableRegister { value: VReg, register: PhysReg },
    #[error("value {value:?} uses reserved scratch register {register:?}")]
    ReservedScratch { value: VReg, register: PhysReg },
    #[error("spill count overflow")]
    SpillCountOverflow,
    #[error("spill count {declared} does not match {actual} stack assignments")]
    SpillCountMismatch { declared: u32, actual: u32 },
    #[error("stack slot count {declared} does not match highest used slot {expected}")]
    StackSlotCountMismatch { declared: u32, expected: u32 },
    #[error("overlapping values {left:?} and {right:?} share register {register:?}")]
    RegisterConflict {
        left: VReg,
        right: VReg,
        register: PhysReg,
    },
    #[error("overlapping values {left:?} and {right:?} share stack slot {slot}")]
    StackSlotConflict { left: VReg, right: VReg, slot: u32 },
}

pub fn verify_assignment(
    intervals: &LiveIntervals,
    regs: &PhysRegSet,
    max_regs: usize,
    assignment: &Assignment,
) -> std::result::Result<(), AssignmentError> {
    let allowed: HashSet<PhysReg> = regs
        .regs
        .iter()
        .copied()
        .take(max_regs.min(regs.regs.len()))
        .collect();
    let expected: HashSet<VReg> = intervals
        .intervals
        .iter()
        .map(|interval| interval.v)
        .collect();

    if assignment.map.len() != expected.len() {
        return Err(AssignmentError::WrongEntryCount {
            actual: assignment.map.len(),
            expected: expected.len(),
        });
    }
    for value in &expected {
        if !assignment.map.contains_key(value) {
            return Err(AssignmentError::MissingValue(*value));
        }
    }
    for value in assignment.map.keys() {
        if !expected.contains(value) {
            return Err(AssignmentError::UnknownValue(*value));
        }
    }

    let mut spill_count = 0_u32;
    let mut highest_slot = None;
    for (value, location) in &assignment.map {
        match location {
            Location::Reg(register) => {
                if regs.scratch.contains(register) {
                    return Err(AssignmentError::ReservedScratch {
                        value: *value,
                        register: *register,
                    });
                }
                if !allowed.contains(register) {
                    return Err(AssignmentError::UnavailableRegister {
                        value: *value,
                        register: *register,
                    });
                }
            }
            Location::Stack(slot) => {
                spill_count = spill_count
                    .checked_add(1)
                    .ok_or(AssignmentError::SpillCountOverflow)?;
                highest_slot =
                    Some(highest_slot.map_or(slot.index, |current: u32| current.max(slot.index)));
            }
        }
    }

    if assignment.spills != spill_count {
        return Err(AssignmentError::SpillCountMismatch {
            declared: assignment.spills,
            actual: spill_count,
        });
    }
    let expected_slots = highest_slot.map_or(0, |slot| slot.saturating_add(1));
    if assignment.stack_slots != expected_slots {
        return Err(AssignmentError::StackSlotCountMismatch {
            declared: assignment.stack_slots,
            expected: expected_slots,
        });
    }

    for (left_index, left) in intervals.intervals.iter().enumerate() {
        let left_location = assignment
            .map
            .get(&left.v)
            .ok_or(AssignmentError::MissingValue(left.v))?;

        for right in intervals.intervals.iter().skip(left_index + 1) {
            if !left.overlaps(right) {
                continue;
            }
            let right_location = assignment
                .map
                .get(&right.v)
                .ok_or(AssignmentError::MissingValue(right.v))?;
            match (left_location, right_location) {
                (Location::Reg(left_register), Location::Reg(right_register))
                    if left_register == right_register =>
                {
                    return Err(AssignmentError::RegisterConflict {
                        left: left.v,
                        right: right.v,
                        register: *left_register,
                    });
                }
                (Location::Stack(left_slot), Location::Stack(right_slot))
                    if left_slot == right_slot =>
                {
                    return Err(AssignmentError::StackSlotConflict {
                        left: left.v,
                        right: right.v,
                        slot: left_slot.index,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(())
}

pub trait Allocator: Send + Sync {
    fn name(&self) -> &'static str;
    fn allocate(
        &self,
        intervals: &LiveIntervals,
        regs: &PhysRegSet,
        max_regs: usize,
    ) -> Result<Assignment>;
}
