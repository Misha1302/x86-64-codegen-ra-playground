use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use alloc::PhysRegSet;
use analysis::{build_interference_graph, compute_live_intervals, validate_function};
use codegen::{disasm, emit_function_i64, simd};
use ir::{examples, interp::Interpreter, Function};

mod allocators;

const RUNNER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Parser)]
#[command(name = "playground")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compile and run an IR example, then compare native results with the interpreter.
    Run {
        #[arg(long, default_value = "basicblock")]
        example: String,
        #[arg(long, default_value = "linear-scan")]
        alloc: String,
        /// Number of allocatable registers. Zero intentionally forces all values to the stack.
        #[arg(long, default_value_t = 4)]
        regs: usize,
        #[arg(long, default_value_t = true)]
        validate: bool,
        #[arg(long, default_value_t = false)]
        dump_disasm: bool,
    },
    /// Scalar versus SSE micro-benchmark.
    SimdBench {
        #[arg(long, value_enum, default_value_t = SimdTarget::Auto)]
        target: SimdTarget,
        #[arg(long, default_value_t = 2_000_000)]
        iters: u32,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum SimdTarget {
    Auto,
    Scalar,
    Sse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum RunnerSpec {
    I64Cases { code: Vec<u8>, args: Vec<Vec<i64>> },
    Sum8F32 { code: Vec<u8>, iters: u32 },
}

fn example_by_name(name: &str) -> Result<Function> {
    match name {
        "basicblock" => examples::basicblock(),
        "trace" => examples::trace(),
        "loop-sum" | "loop_sum" => examples::loop_sum(),
        "phi-swap-loop" | "phi_swap_loop" => examples::phi_swap_loop(),
        _ => anyhow::bail!(
            "unknown example '{name}'; expected basicblock, trace, loop-sum, or phi-swap-loop"
        ),
    }
}

fn drain_pipe<R>(mut reader: R) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn join_pipe(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream_name: &str,
) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("runner {stream_name} reader panicked"))?
        .with_context(|| format!("read runner {stream_name}"))
}

