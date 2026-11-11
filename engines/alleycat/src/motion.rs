// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only visual motion. Native execution stays authoritative.

use crate::{
    alley::Alley,
    framebuffer::{Dimensions, Framebuffer, HEIGHT, WIDTH},
};
use std::collections::{BTreeMap, VecDeque};
mod cloth;
pub(crate) mod hearts_walk;
pub mod history;
pub(crate) mod walking;
pub(crate) use cloth::ClothBand;
#[cfg(test)]
mod tests;

/// Saved player-facing scene presentation, independent of input response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SceneMode {
    #[default]
    Reforged,
    Original,
}
impl SceneMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reforged => "reforged",
            Self::Original => "original",
        }
    }
}
impl std::str::FromStr for SceneMode {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "reforged" => Ok(Self::Reforged),
            "original" => Ok(Self::Original),
            _ => Err("motion must be reforged or original".into()),
        }
    }
}

/// Development comparisons; the saved setting only selects Reforged or Original.
#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Original,
    Aligned,
    Smooth,
    History25,
    HistoryRivals25,
    HistoryRivals40,
    BirdVelocity,
    BirdHistory25,
    HeadHistory60,
    HeadNative60,
    HistorySwim25,
    History35,
    History40,
    History60,
}
#[cfg(feature = "dev-tools")]
impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Original => Self::Aligned,
            Self::Aligned => Self::Smooth,
            Self::Smooth => Self::History25,
            Self::History25 => Self::HistoryRivals25,
            Self::HistoryRivals25 => Self::HeadHistory60,
            Self::HeadHistory60 => Self::HeadNative60,
            Self::HeadNative60 => Self::BirdHistory25,
            Self::HistoryRivals40 => Self::BirdVelocity,
            Self::BirdVelocity => Self::HistorySwim25,
            Self::BirdHistory25 => Self::HistoryRivals40,
            Self::HistorySwim25 => Self::History35,
            Self::History35 => Self::History40,
            Self::History40 => Self::History60,
            Self::History60 => Self::Original,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Aligned => "Aligned steps",
            Self::Smooth => "Aligned smooth",
            Self::History25 => "History smooth · 25 ms",
            Self::HistoryRivals25 => "History smooth · cat + rivals, 25 ms",
            Self::HistoryRivals40 => "History smooth · cat 25 ms, rivals 40 ms",
            Self::BirdVelocity => "Bird · velocity trial (extra lag)",
            Self::BirdHistory25 => "Bird · observed positions, 25 ms",
            Self::HeadHistory60 => "Falling head · observed positions, 60 ms",
            Self::HeadNative60 => "Falling head · native steps, 60 ms",
            Self::HistorySwim25 => "History smooth · swimming 25 ms, rivals 40 ms",
            Self::History35 => "History smooth · 35 ms",
            Self::History40 => "History smooth · 40 ms",
            Self::History60 => "History smooth · 60 ms",
        }
    }
    pub fn label(self, active: bool) -> &'static str {
        match (self, active) {
            (Self::Original, _) => "Original [F8]",
            (Self::Aligned, true) => "Aligned steps [F8]",
            (Self::Aligned, false) => "Aligned steps: native fallback [F8]",
            (Self::Smooth, true) => "Aligned smooth [F8]",
            (Self::Smooth, false) => "Aligned smooth: native fallback [F8]",
            (Self::History25, _) => "History smooth: 25 ms [F8]",
            (Self::HistoryRivals25, _) => "History: cat + rivals, 25 ms [F8]",
            (Self::HistoryRivals40, _) => "History: cat 25 / rivals 40 ms [F8]",
            (Self::BirdVelocity, _) => "Bird: velocity trial [F8]",
            (Self::BirdHistory25, _) => "Bird: history 25 ms [F8]",
            (Self::HeadHistory60, _) => "Falling head: history 60 ms [F8]",
            (Self::HeadNative60, _) => "Falling head: native 60 ms [F8]",
            (Self::HistorySwim25, _) => "History: swimming 25 ms [F8]",
            (Self::History35, _) => "History smooth: 35 ms [F8]",
            (Self::History40, _) => "History smooth: 40 ms [F8]",
            (Self::History60, _) => "History smooth: 60 ms [F8]",
        }
    }
    pub fn delay_ns(self) -> Option<u128> {
        match self {
            Self::History25
            | Self::HistoryRivals25
            | Self::HistoryRivals40
            | Self::BirdVelocity
            | Self::BirdHistory25
            | Self::HeadHistory60
            | Self::HeadNative60
            | Self::HistorySwim25 => Some(25_000_000),
            Self::History35 => Some(35_000_000),
            Self::History40 => Some(40_000_000),
            Self::History60 => Some(60_000_000),
            _ => None,
        }
    }
    pub fn rival_delay_ns(self) -> Option<u128> {
        match self {
            Self::HistoryRivals25 => Some(25_000_000),
            Self::HistoryRivals40
            | Self::HistorySwim25
            | Self::BirdVelocity
            | Self::BirdHistory25
            | Self::HeadHistory60
            | Self::HeadNative60 => Some(40_000_000),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Blend {
    And,
    Or,
    Zero,
    Opaque,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Forecast {
    pub seconds: f64,
    pub delta: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layer {
    pub id: u8,
    pub native: [i32; 2],
    pub position: [f64; 2],
    pub dimensions: Dimensions,
    pub pixels: Vec<u8>,
    pub blend: Blend,
    /// Keep an actor outside the current smoothing scope at its native placement.
    pub stepped: bool,
    /// Track an optional actor without changing its accepted native presentation.
    pub native_by_default: bool,
    /// Furnished-room cat: settle promptly and discard subpixel stopped drift.
    pub indoor_cat: bool,
    /// Artwork-relative correction shared by stepped and smooth composition.
    pub pose_offset: f64,
    /// Sprite-origin limits applied after motion and artwork registration.
    pub paint_x_bounds: Option<[f64; 2]>,
    pub order: u64,
    pub parent: Option<u8>,
    pub epoch: u64,
    pub cloth: Option<ClothBand>,
    /// Accepted actor-update serial, including unchanged/stationary poses.
    pub observation: u64,
    pub forecast: Option<Forecast>,
    /// Retain a logical anchor and estimate this actor's accepted-update cadence.
    pub cadenced: bool,
    /// Integrate visual velocity across uneven actor updates.
    pub smooth_velocity: bool,
    /// Already decoded native displacement per accepted update.
    pub stride: [f64; 2],
    /// Known actor-ring regime; seeds cadence and invalidates an old regime.
    pub cadence_hint: Option<f64>,
    /// Fish/eel direction changes should never coast against their new facing.
    pub abrupt_turns: bool,
    /// Nonzero opts into retaining the native displacement on a facing reversal.
    pub turn_direction: i8,
    /// A native stop decelerates to rest without recoiling to an older pose.
    pub coast_stops: bool,
    /// Stationary axes settle onto their native coordinate without passing it.
    pub settle_stops: [bool; 2],
    /// Response time when reconciling a stationary axis to its native anchor.
    pub stop_response: f64,
    /// Logical movement limits, independent of the sprite's rectangular size.
    pub bounds: Option<([f64; 2], [f64; 2])>,
    /// A fixed world clipping plane, independent of the moving sprite's shape.
    pub clip_bottom: i32,
}
impl Layer {
    pub(crate) fn new(
        id: u8,
        offset: u16,
        dimensions: Dimensions,
        pixels: Vec<u8>,
        blend: Blend,
    ) -> Self {
        let bank = usize::from(offset) & 0x1fff;
        let native = [
            ((bank % 80) * 4) as i32,
            ((bank / 80) * 2 + usize::from(offset & 0x2000 != 0)) as i32,
        ];
        Self {
            id,
            native,
            position: native.map(f64::from),
            dimensions,
            pixels,
            blend,
            stepped: false,
            native_by_default: false,
            indoor_cat: false,
            pose_offset: 0.0,
            paint_x_bounds: None,
            order: 0,
            parent: None,
            epoch: 0,
            cloth: None,
            observation: 0,
            forecast: None,
            cadenced: false,
            smooth_velocity: false,
            stride: [0.0; 2],
            cadence_hint: None,
            abrupt_turns: false,
            turn_direction: 0,
            coast_stops: false,
            settle_stops: [false; 2],
            stop_response: 0.025,
            bounds: None,
            clip_bottom: HEIGHT as i32,
        }
    }
    fn paint_origin(&self, mut at: [f64; 2]) -> [f64; 2] {
        if let Some([left, right]) = self.paint_x_bounds {
            at[0] = at[0].clamp(left, right);
        }
        at
    }
    fn paint(&self, dst: &mut [u8], at: [f64; 2]) {
        let at = self.paint_origin(at);
        if let Some(cloth) = &self.cloth {
            cloth.paint(dst, self.native[1] as usize, at[0]);
            return;
        }
        let [left, top] = at.map(|x| x.round() as i32);
        let width = self.dimensions.width_bytes() * 4;
        for y in 0..self.dimensions.rows() {
            let sy = top + y as i32;
            if !(0..self.clip_bottom.min(HEIGHT as i32)).contains(&sy) {
                continue;
            }
            for x in 0..width {
                let sx = left + x as i32;
                if !(0..WIDTH as i32).contains(&sx) {
                    continue;
                }
                let source = (self.pixels[y * self.dimensions.width_bytes() + x / 4]
                    >> (6 - 2 * (x & 3)))
                    & 3;
                let dest = &mut dst[sy as usize * WIDTH + sx as usize];
                match self.blend {
                    Blend::And => *dest &= source,
                    Blend::Or => *dest |= source,
                    Blend::Zero if source != 0 => *dest = source,
                    Blend::Opaque => *dest = source,
                    _ => {}
                }
            }
        }
    }
}
#[derive(Clone)]
pub struct Publication {
    pub(crate) native: Framebuffer,
    pub(crate) background: Framebuffer,
    pub(crate) colors: [[u8; 4]; 4],
    pub(crate) layers: Vec<Layer>,
    pub(crate) scene: u16,
    pub(crate) supported: bool,
    pub(crate) coherent_water: bool,
    /// Non-success Hearts return, including its wipe and landing hold.
    pub(crate) hearts_presentation: bool,
}
impl Publication {
    fn indices(&self, positions: impl Fn(&Layer) -> [f64; 2]) -> Vec<u8> {
        let mut pixels = self.background.indices();
        for layer in &self.layers {
            layer.paint(&mut pixels, positions(layer))
        }
        pixels
    }
    fn placed_indices(&self, positions: impl Fn(&Layer) -> [f64; 2]) -> Vec<u8> {
        self.indices(|layer| {
            let mut at = positions(layer);
            at[0] += layer.pose_offset;
            at
        })
    }
    fn native_position(layer: &Layer) -> [f64; 2] {
        if layer.cloth.is_some() {
            layer.position
        } else {
            layer.native.map(f64::from)
        }
    }
    /// Correct walking placement at native update positions, without interpolation.
    pub fn placed_rgba(&self) -> Vec<u8> {
        let indices = if self.layers.iter().any(|layer| layer.pose_offset != 0.0)
            && self.native_composition_matches()
        {
            self.placed_indices(Self::native_position)
        } else {
            self.native.indices()
        };
        indices
            .into_iter()
            .flat_map(|i| self.colors[usize::from(i)])
            .collect()
    }
    /// Original placements must reproduce every native pixel before smoothing.
    pub fn native_composition_matches(&self) -> bool {
        self.supported && self.indices(Self::native_position) == self.native.indices()
    }
}

#[derive(Clone, Debug)]
struct Track {
    layer: Layer,
    source_at: f64,
    seen: f64,
    start: [f64; 2],
    velocity: [f64; 2],
    horizon: f64,
    start_velocity: [f64; 2],
    intervals: VecDeque<f64>,
}
impl Track {
    fn velocity_horizon(layer: &Layer, cadence: f64) -> f64 {
        (cadence * 3.0).clamp(0.120, if layer.cloth.is_some() { 0.400 } else { 0.200 })
    }
    fn velocity_state(&self, age: f64) -> ([f64; 2], [f64; 2]) {
        // Integrate a filtered velocity rather than chasing each stepped pose.
        // Retain a finite coast if publications stop, with no hard cutoff.
        let response = 0.040;
        let t = age.min(self.horizon);
        let decay = (-t / response).exp();
        let mut position = [0.0; 2];
        let mut velocity = [0.0; 2];
        for i in 0..2 {
            let fixed_support = if self.layer.coast_stops && i == 1 {
                self.layer
                    .bounds
                    .and_then(|(low, high)| (low[i] == high[i]).then_some(low[i]))
            } else {
                None
            };
            let stopped_axis = self.layer.settle_stops[i] && self.layer.stride[i] == 0.0;
            let crossed_limit = if self.layer.settle_stops[i] {
                self.layer.bounds.and_then(|(low, high)| {
                    if self.start[i] < low[i] && self.layer.position[i] >= low[i] {
                        Some(low[i])
                    } else if self.start[i] > high[i] && self.layer.position[i] <= high[i] {
                        Some(high[i])
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            if let Some(target) = if stopped_axis {
                Some(self.layer.position[i])
            } else {
                fixed_support.or(crossed_limit)
            } {
                // Keep position continuous while reaching a native support or
                // stationary column. If a new landing limit is already crossed,
                // approach it continuously instead of hard-clamping the image.
                // Old velocity cannot carry past the destination.
                let error = self.start[i] - target;
                let response = if stopped_axis {
                    self.layer.stop_response
                } else if self.layer.indoor_cat {
                    0.025
                } else {
                    0.050
                };
                let toward = -error / response;
                let initial = self.start_velocity[i].clamp(toward.min(0.0), toward.max(0.0));
                let tail = initial + error / response;
                let decay = (-age / response).exp();
                position[i] = target + (error + tail * age) * decay;
                velocity[i] = (initial - tail * age / response) * decay;
                continue;
            }
            let requested = self.velocity[i];
            let mut v = requested + (self.start_velocity[i] - requested) * decay;
            let mut x = self.start[i]
                + requested * t
                + (self.start_velocity[i] - requested) * response * (1.0 - decay);
            if age > t {
                let coast = (-(age - t) / response).exp();
                x += v * response * (1.0 - coast);
                v *= coast;
            }
            if let Some((low, high)) = self.layer.bounds {
                if x <= low[i] || x >= high[i] {
                    v = 0.0;
                }
                x = x.clamp(low[i], high[i]);
            }
            position[i] = x;
            velocity[i] = v;
        }
        (position, velocity)
    }
    fn sample(&self, now: f64) -> [f64; 2] {
        let age = (now - self.seen).max(0.0);
        if self.layer.smooth_velocity {
            return self.velocity_state(age).0;
        }
        let t = age.min(self.horizon);
        let response = 0.012;
        std::array::from_fn(|i| {
            let target = self.layer.position[i];
            let velocity = self.velocity[i];
            let x = target
                + velocity * (t - response)
                + (self.start[i] - target + velocity * response) * (-t / response).exp();
            let x = if age > t && self.layer.forecast.is_none() && !self.layer.cadenced {
                target + (x - target) * (-(age - t) / response).exp()
            } else {
                x
            };
            self.layer
                .bounds
                .map_or(x, |(low, high)| x.clamp(low[i], high[i]))
        })
    }
    fn new(layer: Layer, source_at: f64, seen: f64) -> Self {
        let start = layer.position;
        let (mut velocity, mut horizon) = layer.forecast.map_or(([0.0; 2], 0.0), |f| {
            (f.delta.map(|d| d / f.seconds), f.seconds)
        });
        if layer.smooth_velocity
            && let Some(period) = layer.cadence_hint
        {
            velocity = layer.stride.map(|step| step / period);
            horizon = Self::velocity_horizon(&layer, period);
        }
        Self {
            layer,
            source_at,
            seen,
            start,
            velocity,
            horizon,
            start_velocity: [0.0; 2],
            intervals: VecDeque::new(),
        }
    }
    fn update(&mut self, layer: Layer, source_at: f64, seen: f64) {
        if layer.stepped {
            *self = Self::new(layer, source_at, seen);
            return;
        }
        if self.layer == layer {
            return;
        }
        let dt = source_at - self.source_at;
        let delta = std::array::from_fn::<_, 2, _>(|i| layer.position[i] - self.layer.position[i]);
        let distance = delta[0].hypot(delta[1]);
        // Aquatic animation frames share a logical anchor even when the pose
        // changes its rectangle. Artwork changes do not start a new lifetime.
        let changed_shape = layer.dimensions != self.layer.dimensions && !layer.cadenced;
        let reset = dt <= 0.0
            || dt > 0.25
            || distance > 20.0
            || changed_shape
            || layer.parent != self.layer.parent
            || layer.epoch != self.layer.epoch
            || layer.smooth_velocity != self.layer.smooth_velocity;
        if reset {
            *self = Self::new(layer, source_at, seen);
            return;
        }
        if layer.smooth_velocity {
            let (mut start, mut start_velocity) = self.velocity_state((seen - self.seen).max(0.0));
            if layer.turn_direction != 0
                && self.layer.turn_direction == -layer.turn_direction
                && layer.observation != self.layer.observation
            {
                // A walking reversal can include a real native step on the
                // pose-change update. Retain that displacement immediately,
                // carrying the existing visual offset instead of amplifying it.
                // If earlier prediction already reached/passed the new anchor,
                // another turn step would count that movement twice. Only move
                // toward the new native position, by at most its actual step.
                let remaining = layer.position[0] - start[0];
                let step = if delta[0] > 0.0 {
                    delta[0].min(remaining.max(0.0))
                } else {
                    delta[0].max(remaining.min(0.0))
                };
                start[0] += step;
                start_velocity[0] = 0.0;
            }
            if layer.abrupt_turns {
                for (i, speed) in start_velocity.iter_mut().enumerate() {
                    if layer.stride[i] * self.layer.stride[i] < 0.0
                        || (layer.coast_stops && layer.stride[i] * *speed < 0.0)
                    {
                        *speed = 0.0;
                    }
                }
            }
            let mut intervals = std::mem::take(&mut self.intervals);
            let changed_cadence = if layer.cloth.is_some() {
                match (layer.cadence_hint, self.layer.cadence_hint) {
                    (Some(new), Some(old)) => new > old * 1.5 || old > new * 1.5,
                    _ => false,
                }
            } else {
                layer.cadence_hint != self.layer.cadence_hint
            };
            if changed_cadence {
                intervals.clear();
            }
            if intervals.is_empty() {
                // A BIOS-tick prior avoids deriving an extreme launch velocity
                // from one short same-tick acceptance. Real observations replace it.
                let tick =
                    crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
                intervals.extend([layer.cadence_hint.unwrap_or(tick); 8]);
            }
            intervals.pop_front();
            let longest_interval = if layer.cloth.is_some() {
                0.250
            } else if layer.cadence_hint.is_some() {
                0.200
            } else {
                0.150
            };
            intervals.push_back(dt.clamp(0.012, longest_interval));
            let observed = intervals.iter().sum::<f64>() / intervals.len() as f64;
            let cadence = if layer.indoor_cat {
                // Short countdown-expiry bursts cannot sustain more than the
                // gate's two updates per BIOS tick. Do not turn a partial
                // cycle into an extra visual acceleration at startup.
                observed.max(layer.cadence_hint.unwrap_or(0.0))
            } else {
                observed
            };
            let velocity = std::array::from_fn(|i| {
                let base = layer.stride[i] / cadence;
                // Retain two pixels of extra runway once garment history permits
                // it. Otherwise a slightly late scroll exhausts the known strip,
                // clamps velocity to zero, and visibly restarts it next update.
                let runway = layer.cloth.as_ref().map_or(0.0, |cloth| {
                    if i == 0 && cloth.stride >= WIDTH + 8 {
                        cloth.direction * 2.0
                    } else {
                        0.0
                    }
                });
                let error = layer.position[i] - runway - start[i];
                if base == 0.0 {
                    if layer.coast_stops {
                        0.0
                    } else {
                        // A genuine stop still reconciles any small visual offset.
                        (error / 0.18).clamp(-40.0, 40.0)
                    }
                } else {
                    // Correct drift slowly, without turning irregular pose
                    // arrivals into repeated visible acceleration/braking.
                    let correction = (error / 0.30).clamp(-base.abs() * 0.12, base.abs() * 0.12);
                    (base + correction).clamp(-400.0, 400.0)
                }
            });
            let horizon = Self::velocity_horizon(&layer, cadence);
            *self = Self {
                layer,
                source_at,
                seen,
                start,
                start_velocity,
                velocity,
                horizon,
                intervals,
            };
            return;
        }
        if let Some(forecast) = layer.forecast {
            // Slow and accelerating actors keep moving across their own gates.
            // Never spring back toward an already passed native observation.
            let start = self.sample(seen);
            let velocity = forecast.delta.map(|d| d / forecast.seconds);
            *self = Self {
                layer,
                source_at,
                seen,
                start,
                velocity,
                horizon: forecast.seconds,
                start_velocity: [0.0; 2],
                intervals: VecDeque::new(),
            };
            return;
        }
        let mut velocity = delta.map(|d| d / dt.max(0.012));
        let reversed = (0..2).any(|i| velocity[i] * self.velocity[i] < 0.0);
        let start = if reversed {
            layer.position
        } else {
            self.sample(seen)
        };
        if reversed {
            velocity = [0.0; 2]
        }
        for v in &mut velocity {
            *v = v.clamp(-400.0, 400.0)
        }
        let max_velocity = velocity[0].abs().max(velocity[1].abs());
        let horizon = if max_velocity == 0.0 {
            0.0
        } else if layer.cadenced {
            // Fish traverse the actor ring at different rates above/below the
            // PCjr surface gate. Swimmers have their own acceptance cadence.
            // Observe that interval rather than expiring every track at 40 ms.
            dt.clamp(0.025, 0.150)
        } else {
            (4.0 / max_velocity).min(0.040)
        };
        *self = Self {
            layer,
            source_at,
            seen,
            start,
            velocity,
            horizon,
            start_velocity: [0.0; 2],
            intervals: VecDeque::new(),
        };
    }
}

/// Read-only diagnostic sample for private prototype comparisons.
#[derive(Clone, Debug)]
pub struct MotionSample {
    pub actor: u8,
    pub target: [f64; 2],
    pub drawn: [f64; 2],
    pub velocity: [f64; 2],
    pub horizon: f64,
    pub dimensions: u16,
    /// Added by sprite composition; it never enters the velocity estimate.
    pub pose_offset: f64,
}

/// Observer and compositor; its state never enters the game, RNG, or audio paths.
#[derive(Clone, Default)]
pub struct Presenter {
    latest: Option<Publication>,
    tracks: BTreeMap<u8, Track>,
    signature: Option<(u16, u16, u8, u64, u32, bool)>,
    last_at: f64,
    pub observations: u64,
    pub compatible: u64,
}
impl Presenter {
    fn position(&self, layer: &Layer, now: f64) -> [f64; 2] {
        if layer.native_by_default {
            return layer.native.map(f64::from);
        }
        self.tracked_position(layer, now)
    }
    fn tracked_position(&self, layer: &Layer, now: f64) -> [f64; 2] {
        if layer.stepped {
            return layer.position;
        }
        let mut at = self
            .tracks
            .get(&layer.id)
            .map_or(layer.position, |t| t.sample(now));
        if let Some(cloth) = &layer.cloth {
            at[0] -= cloth.direction * 4.0;
        }
        if let Some(parent) = layer.parent.and_then(|id| self.tracks.get(&id)) {
            let direction = parent.layer.cloth.as_ref().map_or(0.0, |c| c.direction);
            at[0] += parent.sample(now)[0] - direction * 4.0;
        }
        at
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn is_active(&self) -> bool {
        !self.tracks.is_empty()
    }
    /// Source timestamps estimate velocity; `seen_ns` is when this batch is available.
    pub fn observe(
        &mut self,
        game: &Alley,
        source_ns: u128,
        seen_ns: u128,
        moving: bool,
    ) -> Result<(), String> {
        let source = source_ns as f64 / 1e9;
        let seen = seen_ns as f64 / 1e9;
        if source < self.last_at {
            self.reset()
        }
        self.last_at = source;
        let signature = game.motion_signature(moving);
        if self.signature == Some(signature)
            && self.latest.as_ref().is_some_and(|p| {
                p.native.bytes() == game.framebuffer().bytes() && p.colors == game.palette()
            })
        {
            return Ok(());
        }
        self.signature = Some(signature);
        let publication = game.motion_publication()?;
        self.publish(publication, source, seen, moving);
        Ok(())
    }
    fn publish(&mut self, publication: Publication, source: f64, seen: f64, moving: bool) {
        self.observations += 1;
        if (!moving && !publication.hearts_presentation)
            || !publication.supported
            || (!publication.coherent_water && !publication.native_composition_matches())
        {
            self.tracks.clear();
            self.latest = Some(publication);
            return;
        }
        self.compatible += 1;
        if self
            .latest
            .as_ref()
            .is_none_or(|p| p.scene != publication.scene)
        {
            self.tracks.clear();
        }
        // Keep the cat's displayed world pose before removing old garment tracks
        // or changing from world coordinates to a garment-relative anchor.
        let player_world = self.tracks.get(&0).map(|track| {
            let mut velocity = track.velocity_state((seen - track.seen).max(0.0)).1;
            let mut target = track.layer.position;
            if let Some(parent) = track.layer.parent.and_then(|id| self.tracks.get(&id)) {
                velocity[0] += parent.velocity_state((seen - parent.seen).max(0.0)).1[0];
                target[0] += parent.layer.position[0];
            }
            (self.position(&track.layer, seen), velocity, target)
        });
        self.tracks
            .retain(|id, _| publication.layers.iter().any(|l| l.id == *id));
        for layer in &publication.layers {
            if layer.id == 0 && layer.coast_stops {
                let parent = layer.parent.and_then(|id| self.tracks.get(&id));
                let (parent_x, parent_velocity, parent_target) =
                    parent.map_or((0.0, 0.0, 0.0), |p| {
                        (
                            self.position(&p.layer, seen)[0],
                            p.velocity_state((seen - p.seen).max(0.0)).1[0],
                            p.layer.position[0],
                        )
                    });
                if let Some(track) = self.tracks.get_mut(&0)
                    && track.layer.coast_stops
                    && (track.layer.parent != layer.parent || track.layer.epoch != layer.epoch)
                    && let Some((position, velocity, target)) = player_world
                {
                    track.start = [position[0] - parent_x, position[1]];
                    track.start_velocity = [velocity[0] - parent_velocity, velocity[1]];
                    track.layer.position = [target[0] - parent_target, target[1]];
                    track.layer.parent = layer.parent;
                    track.layer.epoch = layer.epoch;
                    track.layer.bounds = layer.bounds;
                    track.seen = seen;
                }
            }
            self.tracks
                .entry(layer.id)
                .and_modify(|t| t.update(layer.clone(), source, seen))
                .or_insert_with(|| Track::new(layer.clone(), source, seen));
        }
        self.latest = Some(publication);
    }
    pub fn samples(&self, now_ns: u128) -> Vec<MotionSample> {
        let now = now_ns as f64 / 1e9;
        self.tracks
            .values()
            .map(|t| MotionSample {
                actor: t.layer.id,
                target: {
                    let mut target = t.layer.position;
                    if let Some(parent) = t.layer.parent.and_then(|id| self.tracks.get(&id)) {
                        target[0] += parent.layer.position[0];
                    }
                    target
                },
                drawn: self.position(&t.layer, now),
                velocity: t.velocity,
                horizon: t.horizon,
                dimensions: t.layer.dimensions.0,
                pose_offset: t.layer.pose_offset,
            })
            .collect()
    }
    pub fn rgba(&self, now_ns: u128) -> Option<Vec<u8>> {
        let p = self.latest.as_ref()?;
        let now = now_ns as f64 / 1e9;
        let indices = if self.tracks.is_empty() {
            p.native.indices()
        } else {
            p.placed_indices(|l| self.position(l, now))
        };
        Some(
            indices
                .into_iter()
                .flat_map(|i| p.colors[usize::from(i)])
                .collect(),
        )
    }
    pub fn stepped_rgba(&self) -> Option<Vec<u8>> {
        let p = self.latest.as_ref()?;
        if self.is_active() {
            Some(p.placed_rgba())
        } else {
            Some(
                p.native
                    .indices()
                    .into_iter()
                    .flat_map(|i| p.colors[usize::from(i)])
                    .collect(),
            )
        }
    }
}
