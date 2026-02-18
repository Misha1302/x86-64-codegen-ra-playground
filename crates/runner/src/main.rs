use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;

#[derive(Parser)]
struct Args {
    /// JSON file containing code bytes + kind + inputs
    #[arg(long)]
    spec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Spec {
    I64_3args { code: Vec<u8>, a: i64, b: i64, c: i64 },
    I64_2args { code: Vec<u8>, a: i64, b: i64 },
    Sum8F32 { code: Vec<u8>, iters: u32 },
}

fn apply_limits_best_effort() {
    #[cfg(target_os = "linux")]
    unsafe {
        // no_new_privs
        libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
        // rlimit: 1s CPU, 256MB AS
        let cpu = libc::rlimit { rlim_cur: 1, rlim_max: 2 };
        libc::setrlimit(libc::RLIMIT_CPU, &cpu);
        let as_ = libc::rlimit { rlim_cur: 256 * 1024 * 1024, rlim_max: 512 * 1024 * 1024 };
        libc::setrlimit(libc::RLIMIT_AS, &as_);
    }
}

fn mmap_exec(code: &[u8]) -> Result<*mut u8> {
    #[cfg(not(target_os = "linux"))]
    {
        bail!("runner MVP supports linux mmap exec");
    }
    #[cfg(target_os = "linux")]
    unsafe {
        let size = code.len();
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if ptr == libc::MAP_FAILED {
            bail!("mmap failed");
        }
        std::ptr::copy_nonoverlapping(code.as_ptr(), ptr as *mut u8, size);
        // RX
        if libc::mprotect(ptr, size, libc::PROT_READ | libc::PROT_EXEC) != 0 {
            bail!("mprotect failed");
        }
        Ok(ptr as *mut u8)
    }
}

fn main() -> Result<()> {
    apply_limits_best_effort();

    let args = Args::parse();
    let data = fs::read(&args.spec).context("read spec")?;
    let spec: Spec = serde_json::from_slice(&data).context("parse spec json")?;

    match spec {
        Spec::I64_3args { code, a, b, c } => {
            let ptr = mmap_exec(&code)?;
            let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
            let r = f(a, b, c);
            println!("RESULT {r}");
        }
        Spec::I64_2args { code, a, b } => {
            let ptr = mmap_exec(&code)?;
            let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr) };
            let r = f(a, b);
            println!("RESULT {r}");
        }
        Spec::Sum8F32 { code, iters } => {
            let ptr = mmap_exec(&code)?;
            let f: extern "C" fn(*const f32) -> f32 = unsafe { std::mem::transmute(ptr) };
            let mut arr = [0f32; 8];
            for i in 0..8 { arr[i] = (i as f32) + 0.25; }
            let start = Instant::now();
            let mut sink = 0f32;
            for _ in 0..iters {
                sink += f(arr.as_ptr());
            }
            let dur = start.elapsed();
            println!("SINK {sink}");
            println!("DURATION_NS {}", dur.as_nanos());
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
mod libc {
    pub use ::libc::*;
}
