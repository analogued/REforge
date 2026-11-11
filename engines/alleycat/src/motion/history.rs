// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed-delay presentation: player positions interpolate actual observations.
//!
//! Scenery, poses and other actors use the same delayed publication timeline.
//! Rivals use a separate observation delay. Other actors retain their velocity
//! smoothers; swimming history remains a development comparison. No game execution
//! is delayed.

use super::{Layer, Presenter};
use crate::alley::Alley;
use std::collections::VecDeque;

const HISTORY_SECONDS: f64 = 0.250;
const MAX_FRAMES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BirdPolicy {
    Native,
    Velocity,
    History,
}

#[derive(Clone, Copy, Debug)]
pub struct BirdSample {
    pub native: [f64; 2],
    pub velocity: [f64; 2],
    pub history: [f64; 2],
    pub observation: u64,
    pub bracketed: bool,
    pub dimensions: u16,
}

#[derive(Clone)]
struct Frame {
    at: f64,
    presenter: Presenter,
}

/// Diagnostics for the noninteractive falling head, on its 60 ms clock.
#[derive(Clone, Copy, Debug)]
pub struct FallingHeadSample {
    pub native: [f64; 2],
    pub history: [f64; 2],
    pub bracketed: bool,
    pub interval_ms: f64,
}

/// Read-only measurements at the delayed presentation time, before pose offset.
#[derive(Clone, Copy, Debug)]
pub struct PlayerSample {
    pub at_ns: u128,
    pub drawn: [f64; 2],
    /// Existing predictive placement at exactly the same delayed scene time.
    pub predicted: [f64; 2],
    pub dimensions: u16,
    pub pose_offset: f64,
    pub observation: u64,
    /// This segment uses interpolation between available accepted states.
    /// False denotes either a deliberate event cut or a missing endpoint.
    pub bracketed: bool,
    pub interval_ms: f64,
    pub moving: bool,
}

/// Rival diagnostics at one shared delayed time, for comparing both policies.
#[derive(Clone, Copy, Debug)]
pub struct RivalSample {
    pub actor: u8,
    pub native: [f64; 2],
    pub predicted: [f64; 2],
    pub history: [f64; 2],
    pub pose_offset: f64,
    pub observation: u64,
    pub bracketed: bool,
    pub interval_ms: f64,
}

/// The pose and position actually selected by the separate rival-clock trial.
#[derive(Clone, Copy, Debug)]
pub struct RivalTimingSample {
    pub actor: u8,
    pub drawn: [f64; 2],
    /// Sprite origin after pose registration and field-boundary limits.
    pub painted: [f64; 2],
    pub pose_offset: f64,
    pub observation: u64,
    pub effective_delay_ns: u128,
    /// A visibility/lifetime boundary kept this rival on the main timeline.
    pub boundary_fallback: bool,
}

struct RivalView<'a> {
    layer: &'a Layer,
    position: [f64; 2],
    delay_ns: u128,
    boundary_fallback: bool,
}

/// Bounded actual-position history with per-actor presentation policies.
#[derive(Default)]
pub struct HistoryPresenter {
    base: Presenter,
    frames: VecDeque<Frame>,
    player_observation: Option<u64>,
}

