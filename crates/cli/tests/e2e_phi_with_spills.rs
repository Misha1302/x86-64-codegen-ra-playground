use anyhow::Result;
use std::process::Command;

fn run_cli(args: &[&str]) -> Result<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_cli")).args(args).output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    anyhow::ensure!(out.status.success(), "cli failed:\n{}\n{}", stdout, stderr);
    Ok(stdout)
}

#[test]
fn e2e_trace_phi_under_reg_pressure() -> Result<()> {
    // trace has branches + join. regs=2 is the tightest meaningful pressure.
    for regs in [2usize, 3usize] {
        let out = run_cli(&["run", "--example", "trace", "--regs", &regs.to_string(), "--dump-disasm"])?;
        // must validate (cli would fail), plus we also sanity-check it emitted conditional jumps
        assert!(out.contains("jne") || out.contains("jnz"), "no conditional branch in disasm");
    }
    Ok(())
}
