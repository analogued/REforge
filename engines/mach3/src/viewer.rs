// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and live gameplay.

use reforge_sdl::{
    FrameFormat, FrameTexture, HostPause, HostShortcut, Window,
    host_shortcut as shared_host_shortcut,
    settings::{SettingsMenu, VideoCapabilities},
};
use std::collections::{BTreeSet, VecDeque};

mod input_history;
mod realtime;
use input_history::{EventTime, InputHistory};

#[cfg(feature = "dev-tools")]
use crate::input_trace::{InputTrace, PollSite};
#[cfg(feature = "dev-tools")]
use crate::timing_trace::TimingTrace;

use reforge_sdl::sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
};

use crate::{
    assets::GraphicsAssets,
    frame::{Frame, HEIGHT, WIDTH},
    gameplay::{
        BODY_PERIOD, DisplayMode, Ending, Gameplay, PauseEvent, PauseState, ScriptRules,
        SessionState, Steering, TransitionFrame,
    },
    startup::{Presentation, Screen, StartupFrames},
};

// CGA mode 4 stores 320x200 pixels but fills a 4:3 display (pixel aspect 5:6).
const DISPLAY_HEIGHT: u32 = 240;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playback {
    #[default]
    Animated,
    Step,
    Static,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub f3_mode: crate::gameplay::F3Mode,
    /// None fits the usable display; Some requests a fixed drawable multiplier.
    pub scale: Option<u32>,
    pub presentation: reforge_sdl::Presentation,
    pub playback: Playback,
    pub controls: bool,
    pub audio_renderer: crate::sound::render::AudioRenderer,
    pub body_period: std::time::Duration,
    #[cfg(feature = "dev-tools")]
    pub input_trace: Option<std::path::PathBuf>,
    #[cfg(feature = "dev-tools")]
    pub timing_trace: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            f3_mode: crate::gameplay::F3Mode::Press,
            scale: None,
            presentation: reforge_sdl::Presentation::Pixels,
            playback: Playback::Animated,
            controls: false,
            audio_renderer: crate::sound::render::AudioRenderer::Speaker,
            body_period: BODY_PERIOD,
            #[cfg(feature = "dev-tools")]
            input_trace: None,
            #[cfg(feature = "dev-tools")]
            timing_trace: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Space,
    Close,
    ToggleHostPause,
}

fn title(screen: Screen) -> &'static str {
    match screen {
        Screen::Splash => "REforge — Mach 3 — Splash (Space to advance)",
        Screen::Intro => "REforge — Mach 3 — Intro (Space to advance)",
        Screen::InitialScene => "REforge — Mach 3 — Initial scene (Esc to close)",
    }
}

/// Recognize P through the same host-pause policy as Pause/Break. The original
/// event remains available for tracing; this alias never enters the DOS input.
fn host_shortcut(event: &Event) -> Option<HostShortcut> {
    if let Event::KeyDown {
        keycode: Some(Keycode::P),
        ..
    } = event
    {
        let mut alias = event.clone();
        if let Event::KeyDown { keycode, .. } = &mut alias {
            *keycode = Some(Keycode::Pause);
        }
        return shared_host_shortcut(&alias);
    }
    shared_host_shortcut(event)
}

/// The settings menu owns only a temporary host pause. It never changes the
/// native F1 state and cannot resume a game that was already host-paused.
#[derive(Default)]
struct MenuPause {
    open: bool,
    owns_pause: bool,
}

impl MenuPause {
    fn update(&mut self, open: bool, pause: &mut HostPause, now: std::time::Instant) -> bool {
        if open == self.open {
            return false;
        }
        if open {
            self.owns_pause = !pause.is_paused();
            if self.owns_pause {
                pause.toggle(now);
            }
        } else if self.owns_pause {
            pause.toggle(now);
            self.owns_pause = false;
        }
        self.open = open;
        true
    }
}

fn host_quit(event: &Event) -> bool {
    host_shortcut(event) == Some(HostShortcut::Quit)
        || matches!(
            event,
            Event::Quit { .. }
                | Event::Window {
                    win_event: WindowEvent::CloseRequested,
                    ..
                }
        )
}

fn prepare_settings(menu: &mut SettingsMenu<'_>, window: &mut Window) {
    menu.set_status(window.renderer_status());
    if let Some(settings) = menu.prepare(window.ui_metrics(), window.video_settings()) {
        if let Err(error) = window.apply_video_settings(settings) {
            eprintln!("warning: could not apply video settings: {error}");
            menu.set_error(error);
        } else {
            menu.applied();
        }
    }
}

fn event_action(presentation: &mut Presentation, event: Event, host_paused: bool) -> Action {
    match host_shortcut(&event) {
        Some(HostShortcut::Quit) => return Action::Close,
        Some(HostShortcut::TogglePause) => return Action::ToggleHostPause,
        None => {}
    }
    match event {
        Event::Quit { .. }
        | Event::Window {
            win_event: WindowEvent::CloseRequested,
            ..
        }
        | Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return Action::Close,
        Event::KeyDown {
            keycode: Some(Keycode::Space),
            repeat,
            ..
        } => {
            if !host_paused && presentation.space_down(repeat) {
                return Action::Space;
            }
        }
        Event::KeyUp {
            keycode: Some(Keycode::Space),
            ..
        } => presentation.space_up(),
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => presentation.focus_lost(),
        _ => {}
    }
    Action::Continue
}

#[cfg(test)]
fn handle_event(presentation: &mut Presentation, event: Event) -> bool {
    event_action(presentation, event, false) != Action::Close
}

/// Open a resizable nearest-neighbor viewer. No audio subsystem is initialized.
pub fn run(frames: &StartupFrames, scale: u32) -> Result<(), String> {
    run_with_options(
        frames,
        Options {
            scale: Some(scale),
            ..Options::default()
        },
    )
}

pub fn run_with_options(frames: &StartupFrames, options: Options) -> Result<(), String> {
    run_inner(frames, options, None)
}

/// Run with externally decoded encounter rules, including retained-session restarts.
pub fn run_with_rules(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
) -> Result<(), String> {
    run_inner(frames, options, Some(rules))
}

/// Run animated gameplay on the reconstructed PIT timeline with live sound.
pub fn run_with_sound(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
    samples: crate::sound::SoundAssets,
    intro: crate::sound::IntroProgram,
    procedural: crate::sound::ProceduralProgram,
) -> Result<(), String> {
    if options.playback != Playback::Animated || options.body_period != BODY_PERIOD {
        eprintln!("warning: step/static and custom-period diagnostics use silent nominal stepping");
        return run_inner(frames, options, Some(rules));
    }
    if options
        .scale
        .is_some_and(|scale| !(1..=16).contains(&scale))
    {
        return Err("window scale must be auto or an integer between 1 and 16".into());
    }
    realtime::run(
        frames,
        options,
        rules,
        realtime::Resources {
            samples,
            intro,
            procedural,
        },
    )
}

fn scan_for_key(key: Keycode) -> Option<u8> {
    Some(match key {
        Keycode::S => 0x1f,
        Keycode::X => 0x2d,
        Keycode::B => 0x30,
        Keycode::N => 0x31,
        Keycode::Up => 0x48,
        Keycode::Down => 0x50,
        Keycode::Left => 0x4b,
        Keycode::Right => 0x4d,
        Keycode::Kp5 => 0x4c,
        Keycode::Space => 0x39,
        Keycode::F1 => 0x3b,
        Keycode::F2 => 0x3c,
        Keycode::F3 => 0x3d,
        _ => return None,
    })
}

