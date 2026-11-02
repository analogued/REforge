// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Isolated visual experiment. No authority over execution, input or audio.

use crate::{
    Error,
    frame::{Frame, ROW_BYTES},
    gameplay::experiment::Scene,
};
use std::collections::VecDeque;

const HORIZON: f64 = 0.066;
const BOUND: f64 = 16.0;
const CORRECTION: f64 = 0.033;
const BLEND: f64 = 0.008;

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub at: f64,
    pub id: u64,
    pub body: u16,
    pub pass: u8,
    pub coordinates: [i16; 3],
    pub phase: u16,
    pub controls: u8,
    /// Craft source/width/height followed by shadow source/width/height.
    pub source: [usize; 6],
}

impl Pose {
    pub fn anchor(self) -> f64 {
        let scale = 2_097_152 / (i32::from(self.coordinates[0]) + 64);
        160.0 + f64::from(scale) * f64::from(self.coordinates[1]) / 65_536.0
    }

    fn bank(self) -> bool {
        (11..=15).contains(&self.source[0]) && self.source[3] == self.source[0] + 5
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub anchor: f64,
    pub lead: f64,
    pub velocity: f64,
    pub clipped: bool,
    pub hold: bool,
    pub reason: &'static str,
    pub id: u64,
    pub source_at: f64,
    pub seen: f64,
    pub hard_correction: f64,
    pub reset_jump: f64,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    pose: Pose,
    seen: f64,
    start: f64,
    velocity: f64,
    incoming: f64,
    adjustment: f64,
    lifetime: f64,
    continuous: bool,
    reason: &'static str,
    hard_correction: f64,
    reset_jump: f64,
}

impl Segment {
    fn sample(self, now: f64) -> Sample {
        let age = (now - self.seen).max(0.0);
        let t = age.min(self.lifetime);
        let correction_age = t / CORRECTION;
        let raw = if self.continuous {
            self.start
                + self.velocity * t
                + (self.incoming - self.velocity) * BLEND * (1.0 - (-t / BLEND).exp())
                + self.adjustment * (1.0 - (1.0 + correction_age) * (-correction_age).exp())
        } else {
            self.start + self.velocity * t + self.adjustment * (1.0 - (-correction_age).exp())
        };
        let anchor = raw.clamp(self.pose.anchor() - BOUND, self.pose.anchor() + BOUND);
        Sample {
            anchor,
            lead: anchor - self.pose.anchor(),
            velocity: self.velocity,
            clipped: anchor != raw,
            hold: age >= self.lifetime,
            reason: self.reason,
            id: self.pose.id,
            source_at: self.pose.at,
            seen: self.seen,
            hard_correction: self.hard_correction,
            reset_jump: self.reset_jump,
        }
    }

    fn rate(self, now: f64) -> f64 {
        let t = (now - self.seen).max(0.0);
        if t >= self.lifetime || self.sample(now).clipped {
            return 0.0;
        }
        if self.continuous {
            self.velocity
                + (self.incoming - self.velocity) * (-t / BLEND).exp()
                + self.adjustment * t / CORRECTION.powi(2) * (-t / CORRECTION).exp()
        } else {
            self.velocity + self.adjustment / CORRECTION * (-t / CORRECTION).exp()
        }
    }
}

/// Bounded, forward-only observer. No tape, host controls or future state.
#[derive(Default, Debug)]
pub struct Tracker {
    last: Option<Pose>,
    segment: Option<Segment>,
    cadence: VecDeque<Pose>,
    motion: VecDeque<Pose>,
    period: Option<f64>,
    blocked: bool,
}

fn append(queue: &mut VecDeque<Pose>, pose: Pose) {
    if queue.len() == 4 {
        queue.pop_front();
    }
    queue.push_back(pose);
}

pub fn next_velocity(mut velocity: i16, lateral: i16, controls: u8) -> i16 {
    let right = controls & 8 != 0;
    let left = controls & 4 != 0;
    if right && velocity < 7 {
        velocity += 1;
    }
    if left && velocity > -7 {
        velocity -= 1;
    }
    if !right && !left {
        velocity -= velocity.signum();
    }
    if i32::from(lateral) + i32::from(velocity) * 4 > 280 {
        velocity = (280 - lateral) >> 2;
    }
    if i32::from(lateral) + i32::from(velocity) * 4 < -320 {
        velocity = (-320 - lateral) >> 2;
    }
    velocity
}

impl Tracker {
    pub fn sample(&self, now: f64) -> Option<Sample> {
        self.segment
            .filter(|s| now + 1e-10 >= s.seen)
            .map(|s| s.sample(now))
    }

