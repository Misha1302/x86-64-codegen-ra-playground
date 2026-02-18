use anyhow::Result;
use regex::Regex;
use std::process::Command;

#[test]
fn cli_report_contains_expected_fields() -> Result<()> {
    let out = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["run", "--example", "basicblock", "--regs", "4", "--dump-disasm"])
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    anyhow::ensure!(out.status.success(), "cli failed:\n{}\n{}", stdout, stderr);

    let re = Regex::new(r"Stack slots:\s+\d+\s+Spills:\s+\d+")?;
    assert!(re.is_match(&stdout));

    assert!(stdout.contains("-- Assignment --"));
    assert!(stdout.contains("-- Validation --"));
    assert!(stdout.contains("-- Disasm --"));
    Ok(())
}
