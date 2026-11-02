// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Live adapter for the isolated motion observer. Presentation and diagnostics only.

use crate::{
    assets::GraphicsAssets,
    frame::Frame,
    gameplay::{Gameplay, PauseEvent, execution::GameplayEvent, experiment::Scene},
    motion::{self, Pose, Tracker},
    sound::GameplayAction,
};
use reforge_audio::PIT_HZ;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
};

fn seconds(tick: u64) -> f64 {
    tick as f64 / PIT_HZ as f64
}

struct Observation {
    pose: Pose,
    scene: Scene,
    offset: f64,
}
enum Pending {
    Pose(Observation),
    Event(u64, GameplayEvent),
    Reset(u64, &'static str),
    Suspend(u64, &'static str),
}

pub(super) struct Experiment {
    pub enhanced: bool,
    tracker: Tracker,
    latest: Option<Observation>,
    pending: Vec<Pending>,
    next_id: u64,
    next_presentation: u64,
    trace: BufWriter<std::fs::File>,
    error: Option<String>,
    rows: u64,
    last_poll: Option<(u16, u8)>,
}

impl Experiment {
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut trace = BufWriter::new(file);
        writeln!(trace,"kind\ttick\tseen_tick\tsource_tick\tid\tanchor\tnative_anchor\tleft_px\tenhanced\tcontrols\tphase\treason").map_err(|e|e.to_string())?;
        Ok(Self {
            enhanced: true,
            tracker: Tracker::default(),
            latest: None,
            pending: Vec::new(),
            next_id: 0,
            next_presentation: 0,
            trace,
            error: None,
            rows: 0,
            last_poll: None,
        })
    }

    pub fn label(&self) -> &'static str {
        if self.enhanced {
            "Motion: enhanced (F8 switches)"
        } else {
            "Motion: faithful (F8 switches)"
        }
    }

    fn log(&mut self, kind: &str, tick: u64, seen: u64, reason: &str) {
        if self.rows > 100_000 {
            return;
        }
        if self.rows == 100_000 {
            if let Err(e) = writeln!(
                self.trace,
                "truncated\t{tick}\t{seen}\t0\t0\t0\t0\t0\t{}\t0\t0\trow_limit",
                u8::from(self.enhanced)
            ) {
                self.error = Some(e.to_string());
            }
            self.rows += 1;
            return;
        }
        let sample = self.tracker.sample(seconds(tick));
        let (source, id, native, controls, offset) =
            self.latest.as_ref().map_or((0, 0, 0.0, 0, 0.0), |o| {
                (
                    (o.pose.at * PIT_HZ as f64).round() as u64,
                    o.pose.id,
                    o.pose.anchor(),
                    o.pose.controls,
                    o.offset,
                )
            });
        let phase = self.latest.as_ref().map_or(0, |o| o.pose.phase);
        let anchor = if self.enhanced && phase == 0 {
            sample.map_or(native, |s| s.anchor)
        } else {
            self.latest.as_ref().map_or(native, |o| {
                o.scene
                    .layers
                    .iter()
                    .find(|l| l.slot == 0)
                    .map_or(native, |l| f64::from(l.rect.x_bytes) * 4.0 + offset)
            })
        };
        let left = (anchor - offset + 0.5).floor();
        if let Err(e) = writeln!(
            self.trace,
            "{kind}\t{tick}\t{seen}\t{source}\t{id}\t{anchor:.9}\t{native:.9}\t{left:.0}\t{}\t{controls}\t{phase}\t{reason}",
            u8::from(self.enhanced)
        ) {
            self.error = Some(e.to_string());
        }
        self.rows += 1;
        if self.rows.is_multiple_of(512)
            && let Err(e) = self.trace.flush()
        {
            self.error = Some(e.to_string());
        }
    }

