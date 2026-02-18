use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use alloc::PhysRegSet;
use analysis::{build_interference_graph, compute_live_intervals};
use codegen::{disasm, emit_function_i64, simd};
use ir::{examples, interp::Interpreter, Function};

mod allocators;

#[derive(Parser)]
#[command(name = "playground")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compile + run IR example with allocator, print report
    Run {
        #[arg(long, default_value = "basicblock")]
        example: String,
        #[arg(long, default_value = "linear-scan")]
        alloc: String,
        /// How many allocatable regs (subset of PhysRegSet::regs)
        #[arg(long, default_value_t = 4)]
        regs: usize,
        #[arg(long, default_value_t = true)]
        validate: bool,
        #[arg(long, default_value_t = false)]
        dump_disasm: bool,
    },
    /// SIMD micro-benchmark (scalar vs SIMD)
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
    Sse2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum RunnerSpec {
    I64_3args { code: Vec<u8>, a: i64, b: i64, c: i64 },
    I64_2args { code: Vec<u8>, a: i64, b: i64 },
    Sum8F32 { code: Vec<u8>, iters: u32 },
}

fn example_by_name(name: &str) -> Result<Function> {
    match name {
        "basicblock" => examples::basicblock(),
        "trace" => examples::trace(),
        _ => examples::basicblock(),
    }
}

fn run_runner(spec: &RunnerSpec) -> Result<String> {
    let tmp = tempfile::NamedTempFile::new().context("tmpfile")?;
    std::fs::write(tmp.path(), serde_json::to_vec(spec)?)?;

    let runner = find_or_build_runner_exe().context("locate/build runner")?;
    let out = Command::new(runner)
        .arg("--spec")
        .arg(tmp.path())
        .output()
        .context("spawn runner")?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        anyhow::bail!("runner failed: {}\n{}", stdout, stderr);
    }
    Ok(stdout)
}

/// 1) Try to find runner next to current cli executable (target/debug/runner).
/// 2) If missing, run `cargo build -p runner` at workspace root.
/// 3) Try again.
fn find_or_build_runner_exe() -> Result<PathBuf> {
    if let Some(p) = find_runner_exe_next_to_cli()? {
        return Ok(p);
    }

    build_runner().context("cargo build -p runner")?;

    if let Some(p) = find_runner_exe_next_to_cli()? {
        return Ok(p);
    }

    // As a fallback, also try conventional target paths from workspace root
    let ws = workspace_root();
    let exe = runner_exe_name();
    let candidate_debug = ws.join("target").join("debug").join(exe);
    let candidate_release = ws.join("target").join("release").join(exe);

    if candidate_debug.exists() {
        return Ok(candidate_debug);
    }
    if candidate_release.exists() {
        return Ok(candidate_release);
    }

    anyhow::bail!(
        "runner executable not found after build. Tried next-to-cli and {}/target/{{debug,release}}",
        ws.display()
    );
}

fn find_runner_exe_next_to_cli() -> Result<Option<PathBuf>> {
    let me = std::env::current_exe().context("current_exe")?;
    let dir = me.parent().context("exe parent")?;
    let exe = runner_exe_name();
    let p = dir.join(exe);
    if p.exists() {
        Ok(Some(p))
    } else {
        Ok(None)
    }
}

fn runner_exe_name() -> &'static str {
    if cfg!(windows) { "runner.exe" } else { "runner" }
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to crates/cli at compile time.
    // Workspace root is ../../ from there.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap() // crates
        .parent().unwrap()  // workspace root
        .to_path_buf()
}

