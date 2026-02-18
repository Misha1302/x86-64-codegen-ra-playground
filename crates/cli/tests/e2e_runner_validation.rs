use anyhow::Result;
use std::process::Command;

fn run_cli(args: &[&str]) -> Result<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(args)
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    anyhow::ensure!(out.status.success(), "cli failed:\n{}\n{}", stdout, stderr);
    Ok(stdout)
}

#[test]
fn e2e_basicblock_multiple_regs() -> Result<()> {
    // Just ensure it runs + validates for multiple reg counts
    for regs in [2, 3, 4, 5, 6] {
        let _ = run_cli(&[
            "run",
            "--example",
            "basicblock",
            "--regs",
            &regs.to_string(),
        ])?;
    }
    Ok(())
}

#[test]
fn e2e_trace_multiple_regs() -> Result<()> {
    for regs in [2, 3, 4, 5, 6] {
        let _ = run_cli(&["run", "--example", "trace", "--regs", &regs.to_string()])?;
    }
    Ok(())
}