    pub fn event(&mut self, tick: u64, event: GameplayEvent) {
        self.pending.push(Pending::Event(tick, event));
    }
    pub fn reset(&mut self, tick: u64, reason: &'static str) {
        self.pending.push(Pending::Reset(tick, reason));
    }
    pub fn suspend(&mut self, tick: u64, reason: &'static str) {
        self.pending.push(Pending::Suspend(tick, reason));
    }
    pub fn toggle(&mut self, tick: u64) {
        self.enhanced = !self.enhanced;
        self.log("toggle", tick, tick, self.label());
    }
    pub fn host_key(&mut self, tick: u64, seen: u64, scan: u8) {
        self.log("host_key", tick, seen, &format!("scan={scan:02x}"));
    }

    pub fn publication(
        &mut self,
        tick: u64,
        game: &Gameplay,
        assets: &GraphicsAssets,
    ) -> Result<(), String> {
        let scene = game.experimental_scene(assets).map_err(|e| e.to_string())?;
        let Some(craft) = scene.layers.iter().find(|l| l.slot == 0) else {
            self.reset(tick, "missing_craft");
            return Ok(());
        };
        let Some(shadow) = scene.layers.iter().find(|l| l.slot == 1) else {
            self.reset(tick, "missing_shadow");
            return Ok(());
        };
        let state = game.snapshot();
        let mut source = [
            craft.source,
            craft.rect.width_bytes * 4,
            craft.rect.height,
            shadow.source,
            shadow.rect.width_bytes * 4,
            shadow.rect.height,
        ];
        let horizontal = (2_097_152 / (i32::from(state.player.coordinates[0]) + 64))
            * i32::from(state.player.coordinates[1]);
        let coarse = 160 + 4 * ((horizontal >> 16) >> 2);
        let offset = f64::from(coarse - craft.rect.x_bytes * 4);
        if (11..=15).contains(&source[0]) && offset != 12.0 {
            source[3] = usize::MAX;
        }
        let pose = Pose {
            at: seconds(tick),
            id: self.next_id,
            body: state.body,
            pass: state.render_pass,
            coordinates: state.player.coordinates,
            phase: state.player.phase,
            controls: (state.player.effective_input & 15) as u8,
            source,
        };
        self.next_id = self.next_id.saturating_add(1);
        self.pending.push(Pending::Pose(Observation {
            pose,
            scene,
            offset,
        }));
        Ok(())
    }

    /// All pending observations become available now, never at their past
    /// publication times. A stale batch rebases its last pose exactly once.
    pub fn commit(&mut self, seen_tick: u64) -> Result<(), String> {
        let pending = std::mem::take(&mut self.pending);
        let seen = seconds(seen_tick);
        let stale = pending
            .iter()
            .any(|p| matches!(p,Pending::Pose(o) if seen-o.pose.at>0.066));
        let last = pending.iter().rev().find_map(|p| {
            if let Pending::Pose(o) = p {
                Some(o.pose.id)
            } else {
                None
            }
        });
        for p in pending {
            match p {
                Pending::Pose(o) => {
                    self.tracker
                        .observe(o.pose, seen, stale && Some(o.pose.id) == last);
                    self.latest = Some(o);
                    let reason = self.tracker.sample(seen).map_or("none", |s| s.reason);
                    self.log("observation", seen_tick, seen_tick, reason);
                }
                Pending::Reset(at, reason) => {
                    self.tracker = Tracker::default();
                    self.latest = None;
                    self.log("boundary", at, seen_tick, reason);
                }
                Pending::Suspend(at, reason) => {
                    self.tracker.suspend(seen, reason);
                    self.log("boundary", at, seen_tick, reason);
                }
                Pending::Event(at, event) => match event {
                    GameplayEvent::InputPoll { site, mask } => {
                        // Preserve every steering poll. Native pause busy-polls
                        // can repeat thousands of identical checks per second;
                        // retain only changes there, without changing execution.
                        if site == 0x2289 || self.last_poll != Some((site, mask)) {
                            self.log(
                                "input_poll",
                                at,
                                seen_tick,
                                &format!("mask={mask:02x} site={site:04x}"),
                            );
                        }
                        self.last_poll = Some((site, mask));
                    }
                    GameplayEvent::Pause(PauseEvent::Paused | PauseEvent::EndGame) => {
                        self.tracker.suspend(seen, "native_pause");
                        self.log("boundary", at, seen_tick, "native_pause");
                    }
                    GameplayEvent::Action(GameplayAction::Wait {
                        site: 0x25f9 | 0x2602,
                        ..
                    }) => {
                        self.tracker.suspend(seen, "death_wait");
                        self.log("boundary", at, seen_tick, "death_wait");
                    }
                    _ => {}
                },
            }
        }
        self.check()
    }