impl HistoryPresenter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.base.is_active()
    }

    pub fn observe(
        &mut self,
        game: &Alley,
        source_ns: u128,
        seen_ns: u128,
        moving: bool,
    ) -> Result<(), String> {
        let source = source_ns as f64 / 1e9;
        let seen = seen_ns as f64 / 1e9;
        if source < self.base.last_at || self.frames.back().is_some_and(|frame| seen < frame.at) {
            self.reset();
        }
        let before = self.base.observations;
        self.base.observe(game, source_ns, seen_ns, moving)?;
        let observation = game.motion_player_observation();
        // A stopped/blocked acceptance can retain exactly the same framebuffer.
        // It still bounds an interpolation interval: omitting it would smear a
        // later restart backwards across the whole stationary period.
        let stationary_acceptance = self.player_observation != Some(observation);
        self.player_observation = Some(observation);
        if self.base.observations != before || stationary_acceptance {
            let mut snapshot = self.base.clone();
            if self.base.observations == before {
                snapshot.latest = Some(game.motion_publication()?);
            }
            // History is on the display-availability clock, not the execution
            // cost clock. A blocking pass can publish its picture before its
            // charged end time. Several passes in one host batch have just one
            // visible endpoint: retain that batch's final state at this time.
            self.record(snapshot, seen);
        }
        Ok(())
    }

    fn record(&mut self, snapshot: Presenter, seen: f64) {
        if self.frames.back().is_some_and(|frame| frame.at == seen) {
            self.frames.pop_back();
        }
        self.frames.push_back(Frame {
            at: seen,
            presenter: snapshot,
        });
        // Retain the predecessor at the history boundary as well.
        while self.frames.len() > 2
            && (self.frames[1].at < seen - HISTORY_SECONDS || self.frames.len() > MAX_FRAMES)
        {
            self.frames.pop_front();
        }
    }

    fn frame_at(&self, now_ns: u128, delay_ns: u128) -> Option<(usize, f64)> {
        let now = now_ns as f64 / 1e9;
        let at = now_ns.saturating_sub(delay_ns) as f64 / 1e9;
        let index = self
            .frames
            .iter()
            .rposition(|frame| frame.at <= at && frame.at <= now)
            .or_else(|| self.frames.iter().position(|frame| frame.at <= now))?;
        Some((index, at.max(self.frames[index].at)))
    }

    fn player_layer(frame: &Frame) -> Option<&Layer> {
        Self::actor_layer(frame, 0)
    }

    fn actor_layer(frame: &Frame, actor: u8) -> Option<&Layer> {
        let p = frame.presenter.latest.as_ref()?;
        let eligible = match actor {
            0 => true,
            43 => p.scene == 5,
            48..=54 => p.scene == 7,
            60 => p.hearts_presentation,
            _ => false,
        };
        if !frame.presenter.is_active() || !eligible {
            return None;
        }
        p.layers
            .iter()
            .find(|layer| layer.id == actor && layer.cadenced && !layer.stepped)
    }

    fn same_context(a: &Frame, b: &Frame, left: &Layer, right: &Layer) -> bool {
        a.presenter.latest.as_ref().map(|p| p.scene) == b.presenter.latest.as_ref().map(|p| p.scene)
            && left.parent == right.parent
            && left.epoch == right.epoch
    }

    fn local_player(
        &self,
        index: usize,
        at: f64,
        now: f64,
        swimming: bool,
    ) -> Option<([f64; 2], bool, f64)> {
        if !swimming && self.frames[index].presenter.latest.as_ref()?.scene == 2 {
            return None;
        }
        self.local_actor(index, at, now, 0)
    }

    fn local_actor(
        &self,
        index: usize,
        at: f64,
        now: f64,
        actor: u8,
    ) -> Option<([f64; 2], bool, f64)> {
        let frame = &self.frames[index];
        let layer = Self::actor_layer(frame, actor)?;
        let mut start = index;
        while start > 0 {
            let earlier = &self.frames[start - 1];
            let Some(old) = Self::actor_layer(earlier, actor) else {
                break;
            };
            if !Self::same_context(frame, earlier, layer, old)
                || old.observation != layer.observation
                || old.position != layer.position
            {
                break;
            }
            start -= 1;
        }
        for later in self.frames.iter().skip(index + 1) {
            if later.at > now {
                break;
            }
            let Some(next) = Self::actor_layer(later, actor) else {
                break;
            };
            if !Self::same_context(frame, later, layer, next) {
                break;
            }
            if next.observation == layer.observation && next.position == layer.position {
                continue;
            }
            let delta =
                std::array::from_fn::<_, 2, _>(|axis| next.position[axis] - layer.position[axis]);
            // Retained/cropped entry and explicit large relocations are events,
            // not curves across the room. Parent/scene changes are also cuts.
            if delta.iter().any(|distance| distance.abs() > 32.0) {
                break;
            }
            let duration = later.at - self.frames[start].at;
            if duration <= 0.0 {
                break;
            }
            let phase = ((at - self.frames[start].at) / duration).clamp(0.0, 1.0);
            if actor == 43 {
                // Bird axes stop/turn independently at waypoints. An X turn
                // must not freeze unrelated vertical flight, or vice versa.
                let allowed: [bool; 2] = std::array::from_fn(|axis| {
                    delta[axis] == 0.0
                        || (layer.stride[axis] != 0.0
                            && delta[axis].signum() == layer.stride[axis].signum()
                            && !(axis == 0
                                && next.turn_direction != 0
                                && next.turn_direction != layer.turn_direction))
                });
                return Some((
                    std::array::from_fn(|axis| {
                        layer.position[axis]
                            + if allowed[axis] {
                                phase * delta[axis]
                            } else {
                                0.0
                            }
                    }),
                    allowed
                        .iter()
                        .zip(delta)
                        .any(|(ok, delta)| *ok && delta != 0.0),
                    duration * 1e3,
                ));
            }
            // The preceding pose belongs to this interval. A new start or
            // reversal must not translate its old idle/facing artwork. Retain
            // that one native event as a cut; subsequent matching motion is
            // interpolated normally. Apex/wall direction changes are likewise
            // at most a native step, never an invented ascent or long recoil.
            let pose_cut = if actor == 0 {
                delta.iter().zip(layer.stride).any(|(distance, stride)| {
                    *distance != 0.0 && (stride == 0.0 || distance.signum() != stride.signum())
                })
            } else if actor == 60 {
                // The fixed falling-head path has no walking/facing events.
                delta != [0.0, 12.0]
            } else {
                // Rival forecast strides can be suppressed near the player
                // without changing the walking pose. Use its actual facing,
                // not that predictive contact restriction. Sitting/recoil,
                // reversal and row changes retain their native event step.
                // A boundary can advance outward while selecting the inward
                // picture: keep that last step with the destination pose too.
                delta[1] != 0.0
                    || (delta[0] != 0.0
                        && (layer.turn_direction == 0
                            || (next.turn_direction != 0
                                && next.turn_direction != layer.turn_direction)
                            || delta[0].signum() != f64::from(layer.turn_direction)))
            };
            if pose_cut {
                break;
            }
            return Some((
                std::array::from_fn(|axis| layer.position[axis] + phase * delta[axis]),
                true,
                duration * 1e3,
            ));
        }
        // An exhausted buffer holds the last real position. No extrapolation,
        // spring or subsequently repaid position error is hidden in this trial.
        Some((layer.position, false, 0.0))
    }

    fn player_world(&self, index: usize, at: f64, local: [f64; 2]) -> [f64; 2] {
        let frame = &self.frames[index];
        let Some(layer) = Self::player_layer(frame) else {
            return local;
        };
        let mut position = local;
        if let Some(parent) = layer.parent.and_then(|id| frame.presenter.tracks.get(&id)) {
            // Keep an attached child on the same delayed moving support. The
            // native relative coordinate and the support's smooth coordinate
            // are distinct; neither is transformed twice at a transfer.
            position[0] += frame.presenter.position(&parent.layer, at)[0];
        }
        position
    }

    pub fn player_sample(&self, now_ns: u128, delay_ns: u128) -> Option<PlayerSample> {
        self.player_sample_scope(now_ns, delay_ns, false)
    }

    /// Actual swimmer history; ordinary player diagnostics remain unchanged.
    pub fn swimmer_sample(&self, now_ns: u128, delay_ns: u128) -> Option<PlayerSample> {
        let (index, _) = self.frame_at(now_ns, delay_ns)?;
        if self.frames[index].presenter.latest.as_ref()?.scene != 2 {
            return None;
        }
        self.player_sample_scope(now_ns, delay_ns, true)
    }

    fn player_sample_scope(
        &self,
        now_ns: u128,
        delay_ns: u128,
        swimming: bool,
    ) -> Option<PlayerSample> {
        let (index, at) = self.frame_at(now_ns, delay_ns)?;
        let layer = Self::player_layer(&self.frames[index])?;
        let (local, bracketed, interval_ms) =
            self.local_player(index, at, now_ns as f64 / 1e9, swimming)?;
        Some(PlayerSample {
            at_ns: (at * 1e9).round() as u128,
            drawn: self.player_world(index, at, local),
            predicted: self.frames[index].presenter.position(layer, at),
            dimensions: layer.dimensions.0,
            pose_offset: layer.pose_offset,
            observation: layer.observation,
            bracketed,
            interval_ms,
            moving: layer.stride != [0.0; 2],
        })
    }

    pub fn rgba(&self, now_ns: u128, delay_ns: u128) -> Option<Vec<u8>> {
        self.rgba_scope(now_ns, delay_ns, false, false, BirdPolicy::Native)
    }

    /// Compare actual rival history with the existing player-only history view.
    pub fn rgba_with_rivals(&self, now_ns: u128, delay_ns: u128) -> Option<Vec<u8>> {
        self.rgba_scope(now_ns, delay_ns, true, false, BirdPolicy::Native)
    }

    fn rival_view(
        &self,
        base_index: usize,
        base_at: f64,
        now_ns: u128,
        base_delay_ns: u128,
        rival_delay_ns: u128,
        actor: u8,
    ) -> Option<RivalView<'_>> {
        let base_frame = &self.frames[base_index];
        let base_layer = Self::actor_layer(base_frame, actor)?;
        let unified = || {
            self.local_actor(base_index, base_at, now_ns as f64 / 1e9, actor)
                .map(|(position, _, _)| RivalView {
                    layer: base_layer,
                    position,
                    delay_ns: base_delay_ns,
                    boundary_fallback: rival_delay_ns != base_delay_ns,
                })
        };
        if rival_delay_ns == base_delay_ns {
            return unified();
        }
        let Some((index, at)) = self.frame_at(now_ns, rival_delay_ns) else {
            return unified();
        };
        let Some(layer) = Self::actor_layer(&self.frames[index], actor) else {
            return unified();
        };
        // The main publication owns which sprites have been safely peeled
        // from its background. Never inject an older rival absent from that
        // roster, cross a disappearance, or resurrect a previous lifetime.
        if index > base_index
            || (index..=base_index).any(|i| {
                Self::actor_layer(&self.frames[i], actor).is_none_or(|candidate| {
                    !Self::same_context(base_frame, &self.frames[i], base_layer, candidate)
                })
            })
        {
            return unified();
        }
        let (position, _, _) = self.local_actor(index, at, now_ns as f64 / 1e9, actor)?;
        Some(RivalView {
            layer,
            position,
            delay_ns: rival_delay_ns,
            boundary_fallback: false,
        })
    }

    /// Delay rival artwork and movement together while retaining the main scene clock.
    pub fn rgba_with_rival_delay(
        &self,
        now_ns: u128,
        main_delay_ns: u128,
        rival_delay_ns: u128,
    ) -> Option<Vec<u8>> {
        self.rgba_with_rival_delay_scope(
            now_ns,
            main_delay_ns,
            rival_delay_ns,
            false,
            BirdPolicy::Native,
        )
    }

    /// Compare actual swimming history while retaining every other actor policy.
    pub fn rgba_with_swimming(
        &self,
        now_ns: u128,
        main_delay_ns: u128,
        rival_delay_ns: u128,
    ) -> Option<Vec<u8>> {
        self.rgba_with_rival_delay_scope(
            now_ns,
            main_delay_ns,
            rival_delay_ns,
            true,
            BirdPolicy::Native,
        )
    }

    pub fn rgba_with_bird(
        &self,
        now_ns: u128,
        main_delay_ns: u128,
        rival_delay_ns: u128,
        bird: BirdPolicy,
    ) -> Option<Vec<u8>> {
        self.rgba_with_rival_delay_scope(now_ns, main_delay_ns, rival_delay_ns, false, bird)
    }

    fn bird_position(
        &self,
        index: usize,
        at: f64,
        now: f64,
        layer: &Layer,
        policy: BirdPolicy,
    ) -> [f64; 2] {
        let presenter = &self.frames[index].presenter;
        match policy {
            BirdPolicy::Native => layer.native.map(f64::from),
            BirdPolicy::Velocity => presenter.tracked_position(layer, at),
            BirdPolicy::History => self
                .local_actor(index, at, now, 43)
                .map_or(layer.position, |(position, _, _)| position),
        }
    }

    pub fn bird_sample(&self, now_ns: u128, delay_ns: u128) -> Option<BirdSample> {
        let (index, at) = self.frame_at(now_ns, delay_ns)?;
        let layer = Self::actor_layer(&self.frames[index], 43)?;
        let (history, bracketed, _) = self.local_actor(index, at, now_ns as f64 / 1e9, 43)?;
        Some(BirdSample {
            native: layer.native.map(f64::from),
            velocity: self.frames[index].presenter.tracked_position(layer, at),
            history,
            observation: layer.observation,
            bracketed,
            dimensions: layer.dimensions.0,
        })
    }

    /// Keep the complete noninteractive return on one 60 ms timeline. At entry,
    /// hold its first frame rather than rewinding live gameplay. At exit, retain
    /// the delayed ending until its fill completes. Ordinary play stays at 25 ms.
    fn hearts_frame(&self, now_ns: u128) -> Option<(usize, f64)> {
        let older = self.frame_at(now_ns, 60_000_000)?;
        let current = self.frame_at(now_ns, 25_000_000)?;
        let is_ending = |index: usize| {
            self.frames[index]
                .presenter
                .latest
                .as_ref()
                .is_some_and(|p| p.hearts_presentation)
        };
        if is_ending(older.0) {
            Some(older)
        } else if is_ending(current.0) {
            let first = (older.0..=current.0).find(|&index| is_ending(index))?;
            Some((first, self.frames[first].at))
        } else {
            None
        }
    }

    /// Same selected actor policies outside Hearts presentations. Only actor60's
    /// position changes between these two views; artwork, palette and all phase
    /// boundaries share the complete ending's delayed clock.
    pub fn rgba_with_hearts_drop(&self, now_ns: u128, smooth: bool) -> Option<Vec<u8>> {
        let Some((index, at)) = self.hearts_frame(now_ns) else {
            return self.rgba_with_bird(now_ns, 25_000_000, 40_000_000, BirdPolicy::History);
        };
        let p = self.frames[index].presenter.latest.as_ref()?;
        let indices = if smooth && p.supported {
            p.placed_indices(|layer| {
                self.local_actor(index, at, now_ns as f64 / 1e9, layer.id)
                    .map_or(layer.position, |(position, _, _)| position)
            })
        } else {
            p.native.indices()
        };
        Some(
            indices
                .into_iter()
                .flat_map(|i| p.colors[usize::from(i)])
                .collect(),
        )
    }

    /// Falling-head placement on the same delayed ending timeline as its image.
    pub fn falling_head_sample(&self, now_ns: u128) -> Option<FallingHeadSample> {
        let (index, at) = self.hearts_frame(now_ns)?;
        let layer = Self::actor_layer(&self.frames[index], 60)?;
        let (history, bracketed, interval_ms) =
            self.local_actor(index, at, now_ns as f64 / 1e9, 60)?;
        Some(FallingHeadSample {
            native: layer.position,
            history,
            bracketed,
            interval_ms,
        })
    }

    fn rgba_with_rival_delay_scope(
        &self,
        now_ns: u128,
        main_delay_ns: u128,
        rival_delay_ns: u128,
        swimming: bool,
        bird: BirdPolicy,
    ) -> Option<Vec<u8>> {
        if main_delay_ns == rival_delay_ns {
            return self.rgba_scope(now_ns, main_delay_ns, true, swimming, bird);
        }
        let (index, at) = self.frame_at(now_ns, main_delay_ns)?;
        let presenter = &self.frames[index].presenter;
        let p = presenter.latest.as_ref()?;
        if !presenter.is_active() {
            return presenter.rgba(now_ns.saturating_sub(main_delay_ns));
        }
        let player = self
            .local_player(index, at, now_ns as f64 / 1e9, swimming)
            .map(|(local, _, _)| self.player_world(index, at, local));
        let mut indices = p.background.indices();
        for main_layer in &p.layers {
            let rival = if (48..=54).contains(&main_layer.id) {
                self.rival_view(
                    index,
                    at,
                    now_ns,
                    main_delay_ns,
                    rival_delay_ns,
                    main_layer.id,
                )
            } else {
                None
            };
            let (layer, mut position) = if let Some(view) = rival {
                (view.layer, view.position)
            } else {
                let position = if main_layer.id == 0 {
                    player.unwrap_or_else(|| presenter.position(main_layer, at))
                } else if main_layer.id == 43 {
                    self.bird_position(index, at, now_ns as f64 / 1e9, main_layer, bird)
                } else {
                    presenter.position(main_layer, at)
                };
                (main_layer, position)
            };
            position[0] += layer.pose_offset;
            layer.paint(&mut indices, position);
        }
        Some(
            indices
                .into_iter()
                .flat_map(|i| p.colors[usize::from(i)])
                .collect(),
        )
    }

    pub fn rival_timing_samples(
        &self,
        now_ns: u128,
        main_delay_ns: u128,
        rival_delay_ns: u128,
    ) -> Vec<RivalTimingSample> {
        let Some((index, at)) = self.frame_at(now_ns, main_delay_ns) else {
            return Vec::new();
        };
        (48..=54)
            .filter_map(|actor| {
                let view =
                    self.rival_view(index, at, now_ns, main_delay_ns, rival_delay_ns, actor)?;
                Some(RivalTimingSample {
                    actor,
                    drawn: view.position,
                    painted: view.layer.paint_origin([
                        view.position[0] + view.layer.pose_offset,
                        view.position[1],
                    ]),
                    pose_offset: view.layer.pose_offset,
                    observation: view.layer.observation,
                    effective_delay_ns: view.delay_ns,
                    boundary_fallback: view.boundary_fallback,
                })
            })
            .collect()
    }

    pub fn rival_samples(&self, now_ns: u128, delay_ns: u128) -> Vec<RivalSample> {
        let Some((index, at)) = self.frame_at(now_ns, delay_ns) else {
            return Vec::new();
        };
        let frame = &self.frames[index];
        (48..=54)
            .filter_map(|actor| {
                let layer = Self::actor_layer(frame, actor)?;
                let (history, bracketed, interval_ms) =
                    self.local_actor(index, at, now_ns as f64 / 1e9, actor)?;
                Some(RivalSample {
                    actor,
                    native: layer.position,
                    predicted: frame.presenter.position(layer, at),
                    history,
                    pose_offset: layer.pose_offset,
                    observation: layer.observation,
                    bracketed,
                    interval_ms,
                })
            })
            .collect()
    }

    fn rgba_scope(
        &self,
        now_ns: u128,
        delay_ns: u128,
        rivals: bool,
        swimming: bool,
        bird: BirdPolicy,
    ) -> Option<Vec<u8>> {
        let (index, at) = self.frame_at(now_ns, delay_ns)?;
        let presenter = &self.frames[index].presenter;
        let p = presenter.latest.as_ref()?;
        let player = self
            .local_player(index, at, now_ns as f64 / 1e9, swimming)
            .map(|(local, _, _)| self.player_world(index, at, local));
        let indices = if !presenter.is_active() {
            p.native.indices()
        } else {
            p.placed_indices(|layer| {
                if layer.id == 0 {
                    player.unwrap_or_else(|| presenter.position(layer, at))
                } else if layer.id == 43 {
                    self.bird_position(index, at, now_ns as f64 / 1e9, layer, bird)
                } else if rivals && (48..=54).contains(&layer.id) {
                    self.local_actor(index, at, now_ns as f64 / 1e9, layer.id)
                        .map_or_else(
                            || presenter.position(layer, at),
                            |(position, _, _)| position,
                        )
                } else {
                    presenter.position(layer, at)
                }
            })
        };
        Some(
            indices
                .into_iter()
                .flat_map(|i| p.colors[usize::from(i)])
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        framebuffer::Dimensions,
        motion::tests::{layer, publication},
    };

    fn add(history: &mut HistoryPresenter, ms: u128, position: [f64; 2], observation: u64) {
        let mut player = layer(
            0,
            position[0] as i32,
            super::super::Blend::And,
            vec![0xff; 2],
        );
        player.native[1] = position[1] as i32;
        player.position = position;
        player.cadenced = true;
        player.smooth_velocity = true;
        player.observation = observation;
        player.stride = [8.0, 0.0];
        if let Some(previous) = history
            .frames
            .back()
            .and_then(HistoryPresenter::player_layer)
        {
            let delta = std::array::from_fn(|axis| position[axis] - previous.position[axis]);
            player.stride = delta;
        }
        let mut p = publication(vec![player]);
        p.coherent_water = true;
        let at = ms as f64 / 1000.0;
        history.base.publish(p, at, at, true);
        history.record(history.base.clone(), at);
    }

    fn position(history: &HistoryPresenter, now_ms: u128, delay_ms: u128) -> PlayerSample {
        history
            .player_sample(now_ms * 1_000_000, delay_ms * 1_000_000)
            .unwrap()
    }

    fn swim_event(h: &mut HistoryPresenter, ms: u128, position: [f64; 2], observation: u64) {
        let mut player = layer(0, position[0] as i32, super::super::Blend::And, vec![0; 2]);
        player.native[1] = position[1] as i32;
        player.position = position;
        player.cadenced = true;
        player.smooth_velocity = true;
        player.observation = observation;
        player.stride = [8.0, 4.0];
        let mut p = publication(vec![player]);
        p.scene = 2;
        p.coherent_water = true;
        let at = ms as f64 / 1000.0;
        h.base.publish(p, at, at, true);
        h.record(h.base.clone(), at);
    }

    #[test]
    fn swimming_history_is_opt_in_and_interpolates_real_two_axis_observations() {
        let mut h = HistoryPresenter::default();
        swim_event(&mut h, 0, [100.0, 40.0], 1);
        swim_event(&mut h, 100, [108.0, 44.0], 2);
        assert!(h.player_sample(100_000_000, 25_000_000).is_none());
        let sample = h.swimmer_sample(100_000_000, 25_000_000).unwrap();
        assert_eq!(sample.drawn, [106.0, 43.0]);
        assert!(sample.bracketed);
        assert_eq!(sample.pose_offset, 0.0);
        assert!(
            h.rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000)
                != h.rgba_with_swimming(100_000_000, 25_000_000, 40_000_000)
        );
        // Keeping both clocks equal must still include the swimmer trial.
        assert_eq!(
            h.rgba_with_swimming(100_000_000, 25_000_000, 40_000_000),
            h.rgba_with_swimming(100_000_000, 25_000_000, 25_000_000)
        );
    }

    #[test]
    fn swimming_history_never_extrapolates_or_uses_unavailable_future_positions() {
        let mut h = HistoryPresenter::default();
        swim_event(&mut h, 0, [280.0, 176.0], 1);
        swim_event(&mut h, 100, [290.0, 179.0], 2);
        let early = h.swimmer_sample(75_000_000, 25_000_000).unwrap();
        assert_eq!(early.drawn, [280.0, 176.0]);
        assert!(!early.bracketed);
        for now in 100..=300 {
            let sample = h.swimmer_sample(now * 1_000_000, 25_000_000).unwrap();
            assert!((280.0..=290.0).contains(&sample.drawn[0]));
            assert!((176.0..=179.0).contains(&sample.drawn[1]));
        }
        assert_eq!(
            h.swimmer_sample(300_000_000, 25_000_000).unwrap().drawn,
            [290.0, 179.0]
        );
    }

    #[test]
    fn swimming_trial_preserves_non_water_player_and_rival_policies() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 100, 108, 2, 1);
        for now in 0..=150 {
            assert_eq!(
                h.rgba_with_swimming(now * 1_000_000, 25_000_000, 40_000_000),
                h.rgba_with_rival_delay(now * 1_000_000, 25_000_000, 40_000_000)
            );
            assert!(h.swimmer_sample(now * 1_000_000, 25_000_000).is_none());
        }
        // Crossing a scene boundary cannot join swimming to walking history.
        swim_event(&mut h, 200, [112.0, 44.0], 3);
        assert_eq!(
            h.player_sample(200_000_000, 25_000_000).unwrap().drawn,
            [60.0, 40.0]
        );
        assert_eq!(
            h.swimmer_sample(225_000_000, 25_000_000).unwrap().drawn,
            [112.0, 44.0]
        );
    }

    fn rival_event(h: &mut HistoryPresenter, ms: u128, x: i32, observation: u64, facing: i8) {
        let mut rival = layer(48, x, super::super::Blend::And, vec![0; 2]);
        rival.cadenced = true;
        rival.smooth_velocity = true;
        rival.observation = observation;
        rival.turn_direction = facing;
        // A nearby potential contact suppresses prediction without changing
        // the actual walking direction or the real next accepted displacement.
        rival.stride = [0.0; 2];
        rival.epoch = 1;
        let mut cat = layer(0, 60, super::super::Blend::And, vec![0xff; 2]);
        cat.cadenced = true;
        cat.observation = observation;
        let mut p = publication(vec![cat, rival]);
        p.scene = 7;
        p.coherent_water = true;
        let at = ms as f64 / 1000.0;
        h.base.publish(p, at, at, true);
        h.record(h.base.clone(), at);
    }

    fn rival(h: &HistoryPresenter, now_ms: u128) -> RivalSample {
        h.rival_samples(now_ms * 1_000_000, 25_000_000)[0]
    }

    fn bird_event(
        h: &mut HistoryPresenter,
        ms: u128,
        position: [f64; 2],
        observation: u64,
        facing: i8,
    ) {
        let mut bird = layer(43, position[0] as i32, super::super::Blend::And, vec![0; 2]);
        bird.native[1] = position[1] as i32;
        bird.position = position;
        bird.cadenced = true;
        bird.smooth_velocity = true;
        bird.native_by_default = true;
        bird.observation = observation;
        bird.epoch = 1;
        bird.stride = [f64::from(facing) * 4.0, 2.0];
        bird.turn_direction = facing;
        bird.cadence_hint = Some(0.050);
        bird.bounds = Some((
            [position[0] - 4.0, position[1] - 2.0],
            [position[0] + 4.0, position[1] + 2.0],
        ));
        let mut p = publication(vec![bird]);
        p.scene = 5;
        let at = ms as f64 / 1000.0;
        h.base.publish(p, at, at, true);
        h.record(h.base.clone(), at);
    }

    #[test]
    fn bird_trial_keeps_default_native_and_history_axes_independent_on_a_turn() {
        let mut h = HistoryPresenter::default();
        bird_event(&mut h, 0, [100.0, 80.0], 1, 1);
        bird_event(&mut h, 100, [96.0, 82.0], 2, -1);
        let sample = h.bird_sample(100_000_000, 25_000_000).unwrap();
        assert_eq!(sample.native, [100.0, 80.0]);
        assert_eq!(sample.history, [100.0, 81.5]);
        assert!(sample.bracketed);
        assert!((96.0..=104.0).contains(&sample.velocity[0]));
        assert!((78.0..=82.0).contains(&sample.velocity[1]));
        assert!(
            h.rgba_with_bird(100_000_000, 25_000_000, 40_000_000, BirdPolicy::Native)
                == h.rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000)
        );
        assert!(
            h.rgba_with_bird(100_000_000, 25_000_000, 40_000_000, BirdPolicy::History)
                == h.rgba_with_bird(100_000_000, 25_000_000, 25_000_000, BirdPolicy::History)
        );
    }

    #[test]
    fn bird_history_holds_missing_endpoints_and_never_reanimates_an_erased_picture() {
        let mut h = HistoryPresenter::default();
        bird_event(&mut h, 0, [100.0, 80.0], 1, 1);
        assert_eq!(
            h.bird_sample(100_000_000, 25_000_000).unwrap().history,
            [100.0, 80.0]
        );
        bird_event(&mut h, 100, [104.0, 82.0], 2, 1);
        let p = h
            .frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap();
        p.layers.clear();
        p.native = p.background.clone();
        assert!(h.bird_sample(125_000_000, 25_000_000).is_none());
        assert!(
            h.rgba_with_bird(125_000_000, 25_000_000, 40_000_000, BirdPolicy::History)
                == h.rgba_with_rival_delay(125_000_000, 25_000_000, 40_000_000)
        );
    }

    #[test]
    fn separate_rival_clock_samples_older_artwork_and_position_together() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[1]
            .pose_offset = 1.0;
        rival_event(&mut h, 70, 108, 2, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[1]
            .pixels
            .fill(0xff);
        rival_event(&mut h, 100, 116, 3, 1);
        let sample = h.rival_timing_samples(100_000_000, 25_000_000, 40_000_000)[0];
        assert_eq!(sample.observation, 1);
        assert_eq!(sample.effective_delay_ns, 40_000_000);
        assert!(!sample.boundary_fallback);
        assert!((sample.drawn[0] - (100.0 + 8.0 * 60.0 / 70.0)).abs() < 1e-9);
        assert_eq!(sample.pose_offset, 1.0);
        let player = position(&h, 100, 25);
        let unified = h.rgba_with_rivals(100_000_000, 25_000_000).unwrap();
        let mixed = h
            .rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000)
            .unwrap();
        let pixel = (40 * 320 + 108) * 4;
        assert_eq!(&mixed[pixel..pixel + 4], &[0, 0, 0, 255]);
        assert_eq!(&unified[pixel..pixel + 4], &[170, 170, 170, 255]);
        assert_eq!(position(&h, 100, 25).drawn, player.drawn);
        assert_eq!(
            h.rgba_with_rival_delay(100_000_000, 25_000_000, 25_000_000),
            Some(unified)
        );
    }

    #[test]
    fn separate_clock_never_resurrects_a_missing_rival_or_crosses_a_visibility_gap() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 60, 100, 2, 1);
        rival_event(&mut h, 70, 108, 3, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers
            .pop();
        rival_event(&mut h, 80, 108, 4, 1);
        assert!(
            h.rival_timing_samples(100_000_000, 25_000_000, 40_000_000)
                .is_empty()
        );
        assert_eq!(
            h.rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000),
            h.rgba_with_rivals(100_000_000, 25_000_000)
        );
        // Both endpoints contain the same lifetime, but it disappeared between
        // them. Revert both its artwork and position to the unified timeline.
        let sample = h.rival_timing_samples(105_000_000, 25_000_000, 40_000_000)[0];
        assert!(sample.boundary_fallback);
        assert_eq!(sample.effective_delay_ns, 25_000_000);
        assert_eq!(sample.observation, 4);
        assert_eq!(
            h.rgba_with_rival_delay(105_000_000, 25_000_000, 40_000_000),
            h.rgba_with_rivals(105_000_000, 25_000_000)
        );
    }

    #[test]
    fn separate_clock_keeps_respawns_and_native_fallback_on_the_main_timeline() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 70, 108, 2, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[1]
            .epoch = 2;
        rival_event(&mut h, 100, 116, 3, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[1]
            .epoch = 2;
        let sample = h.rival_timing_samples(100_000_000, 25_000_000, 40_000_000)[0];
        assert!(sample.boundary_fallback);
        assert_eq!(sample.observation, 2);
        assert_eq!(
            h.rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000),
            h.rgba_with_rivals(100_000_000, 25_000_000)
        );
        h.frames[1].presenter.tracks.clear();
        assert!(
            h.rival_timing_samples(100_000_000, 25_000_000, 40_000_000)
                .is_empty()
        );
        assert_eq!(
            h.rgba_with_rival_delay(100_000_000, 25_000_000, 40_000_000),
            h.rgba_with_rivals(100_000_000, 25_000_000)
        );
    }

    #[test]
    fn rival_history_uses_actual_facing_instead_of_contact_suppressed_forecast() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 100, 108, 2, 1);
        let sample = rival(&h, 100);
        assert_eq!(sample.history, [106.0, 40.0]);
        assert!(sample.bracketed);
        assert_eq!(sample.interval_ms, 100.0);
        let before = position(&h, 100, 25);
        let old_view = h.rgba(100_000_000, 25_000_000).unwrap();
        let trial = h.rgba_with_rivals(100_000_000, 25_000_000).unwrap();
        assert_ne!(old_view, trial);
        assert_eq!(before.drawn, position(&h, 100, 25).drawn);
        // Only the rival's pixels change between the two views.
        for (byte, (a, b)) in old_view.iter().zip(&trial).enumerate() {
            let i = byte / 4;
            if i / 320 != 40 || !(96..120).contains(&(i % 320)) {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn rival_stop_and_restart_never_slide_sitting_artwork() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 100, 108, 2, 0);
        rival_event(&mut h, 200, 108, 3, 0);
        rival_event(&mut h, 300, 116, 4, 1);
        // An ordinary final walking step may still interpolate into a sitting
        // endpoint; only an actual facing reversal needs the extra event cut.
        assert_eq!(rival(&h, 100).history, [106.0, 40.0]);
        assert!(rival(&h, 100).bracketed);
        for now in 125..325 {
            assert_eq!(rival(&h, now).history, [108.0, 40.0]);
        }
        assert_eq!(rival(&h, 325).history, [116.0, 40.0]);
    }

    #[test]
    fn rival_field_limit_matches_unified_and_mixed_painted_diagnostics() {
        for (logical, offset, expected) in [(266, 1.0, 264.0), (37, -3.0, 36.0)] {
            let mut h = HistoryPresenter::default();
            for (ms, observation) in [(0, 1), (100, 2)] {
                rival_event(&mut h, ms, logical, observation, 1);
                let rival = &mut h
                    .frames
                    .back_mut()
                    .unwrap()
                    .presenter
                    .latest
                    .as_mut()
                    .unwrap()
                    .layers[1];
                rival.pose_offset = offset;
                rival.paint_x_bounds = Some([36.0, 264.0]);
            }
            for delay in [25_000_000, 40_000_000] {
                let sample = h.rival_timing_samples(100_000_000, 25_000_000, delay)[0];
                assert_eq!(sample.drawn[0], f64::from(logical));
                assert_eq!(sample.painted, [expected, 40.0]);
                let pixels = h
                    .rgba_with_rival_delay(100_000_000, 25_000_000, delay)
                    .unwrap();
                let left = (40 * 320 + expected as usize) * 4;
                assert_eq!(&pixels[left..left + 4], &[0, 0, 0, 255]);
                if expected == 264.0 {
                    let beyond = (40 * 320 + 272) * 4;
                    assert_eq!(&pixels[beyond..beyond + 4], &[170, 170, 170, 255]);
                } else {
                    let before = (40 * 320 + 35) * 4;
                    assert_eq!(&pixels[before..before + 4], &[170, 170, 170, 255]);
                }
            }
        }
    }

    #[test]
    fn rival_boundary_turn_keeps_its_last_outward_step_with_the_new_pose() {
        for (old, end, facing) in [(262, 266, 1), (41, 37, -1)] {
            for delay in [25_000_000, 40_000_000] {
                let mut h = HistoryPresenter::default();
                rival_event(&mut h, 0, old, 1, facing);
                // The native boundary clamp still moves outward, but already
                // selects inward-facing art on that same accepted update.
                rival_event(&mut h, 100, end, 2, -facing);
                for now_ms in 100..100 + delay / 1_000_000 {
                    let sample = h.rival_timing_samples(now_ms * 1_000_000, delay, delay)[0];
                    assert_eq!(sample.drawn, [f64::from(old), 40.0]);
                    assert_eq!(sample.observation, 1);
                }
                let sample = h.rival_timing_samples(100_000_000 + delay, delay, delay)[0];
                assert_eq!(sample.drawn, [f64::from(end), 40.0]);
                assert_eq!(sample.observation, 2);
            }
        }
    }

    #[test]
    fn rival_reversal_and_new_lifetime_retain_discrete_native_events() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 100, 92, 2, -1);
        assert_eq!(rival(&h, 100).history, [100.0, 40.0]);
        assert_eq!(rival(&h, 125).history, [92.0, 40.0]);
        let latest = h
            .frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap();
        latest.layers[1].position = [108.0, 40.0];
        latest.layers[1].epoch = 2;
        assert_eq!(rival(&h, 100).history, [100.0, 40.0]);
    }

    #[test]
    fn temporary_missing_rival_cuts_history_even_when_the_epoch_returns_unchanged() {
        let mut h = HistoryPresenter::default();
        rival_event(&mut h, 0, 100, 1, 1);
        rival_event(&mut h, 20, 100, 2, 1);
        rival_event(&mut h, 40, 108, 3, 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers
            .pop();
        rival_event(&mut h, 60, 108, 4, 1);
        assert_eq!(rival(&h, 60).history, [100.0, 40.0]);
        assert!(h.rival_samples(75_000_000, 25_000_000).is_empty());
        assert_eq!(rival(&h, 85).history, [108.0, 40.0]);
    }

    #[test]
    fn running_has_no_accumulated_debt_and_stop_has_no_settling_slide() {
        let mut h = HistoryPresenter::default();
        for event in 0..=5 {
            add(
                &mut h,
                event * 25,
                [80.0 + event as f64 * 8.0, 80.0],
                event as u64,
            );
        }
        add(&mut h, 150, [120.0, 80.0], 6);
        assert!((position(&h, 150, 40).drawn[0] - 115.2).abs() < 1e-9);
        for now in 165..=190 {
            assert_eq!(position(&h, now, 40).drawn, [120.0, 80.0]);
        }
    }

    #[test]
    fn observed_jump_envelope_cannot_grow_an_extra_apex_or_canceled_ascent() {
        let mut h = HistoryPresenter::default();
        for (event, y) in [100.0, 92.0, 88.0, 89.0, 94.0, 100.0]
            .into_iter()
            .enumerate()
        {
            add(&mut h, event as u128 * 25, [100.0, y], event as u64);
        }
        for now in 40..=165 {
            assert!((88.0..=100.0).contains(&position(&h, now, 40).drawn[1]));
        }
        let mut wall = HistoryPresenter::default();
        add(&mut wall, 0, [290.0, 96.0], 0);
        add(&mut wall, 25, [290.0, 98.0], 1);
        for now in 40..=65 {
            assert!(position(&wall, now, 40).drawn[1] >= 96.0);
        }
    }

    #[test]
    fn equal_position_acceptances_keep_a_restart_out_of_the_stationary_period() {
        let mut h = HistoryPresenter::default();
        for event in 0..=4 {
            add(&mut h, event * 25, [100.0, 80.0], event as u64);
        }
        add(&mut h, 125, [108.0, 80.0], 5);
        assert_eq!(position(&h, 139, 40).drawn, [100.0, 80.0]);
        // The first start is a native step so idle artwork cannot slide.
        assert_eq!(position(&h, 150, 40).drawn, [100.0, 80.0]);
        assert_eq!(position(&h, 165, 40).drawn, [108.0, 80.0]);
    }

    #[test]
    fn unavailable_endpoint_holds_without_prediction_or_future_reads() {
        let mut h = HistoryPresenter::default();
        add(&mut h, 0, [100.0, 80.0], 0);
        add(&mut h, 90, [108.0, 80.0], 1);
        let early = position(&h, 80, 40);
        assert_eq!(early.drawn, [100.0, 80.0]);
        assert!(!early.bracketed);
        assert!(position(&h, 95, 40).bracketed);
    }

    #[test]
    fn reversing_pose_and_its_native_step_arrive_together() {
        let mut h = HistoryPresenter::default();
        add(&mut h, 0, [100.0, 80.0], 0);
        add(&mut h, 25, [108.0, 80.0], 1);
        add(&mut h, 50, [100.0, 80.0], 2);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[0]
            .pixels
            .fill(0);
        assert_eq!(position(&h, 80, 40).drawn, [108.0, 80.0]);
        assert_eq!(position(&h, 90, 40).drawn, [100.0, 80.0]);
        let pixel = (80 * 320 + 100) * 4;
        assert_ne!(
            &h.rgba(80_000_000, 40_000_000).unwrap()[pixel..pixel + 4],
            &[0, 0, 0, 255]
        );
        assert_eq!(
            &h.rgba(90_000_000, 40_000_000).unwrap()[pixel..pixel + 4],
            &[0, 0, 0, 255]
        );
    }

    #[test]
    fn pose_palette_and_scenery_share_the_delayed_time_and_buffer_is_bounded() {
        let mut h = HistoryPresenter::default();
        add(&mut h, 0, [100.0, 80.0], 0);
        add(&mut h, 25, [108.0, 80.0], 1);
        let latest = h
            .frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap();
        latest.colors = [[17, 19, 23, 255]; 4];
        latest.layers[0].dimensions = Dimensions(0x0201);
        latest.layers[0].pixels.resize(4, 0xff);
        assert_eq!(position(&h, 50, 40).observation, 0);
        assert_ne!(
            &h.rgba(50_000_000, 40_000_000).unwrap()[..4],
            &[17, 19, 23, 255]
        );
        assert_eq!(position(&h, 65, 40).observation, 1);
        assert_eq!(
            &h.rgba(65_000_000, 40_000_000).unwrap()[..4],
            &[17, 19, 23, 255]
        );
        for event in 2..=300 {
            add(&mut h, event * 25, [108.0, 80.0], event as u64);
        }
        assert!(h.frames.len() <= 12);
    }

    #[test]
    fn parent_scene_and_epoch_changes_are_cuts_and_same_batch_is_coalesced() {
        let mut h = HistoryPresenter::default();
        add(&mut h, 0, [100.0, 80.0], 0);
        add(&mut h, 25, [108.0, 80.0], 1);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[0]
            .epoch = 1;
        assert_eq!(position(&h, 50, 40).drawn, [100.0, 80.0]);
        add(&mut h, 25, [116.0, 80.0], 2);
        assert_eq!(h.frames.len(), 2);
        assert_eq!(position(&h, 65, 40).drawn, [116.0, 80.0]);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .scene = 1;
        assert_eq!(position(&h, 50, 40).drawn, [100.0, 80.0]);
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .scene = 0;
        h.frames
            .back_mut()
            .unwrap()
            .presenter
            .latest
            .as_mut()
            .unwrap()
            .layers[0]
            .parent = Some(8);
        assert_eq!(position(&h, 50, 40).drawn, [100.0, 80.0]);
    }
}

