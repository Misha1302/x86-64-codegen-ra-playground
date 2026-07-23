use crate::ir::*;
use anyhow::{bail, Context, Result};
use smallvec::SmallVec;
use std::collections::HashMap;

fn parse_vreg(text: &str) -> Result<VReg> {
    let number = text
        .strip_prefix('v')
        .context("expected vN")?
        .parse::<u32>()?;
    Ok(VReg(number))
}

fn parse_block_id(text: &str) -> Result<BlockId> {
    let number = text
        .strip_prefix('b')
        .context("expected bN")?
        .parse::<u32>()?;
    Ok(BlockId(number))
}

fn ensure_exhausted<'a>(parts: &mut impl Iterator<Item = &'a str>, context: &str) -> Result<()> {
    if let Some(extra) = parts.next() {
        bail!("unexpected token '{extra}' after {context}");
    }
    Ok(())
}

pub fn parse(text: &str) -> Result<Function> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));

    let header = lines.next().context("missing func header")?;
    let mut header_parts = header.split_whitespace();
    anyhow::ensure!(header_parts.next() == Some("func"), "expected 'func'");
    let name = header_parts
        .next()
        .context("missing function name")?
        .to_string();
    let args_text = header_parts.next().context("missing args=N")?;
    let args = args_text
        .strip_prefix("args=")
        .context("expected args=N")?
        .parse::<u32>()?;
    ensure_exhausted(&mut header_parts, "function header")?;

    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    let mut current_terminated = false;
    let mut label_to_id = HashMap::new();
    let mut pending_terms: Vec<(usize, String)> = Vec::new();
    let mut pending_phis: Vec<(usize, usize, String)> = Vec::new();

    let finish_current =
        |current: &mut Option<Block>, terminated: bool, blocks: &mut Vec<Block>| -> Result<()> {
            if let Some(block) = current.take() {
                anyhow::ensure!(
                    terminated,
                    "block {:?} has no explicit terminator",
                    block.id
                );
                blocks.push(block);
            }
            Ok(())
        };

    for line in lines {
        if line.starts_with("block ") {
            finish_current(&mut current, current_terminated, &mut blocks)?;

            let label = line
                .strip_prefix("block ")
                .context("block prefix")?
                .trim()
                .strip_suffix(':')
                .context("block label must end with ':'")?
                .to_string();
            let id = parse_block_id(&label)?;
            anyhow::ensure!(
                label_to_id.insert(label, id).is_none(),
                "duplicate block label {:?}",
                id
            );
            current = Some(Block {
                id,
                insts: Vec::new(),
                term: Terminator::Jmp { target: id },
            });
            current_terminated = false;
            continue;
        }

        let block = current.as_mut().context("instruction outside block")?;

        if line.starts_with("ret ") || line.starts_with("jmp ") || line.starts_with("br ") {
            anyhow::ensure!(
                !current_terminated,
                "block {:?} has multiple terminators",
                block.id
            );
            pending_terms.push((blocks.len(), line.to_string()));
            current_terminated = true;
            continue;
        }

        anyhow::ensure!(
            !current_terminated,
            "instruction appears after terminator in block {:?}",
            block.id
        );

        let (lhs, rhs) = line.split_once('=').context("expected '='")?;
        let dst = parse_vreg(lhs.trim())?;
        let rhs = rhs.trim();
        let mut parts = rhs.split_whitespace();
        let op = parts.next().context("missing operation")?;

        match op {
            "const" => {
                let imm = parts.next().context("missing constant")?.parse::<i64>()?;
                ensure_exhausted(&mut parts, "const")?;
                block.insts.push(Inst::ConstI64 { dst, imm });
            }
            "arg" => {
                let idx = parts
                    .next()
                    .context("missing argument index")?
                    .parse::<u32>()?;
                ensure_exhausted(&mut parts, "arg")?;
                block.insts.push(Inst::ArgI64 {
                    dst,
                    idx: ArgId(idx),
                });
            }
            "add" | "mul" | "cmpgt" => {
                let left = parse_vreg(parts.next().context("missing left operand")?)?;
                let right = parse_vreg(parts.next().context("missing right operand")?)?;
                ensure_exhausted(&mut parts, op)?;
                let inst = match op {
                    "add" => Inst::AddI64 {
                        dst,
                        a: left,
                        b: right,
                    },
                    "mul" => Inst::MulI64 {
                        dst,
                        a: left,
                        b: right,
                    },
                    "cmpgt" => Inst::CmpGtI64 {
                        dst,
                        a: left,
                        b: right,
                    },
                    _ => unreachable!(),
                };
                block.insts.push(inst);
            }
            "mov" => {
                let src = parse_vreg(parts.next().context("missing source")?)?;
                ensure_exhausted(&mut parts, "mov")?;
                block.insts.push(Inst::MovI64 { dst, src });
            }
            "phi" => {
                let inst_index = block.insts.len();
                block.insts.push(Inst::ConstI64 { dst, imm: 0 });
                pending_phis.push((blocks.len(), inst_index, rhs.to_string()));
            }
            _ => bail!("unknown operation: {op}"),
        }
    }

    finish_current(&mut current, current_terminated, &mut blocks)?;
    anyhow::ensure!(!blocks.is_empty(), "function has no blocks");

    for (block_index, inst_index, phi_text) in pending_phis {
        let block = blocks
            .get_mut(block_index)
            .context("invalid phi block index")?;
        let mut tokens = phi_text.split_whitespace();
        anyhow::ensure!(tokens.next() == Some("phi"), "invalid phi placeholder");
        let rest = tokens.collect::<Vec<_>>().join(" ");
        let mut incoming: SmallVec<[(BlockId, VReg); 2]> = SmallVec::new();

        for chunk in rest
            .split(',')
            .map(str::trim)
            .filter(|chunk| !chunk.is_empty())
        {
            let mut parts = chunk.split_whitespace();
            let block_name = parts.next().context("missing phi predecessor")?;
            let value_name = parts.next().context("missing phi value")?;
            ensure_exhausted(&mut parts, "phi input")?;
            let predecessor = *label_to_id
                .get(block_name)
                .with_context(|| format!("unknown phi predecessor {block_name}"))?;
            incoming.push((predecessor, parse_vreg(value_name)?));
        }
        anyhow::ensure!(!incoming.is_empty(), "phi must have at least one input");

        let dst = match block.insts.get(inst_index) {
            Some(Inst::ConstI64 { dst, .. }) => *dst,
            _ => bail!("phi placeholder mismatch"),
        };
        block.insts[inst_index] = Inst::PhiI64 { dst, incoming };
    }

    for (block_index, term_text) in pending_terms {
        let block = blocks
            .get_mut(block_index)
            .context("invalid terminator block index")?;
        let mut parts = term_text.split_whitespace();
        let op = parts.next().context("missing terminator")?;
        block.term = match op {
            "ret" => {
                let value = parse_vreg(parts.next().context("missing return value")?)?;
                ensure_exhausted(&mut parts, "ret")?;
                Terminator::Ret { value }
            }
            "jmp" => {
                let target_name = parts.next().context("missing jump target")?;
                ensure_exhausted(&mut parts, "jmp")?;
                let target = *label_to_id
                    .get(target_name)
                    .with_context(|| format!("unknown jump target {target_name}"))?;
                Terminator::Jmp { target }
            }
            "br" => {
                let cond = parse_vreg(parts.next().context("missing branch condition")?)?;
                let then_name = parts.next().context("missing then target")?;
                let else_name = parts.next().context("missing else target")?;
                ensure_exhausted(&mut parts, "br")?;
                let then_bb = *label_to_id
                    .get(then_name)
                    .with_context(|| format!("unknown then target {then_name}"))?;
                let else_bb = *label_to_id
                    .get(else_name)
                    .with_context(|| format!("unknown else target {else_name}"))?;
                Terminator::Br {
                    cond,
                    then_bb,
                    else_bb,
                }
            }
            _ => bail!("unknown terminator: {op}"),
        };
    }

    Ok(Function {
        name,
        args,
        entry: blocks[0].id,
        blocks,
    })
}
