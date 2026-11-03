// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Causal full-scene presentation over immutable native publications.
//!
//! Intrinsic object motion is followed separately from the common camera. All
//! lifetimes, artwork and painter order come from the observed native scene.

mod paint;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};

use super::{BOUND, HORIZON, Pose, RESPONSE, follows, next_velocity};
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::Frame,
    gameplay::{Gameplay, Snapshot, SpecialState, raster::Scene},
};

#[derive(Clone, Debug)]
pub struct Observation {
    pub at: f64,
    pub id: u64,
    state: Snapshot,
    scene: Scene,
    objects: BTreeMap<usize, Object>,
    pose: Pose,
}

#[derive(Clone, Copy, Debug)]
struct Object {
    position: [f64; 3],
    active: bool,
    movement: u8,
    birth: u64,
    projected_birth: u64,
    gate: Option<SpecialState>,
}

impl Observation {
    /// Capture fresh, ordered drawing operations without advancing execution or
    /// resolving any native source or projection cache.
    pub fn capture(
        game: &Gameplay,
        assets: &GraphicsAssets,
        at: f64,
        id: u64,
    ) -> Result<Self, Error> {
        let state = game.snapshot();
        let scene = game.current_scene(assets)?;
        let (births, projected) = game.visual_epochs();
        let mut objects = BTreeMap::new();
        for e in state.weapon.shots.iter().chain(&state.enemies) {
            objects.insert(
                e.slot,
                Object {
                    position: [e.depth, e.lateral, e.height].map(f64::from),
                    active: e.active,
                    movement: e.movement,
                    birth: births[e.slot - 2],
                    projected_birth: projected[e.slot - 2],
                    gate: None,
                },
            );
        }
        for e in &state.terrain {
            objects.insert(
                e.slot,
                Object {
                    position: [f64::from(e.depth), f64::from(e.lateral), -100.0],
                    active: e.active,
                    movement: e.movement,
                    birth: 0,
                    projected_birth: 0,
                    gate: None,
                },
            );
        }
        for e in &state.special {
            objects.insert(
                e.slot,
                Object {
                    position: e.coordinates.map(f64::from),
                    active: e.active,
                    movement: e.movement,
                    birth: 0,
                    projected_birth: 0,
                    gate: Some(*e),
                },
            );
        }
        let player = state.player;
        let craft = &assets.sprites[player.resolved_sprite];
        let shadow = &assets.sprites[player.shadow.resolved_sprite];
        let pose = Pose {
            at,
            id,
            body: state.body,
            pass: state.render_pass,
            coordinates: player.coordinates,
            phase: player.phase,
            controls: player.effective_input as u8,
            horizon: player.horizon,
            origin: state.vertical_origin,
            velocity: player.velocity,
            source: [
                player.resolved_sprite,
                craft.width_bytes * 4,
                craft.height,
                player.shadow.resolved_sprite,
                shadow.width_bytes * 4,
                shadow.height,
            ],
        };
        Ok(Self {
            at,
            id,
            state,
            scene,
            objects,
            pose,
        })
    }

    fn normal_craft(&self) -> bool {
        self.pose.phase == 0
            && self
                .scene
                .layers
                .iter()
                .any(|l| l.slot == 0 && (1..=15).contains(&l.source))
    }
}

/// Interpolated original reciprocal table, retaining its finite precision at
/// integer depths and never interpolating across its signed wrap.
fn scale(depth: f64) -> f64 {
    let depth = depth.clamp(1.0, 999.0);
    let lo = depth.floor() as i32;
    let fraction = depth - f64::from(lo);
    (f64::from(2_097_152 / (lo + 64)) * (1.0 - fraction)
        + f64::from(2_097_152 / (lo + 65)) * fraction)
        / 65_536.0
}

fn project(position: [f64; 3], horizon: f64, origin: f64) -> [f64; 2] {
    let s = scale(position[0]);
    [
        160.0 + s * position[1],
        origin - s * (position[2] - horizon),
    ]
}

