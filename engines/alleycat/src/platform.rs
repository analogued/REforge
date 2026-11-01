// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Alley Cat host input and pause overlay, using REforge's shared SDL backend.

use crate::alley::Machine;
use crate::control_history::{HostChange, Key, Keys};
use crate::menu::MenuKey;
use reforge_sdl::gamepad::{Actions, Gamepads};
#[cfg(test)]
use reforge_sdl::sdl3::keyboard::Keycode;
use reforge_sdl::sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Mod, Scancode},
    pixels::Color,
    render::FRect,
};
use reforge_sdl::settings::{SettingsMenu, VideoCapabilities};
pub use reforge_sdl::{FrameCreator, Window as Display};
use reforge_sdl::{FrameFormat, FrameTexture, HostShortcut, host_shortcut};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// CGA/PCjr 320x200 pixels were intended for a 4:3 display.
pub const DISPLAY_GEOMETRY: reforge_sdl::DisplayGeometry =
    reforge_sdl::DisplayGeometry::new((320, 200), (320, 240));

const CONTROL: u16 = Mod::LCTRLMOD.bits() | Mod::RCTRLMOD.bits();
const SHORTCUT: u16 =
    CONTROL | Mod::LALTMOD.bits() | Mod::RALTMOD.bits() | Mod::LGUIMOD.bits() | Mod::RGUIMOD.bits();

/// Host input sampled after draining SDL's event queue.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub alt: bool,
    pub quit: bool,
    pub pause_pressed: bool,
    pub continue_pressed: bool,
    pub menu_key: Option<MenuKey>,
    pub menu_pressed: bool,
    pub new_game_pressed: bool,
    pub sound_pressed: bool,
    pub settings_open: bool,
    pub settings_changed: bool,
    pub controller_menu_delta: i8,
    pub controller_confirm: bool,
}

/// Host pause bookkeeping shared by the normal and diagnostic runners.
pub struct PauseClock {
    previous: Instant,
    paused_at: Option<Instant>,
    manual_paused: bool,
    settings_open: bool,
}

pub struct PauseSample {
    pub paused: bool,
    pub changed: bool,
    pub elapsed: Duration,
    pub resumed_after: Option<Duration>,
}

impl PauseClock {
    pub fn new(now: Instant) -> Self {
        Self {
            previous: now,
            paused_at: None,
            manual_paused: false,
            settings_open: false,
        }
    }

    pub fn sample(&mut self, now: Instant, input: Input) -> PauseSample {
        let was_paused = self.manual_paused || self.settings_open;
        if input.pause_pressed && !input.settings_open && !input.settings_changed {
            self.manual_paused = !self.manual_paused;
        }
        self.settings_open = input.settings_open;
        let paused = self.manual_paused || self.settings_open;
        let changed = was_paused != paused || input.settings_changed;
        let mut resumed_after = None;
        if !was_paused && paused {
            self.paused_at = Some(now);
        } else if was_paused && !paused {
            resumed_after = self.paused_at.take().map(|start| now.duration_since(start));
        }
        let elapsed = if paused || changed {
            Duration::ZERO
        } else {
            now.duration_since(self.previous)
        };
        self.previous = now;
        PauseSample {
            paused,
            changed,
            elapsed,
            resumed_after,
        }
    }
}

/// Engine-owned pause text; it never modifies the game's framebuffer.
pub struct PauseOverlay {
    lines: [String; 2],
    color: [u8; 4],
}

impl PauseOverlay {
    pub fn new(title: &str, machine: Machine) -> Result<Self, String> {
        if title.contains('\0') {
            return Err("pause text contains a NUL byte".into());
        }
        Ok(Self {
            lines: [title.into(), " Esc / P / Pause to continue...  ".into()],
            color: machine.colors()[2],
        })
    }
}

/// Engine adapter. The separate texture creator gives Rust explicit ownership
/// lifetimes without self-referential objects or manually owned SDL pointers.
pub struct Window<'a> {
    display: Display,
    pub(crate) audio: crate::sound::playback::Player,
    frame: FrameTexture<'a>,
    quit: bool,
    settings: SettingsMenu<'a>,
    settings_was_open: bool,
    suppressed: HashSet<Scancode>,
    control_keys: Keys,
    control_changes: Vec<HostChange>,
    gamepads: Gamepads,
}

