// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and live gameplay.

use reforge_sdl::{FrameFormat, FrameTexture, HostPause, HostShortcut, Window, host_shortcut};
use std::collections::{BTreeSet, VecDeque};

#[cfg(feature = "dev-tools")]
use crate::input_trace::{InputTrace, PollSite};

use reforge_sdl::sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
};

use crate::{
    assets::GraphicsAssets,
    frame::{Frame, HEIGHT, WIDTH},
    opening::{
        BODY_PERIOD, DisplayMode, Ending, Opening, PauseEvent, PauseState, ScriptRules,
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
    pub scale: u32,
    pub playback: Playback,
    pub controls: bool,
    pub body_period: std::time::Duration,
    #[cfg(feature = "dev-tools")]
    pub input_trace: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: reforge_sdl::DEFAULT_SCALE,
            playback: Playback::Animated,
            controls: false,
            body_period: BODY_PERIOD,
            #[cfg(feature = "dev-tools")]
            input_trace: None,
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
            scale,
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

fn control_event(session: &mut SessionState, event: &Event, gameplay: bool) {
    match event {
        Event::KeyDown {
            keycode: Some(key), ..
        }
        | Event::KeyUp {
            keycode: Some(key), ..
        } => {
            if *key == Keycode::Space && !gameplay && matches!(event, Event::KeyDown { .. }) {
                return;
            }
            if let Some(scan) = scan_for_key(*key) {
                let raw = scan
                    | if matches!(event, Event::KeyUp { .. }) {
                        128
                    } else {
                        0
                    };
                session.keyboard.apply_scan(raw);
            }
        }
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => session.clear_held_keys(),
        _ => {}
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

    fn handle(&mut self, session: &mut SessionState, event: &Event, gameplay: bool) {
        match event {
            Event::KeyDown {
                keycode: Some(key),
                repeat,
                ..
            } => {
                if let Some(scan) = scan_for_key(*key) {
                    if *repeat && !self.fresh.contains(&scan) {
                        return;
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
        control_event(session, event, gameplay);
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

fn advance_game_pass_observed(
    opening: &mut Opening,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
    #[cfg(feature = "dev-tools")] trace: &mut Option<InputTrace>,
    #[cfg(feature = "dev-tools")] poll_time: (std::time::Instant, Option<std::time::Instant>),
) -> Result<bool, String> {
    let result = session.controls.poll(session.keyboard.held_keys(0));
    if result.display_toggled {
        opening.reset_video_palette();
    }
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = trace {
        let state = opening.snapshot();
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
    opening
        .advance_subframe(assets, steering_from_mask(result.mask, controls))
        .map_err(|error| error.to_string())?;
    // Pause ran after the original exit gate. Enter the committed body before
    // setting the flag so complete() permits its remaining two render passes.
    if *quit_after_body_entry {
        opening.request_game_over();
        *quit_after_body_entry = false;
    }
    Ok(result.display_toggled)
}

#[cfg(test)]
fn advance_game_pass(
    opening: &mut Opening,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
) -> Result<bool, String> {
    advance_game_pass_observed(
        opening,
        assets,
        session,
        controls,
        quit_after_body_entry,
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
    opening: Option<Opening>,
    ending: Option<Ending>,
    next_frame: Option<std::time::Instant>,
    pause_tick: Option<std::time::Instant>,
    transitions: TransitionPlayback,
    body_polled: bool,
    quit_after_body_entry: bool,
    pending_step: bool,
    color_family: u8,
    #[cfg(feature = "dev-tools")]
    trace: Option<InputTrace>,
}

impl Default for GamePlayback {
    fn default() -> Self {
        Self {
            session: SessionState::default(),
            opening: None,
            ending: None,
            next_frame: None,
            pause_tick: None,
            transitions: TransitionPlayback::default(),
            body_polled: false,
            quit_after_body_entry: false,
            pending_step: false,
            color_family: 1,
            #[cfg(feature = "dev-tools")]
            trace: None,
        }
    }
}

impl GamePlayback {
    fn ordinary_due(&self, options: &Options, now: std::time::Instant) -> bool {
        options.playback == Playback::Animated
            && self.ending.is_none()
            && !self.transitions.active()
            && self.session.pause.state == PauseState::Running
            && self
                .opening
                .as_ref()
                .is_some_and(|opening| !opening.complete())
            && self.next_frame.is_some_and(|deadline| now >= deadline)
    }

    fn advance(
        &mut self,
        assets: &GraphicsAssets,
        options: &Options,
        now: std::time::Instant,
    ) -> Result<bool, String> {
        let mut repaint = false;
        let unit = options.body_period / 3;
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
        if self.transitions.advance(now, unit) {
            repaint = true;
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
                    .controls
                    .poll(self.session.keyboard.held_keys(0));
                if result.display_toggled {
                    if let Some(opening) = &mut self.opening {
                        opening.reset_video_palette();
                    }
                    // Native pause may remain active without an ordinary pass.
                    self.color_family = 1;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    let state = self.opening.as_ref().map(Opening::snapshot);
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
        // Catch up ordinary passes before one presentation. Bound each batch
        // for event responsiveness; preserve any remaining deadline debt.
        for _ in 0..64 {
            if self.transitions.active() || self.session.pause.state != PauseState::Running {
                break;
            }
            let timed = options.playback == Playback::Animated
                && self.next_frame.is_some_and(|deadline| now >= deadline);
            if !self.pending_step && !timed {
                break;
            }
            let Some(opening) = self.opening.as_mut() else {
                break;
            };
            repaint = true;
            if let Some(ending) = self.ending.as_mut() {
                ending.advance(assets).map_err(|error| error.to_string())?;
                self.pending_step = false;
            } else if opening.complete() && opening.snapshot().player.game_over {
                self.ending = Some(
                    Ending::new_with_best(
                        assets,
                        &opening.snapshot(),
                        opening.frame(),
                        self.session.best_score,
                    )
                    .map_err(|error| error.to_string())?,
                );
                self.pending_step = false;
            } else if !opening.complete() {
                if opening.at_body_boundary() && !self.body_polled {
                    let result = self
                        .session
                        .controls
                        .poll(self.session.keyboard.held_keys(0));
                    if result.display_toggled {
                        opening.reset_video_palette();
                    }
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        let state = opening.snapshot();
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
                        3 - if opening.at_body_boundary() {
                            0
                        } else {
                            opening.snapshot().render_pass
                        }
                    } else {
                        1
                    };
                    for _ in 0..passes {
                        repaint |= advance_game_pass_observed(
                            opening,
                            assets,
                            &mut self.session,
                            options.controls,
                            &mut self.quit_after_body_entry,
                            #[cfg(feature = "dev-tools")]
                            &mut self.trace,
                            #[cfg(feature = "dev-tools")]
                            (
                                now,
                                self.next_frame
                                    .filter(|_| options.playback == Playback::Animated),
                            ),
                        )?;
                        self.transitions
                            .append(opening.take_transitions(), now, unit);
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
                    if opening.at_body_boundary() {
                        self.body_polled = false;
                    }
                    self.pending_step =
                        options.playback == Playback::Step && !opening.at_body_boundary();
                }
            } else {
                self.pending_step = false;
            }
            self.color_family = opening.palette_family();
            let period = self
                .ending
                .as_ref()
                .map_or(unit, |state| options.body_period * state.wait_units() / 3);
            #[cfg(feature = "dev-tools")]
            let previous_deadline = self.next_frame;
            self.next_frame = if opening.complete()
                && !opening.snapshot().player.game_over
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
            if self.ending.is_some() || opening.complete() || options.playback != Playback::Animated
            {
                break;
            }
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
    if !(1..=16).contains(&scale) {
        return Err("window scale must be between 1 and 16".into());
    }
    if options.body_period.is_zero() || options.body_period > std::time::Duration::from_secs(5) {
        return Err("opening period must be greater than zero and at most five seconds".into());
    }
    let mut game = GamePlayback::default();
    game.session.select_intro(); // First intro consumes exactly one random word.
    let mut returned_intro = None;
    let mut step_key_held = false;
    let mut host_pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut window = Window::new(title(Screen::Splash), WIDTH as u32, DISPLAY_HEIGHT, scale)?;
    #[cfg(feature = "dev-tools")]
    if let Some(path) = &options.input_trace {
        game.trace = Some(InputTrace::create(path)?);
        game.trace.as_mut().unwrap().boundary(
            "startup",
            std::time::Instant::now(),
            None,
            &game.session,
            &format!(
                "period_ns={} controls={} playback={:?}",
                options.body_period.as_nanos(),
                options.controls,
                options.playback
            ),
        );
    }
    let creator = window.texture_creator();
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    texture
        .update(&display_rgb(
            frames.frame(shown),
            DisplayMode::Color,
            game.color_family,
        ))
        .map_err(|error| error.to_string())?;
    window.present(&texture)?;
    loop {
        let mut repaint = false;
        for event in window.poll_events() {
            repaint |= matches!(event, Event::Window { .. });
            let was_scene = presentation.screen() == Screen::InitialScene;
            let action = event_action(&mut presentation, event.clone(), host_pause.is_paused());
            if action == Action::Close {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "close", &host_pause, &game.session);
                    trace.flush()?;
                }
                return Ok(());
            }
            if action == Action::ToggleHostPause {
                if was_scene {
                    toggle_host_pause(
                        &mut host_pause,
                        &mut game.session,
                        &mut input,
                        std::time::Instant::now(),
                    );
                    game.pending_step = false;
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
            input.handle(&mut game.session, &event, was_scene);
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.event(&event, "handled", &host_pause, &game.session);
            }
            if action == Action::Space && was_scene {
                if let Some(state) = game.ending.as_mut().filter(|state| state.can_return()) {
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
        if host_pause.is_paused() {
            if repaint {
                window.set_title(
                    "REforge — Mach 3 — Paused (Pause/Break to continue, Ctrl+Q to close)",
                )?;
                window.present(&texture)?;
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
                let start = game.session.begin_game();
                game.opening = Some(
                    if let Some(rules) = &rules {
                        Opening::for_play(frames.graphics(), rules.clone(), start)
                    } else if options.controls {
                        Opening::with_steering(frames.graphics())
                    } else {
                        Opening::new(frames.graphics())
                    }
                    .map_err(|error| error.to_string())?,
                );
                game.color_family = game.opening.as_ref().unwrap().palette_family();
                game.ending = None;
                game.transitions = TransitionPlayback::default();
                returned_intro = None;
                game.body_polled = false;
                game.quit_after_body_entry = false;
                game.pending_step = false;
                game.next_frame =
                    Some(host_pause.now(std::time::Instant::now()) + options.body_period / 3);
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.boundary(
                        "game_started",
                        host_pause.now(std::time::Instant::now()),
                        game.next_frame,
                        &game.session,
                        "",
                    );
                }
            }
        }
        let now = host_pause.now(std::time::Instant::now());
        if shown == Screen::InitialScene {
            repaint |= game.advance(frames.graphics(), &options, now)?;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.flush()?;
            }
            // Repump events before the next bounded catch-up batch. Presenting
            // here would add another vsync wait while ordinary work is overdue.
            if game.ordinary_due(&options, now) {
                continue;
            }
        }
        if repaint {
            let frame = if shown == Screen::InitialScene {
                game.transitions
                    .frames
                    .front()
                    .map(|step| &step.frame)
                    .or_else(|| game.ending.as_ref().map(Ending::frame))
                    .or_else(|| game.opening.as_ref().map(Opening::frame))
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
            } else if game.opening.as_ref().is_some_and(Opening::complete) {
                "REforge — Mach 3 — Preview complete (Esc to close)"
            } else if options.playback == Playback::Step {
                "REforge — Mach 3 — Opening (Enter to step, F1 to pause, Esc to close)"
            } else {
                "REforge — Mach 3 — Opening (F1 to pause, Esc to close)"
            };
            window.set_title(caption)?;
            window.present(&texture)?;
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
        let pause = event(Scancode::Pause, Keycode::Pause, Mod::NOMOD, false);
        assert_eq!(
            event_action(&mut presentation, pause.clone(), false),
            Action::ToggleHostPause
        );
        // Pause/Break is a host action, not an F1 make/break alias.
        control_event(&mut session, &pause, true);
        assert_eq!(
            session.keyboard.held_keys(0),
            crate::opening::HeldKeys::default()
        );
        assert_eq!(session.pause.state, PauseState::Running);
        for (mods, repeat) in [(Mod::NOMOD, true), (Mod::LALTMOD, false)] {
            assert_eq!(
                event_action(
                    &mut presentation,
                    event(Scancode::Pause, Keycode::Pause, mods, repeat),
                    true
                ),
                Action::Continue
            );
        }
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
        assert_eq!(Options::default().scale, reforge_sdl::DEFAULT_SCALE);
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
                crate::opening::HeldKeys::default()
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
            assert_ne!(held, crate::opening::HeldKeys::default());
            input.clear(&mut session);
            input.handle(&mut session, &key_event(key, true, true), true);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::opening::HeldKeys::default()
            );
            input.handle(&mut session, &key_event(key, false, false), true);
            input.handle(&mut session, &key_event(key, true, false), true);
            input.handle(&mut session, &key_event(key, true, true), true);
            assert_eq!(session.keyboard.held_keys(0), held);
            input.handle(&mut session, &key_event(key, false, false), true);
            assert_eq!(
                session.keyboard.held_keys(0),
                crate::opening::HeldKeys::default()
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
            opening: Some(Opening::new(assets).unwrap()),
            next_frame: Some(start + options.body_period / 3),
            ..GamePlayback::default()
        }
    }

    fn pass_count(game: &GamePlayback) -> u32 {
        let state = game.opening.as_ref().unwrap().snapshot();
        if state.body == 0 {
            0
        } else {
            u32::from(state.body - 1) * 3 + u32::from(state.render_pass)
        }
    }

    #[test]
    fn f3_restores_palette_at_body_steering_and_native_pause_polls() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let unit = options.body_period / 3;
        let green_game = || {
            let mut data = vec![0; 65536];
            data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
            for index in 0..21 {
                data[0x0a3e + 50 * index..0x0a41 + 50 * index].copy_from_slice(&[0xce, 0xce, 0xcf]);
            }
            let rules = ScriptRules::from_data_segment(&data).unwrap();
            let start_state = SessionState::default().begin_game();
            let mut opening = Opening::for_play(&assets, rules, start_state).unwrap();
            for _ in 0..21 {
                opening.advance(&assets).unwrap();
            }
            assert_eq!(opening.palette_family(), 0);
            GamePlayback {
                opening: Some(opening),
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
            assert_eq!(game.opening.as_ref().unwrap().palette_family(), 1);
            if !body_already_polled {
                // A held F3 switches at body entry and again before steering.
                assert_eq!(
                    game.session.controls.preferences.display_mode,
                    DisplayMode::Color
                );
                game.session.keyboard.apply_scan(0xbd);
            }
            game.advance(&assets, &options, start + unit * 2).unwrap();
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
            assert_eq!(game.opening.as_ref().unwrap().palette_family(), 1);
        }
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Color
        );
        // A script write later in the same pass wins over the earlier F3 reset.
        let mut game = green_game();
        let opening = game.opening.as_mut().unwrap();
        for _ in 0..19 {
            opening.advance(&assets).unwrap();
        }
        for _ in 0..2 {
            opening
                .advance_subframe(&assets, Steering::default())
                .unwrap();
        }
        game.body_polled = true;
        game.session.controls.preferences.display_mode = DisplayMode::Monochrome;
        game.session.keyboard.apply_scan(0x3d);
        game.advance(&assets, &options, start + unit).unwrap();
        assert_eq!(game.opening.as_ref().unwrap().snapshot().body, 41);
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
    fn ordinary_simulation_matches_at_15_30_60_and_120_presentations_per_second() {
        use std::time::{Duration, Instant};
        let assets = synthetic_assets();
        let options = Options {
            controls: true,
            ..Options::default()
        };
        let start = Instant::now();
        let end = start + Duration::from_secs(1);
        let unit = options.body_period / 3;
        let mut reference = running_playback(&assets, start, &options);
        for scan in [0x4d, 0x39, 0x3c] {
            reference.session.keyboard.apply_scan(scan);
        }
        // Reference processes exactly one due original pass per call.
        let passes = (Duration::from_secs(1).as_nanos() / unit.as_nanos()) as u32;
        for pass in 1..=passes {
            reference
                .advance(&assets, &options, start + unit * pass)
                .unwrap();
        }
        assert_eq!(pass_count(&reference), passes);
        assert!(passes > 30);
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
                format!("{:?}", game.opening.as_ref().unwrap().snapshot()),
                format!("{:?}", reference.opening.as_ref().unwrap().snapshot())
            );
            assert_eq!(
                game.opening.as_ref().unwrap().frame(),
                reference.opening.as_ref().unwrap().frame()
            );
            assert_eq!(
                game.session.controls.preferences,
                reference.session.controls.preferences
            );
            assert_eq!(
                game.session.controls.sound_poll_counter,
                reference.session.controls.sound_poll_counter
            );
            assert_eq!(game.next_frame, reference.next_frame);
            assert!(game.next_frame.unwrap() > end);
        }
    }

    #[test]
    fn bounded_catch_up_keeps_debt_without_changing_step_or_static_playback() {
        use std::time::Instant;
        let assets = synthetic_assets();
        let options = Options::default();
        let start = Instant::now();
        let late = start + options.body_period / 3 * 100;
        let mut game = running_playback(&assets, start, &options);
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(pass_count(&game), 64);
        assert!(game.ordinary_due(&options, late));
        assert!(game.next_frame.unwrap() <= late);
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(pass_count(&game), 100);
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
        let unit = options.body_period / 3;
        let late = start + unit * 100;
        let mut paused = running_playback(&assets, start, &options);
        paused.session.keyboard.apply_scan(0x3b);
        paused.advance(&assets, &options, late).unwrap();
        assert_eq!(paused.session.pause.state, PauseState::AwaitInitialRelease);
        assert_eq!(pass_count(&paused), 0);
        let mut game = running_playback(&assets, start, &options);
        let frame = game.opening.as_ref().unwrap().frame().clone();
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
        let unit = options.body_period / 3;
        let mut game = running_playback(&assets, start, &options);
        let opening = game.opening.as_mut().unwrap();
        opening.request_game_over();
        for _ in 0..3 {
            opening
                .advance_subframe(&assets, Steering::default())
                .unwrap();
        }
        let mut expected =
            Ending::new_with_best(&assets, &opening.snapshot(), opening.frame(), 0).unwrap();
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
            Some(late + options.body_period * expected.wait_units() / 3)
        );
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
        let mut opening = Opening::with_steering(&assets).unwrap();
        let mut session = SessionState::default();
        session.keyboard.apply_scan(0x3c); // Held F2 at all four input sites.
        session.controls.poll(session.keyboard.held_keys(0)); // Body-entry poll.
        let mut quit = true; // Pause's second F1 press passed the exit gate.
        advance_game_pass(&mut opening, &assets, &mut session, true, &mut quit).unwrap();
        assert!(opening.snapshot().player.game_over);
        assert!(!opening.complete());
        assert_eq!(opening.snapshot().render_pass, 1);
        for _ in 0..2 {
            advance_game_pass(&mut opening, &assets, &mut session, true, &mut quit).unwrap();
        }
        assert_eq!(opening.snapshot().body, 1);
        assert_eq!(opening.snapshot().render_pass, 3);
        assert!(opening.complete());
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
    fn input_trace_exposes_stalled_poll_history_without_changing_simulation() {
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
        let unit = options.body_period / 3;
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
                format!("{:?}", left.opening.as_ref().unwrap().snapshot()),
                format!("{:?}", right.opening.as_ref().unwrap().snapshot())
            );
            assert_eq!(
                left.opening.as_ref().unwrap().frame(),
                right.opening.as_ref().unwrap().frame()
            );
            assert_eq!(left.next_frame, right.next_frame);
            assert_eq!(left.pause_tick, right.pause_tick);
            assert_eq!(left.body_polled, right.body_polled);
            assert_eq!(left.quit_after_body_entry, right.quit_after_body_entry);
        };
        // All events are handled at P+10ms, after a deliberately overdue poll P.
        // Expected-at-P is a temporal reference, not a tap-retention policy.
        for (label, press_delta, release_delta, expected_at_poll, observed) in [
            ("before", -5, -1, 0, 0),
            ("spanning", -5, 5, 8, 0),
            ("after", 5, 20, 0, 8),
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
                input.handle(&mut game.session, &event, true);
                plain_input.handle(&mut plain.session, &event, true);
                game.trace.as_mut().unwrap().event(
                    &event,
                    "handled",
                    &HostPause::default(),
                    &game.session,
                );
            }
            let late = start + unit + Duration::from_millis(10);
            game.advance(&assets, &options, late).unwrap();
            plain.advance(&assets, &options, late).unwrap();
            assert_same(&game, &plain);
            let rows = read_rows(&mut game, &path);
            assert!(rows.iter().all(|row| row.len() == 13));
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
                assert_eq!(row[9], observed.to_string());
                assert_eq!(row[11], observed.to_string());
            }
            assert_eq!(expected_at_poll == observed, label == "before");
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
        let frame = game.opening.as_ref().unwrap().frame().clone();
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
