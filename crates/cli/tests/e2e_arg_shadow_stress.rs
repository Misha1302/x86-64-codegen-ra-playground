use anyhow::Result;
use std::process::Command;

#[test]
fn argument_shadowing_survives_all_register_counts() -> Result<()> {
    for regs in 0..=5 {
        let output = Command::new(env!("CARGO_BIN_EXE_cli"))
            .args([
                "run",
                "--example",
                "basicblock",
                "--regs",
                &regs.to_string(),
            ])
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "cli failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
