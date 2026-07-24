#![deny(clippy::undocumented_unsafe_blocks)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;

const MAX_CODE_SIZE: usize = 1024 * 1024;
const MAX_SPEC_SIZE: u64 = 8 * 1024 * 1024;
const MAX_CASES: usize = 4096;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    spec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Spec {
    I64Cases { code: Vec<u8>, args: Vec<Vec<i64>> },
    Sum8F32 { code: Vec<u8>, iters: u32 },
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct ExecutableMapping {
    ptr: *mut u8,
    size: usize,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl ExecutableMapping {
    fn new(code: &[u8]) -> Result<Self> {
        anyhow::ensure!(!code.is_empty(), "generated code is empty");
        anyhow::ensure!(code.len() <= MAX_CODE_SIZE, "generated code is too large");
        // SAFETY: `mmap` is called with a null hint and an anonymous private mapping.
        // The returned pointer is checked before it is used, and `size` is kept for `munmap`.
        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                code.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                bail!("mmap failed: {}", std::io::Error::last_os_error());
            }
            std::ptr::copy_nonoverlapping(code.as_ptr(), ptr.cast::<u8>(), code.len());
            if libc::mprotect(ptr, code.len(), libc::PROT_READ | libc::PROT_EXEC) != 0 {
                let error = std::io::Error::last_os_error();
                libc::munmap(ptr, code.len());
                bail!("mprotect failed: {error}");
            }
            Ok(Self {
                ptr: ptr.cast::<u8>(),
                size: code.len(),
            })
        }
    }

    fn ptr(&self) -> *mut u8 {
        self.ptr
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Drop for ExecutableMapping {
    fn drop(&mut self) {
        // SAFETY: `ptr` and `size` come from the successful `mmap` in `new`, and this
        // object owns the mapping, so it is unmapped exactly once here.
        unsafe {
            libc::munmap(self.ptr.cast(), self.size);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn apply_limits() -> Result<()> {
    // SAFETY: these calls only change limits and process attributes of the current runner
    // process. All return values are checked before execution continues.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            bail!(
                "PR_SET_NO_NEW_PRIVS failed: {}",
                std::io::Error::last_os_error()
            );
        }
        let cpu = libc::rlimit {
            rlim_cur: 2,
            rlim_max: 3,
        };
        if libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0 {
            bail!("RLIMIT_CPU failed: {}", std::io::Error::last_os_error());
        }
        let address_space = libc::rlimit {
            rlim_cur: 256 * 1024 * 1024,
            rlim_max: 256 * 1024 * 1024,
        };
        if libc::setrlimit(libc::RLIMIT_AS, &address_space) != 0 {
            bail!("RLIMIT_AS failed: {}", std::io::Error::last_os_error());
        }
        let files = libc::rlimit {
            rlim_cur: 16,
            rlim_max: 16,
        };
        if libc::setrlimit(libc::RLIMIT_NOFILE, &files) != 0 {
            bail!("RLIMIT_NOFILE failed: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

fn execute_i64(ptr: *mut u8, args: &[i64]) -> Result<i64> {
    // SAFETY: `ptr` points to an RX mapping that remains alive for the duration of the
    // call. The emitter and this dispatch agree on the SysV C ABI and support at most
    // six `i64` arguments. Invalid generated code is isolated in this child process.
    unsafe {
        Ok(match args {
            [] => std::mem::transmute::<*mut u8, extern "C" fn() -> i64>(ptr)(),
            [a] => std::mem::transmute::<*mut u8, extern "C" fn(i64) -> i64>(ptr)(*a),
            [a, b] => std::mem::transmute::<*mut u8, extern "C" fn(i64, i64) -> i64>(ptr)(*a, *b),
            [a, b, c] => {
                std::mem::transmute::<*mut u8, extern "C" fn(i64, i64, i64) -> i64>(ptr)(*a, *b, *c)
            }
            [a, b, c, d] => {
                std::mem::transmute::<*mut u8, extern "C" fn(i64, i64, i64, i64) -> i64>(ptr)(
                    *a, *b, *c, *d,
                )
            }
            [a, b, c, d, e] => std::mem::transmute::<
                *mut u8,
                extern "C" fn(i64, i64, i64, i64, i64) -> i64,
            >(ptr)(*a, *b, *c, *d, *e),
            [a, b, c, d, e, f] => std::mem::transmute::<
                *mut u8,
                extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64,
            >(ptr)(*a, *b, *c, *d, *e, *f),
            _ => bail!("runner supports at most 6 i64 arguments"),
        })
    }
}

fn main() -> Result<()> {
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    bail!("runner supports Linux x86-64 only");

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let args = Args::parse();
        apply_limits()?;

        let metadata = fs::metadata(&args.spec).context("read spec metadata")?;
        anyhow::ensure!(
            metadata.len() <= MAX_SPEC_SIZE,
            "runner spec is too large: {} bytes",
            metadata.len()
        );
        let data = fs::read(&args.spec).context("read spec")?;
        let spec: Spec = serde_json::from_slice(&data).context("parse spec JSON")?;

        match spec {
            Spec::I64Cases { code, args } => {
                anyhow::ensure!(args.len() <= MAX_CASES, "too many validation cases");
                let mapping = ExecutableMapping::new(&code)?;
                let mut results = Vec::with_capacity(args.len());
                for case in &args {
                    results.push(execute_i64(mapping.ptr(), case)?);
                }
                println!("RESULTS {}", serde_json::to_string(&results)?);
            }
            Spec::Sum8F32 { code, iters } => {
                let mapping = ExecutableMapping::new(&code)?;
                // SAFETY: the mapping is RX and remains alive while the function runs. The
                // SIMD emitter produces the exact `extern "C" fn(*const f32) -> f32` ABI.
                let function: extern "C" fn(*const f32) -> f32 =
                    unsafe { std::mem::transmute(mapping.ptr()) };
                let mut values = [0_f32; 8];
                for (index, value) in values.iter_mut().enumerate() {
                    *value = index as f32 + 0.25;
                }
                let start = Instant::now();
                let mut sink = 0_f32;
                for _ in 0..iters {
                    sink += function(values.as_ptr());
                }
                println!("SINK {sink}");
                println!("DURATION_NS {}", start.elapsed().as_nanos());
            }
        }
    }

    Ok(())
}
