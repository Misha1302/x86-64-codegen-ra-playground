use alloc::Allocator;
use anyhow::Result;

pub fn get_allocator(name: &str) -> Result<Box<dyn Allocator>> {
    match name {
        "linear-scan" | "ls" => Ok(Box::new(alloc_linear_scan::LinearScan)),
        "sim-anneal" | "sa" => Ok(Box::new(alloc_sim_anneal::SimAnneal::default())),
        _ => anyhow::bail!("unknown allocator '{name}'; expected linear-scan or sim-anneal"),
    }
}