fn run_runner(spec: &RunnerSpec) -> Result<String> {
    let temp = tempfile::NamedTempFile::new().context("create runner spec")?;
    std::fs::write(temp.path(), serde_json::to_vec(spec)?).context("write runner spec")?;

    let runner = find_or_build_runner_exe().context("locate or build runner")?;
    let mut child = Command::new(runner)
        .arg("--spec")
        .arg(temp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn runner")?;
    let stdout_reader = drain_pipe(child.stdout.take().context("capture runner stdout")?);
    let stderr_reader = drain_pipe(child.stderr.take().context("capture runner stderr")?);

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= RUNNER_TIMEOUT {
            child.kill().context("kill timed-out runner")?;
            let _ = child.wait();
            let stdout = join_pipe(stdout_reader, "stdout")?;
            let stderr = join_pipe(stderr_reader, "stderr")?;
            anyhow::bail!(
                "runner timed out after {:?}:\n{}\n{}",
                RUNNER_TIMEOUT,
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
        thread::sleep(Duration::from_millis(5));
    };

    let stdout = join_pipe(stdout_reader, "stdout")?;
    let stderr = join_pipe(stderr_reader, "stderr")?;
    let stdout = String::from_utf8_lossy(&stdout).to_string();
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    anyhow::ensure!(status.success(), "runner failed:\n{stdout}\n{stderr}");
    Ok(stdout)
}

fn find_or_build_runner_exe() -> Result<PathBuf> {
    if let Some(path) = find_runner_exe_next_to_cli()? {
        return Ok(path);
    }

    build_runner()?;
    if let Some(path) = find_runner_exe_next_to_cli()? {
        return Ok(path);
    }

    let workspace = workspace_root();
    let executable = runner_exe_name();
    for profile in ["debug", "release"] {
        let candidate = workspace.join("target").join(profile).join(executable);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("runner executable not found after build")
}

fn find_runner_exe_next_to_cli() -> Result<Option<PathBuf>> {
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

fn lcg_next(state: &mut u64) -> i64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state as i64
}

fn validation_cases(function: &Function) -> Vec<Vec<i64>> {
    match function.name.as_str() {
        "basicblock" => {
            let mut cases = vec![
                vec![0, 0, 0],
                vec![2, 3, 4],
                vec![-1, 1, -1],
                vec![i64::MAX, 1, 2],
                vec![i64::MIN, -1, 3],
            ];
            let mut state = 0xA11C_E5E5_1234_5678;
            for _ in 0..32 {
                cases.push(vec![
                    lcg_next(&mut state),
                    lcg_next(&mut state),
                    lcg_next(&mut state),
                ]);
            }
            cases
        }
        "trace" => {
            let values = [i64::MIN, -100, -1, 0, 1, 100, i64::MAX];
            let mut cases = Vec::new();
            for left in values {
                for right in values {
                    cases.push(vec![left, right]);
                }
            }
            cases
        }
        "loop_sum" => (-3..=40).map(|value| vec![value]).collect(),
        "phi_swap_loop" => {
            let mut cases = Vec::new();
            for iterations in 0..=16 {
                cases.push(vec![11, 29, iterations]);
                cases.push(vec![-7, 5, iterations]);
            }
            cases
        }
        _ => vec![vec![0; function.args as usize]],
    }
}

fn parse_results(stdout: &str) -> Result<Vec<i64>> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("RESULTS ") {
            return Ok(serde_json::from_str(rest.trim())?);
        }
    }
    anyhow::bail!("runner output contains no RESULTS line: {stdout}")
}

fn parse_duration_ns(stdout: &str) -> Result<u128> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("DURATION_NS ") {
            return Ok(rest.trim().parse()?);
        }
    }
    anyhow::bail!("runner output contains no DURATION_NS line: {stdout}")
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run {
            example,
            alloc,
            regs,
            validate,
            dump_disasm,
        } => {
            let function = example_by_name(&example)?;
            validate_function(&function)?;
            let register_set = PhysRegSet::default_gp_with_scratch();
            register_set.validate_for_codegen()?;
            anyhow::ensure!(
                regs <= register_set.regs.len(),
                "--regs must be between 0 and {}",
                register_set.regs.len()
            );

            let intervals = compute_live_intervals(&function)?;
            let allocator = allocators::get_allocator(&alloc)?;
            let assignment = allocator.allocate(&intervals, &register_set, regs)?;
            let emitted = emit_function_i64(&function, &assignment, &register_set)?;
            let graph = build_interference_graph(&function)?;

            println!("== Example: {example} ==");
            println!("Allocator: {} (regs={regs})", allocator.name());
            println!(
                "Stack slots: {}  Spills: {}",
                assignment.stack_slots, assignment.spills
            );
            println!(
                "Code size: {} bytes  loads={} stores={}",
                emitted.metrics.code_size, emitted.metrics.loads, emitted.metrics.stores
            );
            println!("Interference nodes: {}", graph.nodes.len());
            println!("\n-- Assignment --");
            for (value, location) in &assignment.map {
                println!("{value:?} -> {location:?}");
            }

            if dump_disasm {
                println!("\n-- Disasm --\n{}", disasm(&emitted.bytes, 0));
            }

            if validate {
                let cases = validation_cases(&function);
                let interpreter = Interpreter;
                let expected = cases
                    .iter()
                    .map(|args| interpreter.eval_i64(&function, args))
                    .collect::<Result<Vec<_>>>()?;
                let output = run_runner(&RunnerSpec::I64Cases {
                    code: emitted.bytes.clone(),
                    args: cases,
                })?;
                let actual = parse_results(&output)?;
                anyhow::ensure!(
                    expected == actual,
                    "native/interpreter mismatch: expected {expected:?}, got {actual:?}"
                );
                println!(
                    "\n-- Validation --\n{} differential cases passed",
                    expected.len()
                );
            }
        }
        Cmd::SimdBench { target, iters } => {
            let scalar = simd::emit_sum8_scalar()?;
            let sse = simd::emit_sum8_sse()?;
            let use_sse = match target {
                SimdTarget::Scalar => false,
                SimdTarget::Sse => true,
                SimdTarget::Auto => {
                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    {
                        std::is_x86_feature_detected!("sse")
                    }
                    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                    {
                        false
                    }
                }
            };

            println!("SIMD target: {target:?} (iters={iters})");
            let scalar_output = run_runner(&RunnerSpec::Sum8F32 {
                code: scalar.clone(),
                iters,
            })?;
            let scalar_ns = parse_duration_ns(&scalar_output)?;
            println!("scalar: {scalar_ns} ns");

            if use_sse {
                let sse_output = run_runner(&RunnerSpec::Sum8F32 {
                    code: sse.clone(),
                    iters,
                })?;
                let sse_ns = parse_duration_ns(&sse_output)?;
                println!("sse: {sse_ns} ns");
                println!("speedup: {:.2}x", scalar_ns as f64 / sse_ns as f64);
                println!("\n-- Disasm (scalar) --\n{}", disasm(&scalar, 0));
                println!("\n-- Disasm (sse) --\n{}", disasm(&sse, 0));
            } else {
                println!("SSE is unavailable or disabled; only scalar code ran.");
            }
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;

    #[test]
    fn runner_drains_output_larger_than_a_pipe_buffer() -> Result<()> {
        let function = ir::parser::parse(
            r#"
            func large_output args=0
            block b0:
              v0 = const 9223372036854775807
              ret v0
            "#,
        )?;
        validate_function(&function)?;
        let register_set = PhysRegSet::default_gp_with_scratch();
        let intervals = compute_live_intervals(&function)?;
        let allocator = allocators::get_allocator("linear-scan")?;
        let assignment = allocator.allocate(&intervals, &register_set, 1)?;
        let emitted = emit_function_i64(&function, &assignment, &register_set)?;
        let output = run_runner(&RunnerSpec::I64Cases {
            code: emitted.bytes,
            args: vec![Vec::new(); 4096],
        })?;
        assert!(
            output.len() > 64 * 1024,
            "test must exceed a typical pipe buffer, got {} bytes",
            output.len()
        );
        let results = parse_results(&output)?;
        assert_eq!(results.len(), 4096);
        assert!(results.iter().all(|value| *value == i64::MAX));
        Ok(())
    }
}