    pub fn suspend(&mut self, seen: f64, reason: &'static str) {
        if let Some(segment) = self.segment {
            self.segment = Some(Segment {
                start: segment.sample(seen).anchor,
                seen,
                velocity: 0.0,
                incoming: 0.0,
                adjustment: 0.0,
                lifetime: 0.0,
                continuous: false,
                reason,
                hard_correction: 0.0,
                reset_jump: 0.0,
                ..segment
            });
        }
        self.cadence.clear();
        self.motion.clear();
        self.period = None;
        self.blocked = true;
    }

    pub fn observe(&mut self, pose: Pose, seen: f64, rebase: bool) {
        debug_assert!(seen + 1e-10 >= pose.at);
        debug_assert!(self.segment.is_none_or(|s| seen >= s.seen));
        let previous = self.last;
        let old = self.sample(seen);
        let incoming = self.segment.map_or(0.0, |s| s.rate(seen));
        let mut position = old.map_or(pose.anchor(), |s| s.anchor);
        let gap = previous.map_or(0.0, |p| pose.at - p.at);
        let source_change =
            previous.is_some_and(|p| p.source != pose.source && !(p.bank() && pose.bank()));
        let boundary = previous.is_none()
            || rebase
            || pose.phase != 0
            || previous.is_some_and(|p| p.phase != 0)
            || source_change
            || gap > 0.075
            || self.blocked
            || seen - pose.at > HORIZON;
        let mut reason = "tracking";
        if boundary {
            reason = if previous.is_none() {
                "first"
            } else if rebase {
                "catch_up_rebase"
            } else if pose.phase != 0 || previous.is_some_and(|p| p.phase != 0) {
                "damage"
            } else if source_change {
                "artwork_guard"
            } else if self.blocked {
                "resume"
            } else if seen - pose.at > HORIZON {
                "stale"
            } else {
                "long_gap"
            };
            position = pose.anchor();
            self.motion.clear();
            if gap > 0.075 || self.blocked || pose.phase != 0 {
                self.cadence.clear();
                self.period = None;
            }
        }
        if pose.phase == 0 {
            append(&mut self.cadence, pose);
            if self.cadence.len() == 4 {
                let a = self.cadence[0];
                let span = pose.at - a.at;
                if pose.pass == a.pass
                    && pose.body.wrapping_sub(a.body) == 1
                    && (0.075..=0.150).contains(&span)
                {
                    self.period = Some(span / 3.0);
                }
            }
        }
        let controls_change = previous.is_some_and(|p| p.controls & 12 != pose.controls & 12);
        let delta = previous.map_or(0.0, |p| pose.anchor() - p.anchor());
        let stopped = previous.is_some() && delta.abs() < 1e-9;
        let reversal = self.motion.len() >= 2
            && delta
                * (self.motion[self.motion.len() - 1].anchor()
                    - self.motion[self.motion.len() - 2].anchor())
                < 0.0;
        if controls_change || reversal {
            self.motion.clear();
            if !boundary {
                reason = if controls_change {
                    "accepted_input_change"
                } else {
                    "observed_reversal"
                };
            }
        }
        append(&mut self.motion, pose);
        let mut velocity = 0.0;
        if !boundary
            && !stopped
            && let Some(period) = self.period
        {
            if controls_change || reversal {
                velocity = delta / period;
            } else if self.motion.len() == 4 {
                let a = self.motion[0];
                let span = pose.at - a.at;
                if (0.075..=0.150).contains(&span) {
                    velocity = (pose.anchor() - a.anchor()) / span;
                }
            } else if self.motion.len() >= 2 {
                velocity = delta / period;
            }
        }
        if stopped && !boundary {
            reason = "observed_stop";
        }
        if !boundary && let (Some(previous), Some(period)) = (previous, self.period) {
            let lateral_delta = i32::from(pose.coordinates[1]) - i32::from(previous.coordinates[1]);
            if pose.coordinates[0] == previous.coordinates[0]
                && lateral_delta % 4 == 0
                && lateral_delta.abs() <= 28
            {
                let next = next_velocity(
                    (lateral_delta / 4) as i16,
                    pose.coordinates[1],
                    pose.controls,
                );
                let scale = 2_097_152 / (i32::from(pose.coordinates[0]) + 64);
                velocity = f64::from(next) * 4.0 * f64::from(scale) / 65_536.0 / period;
            }
        }
        let reset_jump = if boundary {
            old.map_or(0.0, |s| position - s.anchor)
        } else {
            0.0
        };
        let before = position;
        position = position.clamp(pose.anchor() - BOUND, pose.anchor() + BOUND);
        let error = pose.anchor() - position;
        let deadband = if stopped || controls_change { 0.0 } else { 4.0 };
        self.segment = Some(Segment {
            pose,
            seen,
            start: position,
            velocity,
            incoming,
            adjustment: if boundary {
                0.0
            } else {
                error - error.clamp(-deadband, deadband)
            },
            lifetime: if boundary && pose.phase != 0 {
                0.0
            } else {
                (pose.at + HORIZON - seen).max(0.0)
            },
            continuous: !boundary,
            reason,
            hard_correction: position - before,
            reset_jump,
        });
        self.last = Some(pose);
        self.blocked = false;
    }
}

/// Recompose original operations in original order, moving only slots zero/one.
/// Placement rounds once to a native pixel, including negative coordinates.
pub fn compose(scene: &Scene, craft_left: Option<f64>) -> Result<Frame, Error> {
    let mut packed = scene.base.packed().to_vec();
    for layer in &scene.layers {
        let rect = layer.rect;
        let moved = craft_left.filter(|_| layer.slot <= 1);
        if let Some(x) = moved {
            let x = (x + 0.5).floor() as i32;
            let width = rect.width_bytes as i32 * 4;
            for y in rect.y.max(44)..(rect.y + rect.height as i32).min(158) {
                for dx in x.max(16)..(x + width).min(304) {
                    let sx = (dx - x) as usize;
                    let src = (y - rect.y) as usize * rect.width_bytes + sx / 4;
                    let ss = 6 - (sx % 4) * 2;
                    let mask = (layer.mask[src] >> ss) & 3;
                    let color = (layer.color[src] >> ss) & 3;
                    let dst = y as usize * ROW_BYTES + dx as usize / 4;
                    let shift = 6 - (dx as usize % 4) * 2;
                    let old = (packed[dst] >> shift) & 3;
                    packed[dst] = (packed[dst] & !(3 << shift)) | (((old & mask) | color) << shift);
                }
            }
        } else {
            for y in rect.y.max(44)..(rect.y + rect.height as i32).min(158) {
                for x in rect.x_bytes.max(4)..(rect.x_bytes + rect.width_bytes as i32).min(76) {
                    let src =
                        (y - rect.y) as usize * rect.width_bytes + (x - rect.x_bytes) as usize;
                    let dst = y as usize * ROW_BYTES + x as usize;
                    packed[dst] = (packed[dst] & layer.mask[src]) | layer.color[src];
                }
            }
        }
    }
    Frame::from_packed(&packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(i: u64, t: f64, lateral: i16) -> Pose {
        Pose {
            at: t,
            id: i,
            body: (i / 3) as u16,
            pass: (i % 3 + 1) as u8,
            coordinates: [14, lateral, -100],
            phase: 0,
            controls: 8,
            source: [13, 56, 18, 18, 56, 12],
        }
    }
    fn ready() -> Tracker {
        let mut t = Tracker::default();
        for (i, at) in [0.0, 0.028, 0.055, 0.110, 0.138, 0.165, 0.220]
            .into_iter()
            .enumerate()
        {
            t.observe(pose(i as u64, at, i as i16 * 4), at, false);
        }
        t
    }
    #[test]
    fn banking_and_release_preserve_position_and_velocity() {
        let mut t = ready();
        let old = t.sample(0.248).unwrap();
        let rate = t.segment.unwrap().rate(0.248);
        let mut p = pose(7, 0.248, 28);
        p.controls = 0;
        p.source = [14, 56, 21, 19, 56, 12];
        t.observe(p, 0.248, false);
        assert!((t.sample(0.248).unwrap().anchor - old.anchor).abs() < 1e-10);
        assert!((t.segment.unwrap().rate(0.248) - rate).abs() < 1e-10);
        assert_eq!(t.segment.unwrap().velocity, 0.0);
    }
    #[test]
    fn suspensions_and_late_recovery_never_backdate_motion() {
        let mut t = ready();
        t.suspend(0.24, "pause");
        assert_eq!(
            t.sample(0.24).unwrap().anchor,
            t.sample(1.0).unwrap().anchor
        );
        t.observe(pose(7, 0.248, 28), 0.5, true);
        assert!(t.sample(0.4).is_none());
        assert_eq!(t.sample(0.8).unwrap().anchor, pose(7, 0.248, 28).anchor());
    }
    #[test]
    fn limits_directions_horizon_and_bounds_are_independent() {
        assert_eq!(next_velocity(-7, -320, 4), 0);
        assert_eq!(next_velocity(7, 280, 8), 0);
        assert_eq!(next_velocity(7, 272, 8), 2);
        assert_eq!(next_velocity(7, 0, 12), 6);
        let t = ready();
        for i in 0..200 {
            assert!(t.sample(0.22 + f64::from(i) / 1000.0).unwrap().lead.abs() <= BOUND);
        }
        assert_eq!(t.sample(0.4).unwrap().anchor, t.sample(0.6).unwrap().anchor);
    }
}
