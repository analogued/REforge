// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Idle game cursors, independent of simulation time and presentation cadence.

use sdl3::{
    event::{Event, WindowEvent},
    mouse::MouseUtil,
    video::Window,
};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(2);

struct Activity {
    last_mouse: Instant,
}

impl Activity {
    fn observe(&mut self, event: &Event, window_id: u32, now: Instant) {
        let activity = match event {
            Event::MouseMotion {
                window_id: id,
                xrel,
                yrel,
                ..
            } => *id == window_id && (*xrel != 0.0 || *yrel != 0.0),
            Event::MouseButtonDown { window_id: id, .. }
            | Event::MouseButtonUp { window_id: id, .. }
            | Event::MouseWheel { window_id: id, .. } => *id == window_id,
            Event::Window {
                window_id: id,
                win_event: WindowEvent::MouseEnter | WindowEvent::FocusGained,
                ..
            } => *id == window_id,
            _ => false,
        };
        if activity {
            self.last_mouse = now;
        }
    }

    fn visible(&mut self, now: Instant, active: bool, host_ui: bool) -> bool {
        if !active || host_ui {
            // Give the player a fresh interval after leaving a menu or returning
            // to the window, even if no mouse motion accompanied that transition.
            self.last_mouse = now;
            return true;
        }
        now.saturating_duration_since(self.last_mouse) < IDLE
    }
}

pub(super) struct AutoHide {
    activity: Activity,
    mouse: MouseUtil,
    hidden: bool,
}

impl AutoHide {
    pub(super) fn new(mouse: MouseUtil) -> Self {
        Self {
            activity: Activity {
                last_mouse: Instant::now(),
            },
            mouse,
            hidden: false,
        }
    }

    pub(super) fn observe(&mut self, event: &Event, window_id: u32) {
        self.activity.observe(event, window_id, Instant::now());
    }

    pub(super) fn update(&mut self, window: &Window, host_ui: bool) {
        let active = window.has_input_focus() && window.has_mouse_focus();
        self.update_context(Instant::now(), active, host_ui);
    }

    fn update_context(&mut self, now: Instant, active: bool, host_ui: bool) {
        let visible = self.activity.visible(now, active, host_ui);
        let hidden = !visible;
        if hidden != self.hidden {
            self.mouse.show_cursor(visible);
            self.hidden = hidden;
        }
    }
}

impl Drop for AutoHide {
    fn drop(&mut self) {
        // SDL cursor visibility is process-wide. Returning to the launcher or
        // replacing a renderer must not leave its mouse cursor hidden.
        if self.hidden {
            self.mouse.show_cursor(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion(window_id: u32, delta: f32) -> Event {
        Event::MouseMotion {
            timestamp: 0,
            window_id,
            which: 0,
            mousestate: sdl3::mouse::MouseState::from_sdl_state(0),
            x: 10.0,
            y: 20.0,
            xrel: delta,
            yrel: 0.0,
        }
    }

    #[test]
    fn idle_cursor_reappears_on_motion_and_ignores_other_windows_and_zero_motion() {
        let start = Instant::now();
        let mut activity = Activity { last_mouse: start };
        assert!(activity.visible(start + IDLE - Duration::from_millis(1), true, false));
        assert!(!activity.visible(start + IDLE, true, false));
        for event in [motion(2, 1.0), motion(1, 0.0)] {
            activity.observe(&event, 1, start + IDLE);
            assert!(!activity.visible(start + IDLE, true, false));
        }
        activity.observe(&motion(1, 1.0), 1, start + IDLE);
        assert!(activity.visible(start + IDLE, true, false));
        assert!(!activity.visible(start + IDLE * 2, true, false));
    }

    #[test]
    fn host_ui_and_inactive_windows_keep_cursor_visible_and_restart_idle_interval() {
        let start = Instant::now();
        for (active, host_ui) in [(true, true), (false, false)] {
            let mut activity = Activity { last_mouse: start };
            assert!(!activity.visible(start + IDLE, true, false));
            assert!(activity.visible(start + IDLE * 10, active, host_ui));
            assert!(activity.visible(start + IDLE * 10, true, false));
            assert!(!activity.visible(start + IDLE * 11, true, false));
        }
    }

    #[test]
    fn mouse_clicks_wake_cursor_but_game_input_does_not_keep_it_awake() {
        use sdl3::{keyboard::Mod, mouse::MouseButton};
        let start = Instant::now();
        let mut activity = Activity { last_mouse: start };
        for event in [
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                which: 0,
                raw: 0,
                scancode: None,
                keycode: None,
                keymod: Mod::NOMOD,
                repeat: true,
            },
            Event::GamepadAxisMotion {
                timestamp: 0,
                which: 0.into(),
                axis: sdl3::gamepad::Axis::LeftX,
                value: 32767,
            },
        ] {
            activity.observe(&event, 1, start + IDLE);
            assert!(!activity.visible(start + IDLE, true, false));
        }
        activity.observe(
            &Event::MouseButtonDown {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: 10.0,
                y: 20.0,
            },
            1,
            start + IDLE,
        );
        assert!(activity.visible(start + IDLE, true, false));
    }

    #[test]
    #[ignore = "owns SDL video; run alone with SDL_VIDEODRIVER=dummy"]
    fn native_cursor_restores_for_settings_file_ui_and_window_drop() {
        use crate::settings::{Gui, RuntimeSettings, SettingsMenu, VideoCapabilities};
        use crate::{DisplayGeometry, FrameFormat, FrameTexture, Presentation, Window};
        let mut window = Window::build(
            "cursor lifecycle",
            DisplayGeometry::new((320, 200), (320, 240)),
            RuntimeSettings {
                renderer: reforge_settings::Renderer::Software,
                scale: Some(2),
                vsync: false,
                ..Default::default()
            },
            Presentation::Pixels,
            true,
            reforge_settings::Session::for_window(Some(2)),
            None,
        )
        .unwrap();
        let mouse = window._sdl.mouse();
        let creator = window.texture_creator();
        let frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgb).unwrap();
        let start = Instant::now();
        window.cursor.activity.last_mouse = start;
        window.cursor.update_context(start + IDLE, true, false);
        assert!(!mouse.is_cursor_showing());

        let mut settings = SettingsMenu::new(&creator, VideoCapabilities::default());
        settings.toggle();
        window.prepare_settings(&mut settings);
        window
            .present_with(&frame, |canvas| settings.paint(canvas))
            .unwrap();
        assert!(creator.pointer_ui.get());
        window
            .cursor
            .update_context(start + IDLE * 10, true, creator.pointer_ui.get());
        assert!(
            mouse.is_cursor_showing(),
            "settings remain usable while idle"
        );

        let mut files = Gui::new(&creator);
        files.prepare(window.ui_metrics(), |context| {
            egui::Window::new("Game files").show(context, |ui| {
                ui.label("Bookmark filename");
            });
        });
        window
            .present_with(&frame, |canvas| files.paint(canvas))
            .unwrap();
        assert!(creator.pointer_ui.get());
        window
            .cursor
            .update_context(start + IDLE * 20, true, creator.pointer_ui.get());
        assert!(
            mouse.is_cursor_showing(),
            "file prompts remain usable while idle"
        );

        window.present(&frame).unwrap();
        assert!(
            !creator.pointer_ui.get(),
            "closing UI releases the cursor policy"
        );
        window
            .cursor
            .update_context(start + IDLE * 21, true, creator.pointer_ui.get());
        assert!(!mouse.is_cursor_showing());
        drop(window);
        assert!(
            mouse.is_cursor_showing(),
            "returning to the launcher restores the cursor"
        );
    }
}
