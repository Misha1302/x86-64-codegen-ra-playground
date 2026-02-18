use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use analysis::LiveIntervals;
use ir::VReg;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PhysReg {
    Rax, Rcx, Rdx, Rbx,
    Rsi, Rdi,
    R8, R9, R10, R11,
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
    pub index: u32, // 0..N
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Location {
    Reg(PhysReg),
    Stack(StackSlot),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub map: IndexMap<VReg, Location>,
    pub stack_slots: u32,
    pub spills: u32,
}

#[derive(Debug, Clone)]
pub struct PhysRegSet {
    pub regs: Vec<PhysReg>,
    pub scratch: Vec<PhysReg>, // reserved for codegen
}

impl PhysRegSet {
    pub fn default_gp_with_scratch() -> Self {
        // Use caller-saved regs; reserve r10/r11 as scratch by default.
        Self {
            regs: vec![PhysReg::Rax, PhysReg::Rcx, PhysReg::Rdx, PhysReg::R8, PhysReg::R9],
            scratch: vec![PhysReg::R10, PhysReg::R11],
        }
    }
}

pub trait Allocator: Send + Sync {
    fn name(&self) -> &'static str;
    fn allocate(&self, intervals: &LiveIntervals, regs: &PhysRegSet, max_regs: usize) -> Result<Assignment>;
}
