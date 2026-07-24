use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RunnerSpec {
    I64Cases { code: Vec<u8>, args: Vec<Vec<i64>> },
    Sum8F32 { code: Vec<u8>, iters: u32 },
}

pub struct RunnerClient {
    timeout: Duration,
}

impl Default for RunnerClient {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

impl RunnerClient {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn run(&self, spec: &RunnerSpec) -> Result<String> {
        let temp = tempfile::NamedTempFile::new().context("create runner spec")?;
        std::fs::write(temp.path(), serde_json::to_vec(spec)?).context("write runner spec")?;

        let executable = find_or_build_runner_exe().context("locate or build runner")?;
        let mut child = Command::new(executable)
            .arg("--spec")
            .arg(temp.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn runner")?;

        // Pipes must be drained while the child is running. Waiting first can deadlock when
        // the JSON result is larger than the operating system's pipe buffer.
        let stdout = read_in_background(child.stdout.take().context("capture runner stdout")?);
        let stderr = read_in_background(child.stderr.take().context("capture runner stderr")?);

        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() >= self.timeout {
                child.kill().context("kill timed-out runner")?;
                let _ = child.wait();
                let stdout = finish_read(stdout, "stdout")?;
                let stderr = finish_read(stderr, "stderr")?;
                anyhow::bail!(
                    "runner timed out after {:?}:\n{}\n{}",
                    self.timeout,
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
            }
            thread::sleep(Duration::from_millis(5));
        };

        let stdout = finish_read(stdout, "stdout")?;
        let stderr = finish_read(stderr, "stderr")?;
        let stdout = String::from_utf8_lossy(&stdout).to_string();
        let stderr = String::from_utf8_lossy(&stderr).to_string();
        anyhow::ensure!(status.success(), "runner failed:\n{stdout}\n{stderr}");
        Ok(stdout)
    }
}

fn read_in_background<R>(mut reader: R) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn finish_read(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("runner {stream} reader panicked"))?
        .with_context(|| format!("read runner {stream}"))
}

fn find_or_build_runner_exe() -> Result<PathBuf> {
    if let Some(path) = find_runner_next_to_cli()? {
        return Ok(path);
    }

    build_runner()?;
    if let Some(path) = find_runner_next_to_cli()? {
        return Ok(path);
    }

    for profile in ["debug", "release"] {
        let candidate = workspace_root()
            .join("target")
            .join(profile)
            .join(runner_exe_name());
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("runner executable not found after build")
}

fn find_runner_next_to_cli() -> Result<Option<PathBuf>> {
    let current = std::env::current_exe().context("current executable")?;
    let directory = current.parent().context("current executable parent")?;
    let candidate = directory.join(runner_exe_name());
    Ok(candidate.exists().then_some(candidate))
}

fn runner_exe_name() -> &'static str {
    if cfg!(windows) {
        "runner.exe"
    } else {
        "runner"
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn build_runner() -> Result<()> {
    let status = Command::new("cargo")
        .args(["build", "-p", "runner"])
        .current_dir(workspace_root())
        .status()
        .context("spawn cargo build -p runner")?;
    anyhow::ensure!(
        status.success(),
        "cargo build -p runner failed with {status}"
    );
    Ok(())
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use std::time::Duration;

    use alloc::PhysRegSet;
    use analysis::{compute_live_intervals, validate_function};
    use anyhow::Result;
    use codegen::emit_function_i64;

    use super::{RunnerClient, RunnerSpec};
    use crate::{allocators, parse_results};

    fn emit_constant(value: i64) -> Result<Vec<u8>> {
        let function = ir::parser::parse(&format!(
            "func constant args=0\nblock b0:\n  v0 = const {value}\n  ret v0\n"
        ))?;
        validate_function(&function)?;
        let registers = PhysRegSet::default_gp_with_scratch();
        let intervals = compute_live_intervals(&function)?;
        let allocator = allocators::get_allocator("linear-scan")?;
        let assignment = allocator.allocate(&intervals, &registers, 1)?;
        Ok(emit_function_i64(&function, &assignment, &registers)?.bytes)
    }

    #[test]
    fn large_result_does_not_block_the_child_process() -> Result<()> {
        let output = RunnerClient::default().run(&RunnerSpec::I64Cases {
            code: emit_constant(i64::MAX)?,
            args: vec![Vec::new(); 4096],
        })?;

        assert!(output.len() > 64 * 1024);
        let results = parse_results(&output)?;
        assert_eq!(results, vec![i64::MAX; 4096]);
        Ok(())
    }

    #[test]
    fn nonterminating_machine_code_is_killed_by_the_parent_timeout() {
        // `jmp $` loops forever. The child process must not survive the test.
        let result = RunnerClient::new(Duration::from_millis(250)).run(&RunnerSpec::I64Cases {
            code: vec![0xEB, 0xFE],
            args: vec![Vec::new()],
        });

        let error = result.expect_err("infinite loop must time out");
        assert!(error.to_string().contains("runner timed out"));
    }
}