fn build_runner() -> Result<()> {
    let ws = workspace_root();
    let status = Command::new("cargo")
        .args(["build", "-p", "runner"])
        .current_dir(ws)
        .status()
        .context("spawn cargo build")?;
    if !status.success() {
        anyhow::bail!("cargo build -p runner failed with status {}", status);
    }
    Ok(())
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
            let f = example_by_name(&example)?;
            let interpreter = Interpreter::default();
            let regs_set = PhysRegSet::default_gp_with_scratch();

            let li = compute_live_intervals(&f)?;
            let allocator = allocators::get_allocator(&alloc);
            let asg = allocator.allocate(&li, &regs_set, regs)?;

            let emitted = emit_function_i64(&f, &asg, &regs_set)?;
            let graph = build_interference_graph(&f)?;

            println!("== Example: {} ==", example);
            println!("Allocator: {} (regs={})", allocator.name(), regs);
            println!("Stack slots: {}  Spills: {}", asg.stack_slots, asg.spills);
            println!(
                "Code size: {} bytes  loads={} stores={}",
                emitted.metrics.code_size, emitted.metrics.loads, emitted.metrics.stores
            );
            println!("Interference nodes: {}", graph.nodes.len());

            println!("\n-- Assignment --");
            for (v, loc) in asg.map.iter() {
                println!("{:?} -> {:?}", v, loc);
            }

            if dump_disasm {
                println!("\n-- Disasm --\n{}", disasm(&emitted.bytes, 0));
            }

            if validate {
                if f.args == 3 {
                    let args = [2i64, 3i64, 4i64];
                    let ref_r = interpreter.eval_i64(&f, &args)?;
                    let out = run_runner(&RunnerSpec::I64_3args {
                        code: emitted.bytes.clone(),
                        a: args[0],
                        b: args[1],
                        c: args[2],
                    })?;
                    let got = parse_result(&out)?;
                    println!("\n-- Validation --\nref={ref_r} got={got}");
                    anyhow::ensure!(ref_r == got, "mismatch");
                } else if f.args == 2 {
                    let args = [10i64, 7i64];
                    let ref_r = interpreter.eval_i64(&f, &args)?;
                    let out = run_runner(&RunnerSpec::I64_2args {
                        code: emitted.bytes.clone(),
                        a: args[0],
                        b: args[1],
                    })?;
                    let got = parse_result(&out)?;
                    println!("\n-- Validation --\nref={ref_r} got={got}");
                    anyhow::ensure!(ref_r == got, "mismatch");
                }
            }
        }
        Cmd::SimdBench { target, iters } => {
            let scalar = simd::emit_sum8_scalar()?;
            let sse = simd::emit_sum8_sse()?;

            let do_sse = match target {
                SimdTarget::Scalar => false,
                SimdTarget::Sse2 => true,
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

            println!("SIMD target: {:?} (iters={})", target, iters);

            let out_scalar = run_runner(&RunnerSpec::Sum8F32 {
                code: scalar.clone(),
                iters,
            })?;
            let ns_scalar = parse_duration_ns(&out_scalar)?;
            println!("scalar:  {} ns", ns_scalar);

            if do_sse {
                let out_sse = run_runner(&RunnerSpec::Sum8F32 {
                    code: sse.clone(),
                    iters,
                })?;
                let ns_sse = parse_duration_ns(&out_sse)?;
                println!("sse:     {} ns", ns_sse);
                println!("speedup: {:.2}x", (ns_scalar as f64) / (ns_sse as f64));
                println!("\n-- Disasm (scalar) --\n{}", disasm(&scalar, 0));
                println!("\n-- Disasm (sse) --\n{}", disasm(&sse, 0));
            } else {
                println!("SSE not available or disabled; only scalar ran.");
            }
        }
    }
    Ok(())
}

fn parse_result(stdout: &str) -> Result<i64> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("RESULT ") {
            return Ok(rest.trim().parse()?);
        }
    }
    anyhow::bail!("no RESULT line in runner output: {}", stdout);
}

fn parse_duration_ns(stdout: &str) -> Result<u128> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("DURATION_NS ") {
            return Ok(rest.trim().parse()?);
        }
    }
    anyhow::bail!("no DURATION_NS line in runner output: {}", stdout);
}