#[derive(Clone, Copy, Debug)]
struct Scalar {
    at: f64,
    start: f64,
    target: f64,
    velocity: f64,
    stop: f64,
}

impl Scalar {
    fn fixed(at: f64, value: f64) -> Self {
        Self {
            at,
            start: value,
            target: value,
            velocity: 0.0,
            stop: 0.0,
        }
    }

    fn sample(self, at: f64) -> f64 {
        let age = (at - self.at).max(0.0);
        let t = age.min(self.stop);
        let x = self.target
            + self.velocity * (t - RESPONSE)
            + (self.start - self.target + self.velocity * RESPONSE) * (-t / RESPONSE).exp();
        if age > t {
            let end = self.target + self.velocity * t;
            end + (x - end) * (-(age - t) / RESPONSE).exp()
        } else {
            x
        }
    }

    fn follow(
        old: Option<Self>,
        at: f64,
        value: f64,
        velocity: f64,
        stop: f64,
        delay: f64,
        reset: bool,
    ) -> Self {
        let consumed = delay.max(0.0).min(stop);
        let target = value + velocity * consumed;
        Self {
            at,
            start: if reset {
                target
            } else {
                old.map_or(target, |s| s.sample(at))
            },
            target,
            velocity,
            stop: (stop - consumed).max(0.0),
        }
    }

    fn freeze(&mut self, at: f64) {
        *self = Self::fixed(at, self.sample(at));
    }
}

#[derive(Debug)]
struct Track {
    identity: (u64, u8),
    history: VecDeque<(f64, [f64; 3])>,
    segments: [Scalar; 3],
}

#[derive(Default, Debug)]
pub struct Tracker {
    frame: Option<Observation>,
    cadence: VecDeque<Pose>,
    camera: Option<[Scalar; 3]>,
    tracks: BTreeMap<usize, Track>,
    suspended: bool,
    seen: f64,
}

impl Tracker {
    /// Freeze the analytical trajectories at an actual native wait boundary.
    pub fn suspend(&mut self, seen: f64) {
        if let Some(camera) = &mut self.camera {
            for s in camera {
                s.freeze(seen);
            }
        }
        for t in self.tracks.values_mut() {
            for s in &mut t.segments {
                s.freeze(seen);
            }
        }
        self.seen = seen;
        self.suspended = true;
        self.cadence.clear();
    }

