use anyhow::Result;
use indexmap::IndexMap;

use alloc::{Allocator, Assignment, Location, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals};

pub struct LinearScan;

impl LinearScan {
    fn expire_old(active: &mut Vec<(LiveInterval, alloc::PhysReg)>, cur: &LiveInterval, free: &mut Vec<alloc::PhysReg>) {
        active.sort_by_key(|(i, _)| i.end);
        let mut i = 0;
        while i < active.len() {
            if active[i].0.end < cur.start {
                let (_, r) = active.remove(i);
                free.push(r);
            } else {
                i += 1;
            }
        }
    }
}

impl Allocator for LinearScan {
    fn name(&self) -> &'static str { "linear-scan" }

    fn allocate(&self, intervals: &LiveIntervals, regs: &PhysRegSet, max_regs: usize) -> Result<Assignment> {
        let mut free: Vec<alloc::PhysReg> = regs.regs.iter().copied().take(max_regs).collect();
        let mut active: Vec<(LiveInterval, alloc::PhysReg)> = Vec::new();
        let mut map: IndexMap<ir::VReg, Location> = IndexMap::new();

        let mut stack_slots: u32 = 0;
        let mut spills: u32 = 0;

        for it in &intervals.intervals {
            Self::expire_old(&mut active, it, &mut free);

            if let Some(r) = free.pop() {
                active.push((it.clone(), r));
                map.insert(it.v, Location::Reg(r));
            } else {
                // Spill heuristic: spill the one with farthest end
                active.sort_by_key(|(i, _)| i.end);
                let last_idx = active.len() - 1;
                if active[last_idx].0.end > it.end {
                    // Spill active[last]
                    let (spilled_it, spilled_reg) = active.remove(last_idx);
                    let slot = StackSlot { index: stack_slots };
                    stack_slots += 1;
                    spills += 1;
                    map.insert(spilled_it.v, Location::Stack(slot));

                    // Assign its reg to current
                    active.push((it.clone(), spilled_reg));
                    map.insert(it.v, Location::Reg(spilled_reg));
                } else {
                    // Spill current
                    let slot = StackSlot { index: stack_slots };
                    stack_slots += 1;
                    spills += 1;
                    map.insert(it.v, Location::Stack(slot));
                }
            }
        }

        Ok(Assignment { map, stack_slots, spills })
    }
}
