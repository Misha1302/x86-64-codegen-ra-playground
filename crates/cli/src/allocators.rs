use alloc::Allocator;

pub fn get_allocator(name: &str) -> Box<dyn Allocator> {
    match name {
        "linear-scan" | "ls" => Box::new(alloc_linear_scan::LinearScan),
        _ => Box::new(alloc_linear_scan::LinearScan),
    }
}
