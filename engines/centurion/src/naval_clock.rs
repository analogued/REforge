// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic reference execution budget for the DOS naval busy loop.
//!
//! The 6000-unit/ms reference performs short polls between expensive repaints.
//! These measured cost classes preserve that scheduling pattern independently of
//! host speed. They are a calibrated compatibility clock, not a cycle-exact CPU.
use crate::intro::{PIT_DIVISOR, PIT_HZ};

const UNITS_PER_SECOND: u128 = 6_000_000;
const POLL: u64 = 3_040;
const REPAINT: u64 = 166_000;

#[derive(Default)]
pub struct ExecutionClock {
    ticks: u64,
    next: u64,
    end: u64,
    polls: u64,
}
impl ExecutionClock {
    pub fn advance_tick(&mut self) {
        self.ticks += 1;
        self.end = (u128::from(self.ticks) * UNITS_PER_SECOND * PIT_DIVISOR / PIT_HZ) as u64;
    }
    pub fn next_poll(&self) -> Option<u32> {
        (self.next < self.end)
            .then(|| (u128::from(self.next) * PIT_HZ / (UNITS_PER_SECOND * PIT_DIVISOR)) as u32)
    }
    pub fn polls(&self) -> u64 {
        self.polls
    }
    pub fn charge(&mut self, repaint: bool, sound: u64) {
        self.polls += 1;
        self.next += POLL + if repaint { REPAINT } else { 0 } + sound;
    }
    pub fn now(&self) -> u32 {
        self.ticks as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_burst_between_repaints_and_sound_costs_do_not_stop_the_timer() {
        let mut clock = ExecutionClock::default();
        let mut stamp = u32::MAX - 2;
        let mut counts = Vec::new();
        for _ in 0..72 {
            clock.advance_tick();
            let mut count = 0;
            while let Some(now) = clock.next_poll() {
                let repaint = now.wrapping_sub(stamp) > 2;
                if repaint {
                    stamp = now;
                }
                clock.charge(repaint, 0);
                count += 1;
            }
            counts.push(count);
        }
        assert!(counts.contains(&0));
        assert!(counts.iter().any(|n| *n >= 25));
        assert!((500..800).contains(&counts.iter().sum::<u32>()));
        let mut clock = ExecutionClock::default();
        clock.advance_tick();
        assert_eq!(clock.next_poll(), Some(0));
        clock.charge(false, 6_000_000);
        for _ in 0..60 {
            clock.advance_tick();
            assert!(clock.next_poll().is_none());
        }
        assert_eq!(clock.now(), 61);
    }
}
