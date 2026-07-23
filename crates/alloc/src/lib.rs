use std::collections::HashSet;

use anyhow::{Context, Result};
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

pub fn verify_assignment(
    intervals: &LiveIntervals,
    regs: &PhysRegSet,
    max_regs: usize,
    assignment: &Assignment,
) -> Result<()> {
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

    anyhow::ensure!(
        assignment.map.len() == expected.len(),
        "assignment contains {} entries for {} intervals",
        assignment.map.len(),
        expected.len()
    );
    for value in &expected {
        anyhow::ensure!(
            assignment.map.contains_key(value),
            "assignment is missing {:?}",
            value
        );
    }
    for value in assignment.map.keys() {
        anyhow::ensure!(
            expected.contains(value),
            "assignment contains unknown value {:?}",
            value
        );
    }

    let mut stack_count = 0_u32;
    let mut highest_slot = None;
    for (value, location) in &assignment.map {
        match location {
            Location::Reg(register) => {
                anyhow::ensure!(
                    allowed.contains(register),
                    "value {:?} uses unavailable register {:?}",
                    value,
                    register
                );
                anyhow::ensure!(
                    !regs.scratch.contains(register),
                    "value {:?} uses reserved scratch register {:?}",
                    value,
                    register
                );
            }
            Location::Stack(slot) => {
                stack_count = stack_count.checked_add(1).context("spill count overflow")?;
                highest_slot =
                    Some(highest_slot.map_or(slot.index, |current: u32| current.max(slot.index)));
            }
        }
    }

    anyhow::ensure!(
        assignment.spills == stack_count,
        "spill count does not match stack assignments"
    );
    let expected_slots = highest_slot.map_or(0, |slot| slot.saturating_add(1));
    anyhow::ensure!(
        assignment.stack_slots == expected_slots,
        "stack slot count {} does not match highest used slot {}",
        assignment.stack_slots,
        expected_slots
    );

    for (left_index, left) in intervals.intervals.iter().enumerate() {
        let left_location = assignment
            .map
            .get(&left.v)
            .with_context(|| format!("missing assignment for {:?}", left.v))?;
        let Location::Reg(left_register) = left_location else {
            continue;
        };

        for right in intervals.intervals.iter().skip(left_index + 1) {
            if !left.overlaps(right) {
                continue;
            }
            if assignment.map.get(&right.v) == Some(&Location::Reg(*left_register)) {
                anyhow::bail!(
                    "overlapping values {:?} and {:?} share register {:?}",
                    left.v,
                    right.v,
                    left_register
                );
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
