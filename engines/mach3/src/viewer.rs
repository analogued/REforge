// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and live gameplay.

use reforge_sdl::{FrameFormat, FrameTexture, HostPause, HostShortcut, Window, host_shortcut};
use std::collections::{BTreeSet, VecDeque};

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

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub scale: u32,
    pub playback: Playback,
    pub controls: bool,
    pub body_period: std::time::Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: reforge_sdl::DEFAULT_SCALE,
            playback: Playback::Animated,
            controls: false,
            body_period: BODY_PERIOD,
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

fn advance_game_pass(
    opening: &mut Opening,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
) -> Result<bool, String> {
    let result = session.controls.poll(session.keyboard.held_keys(0));
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
    let mut session = SessionState::default();
    session.select_intro(); // First intro consumes exactly one random word.
    let mut opening: Option<Opening> = None;
    let mut color_family = 1;
    let mut ending: Option<Ending> = None;
    let mut returned_intro = None;
    let mut next_frame = None;
    let mut pause_tick = None;
    let mut transitions = TransitionPlayback::default();
    let mut body_polled = false;
    let mut quit_after_body_entry = false;
    let mut pending_step = false;
    let mut step_key_held = false;
    let mut host_pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut window = Window::new(title(Screen::Splash), WIDTH as u32, DISPLAY_HEIGHT, scale)?;
    let creator = window.texture_creator();
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    texture
        .update(&display_rgb(
            frames.frame(shown),
            DisplayMode::Color,
            color_family,
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
                return Ok(());
            }
            if action == Action::ToggleHostPause {
                if was_scene {
                    toggle_host_pause(
                        &mut host_pause,
                        &mut session,
                        &mut input,
                        std::time::Instant::now(),
                    );
                    pending_step = false;
                    step_key_held = false;
                    repaint = true;
                }
                continue;
            }
            if host_pause.is_paused() {
                continue;
            }
            match &event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    if !step_key_held {
                        pending_step |= was_scene && options.playback == Playback::Step;
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
            input.handle(&mut session, &event, was_scene);
            if action == Action::Space && was_scene {
                if let Some(state) = ending.as_mut().filter(|state| state.can_return()) {
                    returned_intro = state.returned_intro_choice();
                    session.remember_ending(&state.snapshot(), state.special_snapshots());
                    presentation.return_to_intro();
                    next_frame = None;
                }
            }
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
                color_family = 1; // Repeat entry selects palette family1 before intro.
            }
            if shown == Screen::InitialScene && options.playback != Playback::Static {
                let start = session.begin_game();
                opening = Some(
                    if let Some(rules) = &rules {
                        Opening::for_play(frames.graphics(), rules.clone(), start)
                    } else if options.controls {
                        Opening::with_steering(frames.graphics())
                    } else {
                        Opening::new(frames.graphics())
                    }
                    .map_err(|error| error.to_string())?,
                );
                color_family = opening.as_ref().unwrap().palette_family();
                ending = None;
                transitions = TransitionPlayback::default();
                returned_intro = None;
                body_polled = false;
                quit_after_body_entry = false;
                pending_step = false;
                next_frame =
                    Some(host_pause.now(std::time::Instant::now()) + options.body_period / 3);
            }
        }
        let now = host_pause.now(std::time::Instant::now());
        let transition_was_active = transitions.active();
        if transitions.advance(now, options.body_period / 3) {
            repaint = true;
            if !transitions.active() {
                next_frame = Some(now + options.body_period / 3);
            }
        }
        // Pause's busy input polls are separate from its two timed waits.
        if !transition_was_active && session.pause.state != PauseState::Running {
            if pause_waiting(session.pause.state) {
                if pause_tick.is_some_and(|deadline| now >= deadline) {
                    session.pause.tick();
                    pause_tick = Some(now + options.body_period / 3);
                }
            } else {
                let result = session.controls.poll(session.keyboard.held_keys(0));
                repaint |= result.display_toggled;
                match session.pause.poll(result.mask) {
                    PauseEvent::EndGame => quit_after_body_entry = true,
                    PauseEvent::Resumed => next_frame = Some(now),
                    _ => {}
                }
                if pause_waiting(session.pause.state) {
                    pause_tick = Some(now + options.body_period / 3);
                }
            }
        }
        if shown == Screen::InitialScene
            && !transition_was_active
            && session.pause.state == PauseState::Running
        {
            let timed = options.playback == Playback::Animated
                && next_frame.is_some_and(|deadline| now >= deadline);
            if pending_step || timed {
                if let Some(opening) = &mut opening {
                    repaint = true;
                    if let Some(ending) = &mut ending {
                        ending
                            .advance(frames.graphics())
                            .map_err(|error| error.to_string())?;
                        pending_step = false;
                    } else if opening.complete() && opening.snapshot().player.game_over {
                        ending = Some(
                            Ending::new_with_best(
                                frames.graphics(),
                                &opening.snapshot(),
                                opening.frame(),
                                session.best_score,
                            )
                            .map_err(|error| error.to_string())?,
                        );
                        pending_step = false;
                    } else if !opening.complete() {
                        if opening.at_body_boundary() && !body_polled {
                            let result = session.controls.poll(session.keyboard.held_keys(0));
                            repaint |= result.display_toggled;
                            body_polled = true;
                            session.pause.poll(result.mask);
                        }
                        if session.pause.state == PauseState::Running {
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
                                repaint |= advance_game_pass(
                                    opening,
                                    frames.graphics(),
                                    &mut session,
                                    options.controls,
                                    &mut quit_after_body_entry,
                                )?;
                                transitions.append(
                                    opening.take_transitions(),
                                    now,
                                    options.body_period / 3,
                                );
                                if transitions.active() {
                                    break;
                                }
                            }
                            if opening.at_body_boundary() {
                                body_polled = false;
                            }
                            pending_step =
                                options.playback == Playback::Step && !opening.at_body_boundary();
                        }
                    } else {
                        pending_step = false;
                    }
                    color_family = opening.palette_family();
                    let period = ending.as_ref().map_or(options.body_period / 3, |state| {
                        options.body_period * state.wait_units() / 3
                    });
                    next_frame = if opening.complete()
                        && !opening.snapshot().player.game_over
                        && ending.is_none()
                    {
                        None
                    } else if timed {
                        next_frame.map(|deadline| deadline + period)
                    } else {
                        Some(now + period)
                    };
                }
            }
        }
        if repaint {
            let frame = if shown == Screen::InitialScene {
                transitions
                    .frames
                    .front()
                    .map(|step| &step.frame)
                    .or_else(|| ending.as_ref().map(Ending::frame))
                    .or_else(|| opening.as_ref().map(Opening::frame))
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
                    session.controls.preferences.display_mode,
                    color_family,
                ))
                .map_err(|error| error.to_string())?;
            let caption = if session.pause.state != PauseState::Running {
                "REforge — Mach 3 — Paused (release keys, then F1 to end game or another key to resume)"
            } else if shown != Screen::InitialScene {
                title(shown)
            } else if ending.as_ref().is_some_and(Ending::can_return) {
                "REforge — Mach 3 — Game Over / High Score (Space to return, Esc to close)"
            } else if ending.is_some() {
                "REforge — Mach 3 — Game Over / High Score (Esc to close)"
            } else if opening.as_ref().is_some_and(Opening::complete) {
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
        let deadline = transitions.deadline.or_else(|| {
            next_frame.filter(|_| {
                options.playback == Playback::Animated && session.pause.state == PauseState::Running
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
}
