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
fn e2e_basicblock_arg_shadow_regs_2_to_6() -> Result<()> {
    // This used to fail when arg0 was assigned to RDX and arg2 lived in RDX.
    for regs in 2..=6 {
        let _ = run_cli(&["run", "--example", "basicblock", "--regs", &regs.to_string()])?;
    }
    Ok(())
}
