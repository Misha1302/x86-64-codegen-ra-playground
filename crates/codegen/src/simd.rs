use anyhow::Result;
use iced_x86::code_asm::*;

/// A tiny SIMD codegen template:
/// - scalar: sum 8 f32 from ptr and return f32 in xmm0
/// - simd: use addps and horizontal add to reduce
///
/// Signature (SysV):
/// extern "C" fn(ptr: *const f32) -> f32
///
/// Note: this is a demo; real playground would lower IR to SIMD.
pub fn emit_sum8_scalar() -> Result<Vec<u8>> {
    let mut a = CodeAssembler::new(64)?;
    // prologue
    a.push(rbp)?;
    a.mov(rbp, rsp)?;
    // rdi = ptr
    // xmm0 = 0
    a.xorps(xmm0, xmm0)?;
    for i in 0..8 {
        // movss xmm1, [rdi + i*4]
        a.movss(xmm1, dword_ptr(rdi + i * 4))?;
        a.addss(xmm0, xmm1)?;
    }
    // epilogue
    a.pop(rbp)?;
    a.ret()?;
    Ok(a.assemble(0)?)
}

pub fn emit_sum8_sse() -> Result<Vec<u8>> {
    let mut a = CodeAssembler::new(64)?;
    a.push(rbp)?;
    a.mov(rbp, rsp)?;
    // load 8 floats: xmm0=[0..3], xmm1=[4..7]
    a.movups(xmm0, xmmword_ptr(rdi))?;
    a.movups(xmm1, xmmword_ptr(rdi + 16))?;
    a.addps(xmm0, xmm1)?;
    // horizontal reduce xmm0: [a b c d] -> sum
    // shuffle + add: (a+c, b+d, ?, ?)
    a.movaps(xmm1, xmm0)?;
    a.shufps(xmm1, xmm1, 0b01_00_11_10)?; // swap pairs
    a.addps(xmm0, xmm1)?;
    a.movaps(xmm1, xmm0)?;
    a.shufps(xmm1, xmm1, 0b10_11_00_01)?; // rotate
    a.addss(xmm0, xmm1)?; // sum in xmm0[0]
    a.pop(rbp)?;
    a.ret()?;
    Ok(a.assemble(0)?)
}