    pub fn due(&self, tick: u64) -> bool {
        tick >= self.next_presentation
    }
    pub fn deadline(&self) -> u64 {
        self.next_presentation
    }
    pub fn wake_deadline(&self, tick: u64) -> Option<u64> {
        let deadline = self.deadline();
        (deadline > tick).then_some(deadline)
    }
    pub fn presented(&mut self, tick: u64, seen: u64) {
        self.next_presentation =
            ((tick as u128 * 120 / PIT_HZ as u128 + 1) * PIT_HZ as u128).div_ceil(120) as u64;
        self.log("present", tick, seen, "host_submission_not_scanout");
    }
    pub fn render(&self, tick: u64) -> Result<Option<Frame>, String> {
        if !self.enhanced {
            return Ok(None);
        }
        let Some(o) = &self.latest else {
            return Ok(None);
        };
        if o.pose.phase != 0 {
            return Ok(None);
        }
        let Some(sample) = self.tracker.sample(seconds(tick)) else {
            return Ok(None);
        };
        motion::compose(&o.scene, Some(sample.anchor - o.offset))
            .map(Some)
            .map_err(|e| e.to_string())
    }
    pub fn check(&mut self) -> Result<(), String> {
        self.error.take().map_or(Ok(()), Err)
    }
    pub fn flush(&mut self) -> Result<(), String> {
        self.trace.flush().map_err(|e| e.to_string())?;
        self.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mach3-motion-adapter-{}-{}.tsv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn trace_is_new_and_mode_switching_cannot_advance_presentation() {
        let path = path();
        let mut e = Experiment::create(&path).unwrap();
        e.flush().unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(Experiment::create(&path).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        assert!(e.due(0));
        e.presented(0, 0);
        let deadline = e.deadline();
        assert!(!e.due(deadline - 1));
        assert!(e.due(deadline));
        assert_eq!(e.wake_deadline(deadline - 1), Some(deadline));
        assert_eq!(e.wake_deadline(deadline), None);
        e.toggle(0);
        assert!(!e.enhanced);
        assert_eq!(e.deadline(), deadline);
        assert!(e.tracker.sample(1.0).is_none());
        e.flush().unwrap();
        drop(e);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn native_pause_freezes_the_visual_pose_and_transition_discards_the_scene() {
        let path = path();
        let mut e = Experiment::create(&path).unwrap();
        let pose = Pose {
            at: 0.0,
            id: 0,
            body: 0,
            pass: 1,
            coordinates: [14, 0, -100],
            phase: 0,
            controls: 0,
            source: [13, 56, 18, 18, 56, 12],
        };
        let scene = Scene {
            base: Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap(),
            layers: Vec::new(),
        };
        e.pending.push(Pending::Pose(Observation {
            pose,
            scene,
            offset: 12.0,
        }));
        e.commit(0).unwrap();
        assert!(e.render(0).unwrap().is_some());
        e.event(100, GameplayEvent::Pause(PauseEvent::Paused));
        e.commit(100).unwrap();
        assert_eq!(
            e.tracker.sample(seconds(100)).unwrap().anchor,
            e.tracker.sample(seconds(PIT_HZ)).unwrap().anchor
        );
        e.reset(200, "transition");
        e.commit(200).unwrap();
        assert!(e.render(200).unwrap().is_none());
        drop(e);
        std::fs::remove_file(path).unwrap();
    }
}
