// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and live gameplay.

use reforge_sdl::{
    FrameFormat, FrameTexture, HostPause, HostShortcut, Window, game_title,
    host_shortcut as shared_host_shortcut,
    settings::{SettingsMenu, VideoCapabilities},
};
use std::collections::{BTreeSet, VecDeque};

mod audio_output;
mod input_history;
mod preview;
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
        DisplayMode, Ending, Gameplay, NOMINAL_PREVIEW_BODY_PERIOD, PauseEvent, PauseState,
        ScriptRules, SessionState, Steering, TransitionFrame,
    },
    startup::{Presentation, Screen, StartupFrames},
};

// CGA mode 4 stores 320x200 pixels but fills a 4:3 display (pixel aspect 5:6).
const DISPLAY_HEIGHT: u32 = 240;
const GAME_NAME: &str = "Mach 3";

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
    /// Native sample execution; None uses the saved Mach 3 startup preference.
    pub digitized_samples: Option<bool>,
    /// None fits the usable display; Some requests a fixed drawable multiplier.
    pub scale: Option<u32>,
    pub presentation: reforge_sdl::Presentation,
    pub playback: Playback,
    pub controls: bool,
    pub audio_renderer: crate::sound::render::AudioRenderer,
    /// Explicit silent-preview override; None leaves normal playback on the PIT clock.
    pub preview_period: Option<std::time::Duration>,
    #[cfg(feature = "dev-tools")]
    pub input_trace: Option<std::path::PathBuf>,
    #[cfg(feature = "dev-tools")]
    pub timing_trace: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            f3_mode: crate::gameplay::F3Mode::Press,
            digitized_samples: None,
            scale: None,
            presentation: reforge_sdl::Presentation::Pixels,
            playback: Playback::Animated,
            controls: false,
            audio_renderer: crate::sound::render::AudioRenderer::Speaker,
            preview_period: None,
            #[cfg(feature = "dev-tools")]
            input_trace: None,
            #[cfg(feature = "dev-tools")]
            timing_trace: None,
        }
    }
}

impl Options {
    /// Whether command-line options request silent nominal stepping.
    pub(crate) fn is_preview(&self) -> bool {
        self.playback != Playback::Animated || self.preview_period.is_some()
    }

    fn nominal_body_period(&self) -> std::time::Duration {
        self.preview_period.unwrap_or(NOMINAL_PREVIEW_BODY_PERIOD)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Space,
    Close,
    ToggleHostPause,
}

fn screen_status(screen: Screen) -> &'static str {
    match screen {
        Screen::Splash => "Splash (Space to advance)",
        Screen::Intro => "Intro (Space to advance)",
        Screen::InitialScene => "Initial scene (Esc to close)",
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
    window.prepare_settings(menu);
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

/// Run a silent nominal preview, including retained-session restarts.
pub fn run_preview(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
) -> Result<(), String> {
    preview::run(frames, options, rules)
}

/// Run animated gameplay on the reconstructed PIT timeline with live sound.
pub fn run_live(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
    samples: crate::sound::SoundAssets,
    intro: crate::sound::IntroProgram,
    procedural: crate::sound::ProceduralProgram,
) -> Result<(), String> {
    if options.is_preview() {
        return Err("step/static and custom-period options require run_preview".into());
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

#[cfg(test)]
mod options_tests {
    use super::*;

    #[test]
    fn preview_selection_depends_on_intent_rather_than_period_value() {
        let mut options = Options::default();
        assert!(!options.is_preview());
        options.preview_period = Some(NOMINAL_PREVIEW_BODY_PERIOD);
        assert!(options.is_preview());
        assert_eq!(options.nominal_body_period(), NOMINAL_PREVIEW_BODY_PERIOD);
        for playback in [Playback::Step, Playback::Static] {
            options.playback = playback;
            options.preview_period = None;
            assert!(options.is_preview());
        }
    }
}
