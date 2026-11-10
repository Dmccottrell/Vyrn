//! `Clock`: a fixed-rate frame clock for game loops.
//!
//! ```text
//! var clock = Clock(hz = 120)
//! while clock.running:
//!     step(mut world, clock.tick())
//! end
//! ```
//!
//! `tick()` waits until the next frame is due and returns the fixed step,
//! `1 / hz` seconds, so physics stays deterministic whatever the machine
//! does. A clock that falls far behind (a breakpoint, a slow frame) skips
//! ahead instead of running a burst of frames to catch up.

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Clock {
    pub hz: f64,
    /// Stop after this many frames; `None` runs until `stop()`.
    pub limit: Option<u64>,
    pub frame: u64,
    pub start: Instant,
    /// When the next frame is due.
    next: Instant,
    last: Instant,
    /// Seconds the last frame really took.
    pub real_dt: f64,
    pub stopped: bool,
}

/// How far behind schedule a clock may fall before it skips ahead.
const MAX_LAG: Duration = Duration::from_millis(250);

impl Clock {
    pub fn new(hz: f64, limit: Option<u64>) -> Clock {
        let now = Instant::now();
        Clock {
            hz,
            limit,
            frame: 0,
            start: now,
            next: now,
            last: now,
            real_dt: 0.0,
            stopped: false,
        }
    }

    pub fn step(&self) -> f64 {
        1.0 / self.hz
    }

    pub fn running(&self) -> bool {
        !self.stopped && self.limit.is_none_or(|n| self.frame < n)
    }

    /// Waits for the next frame and returns the fixed step in seconds.
    pub fn tick(&mut self) -> f64 {
        let period = Duration::from_secs_f64(self.step());
        let mut now = Instant::now();
        if self.frame > 0 {
            if now < self.next {
                std::thread::sleep(self.next - now);
                now = Instant::now();
            } else if now - self.next > MAX_LAG {
                self.next = now;
            }
        } else {
            self.next = now;
        }
        self.real_dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.next += period;
        self.frame += 1;
        self.step()
    }

    pub fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Frames per second over the run so far.
    pub fn fps(&self) -> f64 {
        let elapsed = (self.last - self.start).as_secs_f64();
        if self.frame < 2 || elapsed <= 0.0 {
            0.0
        } else {
            (self.frame - 1) as f64 / elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_keep_the_rate() {
        let mut clock = Clock::new(200.0, Some(5));
        let start = Instant::now();
        let mut steps = 0.0;
        while clock.running() {
            steps += clock.tick();
        }
        assert_eq!(clock.frame, 5);
        assert!((steps - 0.025).abs() < 1e-9);
        // Four waits of 5 ms after the first frame.
        assert!(start.elapsed() >= Duration::from_millis(19));
    }
}
