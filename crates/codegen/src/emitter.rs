use anyhow::{bail, Result};
use alloc::{Assignment, Location, PhysReg, PhysRegSet, StackSlot};
use iced_x86::code_asm::*;
use indexmap::IndexMap;
use ir::{BlockId, Function, Inst, Terminator, VReg};

#[derive(Debug, Clone)]
pub struct CodegenMetrics {
    pub code_size: usize,
    pub loads: u32,
    pub stores: u32,
}

#[derive(Debug, Clone)]
pub struct EmittedCode {
    pub bytes: Vec<u8>,
    pub metrics: CodegenMetrics,
}

fn reg64(r: PhysReg) -> AsmRegister64 {
    match r {
        PhysReg::Rax => rax,
        PhysReg::Rcx => rcx,
        PhysReg::Rdx => rdx,
        PhysReg::Rbx => rbx,
        PhysReg::Rsi => rsi,
        PhysReg::Rdi => rdi,
        PhysReg::R8 => r8,
        PhysReg::R9 => r9,
        PhysReg::R10 => r10,
        PhysReg::R11 => r11,
    }
}

/// rbp-based frame.
/// We reserve:
/// - arg shadow slots: indices [0 .. arg_shadow_slots)
/// - spill slots: after that, indices [arg_shadow_slots .. arg_shadow_slots + stack_slots)
fn stack_addr_with_base(arg_shadow_slots: i32, slot: StackSlot) -> AsmMemoryOperand {
    let idx = arg_shadow_slots + (slot.index as i32);
    let disp = -8i32 * (idx + 1);
    qword_ptr(rbp + disp)
}

fn arg_shadow_addr(arg_idx: i32) -> AsmMemoryOperand {
    // arg shadow lives starting at slot 0: [rbp-8], [rbp-16], ...
    let disp = -8i32 * (arg_idx + 1);
    qword_ptr(rbp + disp)
}

fn get_val(
    a: &mut CodeAssembler,
    loc: &IndexMap<VReg, Location>,
    loads: &mut u32,
    arg_shadow_slots: i32,
    v: VReg,
    prefer: AsmRegister64,
) -> Result<AsmRegister64> {
    match *loc
        .get(&v)
        .ok_or_else(|| anyhow::anyhow!("missing vreg {:?}", v))?
    {
        Location::Reg(r) => Ok(reg64(r)),
        Location::Stack(slot) => {
            a.mov(prefer, stack_addr_with_base(arg_shadow_slots, slot))?;
            *loads += 1;
            Ok(prefer)
        }
    }
}

fn set_val(
    a: &mut CodeAssembler,
    loc: &IndexMap<VReg, Location>,
    stores: &mut u32,
    arg_shadow_slots: i32,
    v: VReg,
    from: AsmRegister64,
) -> Result<()> {
    match *loc
        .get(&v)
        .ok_or_else(|| anyhow::anyhow!("missing vreg {:?}", v))?
    {
        Location::Reg(r) => {
            let rr = reg64(r);
            if rr != from {
                a.mov(rr, from)?;
            }
        }
        Location::Stack(slot) => {
            a.mov(stack_addr_with_base(arg_shadow_slots, slot), from)?;
            *stores += 1;
        }
    }
    Ok(())
}

