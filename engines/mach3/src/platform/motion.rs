// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Live visual presenter; native simulation remains authoritative.

use crate::{
    assets::GraphicsAssets,
    frame::Frame,
    gameplay::{Gameplay, PauseEvent, execution::GameplayEvent},
    motion::{self, Observation, Tracker, World, scene},
    sound::GameplayAction,
};
use reforge_audio::PIT_HZ;
#[cfg(feature = "dev-tools")]
use std::{
    cell::Cell,
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

fn seconds(tick: u64) -> f64 {
    tick as f64 / PIT_HZ as f64
}

enum Pending {
    Pose(Box<Observation>),
    Scene(Box<scene::Observation>),
    Event(u64, GameplayEvent),
    Reset(u64, &'static str),
    Suspend(u64, &'static str),
    SuspendPlayer(u64, &'static str),
    SuspendScene(u64, &'static str),
}

#[cfg(feature = "dev-tools")]
struct Trace {
    writer: Box<dyn Write>,
    rows: u64,
}

pub(super) struct MotionPresenter {
    pub enhanced: bool,
    mode: super::MotionMode,
    tracker: Tracker,
    scene: scene::Tracker,
    world: World,
    #[cfg(feature = "dev-tools")]
    render_us: Cell<u64>,
    #[cfg(feature = "dev-tools")]
    submit_us: u64,
    #[cfg(feature = "dev-tools")]
    export_us: u64,
    latest: Option<Observation>,
    pending: Vec<Pending>,
    next_id: u64,
    #[cfg(feature = "dev-tools")]
    trace: Option<Trace>,
    #[cfg(feature = "dev-tools")]
    last_poll: Option<(u16, u8)>,
}

impl MotionPresenter {
    pub fn new() -> Self {
        Self {
            enhanced: true,
            mode: super::MotionMode::Enhanced,
            tracker: Tracker::default(),
            scene: scene::Tracker::default(),
            world: World::default(),
            #[cfg(feature = "dev-tools")]
            render_us: Cell::new(0),
            #[cfg(feature = "dev-tools")]
            submit_us: 0,
            #[cfg(feature = "dev-tools")]
            export_us: 0,
            latest: None,
            pending: Vec::new(),
            next_id: 0,
            #[cfg(feature = "dev-tools")]
            trace: None,
            #[cfg(feature = "dev-tools")]
            last_poll: None,
        }
    }

    #[cfg(feature = "dev-tools")]
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut writer = BufWriter::new(file);
        writeln!(writer,"kind\ttick\tseen_tick\tsource_tick\tid\tanchor\tnative_anchor\tleft_px\tenhanced\tcontrols\tphase\treason\tanchor_y\tshadow_y\tnative_y\tnative_shadow_y\tcraft_source\tshadow_source\tlateral\theight\thorizon\torigin\tvx\tvy\treset_distance\trender_us\tsubmit_us\texport_us\tbody\tpass\tdepth\tmotion_mode").map_err(|e|e.to_string())?;
        // Discover header/output failures now, before enabling this sink.
        writer.flush().map_err(|e| e.to_string())?;
        Ok(Self {
            trace: Some(Trace {
                writer: Box::new(writer),
                rows: 0,
            }),
            ..Self::new()
        })
    }

    pub fn for_options(options: &super::Options) -> Self {
        let mut observer = Self::new();
        #[cfg(feature = "dev-tools")]
        if let Some(path) = options.motion_trace.as_deref() {
            match Self::create(path) {
                Ok(traced) => observer = traced,
                Err(error) => eprintln!("Warning: motion trace disabled: {error}"),
            }
        }
        observer.set_mode(options.motion.unwrap_or_default());
        observer
    }

    pub fn set_mode(&mut self, mode: super::MotionMode) -> bool {
        let changed = self.mode != mode;
        self.mode = mode;
        self.enhanced = mode != super::MotionMode::Original;
        changed
    }

    fn tracing(&self) -> bool {
        #[cfg(feature = "dev-tools")]
        {
            self.trace.is_some()
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            false
        }
    }

    #[cfg(feature = "dev-tools")]
    fn trace_failed(&mut self, error: std::io::Error) {
        self.trace = None;
        eprintln!("Warning: motion trace disabled: {error}");
    }

    #[cfg(feature = "dev-tools")]
    fn log(&mut self, kind: &str, tick: u64, seen: u64, reason: &str) {
        let Some(trace) = &mut self.trace else {
            return;
        };
        if trace.rows > 100_000 {
            return;
        }
        if trace.rows == 100_000 {
            let result = writeln!(
                trace.writer,
                "truncated\t{tick}\t{seen}\t0\t0\t0\t0\t0\t{}\t0\t0\trow_limit{}\t{}",
                u8::from(self.enhanced),
                "\t0".repeat(19),
                self.mode.as_str()
            )
            .and_then(|()| trace.writer.flush());
            trace.rows += 1;
            if let Err(error) = result {
                self.trace_failed(error);
            }
            return;
        }
        let sample = self.tracker.sample(seconds(tick));
        let pose = self.latest.as_ref().map(|o| o.pose);
        let native = pose.map_or([0.0; 3], |p| p.anchors());
        let mut anchors = if self.enhanced && pose.is_some_and(|p| p.normal()) {
            sample.map_or(native, |s| s.anchors)
        } else {
            self.latest.as_ref().map_or(native, |o| o.native_anchors)
        };
        if self.mode == super::MotionMode::Reforged {
            anchors = self
                .scene
                .craft_anchors(seconds(tick), anchors)
                .unwrap_or_else(|| self.latest.as_ref().map_or(native, |o| o.native_anchors));
        }
        let offset = self.latest.as_ref().map_or(0.0, |o| o.offsets[0]);
        let source = pose.map_or(0, |p| (p.at * PIT_HZ as f64).round() as u64);
        let id = pose.map_or(0, |p| p.id);
        let controls = pose.map_or(0, |p| p.controls);
        let phase = pose.map_or(0, |p| p.phase);
        let left = (anchors[0] - offset + 0.5).floor();
        let source_ids = pose.map_or([0, 0], |p| [p.source[0], p.source[3]]);
        let coordinates = pose.map_or([0; 3], |p| p.coordinates);
        let velocity = pose.map_or([0; 2], |p| p.velocity);
        let result = writeln!(
            trace.writer,
            "{kind}\t{tick}\t{seen}\t{source}\t{id}\t{:.9}\t{:.9}\t{left:.0}\t{}\t{controls}\t{phase}\t{reason}\t{:.9}\t{:.9}\t{:.9}\t{:.9}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.9}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            anchors[0],
            native[0],
            u8::from(self.enhanced),
            anchors[1],
            anchors[2],
            native[1],
            native[2],
            source_ids[0],
            source_ids[1],
            coordinates[1],
            coordinates[2],
            pose.map_or(0, |p| p.horizon),
            pose.map_or(0, |p| p.origin),
            velocity[0],
            velocity[1],
            sample.map_or(0.0, |s| s.reset_distance),
            self.render_us.get(),
            self.submit_us,
            self.export_us,
            pose.map_or(0, |p| p.body),
            pose.map_or(0, |p| p.pass),
            coordinates[0],
            self.mode.as_str()
        );
        trace.rows += 1;
        let result = result.and_then(|()| {
            if trace.rows.is_multiple_of(512) {
                trace.writer.flush()
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.trace_failed(error);
        }
    }

    #[cfg(not(feature = "dev-tools"))]
    fn log(&mut self, _kind: &str, _tick: u64, _seen: u64, _reason: &str) {}

    pub fn event(&mut self, tick: u64, event: GameplayEvent) {
        // Poll diagnostics must not grow a queue when logging is disabled.
        if matches!(event, GameplayEvent::InputPoll { .. }) && !self.tracing() {
            return;
        }
        self.pending.push(Pending::Event(tick, event));
    }
    pub fn reset(&mut self, tick: u64, reason: &'static str) {
        self.world.reset();
        self.pending.push(Pending::Reset(tick, reason));
    }
    pub fn suspend(&mut self, tick: u64, reason: &'static str) {
        self.pending.push(Pending::Suspend(tick, reason));
    }

    /// A damage response disables ordinary craft tracking while the world still
    /// advances. Active sample IRQs suspend the world as well. Retain the old
    /// craft-only presenter's suspension policy in either legacy mode.
    pub fn pending_wait(&mut self, tick: u64, damaged: bool, blocking_sample: bool) {
        if damaged {
            self.pending
                .push(Pending::SuspendPlayer(tick, "damage_pending"));
            if blocking_sample {
                self.pending
                    .push(Pending::SuspendScene(tick, "blocking_sample"));
            }
        } else if blocking_sample {
            self.suspend(tick, "blocking_sample");
        }
    }
    #[cfg(feature = "dev-tools")]
    pub fn host_key(&mut self, tick: u64, seen: u64, scan: u8) {
        if self.trace.is_some() {
            self.log("host_key", tick, seen, &format!("scan={scan:02x}"));
        }
    }

    #[cfg(feature = "dev-tools")]
    pub fn host_gamepad(&mut self, tick: u64, seen: u64, mask: u8) {
        if self.trace.is_some() {
            self.log("host_gamepad", tick, seen, &format!("mask={mask:02x}"));
        }
    }

    pub fn publication(
        &mut self,
        tick: u64,
        game: &Gameplay,
        assets: &GraphicsAssets,
    ) -> Result<(), String> {
        #[cfg(feature = "dev-tools")]
        let started = Instant::now();
        if let Some(o) = self
            .world
            .capture(game, assets, seconds(tick), self.next_id)
            .map_err(|e| e.to_string())?
        {
            let scene = scene::Observation::capture(game, assets, seconds(tick), self.next_id)
                .map_err(|e| e.to_string())?;
            self.next_id = self.next_id.saturating_add(1);
            self.pending.push(Pending::Pose(Box::new(o)));
            self.pending.push(Pending::Scene(Box::new(scene)));
        }
        #[cfg(feature = "dev-tools")]
        {
            self.export_us = started.elapsed().as_micros() as u64;
        }
        Ok(())
    }

    /// All pending observations become available now, never at their past
    /// publication times. A stale batch rebases its last pose exactly once.
    pub fn commit(&mut self, seen_tick: u64) -> Result<(), String> {
        let pending = std::mem::take(&mut self.pending);
        let seen = seconds(seen_tick);
        let stale = pending
            .iter()
            .any(|p| matches!(p,Pending::Pose(o) if seen-o.pose.at>motion::HORIZON));
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
                    self.latest = Some(*o);
                    let reason = self.tracker.sample(seen).map_or("none", |s| s.reason);
                    if self.mode != super::MotionMode::Reforged {
                        self.log("observation", seen_tick, seen_tick, reason);
                    }
                }
                Pending::Scene(o) => {
                    let rebase = stale && Some(o.id) == last;
                    self.scene.observe(*o, seen, rebase);
                    if self.mode == super::MotionMode::Reforged {
                        let reason = self.tracker.sample(seen).map_or("none", |s| s.reason);
                        self.log("observation", seen_tick, seen_tick, reason);
                    }
                }
                Pending::Reset(at, reason) => {
                    self.tracker = Tracker::default();
                    self.scene = scene::Tracker::default();
                    self.latest = None;
                    self.log("boundary", at, seen_tick, reason);
                }
                Pending::Suspend(at, reason) => {
                    self.tracker.suspend(seen, reason);
                    self.scene.suspend(seen);
                    self.log("boundary", at, seen_tick, reason);
                }
                Pending::SuspendPlayer(at, reason) => {
                    self.tracker.suspend(seen, reason);
                    self.log("player_boundary", at, seen_tick, reason);
                }
                Pending::SuspendScene(at, reason) => {
                    self.scene.suspend(seen);
                    self.log("scene_boundary", at, seen_tick, reason);
                }
                Pending::Event(at, event) => match event {
                    GameplayEvent::InputPoll { site, mask } => {
                        // Preserve every steering poll. Native pause busy-polls
                        // can repeat thousands of identical checks per second;
                        // retain only changes there, without changing execution.
                        #[cfg(feature = "dev-tools")]
                        {
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
                        #[cfg(not(feature = "dev-tools"))]
                        let _ = (site, mask);
                    }
                    GameplayEvent::Pause(PauseEvent::Paused | PauseEvent::EndGame) => {
                        self.tracker.suspend(seen, "native_pause");
                        self.scene.suspend(seen);
                        self.log("boundary", at, seen_tick, "native_pause");
                    }
                    GameplayEvent::Action(GameplayAction::Wait {
                        site: 0x25f9 | 0x2602,
                        ..
                    }) => {
                        self.tracker.suspend(seen, "death_wait");
                        self.scene.suspend(seen);
                        self.log("boundary", at, seen_tick, "death_wait");
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }

    #[cfg(feature = "dev-tools")]
    pub fn presented(&mut self, tick: u64, seen: u64, submit_us: u64) {
        self.submit_us = submit_us;
        self.log("present", tick, seen, "host_submission_not_scanout");
    }
    pub fn render(&self, tick: u64, assets: &GraphicsAssets) -> Result<Option<Frame>, String> {
        #[cfg(feature = "dev-tools")]
        self.render_us.set(0);
        if !self.enhanced {
            return Ok(None);
        }
        if self.mode == super::MotionMode::Reforged {
            #[cfg(feature = "dev-tools")]
            let started = Instant::now();
            let result = self
                .scene
                .render(
                    seconds(tick),
                    self.tracker.sample(seconds(tick)).map(|s| s.anchors),
                    assets,
                )
                .map_err(|e| e.to_string());
            #[cfg(feature = "dev-tools")]
            self.render_us.set(started.elapsed().as_micros() as u64);
            return result;
        }
        let Some(o) = &self.latest else {
            return Ok(None);
        };
        if !o.pose.normal() {
            return Ok(None);
        }
        let Some(sample) = self.tracker.sample(seconds(tick)) else {
            return Ok(None);
        };
        #[cfg(feature = "dev-tools")]
        let started = Instant::now();
        let frame = motion::compose(o, sample.anchors)
            .map(Some)
            .map_err(|e| e.to_string());
        #[cfg(feature = "dev-tools")]
        self.render_us.set(started.elapsed().as_micros() as u64);
        frame
    }
    #[cfg(feature = "dev-tools")]
    pub fn flush(&mut self) {
        if let Some(trace) = &mut self.trace
            && let Err(error) = trace.writer.flush()
        {
            self.trace_failed(error);
        }
    }
}

#[cfg(all(test, feature = "dev-tools"))]
mod tests {
    use super::*;

    fn empty_assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: Vec::new(),
            alternate: Vec::new(),
            sprites: Vec::new(),
        }
    }

    fn observation(i: u64) -> Observation {
        let pose = crate::motion::Pose {
            at: i as f64 * 0.04,
            id: i,
            body: (i / 3) as u16,
            pass: (i % 3 + 1) as u8,
            coordinates: [14, i as i16 * 4, 80],
            phase: 0,
            controls: 9,
            horizon: 40,
            origin: 106,
            velocity: [1, 1],
            source: [13, 4, 1, 18, 4, 1],
        };
        let layer = crate::gameplay::raster::Layer {
            slot: 0,
            source: 13,
            rect: crate::frame::Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: 1,
                height: 1,
            },
            color: vec![255],
            mask: vec![0],
        };
        Observation {
            pose,
            scene: crate::gameplay::raster::Scene {
                base: Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap(),
                layers: Vec::new(),
            },
            order: std::array::from_fn(|i| i),
            actors: [
                layer.clone(),
                crate::gameplay::raster::Layer { slot: 1, ..layer },
            ],
            offsets: [0.0; 3],
            native_anchors: pose.anchors(),
        }
    }

    struct FailedOutput {
        flush_only: bool,
    }
    impl Write for FailedOutput {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            if self.flush_only {
                Ok(data.len())
            } else {
                Err(std::io::Error::other("injected full output"))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("injected flush failure"))
        }
    }

    #[test]
    fn no_trace_and_failed_trace_preserve_visual_motion_and_frames() {
        let path = path();
        let mut observers = [
            MotionPresenter::new(),
            MotionPresenter::create(&path).unwrap(),
            MotionPresenter::new(),
            MotionPresenter::new(),
        ];
        for (i, flush_only) in [(2, false), (3, true)] {
            observers[i].trace = Some(Trace {
                writer: Box::new(FailedOutput { flush_only }),
                rows: 511,
            });
        }
        for i in 0..16 {
            let o = observation(i);
            let tick = (o.pose.at * PIT_HZ as f64).ceil() as u64;
            for e in &mut observers {
                e.pending.push(Pending::Pose(Box::new(o.clone())));
                if i == 7 {
                    e.event(tick, GameplayEvent::Pause(PauseEvent::Paused));
                }
                e.commit(tick).unwrap();
            }
            for query in [tick, tick + 1000, tick + 10_000] {
                let expected = observers[0].render(query, &empty_assets()).unwrap();
                let anchors = observers[0].tracker.sample(seconds(query)).unwrap().anchors;
                for e in &observers[1..] {
                    assert_eq!(e.render(query, &empty_assets()).unwrap(), expected);
                    assert_eq!(e.tracker.sample(seconds(query)).unwrap().anchors, anchors);
                }
            }
        }
        assert!(observers[0].trace.is_none());
        assert!(observers[1].trace.is_some());
        assert!(observers[2].trace.is_none() && observers[3].trace.is_none());
        // A failure at final flush is also diagnostic-only.
        observers[1].trace.as_mut().unwrap().writer = Box::new(FailedOutput { flush_only: true });
        observers[1].flush();
        assert!(observers[1].trace.is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn options_enable_motion_without_storage_and_preserve_existing_files() {
        use super::super::{MotionMode, Options};
        assert!(MotionPresenter::for_options(&Options::default()).enhanced);
        let mut options = Options {
            motion: Some(MotionMode::Enhanced),
            ..Options::default()
        };
        let e = MotionPresenter::for_options(&options);
        assert!(e.enhanced && e.trace.is_none());
        let path = path();
        std::fs::write(&path, "preserve this").unwrap();
        options.motion_trace = Some(path.clone());
        let e = MotionPresenter::for_options(&options);
        assert!(e.enhanced && e.trace.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "preserve this");
        std::fs::remove_file(&path).unwrap();
        options.motion = None;
        let mut e = MotionPresenter::for_options(&options);
        assert!(e.enhanced && e.trace.is_some());
        e.flush();
        std::fs::remove_file(path).unwrap();
    }
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
    fn trace_is_new_and_mode_switching_does_not_create_observations() {
        let path = path();
        let mut e = MotionPresenter::create(&path).unwrap();
        e.flush();
        let before = std::fs::read(&path).unwrap();
        assert!(MotionPresenter::create(&path).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        e.presented(0, 0, 0);
        e.set_mode(super::super::MotionMode::Original);
        assert!(!e.enhanced);
        assert!(e.tracker.sample(1.0).is_none());
        e.trace.as_mut().unwrap().rows = 100_000;
        e.presented(100, 100, 0);
        e.flush();
        drop(e);
        let text = std::fs::read_to_string(&path).unwrap();
        let columns = text.lines().next().unwrap().split('\t').count();
        assert!(text.lines().all(|line| line.split('\t').count() == columns));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn native_pause_freezes_the_visual_pose_and_transition_discards_the_scene() {
        let path = path();
        let mut e = MotionPresenter::create(&path).unwrap();
        let pose = crate::motion::Pose {
            at: 0.0,
            id: 0,
            body: 0,
            pass: 1,
            coordinates: [14, 0, -100],
            phase: 0,
            controls: 0,
            horizon: 0,
            origin: 106,
            velocity: [0, 0],
            source: [13, 4, 1, 18, 4, 1],
        };
        let layer = crate::gameplay::raster::Layer {
            slot: 0,
            source: 13,
            rect: crate::frame::Rect {
                x_bytes: 37,
                y: 94,
                width_bytes: 1,
                height: 1,
            },
            color: vec![0],
            mask: vec![255],
        };
        e.pending.push(Pending::Pose(Box::new(Observation {
            pose,
            scene: crate::gameplay::raster::Scene {
                base: Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap(),
                layers: Vec::new(),
            },
            order: std::array::from_fn(|i| i),
            actors: [
                layer.clone(),
                crate::gameplay::raster::Layer { slot: 1, ..layer },
            ],
            offsets: [12.0, 12.0, 0.0],
            native_anchors: pose.anchors(),
        })));
        e.commit(0).unwrap();
        assert!(e.render(0, &empty_assets()).unwrap().is_some());
        e.event(100, GameplayEvent::Pause(PauseEvent::Paused));
        e.commit(100).unwrap();
        assert_eq!(
            e.tracker.sample(seconds(100)).unwrap().anchors,
            e.tracker.sample(seconds(PIT_HZ)).unwrap().anchors
        );
        e.reset(200, "transition");
        e.commit(200).unwrap();
        assert!(e.render(200, &empty_assets()).unwrap().is_none());
        drop(e);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn host_pause_freezes_prediction_and_settling_without_aging_them() {
        use crate::motion::Pose;
        use reforge_sdl::HostPause;
        use std::time::Duration;
        let mut t = Tracker::default();
        for i in 0..4 {
            let pose = Pose {
                at: f64::from(i) * 0.04,
                id: i as u64,
                body: i / 3,
                pass: (i % 3 + 1) as u8,
                coordinates: [14, 0, 80],
                phase: 0,
                controls: 8,
                horizon: 40,
                origin: 106,
                velocity: [1, 0],
                source: [13, 4, 1, 18, 4, 1],
            };
            t.observe(pose, pose.at, false);
        }
        let epoch = Instant::now();
        for ms in [135, 200] {
            // active prediction, then post-horizon settling
            let mut pause = HostPause::default();
            let at = epoch + Duration::from_millis(ms);
            let query = |now| pause.now(now).duration_since(epoch).as_secs_f64();
            let expected = t.sample(query(at)).unwrap();
            pause.toggle(at);
            assert_eq!(
                t.sample(
                    pause
                        .now(at + Duration::from_secs(5))
                        .duration_since(epoch)
                        .as_secs_f64()
                )
                .unwrap()
                .anchors,
                expected.anchors
            );
            pause.toggle(at + Duration::from_secs(5));
            let resumed = pause
                .now(at + Duration::from_millis(5010))
                .duration_since(epoch)
                .as_secs_f64();
            assert_eq!(
                t.sample(resumed).unwrap().anchors,
                t.sample(ms as f64 / 1000.0 + 0.01).unwrap().anchors
            );
        }
    }
}
