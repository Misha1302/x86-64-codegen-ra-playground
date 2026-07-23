use anyhow::Result;
use std::process::Command;

#[test]
fn phi_edges_and_parallel_cycles_work_with_spills() -> Result<()> {
    for example in ["trace", "phi-swap-loop"] {
        for allocator in ["linear-scan", "sim-anneal"] {
            for regs in [0_usize, 1, 2] {
                let output = Command::new(env!("CARGO_BIN_EXE_cli"))
                    .args([
                        "run",
                        "--example",
                        example,
                        "--alloc",
                        allocator,
                        "--regs",
                        &regs.to_string(),
                        "--dump-disasm",
                    ])
                    .output()?;
                anyhow::ensure!(
                    output.status.success(),
                    "cli failed:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(String::from_utf8_lossy(&output.stdout).contains("differential cases passed"));
            }
        }
    }
    Ok(())
}