fn lbl(labels: &IndexMap<BlockId, CodeLabel>, id: BlockId) -> CodeLabel {
    *labels.get(&id).expect("label exists")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum L {
    Reg(AsmRegister64),
    Stack(StackSlot),
}

fn loc_of_vreg(loc: &IndexMap<VReg, Location>, v: VReg) -> L {
    match loc[&v] {
        Location::Reg(r) => L::Reg(reg64(r)),
        Location::Stack(s) => L::Stack(s),
    }
}

fn read_loc_to_reg(
    a: &mut CodeAssembler,
    loads: &mut u32,
    arg_shadow_slots: i32,
    src: L,
    tmp: AsmRegister64,
) -> Result<AsmRegister64> {
    match src {
        L::Reg(r) => Ok(r),
        L::Stack(s) => {
            a.mov(tmp, stack_addr_with_base(arg_shadow_slots, s))?;
            *loads += 1;
            Ok(tmp)
        }
    }
}

fn write_reg_to_loc(
    a: &mut CodeAssembler,
    stores: &mut u32,
    arg_shadow_slots: i32,
    dst: L,
    from: AsmRegister64,
) -> Result<()> {
    match dst {
        L::Reg(r) => {
            if r != from {
                a.mov(r, from)?;
            }
        }
        L::Stack(s) => {
            a.mov(stack_addr_with_base(arg_shadow_slots, s), from)?;
            *stores += 1;
        }
    }
    Ok(())
}

/// Emit parallel moves (phi-lowering) safely (handles cycles) using tmp regs.
fn emit_parallel_moves(
    a: &mut CodeAssembler,
    loads: &mut u32,
    stores: &mut u32,
    arg_shadow_slots: i32,
    mut moves: Vec<(L, L)>, // (dst, src)
    tmp: AsmRegister64,
    tmp2: AsmRegister64,
) -> Result<()> {
    moves.retain(|(d, s)| d != s);
    if moves.is_empty() {
        return Ok(());
    }

    let srcs = |ms: &Vec<(L, L)>| -> Vec<L> { ms.iter().map(|(_, s)| *s).collect() };

    while !moves.is_empty() {
        let sources = srcs(&moves);

        if let Some(idx) = moves
            .iter()
            .position(|(d, _)| !sources.iter().any(|s| *s == *d))
        {
            let (dst, src) = moves.remove(idx);
            let rsrc = read_loc_to_reg(a, loads, arg_shadow_slots, src, tmp)?;
            write_reg_to_loc(a, stores, arg_shadow_slots, dst, rsrc)?;
            continue;
        }

        // Cycle: break it using tmp (save old dst)
        let (dst, src) = moves.remove(0);

        let old_dst = read_loc_to_reg(a, loads, arg_shadow_slots, dst, tmp)?;
        if old_dst != tmp {
            a.mov(tmp, old_dst)?;
        }

        let rsrc = read_loc_to_reg(a, loads, arg_shadow_slots, src, tmp2)?;
        write_reg_to_loc(a, stores, arg_shadow_slots, dst, rsrc)?;

        for (_, s) in moves.iter_mut() {
            if *s == dst {
                *s = L::Reg(tmp);
            }
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct Phi {
    dst: VReg,
    incoming: Vec<(BlockId, VReg)>,
}

fn collect_phis(f: &Function) -> IndexMap<BlockId, Vec<Phi>> {
    let mut map: IndexMap<BlockId, Vec<Phi>> = IndexMap::new();
    for b in &f.blocks {
        for inst in &b.insts {
            if let Inst::PhiI64 { dst, incoming } = inst {
                let inc = incoming.iter().copied().collect::<Vec<_>>();
                map.entry(b.id).or_default().push(Phi { dst: *dst, incoming: inc });
            }
        }
    }
    map
}

fn phi_moves_for_edge(
    phis: &IndexMap<BlockId, Vec<Phi>>,
    loc: &IndexMap<VReg, Location>,
    pred: BlockId,
    succ: BlockId,
) -> Vec<(L, L)> {
    let mut moves = Vec::new();
    let Some(list) = phis.get(&succ) else { return moves; };

    for phi in list {
        if let Some((_, src_v)) = phi.incoming.iter().find(|(bb, _)| *bb == pred) {
            let dst_l = loc_of_vreg(loc, phi.dst);
            let src_l = loc_of_vreg(loc, *src_v);
            if dst_l != src_l {
                moves.push((dst_l, src_l));
            }
        }
    }

    moves
}

pub fn emit_function_i64(
    f: &Function,
    assignment: &Assignment,
    regs: &PhysRegSet,
) -> Result<EmittedCode> {
    if f.args > 6 {
        bail!("MVP supports up to 6 args");
    }

    let scratch0 = reg64(*regs.scratch.get(0).unwrap_or(&PhysReg::R10));
    let scratch1 = reg64(*regs.scratch.get(1).unwrap_or(&PhysReg::R11));

    let mut a = CodeAssembler::new(64)?;
    let mut loads = 0u32;
    let mut stores = 0u32;

    // --- Frame layout ---
    // Reserve arg shadow slots first (so we never clobber incoming arg regs):
    // shadow_slots = f.args
    // spill_slots = assignment.stack_slots
    let arg_shadow_slots = f.args as i32;
    let spill_slots = assignment.stack_slots as i32;
    let total_slots = arg_shadow_slots + spill_slots;

    // Prologue
    a.push(rbp)?;
    a.mov(rbp, rsp)?;

    let mut aligned = 0i32;
    if total_slots > 0 {
        let bytes = total_slots * 8;
        aligned = ((bytes + 15) / 16) * 16;
        a.sub(rsp, aligned)?;
    }

    // Save incoming args into shadow area immediately
    let arg_regs: [AsmRegister64; 6] = [rdi, rsi, rdx, rcx, r8, r9];
    for i in 0..(f.args as i32) {
        a.mov(arg_shadow_addr(i), arg_regs[i as usize])?;
        stores += 1;
    }

    let loc: IndexMap<VReg, Location> = assignment.map.clone();

    // Materialize ArgI64 from shadow slots into assigned locations
    for b in &f.blocks {
        for inst in &b.insts {
            if let Inst::ArgI64 { dst, idx } = *inst {
                let ai = idx.0 as i32; // ArgId newtype
                // load from shadow slot into scratch0, then store to destination
                a.mov(scratch0, arg_shadow_addr(ai))?;
                loads += 1;
                set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, scratch0)?;
            }
        }
    }

    // Labels
    let mut labels: IndexMap<BlockId, CodeLabel> = IndexMap::new();
    for b in &f.blocks {
        labels.insert(b.id, a.create_label());
    }

    // Phi info
    let phis = collect_phis(f);

    // Jump to entry
    a.jmp(lbl(&labels, f.entry))?;

    // Emit blocks
    for b in &f.blocks {
        let l = labels.get_mut(&b.id).expect("label exists");
        a.set_label(l)?;

        for inst in &b.insts {
            match *inst {
                Inst::ConstI64 { dst, imm } => {
                    let tmp = match loc[&dst] {
                        Location::Reg(r) => reg64(r),
                        Location::Stack(_) => scratch0,
                    };
                    a.mov(tmp, imm)?;
                    set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, tmp)?;
                }
                Inst::AddI64 { dst, a: va, b: vb } => {
                    let dst_reg = match loc[&dst] {
                        Location::Reg(r) => reg64(r),
                        Location::Stack(_) => scratch0,
                    };

                    let ra = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, va, dst_reg)?;
                    if ra != dst_reg {
                        a.mov(dst_reg, ra)?;
                    }
                    let rb = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, vb, scratch1)?;
                    a.add(dst_reg, rb)?;
                    set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, dst_reg)?;
                }
                Inst::MulI64 { dst, a: va, b: vb } => {
                    let dst_reg = match loc[&dst] {
                        Location::Reg(r) => reg64(r),
                        Location::Stack(_) => scratch0,
                    };

                    let ra = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, va, dst_reg)?;
                    if ra != dst_reg {
                        a.mov(dst_reg, ra)?;
                    }
                    let rb = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, vb, scratch1)?;
                    a.imul_2(dst_reg, rb)?;
                    set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, dst_reg)?;
                }
                Inst::MovI64 { dst, src } => {
                    let rsrc = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, src, scratch0)?;
                    set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, rsrc)?;
                }
                Inst::CmpGtI64 { dst, a: va, b: vb } => {
                    let ra = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, va, scratch0)?;
                    let rb = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, vb, scratch1)?;
                    a.cmp(ra, rb)?;
                    a.setg(al)?;
                    a.movzx(scratch0, al)?;
                    set_val(&mut a, &loc, &mut stores, arg_shadow_slots, dst, scratch0)?;
                }
                Inst::PhiI64 { .. } => {
                    // handled by edge moves
                }
                Inst::ArgI64 { .. } => {
                    // already materialized above
                }
            }
        }

        match b.term.clone() {
            Terminator::Ret { value } => {
                let rv = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, value, rax)?;
                if rv != rax {
                    a.mov(rax, rv)?;
                }
                if aligned > 0 {
                    a.add(rsp, aligned)?;
                }
                a.pop(rbp)?;
                a.ret()?;
            }
            Terminator::Jmp { target } => {
                let mv = phi_moves_for_edge(&phis, &loc, b.id, target);
                emit_parallel_moves(
                    &mut a,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    mv,
                    scratch0,
                    scratch1,
                )?;
                a.jmp(lbl(&labels, target))?;
            }
            Terminator::Br { cond, then_bb, else_bb } => {
                let rc = get_val(&mut a, &loc, &mut loads, arg_shadow_slots, cond, scratch0)?;
                a.test(rc, rc)?;

                let mut then_stub = a.create_label();
                let mut else_stub = a.create_label();

                a.jnz(then_stub)?;
                a.jmp(else_stub)?;

                a.set_label(&mut then_stub)?;
                let mv_t = phi_moves_for_edge(&phis, &loc, b.id, then_bb);
                emit_parallel_moves(
                    &mut a,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    mv_t,
                    scratch0,
                    scratch1,
                )?;
                a.jmp(lbl(&labels, then_bb))?;

                a.set_label(&mut else_stub)?;
                let mv_e = phi_moves_for_edge(&phis, &loc, b.id, else_bb);
                emit_parallel_moves(
                    &mut a,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    mv_e,
                    scratch0,
                    scratch1,
                )?;
                a.jmp(lbl(&labels, else_bb))?;
            }
        }
    }

    let bytes = a.assemble(0)?;
    Ok(EmittedCode {
        metrics: CodegenMetrics {
            code_size: bytes.len(),
            loads,
            stores,
        },
        bytes,
    })
}

