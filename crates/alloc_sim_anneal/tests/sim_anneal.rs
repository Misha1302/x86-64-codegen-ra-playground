use std::time::Duration;

use alloc::{Allocator, Location, PhysRegSet};
use alloc_sim_anneal::SimAnneal;
use analysis::compute_live_intervals;
use anyhow::Result;
use ir::examples;

fn intervals_overlap(a: &analysis::LiveInterval, b: &analysis::LiveInterval) -> bool {
    !(a.end < b.start || b.end < a.start)
}

#[test]
fn assigns_location_for_each_interval() -> Result<()> {
    let f = examples::trace()?;
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;

    let allocator = SimAnneal {
        time_limit: Duration::from_millis(50),
        ..SimAnneal::default()
    };
    let asg = allocator.allocate(&li, &regs, 4)?;

    assert_eq!(asg.map.len(), li.intervals.len());
    Ok(())
}

#[test]
fn no_register_conflicts_on_overlapping_intervals() -> Result<()> {
    let f = examples::basicblock()?;
    let regs = PhysRegSet::default_gp_with_scratch();
    let li = compute_live_intervals(&f)?;

    let asg = SimAnneal {
        time_limit: Duration::from_millis(50),
        ..SimAnneal::default()
    }
    .allocate(&li, &regs, 4)?;

    for i in 0..li.intervals.len() {
        for j in (i + 1)..li.intervals.len() {
            if !intervals_overlap(&li.intervals[i], &li.intervals[j]) {
                continue;
            }

            let li_loc = asg
                .map
                .get(&li.intervals[i].v)
                .expect("all intervals assigned");
            let lj_loc = asg
                .map
                .get(&li.intervals[j].v)
                .expect("all intervals assigned");
            if let (Location::Reg(r1), Location::Reg(r2)) = (li_loc, lj_loc) {
                assert_ne!(r1, r2, "overlapping intervals must not share a register");
            }
        }
    }

    Ok(())
}
