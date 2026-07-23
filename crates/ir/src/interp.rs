use crate::ir::*;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;

const DEFAULT_STEP_LIMIT: usize = 1_000_000;

#[derive(Default)]
pub struct Interpreter;

impl Interpreter {
    pub fn eval_i64(&self, f: &Function, args: &[i64]) -> Result<i64> {
        self.eval_i64_with_limit(f, args, DEFAULT_STEP_LIMIT)
    }

    pub fn eval_i64_with_limit(
        &self,
        f: &Function,
        args: &[i64],
        step_limit: usize,
    ) -> Result<i64> {
        if args.len() != f.args as usize {
            bail!("expected {} args, got {}", f.args, args.len());
        }
        anyhow::ensure!(step_limit > 0, "step limit must be positive");

        let mut current = f.entry;
        let mut regs: IndexMap<VReg, i64> = IndexMap::new();
        let mut predecessor: Option<BlockId> = None;
        let mut steps = 0_usize;

        loop {
            steps = steps
                .checked_add(1)
                .context("interpreter step counter overflow")?;
            anyhow::ensure!(steps <= step_limit, "interpreter step limit exceeded");

            let block = f
                .try_block(current)
                .with_context(|| format!("unknown block {:?}", current))?;

            let mut phi_results = Vec::new();
            for inst in &block.insts {
                let Inst::PhiI64 { dst, incoming } = inst else {
                    continue;
                };
                let pred = predecessor.context("phi in entry block")?;
                let source = incoming
                    .iter()
                    .find_map(|(block_id, value)| (*block_id == pred).then_some(*value))
                    .context("phi missing incoming for predecessor")?;
                let value = *regs
                    .get(&source)
                    .with_context(|| format!("phi uses undefined value {:?}", source))?;
                phi_results.push((*dst, value));
            }
            for (dst, value) in phi_results {
                regs.insert(dst, value);
            }

            for inst in &block.insts {
                steps = steps
                    .checked_add(1)
                    .context("interpreter step counter overflow")?;
                anyhow::ensure!(steps <= step_limit, "interpreter step limit exceeded");

                match *inst {
                    Inst::ConstI64 { dst, imm } => {
                        regs.insert(dst, imm);
                    }
                    Inst::ArgI64 { dst, idx } => {
                        let value = *args
                            .get(idx.0 as usize)
                            .with_context(|| format!("argument index {} is out of range", idx.0))?;
                        regs.insert(dst, value);
                    }
                    Inst::AddI64 { dst, a, b } => {
                        let left = *regs
                            .get(&a)
                            .with_context(|| format!("undefined value {:?}", a))?;
                        let right = *regs
                            .get(&b)
                            .with_context(|| format!("undefined value {:?}", b))?;
                        regs.insert(dst, left.wrapping_add(right));
                    }
                    Inst::MulI64 { dst, a, b } => {
                        let left = *regs
                            .get(&a)
                            .with_context(|| format!("undefined value {:?}", a))?;
                        let right = *regs
                            .get(&b)
                            .with_context(|| format!("undefined value {:?}", b))?;
                        regs.insert(dst, left.wrapping_mul(right));
                    }
                    Inst::MovI64 { dst, src } => {
                        let value = *regs
                            .get(&src)
                            .with_context(|| format!("undefined value {:?}", src))?;
                        regs.insert(dst, value);
                    }
                    Inst::CmpGtI64 { dst, a, b } => {
                        let left = *regs
                            .get(&a)
                            .with_context(|| format!("undefined value {:?}", a))?;
                        let right = *regs
                            .get(&b)
                            .with_context(|| format!("undefined value {:?}", b))?;
                        regs.insert(dst, if left > right { 1 } else { 0 });
                    }
                    Inst::PhiI64 { .. } => {}
                }
            }

            match block.term {
                Terminator::Ret { value } => {
                    return regs
                        .get(&value)
                        .copied()
                        .with_context(|| format!("undefined return value {:?}", value));
                }
                Terminator::Jmp { target } => {
                    predecessor = Some(current);
                    current = target;
                }
                Terminator::Br {
                    cond,
                    then_bb,
                    else_bb,
                } => {
                    let condition = *regs
                        .get(&cond)
                        .with_context(|| format!("undefined branch condition {:?}", cond))?;
                    predecessor = Some(current);
                    current = if condition != 0 { then_bb } else { else_bb };
                }
            }
        }
    }
}
