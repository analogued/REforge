// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Isolated two-dimensional visual observer. Execution and input stay authoritative.

mod raster;
pub use raster::{Observation, World, compose};

use std::collections::VecDeque;

pub const HORIZON: f64 = 0.050;
const BOUND: f64 = 12.0;
const RESPONSE: f64 = 0.020;

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub at: f64,
    pub id: u64,
    pub body: u16,
    pub pass: u8,
    pub coordinates: [i16; 3],
    pub phase: u16,
    pub controls: u8,
    pub horizon: i16,
    pub origin: i32,
    pub velocity: [i16; 2],
    /// Craft source/width/height followed by shadow source/width/height.
    pub source: [usize; 6],
}

impl Pose {
    pub fn normal(self) -> bool {
        self.phase == 0 && (1..=15).contains(&self.source[0]) && (16..=20).contains(&self.source[3])
    }

    /// Project without changing the native projection caches. Shadow X follows
    /// craft X; its vertical anchor remains on the original ground plane.
    pub fn anchors(self) -> [f64; 3] {
        let [depth, lateral, height] = self.coordinates;
        let scale = f64::from(2_097_152 / (i32::from(depth) + 64)) / 65_536.0;
        let shadow_scale = f64::from(2_097_152 / (i32::from(depth) + 65)) / 65_536.0;
        [
            160.0 + scale * f64::from(lateral),
            f64::from(self.origin) - scale * f64::from(height.wrapping_sub(self.horizon)),
            f64::from(self.origin)
                - shadow_scale * f64::from((-115_i16).wrapping_sub(self.horizon)),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// Craft X, craft Y and shadow Y before discrete artwork offsets.
    pub anchors: [f64; 3],
    pub target: [f64; 3],
    pub velocity: [f64; 3],
    pub hold: bool,
    pub reason: &'static str,
    pub id: u64,
    pub source_at: f64,
    pub seen: f64,
    pub reset_distance: f64,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    pose: Pose,
    seen: f64,
    start: [f64; 3],
    target: [f64; 3],
    velocity: [f64; 3],
    lifetime: f64,
    response: f64,
    reason: &'static str,
    reset_distance: f64,
}

impl Segment {
    fn sample(self, now: f64) -> Sample {
        let age = (now - self.seen).max(0.0);
        let t = age.min(self.lifetime);
        let anchors = if self.response == 0.0 {
            self.start
        } else {
            std::array::from_fn(|i| {
                let x = self.target[i]
                    + self.velocity[i] * (t - self.response)
                    + (self.start[i] - self.target[i] + self.velocity[i] * self.response)
                        * (-t / self.response).exp();
                if age > t {
                    let end = self.target[i] + self.velocity[i] * t;
                    end + (x - end) * (-(age - t) / self.response).exp()
                } else {
                    x
                }
            })
        };
        Sample {
            anchors,
            target: self.target,
            velocity: self.velocity,
            hold: age >= self.lifetime,
            reason: self.reason,
            id: self.pose.id,
            source_at: self.pose.at,
            seen: self.seen,
            reset_distance: self.reset_distance,
        }
    }
}

fn accelerate(mut velocity: i16, positive: bool, negative: bool, limit: i16) -> i16 {
    if positive && velocity < limit {
        velocity += 1;
    }
    if negative && velocity > -limit {
        velocity -= 1;
    }
    if !positive && !negative {
        velocity -= velocity.signum();
    }
    velocity
}

/// One visual forecast using already accepted controls and original movement
/// clamps. The measured body cadence converts updates to presentation velocity;
/// this never advances the game or consults the host's unpolled held state.
pub fn forecast(pose: Pose, period: f64) -> [f64; 3] {
    let [depth, lateral, height] = pose.coordinates;
    let mut vx = accelerate(
        pose.velocity[0],
        pose.controls & 8 != 0,
        pose.controls & 4 != 0,
        7,
    );
    let mut vy = accelerate(
        pose.velocity[1],
        pose.controls & 1 != 0,
        pose.controls & 2 != 0,
        6,
    );
    vx = vx.min((280 - lateral) >> 2).max((-320 - lateral) >> 2);
    vy = vy.max(-pose.horizon);
    if i32::from(height) + 4 * i32::from(vy) > 230 {
        vy = (230 - height) >> 2;
    }
    let scale = f64::from(2_097_152 / (i32::from(depth) + 64)) / 65_536.0;
    let shadow_scale = f64::from(2_097_152 / (i32::from(depth) + 65)) / 65_536.0;
    [
        4.0 * scale * f64::from(vx) / period,
        -3.0 * scale * f64::from(vy) / period,
        shadow_scale * f64::from(vy) / period,
    ]
}

/// Bounded causal forecast with an exactly integrated first-order response.
/// The forecast target stops at its horizon/bound; the filter can still settle.
#[derive(Default, Debug)]
pub struct Tracker {
    last: Option<Pose>,
    segment: Option<Segment>,
    cadence: VecDeque<Pose>,
    blocked: bool,
}

impl Tracker {
    pub fn sample(&self, now: f64) -> Option<Sample> {
        self.segment
            .filter(|s| now + 1e-10 >= s.seen)
            .map(|s| s.sample(now))
    }

    pub fn suspend(&mut self, seen: f64, reason: &'static str) {
        if let Some(segment) = self.segment {
            let position = segment.sample(seen).anchors;
            self.segment = Some(Segment {
                seen,
                start: position,
                target: position,
                velocity: [0.0; 3],
                lifetime: 0.0,
                response: 0.0,
                reason,
                reset_distance: 0.0,
                ..segment
            });
        }
        self.cadence.clear();
        self.blocked = true;
    }

    pub fn observe(&mut self, pose: Pose, seen: f64, rebase: bool) {
        debug_assert!(seen + 1e-10 >= pose.at);
        debug_assert!(self.segment.is_none_or(|s| seen >= s.seen));
        debug_assert!(self.last.is_none_or(|p| pose.id > p.id && pose.at >= p.at));
        let mut target = pose.anchors();
        let mut position = self.sample(seen).map_or(target, |s| s.anchors);
        let boundary = self
            .last
            .is_none_or(|p| !pose.normal() || !p.normal() || pose.at - p.at > 0.075)
            || self.blocked
            || rebase;
        let reason = if self.last.is_none() {
            "first"
        } else if rebase {
            "rebase"
        } else if boundary {
            "boundary"
        } else {
            "tracking"
        };
        let reset_distance = if boundary {
            let distance = (position[0] - target[0]).hypot(position[1] - target[1]);
            position = target;
            self.cadence.clear();
            distance
        } else {
            0.0
        };
        if self.cadence.len() == 4 {
            self.cadence.pop_front();
        }
        self.cadence.push_back(pose);
        let period = if self.cadence.len() == 4 {
            let first = self.cadence[0];
            let span = pose.at - first.at;
            (pose.body.wrapping_sub(first.body) == 1 && (0.075..=0.150).contains(&span))
                .then_some(span / 3.0)
        } else {
            None
        };
        let mut velocity = [0.0; 3];
        let mut lifetime = 0.0;
        if let Some(period) = period
            && pose.normal()
            && reason == "tracking"
        {
            velocity = forecast(pose, period);
            let speed = velocity[0].hypot(velocity[1]);
            let max_age = HORIZON.min(if speed == 0.0 { HORIZON } else { BOUND / speed });
            let age = (seen - pose.at).max(0.0).min(max_age);
            for i in 0..3 {
                target[i] += velocity[i] * age;
            }
            lifetime = (max_age - age).max(0.0);
        }
        self.segment = Some(Segment {
            pose,
            seen,
            start: position,
            target,
            velocity,
            lifetime,
            response: if pose.normal() { RESPONSE } else { 0.0 },
            reason,
            reset_distance,
        });
        self.last = Some(pose);
        self.blocked = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(i: u64) -> Pose {
        Pose {
            at: i as f64 * 0.04,
            id: i,
            body: (i / 3) as u16 + 1,
            pass: (i % 3 + 1) as u8,
            coordinates: [14, i as i16 * 4, 0],
            phase: 0,
            controls: 8,
            horizon: 0,
            origin: 106,
            velocity: [1, 0],
            source: [13, 56, 18, 18, 56, 12],
        }
    }
    #[test]
    fn normal_artwork_changes_do_not_reset_either_axis() {
        let mut t = Tracker::default();
        for source in 1..=15 {
            let mut p = pose(source as u64 - 1);
            p.source[0] = source;
            p.source[3] = 16 + (source - 1) % 5;
            let before = t.sample(p.at);
            t.observe(p, p.at, false);
            if let Some(before) = before {
                assert_eq!(t.sample(p.at).unwrap().reason, "tracking");
                for i in 0..3 {
                    assert!((before.anchors[i] - t.sample(p.at).unwrap().anchors[i]).abs() < 1e-10);
                }
            }
        }
    }
    #[test]
    fn suspensions_and_late_recovery_do_not_replay_obsolete_motion() {
        let mut t = Tracker::default();
        for i in 0..7 {
            let p = pose(i);
            t.observe(p, p.at, false);
        }
        t.suspend(0.25, "pause");
        assert_eq!(
            t.sample(0.25).unwrap().anchors,
            t.sample(2.0).unwrap().anchors
        );
        let p = pose(7);
        t.observe(p, 0.5, true);
        assert!(t.sample(0.4).is_none());
        assert_eq!(t.sample(0.6).unwrap().anchors, p.anchors());
    }
    #[test]
    fn query_partitioning_does_not_change_the_trajectory() {
        let mut t = Tracker::default();
        for i in 0..7 {
            let p = pose(i);
            t.observe(p, p.at, false);
        }
        let expected = t.sample(0.3).unwrap().anchors;
        for i in 0..60 {
            t.sample(0.24 + f64::from(i) / 1000.0);
        }
        assert_eq!(expected, t.sample(0.3).unwrap().anchors);
    }
    #[test]
    fn original_clamps_and_simultaneous_key_order_are_preserved() {
        let mut p = pose(0);
        p.coordinates = [14, 280, 230];
        p.horizon = 100;
        p.velocity = [7, 6];
        p.controls = 9;
        assert_eq!(forecast(p, 0.04), [0.0; 3]);
        p.coordinates = [14, 0, 0];
        p.velocity = [7, 0];
        p.controls = 12;
        assert!(
            (forecast(p, 0.04)[0] - 6.0 * 4.0 * f64::from(2_097_152 / 78) / 65_536.0 / 0.04).abs()
                < 1e-12
        );
    }
    #[test]
    fn forecast_target_has_a_time_and_displacement_bound() {
        let mut t = Tracker::default();
        for i in 0..10 {
            let mut p = pose(i);
            p.velocity = [7, 6];
            p.controls = 9;
            t.observe(p, p.at, false);
            let s = t.segment.unwrap();
            assert!(s.lifetime <= HORIZON);
            assert!(s.velocity[0].hypot(s.velocity[1]) * s.lifetime <= BOUND + 1e-12);
        }
    }
}