fn event_scan(event: &Event, gameplay: bool) -> Option<u8> {
    match event {
        Event::KeyDown {
            keycode: Some(key), ..
        }
        | Event::KeyUp {
            keycode: Some(key), ..
        } => {
            if *key == Keycode::Space && !gameplay && matches!(event, Event::KeyDown { .. }) {
                return None;
            }
            if let Some(scan) = scan_for_key(*key) {
                let raw = scan
                    | if matches!(event, Event::KeyUp { .. }) {
                        128
                    } else {
                        0
                    };
                return Some(raw);
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
fn control_event(session: &mut SessionState, event: &Event, gameplay: bool) {
    if let Some(scan) = event_scan(event, gameplay) {
        session.keyboard.apply_scan(scan);
    } else if matches!(
        event,
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        }
    ) {
        session.clear_held_keys();
    }
}

/// Host repeat eligibility is separate from the reconstructed DOS scan-byte filter.
#[derive(Default)]
struct ControlInput {
    fresh: BTreeSet<u8>,
}

impl ControlInput {
    fn clear(&mut self, session: &mut SessionState) {
        self.fresh.clear();
        session.clear_held_keys();
    }

    fn accept(&mut self, event: &Event) -> bool {
        match event {
            Event::KeyDown {
                keycode: Some(key),
                repeat,
                ..
            } => {
                if let Some(scan) = scan_for_key(*key) {
                    if *repeat && !self.fresh.contains(&scan) {
                        return false;
                    }
                    if !repeat {
                        self.fresh.insert(scan);
                    }
                }
            }
            Event::KeyUp {
                keycode: Some(key), ..
            } => {
                if let Some(scan) = scan_for_key(*key) {
                    self.fresh.remove(&scan);
                }
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => self.fresh.clear(),
            _ => {}
        }
        true
    }

    #[cfg(test)]
    fn handle(&mut self, session: &mut SessionState, event: &Event, gameplay: bool) {
        if self.accept(event) {
            control_event(session, event, gameplay);
        }
    }

    fn receive(
        &mut self,
        session: &mut SessionState,
        history: &mut InputHistory,
        event: &Event,
        gameplay: bool,
        time: EventTime,
    ) -> &'static str {
        if matches!(
            event,
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            }
        ) {
            self.clear(session);
            history.reset(time);
            return "focus_reset";
        }
        // Check the original host epoch before accepting its mapped timeline.
        // Old SDL events must not become fresh input in the resumed epoch.
        if !history.accepts(time.host) {
            return "ignored_before_reset";
        }
        if !self.accept(event) {
            return "ignored_repeat";
        }
        if let Some(scan) = event_scan(event, gameplay) {
            if gameplay {
                history.push(time.timeline, scan);
                return "queued";
            }
            history.apply_until(session, time.now);
            session.keyboard.apply_scan(scan);
        }
        "handled"
    }
}

fn toggle_host_pause(
    pause: &mut HostPause,
    session: &mut SessionState,
    input: &mut ControlInput,
    now: std::time::Instant,
) {
    pause.toggle(now);
    input.clear(session);
}

fn steering_from_mask(mask: u8, enabled: bool) -> Steering {
    if !enabled {
        return Steering::default();
    }
    Steering {
        up: mask & 1 != 0,
        down: mask & 2 != 0,
        left: mask & 4 != 0,
        right: mask & 8 != 0,
        fire: mask & 16 != 0,
    }
}

/// A fixed 640-pixel texture preserves both raw mode-6 bits and mode-4 pixels.
fn display_rgb(frame: &Frame, mode: DisplayMode, family: u8) -> Vec<u8> {
    let mut output = Vec::with_capacity(WIDTH * 2 * HEIGHT * 3);
    if mode == DisplayMode::Color {
        let palette = if family == 0 {
            [[0, 0, 0], [85, 255, 85], [255, 85, 85], [255, 255, 85]]
        } else {
            crate::frame::PALETTE
        };
        for byte in frame.packed() {
            for shift in [6, 4, 2, 0] {
                let pixel = palette[usize::from((byte >> shift) & 3)];
                output.extend_from_slice(&pixel);
                output.extend_from_slice(&pixel);
            }
        }
    } else {
        for byte in frame.packed() {
            for shift in (0..8).rev() {
                let level = if byte & (1 << shift) != 0 { 255 } else { 0 };
                output.extend_from_slice(&[level; 3]);
            }
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn advance_game_pass_observed(
    gameplay: &mut Gameplay,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
    #[cfg(feature = "dev-tools")] trace: &mut Option<InputTrace>,
    #[cfg(feature = "dev-tools")] timing: &mut Option<TimingTrace>,
    #[cfg(feature = "dev-tools")] poll_time: (std::time::Instant, Option<std::time::Instant>),
) -> Result<bool, String> {
    let result = session.keyboard.poll_controls(&mut session.controls, 0);
    if result.display_toggled {
        gameplay.reset_video_palette();
    }
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = trace {
        let state = gameplay.snapshot();
        trace.poll(
            PollSite {
                label: "steering",
                timeline: poll_time.0,
                deadline: poll_time.1,
                body: state.body,
                pass: state.render_pass,
                controls,
            },
            session,
            result,
        );
    }
    #[cfg(feature = "dev-tools")]
    let host_start = timing.as_mut().map(TimingTrace::now);
    gameplay
        .advance_subframe(assets, steering_from_mask(result.mask, controls))
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = timing {
        let end = trace.now();
        trace.span(
            "pass",
            poll_time.0,
            poll_time.1,
            (host_start.unwrap(), end),
            Some(&gameplay.snapshot()),
            if gameplay.pass_one_wait_required() {
                "extra_wait=true"
            } else {
                "extra_wait=false"
            },
        );
    }
    // Pause ran after the original exit gate. Enter the committed body before
    // setting the flag so complete() permits its remaining two render passes.
    if *quit_after_body_entry {
        gameplay.request_game_over();
        *quit_after_body_entry = false;
    }
    Ok(result.display_toggled)
}

#[cfg(test)]
fn advance_game_pass(
    gameplay: &mut Gameplay,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
) -> Result<bool, String> {
    advance_game_pass_observed(
        gameplay,
        assets,
        session,
        controls,
        quit_after_body_entry,
        #[cfg(feature = "dev-tools")]
        &mut None,
        #[cfg(feature = "dev-tools")]
        &mut None,
        #[cfg(feature = "dev-tools")]
        (std::time::Instant::now(), None),
    )
}

fn pause_waiting(state: PauseState) -> bool {
    matches!(
        state,
        PauseState::WaitBeforePress(_) | PauseState::WaitBeforeDecision(_)
    )
}

#[derive(Default)]
struct TransitionPlayback {
    frames: VecDeque<TransitionFrame>,
    deadline: Option<std::time::Instant>,
}

impl TransitionPlayback {
    fn append(
        &mut self,
        frames: Vec<TransitionFrame>,
        now: std::time::Instant,
        unit: std::time::Duration,
    ) {
        self.frames.extend(frames);
        if self.deadline.is_none() {
            self.deadline = self
                .frames
                .front()
                .map(|step| now + unit * u32::from(step.wait_count));
        }
    }

    fn advance(&mut self, now: std::time::Instant, unit: std::time::Duration) -> bool {
        if !self.deadline.is_some_and(|deadline| now >= deadline) {
            return false;
        }
        self.frames.pop_front();
        self.deadline = self
            .frames
            .front()
            .map(|step| now + unit * u32::from(step.wait_count));
        true
    }

    fn active(&self) -> bool {
        !self.frames.is_empty()
    }
}

/// Mach 3's scheduling state is independent of how often SDL presents a frame.
struct GamePlayback {
    session: SessionState,
    history: InputHistory,
    gameplay: Option<Gameplay>,
    ending: Option<Ending>,
    next_frame: Option<std::time::Instant>,
    // A composed pass is not necessarily visible until its separate CGA-copy
    // deadline. Keep publication independent from input/state advancement.
    visible_frame: Option<Frame>,
    pending_publication: bool,
    pause_tick: Option<std::time::Instant>,
    transitions: TransitionPlayback,
    body_polled: bool,
    quit_after_body_entry: bool,
    pending_step: bool,
    color_family: u8,
    #[cfg(feature = "dev-tools")]
    trace: Option<InputTrace>,
    #[cfg(feature = "dev-tools")]
    timing: Option<TimingTrace>,
}

impl Default for GamePlayback {
    fn default() -> Self {
        Self {
            session: SessionState::default(),
            history: InputHistory::default(),
            gameplay: None,
            ending: None,
            next_frame: None,
            visible_frame: None,
            pending_publication: false,
            pause_tick: None,
            transitions: TransitionPlayback::default(),
            body_polled: false,
            quit_after_body_entry: false,
            pending_step: false,
            color_family: 1,
            #[cfg(feature = "dev-tools")]
            trace: None,
            #[cfg(feature = "dev-tools")]
            timing: None,
        }
    }
}

impl GamePlayback {
    fn begin_game(
        &mut self,
        assets: &GraphicsAssets,
        rules: Option<&ScriptRules>,
        options: &Options,
        now: std::time::Instant,
    ) -> Result<(), String> {
        #[cfg(feature = "dev-tools")]
        let timing_start = self.timing.as_mut().map(TimingTrace::now);
        // Fold intro/ending input before initialization. Keep the engine's
        // keyboard table and duplicate byte; do not restore a host snapshot.
        self.history.apply_until(&mut self.session, now);
        let start = self.session.begin_game();
        self.gameplay = Some(
            if let Some(rules) = rules {
                Gameplay::for_play(assets, rules.clone(), start)
            } else if options.controls {
                Gameplay::with_steering(assets)
            } else {
                Gameplay::new(assets)
            }
            .map_err(|error| error.to_string())?,
        );
        self.visible_frame = Some(self.gameplay.as_ref().unwrap().frame().clone());
        self.pending_publication = false;
        self.color_family = self.gameplay.as_ref().unwrap().palette_family();
        self.ending = None;
        self.transitions = TransitionPlayback::default();
        self.body_polled = false;
        self.quit_after_body_entry = false;
        self.pending_step = false;
        self.next_frame = Some(now + options.body_period / 4);
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.timing {
            let end = trace.now();
            trace.begin_game();
            trace.span(
                "game_start",
                now,
                self.next_frame,
                (timing_start.unwrap(), end),
                Some(&self.gameplay.as_ref().unwrap().snapshot()),
                &format!(
                    "controls={} playback={:?}",
                    options.controls, options.playback
                ),
            );
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.trace {
            trace.boundary("game_started", now, self.next_frame, &self.session, "");
        }
        Ok(())
    }

    fn toggle_host_pause(
        &mut self,
        pause: &mut HostPause,
        input: &mut ControlInput,
        received: std::time::Instant,
    ) {
        toggle_host_pause(pause, &mut self.session, input, received);
        self.reset_host_input(input, pause, received);
    }

    fn reset_host_input(
        &mut self,
        input: &mut ControlInput,
        pause: &HostPause,
        received: std::time::Instant,
    ) {
        input.clear(&mut self.session);
        let now = pause.now(received);
        self.history.reset(EventTime {
            host: received,
            timeline: now,
            now,
        });
        self.pending_step = false;
    }

    fn ordinary_due(&self, options: &Options, now: std::time::Instant) -> bool {
        options.playback == Playback::Animated
            && self.ending.is_none()
            && !self.transitions.active()
            && self.session.pause.state == PauseState::Running
            && self
                .gameplay
                .as_ref()
                .is_some_and(|gameplay| !gameplay.complete())
            && self.next_frame.is_some_and(|deadline| now >= deadline)
    }

    fn advance(
        &mut self,
        assets: &GraphicsAssets,
        options: &Options,
        now: std::time::Instant,
    ) -> Result<bool, String> {
        let mut repaint = false;
        let unit = options.body_period / 4;
        // These intervals have deliberate waits, not ordinary deadline debt.
        // Fold their scan history without manufacturing additional guest polls.
        if options.playback != Playback::Animated
            || self.transitions.active()
            || self.ending.is_some()
            || self.session.pause.state != PauseState::Running
            || self.gameplay.as_ref().is_none_or(Gameplay::complete)
        {
            self.history.apply_until(&mut self.session, now);
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.trace {
            trace.boundary(
                "batch",
                now,
                self.next_frame,
                &self.session,
                if self.transitions.active() {
                    "transition=true"
                } else {
                    "transition=false"
                },
            );
        }
        let transition_was_active = self.transitions.active();
        #[cfg(feature = "dev-tools")]
        let previous_transition_deadline = self.transitions.deadline;
        #[cfg(feature = "dev-tools")]
        let transition_start = self
            .transitions
            .deadline
            .is_some_and(|deadline| now >= deadline)
            .then(|| self.timing.as_mut().map(TimingTrace::now))
            .flatten();
        let transition_frame = self
            .transitions
            .frames
            .front()
            .map(|step| step.frame.clone());
        if self.transitions.advance(now, unit) {
            // Retain the last displayed transition after its queue is drained;
            // the prepared ordinary framebuffer need not have been published.
            self.visible_frame = transition_frame;
            repaint = true;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.timing {
                let end = trace.now();
                trace.span(
                    "transition_wait",
                    now,
                    previous_transition_deadline,
                    (transition_start.unwrap(), end),
                    self.gameplay.as_ref().map(Gameplay::snapshot).as_ref(),
                    &format!(
                        "remaining={} next_wait_count={}",
                        self.transitions.frames.len(),
                        self.transitions
                            .frames
                            .front()
                            .map_or(0, |frame| frame.wait_count)
                    ),
                );
            }
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.trace {
                trace.rebase(
                    "transition_frame_wait",
                    now,
                    previous_transition_deadline,
                    self.transitions.deadline,
                    &self.session,
                );
            }
            if !self.transitions.active() {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    trace.rebase(
                        "transition_finished",
                        now,
                        self.next_frame,
                        Some(now + unit),
                        &self.session,
                    );
                }
                self.next_frame = Some(now + unit);
            }
        }
        // Each intentional transition retains its own displayed wait. Do not
        // spend ordinary-frame catch-up debt inside that blocking sequence.
        if transition_was_active {
            return Ok(repaint);
        }
        if self.session.pause.state != PauseState::Running {
            if pause_waiting(self.session.pause.state) {
                if self.pause_tick.is_some_and(|deadline| now >= deadline) {
                    self.session.pause.tick();
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        trace.rebase(
                            "native_pause_tick",
                            now,
                            self.pause_tick,
                            Some(now + unit),
                            &self.session,
                        );
                    }
                    self.pause_tick = Some(now + unit);
                }
            } else {
                let result = self
                    .session
                    .keyboard
                    .poll_controls(&mut self.session.controls, 0);
                if result.display_toggled {
                    if let Some(gameplay) = &mut self.gameplay {
                        gameplay.reset_video_palette();
                    }
                    // Native pause may remain active without an ordinary pass.
                    self.color_family = 1;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    let state = self.gameplay.as_ref().map(Gameplay::snapshot);
                    trace.poll(
                        PollSite {
                            label: "native_pause",
                            timeline: now,
                            deadline: None,
                            body: state.as_ref().map_or(0, |state| state.body),
                            pass: state.as_ref().map_or(0, |state| state.render_pass),
                            controls: options.controls,
                        },
                        &self.session,
                        result,
                    );
                }
                repaint |= result.display_toggled;
                match self.session.pause.poll(result.mask) {
                    PauseEvent::EndGame => self.quit_after_body_entry = true,
                    PauseEvent::Resumed => {
                        #[cfg(feature = "dev-tools")]
                        if let Some(trace) = &mut self.trace {
                            trace.rebase(
                                "native_pause_resumed",
                                now,
                                self.next_frame,
                                Some(now),
                                &self.session,
                            );
                        }
                        self.next_frame = Some(now);
                    }
                    _ => {}
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    trace.boundary(
                        "native_pause_phase",
                        now,
                        self.next_frame,
                        &self.session,
                        &format!("end_game_committed={}", self.quit_after_body_entry),
                    );
                }
                if pause_waiting(self.session.pause.state) {
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        trace.rebase(
                            "native_pause_wait",
                            now,
                            self.pause_tick,
                            Some(now + unit),
                            &self.session,
                        );
                    }
                    self.pause_tick = Some(now + unit);
                }
            }
        }
        // Catch up preparation/publication events before one presentation.
        // Bound each batch for responsiveness; retain remaining deadline debt.
        for _ in 0..64 {
            if self.transitions.active() || self.session.pause.state != PauseState::Running {
                break;
            }
            let timed = options.playback == Playback::Animated
                && self.next_frame.is_some_and(|deadline| now >= deadline);
            if !self.pending_step && !timed {
                break;
            }
            let Some(gameplay) = self.gameplay.as_mut() else {
                break;
            };
            let poll_at = self.next_frame.filter(|_| timed).unwrap_or(now);
            if self.pending_publication {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                self.visible_frame = Some(gameplay.pass_frame().clone());
                self.pending_publication = false;
                repaint = true;
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    trace.span(
                        "pass_publish",
                        now,
                        Some(poll_at),
                        (host_start.unwrap(), end),
                        gameplay.pass_snapshot(),
                        "delayed=true",
                    );
                }
                self.next_frame = Some(poll_at + unit);
                continue;
            }
            self.history.apply_until(&mut self.session, poll_at);
            if let Some(ending) = self.ending.as_mut() {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                #[cfg(feature = "dev-tools")]
                let previous_wait_units = ending.wait_units();
                ending.advance(assets).map_err(|error| error.to_string())?;
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    let state = ending.snapshot();
                    trace.span(
                        "ending_step",
                        now,
                        self.next_frame.filter(|_| timed),
                        (host_start.unwrap(), end),
                        Some(&gameplay.snapshot()),
                        &format!(
                            "title={:?} stage={:?} draw={} hold={} wait_units={} previous_wait_units={}",
                            state.title,
                            state.stage,
                            state.draw_index,
                            state.hold_tick,
                            ending.wait_units(), previous_wait_units
                        ),
                    );
                }
                self.pending_step = false;
                repaint = true;
            } else if gameplay.complete() && gameplay.snapshot().player.game_over {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                self.ending = Some(
                    Ending::new_with_best(
                        assets,
                        &gameplay.snapshot(),
                        gameplay.frame(),
                        self.session.best_score,
                    )
                    .map_err(|error| error.to_string())?,
                );
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    trace.span(
                        "ending_start",
                        now,
                        self.next_frame.filter(|_| timed),
                        (host_start.unwrap(), end),
                        Some(&gameplay.snapshot()),
                        "wait_units=2",
                    );
                }
                self.pending_step = false;
                repaint = true;
            } else if !gameplay.complete() {
                if gameplay.at_body_boundary() && !self.body_polled {
                    let result = self
                        .session
                        .keyboard
                        .poll_controls(&mut self.session.controls, 0);
                    if result.display_toggled {
                        gameplay.reset_video_palette();
                    }
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        let state = gameplay.snapshot();
                        trace.poll(
                            PollSite {
                                label: "body_entry",
                                timeline: now,
                                deadline: self
                                    .next_frame
                                    .filter(|_| options.playback == Playback::Animated),
                                body: state.body,
                                pass: state.render_pass,
                                controls: options.controls,
                            },
                            &self.session,
                            result,
                        );
                    }
                    repaint |= result.display_toggled;
                    self.body_polled = true;
                    self.session.pause.poll(result.mask);
                    #[cfg(feature = "dev-tools")]
                    if self.session.pause.state != PauseState::Running {
                        if let Some(trace) = &mut self.trace {
                            trace.boundary(
                                "native_pause_phase",
                                now,
                                self.next_frame,
                                &self.session,
                                &format!("end_game_committed={}", self.quit_after_body_entry),
                            );
                        }
                    }
                }
                if self.session.pause.state == PauseState::Running {
                    let passes = if options.playback == Playback::Step {
                        3 - if gameplay.at_body_boundary() {
                            0
                        } else {
                            gameplay.snapshot().render_pass
                        }
                    } else {
                        1
                    };
                    for _ in 0..passes {
                        repaint |= advance_game_pass_observed(
                            gameplay,
                            assets,
                            &mut self.session,
                            options.controls,
                            &mut self.quit_after_body_entry,
                            #[cfg(feature = "dev-tools")]
                            &mut self.trace,
                            #[cfg(feature = "dev-tools")]
                            &mut self.timing,
                            #[cfg(feature = "dev-tools")]
                            (
                                now,
                                self.next_frame
                                    .filter(|_| options.playback == Playback::Animated),
                            ),
                        )?;
                        self.pending_publication = options.playback == Playback::Animated
                            && gameplay.pass_one_wait_required();
                        if !self.pending_publication {
                            #[cfg(feature = "dev-tools")]
                            let host_start = self.timing.as_mut().map(TimingTrace::now);
                            self.visible_frame = Some(gameplay.pass_frame().clone());
                            repaint = true;
                            #[cfg(feature = "dev-tools")]
                            if let Some(trace) = &mut self.timing {
                                let end = trace.now();
                                trace.span(
                                    "pass_publish",
                                    now,
                                    self.next_frame.filter(|_| timed),
                                    (host_start.unwrap(), end),
                                    gameplay.pass_snapshot(),
                                    "delayed=false",
                                );
                            }
                        }
                        self.transitions
                            .append(gameplay.take_transitions(), now, unit);
                        repaint |= self.transitions.active();
                        #[cfg(feature = "dev-tools")]
                        if self.transitions.active() {
                            if let Some(trace) = &mut self.trace {
                                trace.boundary(
                                    "transition_started",
                                    now,
                                    self.transitions.deadline,
                                    &self.session,
                                    "",
                                );
                            }
                        }
                        if self.transitions.active() {
                            break;
                        }
                    }
                    if gameplay.at_body_boundary() {
                        self.body_polled = false;
                    }
                    self.pending_step =
                        options.playback == Playback::Step && !gameplay.at_body_boundary();
                }
            } else {
                self.pending_step = false;
            }
            self.color_family = gameplay.palette_family();
            let period = self
                .ending
                .as_ref()
                .map_or(unit, |state| options.body_period * state.wait_units() / 4);
            #[cfg(feature = "dev-tools")]
            let previous_deadline = self.next_frame;
            self.next_frame = if gameplay.complete()
                && !gameplay.snapshot().player.game_over
                && self.ending.is_none()
            {
                None
            } else if self.ending.is_some() || !timed {
                // Ending frames carry deliberate visible waits, like transitions.
                Some(now + period)
            } else {
                self.next_frame.map(|deadline| deadline + period)
            };
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.trace {
                if self.ending.is_some() || !timed || self.next_frame.is_none() {
                    trace.rebase(
                        if self.ending.is_some() {
                            "ending_wait"
                        } else if self.next_frame.is_none() {
                            "preview_finished"
                        } else {
                            "step_wait"
                        },
                        now,
                        previous_deadline,
                        self.next_frame,
                        &self.session,
                    );
                }
            }
            if self.ending.is_some()
                || gameplay.complete()
                || options.playback != Playback::Animated
            {
                break;
            }
        }
        if self.transitions.active()
            || self.ending.is_some()
            || self.session.pause.state != PauseState::Running
            || self.next_frame.is_none()
        {
            self.history.apply_until(&mut self.session, now);
        }
        Ok(repaint)
    }
}

