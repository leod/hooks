use std::ops::AddAssign;
use std::time::{Duration, Instant};

pub fn duration_to_secs(d: Duration) -> f32 {
    let seconds = d.as_secs() as f32;
    let nanos = d.subsec_nanos() as f32;
    seconds + (nanos * 1e-9)
}

pub fn secs_to_duration(t: f32) -> Duration {
    debug_assert!(t >= 0.0, "secs_to_duration passed a negative number");

    let seconds = t.trunc();
    let nanos = t.fract() * 1e9;
    Duration::new(seconds as u64, nanos as u32)
}

/// A timer that can be used to trigger events that happen periodically.
#[derive(Debug)]
pub struct Timer {
    period: Duration,
    accum: Duration,
}

impl Timer {
    pub fn new(period: Duration) -> Timer {
        Timer {
            period,
            accum: Default::default(),
        }
    }

    pub fn from_hz(hz: f32) -> Timer {
        Timer::new(secs_to_duration(1.0 / hz))
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    /// Has the timer accumulated enough time for one period?
    /// If yes, subtract the period from the timer.
    pub fn trigger(&mut self) -> bool {
        if self.accum >= self.period {
            self.accum = self.accum.checked_sub(self.period).unwrap();
            true
        } else {
            false
        }
    }

    /// Has the timer accumulated enough time for one period?
    /// If yes, reset the timer to zero.
    pub fn trigger_reset(&mut self) -> bool {
        if self.accum >= self.period {
            self.accum = Duration::default();
            true
        } else {
            false
        }
    }

    /// Reset the accumulated time. Returns true if enough time has passed for one period.
    pub fn reset(&mut self) -> bool {
        let trigger = self.accum >= self.period;
        self.accum = Duration::default();
        trigger
    }

    /// Percentual progress until the next period.
    pub fn progress(&self) -> f32 {
        duration_to_secs(self.accum) / duration_to_secs(self.period)
    }

    pub fn accum(&self) -> Duration {
        self.accum
    }

    pub fn accum_secs(&self) -> f32 {
        duration_to_secs(self.accum)
    }
}

impl AddAssign<Duration> for Timer {
    fn add_assign(&mut self, other: Duration) {
        self.accum = self.accum.checked_add(other).unwrap();
    }
}

/// A stopwatch for repeatedly measuring time intervals.
pub struct Stopwatch {
    last_reset: Option<Instant>,
}

impl Stopwatch {
    pub fn new() -> Stopwatch {
        Stopwatch { last_reset: None }
    }

    pub fn get_reset(&mut self) -> Duration {
        let duration = if let Some(last_reset) = self.last_reset {
            last_reset.elapsed()
        } else {
            Duration::from_secs(0)
        };

        self.last_reset = Some(Instant::now());
        duration
    }
}