impl<'a> Window<'a> {
    pub fn new(
        display: Display,
        creator: &'a FrameCreator,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            gamepads: Gamepads::new(display.settings().stick_dead_zone),
            audio: crate::sound::playback::Player::new(creator.audio_playback())?,
            display,
            frame: FrameTexture::new(creator, width, height, FrameFormat::Rgba)?,
            quit: false,
            settings: SettingsMenu::new(creator, VideoCapabilities::default())
                .with_alleycat_options()
                .with_audio_renderers(&[reforge_sdl::settings::SoundRenderer::Speaker])
                .with_audio_categories(&[
                    reforge_audio::category::SoundCategory::Music,
                    reforge_audio::category::SoundCategory::Effects,
                ]),
            settings_was_open: false,
            suppressed: HashSet::new(),
            control_keys: Keys::default(),
            control_changes: Vec::new(),
        })
    }

    pub fn poll_input(&mut self) -> Input {
        // Startup and diagnostic callers need only the latest snapshot; do not
        // accumulate history across polls which have no gameplay consumer.
        self.control_changes.clear();
        self.gamepads
            .set_dead_zone(self.display.settings().stick_dead_zone);
        let mut presses = Input::default();
        let mut settings_changed = self.settings_was_open != self.settings.is_open();
        for event in self.display.poll_events() {
            let pad = self.gamepads.handle(&event);
            if host_shortcut(&event) == Some(HostShortcut::Quit) {
                self.quit = true;
            }
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) {
                self.quit = true;
            }
            let was_open = self.settings.is_open();
            let pad_consumed = pad.is_some_and(|pad| self.settings.handle_gamepad(pad));
            let consumed = self.settings.handle_event(&event) || pad_consumed;
            settings_changed |= was_open != self.settings.is_open();
            route_guest_key(
                &event,
                consumed || settings_changed,
                &mut self.suppressed,
                &mut presses,
            );
            let suppressed = match &event {
                Event::KeyDown {
                    scancode: Some(scan),
                    ..
                } => self.suppressed.contains(scan),
                _ => false,
            };
            record_controls(
                &event,
                consumed || settings_changed || suppressed,
                &mut self.control_keys,
                &mut self.control_changes,
            );
            if let Some(pad) = pad {
                if !consumed && !settings_changed {
                    presses.pause_pressed |= pad.pause;
                    presses.continue_pressed |= pad.pressed.action;
                    if !presses.controller_confirm && pad.pressed.up {
                        presses.controller_menu_delta =
                            presses.controller_menu_delta.saturating_sub(1);
                    }
                    if !presses.controller_confirm && pad.pressed.down {
                        presses.controller_menu_delta =
                            presses.controller_menu_delta.saturating_add(1);
                    }
                    presses.controller_confirm |= pad.pressed.action;
                    set_pad_keys(&mut self.control_keys, pad.actions);
                } else {
                    if settings_changed {
                        self.gamepads.reset();
                    }
                    set_pad_keys(&mut self.control_keys, Actions::default());
                }
                self.control_changes.push(HostChange {
                    at_ns: event.get_timestamp(),
                    keys: self.control_keys,
                });
            }
            if presses.pause_pressed {
                self.gamepads.reset();
                set_pad_keys(&mut self.control_keys, Actions::default());
            }
        }
        self.quit |= presses.quit;
        let held = self.display.keyboard_state();
        self.suppressed
            .retain(|scan| held.is_scancode_pressed(*scan));
        let settings_open = self.settings.is_open();
        if settings_changed {
            self.suppressed.extend(held.pressed_scancodes());
            presses = Input::default();
        }
        self.settings_was_open = settings_open;
        let held_key = |scan| {
            !settings_open
                && !settings_changed
                && !self.suppressed.contains(&scan)
                && held.is_scancode_pressed(scan)
        };
        let mut keys = Keys::default();
        for (scan, key) in control_scancodes() {
            keys.set(key, held_key(scan));
        }
        let pad = if settings_open || settings_changed {
            Actions::default()
        } else {
            self.gamepads.actions()
        };
        set_pad_keys(&mut keys, pad);
        if keys != self.control_keys {
            self.control_keys = keys;
            self.control_changes.push(HostChange {
                at_ns: control_timestamp(),
                keys,
            });
        }
        Input {
            up: held_key(Scancode::Up) || pad.up,
            down: held_key(Scancode::Down) || pad.down,
            left: held_key(Scancode::Left) || pad.left,
            right: held_key(Scancode::Right) || pad.right,
            alt: held_key(Scancode::LAlt) || held_key(Scancode::RAlt) || pad.action,
            quit: self.quit,
            pause_pressed: presses.pause_pressed,
            continue_pressed: presses.continue_pressed,
            menu_key: presses.menu_key,
            menu_pressed: presses.menu_pressed,
            new_game_pressed: presses.new_game_pressed,
            sound_pressed: presses.sound_pressed,
            settings_open,
            settings_changed,
            controller_menu_delta: presses.controller_menu_delta,
            controller_confirm: presses.controller_confirm,
        }
    }

    pub(crate) fn control_keys(&self) -> Keys {
        self.control_keys
    }

    pub(crate) fn reset_controller(&mut self) {
        self.gamepads.reset();
        set_pad_keys(&mut self.control_keys, Actions::default());
    }

    pub(crate) fn responsive_controls(&self) -> bool {
        self.display.settings().alleycat.control_mode
            == reforge_sdl::settings::ControlMode::Responsive
    }

    pub(crate) fn take_control_changes(&mut self) -> (u64, Vec<HostChange>) {
        (
            control_timestamp(),
            std::mem::take(&mut self.control_changes),
        )
    }

    pub fn present(&mut self, rgba: &[u8]) -> Result<(), String> {
        self.present_with_pause(rgba, None)
    }

    pub fn presentation_due(&self) -> bool {
        self.display.presentation_due()
    }

    pub fn present_with_pause(
        &mut self,
        rgba: &[u8],
        pause: Option<&PauseOverlay>,
    ) -> Result<(), String> {
        self.display.prepare_settings(&mut self.settings);
        self.frame.update(rgba)?;
        let settings = &mut self.settings;
        self.display.present_with(&self.frame, |canvas| {
            if let Some(pause) = pause {
                // Keep original 320x200 text placement under either aspect policy.
                let (width, height, _) = canvas.logical_size();
                canvas
                    .set_scale(width as f32 / 320.0, height as f32 / 200.0)
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    canvas
                        .fill_rect(FRect::new(40.0, 88.0, 256.0, 16.0))
                        .map_err(|error| error.to_string())?;
                    let [r, g, b, a] = pause.color;
                    canvas.set_draw_color(Color::RGBA(r, g, b, a));
                    canvas
                        .draw_debug_text(&pause.lines[0], (40.0, 88.0))
                        .map_err(|error| error.to_string())?;
                    canvas
                        .draw_debug_text(&pause.lines[1], (40.0, 96.0))
                        .map_err(|error| error.to_string())
                })();
                canvas
                    .set_scale(1.0, 1.0)
                    .map_err(|error| error.to_string())?;
                canvas.set_draw_color(Color::RGB(0, 0, 0));
                result?;
            }
            settings.paint(canvas)
        })
    }
}