fn run_inner(
    frames: &StartupFrames,
    options: Options,
    rules: Option<ScriptRules>,
) -> Result<(), String> {
    let scale = options.scale;
    if scale.is_some_and(|scale| !(1..=16).contains(&scale)) {
        return Err("window scale must be auto or an integer between 1 and 16".into());
    }
    if options.body_period.is_zero() || options.body_period > std::time::Duration::from_secs(5) {
        return Err("opening period must be greater than zero and at most five seconds".into());
    }
    let mut game = GamePlayback::default();
    game.session.keyboard.set_f3_mode(options.f3_mode);
    game.session.select_intro(); // First intro consumes exactly one random word.
    let mut returned_intro = None;
    let mut step_key_held = false;
    let mut host_pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut window = Window::new_with_presentation(
        title(Screen::Splash),
        WIDTH as u32,
        DISPLAY_HEIGHT,
        scale,
        options.presentation,
    )?;
    #[cfg(feature = "dev-tools")]
    if let Some(path) = &options.input_trace {
        game.trace = Some(InputTrace::create(path)?);
        game.trace.as_mut().unwrap().boundary(
            "startup",
            std::time::Instant::now(),
            None,
            &game.session,
            &format!(
                "period_ns={} controls={} playback={:?} f3_mode={}",
                options.body_period.as_nanos(),
                options.controls,
                options.playback,
                options.f3_mode.as_str()
            ),
        );
    }
    #[cfg(feature = "dev-tools")]
    if let Some(path) = &options.timing_trace {
        game.timing = Some(TimingTrace::create(path, options.body_period)?);
    }
    let creator = window.texture_creator();
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
    let mut menu_pause = MenuPause::default();
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    #[cfg(feature = "dev-tools")]
    let present_start = game.timing.as_mut().map(TimingTrace::now);
    texture
        .update(&display_rgb(
            frames.frame(shown),
            DisplayMode::Color,
            game.color_family,
        ))
        .map_err(|error| error.to_string())?;
    window.present(&texture)?;
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = &mut game.timing {
        let end = trace.now();
        trace.span(
            "present",
            host_pause.now(end),
            None,
            (present_start.unwrap(), end),
            None,
            &format!(
                "screen={shown:?} controls={} playback={:?}",
                options.controls, options.playback
            ),
        );
        trace.flush()?;
    }
    loop {
        let mut repaint = false;
        for event in window.poll_events() {
            let clock = reforge_sdl::event_clock_sample();
            let received = std::time::Instant::now();
            let time = EventTime::sample(&event, clock, &host_pause, received);
            repaint |= matches!(event, Event::Window { .. });
            let was_scene = presentation.screen() == Screen::InitialScene;
            // Quit remains global; Escape closes settings before it can quit.
            let menu_consumed = !host_quit(&event) && menu.handle_event(&event);
            if menu_pause.update(menu.is_open(), &mut host_pause, received) {
                game.reset_host_input(&mut input, &host_pause, received);
                presentation.focus_lost();
                step_key_held = false;
                repaint = true;
            }
            if menu_consumed {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "settings_menu", &host_pause, &game.session);
                }
                continue;
            }
            let action = event_action(&mut presentation, event.clone(), host_pause.is_paused());
            if action == Action::Close {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "close", &host_pause, &game.session);
                    trace.flush()?;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.timing {
                    trace.flush()?;
                }
                return Ok(());
            }
            if action == Action::ToggleHostPause {
                if was_scene {
                    game.toggle_host_pause(&mut host_pause, &mut input, received);
                    step_key_held = false;
                    repaint = true;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(
                        &event,
                        if was_scene {
                            "host_shortcut"
                        } else {
                            "ignored_outside_gameplay"
                        },
                        &host_pause,
                        &game.session,
                    );
                    if was_scene {
                        trace.boundary(
                            if host_pause.is_paused() {
                                "host_pause_started"
                            } else {
                                "host_pause_ended"
                            },
                            host_pause.now(std::time::Instant::now()),
                            game.next_frame,
                            &game.session,
                            "held_cleared=true",
                        );
                    }
                }
                continue;
            }
            if host_pause.is_paused() {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "ignored_host_pause", &host_pause, &game.session);
                }
                continue;
            }
            match &event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    if !step_key_held {
                        game.pending_step |= was_scene && options.playback == Playback::Step;
                    }
                    step_key_held = true;
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Return),
                    ..
                }
                | Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => step_key_held = false,
                _ => {}
            }
            let disposition = input.receive(
                &mut game.session,
                &mut game.history,
                &event,
                was_scene,
                time,
            );
            #[cfg(not(feature = "dev-tools"))]
            let _ = disposition;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.event_with_input_time(
                    &event,
                    disposition,
                    &host_pause,
                    &game.session,
                    (disposition == "queued")
                        .then(|| game.history.queued_at())
                        .flatten(),
                );
            }
            if action == Action::Space && was_scene {
                if let Some(state) = game.ending.as_mut().filter(|state| state.can_return()) {
                    game.history.apply_until(&mut game.session, time.now);
                    returned_intro = state.returned_intro_choice();
                    game.session
                        .remember_ending(&state.snapshot(), state.special_snapshots());
                    presentation.return_to_intro();
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut game.trace {
                        trace.rebase(
                            "return_to_intro",
                            host_pause.now(std::time::Instant::now()),
                            game.next_frame,
                            None,
                            &game.session,
                        );
                    }
                    game.next_frame = None;
                }
            }
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut game.trace {
            trace.flush()?;
        }
        prepare_settings(&mut menu, &mut window);
        if menu_pause.update(menu.is_open(), &mut host_pause, std::time::Instant::now()) {
            game.reset_host_input(&mut input, &host_pause, std::time::Instant::now());
            presentation.focus_lost();
            step_key_held = false;
            repaint = true;
        }
        repaint |= menu.is_open();
        if host_pause.is_paused() {
            if repaint {
                window.set_title(if menu.is_open() {
                    "REforge — Mach 3 — Settings (F10 or Escape to close, Ctrl+Q to quit)"
                } else {
                    "REforge — Mach 3 — Paused (P or Pause/Break to continue, Ctrl+Q to close)"
                })?;
                window.present_with(&texture, |canvas| menu.paint(canvas))?;
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
            continue;
        }
        if presentation.screen() != shown {
            shown = presentation.screen();
            repaint = true;
            if shown == Screen::Intro {
                game.color_family = 1; // Repeat entry selects palette family1 before intro.
            }
            if shown == Screen::InitialScene && options.playback != Playback::Static {
                game.begin_game(
                    frames.graphics(),
                    rules.as_ref(),
                    &options,
                    host_pause.now(std::time::Instant::now()),
                )?;
                returned_intro = None;
            }
        }
        let now = host_pause.now(std::time::Instant::now());
        if shown == Screen::InitialScene {
            repaint |= game.advance(frames.graphics(), &options, now)?;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.flush()?;
            }
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.timing {
                trace.flush()?;
            }
            // Repump events before the next bounded catch-up batch. Presenting
            // here would add another vsync wait while ordinary work is overdue.
            if game.ordinary_due(&options, now) {
                continue;
            }
        }
        if repaint {
            #[cfg(feature = "dev-tools")]
            let present_start = game.timing.as_mut().map(TimingTrace::now);
            let frame = if shown == Screen::InitialScene {
                game.transitions
                    .frames
                    .front()
                    .map(|step| &step.frame)
                    .or_else(|| game.ending.as_ref().map(Ending::frame))
                    .or(game.visible_frame.as_ref())
                    .or_else(|| game.gameplay.as_ref().map(Gameplay::frame))
                    .unwrap_or_else(|| frames.frame(shown))
            } else if shown == Screen::Intro {
                returned_intro
                    .map_or_else(|| frames.frame(shown), |choice| frames.return_intro(choice))
            } else {
                frames.frame(shown)
            };
            texture
                .update(&display_rgb(
                    frame,
                    game.session.controls.preferences.display_mode,
                    game.color_family,
                ))
                .map_err(|error| error.to_string())?;
            let caption = if game.session.pause.state != PauseState::Running {
                "REforge — Mach 3 — Paused (release keys, then F1 to end game or another key to resume)"
            } else if shown != Screen::InitialScene {
                title(shown)
            } else if game.ending.as_ref().is_some_and(Ending::can_return) {
                "REforge — Mach 3 — Game Over / High Score (Space to return, Esc to close)"
            } else if game.ending.is_some() {
                "REforge — Mach 3 — Game Over / High Score (Esc to close)"
            } else if game.gameplay.as_ref().is_some_and(Gameplay::complete) {
                "REforge — Mach 3 — Preview complete (Esc to close)"
            } else if options.playback == Playback::Step {
                "REforge — Mach 3 — Opening (Enter to step, F1 to pause, Esc to close)"
            } else {
                "REforge — Mach 3 — Opening (F1 to pause, Esc to close)"
            };
            window.set_title(caption)?;
            window.present_with(&texture, |canvas| menu.paint(canvas))?;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.timing {
                let end = trace.now();
                trace.span(
                    "present",
                    now,
                    None,
                    (present_start.unwrap(), end),
                    game.gameplay.as_ref().map(Gameplay::snapshot).as_ref(),
                    &format!(
                        "screen={shown:?} transition={} ending={} host_paused={} pending_publication={}",
                        game.transitions.active(),
                        game.ending.is_some(),
                        host_pause.is_paused(),
                        game.pending_publication
                    ),
                );
                trace.flush()?;
            }
        }
        let poll_interval = std::time::Duration::from_millis(4);
        let deadline = game.transitions.deadline.or_else(|| {
            game.next_frame.filter(|_| {
                options.playback == Playback::Animated
                    && game.session.pause.state == PauseState::Running
            })
        });
        let delay = deadline.map_or(poll_interval, |deadline| {
            deadline
                .saturating_duration_since(host_pause.now(std::time::Instant::now()))
                .min(poll_interval)
        });
        std::thread::sleep(delay);
    }
}

