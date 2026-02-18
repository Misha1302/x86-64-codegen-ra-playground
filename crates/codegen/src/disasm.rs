use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter};

pub fn disasm(bytes: &[u8], ip: u64) -> String {
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    decoder.set_ip(ip);
    let mut fmt = NasmFormatter::new();
    let mut out = String::new();
    let mut instr = Instruction::default();
    while decoder.can_decode() {
        decoder.decode_out(&mut instr);
        let mut s = String::new();
        fmt.format(&instr, &mut s);
        out.push_str(&format!("{:016X}  {}
", instr.ip(), s));
    }
    out
}