// A menu key held while returning to the game remains blocked until its release.
// This is separate from the native keyboard snapshot and polling priorities.
fn route_guest_key(
    event: &Event,
    consumed: bool,
    suppressed: &mut HashSet<Scancode>,
    presses: &mut Input,
) {
    match event {
        Event::KeyUp {
            scancode: Some(scan),
            ..
        } => {
            suppressed.remove(scan);
        }
        Event::KeyDown {
            scancode: Some(scan),
            ..
        } => {
            if consumed {
                suppressed.insert(*scan);
            } else if !suppressed.contains(scan) {
                presses.record_event(event);
            }
        }
        _ => {}
    }
}

fn control_timestamp() -> u64 {
    // SDL keyboard-event timestamps use this clock. The wrapper currently
    // exposes only millisecond ticks, so isolate the nanosecond FFI call here.
    // SAFETY: SDL_GetTicksNS takes no pointers and is safe on any thread.
    unsafe { reforge_sdl::sdl3::sys::timer::SDL_GetTicksNS() }
}

fn set_pad_keys(keys: &mut Keys, pad: Actions) {
    for (key, held) in [
        (Key::PadUp, pad.up),
        (Key::PadDown, pad.down),
        (Key::PadLeft, pad.left),
        (Key::PadRight, pad.right),
        (Key::PadAction, pad.action),
    ] {
        keys.set(key, held);
    }
}