#[cfg(test)]
mod tests {
    mod input_history_cases;

    use super::*;
    use reforge_sdl::sdl3::keyboard::Mod;

    #[test]
    fn global_shortcuts_do_not_change_native_pause_or_advance_while_host_paused() {
        use reforge_sdl::sdl3::keyboard::Scancode;
        let event = |scan, key, mods, repeat| Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: Some(scan),
            keycode: Some(key),
            keymod: mods,
            repeat,
        };
        let mut presentation = Presentation::default();
        assert_eq!(
            event_action(&mut presentation, space(true, false), true),
            Action::Continue
        );
        assert_eq!(presentation.screen(), Screen::Splash);
        let mut session = SessionState::default();
        for (scan, key) in [(Scancode::Pause, Keycode::Pause), (Scancode::P, Keycode::P)] {
            let pause = event(scan, key, Mod::NOMOD, false);
            for host_paused in [false, true] {
                assert_eq!(
                    event_action(&mut presentation, pause.clone(), host_paused),
                    Action::ToggleHostPause
                );
            }
            // Both shortcuts are host actions, not F1 make/break aliases.
            control_event(&mut session, &pause, true);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::gameplay::HeldKeys::default()
            );
            assert_eq!(session.pause.state, PauseState::Running);
            for mods in [
                Mod::NOMOD,
                Mod::LSHIFTMOD,
                Mod::RSHIFTMOD,
                Mod::CAPSMOD,
                Mod::NUMMOD,
                Mod::LCTRLMOD,
                Mod::RCTRLMOD,
                Mod::LALTMOD,
                Mod::RALTMOD,
                Mod::LGUIMOD,
                Mod::RGUIMOD,
            ] {
                for repeat in [false, true] {
                    assert_eq!(
                        host_shortcut(&event(scan, key, mods, repeat)),
                        shared_host_shortcut(&event(Scancode::Pause, Keycode::Pause, mods, repeat))
                    );
                }
            }
        }
        assert_eq!(host_shortcut(&key_event(Keycode::P, false, false)), None);
        // P follows the letter on the active layout, as do Mach 3's other keys.
        assert_eq!(
            host_shortcut(&event(Scancode::A, Keycode::P, Mod::NOMOD, false)),
            Some(HostShortcut::TogglePause)
        );
        assert_eq!(
            host_shortcut(&event(Scancode::P, Keycode::A, Mod::NOMOD, false)),
            None
        );
        assert_eq!(
            event_action(
                &mut presentation,
                event(Scancode::A, Keycode::Q, Mod::RCTRLMOD, false),
                true
            ),
            Action::Close
        );
        assert_eq!(
            event_action(
                &mut presentation,
                event(Scancode::Q, Keycode::Q, Mod::NOMOD, false),
                false
            ),
            Action::Continue
        );
        assert_eq!(Options::default().scale, None);
    }

    fn space(down: bool, repeat: bool) -> Event {
        if down {
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
                which: 0,
                raw: 0,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
                which: 0,
                raw: 0,
            }
        }
    }

    fn key_event(key: Keycode, down: bool, repeat: bool) -> Event {
        let mut event = space(down, repeat);
        match &mut event {
            Event::KeyDown { keycode, .. } | Event::KeyUp { keycode, .. } => *keycode = Some(key),
            _ => unreachable!(),
        }
        event
    }

    #[test]
    fn host_pause_and_focus_loss_preserve_every_native_pause_phase() {
        use std::time::{Duration, Instant};
        for phase in [
            PauseState::Running,
            PauseState::AwaitInitialRelease,
            PauseState::WaitBeforePress(2),
            PauseState::WaitBeforePress(1),
            PauseState::AwaitPress,
            PauseState::WaitBeforeDecision(2),
            PauseState::WaitBeforeDecision(1),
            PauseState::SampleDecision,
            PauseState::AwaitFinalRelease,
        ] {
            let mut session = SessionState::default();
            let mut input = ControlInput::default();
            input.handle(&mut session, &key_event(Keycode::F1, true, false), true);
            session.pause.state = phase;
            session.controls.sound_poll_counter = 3;
            let mut host = HostPause::default();
            let start = Instant::now();
            let deadline = start + Duration::from_millis(100);
            toggle_host_pause(&mut host, &mut session, &mut input, start);
            assert_eq!(session.pause.state, phase);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::gameplay::HeldKeys::default()
            );
            assert_eq!(host.now(start + Duration::from_secs(60)), start);
            toggle_host_pause(
                &mut host,
                &mut session,
                &mut input,
                start + Duration::from_secs(60),
            );
            assert_eq!(session.pause.state, phase);
            assert_eq!(
                deadline.duration_since(host.now(start + Duration::from_secs(60))),
                Duration::from_millis(100)
            );
            input.handle(&mut session, &key_event(Keycode::F1, true, true), true);
            assert!(!session.keyboard.held_keys(0).f1);
            input.handle(
                &mut session,
                &Event::Window {
                    timestamp: 0,
                    window_id: 1,
                    win_event: WindowEvent::FocusLost,
                },
                true,
            );
            assert_eq!(session.pause.state, phase);
            assert_eq!(session.controls.sound_poll_counter, 3);
        }
    }

    #[test]
    fn settings_pause_restores_host_state_and_preserves_native_deadlines() {
        use std::time::{Duration, Instant};
        for already_paused in [false, true] {
            for phase in [
                PauseState::Running,
                PauseState::AwaitInitialRelease,
                PauseState::WaitBeforePress(2),
                PauseState::AwaitPress,
                PauseState::WaitBeforeDecision(2),
                PauseState::SampleDecision,
                PauseState::AwaitFinalRelease,
            ] {
                let start = Instant::now();
                let deadline = start + Duration::from_millis(30);
                let mut game = GamePlayback::default();
                game.session.pause.state = phase;
                game.next_frame = Some(deadline);
                game.pause_tick = Some(deadline);
                let mut input = ControlInput::default();
                input.handle(
                    &mut game.session,
                    &key_event(Keycode::Right, true, false),
                    true,
                );
                let mut host = HostPause::default();
                if already_paused {
                    host.toggle(start);
                }
                let mut menu = MenuPause::default();
                assert!(menu.update(true, &mut host, start));
                game.reset_host_input(&mut input, &host, start);
                assert!(host.is_paused());
                assert!(!menu.update(true, &mut host, start + Duration::from_secs(5)));
                assert_eq!(host.now(start + Duration::from_secs(5)), start);
                let close = start + Duration::from_secs(10);
                assert!(menu.update(false, &mut host, close));
                game.reset_host_input(&mut input, &host, close);
                assert_eq!(host.is_paused(), already_paused);
                assert_eq!(host.now(close), start);
                assert_eq!(game.next_frame, Some(deadline));
                assert_eq!(game.pause_tick, Some(deadline));
                assert_eq!(game.session.pause.state, phase);
                assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
                input.handle(
                    &mut game.session,
                    &key_event(Keycode::Right, true, true),
                    true,
                );
                assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
                input.handle(
                    &mut game.session,
                    &key_event(Keycode::Right, true, false),
                    true,
                );
                assert_eq!(game.session.keyboard.held_keys(0).actions, 8);
            }
        }
    }

    #[test]
    fn cleared_native_pause_handshake_observes_releases_and_fresh_presses() {
        let mut session = SessionState::default();
        let mut input = ControlInput::default();
        session.pause.state = PauseState::AwaitInitialRelease;
        input.clear(&mut session);
        let result = session.controls.poll(session.keyboard.held_keys(0));
        session.pause.poll(result.mask);
        assert_eq!(session.pause.state, PauseState::WaitBeforePress(2));
        session.pause.tick();
        session.pause.tick();
        assert_eq!(session.pause.state, PauseState::AwaitPress);
        input.handle(&mut session, &key_event(Keycode::F1, true, true), true);
        let result = session.controls.poll(session.keyboard.held_keys(0));
        session.pause.poll(result.mask);
        assert_eq!(session.pause.state, PauseState::AwaitPress);
        input.handle(&mut session, &key_event(Keycode::F1, false, false), true);
        input.handle(&mut session, &key_event(Keycode::F1, true, false), true);
        let result = session.controls.poll(session.keyboard.held_keys(0));
        session.pause.poll(result.mask);
        assert_eq!(session.pause.state, PauseState::WaitBeforeDecision(2));
        // Clearing keys is a release, not a reset or a synthetic F1 press.
        input.clear(&mut session);
        session.pause.tick();
        session.pause.tick();
        assert_eq!(session.pause.poll(0), PauseEvent::None);
        assert_eq!(session.pause.state, PauseState::AwaitFinalRelease);
        assert_eq!(session.pause.poll(0), PauseEvent::Resumed);
    }

    #[test]
    fn game_repeats_require_a_fresh_press_after_input_reset() {
        for key in [
            Keycode::S,
            Keycode::X,
            Keycode::B,
            Keycode::N,
            Keycode::Up,
            Keycode::Down,
            Keycode::Left,
            Keycode::Right,
            Keycode::Space,
            Keycode::Kp5,
            Keycode::F1,
            Keycode::F2,
            Keycode::F3,
        ] {
            let mut session = SessionState::default();
            let mut input = ControlInput::default();
            input.handle(&mut session, &key_event(key, true, false), true);
            let held = session.keyboard.held_keys(0);
            assert_ne!(held, crate::gameplay::HeldKeys::default());
            input.clear(&mut session);
            input.handle(&mut session, &key_event(key, true, true), true);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::gameplay::HeldKeys::default()
            );
            input.handle(&mut session, &key_event(key, false, false), true);
            input.handle(&mut session, &key_event(key, true, false), true);
            input.handle(&mut session, &key_event(key, true, true), true);
            assert_eq!(session.keyboard.held_keys(0), held);
            input.handle(&mut session, &key_event(key, false, false), true);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::gameplay::HeldKeys::default()
            );
        }
    }

    #[test]
    fn sdl_events_advance_once_and_close() {
        let mut state = Presentation::default();
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::Intro);
        assert!(handle_event(&mut state, space(true, true)));
        assert_eq!(state.screen(), Screen::Intro);
        assert!(handle_event(
            &mut state,
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost
            }
        ));
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::InitialScene);
        assert!(handle_event(&mut state, space(false, false)));
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::InitialScene);
        assert!(!handle_event(&mut state, Event::Quit { timestamp: 0 }));
        assert!(!handle_event(
            &mut state,
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::CloseRequested
            }
        ));
    }

    fn synthetic_assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![0x55; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|_| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 37,
                    descriptor: [0; 7],
                    color: vec![0; 14 * 37],
                    mask: vec![255; 14 * 37],
                })
                .collect(),
        }
    }

    fn running_playback(
        assets: &GraphicsAssets,
        start: std::time::Instant,
        options: &Options,
    ) -> GamePlayback {
        GamePlayback {
            visible_frame: Some(Gameplay::new(assets).unwrap().frame().clone()),
            gameplay: Some(Gameplay::new(assets).unwrap()),
            next_frame: Some(start + options.body_period / 4),
            ..GamePlayback::default()
        }
    }

    fn pass_count(game: &GamePlayback) -> u32 {
        let state = game.gameplay.as_ref().unwrap().snapshot();
        if state.body == 0 {
            0
        } else {
            u32::from(state.body - 1) * 3 + u32::from(state.render_pass)
        }
    }

    fn receive_at(
        game: &mut GamePlayback,
        input: &mut ControlInput,
        event: &Event,
        at: std::time::Instant,
        received: std::time::Instant,
    ) -> &'static str {
        input.receive(
            &mut game.session,
            &mut game.history,
            event,
            true,
            EventTime {
                host: at,
                timeline: at,
                now: received,
            },
        )
    }

    #[test]
    fn f3_restores_palette_at_body_steering_and_native_pause_polls() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 4;
        let green_game = || {
            let mut data = vec![0; 65536];
            data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
            for index in 0..21 {
                data[0x0a3e + 50 * index..0x0a41 + 50 * index].copy_from_slice(&[0xce, 0xce, 0xcf]);
            }
            let rules = ScriptRules::from_data_segment(&data).unwrap();
            let start_state = SessionState::default().begin_game();
            let mut gameplay = Gameplay::for_play(&assets, rules, start_state).unwrap();
            for _ in 0..21 {
                gameplay.advance(&assets).unwrap();
            }
            assert_eq!(gameplay.palette_family(), 0);
            GamePlayback {
                gameplay: Some(gameplay),
                color_family: 0,
                next_frame: Some(start + unit),
                ..GamePlayback::default()
            }
        };
        for body_already_polled in [false, true] {
            let mut game = green_game();
            game.body_polled = body_already_polled;
            game.session.keyboard.apply_scan(0x3d);
            game.advance(&assets, &options, start + unit).unwrap();
            assert_eq!(game.color_family, 1);
            assert_eq!(game.gameplay.as_ref().unwrap().palette_family(), 1);
            if !body_already_polled {
                // A held F3 switches at body entry and again before steering.
                assert_eq!(
                    game.session.controls.preferences.display_mode,
                    DisplayMode::Color
                );
                game.session.keyboard.apply_scan(0xbd);
            }
            game.advance(&assets, &options, start + unit * 3).unwrap();
            assert_eq!(
                game.session.controls.preferences.display_mode,
                DisplayMode::Color
            );
            assert_eq!(game.color_family, 1);
            game.session.keyboard.apply_scan(0xbd);
            game.advance(&assets, &options, start + unit * 3).unwrap();
            assert_eq!(game.color_family, 1);
        }
        let mut game = green_game();
        let passes = pass_count(&game);
        game.session.pause.state = PauseState::AwaitPress;
        game.session.keyboard.apply_scan(0x3d);
        for now in [start, start + unit] {
            game.advance(&assets, &options, now).unwrap();
            assert_eq!(game.session.pause.state, PauseState::AwaitPress);
            assert_eq!(pass_count(&game), passes);
            assert_eq!(game.color_family, 1);
            assert_eq!(game.gameplay.as_ref().unwrap().palette_family(), 1);
        }
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Color
        );
        // A script write later in the same pass wins over the earlier F3 reset.
        let mut game = green_game();
        let gameplay = game.gameplay.as_mut().unwrap();
        for _ in 0..19 {
            gameplay.advance(&assets).unwrap();
        }
        for _ in 0..2 {
            gameplay
                .advance_subframe(&assets, Steering::default())
                .unwrap();
        }
        game.body_polled = true;
        game.session.controls.preferences.display_mode = DisplayMode::Monochrome;
        game.session.keyboard.apply_scan(0x3d);
        game.advance(&assets, &options, start + unit).unwrap();
        assert_eq!(game.gameplay.as_ref().unwrap().snapshot().body, 41);
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Color
        );
        assert_eq!(game.color_family, 0);
        // F2's fourth active poll takes priority and performs no BIOS mode set.
        let mut game = green_game();
        game.body_polled = true;
        game.session.controls.sound_poll_counter = 3;
        game.session.keyboard.apply_scan(0x3c);
        game.session.keyboard.apply_scan(0x3d);
        game.advance(&assets, &options, start + unit).unwrap();
        assert!(game.session.controls.preferences.sound_disabled);
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Color
        );
        assert_eq!(game.color_family, 0);
    }

    #[test]
    fn publication_wait_retains_pixels_and_consumes_no_input_poll() {
        use std::time::{Duration, Instant};
        let mut assets = synthetic_assets();
        // Different terrain strips guarantee a changed prepared framebuffer.
        for (index, strip) in assets.primary[16_000..].chunks_mut(72 * 15).enumerate() {
            strip.fill(index as u8 * 17);
        }
        let options = Options {
            controls: true,
            ..Options::default()
        };
        let start = Instant::now();
        let unit = options.body_period / 4;
        let mut game = running_playback(&assets, start, &options);
        let initial = game.visible_frame.clone();
        game.advance(&assets, &options, start + unit).unwrap();
        assert!(game.pending_publication);
        assert_eq!(game.visible_frame, initial);
        let prepared = game.gameplay.as_ref().unwrap().pass_frame().clone();
        assert_ne!(Some(&prepared), initial.as_ref());
        let state = format!("{:?}", game.gameplay.as_ref().unwrap().snapshot());
        let session = format!("{:?}", game.session);
        let mut input = ControlInput::default();
        // A tap wholly inside the wait is not steering or a synthetic poll.
        for (offset, down) in [
            (Duration::from_millis(2), true),
            (Duration::from_millis(5), false),
        ] {
            receive_at(
                &mut game,
                &mut input,
                &key_event(Keycode::Right, down, false),
                start + unit + offset,
                start + unit * 2,
            );
        }
        assert!(
            !game
                .advance(
                    &assets,
                    &options,
                    start + unit * 2 - Duration::from_nanos(1)
                )
                .unwrap()
        );
        assert_eq!(game.visible_frame, initial);
        assert!(game.advance(&assets, &options, start + unit * 2).unwrap());
        assert!(!game.pending_publication);
        assert_eq!(game.visible_frame.as_ref(), Some(&prepared));
        assert_eq!(format!("{:?}", game.session), session);
        assert_eq!(
            format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
            state
        );
        assert_eq!(game.next_frame, Some(start + unit * 3));
        game.advance(&assets, &options, start + unit * 3).unwrap();
        assert_eq!(
            game.gameplay
                .as_ref()
                .unwrap()
                .snapshot()
                .player
                .effective_input,
            0
        );
        assert_eq!(
            game.visible_frame.as_ref(),
            Some(game.gameplay.as_ref().unwrap().pass_frame())
        );
    }

    #[test]
    fn changed_layer_publishes_immediately_then_restores_the_extra_wait() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 4;
        let mut data = vec![0; 65536];
        data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
        for index in 0..21 {
            data[0x0a3e + 50 * index..0x0a41 + 50 * index].copy_from_slice(&[0xcb, 0x85, 0xcf]);
        }
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let mut gameplay =
            Gameplay::for_play(&assets, rules, SessionState::default().begin_game()).unwrap();
        for _ in 0..21 {
            gameplay.advance(&assets).unwrap();
        }
        assert!(!gameplay.take_transitions().is_empty());
        let mut game = GamePlayback {
            visible_frame: Some(gameplay.frame().clone()),
            gameplay: Some(gameplay),
            next_frame: Some(start + unit),
            ..GamePlayback::default()
        };
        assert!(game.advance(&assets, &options, start + unit).unwrap());
        assert!(!game.pending_publication);
        assert_eq!(
            game.visible_frame.as_ref(),
            Some(game.gameplay.as_ref().unwrap().pass_frame())
        );
        assert_eq!(game.next_frame, Some(start + unit * 2));
        game.advance(&assets, &options, start + unit * 3).unwrap();
        assert_eq!(game.gameplay.as_ref().unwrap().snapshot().render_pass, 3);
        let old = game.visible_frame.clone();
        game.advance(&assets, &options, start + unit * 4).unwrap();
        assert!(game.pending_publication);
        assert_eq!(game.visible_frame, old);
        assert_eq!(game.next_frame, Some(start + unit * 5));
    }

    #[test]
    fn f3_repaints_previous_pixels_while_first_pass_publication_is_pending() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 4;
        let mut game = running_playback(&assets, start, &options);
        let old = game.visible_frame.clone();
        // Skip the body's function-key poll to exercise one F3 mode switch at
        // the steering poll before composition and its additional wait.
        game.body_polled = true;
        game.session.keyboard.apply_scan(0x3d);
        assert!(game.advance(&assets, &options, start + unit).unwrap());
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Monochrome
        );
        assert!(game.pending_publication);
        assert_eq!(game.visible_frame, old);
        assert_eq!(game.color_family, 1);
        let session = format!("{:?}", game.session);
        game.advance(&assets, &options, start + unit * 2).unwrap();
        assert_eq!(format!("{:?}", game.session), session);
    }

    #[test]
    fn pending_publication_survives_focus_and_host_pause_and_is_reset_by_new_game() {
        use std::time::{Duration, Instant};
        let assets = synthetic_assets();
        let options = Options {
            controls: true,
            ..Options::default()
        };
        let start = Instant::now();
        let unit = options.body_period / 4;
        for focus in [false, true] {
            let mut game = running_playback(&assets, start, &options);
            game.advance(&assets, &options, start + unit).unwrap();
            let old = game.visible_frame.clone();
            let prepared = game.gameplay.as_ref().unwrap().pass_frame().clone();
            let mut input = ControlInput::default();
            if focus {
                receive_at(
                    &mut game,
                    &mut input,
                    &Event::Window {
                        timestamp: 0,
                        window_id: 1,
                        win_event: WindowEvent::FocusLost,
                    },
                    start + unit,
                    start + unit,
                );
            } else {
                let mut pause = HostPause::default();
                game.toggle_host_pause(&mut pause, &mut input, start + unit);
                game.toggle_host_pause(
                    &mut pause,
                    &mut input,
                    start + unit + Duration::from_secs(60),
                );
                assert_eq!(
                    pause.now(start + unit + Duration::from_secs(60)),
                    start + unit
                );
            }
            assert_eq!(game.visible_frame, old);
            assert!(game.pending_publication);
            game.advance(&assets, &options, start + unit * 2).unwrap();
            assert_eq!(game.visible_frame.as_ref(), Some(&prepared));
            game.advance(&assets, &options, start + unit * 5).unwrap();
            assert!(game.pending_publication);
            game.begin_game(&assets, None, &options, start + unit * 5)
                .unwrap();
            assert!(!game.pending_publication);
            assert_eq!(
                game.visible_frame.as_ref(),
                Some(game.gameplay.as_ref().unwrap().frame())
            );
            assert_eq!(game.next_frame, Some(start + unit * 6));
        }
    }

    #[test]
    fn ordinary_simulation_matches_at_15_30_60_and_120_presentations_per_second() {
        use std::time::{Duration, Instant};
        let assets = synthetic_assets();
        let options = Options {
            controls: true,
            ..Options::default()
        };
        let start = Instant::now();
        let end = start + Duration::from_secs(1);
        let unit = options.body_period / 4;
        let mut reference = running_playback(&assets, start, &options);
        for scan in [0x4d, 0x39, 0x3c] {
            reference.session.keyboard.apply_scan(scan);
        }
        // Reference processes exactly one due original pass per call.
        let units = (Duration::from_secs(1).as_nanos() / unit.as_nanos()) as u32;
        let passes = units - (units + 2) / 4;
        for pass in 1..=units {
            reference
                .advance(&assets, &options, start + unit * pass)
                .unwrap();
        }
        assert_eq!(pass_count(&reference), passes);
        assert!(passes > 25);
        for fps in [15, 30, 60, 120] {
            let mut game = running_playback(&assets, start, &options);
            for scan in [0x4d, 0x39, 0x3c] {
                game.session.keyboard.apply_scan(scan);
            }
            // Simulate blocking presentation by advancing the host clock between
            // calls to the exact same scheduling path used by the SDL viewer.
            for frame in 1..=fps {
                let now = start + Duration::from_nanos(1_000_000_000 * frame / fps);
                game.advance(&assets, &options, now).unwrap();
                assert!(!game.transitions.active());
            }
            assert_eq!(pass_count(&game), passes, "presentation rate {fps}");
            // Snapshot has no PartialEq; its Debug covers every reconstructed field.
            assert_eq!(
                format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
                format!("{:?}", reference.gameplay.as_ref().unwrap().snapshot())
            );
            assert_eq!(
                game.gameplay.as_ref().unwrap().frame(),
                reference.gameplay.as_ref().unwrap().frame()
            );
            assert_eq!(
                game.session.controls.preferences,
                reference.session.controls.preferences
            );
            assert_eq!(
                game.session.controls.sound_poll_counter,
                reference.session.controls.sound_poll_counter
            );
            assert_eq!(game.visible_frame, reference.visible_frame);
            assert_eq!(game.pending_publication, reference.pending_publication);
            assert_eq!(game.next_frame, reference.next_frame);
            assert!(game.next_frame.unwrap() > end);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn timing_spans_observe_catch_up_and_blocking_waits_without_changing_state() {
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
        let assets = synthetic_assets();
        let options = Options::default();
        let unit = options.body_period / 4;
        let origin = Instant::now();
        let path = std::env::temp_dir().join(format!(
            "mach3-timing-viewer-{}-{}.tsv",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut ticks = 0;
        let mut trace =
            TimingTrace::create_with_clock(&path, options.body_period, origin, move || {
                ticks += 1;
                origin + Duration::from_micros(ticks)
            })
            .unwrap();
        trace.begin_game();
        let mut game = running_playback(&assets, origin, &options);
        let mut plain = running_playback(&assets, origin, &options);
        game.timing = Some(trace);
        let late = origin + unit * 100;
        game.advance(&assets, &options, late).unwrap();
        plain.advance(&assets, &options, late).unwrap();
        assert_eq!(pass_count(&game), 48);
        assert_eq!(game.next_frame, plain.next_frame);
        assert_eq!(
            format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
            format!("{:?}", plain.gameplay.as_ref().unwrap().snapshot())
        );
        let frame = game.gameplay.as_ref().unwrap().frame().clone();
        let steps = [2, 5]
            .into_iter()
            .map(|wait_count| TransitionFrame {
                frame: frame.clone(),
                clip: crate::frame::Rect {
                    x_bytes: 0,
                    y: 0,
                    width_bytes: 1,
                    height: 1,
                },
                wait_count,
                before_pass: false,
                palette_family: 1,
                state: None,
            })
            .collect::<Vec<_>>();
        game.transitions.append(steps.clone(), late, unit);
        plain.transitions.append(steps, late, unit);
        for at in [late + unit * 20, late + unit * 25] {
            game.advance(&assets, &options, at).unwrap();
            plain.advance(&assets, &options, at).unwrap();
            assert_eq!(pass_count(&game), 48);
            assert_eq!(game.transitions.deadline, plain.transitions.deadline);
            assert_eq!(game.next_frame, plain.next_frame);
            assert_eq!(
                game.gameplay.as_ref().unwrap().frame(),
                plain.gameplay.as_ref().unwrap().frame()
            );
        }
        game.begin_game(&assets, None, &options, late + unit * 26)
            .unwrap();
        game.timing.as_mut().unwrap().flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<Vec<_>> = text
            .lines()
            .skip(2)
            .map(|line| line.split('\t').collect())
            .collect();
        let passes: Vec<_> = rows.iter().filter(|row| row[2] == "pass").collect();
        assert_eq!(passes.len(), 48);
        let publications: Vec<_> = rows.iter().filter(|row| row[2] == "pass_publish").collect();
        assert_eq!(publications.len(), 48);
        for (prepare, publish) in passes.iter().zip(&publications) {
            assert_eq!(&prepare[3..5], &publish[3..5]);
            let deferred = prepare[4] == "1";
            assert_eq!(
                publish[5].parse::<u128>().unwrap(),
                prepare[5].parse::<u128>().unwrap() + if deferred { unit.as_nanos() } else { 0 }
            );
            assert_eq!(
                publish[21],
                if deferred {
                    "delayed=true"
                } else {
                    "delayed=false"
                }
            );
        }
        assert_eq!(passes[0][3], "1");
        assert_eq!(passes[0][4], "1");
        assert_eq!(passes[0][5].parse::<u128>().unwrap(), unit.as_nanos());
        assert_eq!(
            passes[0][6].parse::<u128>().unwrap(),
            (unit * 100).as_nanos()
        );
        for pass in passes {
            assert_eq!(
                pass[8].parse::<i128>().unwrap() - pass[7].parse::<i128>().unwrap(),
                1000
            );
        }
        assert_eq!(
            rows.iter()
                .filter(|row| row[2] == "transition_wait")
                .count(),
            2
        );
        assert_eq!(rows.last().unwrap()[1], "1");
        assert_eq!(rows.last().unwrap()[2], "game_start");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn bounded_catch_up_keeps_debt_without_changing_step_or_static_playback() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let late = start + options.body_period / 4 * 100;
        let mut game = running_playback(&assets, start, &options);
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(pass_count(&game), 48);
        assert!(game.ordinary_due(&options, late));
        assert!(game.next_frame.unwrap() <= late);
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(pass_count(&game), 75);
        assert!(!game.ordinary_due(&options, late));
        assert!(game.next_frame.unwrap() > late);
        for playback in [Playback::Step, Playback::Static] {
            let options = Options {
                playback,
                ..options.clone()
            };
            let mut game = running_playback(&assets, start, &options);
            assert!(!game.advance(&assets, &options, late).unwrap());
            assert_eq!(pass_count(&game), 0);
            if playback == Playback::Step {
                game.pending_step = true;
                assert!(game.advance(&assets, &options, late).unwrap());
                assert_eq!(pass_count(&game), 3);
                assert!(!game.advance(&assets, &options, late).unwrap());
                assert_eq!(pass_count(&game), 3);
            }
        }
    }

    #[test]
    fn catch_up_stops_for_native_pause_and_preserves_transition_waits() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 4;
        let late = start + unit * 100;
        let mut paused = running_playback(&assets, start, &options);
        paused.session.keyboard.apply_scan(0x3b);
        paused.advance(&assets, &options, late).unwrap();
        assert_eq!(paused.session.pause.state, PauseState::AwaitInitialRelease);
        assert_eq!(pass_count(&paused), 0);
        let mut game = running_playback(&assets, start, &options);
        let frame = game.gameplay.as_ref().unwrap().frame().clone();
        game.transitions.append(
            [2, 5]
                .into_iter()
                .map(|wait_count| TransitionFrame {
                    frame: frame.clone(),
                    clip: crate::frame::Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: 1,
                        height: 1,
                    },
                    wait_count,
                    before_pass: false,
                    palette_family: 1,
                    state: None,
                })
                .collect(),
            start,
            unit,
        );
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(game.transitions.frames.len(), 1);
        assert_eq!(game.transitions.deadline, Some(late + unit * 5));
        assert_eq!(pass_count(&game), 0);
        game.advance(&assets, &options, late + unit * 4).unwrap();
        assert_eq!(game.transitions.frames.len(), 1);
        game.advance(&assets, &options, late + unit * 5).unwrap();
        assert!(!game.transitions.active());
        assert_eq!(game.next_frame, Some(late + unit * 6));
        assert_eq!(pass_count(&game), 0);
        game.advance(&assets, &options, late + unit * 6).unwrap();
        assert_eq!(pass_count(&game), 1);
    }

    #[test]
    fn ending_frames_keep_their_visible_waits_instead_of_catching_up() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 4;
        let mut game = running_playback(&assets, start, &options);
        #[cfg(feature = "dev-tools")]
        let path = std::env::temp_dir().join(format!(
            "mach3-ending-timing-{}-{}.tsv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        #[cfg(feature = "dev-tools")]
        {
            let mut trace =
                TimingTrace::create_with_clock(&path, options.body_period, start, move || start)
                    .unwrap();
            trace.begin_game();
            game.timing = Some(trace);
        }
        let gameplay = game.gameplay.as_mut().unwrap();
        gameplay.request_game_over();
        for _ in 0..3 {
            gameplay
                .advance_subframe(&assets, Steering::default())
                .unwrap();
        }
        let mut expected =
            Ending::new_with_best(&assets, &gameplay.snapshot(), gameplay.frame(), 0).unwrap();
        game.advance(&assets, &options, start + unit * 100).unwrap();
        assert!(game.ending.is_some());
        let next = game.next_frame.unwrap();
        game.advance(&assets, &options, next - std::time::Duration::from_nanos(1))
            .unwrap();
        assert_eq!(
            format!("{:?}", game.ending.as_ref().unwrap().snapshot()),
            format!("{:?}", expected.snapshot())
        );
        expected.advance(&assets).unwrap();
        let late = next + unit * 100;
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(
            format!("{:?}", game.ending.as_ref().unwrap().snapshot()),
            format!("{:?}", expected.snapshot())
        );
        assert_eq!(
            game.next_frame,
            Some(late + options.body_period * expected.wait_units() / 4)
        );
        #[cfg(feature = "dev-tools")]
        {
            game.timing.as_mut().unwrap().flush().unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            let kinds: Vec<_> = text
                .lines()
                .skip(2)
                .map(|row| row.split('\t').nth(2).unwrap())
                .collect();
            assert_eq!(kinds, ["ending_start", "ending_step"]);
            assert!(text.contains("previous_wait_units=2"));
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn deferred_pause_exit_finishes_the_committed_body_and_four_input_polls() {
        let assets = GraphicsAssets {
            primary: vec![0; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|_| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: [0; 7],
                    color: vec![0; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        };
        let mut gameplay = Gameplay::with_steering(&assets).unwrap();
        let mut session = SessionState::default();
        session.keyboard.apply_scan(0x3c); // Held F2 at all four input sites.
        session.controls.poll(session.keyboard.held_keys(0)); // Body-entry poll.
        let mut quit = true; // Pause's second F1 press passed the exit gate.
        advance_game_pass(&mut gameplay, &assets, &mut session, true, &mut quit).unwrap();
        assert!(gameplay.snapshot().player.game_over);
        assert!(!gameplay.complete());
        assert_eq!(gameplay.snapshot().render_pass, 1);
        for _ in 0..2 {
            advance_game_pass(&mut gameplay, &assets, &mut session, true, &mut quit).unwrap();
        }
        assert_eq!(gameplay.snapshot().body, 1);
        assert_eq!(gameplay.snapshot().render_pass, 3);
        assert!(gameplay.complete());
        assert!(session.controls.preferences.sound_disabled);
        assert_eq!(session.controls.sound_poll_counter, 0);
    }

    #[test]
    fn startup_space_does_not_fire_and_gameplay_aliases_release_on_focus_loss() {
        let mut session = SessionState::default();
        control_event(&mut session, &space(true, false), false);
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        control_event(&mut session, &space(false, false), false);
        control_event(&mut session, &space(true, false), true);
        let poll = session.controls.poll(session.keyboard.held_keys(0));
        assert!(steering_from_mask(poll.mask, true).fire);
        assert!(!steering_from_mask(poll.mask, false).fire);
        control_event(
            &mut session,
            &Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
            true,
        );
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        assert_eq!(scan_for_key(Keycode::B), Some(0x30));
        assert_eq!(scan_for_key(Keycode::Kp5), Some(0x4c));
        assert_eq!(scan_for_key(Keycode::Return), None);
    }

    #[test]
    fn monochrome_uses_each_raw_bit_while_color_preserves_two_bit_pixels() {
        let mut packed = vec![0; crate::frame::FRAME_BYTES];
        packed[0] = 0x81;
        packed[crate::frame::ROW_BYTES] = 0x40;
        let frame = Frame::from_packed(&packed).unwrap();
        let mono = display_rgb(&frame, DisplayMode::Monochrome, 1);
        assert_eq!(mono.len(), 640 * 200 * 3);
        for x in 0..8 {
            assert_eq!(
                &mono[x * 3..x * 3 + 3],
                &[if x == 0 || x == 7 { 255 } else { 0 }; 3]
            );
        }
        assert_eq!(&mono[(640 + 1) * 3..(640 + 2) * 3], &[255; 3]);
        let color = display_rgb(&frame, DisplayMode::Color, 1);
        let source = frame.rgb();
        for (index, pixel) in source.chunks_exact(3).enumerate() {
            assert_eq!(&color[index * 6..index * 6 + 3], pixel);
            assert_eq!(&color[index * 6 + 3..index * 6 + 6], pixel);
        }
    }

    #[test]
    fn color_palette_families_use_the_original_high_intensity_colors() {
        let mut packed = vec![0; crate::frame::FRAME_BYTES];
        packed[0] = 0x1b; // All four two-bit indices in one byte.
        let frame = Frame::from_packed(&packed).unwrap();
        let green = display_rgb(&frame, DisplayMode::Color, 0);
        let cyan = display_rgb(&frame, DisplayMode::Color, 1);
        for (index, (family0, family1)) in [
            ([0, 0, 0], [0, 0, 0]),
            ([85, 255, 85], [85, 255, 255]),
            ([255, 85, 85], [255, 85, 255]),
            ([255, 255, 85], [255, 255, 255]),
        ]
        .into_iter()
        .enumerate()
        {
            for duplicate in 0..2 {
                let offset = index * 6 + duplicate * 3;
                assert_eq!(green[offset..offset + 3], family0);
                assert_eq!(cyan[offset..offset + 3], family1);
            }
        }
        assert_eq!(
            display_rgb(&frame, DisplayMode::Monochrome, 0),
            display_rgb(&frame, DisplayMode::Monochrome, 1)
        );
    }

    #[test]
    fn transition_queue_displays_each_frame_for_its_wait_and_then_releases_simulation() {
        let now = std::time::Instant::now();
        let unit = std::time::Duration::from_millis(10);
        let frame = Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap();
        let mut playback = TransitionPlayback::default();
        playback.append(
            [2, 5]
                .into_iter()
                .map(|wait_count| TransitionFrame {
                    frame: frame.clone(),
                    clip: crate::frame::Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: 1,
                        height: 1,
                    },
                    wait_count,
                    before_pass: false,
                    palette_family: 1,
                    state: None,
                })
                .collect(),
            now,
            unit,
        );
        assert!(playback.active());
        assert!(!playback.advance(now + unit, unit));
        assert!(playback.advance(now + unit * 2, unit));
        assert_eq!(playback.frames.front().unwrap().wait_count, 5);
        assert!(!playback.advance(now + unit * 6, unit));
        assert!(playback.advance(now + unit * 7, unit));
        assert!(!playback.active());
        assert_eq!(playback.deadline, None);
    }
    #[cfg(feature = "dev-tools")]
    #[test]
    fn input_tracing_preserves_historical_poll_results() {
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
        // Only this test initializes SDL in this test binary. No video device
        // or synthetic SDL device-state update is needed; ControlInput consumes
        // these authored events exactly as it consumes the frontend's events.
        let _sdl = reforge_sdl::sdl3::init().unwrap();
        let folder = std::env::temp_dir().join(format!(
            "reforge-input-trace-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(folder.clone());
        let assets = synthetic_assets();
        let options = Options {
            controls: true,
            ..Options::default()
        };
        let unit = options.body_period / 4;
        let read_rows = |game: &mut GamePlayback, path: &std::path::Path| {
            game.trace.as_mut().unwrap().flush().unwrap();
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .skip(2)
                .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        let assert_same = |left: &GamePlayback, right: &GamePlayback| {
            assert_eq!(
                format!("{:?}", left.session),
                format!("{:?}", right.session)
            );
            assert_eq!(
                format!("{:?}", left.gameplay.as_ref().unwrap().snapshot()),
                format!("{:?}", right.gameplay.as_ref().unwrap().snapshot())
            );
            assert_eq!(
                left.gameplay.as_ref().unwrap().frame(),
                right.gameplay.as_ref().unwrap().frame()
            );
            assert_eq!(left.next_frame, right.next_frame);
            assert_eq!(left.pause_tick, right.pause_tick);
            assert_eq!(left.body_polled, right.body_polled);
            assert_eq!(left.quit_after_body_entry, right.quit_after_body_entry);
        };
        // All events are handled at P+10ms, after a deliberately overdue poll P.
        // Expected-at-P is a temporal reference, not a tap-retention policy.
        for (label, press_delta, release_delta, expected_at_poll) in [
            ("before", -5, -1, 0),
            ("spanning", -5, 5, 8),
            ("after", 5, 20, 0),
        ] {
            let start = Instant::now();
            let path = folder.join(format!("{label}.tsv"));
            let mut game = running_playback(&assets, start, &options);
            let mut plain = running_playback(&assets, start, &options);
            game.trace = Some(InputTrace::create_at(&path, start).unwrap());
            let mut input = ControlInput::default();
            let mut plain_input = ControlInput::default();
            let clock = reforge_sdl::event_clock_sample();
            for (down, delta) in [(true, press_delta), (false, release_delta)] {
                if delta > 10 {
                    continue;
                }
                let mut event = key_event(Keycode::Right, down, false);
                let timestamp = clock.sdl_ns + (unit.as_nanos() as i64 + delta * 1_000_000) as u64;
                match &mut event {
                    Event::KeyDown {
                        timestamp: value, ..
                    }
                    | Event::KeyUp {
                        timestamp: value, ..
                    } => *value = timestamp,
                    _ => unreachable!(),
                }
                let at = if delta < 0 {
                    start + unit - Duration::from_millis((-delta) as u64)
                } else {
                    start + unit + Duration::from_millis(delta as u64)
                };
                let received = start + unit + Duration::from_millis(10);
                receive_at(&mut game, &mut input, &event, at, received);
                receive_at(&mut plain, &mut plain_input, &event, at, received);
                game.trace.as_mut().unwrap().event_with_input_time(
                    &event,
                    "queued",
                    &HostPause::default(),
                    &game.session,
                    game.history.queued_at(),
                );
            }
            let late = start + unit + Duration::from_millis(10);
            game.advance(&assets, &options, late).unwrap();
            plain.advance(&assets, &options, late).unwrap();
            assert_same(&game, &plain);
            let rows = read_rows(&mut game, &path);
            assert!(rows.iter().all(|row| row.len() == 13));
            assert!(rows.iter().filter(|row| row[1] == "event").all(|row| {
                row[12].contains("disposition=queued") && row[12].contains("effective_input_ns=")
            }));
            let polls = rows
                .iter()
                .filter(|row| row[1] == "poll")
                .collect::<Vec<_>>();
            assert_eq!(polls.len(), 2);
            assert_eq!(polls[0][2], "body_entry");
            assert_eq!(polls[1][2], "steering");
            for row in polls {
                assert_eq!(row[8], unit.as_nanos().to_string());
                assert_eq!(
                    row[4],
                    (unit + Duration::from_millis(10)).as_nanos().to_string()
                );
                assert_eq!(row[9], expected_at_poll.to_string());
                assert_eq!(row[11], expected_at_poll.to_string());
            }
        }

        // A full late catch-up batch, function-key handling, native waits and
        // transition waits must produce the same state with tracing enabled.
        let start = Instant::now();
        let path = folder.join("boundaries.tsv");
        let mut game = running_playback(&assets, start, &options);
        let mut plain = running_playback(&assets, start, &options);
        game.trace = Some(InputTrace::create_at(&path, start).unwrap());
        for scan in [0x4d, 0x39, 0x3c] {
            game.session.keyboard.apply_scan(scan);
            plain.session.keyboard.apply_scan(scan);
        }
        let late = start + unit * 100;
        for _ in 0..2 {
            game.advance(&assets, &options, late).unwrap();
            plain.advance(&assets, &options, late).unwrap();
            assert_same(&game, &plain);
        }
        let mut input = ControlInput::default();
        let mut plain_input = ControlInput::default();
        for key in [Keycode::F2, Keycode::F3, Keycode::F1] {
            let event = key_event(key, true, false);
            input.handle(&mut game.session, &event, true);
            plain_input.handle(&mut plain.session, &event, true);
            game.trace.as_mut().unwrap().event(
                &event,
                "handled",
                &HostPause::default(),
                &game.session,
            );
        }
        let next = game.next_frame.unwrap();
        game.advance(&assets, &options, next).unwrap();
        plain.advance(&assets, &options, next).unwrap();
        assert_same(&game, &plain);
        game.session.clear_held_keys();
        plain.session.clear_held_keys();
        game.session.pause.state = PauseState::AwaitInitialRelease;
        plain.session.pause.state = PauseState::AwaitInitialRelease;
        for now in [next, next + unit, next + unit * 2] {
            game.advance(&assets, &options, now).unwrap();
            plain.advance(&assets, &options, now).unwrap();
            assert_same(&game, &plain);
        }
        game.session.pause.state = PauseState::AwaitFinalRelease;
        plain.session.pause.state = PauseState::AwaitFinalRelease;
        game.advance(&assets, &options, next + unit * 3).unwrap();
        plain.advance(&assets, &options, next + unit * 3).unwrap();
        assert_same(&game, &plain);
        let frame = game.gameplay.as_ref().unwrap().frame().clone();
        let steps = [2, 5]
            .into_iter()
            .map(|wait_count| TransitionFrame {
                frame: frame.clone(),
                clip: crate::frame::Rect {
                    x_bytes: 0,
                    y: 0,
                    width_bytes: 1,
                    height: 1,
                },
                wait_count,
                before_pass: false,
                palette_family: 1,
                state: None,
            })
            .collect::<Vec<_>>();
        game.transitions.append(steps.clone(), late, unit);
        plain.transitions.append(steps, late, unit);
        for now in [late + unit * 20, late + unit * 25] {
            game.advance(&assets, &options, now).unwrap();
            plain.advance(&assets, &options, now).unwrap();
            assert_same(&game, &plain);
            assert_eq!(game.transitions.deadline, plain.transitions.deadline);
        }
        let rows = read_rows(&mut game, &path);
        assert!(
            rows.iter()
                .any(|row| row[1] == "poll" && row[12].contains("sound_toggled=true"))
        );
        assert!(
            rows.iter()
                .any(|row| row[1] == "poll" && row[12].contains("display_toggled=true"))
        );
        assert!(rows.iter().any(|row| row[1] == "event" && row[10] == "7"));
        for label in [
            "native_pause_wait",
            "native_pause_tick",
            "native_pause_resumed",
            "transition_frame_wait",
            "transition_finished",
        ] {
            assert!(rows.iter().any(|row| row[2] == label), "missing {label}");
        }
        assert_eq!(
            rows.iter()
                .filter(|row| row[2] == "transition_frame_wait")
                .count(),
            2
        );
        for pair in rows.windows(2) {
            assert!(pair[1][0].parse::<u64>().unwrap() > pair[0][0].parse::<u64>().unwrap());
            assert!(pair[1][3].parse::<i128>().unwrap() >= pair[0][3].parse::<i128>().unwrap());
            assert!(pair[1][5].parse::<u64>().unwrap() >= pair[0][5].parse::<u64>().unwrap());
        }
    }
}
