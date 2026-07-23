use anyhow::Result;
use indexmap::IndexMap;

use alloc::{verify_assignment, Allocator, Assignment, Location, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals};

pub struct LinearScan;

impl LinearScan {
    fn expire_old(
        active: &mut Vec<(LiveInterval, alloc::PhysReg)>,
        current: &LiveInterval,
        free: &mut Vec<alloc::PhysReg>,
    ) {
        active.sort_by_key(|(interval, _)| interval.end);
        let mut index = 0;
        while index < active.len() {
            if active[index].0.end < current.start {
                let (_, register) = active.remove(index);
                free.push(register);
            } else {
                index += 1;
            }
        }
    }
}

impl Allocator for LinearScan {
    fn name(&self) -> &'static str {
        "linear-scan"
    }

    fn allocate(
        &self,
        intervals: &LiveIntervals,
        regs: &PhysRegSet,
        max_regs: usize,
    ) -> Result<Assignment> {
        let register_limit = max_regs.min(regs.regs.len());
        let mut free: Vec<alloc::PhysReg> = regs
            .regs
            .iter()
            .copied()
            .take(register_limit)
            .collect();
        let mut active: Vec<(LiveInterval, alloc::PhysReg)> = Vec::new();
        let mut map: IndexMap<ir::VReg, Location> = IndexMap::new();
        let mut stack_slots = 0_u32;

        let mut ordered = intervals.intervals.clone();
        ordered.sort_by_key(|interval| (interval.start, interval.end, interval.v.0));

        for interval in &ordered {
            Self::expire_old(&mut active, interval, &mut free);

            if let Some(register) = free.pop() {
                active.push((interval.clone(), register));
                map.insert(interval.v, Location::Reg(register));
                continue;
            }

            if active.is_empty() {
                let slot = StackSlot { index: stack_slots };
                stack_slots += 1;
                map.insert(interval.v, Location::Stack(slot));
                continue;
            }

            active.sort_by_key(|(active_interval, _)| active_interval.end);
            let spill_index = active.len() - 1;
            if active[spill_index].0.end > interval.end {
                let (spilled, register) = active.remove(spill_index);
                let slot = StackSlot { index: stack_slots };
                stack_slots += 1;
                map.insert(spilled.v, Location::Stack(slot));
                active.push((interval.clone(), register));
                map.insert(interval.v, Location::Reg(register));
            } else {
                let slot = StackSlot { index: stack_slots };
                stack_slots += 1;
                map.insert(interval.v, Location::Stack(slot));
            }
        }

        let assignment = Assignment {
            spills: stack_slots,
            stack_slots,
            map,
        };
        verify_assignment(intervals, regs, register_limit, &assignment)?;
        Ok(assignment)
    }
}