fn control_scancodes() -> [(Scancode, Key); 6] {
    [
        (Scancode::Up, Key::Up),
        (Scancode::Down, Key::Down),
        (Scancode::Left, Key::Left),
        (Scancode::Right, Key::Right),
        (Scancode::LAlt, Key::LeftAlt),
        (Scancode::RAlt, Key::RightAlt),
    ]
}

fn record_controls(event: &Event, blocked: bool, keys: &mut Keys, changes: &mut Vec<HostChange>) {
    let before = *keys;
    let timestamp = match event {
        Event::KeyDown {
            timestamp,
            scancode: Some(scan),
            repeat: false,
            ..
        } if !blocked => {
            if let Some((_, key)) = control_scancodes().into_iter().find(|(s, _)| s == scan) {
                keys.set(key, true);
            }
            *timestamp
        }
        Event::KeyUp {
            timestamp,
            scancode: Some(scan),
            ..
        } => {
            if let Some((_, key)) = control_scancodes().into_iter().find(|(s, _)| s == scan) {
                keys.set(key, false);
            }
            *timestamp
        }
        Event::Window {
            timestamp,
            win_event: WindowEvent::FocusLost,
            ..
        } => {
            *keys = Keys::default();
            *timestamp
        }
        _ => return,
    };
    if before != *keys {
        changes.push(HostChange {
            at_ns: timestamp,
            keys: *keys,
        });
    }
}

fn modifier_key(scancode: Scancode) -> bool {
    matches!(
        scancode,
        Scancode::LCtrl
            | Scancode::LShift
            | Scancode::LAlt
            | Scancode::LGui
            | Scancode::RCtrl
            | Scancode::RShift
            | Scancode::RAlt
            | Scancode::RGui
    )
}