    pub fn observe(&mut self, observation: Observation, seen: f64, rebase: bool) {
        let f = &observation;
        if self.frame.is_none() && f.pose.pass != 1 {
            return;
        }
        debug_assert!(seen + 1e-10 >= f.at);
        let continuous = self.frame.as_ref().is_some_and(|old| {
            follows(old.pose, f.pose)
                && (i32::from(f.state.static_lateral) - i32::from(old.state.static_lateral)).abs()
                    < 2048
                && !self.suspended
                && !rebase
        });
        if !continuous {
            self.cadence.clear();
            self.tracks.clear();
        }
        if self.cadence.len() == 4 {
            self.cadence.pop_front();
        }
        self.cadence.push_back(f.pose);
        let period = if self.cadence.len() == 4 {
            let first = self.cadence[0];
            let span = f.at - first.at;
            (f.pose.body.wrapping_sub(first.body) == 1 && span > 0.0).then_some(span / 3.0)
        } else {
            None
        };
        let delay = (seen - f.at).max(0.0);
        let (rates, stops) = camera_forecast(f, period);
        let values = [
            f64::from(f.state.static_lateral),
            f64::from(f.pose.horizon),
            f64::from(f.pose.origin),
        ];
        self.camera = Some(std::array::from_fn(|i| {
            Scalar::follow(
                self.camera.map(|c| c[i]),
                seen,
                values[i],
                rates[i],
                stops[i],
                delay,
                !continuous,
            )
        }));
        let mut keep = Vec::new();
        for (&slot, o) in &f.objects {
            let d = o.position[0];
            if !o.active || !(0.0..1000.0).contains(&d) || d == 0.0 || o.projected_birth != o.birth
            {
                continue;
            }
            let movement = if (10..=14).contains(&slot) {
                f.objects[&(slot - 5)].movement
            } else {
                o.movement
            };
            let family = if (15..=22).contains(&slot) && matches!(movement, 2 | 3 | 252) {
                2
            } else {
                movement
            };
            let identity = (o.birth, family);
            let old = self.tracks.get(&slot);
            let recycled = slot >= 15
                && old.is_some_and(|t| t.history.back().is_some_and(|p| d > p.1[0] + 1.0));
            let fresh = !continuous || recycled || old.is_none_or(|t| t.identity != identity);
            keep.push(slot);
            if f.pose.pass == 2 && !fresh {
                continue;
            }
            let mut history = if fresh {
                VecDeque::new()
            } else {
                old.unwrap().history.clone()
            };
            let values = [
                d,
                o.position[1] - f64::from(f.state.static_lateral),
                o.position[2],
            ];
            if history.len() == 3 {
                history.pop_front();
            }
            history.push_back((f.at, values));
            let (rates, stop) =
                if let Some(period) = period.filter(|_| !fresh || matches!(slot, 2 | 3)) {
                    object_forecast(f, slot, movement, values, &history, period)
                } else {
                    ([0.0; 3], 0.0)
                };
            let segments = std::array::from_fn(|i| {
                Scalar::follow(
                    old.map(|t| t.segments[i]),
                    seen,
                    values[i],
                    rates[i],
                    stop,
                    delay,
                    fresh,
                )
            });
            self.tracks.insert(
                slot,
                Track {
                    identity,
                    history,
                    segments,
                },
            );
        }
        self.tracks.retain(|slot, _| keep.contains(slot));
        self.frame = Some(observation);
        self.seen = seen;
        self.suspended = false;
    }

    fn positions(&self, at: f64) -> (BTreeMap<usize, [f64; 3]>, [f64; 3]) {
        let f = self.frame.as_ref().unwrap();
        let camera = self.camera.unwrap().map(|s| s.sample(at));
        let positions = f
            .objects
            .iter()
            .map(|(&slot, o)| {
                let mut p = o.position;
                if let Some(track) = self.tracks.get(&slot) {
                    p = track.segments.map(|s| s.sample(at));
                    p[1] += camera[0];
                } else {
                    p[1] += camera[0] - f64::from(f.state.static_lateral);
                }
                (slot, p)
            })
            .collect();
        // A single continuous horizon drives both terms; filtering the native
        // cached origin staircase independently can reverse forward terrain.
        (positions, [camera[0], camera[1], 106.0 + camera[1] / 16.0])
    }

    /// Apply the same common-camera origin used by the full scene painter to
    /// ordinary craft and shadow anchors supplied by the craft tracker.
    pub(crate) fn craft_anchors(&self, at: f64, anchors: [f64; 3]) -> Option<[f64; 3]> {
        let frame = self.frame.as_ref()?;
        if !frame.normal_craft() || at + 1e-10 < self.seen {
            return None;
        }
        let camera = self.camera?;
        let correction = 106.0 + camera[1].sample(at) / 16.0 - camera[2].sample(at);
        Some([anchors[0], anchors[1] + correction, anchors[2] + correction])
    }

    pub fn render(
        &self,
        at: f64,
        craft_anchors: Option<[f64; 3]>,
        assets: &GraphicsAssets,
    ) -> Result<Option<Frame>, Error> {
        if self.frame.is_none() || at + 1e-10 < self.seen {
            return Ok(None);
        }
        paint::render(self, at, craft_anchors, assets).map(Some)
    }
}

