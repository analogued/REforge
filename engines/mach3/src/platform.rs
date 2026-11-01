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
#[cfg(feature = "dev-tools")]
mod preview;
mod realtime;
mod wakeup;
use input_history::EventTime;
#[cfg(feature = "dev-tools")]
use input_history::InputHistory;

#[cfg(feature = "dev-tools")]
use crate::input_trace::{InputTrace, PollSite};
#[cfg(feature = "dev-tools")]
use crate::timing_trace::TimingTrace;

use reforge_sdl::sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
};

use crate::{
    frame::{Frame, HEIGHT, WIDTH},
    gameplay::{DisplayMode, Ending, Gameplay, PauseState, ScriptRules, SessionState},
    startup::{Screen, StartupFrames},
};

// Modes 4 and 6 share a 4:3 display, but have different native pixel grids.
// display_rgb's doubled colour texture is an implementation detail, not mode 6.
fn display_geometry(mode: DisplayMode) -> reforge_sdl::DisplayGeometry {
    let width = match mode {
        DisplayMode::Color => WIDTH as u32,
        DisplayMode::Monochrome => WIDTH as u32 * 2,
    };
    reforge_sdl::DisplayGeometry::new((width, HEIGHT as u32), (320, 240))
}
const GAME_NAME: &str = "Mach 3";

#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playback {
    #[default]
    Animated,
    Step,
    Static,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub video: DisplayMode,
    pub f3_mode: crate::gameplay::F3Mode,
    /// Native sample execution; None uses the saved Mach 3 startup preference.
    pub digitized_samples: Option<bool>,
    /// None fits the usable display; Some requests a fixed drawable multiplier.
    pub scale: Option<u32>,
    pub presentation: reforge_sdl::Presentation,
    #[cfg(feature = "dev-tools")]
    pub playback: Playback,
    pub controls: bool,
    pub audio_renderer: crate::sound::render::AudioRenderer,
    #[cfg(feature = "dev-tools")]
    pub input_trace: Option<std::path::PathBuf>,
    #[cfg(feature = "dev-tools")]
    pub timing_trace: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            video: DisplayMode::Color,
            f3_mode: crate::gameplay::F3Mode::Press,
            digitized_samples: None,
            scale: None,
            presentation: reforge_sdl::Presentation::Pixels,
            #[cfg(feature = "dev-tools")]
            playback: Playback::Animated,
            controls: true,
            audio_renderer: crate::sound::render::AudioRenderer::Speaker,
            #[cfg(feature = "dev-tools")]
            input_trace: None,
            #[cfg(feature = "dev-tools")]
            timing_trace: None,
        }
    }
}

impl Options {
    /// Set the initial native display preference. Subsequent F3 handling and
    /// session retention remain the original controls' responsibility.
    fn apply_startup_display(&self, preferences: &mut crate::gameplay::Preferences) {
        preferences.display_mode = self.video;
    }
}

#[cfg(feature = "dev-tools")]
impl Options {
    /// Whether command-line options request silent nominal stepping.
    pub(crate) fn is_preview(&self) -> bool {
        self.playback != Playback::Animated
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

/// Run a silent nominal preview, including retained-session restarts.
#[cfg(feature = "dev-tools")]
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
    #[cfg(feature = "dev-tools")]
    if options.is_preview() {
        return Err("step/static options require run_preview".into());
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

/// Host repeat eligibility is separate from the reconstructed DOS scan-byte filter.
#[derive(Default)]
struct ControlInput {
    fresh: BTreeSet<u8>,
}

impl ControlInput {
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
}

/// A fixed 640-pixel texture preserves both raw mode-6 bits and mode-4 pixels.
pub(crate) fn display_rgb(frame: &Frame, mode: DisplayMode, family: u8) -> Vec<u8> {
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
    fn controls_are_enabled_by_default() {
        assert!(Options::default().controls);
    }

    #[test]
    fn startup_video_preserves_f3_switching() {
        for initial in [DisplayMode::Color, DisplayMode::Monochrome] {
            let options = Options {
                video: initial,
                ..Options::default()
            };
            let mut controls = crate::gameplay::Controls::default();
            options.apply_startup_display(&mut controls.preferences);
            assert_eq!(controls.preferences.display_mode, initial);
            let mut keyboard = crate::gameplay::KeyboardState::default();
            keyboard.apply_scan(0x3d);
            assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
            assert_ne!(controls.preferences.display_mode, initial);
            keyboard.apply_scan(0xbd);
            keyboard.apply_scan(0x3d);
            assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
            assert_eq!(controls.preferences.display_mode, initial);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn only_explicit_step_or_static_modes_select_preview() {
        let mut options = Options::default();
        assert!(!options.is_preview());
        for playback in [Playback::Step, Playback::Static] {
            options.playback = playback;
            assert!(options.is_preview());
        }
    }
}