impl Input {
    fn record_event(&mut self, event: &Event) {
        match host_shortcut(event) {
            Some(HostShortcut::Quit) => self.quit = true,
            Some(HostShortcut::TogglePause) => self.pause_pressed = true,
            None => {}
        }
        let Event::KeyDown {
            scancode: Some(scancode),
            keymod,
            repeat,
            ..
        } = event
        else {
            return;
        };
        let (scancode, modifiers, repeat) = (*scancode, keymod.bits(), *repeat);
        if !repeat && modifiers & CONTROL != 0 && modifiers & 0x0f00 == 0 {
            self.new_game_pressed |= scancode == Scancode::R;
            self.menu_pressed |= scancode == Scancode::M;
            self.sound_pressed |= scancode == Scancode::S;
        }
        if !repeat && modifiers & SHORTCUT == 0 {
            // Modifier keys alone and desktop shortcuts do not dismiss startup.
            self.continue_pressed |= !modifier_key(scancode);
            if !modifier_key(scancode) {
                // Keep the first fresh press so a queued second key cannot
                // replace a skill choice or skip the newly revealed controls.
                self.menu_key.get_or_insert(match scancode {
                    Scancode::K => MenuKey::Kitten,
                    Scancode::H => MenuKey::HouseCat,
                    Scancode::T => MenuKey::Tomcat,
                    Scancode::A => MenuKey::AlleyCat,
                    Scancode::Up => MenuKey::Up,
                    Scancode::Down => MenuKey::Down,
                    Scancode::Return | Scancode::KpEnter | Scancode::Space => MenuKey::Confirm,
                    _ => MenuKey::Other,
                });
            }
            self.pause_pressed |= matches!(scancode, Scancode::Escape | Scancode::P);
        }
    }
    #[cfg(test)]
    fn record_press(&mut self, scan: Scancode, modifiers: u16, repeat: bool) {
        self.record_event(&Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: Some(scan),
            keycode: (scan == Scancode::Q).then_some(Keycode::Q),
            keymod: Mod::from_bits_truncate(modifiers),
            repeat,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_events_keep_timestamps_and_release_without_repeats_or_menu_presses() {
        let event = |timestamp, scan, repeat| Event::KeyDown {
            timestamp,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: Some(scan),
            keycode: None,
            keymod: Mod::NOMOD,
            repeat,
        };
        let mut keys = Keys::default();
        let mut changes = Vec::new();
        record_controls(
            &event(60, Scancode::Right, false),
            false,
            &mut keys,
            &mut changes,
        );
        record_controls(
            &event(70, Scancode::Right, true),
            false,
            &mut keys,
            &mut changes,
        );
        record_controls(
            &event(75, Scancode::Left, false),
            true,
            &mut keys,
            &mut changes,
        );
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].at_ns, 60);
        assert_eq!(changes[0].keys.controls().0.x, 1);
        record_controls(
            &Event::KeyUp {
                timestamp: 80,
                window_id: 1,
                which: 0,
                raw: 0,
                scancode: Some(Scancode::Right),
                keycode: None,
                keymod: Mod::NOMOD,
                repeat: false,
            },
            true,
            &mut keys,
            &mut changes,
        );
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1].at_ns, 80);
        assert_eq!(changes[1].keys, Keys::default());
        record_controls(
            &event(90, Scancode::LAlt, false),
            false,
            &mut keys,
            &mut changes,
        );
        record_controls(
            &Event::Window {
                timestamp: 100,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
            false,
            &mut keys,
            &mut changes,
        );
        assert_eq!(changes.last().unwrap().at_ns, 100);
        assert_eq!(keys, Keys::default());
    }

    #[test]
    fn pause_discards_wall_time_and_requires_a_fresh_press() {
        let start = Instant::now();
        let mut clock = PauseClock::new(start);
        let running = clock.sample(start + Duration::from_millis(10), Input::default());
        assert_eq!(running.elapsed, Duration::from_millis(10));
        let mut press = Input::default();
        press.record_press(Scancode::Escape, 0, false);
        let entered = clock.sample(start + Duration::from_millis(20), press);
        assert!(entered.paused && entered.changed);
        assert_eq!(entered.elapsed, Duration::ZERO);
        let mut repeated = Input::default();
        repeated.record_press(Scancode::Escape, 0, true);
        let held = clock.sample(start + Duration::from_secs(30), repeated);
        assert!(held.paused && !held.changed);
        assert_eq!(held.elapsed, Duration::ZERO);
        let mut resume = Input::default();
        resume.record_press(Scancode::P, 0, false);
        let resumed = clock.sample(start + Duration::from_secs(60), resume);
        assert!(!resumed.paused && resumed.changed);
        assert_eq!(resumed.elapsed, Duration::ZERO);
        assert_eq!(resumed.resumed_after, Some(Duration::from_millis(59_980)));
        // Diagnostic deadlines retain the unelapsed portion of a blocking wait.
        let deadline = start + Duration::from_millis(100) + resumed.resumed_after.unwrap();
        assert_eq!(
            deadline.duration_since(start + Duration::from_secs(60)),
            Duration::from_millis(80)
        );
        let running = clock.sample(start + Duration::from_millis(60_015), Input::default());
        assert_eq!(running.elapsed, Duration::from_millis(15));
    }

    #[test]
    fn menu_keys_do_not_pause_or_dismiss_native_questions_after_closing() {
        let key = |scan, repeat| Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: Some(scan),
            keycode: None,
            keymod: Mod::NOMOD,
            repeat,
        };
        let mut suppressed = HashSet::new();
        let mut input = Input::default();
        for scan in [Scancode::Escape, Scancode::P, Scancode::K, Scancode::Return] {
            route_guest_key(&key(scan, false), true, &mut suppressed, &mut input);
            // A repeat or second down after closing is not a fresh game press.
            route_guest_key(&key(scan, true), false, &mut suppressed, &mut input);
            route_guest_key(&key(scan, false), false, &mut suppressed, &mut input);
        }
        assert_eq!(input, Input::default());
        route_guest_key(
            &Event::KeyUp {
                timestamp: 0,
                window_id: 1,
                which: 0,
                raw: 0,
                scancode: Some(Scancode::K),
                keycode: None,
                keymod: Mod::NOMOD,
                repeat: false,
            },
            false,
            &mut suppressed,
            &mut input,
        );
        route_guest_key(&key(Scancode::K, false), false, &mut suppressed, &mut input);
        assert_eq!(input.menu_key, Some(MenuKey::Kitten));
        assert!(input.continue_pressed && !input.pause_pressed);
    }

    #[test]
    fn settings_freezes_time_and_preserves_the_manual_pause() {
        let start = Instant::now();
        let mut clock = PauseClock::new(start);
        let settings = |open| Input {
            settings_open: open,
            settings_changed: true,
            ..Input::default()
        };
        let opened = clock.sample(start + Duration::from_millis(20), settings(true));
        assert!(opened.paused && opened.changed);
        assert_eq!(opened.elapsed, Duration::ZERO);
        let closed = clock.sample(start + Duration::from_secs(60), settings(false));
        assert!(!closed.paused && closed.changed);
        assert_eq!(closed.elapsed, Duration::ZERO);
        assert_eq!(closed.resumed_after, Some(Duration::from_millis(59_980)));
        assert_eq!(
            clock
                .sample(start + Duration::from_millis(60_010), Input::default())
                .elapsed,
            Duration::from_millis(10)
        );

        let pause = Input {
            pause_pressed: true,
            ..Input::default()
        };
        assert!(clock.sample(start + Duration::from_secs(61), pause).paused);
        assert!(
            clock
                .sample(start + Duration::from_secs(62), settings(true))
                .paused
        );
        let closed = clock.sample(start + Duration::from_secs(70), settings(false));
        assert!(closed.paused && closed.changed);
        assert_eq!(closed.resumed_after, None);
        // The panel's close key cannot cancel a pre-existing manual pause.
        let resumed = clock.sample(start + Duration::from_secs(71), pause);
        assert!(!resumed.paused);
        assert_eq!(resumed.resumed_after, Some(Duration::from_secs(10)));
    }

    #[test]
    fn settings_boundary_suppresses_pause_keys_and_elapsed_time() {
        let start = Instant::now();
        let mut clock = PauseClock::new(start);
        // Even an open and close within one event pump is a host boundary.
        let sample = clock.sample(
            start + Duration::from_secs(3),
            Input {
                settings_changed: true,
                pause_pressed: true,
                ..Input::default()
            },
        );
        assert!(!sample.paused && sample.changed);
        assert_eq!(sample.elapsed, Duration::ZERO);
        assert_eq!(sample.resumed_after, None);
        let opened = clock.sample(
            start + Duration::from_secs(4),
            Input {
                settings_open: true,
                pause_pressed: true,
                ..Input::default()
            },
        );
        assert!(opened.paused);
        let closed = clock.sample(start + Duration::from_secs(5), Input::default());
        assert!(!closed.paused);
    }

    #[test]
    fn all_pause_keys_can_resume_without_immediately_repausing() {
        for scancode in [Scancode::Escape, Scancode::P, Scancode::Pause] {
            let now = Instant::now();
            let mut clock = PauseClock::new(now);
            let mut press = Input::default();
            press.record_press(scancode, 0, false);
            assert!(!press.quit);
            assert!(clock.sample(now, press).paused);
            assert!(!clock.sample(now + Duration::from_secs(1), press).paused);
            assert!(
                !clock
                    .sample(now + Duration::from_secs(2), Input::default())
                    .paused
            );
        }
    }

    #[test]
    fn movement_and_desktop_shortcuts_do_not_change_pause_state() {
        let mut ignored = vec![
            (Scancode::Up, 0),
            (Scancode::Down, 0),
            (Scancode::Left, 0),
            (Scancode::Right, 0),
            (Scancode::Tab, 0x0100),  // Alt+Tab
            (Scancode::LAlt, 0x0100), // Alt itself, before Tab arrives
            (Scancode::Return, 0),    // Enter
            (Scancode::Space, 0),     // Space
        ];
        for modifier in [0x0040, 0x0080, 0x0100, 0x0200, 0x0400, 0x0800] {
            for key in [Scancode::Escape, Scancode::P, Scancode::Pause] {
                ignored.push((key, modifier));
            }
        }
        for (key, modifier) in ignored {
            let start = Instant::now();
            let mut clock = PauseClock::new(start);
            let mut input = Input::default();
            input.record_press(key, modifier, false);
            assert!(!clock.sample(start, input).changed);
            let mut pause = Input::default();
            pause.record_press(Scancode::Escape, 0, false);
            assert!(clock.sample(start, pause).paused);
            let ignored = clock.sample(start + Duration::from_secs(1), input);
            assert!(ignored.paused && !ignored.changed);
            assert_eq!(ignored.elapsed, Duration::ZERO);
        }
    }

    #[test]
    fn quit_follows_the_layout_letter_instead_of_the_physical_q_position() {
        let event = |scan, key| Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: Some(scan),
            keycode: Some(key),
            keymod: Mod::LCTRLMOD,
            repeat: false,
        };
        let mut input = Input::default();
        input.record_event(&event(Scancode::Q, Keycode::A));
        assert!(!input.quit);
        input.record_event(&event(Scancode::A, Keycode::Q));
        assert!(input.quit);
    }

    #[test]
    fn quit_requires_control() {
        let mut plain = Input::default();
        plain.record_press(Scancode::Q, 0, false);
        assert!(!plain.quit);
        for control in [0x0040, 0x0080] {
            let mut input = Input::default();
            input.record_press(Scancode::Q, control, false);
            assert!(input.quit);
            assert!(!input.pause_pressed);
        }
    }

    #[test]
    fn startup_requires_a_fresh_key_without_desktop_shortcuts() {
        for (scancode, modifiers, repeat, expected) in [
            (Scancode::Space, 0, false, true),  // Space.
            (Scancode::Return, 0, false, true), // Return.
            (Scancode::Space, 0, true, false),
            (Scancode::Tab, 0x0100, false, false), // Alt+Tab.
            (Scancode::LShift, 0x0001, false, false), // Shift alone.
            (Scancode::Q, CONTROL, false, false),
        ] {
            let mut input = Input::default();
            input.record_press(scancode, modifiers, repeat);
            assert_eq!(input.continue_pressed, expected);
        }
    }

    #[test]
    fn menu_choices_and_game_shortcuts_use_fresh_key_events() {
        let mut queued = Input::default();
        queued.record_press(Scancode::K, 0, false);
        queued.record_press(Scancode::Return, 0, false);
        assert_eq!(queued.menu_key, Some(MenuKey::Kitten));
        for (scancode, expected) in [
            (Scancode::K, MenuKey::Kitten),
            (Scancode::H, MenuKey::HouseCat),
            (Scancode::T, MenuKey::Tomcat),
            (Scancode::A, MenuKey::AlleyCat),
            (Scancode::Up, MenuKey::Up),
            (Scancode::Down, MenuKey::Down),
            (Scancode::Return, MenuKey::Confirm),
            (Scancode::KpEnter, MenuKey::Confirm),
            (Scancode::Space, MenuKey::Confirm),
        ] {
            let mut input = Input::default();
            input.record_press(scancode, 0, false);
            assert_eq!(input.menu_key, Some(expected));
            for (modifiers, repeat) in [(0, true), (0x0100, false), (0x0040, false)] {
                let mut ignored = Input::default();
                ignored.record_press(scancode, modifiers, repeat);
                assert_eq!(ignored.menu_key, None);
            }
        }
        for control in [0x0040, 0x0080] {
            let mut input = Input::default();
            input.record_press(Scancode::M, control, false);
            assert!(input.menu_pressed);
            assert!(!input.continue_pressed && !input.pause_pressed && !input.quit);
            let mut input = Input::default();
            input.record_press(Scancode::R, control, false);
            assert!(input.new_game_pressed);
            assert!(!input.pause_pressed);
            let mut input = Input::default();
            input.record_press(Scancode::S, control, false);
            assert!(input.sound_pressed);
            assert!(!input.continue_pressed && input.menu_key.is_none());
        }
        for (modifiers, repeat) in [(0, false), (0x0040, true), (0x0140, false)] {
            let mut input = Input::default();
            input.record_press(Scancode::R, modifiers, repeat);
            input.record_press(Scancode::M, modifiers, repeat);
            input.record_press(Scancode::S, modifiers, repeat);
            assert!(!input.menu_pressed && !input.new_game_pressed && !input.sound_pressed);
        }
    }
}
