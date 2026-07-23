use anyhow::{bail, Result};
use alloc::{verify_assignment, Assignment, Location, PhysReg, PhysRegSet, StackSlot};
use analysis::{compute_live_intervals, validate_function};
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

fn reg64(register: PhysReg) -> AsmRegister64 {
    match register {
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

fn stack_addr_with_base(arg_shadow_slots: i32, slot: StackSlot) -> AsmMemoryOperand {
    let index = arg_shadow_slots + slot.index as i32;
    qword_ptr(rbp - 8_i32 * (index + 1))
}

fn arg_shadow_addr(arg_index: i32) -> AsmMemoryOperand {
    qword_ptr(rbp - 8_i32 * (arg_index + 1))
}

fn get_val(
    assembler: &mut CodeAssembler,
    locations: &IndexMap<VReg, Location>,
    loads: &mut u32,
    arg_shadow_slots: i32,
    value: VReg,
    preferred: AsmRegister64,
) -> Result<AsmRegister64> {
    match *locations
        .get(&value)
        .ok_or_else(|| anyhow::anyhow!("missing vreg {:?}", value))?
    {
        Location::Reg(register) => Ok(reg64(register)),
        Location::Stack(slot) => {
            assembler.mov(preferred, stack_addr_with_base(arg_shadow_slots, slot))?;
            *loads += 1;
            Ok(preferred)
        }
    }
}

fn set_val(
    assembler: &mut CodeAssembler,
    locations: &IndexMap<VReg, Location>,
    stores: &mut u32,
    arg_shadow_slots: i32,
    value: VReg,
    source: AsmRegister64,
) -> Result<()> {
    match *locations
        .get(&value)
        .ok_or_else(|| anyhow::anyhow!("missing vreg {:?}", value))?
    {
        Location::Reg(register) => {
            let destination = reg64(register);
            if destination != source {
                assembler.mov(destination, source)?;
            }
        }
        Location::Stack(slot) => {
            assembler.mov(stack_addr_with_base(arg_shadow_slots, slot), source)?;
            *stores += 1;
        }
    }
    Ok(())
}

fn label(labels: &IndexMap<BlockId, CodeLabel>, id: BlockId) -> CodeLabel {
    *labels.get(&id).expect("validated block label")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveLocation {
    Reg(AsmRegister64),
    Stack(StackSlot),
}

fn move_location(locations: &IndexMap<VReg, Location>, value: VReg) -> MoveLocation {
    match locations[&value] {
        Location::Reg(register) => MoveLocation::Reg(reg64(register)),
        Location::Stack(slot) => MoveLocation::Stack(slot),
    }
}

fn read_move_location(
    assembler: &mut CodeAssembler,
    loads: &mut u32,
    arg_shadow_slots: i32,
    source: MoveLocation,
    temporary: AsmRegister64,
) -> Result<AsmRegister64> {
    match source {
        MoveLocation::Reg(register) => Ok(register),
        MoveLocation::Stack(slot) => {
            assembler.mov(temporary, stack_addr_with_base(arg_shadow_slots, slot))?;
            *loads += 1;
            Ok(temporary)
        }
    }
}

fn write_move_location(
    assembler: &mut CodeAssembler,
    stores: &mut u32,
    arg_shadow_slots: i32,
    destination: MoveLocation,
    source: AsmRegister64,
) -> Result<()> {
    match destination {
        MoveLocation::Reg(register) => {
            if register != source {
                assembler.mov(register, source)?;
            }
        }
        MoveLocation::Stack(slot) => {
            assembler.mov(stack_addr_with_base(arg_shadow_slots, slot), source)?;
            *stores += 1;
        }
    }
    Ok(())
}

fn emit_parallel_moves(
    assembler: &mut CodeAssembler,
    loads: &mut u32,
    stores: &mut u32,
    arg_shadow_slots: i32,
    mut moves: Vec<(MoveLocation, MoveLocation)>,
    temporary: AsmRegister64,
    temporary2: AsmRegister64,
) -> Result<()> {
    moves.retain(|(destination, source)| destination != source);

    while !moves.is_empty() {
        let sources: Vec<MoveLocation> = moves.iter().map(|(_, source)| *source).collect();
        if let Some(index) = moves
            .iter()
            .position(|(destination, _)| !sources.contains(destination))
        {
            let (destination, source) = moves.remove(index);
            let source_register = read_move_location(
                assembler,
                loads,
                arg_shadow_slots,
                source,
                temporary,
            )?;
            write_move_location(
                assembler,
                stores,
                arg_shadow_slots,
                destination,
                source_register,
            )?;
            continue;
        }

        let (destination, source) = moves.remove(0);
        let old_destination = read_move_location(
            assembler,
            loads,
            arg_shadow_slots,
            destination,
            temporary,
        )?;
        if old_destination != temporary {
            assembler.mov(temporary, old_destination)?;
        }
        let source_register = read_move_location(
            assembler,
            loads,
            arg_shadow_slots,
            source,
            temporary2,
        )?;
        write_move_location(
            assembler,
            stores,
            arg_shadow_slots,
            destination,
            source_register,
        )?;
        for (_, remaining_source) in &mut moves {
            if *remaining_source == destination {
                *remaining_source = MoveLocation::Reg(temporary);
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

fn collect_phis(function: &Function) -> IndexMap<BlockId, Vec<Phi>> {
    let mut phis = IndexMap::new();
    for block in &function.blocks {
        for inst in &block.insts {
            if let Inst::PhiI64 { dst, incoming } = inst {
                phis.entry(block.id).or_insert_with(Vec::new).push(Phi {
                    dst: *dst,
                    incoming: incoming.iter().copied().collect(),
                });
            }
        }
    }
    phis
}

fn phi_moves_for_edge(
    phis: &IndexMap<BlockId, Vec<Phi>>,
    locations: &IndexMap<VReg, Location>,
    predecessor: BlockId,
    successor: BlockId,
) -> Vec<(MoveLocation, MoveLocation)> {
    let mut moves = Vec::new();
    let Some(successor_phis) = phis.get(&successor) else {
        return moves;
    };

    for phi in successor_phis {
        if let Some((_, source)) = phi
            .incoming
            .iter()
            .find(|(block, _)| *block == predecessor)
        {
            let destination = move_location(locations, phi.dst);
            let source = move_location(locations, *source);
            if destination != source {
                moves.push((destination, source));
            }
        }
    }
    moves
}

pub fn emit_function_i64(
    function: &Function,
    assignment: &Assignment,
    registers: &PhysRegSet,
) -> Result<EmittedCode> {
    validate_function(function)?;
    registers.validate_for_codegen()?;
    if function.args > 6 {
        bail!("MVP supports up to 6 arguments");
    }
    let intervals = compute_live_intervals(function)?;
    verify_assignment(&intervals, registers, registers.regs.len(), assignment)?;

    let scratch0 = reg64(registers.scratch[0]);
    let scratch1 = reg64(registers.scratch[1]);
    let mut assembler = CodeAssembler::new(64)?;
    let mut loads = 0_u32;
    let mut stores = 0_u32;

    let arg_shadow_slots = function.args as i32;
    let spill_slots = i32::try_from(assignment.stack_slots)?;
    let total_slots = arg_shadow_slots
        .checked_add(spill_slots)
        .ok_or_else(|| anyhow::anyhow!("stack frame slot overflow"))?;

    assembler.push(rbp)?;
    assembler.mov(rbp, rsp)?;

    let aligned_bytes = if total_slots == 0 {
        0
    } else {
        let bytes = total_slots
            .checked_mul(8)
            .ok_or_else(|| anyhow::anyhow!("stack frame byte overflow"))?;
        ((bytes + 15) / 16) * 16
    };
    if aligned_bytes > 0 {
        assembler.sub(rsp, aligned_bytes)?;
    }

    let argument_registers: [AsmRegister64; 6] = [rdi, rsi, rdx, rcx, r8, r9];
    for index in 0..function.args as i32 {
        assembler.mov(arg_shadow_addr(index), argument_registers[index as usize])?;
        stores += 1;
    }

    let locations = assignment.map.clone();
    let mut labels = IndexMap::new();
    for block in &function.blocks {
        labels.insert(block.id, assembler.create_label());
    }
    let phis = collect_phis(function);
    assembler.jmp(label(&labels, function.entry))?;

    for block in &function.blocks {
        let block_label = labels.get_mut(&block.id).expect("validated block label");
        assembler.set_label(block_label)?;

        for inst in &block.insts {
            match *inst {
                Inst::ConstI64 { dst, imm } => {
                    let destination = match locations[&dst] {
                        Location::Reg(register) => reg64(register),
                        Location::Stack(_) => scratch0,
                    };
                    assembler.mov(destination, imm)?;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        destination,
                    )?;
                }
                Inst::AddI64 { dst, a, b } => {
                    let destination = match locations[&dst] {
                        Location::Reg(register) => reg64(register),
                        Location::Stack(_) => scratch0,
                    };
                    let left = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        a,
                        destination,
                    )?;
                    if left != destination {
                        assembler.mov(destination, left)?;
                    }
                    let right = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        b,
                        scratch1,
                    )?;
                    assembler.add(destination, right)?;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        destination,
                    )?;
                }
                Inst::MulI64 { dst, a, b } => {
                    let destination = match locations[&dst] {
                        Location::Reg(register) => reg64(register),
                        Location::Stack(_) => scratch0,
                    };
                    let left = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        a,
                        destination,
                    )?;
                    if left != destination {
                        assembler.mov(destination, left)?;
                    }
                    let right = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        b,
                        scratch1,
                    )?;
                    assembler.imul_2(destination, right)?;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        destination,
                    )?;
                }
                Inst::MovI64 { dst, src } => {
                    let source = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        src,
                        scratch0,
                    )?;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        source,
                    )?;
                }
                Inst::CmpGtI64 { dst, a, b } => {
                    let left = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        a,
                        scratch0,
                    )?;
                    let right = get_val(
                        &mut assembler,
                        &locations,
                        &mut loads,
                        arg_shadow_slots,
                        b,
                        scratch1,
                    )?;
                    assembler.cmp(left, right)?;
                    let mut true_label = assembler.create_label();
                    let mut done_label = assembler.create_label();
                    assembler.jg(true_label)?;
                    assembler.mov(scratch0, 0_i64)?;
                    assembler.jmp(done_label)?;
                    assembler.set_label(&mut true_label)?;
                    assembler.mov(scratch0, 1_i64)?;
                    assembler.set_label(&mut done_label)?;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        scratch0,
                    )?;
                }
                Inst::PhiI64 { .. } => {}
                Inst::ArgI64 { dst, idx } => {
                    assembler.mov(scratch0, arg_shadow_addr(idx.0 as i32))?;
                    loads += 1;
                    set_val(
                        &mut assembler,
                        &locations,
                        &mut stores,
                        arg_shadow_slots,
                        dst,
                        scratch0,
                    )?;
                }
            }
        }

        match block.term {
            Terminator::Ret { value } => {
                let result = get_val(
                    &mut assembler,
                    &locations,
                    &mut loads,
                    arg_shadow_slots,
                    value,
                    rax,
                )?;
                if result != rax {
                    assembler.mov(rax, result)?;
                }
                if aligned_bytes > 0 {
                    assembler.add(rsp, aligned_bytes)?;
                }
                assembler.pop(rbp)?;
                assembler.ret()?;
            }
            Terminator::Jmp { target } => {
                let moves = phi_moves_for_edge(&phis, &locations, block.id, target);
                emit_parallel_moves(
                    &mut assembler,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    moves,
                    scratch0,
                    scratch1,
                )?;
                assembler.jmp(label(&labels, target))?;
            }
            Terminator::Br {
                cond,
                then_bb,
                else_bb,
            } => {
                let condition = get_val(
                    &mut assembler,
                    &locations,
                    &mut loads,
                    arg_shadow_slots,
                    cond,
                    scratch0,
                )?;
                assembler.test(condition, condition)?;
                let mut then_stub = assembler.create_label();
                let mut else_stub = assembler.create_label();
                assembler.jnz(then_stub)?;
                assembler.jmp(else_stub)?;

                assembler.set_label(&mut then_stub)?;
                let then_moves = phi_moves_for_edge(&phis, &locations, block.id, then_bb);
                emit_parallel_moves(
                    &mut assembler,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    then_moves,
                    scratch0,
                    scratch1,
                )?;
                assembler.jmp(label(&labels, then_bb))?;

                assembler.set_label(&mut else_stub)?;
                let else_moves = phi_moves_for_edge(&phis, &locations, block.id, else_bb);
                emit_parallel_moves(
                    &mut assembler,
                    &mut loads,
                    &mut stores,
                    arg_shadow_slots,
                    else_moves,
                    scratch0,
                    scratch1,
                )?;
                assembler.jmp(label(&labels, else_bb))?;
            }
        }
    }

    let bytes = assembler.assemble(0)?;
    Ok(EmittedCode {
        metrics: CodegenMetrics {
            code_size: bytes.len(),
            loads,
            stores,
        },
        bytes,
    })
}