fn camera_forecast(f: &Observation, period: Option<f64>) -> ([f64; 3], [f64; 3]) {
    let Some(period) = period else {
        return ([0.0; 3], [0.0; 3]);
    };
    let [vx, vy] = if f.normal_craft() {
        next_velocity(f.pose).map(f64::from)
    } else {
        [0.0; 2]
    };
    let craft_scale = scale(f64::from(f.pose.coordinates[0]));
    let speed = (4.0 * craft_scale * vx / period).hypot(-3.0 * craft_scale * vy / period);
    let duration = if speed == 0.0 {
        HORIZON
    } else {
        HORIZON.min(BOUND / speed)
    };
    let [_, x, y] = f.pose.coordinates.map(f64::from);
    let tx = if vx > 0.0 {
        (280.0 - x) / (4.0 * vx) * period
    } else if vx < 0.0 {
        (-320.0 - x) / (4.0 * vx) * period
    } else {
        f64::INFINITY
    };
    let ty = if vy > 0.0 {
        (230.0 - y) / (4.0 * vy) * period
    } else if vy < 0.0 {
        f64::from(f.pose.horizon) / -vy * period
    } else {
        f64::INFINITY
    };
    (
        [-8.0 * vx / period, vy / period, 0.0],
        [duration.min(tx).max(0.0), duration.min(ty).max(0.0), 0.0],
    )
}

fn object_forecast(
    f: &Observation,
    slot: usize,
    movement: u8,
    values: [f64; 3],
    history: &VecDeque<(f64, [f64; 3])>,
    period: f64,
) -> ([f64; 3], f64) {
    let world_period = period * 1.5;
    let mut rates = [0.0; 3];
    if history.len() == 3 {
        let elapsed = f.at - history[0].0;
        if elapsed > 0.0 {
            rates = std::array::from_fn(|i| (values[i] - history[0].1[i]) / elapsed);
        }
    }
    if matches!(slot, 2 | 3) {
        rates = [
            f64::from((values[0] as i32 >> 4) + 22) / world_period,
            0.0,
            0.0,
        ];
    } else if (5..=14).contains(&slot) && matches!(movement, 6 | 7 | 17) {
        rates = [
            -f64::from(f.state.speeds[if movement == 7 { 2 } else { 1 }]) / world_period,
            0.0,
            0.0,
        ];
    } else if slot == 4 {
        rates = [-f64::from(f.state.speeds[0]) / world_period, 0.0, 0.0];
        for (i, rate) in rates.iter_mut().enumerate().skip(1) {
            *rate = (f64::from(f.state.enemy_target[i - 1]) - f.objects[&slot].position[i])
                .clamp(-80.0, 80.0)
                / world_period;
        }
    } else if slot >= 15 {
        rates = if slot >= 23 || movement == 2 {
            [-f64::from(f.state.speeds[0]) / world_period, 0.0, 0.0]
        } else {
            [0.0; 3]
        };
    }
    let mut duration = HORIZON;
    if rates[0] > 0.0 {
        duration = duration.min(
            ((if matches!(slot, 2 | 3) { 300.0 } else { 999.0 }) - values[0]).max(0.0) / rates[0],
        );
    } else if rates[0] < 0.0 {
        let floor = if slot >= 15 {
            f64::from(f.pose.coordinates[0])
        } else {
            1.0
        };
        duration = duration.min((values[0] - floor).max(0.0) / -rates[0]);
    }
    let mut start = values;
    start[1] += f64::from(f.state.static_lateral);
    let original = project(start, f64::from(f.pose.horizon), f64::from(f.pose.origin));
    let travel = |t: f64| {
        let end = project(
            std::array::from_fn(|i| start[i] + rates[i] * t),
            f64::from(f.pose.horizon),
            f64::from(f.pose.origin),
        );
        (end[0] - original[0]).hypot(end[1] - original[1])
    };
    if travel(duration) > BOUND {
        let (mut lo, mut hi) = (0.0, duration);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if travel(mid) <= BOUND {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        duration = lo;
    }
    (rates, duration)
}
