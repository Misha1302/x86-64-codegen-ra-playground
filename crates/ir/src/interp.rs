use crate::ir::*;
use anyhow::{bail, Result};
use indexmap::IndexMap;

#[derive(Default)]
pub struct Interpreter;

impl Interpreter {
    pub fn eval_i64(&self, f: &Function, args: &[i64]) -> Result<i64> {
        if args.len() != f.args as usize {
            bail!("expected {} args, got {}", f.args, args.len());
        }
        let mut cur = f.entry;
        // Values per block execution, keyed by VReg
        let mut regs: IndexMap<VReg, i64> = IndexMap::new();
        let mut pred: Option<BlockId> = None;

        loop {
            let b = f.block(cur);

            // Handle phis first
            for inst in &b.insts {
                if let Inst::PhiI64 { dst, incoming } = inst {
                    let p = pred.ok_or_else(|| anyhow::anyhow!("phi in entry block"))?;
                    let mut chosen = None;
                    for (bb, v) in incoming.iter() {
                        if *bb == p { chosen = Some(*v); break; }
                    }
                    let v = chosen.ok_or_else(|| anyhow::anyhow!("phi missing incoming for pred"))?;
                    let val = *regs.get(&v).ok_or_else(|| anyhow::anyhow!("phi uses undef {:?}", v))?;
                    regs.insert(*dst, val);
                }
            }

            for inst in &b.insts {
                match *inst {
                    Inst::ConstI64 { dst, imm } => { regs.insert(dst, imm); }
                    Inst::ArgI64 { dst, idx } => { regs.insert(dst, args[idx.0 as usize]); }
                    Inst::AddI64 { dst, a, b } => {
                        let va = *regs.get(&a).ok_or_else(|| anyhow::anyhow!("undef {:?}", a))?;
                        let vb = *regs.get(&b).ok_or_else(|| anyhow::anyhow!("undef {:?}", b))?;
                        regs.insert(dst, va.wrapping_add(vb));
                    }
                    Inst::MulI64 { dst, a, b } => {
                        let va = *regs.get(&a).ok_or_else(|| anyhow::anyhow!("undef {:?}", a))?;
                        let vb = *regs.get(&b).ok_or_else(|| anyhow::anyhow!("undef {:?}", b))?;
                        regs.insert(dst, va.wrapping_mul(vb));
                    }
                    Inst::MovI64 { dst, src } => {
                        let v = *regs.get(&src).ok_or_else(|| anyhow::anyhow!("undef {:?}", src))?;
                        regs.insert(dst, v);
                    }
                    Inst::CmpGtI64 { dst, a, b } => {
                        let va = *regs.get(&a).ok_or_else(|| anyhow::anyhow!("undef {:?}", a))?;
                        let vb = *regs.get(&b).ok_or_else(|| anyhow::anyhow!("undef {:?}", b))?;
                        regs.insert(dst, if va > vb { 1 } else { 0 });
                    }
                    Inst::PhiI64 { .. } => {} // already handled
                }
            }

            match b.term {
                Terminator::Ret { value } => {
                    return Ok(*regs.get(&value).ok_or_else(|| anyhow::anyhow!("undef {:?}", value))?)
                }
                Terminator::Jmp { target } => {
                    pred = Some(cur);
                    cur = target;
                }
                Terminator::Br { cond, then_bb, else_bb } => {
                    let c = *regs.get(&cond).ok_or_else(|| anyhow::anyhow!("undef {:?}", cond))?;
                    pred = Some(cur);
                    cur = if c != 0 { then_bb } else { else_bb };
                }
            }
        }
    }
}
