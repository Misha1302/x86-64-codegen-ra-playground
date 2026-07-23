use anyhow::Result;
use std::process::Command;

fn run_cli(args: &[&str]) -> Result<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(args)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    anyhow::ensure!(output.status.success(), "cli failed:\n{stdout}\n{stderr}");
    Ok(stdout)
}

#[test]
fn differential_validation_covers_cfgs_allocators_and_pressure() -> Result<()> {
    for example in ["basicblock", "trace", "loop-sum", "phi-swap-loop"] {
        for allocator in ["linear-scan", "sim-anneal"] {
            for regs in [0_usize, 1, 2, 5] {
                let stdout = run_cli(&[
                    "run",
                    "--example",
                    example,
                    "--alloc",
                    allocator,
                    "--regs",
                    &regs.to_string(),
                ])?;
                assert!(stdout.contains("differential cases passed"));
            }
        }
    }
    Ok(())
}

#[test]
fn rejects_unknown_examples_allocators_and_excess_registers() -> Result<()> {
    for args in [
        vec!["run", "--example", "missing"],
        vec!["run", "--alloc", "missing"],
        vec!["run", "--regs", "6"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cli"))
            .args(args)
            .output()?;
        assert!(!output.status.success());
    }
    Ok(())
}
