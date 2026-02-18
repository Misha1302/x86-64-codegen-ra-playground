use std::time::{Duration, Instant};

use anyhow::Result;
use indexmap::IndexMap;

use alloc::{Allocator, Assignment, Location, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals};

/// Simulated annealing allocator for pedagogical experiments.
///
/// State: assignment of each interval either to one physical register or spill.
/// Cost:
/// - very high penalty for conflicts (overlapping intervals sharing register),
/// - spill penalty proportional to live-range length,
/// - tiny register preference term to stabilize output.
pub struct SimAnneal {
    pub seed: u64,
    pub time_limit: Duration,
    pub start_temp: f64,
    pub end_temp: f64,
}

impl Default for SimAnneal {
    fn default() -> Self {
        Self {
            seed: 0xC0DEC0DE,
            time_limit: Duration::from_secs(3),
            start_temp: 4.0,
            end_temp: 0.02,
        }
    }
}

#[derive(Clone)]
struct Problem {
    intervals: Vec<LiveInterval>,
    overlaps: Vec<Vec<usize>>,
    reg_count: usize,
}

impl Problem {
    fn from_live_intervals(intervals: &LiveIntervals, reg_count: usize) -> Self {
        let data = intervals.intervals.clone();
        let mut overlaps = vec![Vec::new(); data.len()];

        for i in 0..data.len() {
            for j in (i + 1)..data.len() {
                if Self::overlap(&data[i], &data[j]) {
                    overlaps[i].push(j);
                    overlaps[j].push(i);
                }
            }
        }

        Self {
            intervals: data,
            overlaps,
            reg_count,
        }
    }

    fn overlap(a: &LiveInterval, b: &LiveInterval) -> bool {
        !(a.end < b.start || b.end < a.start)
    }

    fn cost(&self, state: &[usize]) -> i64 {
        let mut cost = 0_i64;

        for (i, &choice) in state.iter().enumerate() {
            if choice == self.reg_count {
                let len = (self.intervals[i].end.0 - self.intervals[i].start.0 + 1) as i64;
                cost += 100 + len;
            } else {
                cost += choice as i64;
            }
        }

        for i in 0..state.len() {
            for &j in &self.overlaps[i] {
                if j > i && state[i] != self.reg_count && state[i] == state[j] {
                    cost += 20_000;
                }
            }
        }

        cost
    }

    fn greedy_initial_state(&self) -> Vec<usize> {
        let mut state = vec![self.reg_count; self.intervals.len()];

        for i in 0..self.intervals.len() {
            let mut used = vec![false; self.reg_count];
            for &j in &self.overlaps[i] {
                let c = state[j];
                if c < self.reg_count {
                    used[c] = true;
                }
            }

            if let Some((r, _)) = used.iter().enumerate().find(|(_, v)| !**v) {
                state[i] = r;
            }
        }

        state
    }
}

#[derive(Clone)]
struct Lcg {
    s: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            s: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.s = self
            .s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.s
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        (self.next_u64() as usize) % upper
    }

    fn next_f64(&mut self) -> f64 {
        const DEN: f64 = (1_u64 << 53) as f64;
        ((self.next_u64() >> 11) as f64) / DEN
    }
}

impl SimAnneal {
    fn anneal(&self, problem: &Problem) -> Vec<usize> {
        let mut rng = Lcg::new(self.seed ^ (problem.intervals.len() as u64).wrapping_mul(17));

        let mut cur = problem.greedy_initial_state();
        let mut cur_cost = problem.cost(&cur);

        let mut best = cur.clone();
        let mut best_cost = cur_cost;

        let choices = problem.reg_count + 1; // + spill
        let limit = self.time_limit.max(Duration::from_millis(1));
        let start = Instant::now();
        let deadline = start + limit;

        let mut step: u64 = 0;
        while Instant::now() < deadline {
            step = step.saturating_add(1);
            let elapsed = start.elapsed().as_secs_f64();
            let progress = (elapsed / limit.as_secs_f64()).clamp(0.0, 1.0);
            let t = self.start_temp * (self.end_temp / self.start_temp).powf(progress);

            let idx = rng.next_usize(cur.len());
            let mut next = cur.clone();

            let mut candidate = rng.next_usize(choices);
            if candidate == next[idx] {
                candidate = (candidate + 1) % choices;
            }
            next[idx] = candidate;

            let next_cost = problem.cost(&next);
            let delta = next_cost - cur_cost;
            let accept = if delta <= 0 {
                true
            } else {
                let p = (-(delta as f64) / t.max(1e-9)).exp();
                rng.next_f64() < p
            };

            if accept {
                cur = next;
                cur_cost = next_cost;
                if cur_cost < best_cost {
                    best = cur.clone();
                    best_cost = cur_cost;
                }
            }
        }

        if step == 0 {
            cur
        } else {
            best
        }
    }
}

impl Allocator for SimAnneal {
    fn name(&self) -> &'static str {
        "sim-anneal"
    }

    fn allocate(
        &self,
        intervals: &LiveIntervals,
        regs: &PhysRegSet,
        max_regs: usize,
    ) -> Result<Assignment> {
        if intervals.intervals.is_empty() {
            return Ok(Assignment {
                map: IndexMap::new(),
                stack_slots: 0,
                spills: 0,
            });
        }

        let reg_count = max_regs.min(regs.regs.len());
        if reg_count == 0 {
            let mut map = IndexMap::new();
            for (idx, it) in intervals.intervals.iter().enumerate() {
                map.insert(it.v, Location::Stack(StackSlot { index: idx as u32 }));
            }
            return Ok(Assignment {
                map,
                stack_slots: intervals.intervals.len() as u32,
                spills: intervals.intervals.len() as u32,
            });
        }

        let problem = Problem::from_live_intervals(intervals, reg_count);
        let best = self.anneal(&problem);

        let mut map = IndexMap::new();
        let mut stack_slots = 0_u32;
        let mut spills = 0_u32;

        for (idx, it) in problem.intervals.iter().enumerate() {
            let choice = best[idx];
            if choice < reg_count {
                map.insert(it.v, Location::Reg(regs.regs[choice]));
            } else {
                let slot = StackSlot { index: stack_slots };
                stack_slots += 1;
                spills += 1;
                map.insert(it.v, Location::Stack(slot));
            }
        }

        Ok(Assignment {
            map,
            stack_slots,
            spills,
        })
    }
}
