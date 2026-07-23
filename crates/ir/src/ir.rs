use smallvec::{smallvec, SmallVec};

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct VReg(pub u32);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u32);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct InstId(pub u32);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ArgId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub args: u32,
    pub entry: BlockId,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    pub insts: Vec<Inst>,
    pub term: Terminator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Inst {
    ConstI64 {
        dst: VReg,
        imm: i64,
    },
    AddI64 {
        dst: VReg,
        a: VReg,
        b: VReg,
    },
    MulI64 {
        dst: VReg,
        a: VReg,
        b: VReg,
    },
    MovI64 {
        dst: VReg,
        src: VReg,
    },
    CmpGtI64 {
        dst: VReg,
        a: VReg,
        b: VReg,
    },
    PhiI64 {
        dst: VReg,
        incoming: SmallVec<[(BlockId, VReg); 2]>,
    },
    ArgI64 {
        dst: VReg,
        idx: ArgId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Terminator {
    Ret {
        value: VReg,
    },
    Jmp {
        target: BlockId,
    },
    Br {
        cond: VReg,
        then_bb: BlockId,
        else_bb: BlockId,
    },
}

impl Function {
    pub fn try_block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|block| block.id == id)
    }

    pub fn try_block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|block| block.id == id)
    }

    pub fn block(&self, id: BlockId) -> &Block {
        self.try_block(id).expect("block id")
    }

    pub fn block_mut(&mut self, id: BlockId) -> &mut Block {
        self.try_block_mut(id).expect("block id")
    }

    pub fn blocks_in_order(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter()
    }
}

pub fn successors(term: &Terminator) -> SmallVec<[BlockId; 2]> {
    match *term {
        Terminator::Ret { .. } => SmallVec::new(),
        Terminator::Jmp { target } => smallvec![target],
        Terminator::Br {
            then_bb, else_bb, ..
        } => smallvec![then_bb, else_bb],
    }
}

pub fn defs(inst: &Inst) -> Option<VReg> {
    match *inst {
        Inst::ConstI64 { dst, .. }
        | Inst::AddI64 { dst, .. }
        | Inst::MulI64 { dst, .. }
        | Inst::MovI64 { dst, .. }
        | Inst::CmpGtI64 { dst, .. }
        | Inst::PhiI64 { dst, .. }
        | Inst::ArgI64 { dst, .. } => Some(dst),
    }
}

pub fn uses(inst: &Inst) -> SmallVec<[VReg; 4]> {
    match *inst {
        Inst::ConstI64 { .. } | Inst::ArgI64 { .. } => SmallVec::new(),
        Inst::AddI64 { a, b, .. } | Inst::MulI64 { a, b, .. } | Inst::CmpGtI64 { a, b, .. } => {
            smallvec![a, b]
        }
        Inst::MovI64 { src, .. } => smallvec![src],
        Inst::PhiI64 { ref incoming, .. } => incoming.iter().map(|(_, value)| *value).collect(),
    }
}

pub fn term_uses(term: &Terminator) -> SmallVec<[VReg; 2]> {
    match *term {
        Terminator::Ret { value } => smallvec![value],
        Terminator::Jmp { .. } => SmallVec::new(),
        Terminator::Br { cond, .. } => smallvec![cond],
    }
}
