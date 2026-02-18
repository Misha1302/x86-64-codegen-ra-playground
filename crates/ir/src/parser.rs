use crate::ir::*;
use anyhow::{bail, Context, Result};
use smallvec::SmallVec;
use std::collections::HashMap;

/// Tiny text format (MVP). Example:
///
/// func basic args=3
/// block b0:
///   v0 = arg 0
///   v1 = arg 1
///   v2 = add v0 v1
///   ret v2
///
/// For multi-block:
/// block b0:
///   v0 = arg 0
///   v1 = arg 1
///   v2 = cmpgt v0 v1
///   br v2 b1 b2
/// block b1:
///   v3 = mov v0
///   jmp b3
/// block b2:
///   v4 = mov v1
///   jmp b3
/// block b3:
///   v5 = phi b1 v3, b2 v4
///   ret v5
pub fn parse(text: &str) -> Result<Function> {
    let mut lines = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#'));

    let header = lines.next().context("missing func header")?;
    let mut parts = header.split_whitespace();
    let kw = parts.next().unwrap();
    if kw != "func" { bail!("expected 'func'"); }
    let name = parts.next().context("missing name")?.to_string();
    let args_part = parts.next().context("missing args")?;
    let args = args_part.strip_prefix("args=").context("args=...")?.parse::<u32>()?;

    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;

    let mut label_to_id: HashMap<String, BlockId> = HashMap::new();
    let mut pending_terms: Vec<(usize, String)> = Vec::new(); // block index, term text
    let mut pending_phis: Vec<(usize, usize, String)> = Vec::new(); // block idx, inst idx, phi text

    fn parse_vreg(s: &str) -> Result<VReg> {
        let n = s.strip_prefix('v').context("vN")?.parse::<u32>()?;
        Ok(VReg(n))
    }
    fn parse_blockid(name: &str) -> Result<BlockId> {
        let n = name.strip_prefix('b').context("bN")?.parse::<u32>()?;
        Ok(BlockId(n))
    }

    while let Some(line) = lines.next() {
        if line.starts_with("block ") {
            if let Some(b) = cur.take() { blocks.push(b); }
            let label = line.strip_prefix("block ").unwrap().trim().trim_end_matches(':').to_string();
            let id = parse_blockid(&label)?;
            label_to_id.insert(label, id);
            cur = Some(Block { id, insts: vec![], term: Terminator::Jmp { target: id } });
            continue;
        }
        let b = cur.as_mut().context("instruction outside block")?;

        if line.starts_with("ret ") || line.starts_with("jmp ") || line.starts_with("br ") {
            pending_terms.push((blocks.len(), line.to_string()));
            b.term = Terminator::Jmp { target: b.id }; // temp
            continue;
        }

        // vX = op ...
        let (lhs, rhs) = line.split_once('=').context("expected '='")?;
        let dst = parse_vreg(lhs.trim())?;
        let rhs = rhs.trim();
        let mut p = rhs.split_whitespace();
        let op = p.next().context("missing op")?;
        match op {
            "const" => {
                let imm = p.next().context("imm")?.parse::<i64>()?;
                b.insts.push(Inst::ConstI64 { dst, imm });
            }
            "arg" => {
                let idx = p.next().context("arg idx")?.parse::<u32>()?;
                b.insts.push(Inst::ArgI64 { dst, idx: ArgId(idx) });
            }
            "add" => {
                let a = parse_vreg(p.next().context("a")?)?;
                let b2 = parse_vreg(p.next().context("b")?)?;
                b.insts.push(Inst::AddI64 { dst, a, b: b2 });
            }
            "mul" => {
                let a = parse_vreg(p.next().context("a")?)?;
                let b2 = parse_vreg(p.next().context("b")?)?;
                b.insts.push(Inst::MulI64 { dst, a, b: b2 });
            }
            "mov" => {
                let s = parse_vreg(p.next().context("src")?)?;
                b.insts.push(Inst::MovI64 { dst, src: s });
            }
            "cmpgt" => {
                let a = parse_vreg(p.next().context("a")?)?;
                let b2 = parse_vreg(p.next().context("b")?)?;
                b.insts.push(Inst::CmpGtI64 { dst, a, b: b2 });
            }
            "phi" => {
                // defer because needs block id mapping
                let inst_idx = b.insts.len();
                b.insts.push(Inst::ConstI64 { dst, imm: 0 }); // placeholder
                pending_phis.push((blocks.len(), inst_idx, rhs.to_string()));
            }
            _ => bail!("unknown op: {op}"),
        }
    }
    if let Some(b) = cur.take() { blocks.push(b); }
    if blocks.is_empty() { bail!("no blocks"); }

    // Resolve phis
    for (block_idx, inst_idx, text_phi) in pending_phis {
        let blk = blocks.get_mut(block_idx).context("bad block idx")?;
        let mut it = text_phi.split_whitespace();
        let _phi = it.next().unwrap(); // "phi"
        let rest: String = it.collect::<Vec<_>>().join(" ");
        // format: "b1 v3, b2 v4"
        let mut incoming: SmallVec<[(BlockId, VReg); 2]> = SmallVec::new();
        for chunk in rest.split(',').map(|c| c.trim()).filter(|c| !c.is_empty()) {
            let mut p = chunk.split_whitespace();
            let bname = p.next().context("phi block")?;
            let vname = p.next().context("phi vreg")?;
            let bid = *label_to_id.get(bname).context("unknown block label in phi")?;
            let vr = parse_vreg(vname)?;
            incoming.push((bid, vr));
        }
        let dst = match blk.insts[inst_idx] {
            Inst::ConstI64 { dst, .. } => dst, // placeholder stored dst here
            _ => bail!("phi placeholder mismatch"),
        };
        blk.insts[inst_idx] = Inst::PhiI64 { dst, incoming };
    }

    // Resolve terminators
    for (block_idx, term_text) in pending_terms {
        let blk = blocks.get_mut(block_idx).context("bad block idx")?;
        let mut p = term_text.split_whitespace();
        let op = p.next().unwrap();
        match op {
            "ret" => {
                let v = parse_vreg(p.next().context("ret vreg")?)?;
                blk.term = Terminator::Ret { value: v };
            }
            "jmp" => {
                let t = p.next().context("jmp target")?.to_string();
                let bid = *label_to_id.get(&t).context("unknown jmp target")?;
                blk.term = Terminator::Jmp { target: bid };
            }
            "br" => {
                let c = parse_vreg(p.next().context("br cond")?)?;
                let t = p.next().context("then")?.to_string();
                let e = p.next().context("else")?.to_string();
                let tb = *label_to_id.get(&t).context("unknown then bb")?;
                let eb = *label_to_id.get(&e).context("unknown else bb")?;
                blk.term = Terminator::Br { cond: c, then_bb: tb, else_bb: eb };
            }
            _ => bail!("unknown terminator: {op}"),
        }
    }

    Ok(Function { name, args, entry: blocks[0].id, blocks })
}
