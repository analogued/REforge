// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Modern host shortcuts, separate from reconstructed DOS keyboard processing.

use sdl3::keyboard::{Mod, Scancode};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostShortcut {
    Quit,
    TogglePause,
}

pub fn host_shortcut(scancode: Scancode, modifiers: Mod, repeat: bool) -> Option<HostShortcut> {
    let control = Mod::LCTRLMOD | Mod::RCTRLMOD;
    let desktop = control | Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD;
    if scancode == Scancode::Q && modifiers.intersects(control) {
        Some(HostShortcut::Quit)
    } else if scancode == Scancode::Pause && !repeat && !modifiers.intersects(desktop) {
        Some(HostShortcut::TogglePause)
    } else {
        None
    }
}

/// A host timeline that freezes while paused. Existing engine deadlines remain
/// in that timeline, preserving their remaining waits without catching up.
#[derive(Default)]
pub struct HostPause {
    started: Option<Instant>,
    elapsed: Duration,
}

impl HostPause {
    pub fn toggle(&mut self, now: Instant) {
        if let Some(started) = self.started.take() {
            self.elapsed += now.duration_since(started);
        } else {
            self.started = Some(now);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.started.is_some()
    }

    pub fn now(&self, now: Instant) -> Instant {
        self.started.unwrap_or(now) - self.elapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_leave_game_keys_and_desktop_combinations_alone() {
        for control in [Mod::LCTRLMOD, Mod::RCTRLMOD] {
            assert_eq!(
                host_shortcut(Scancode::Q, control, false),
                Some(HostShortcut::Quit)
            );
        }
        assert_eq!(host_shortcut(Scancode::Q, Mod::NOMOD, false), None);
        assert_eq!(
            host_shortcut(Scancode::Pause, Mod::NOMOD, false),
            Some(HostShortcut::TogglePause)
        );
        for modifier in [
            Mod::LCTRLMOD,
            Mod::RCTRLMOD,
            Mod::LALTMOD,
            Mod::RALTMOD,
            Mod::LGUIMOD,
            Mod::RGUIMOD,
        ] {
            assert_eq!(host_shortcut(Scancode::Pause, modifier, false), None);
        }
        assert_eq!(host_shortcut(Scancode::Pause, Mod::NOMOD, true), None);
        for scan in [
            Scancode::Escape,
            Scancode::P,
            Scancode::F1,
            Scancode::Space,
            Scancode::Up,
        ] {
            assert_eq!(host_shortcut(scan, Mod::NOMOD, false), None);
        }
    }

    #[test]
    fn host_pause_preserves_deadlines_across_multiple_pauses() {
        let start = Instant::now();
        let mut pause = HostPause::default();
        let deadline = start + Duration::from_millis(100);
        pause.toggle(start + Duration::from_millis(20));
        assert!(pause.is_paused());
        assert_eq!(
            pause.now(start + Duration::from_secs(60)),
            start + Duration::from_millis(20)
        );
        pause.toggle(start + Duration::from_secs(60));
        assert!(!pause.is_paused());
        assert_eq!(
            deadline.duration_since(pause.now(start + Duration::from_secs(60))),
            Duration::from_millis(80)
        );
        pause.toggle(start + Duration::from_millis(60_010));
        pause.toggle(start + Duration::from_millis(90_010));
        assert_eq!(
            pause.now(start + Duration::from_millis(90_030)),
            start + Duration::from_millis(50)
        );
    }
}
