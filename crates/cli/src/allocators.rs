use alloc::Allocator;

pub fn get_allocator(name: &str) -> Box<dyn Allocator> {
    match name {
        "linear-scan" | "ls" => Box::new(alloc_linear_scan::LinearScan),
        "sim-anneal" | "sa" => Box::new(alloc_sim_anneal::SimAnneal::default()),
        _ => Box::new(alloc_linear_scan::LinearScan),
    }
}
