// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Alley Cat host input and pause overlay, using REforge's shared SDL backend.

use crate::alley::Machine;
use crate::menu::MenuKey;
use reforge_sdl::sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Mod, Scancode},
    pixels::Color,
    render::FRect,
};
pub use reforge_sdl::{FrameCreator, Window as Display};
use reforge_sdl::{FrameFormat, FrameTexture};
use std::time::{Duration, Instant};

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
}

/// Host pause bookkeeping shared by the normal and diagnostic runners.
pub struct PauseClock {
    previous: Instant,
    paused_at: Option<Instant>,
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
        }
    }

    pub fn sample(&mut self, now: Instant, input: Input) -> PauseSample {
        let mut resumed_after = None;
        let changed = if let Some(start) = self.paused_at {
            if input.pause_pressed {
                self.paused_at = None;
                resumed_after = Some(now.duration_since(start));
                true
            } else {
                false
            }
        } else if input.pause_pressed {
            self.paused_at = Some(now);
            true
        } else {
            false
        };
        let paused = self.paused_at.is_some();
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
    frame: FrameTexture<'a>,
    quit: bool,
}

impl<'a> Window<'a> {
    pub fn new(
        display: Display,
        creator: &'a FrameCreator,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            display,
            frame: FrameTexture::new(creator, width, height, FrameFormat::Rgba)?,
            quit: false,
        })
    }

    pub fn poll_input(&mut self) -> Input {
        let mut presses = Input::default();
        for event in self.display.poll_events() {
            match event {
                Event::Quit { .. }
                | Event::Window {
                    win_event: WindowEvent::CloseRequested,
                    ..
                } => self.quit = true,
                Event::KeyDown {
                    scancode: Some(scancode),
                    keymod,
                    repeat,
                    ..
                } => presses.record_press(scancode, keymod.bits(), repeat),
                _ => {}
            }
        }
        self.quit |= presses.quit;
        let held = self.display.keyboard_state();
        Input {
            up: held.is_scancode_pressed(Scancode::Up),
            down: held.is_scancode_pressed(Scancode::Down),
            left: held.is_scancode_pressed(Scancode::Left),
            right: held.is_scancode_pressed(Scancode::Right),
            alt: held.is_scancode_pressed(Scancode::LAlt)
                || held.is_scancode_pressed(Scancode::RAlt),
            quit: self.quit,
            pause_pressed: presses.pause_pressed,
            continue_pressed: presses.continue_pressed,
            menu_key: presses.menu_key,
            menu_pressed: presses.menu_pressed,
            new_game_pressed: presses.new_game_pressed,
        }
    }

    pub fn present(&mut self, rgba: &[u8]) -> Result<(), String> {
        self.present_with_pause(rgba, None)
    }

    pub fn present_with_pause(
        &mut self,
        rgba: &[u8],
        pause: Option<&PauseOverlay>,
    ) -> Result<(), String> {
        self.frame.update(rgba)?;
        self.display.present_with(&self.frame, |canvas| {
            if let Some(pause) = pause {
                // The overlay uses original 320x200 coordinates in a 320x240 display.
                canvas
                    .set_scale(1.0, 1.2)
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
            Ok(())
        })
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
    fn record_press(&mut self, scancode: Scancode, modifiers: u16, repeat: bool) {
        if scancode == Scancode::Q && modifiers & CONTROL != 0 {
            self.quit = true;
        }
        if !repeat && modifiers & CONTROL != 0 && modifiers & 0x0f00 == 0 {
            self.new_game_pressed |= scancode == Scancode::R;
            self.menu_pressed |= scancode == Scancode::M;
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
                    _ => MenuKey::Other,
                });
            }
            self.pause_pressed |=
                matches!(scancode, Scancode::Escape | Scancode::P | Scancode::Pause);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            (Scancode::Return, MenuKey::Other),
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
        }
        for (modifiers, repeat) in [(0, false), (0x0040, true), (0x0140, false)] {
            let mut input = Input::default();
            input.record_press(Scancode::R, modifiers, repeat);
            input.record_press(Scancode::M, modifiers, repeat);
            assert!(!input.menu_pressed && !input.new_game_pressed);
        }
    }
}