#[cfg(test)]
mod falling_head_tests {
    use super::*;
    use crate::motion::{
        Blend,
        tests::{layer, publication},
    };

    fn frame(h: &mut HistoryPresenter, ms: u128, ending: bool, y: Option<i32>) {
        let layers = y
            .map(|y| {
                let mut l = layer(60, 148, Blend::Opaque, vec![0xff; 2]);
                l.native[1] = y;
                l.position = [148.0, f64::from(y)];
                l.cadenced = true;
                l.native_by_default = true;
                l.observation = y as u64;
                l
            })
            .into_iter()
            .collect();
        let mut p = publication(layers);
        p.scene = 7;
        p.hearts_presentation = ending;
        p.supported = y.is_some();
        let t = ms as f64 / 1e3;
        h.base.publish(p, t, t, false); // Runtime marks presentation polls as waits.
        h.record(h.base.clone(), t);
    }

    #[test]
    fn head_interpolates_observed_steps_during_waits_and_cuts_at_landing() {
        let mut h = HistoryPresenter::default();
        frame(&mut h, 0, true, None);
        frame(&mut h, 100, true, Some(144));
        frame(&mut h, 155, true, Some(156));
        frame(&mut h, 210, true, Some(168));
        frame(&mut h, 215, true, None); // Different wider landing artwork.
        let sample = h.falling_head_sample(240_000_000).unwrap();
        assert_eq!(sample.native, [148.0, 156.0]);
        assert!(sample.bracketed);
        assert!((sample.history[1] - (156.0 + 12.0 * 25.0 / 55.0)).abs() < 1e-9);
        assert!(
            h.rgba_with_hearts_drop(240_000_000, true).unwrap()
                != h.rgba_with_hearts_drop(240_000_000, false).unwrap()
        );
        let final_fall = h.falling_head_sample(272_000_000).unwrap();
        assert_eq!(final_fall.history, [148.0, 168.0]);
        assert!(!final_fall.bracketed);
        assert!(h.falling_head_sample(275_000_000).is_none());
        assert_eq!(
            h.rgba_with_hearts_drop(275_000_000, true),
            h.rgba_with_hearts_drop(275_000_000, false)
        );
    }

    #[test]
    fn ending_clock_holds_entry_instead_of_rewinding_and_keeps_exit_tail() {
        let mut h = HistoryPresenter::default();
        frame(&mut h, 0, false, None);
        frame(&mut h, 100, true, None);
        frame(&mut h, 130, true, Some(12));
        assert_eq!(h.hearts_frame(140_000_000).unwrap(), (1, 0.1));
        frame(&mut h, 200, false, None);
        assert_eq!(h.hearts_frame(230_000_000).unwrap(), (2, 0.17));
        assert!(h.hearts_frame(265_000_000).is_none());
    }
}
