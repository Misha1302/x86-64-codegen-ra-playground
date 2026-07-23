use alloc::{verify_assignment, Allocator, PhysRegSet};
use alloc_sim_anneal::SimAnneal;
use analysis::compute_live_intervals;
use anyhow::Result;
use ir::examples;

#[test]
fn produces_valid_assignments_across_seeds_and_pressure() -> Result<()> {
    let regs = PhysRegSet::default_gp_with_scratch();
    let functions = [
        examples::basicblock()?,
        examples::trace()?,
        examples::loop_sum()?,
        examples::phi_swap_loop()?,
    ];

    for function in functions {
        let intervals = compute_live_intervals(&function)?;
        for seed in 0..16 {
            for register_count in 0..=regs.regs.len() {
                let allocator = SimAnneal {
                    seed,
                    iterations: 500,
                    ..SimAnneal::default()
                };
                let assignment = allocator.allocate(&intervals, &regs, register_count)?;
                verify_assignment(&intervals, &regs, register_count, &assignment)?;
            }
        }
    }
    Ok(())
}

#[test]
fn is_deterministic_for_fixed_seed_and_iterations() -> Result<()> {
    let regs = PhysRegSet::default_gp_with_scratch();
    let intervals = compute_live_intervals(&examples::basicblock()?)?;
    let allocator = SimAnneal {
        seed: 42,
        iterations: 2_000,
        ..SimAnneal::default()
    };
    let first = allocator.allocate(&intervals, &regs, 3)?;
    let second = allocator.allocate(&intervals, &regs, 3)?;
    assert_eq!(first, second);
    Ok(())
}
