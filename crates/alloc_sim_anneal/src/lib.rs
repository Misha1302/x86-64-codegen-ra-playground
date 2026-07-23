use anyhow::Result;
use indexmap::IndexMap;

use alloc::{verify_assignment, Allocator, Assignment, Location, PhysRegSet, StackSlot};
use analysis::{LiveInterval, LiveIntervals};

pub struct SimAnneal {
    pub seed: u64,
    pub iterations: u64,
    pub start_temp: f64,
    pub end_temp: f64,
}

impl Default for SimAnneal {
    fn default() -> Self {
        Self {
            seed: 0xC0DE_C0DE,
            iterations: 10_000,
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
    fn new(intervals: &LiveIntervals, reg_count: usize) -> Self {
        let data = intervals.intervals.clone();
        let mut overlaps = vec![Vec::new(); data.len()];
        for left in 0..data.len() {
            for right in (left + 1)..data.len() {
                if data[left].overlaps(&data[right]) {
                    overlaps[left].push(right);
                    overlaps[right].push(left);
                }
            }
        }
        Self {
            intervals: data,
            overlaps,
            reg_count,
        }
    }

    fn cost(&self, state: &[usize]) -> i64 {
        let mut cost = 0_i64;
        for (index, choice) in state.iter().copied().enumerate() {
            if choice == self.reg_count {
                let interval = &self.intervals[index];
                let length = i64::from(interval.end.0 - interval.start.0 + 1);
                cost += 100 + length;
            } else {
                cost += choice as i64;
            }
        }

        for left in 0..state.len() {
            for right in &self.overlaps[left] {
                if *right > left
                    && state[left] < self.reg_count
                    && state[left] == state[*right]
                {
                    cost += 1_000_000;
                }
            }
        }
        cost
    }

    fn greedy_state(&self) -> Vec<usize> {
        let mut state = vec![self.reg_count; self.intervals.len()];
        for index in 0..self.intervals.len() {
            let mut used = vec![false; self.reg_count];
            for neighbor in &self.overlaps[index] {
                if *neighbor >= index {
                    continue;
                }
                let choice = state[*neighbor];
                if choice < self.reg_count {
                    used[choice] = true;
                }
            }
            if let Some(register) = used.iter().position(|used| !*used) {
                state[index] = register;
            }
        }
        state
    }

    fn repair(&self, preferred: &[usize]) -> Vec<usize> {
        let mut repaired = vec![self.reg_count; self.intervals.len()];
        for index in 0..self.intervals.len() {
            let mut used = vec![false; self.reg_count];
            for neighbor in &self.overlaps[index] {
                if *neighbor >= index {
                    continue;
                }
                let choice = repaired[*neighbor];
                if choice < self.reg_count {
                    used[choice] = true;
                }
            }

            let preferred_register = preferred[index];
            if preferred_register < self.reg_count && !used[preferred_register] {
                repaired[index] = preferred_register;
            } else if let Some(register) = used.iter().position(|used| !*used) {
                repaired[index] = register;
            }
        }
        repaired
    }
}

#[derive(Clone)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        (self.next_u64() as usize) % upper
    }

    fn next_f64(&mut self) -> f64 {
        const DENOMINATOR: f64 = (1_u64 << 53) as f64;
        ((self.next_u64() >> 11) as f64) / DENOMINATOR
    }
}

impl SimAnneal {
    fn anneal(&self, problem: &Problem) -> Vec<usize> {
        let mut current = problem.greedy_state();
        if current.is_empty() || problem.reg_count == 0 || self.iterations == 0 {
            return current;
        }

        let mut rng = Lcg::new(self.seed ^ current.len() as u64);
        let mut current_cost = problem.cost(&current);
        let mut best = current.clone();
        let mut best_cost = current_cost;
        let choices = problem.reg_count + 1;

        for step in 0..self.iterations {
            let progress = if self.iterations <= 1 {
                1.0
            } else {
                step as f64 / (self.iterations - 1) as f64
            };
            let temperature = self.start_temp
                * (self.end_temp / self.start_temp.max(f64::MIN_POSITIVE)).powf(progress);
            let index = rng.next_usize(current.len());
            let old_choice = current[index];
            let mut new_choice = rng.next_usize(choices);
            if new_choice == old_choice {
                new_choice = (new_choice + 1) % choices;
            }

            current[index] = new_choice;
            let next_cost = problem.cost(&current);
            let delta = next_cost - current_cost;
            let accept = delta <= 0
                || rng.next_f64()
                    < (-(delta as f64) / temperature.max(f64::MIN_POSITIVE)).exp();
            if accept {
                current_cost = next_cost;
                if current_cost < best_cost {
                    best = current.clone();
                    best_cost = current_cost;
                }
            } else {
                current[index] = old_choice;
            }
        }

        problem.repair(&best)
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
        let register_limit = max_regs.min(regs.regs.len());
        let problem = Problem::new(intervals, register_limit);
        let choices = self.anneal(&problem);
        let mut map = IndexMap::new();
        let mut stack_slots = 0_u32;

        for (index, interval) in problem.intervals.iter().enumerate() {
            let choice = choices.get(index).copied().unwrap_or(register_limit);
            if choice < register_limit {
                map.insert(interval.v, Location::Reg(regs.regs[choice]));
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
