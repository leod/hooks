//! This module allows collecting statistics for values that change over time. Its purpose is in
//! debugging.
//!
//! Currently, a thread local variable is used to collect statistics.

use std::f32;
use std::fmt;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::collections::BTreeMap;

use debug;

thread_local!(static STATS: RefCell<Stats> = RefCell::new(Stats::new()));

type Time = f32;

/// All statistics of one variable.
pub struct Var {
    /// Last recorded values.
    pub recent_values: VecDeque<(Time, f32)>,

    /// Overall average.
    pub average: f32,

    /// Overall number of samples.
    pub num_samples: usize,
}

impl Var {
    fn new() -> Var {
        Var {
            recent_values: VecDeque::new(),
            average: 0.0,
            num_samples: 0,
        }
    }

    fn record(&mut self, time: Time, value: f32) {
        self.recent_values.push_back((time, value));

        self.num_samples += 1;

        // https://math.stackexchange.com/questions/106700/incremental-averageing
        self.average = self.average + (value - self.average) / self.num_samples as f32;
    }

    fn prune(&mut self, min_time: Time) {
        self.recent_values.retain(|&(time, _)| time >= min_time);
    }

    fn recent_average(&self) -> f32 {
        self.recent_values
            .iter()
            .map(|&(_, value)| value)
            .sum::<f32>() / self.recent_values.len() as f32
    }
}

impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "value {:.4}, recent μ={:.4} #={}, overall μ={:.4} #={}",
            self.recent_values
                .back()
                .map(|&(_, value)| value)
                .unwrap_or(f32::NAN),
            self.recent_average(),
            self.recent_values.len(),
            self.average,
            self.num_samples,
        )
    }
}

struct Stats {
    time: Time,
    max_age: Time,
    vars: BTreeMap<&'static str, Var>,
}

impl Stats {
    fn new() -> Stats {
        Stats {
            time: 0.0,
            max_age: 10.0,
            vars: BTreeMap::new(),
        }
    }
}

pub fn record(name: &'static str, value: f32) {
    STATS.with(|stats| {
        let mut stats = stats.borrow_mut();

        let time = stats.time;
        let var = stats.vars.entry(name).or_insert(Var::new());
        var.record(time, value);
    });
}

pub fn clear() {
    STATS.with(|state| state.borrow_mut().vars.clear());
}

pub fn update(dt: Time) {
    STATS.with(|stats| stats.borrow_mut().time += dt);

    STATS.with(|stats| {
        let mut stats = stats.borrow_mut();

        let min_time = stats.time - stats.max_age;
        for var in stats.vars.values_mut() {
            var.prune(min_time);
        }
    });
}

pub fn print() {
    STATS.with(|stats| {
        let stats = stats.borrow();

        for (name, var) in stats.vars.iter() {
            debug!("{}\t{:?}", name, var);
        }
    });
}

pub fn inspect() -> debug::Vars {
    STATS.with(|stats| {
        let stats = stats.borrow();

        let vars = stats
            .vars
            .iter()
            .map(|(name, var)| (name.to_string(), debug::Vars::Leaf(format!("{:?}", var))))
            .collect();
        debug::Vars::Node(vars)
    })
}
