// Copyright (c) 2026 analogued. Project license has not been selected.
//! Persistent application state and the original keyboard/pause control rules.

use crate::sound::{self, Effect, Foreground, Tone};

use super::{EndingSnapshot, EnemyState, Random, SpecialState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    #[default]
    Color,
    Monochrome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub display_mode: DisplayMode,
    /// Original S/s switch: skip digitized samples, retaining procedural sound.
    pub digitized_samples_enabled: bool,
    /// F2's persistent playback-disable flag.
    pub sound_disabled: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            display_mode: DisplayMode::Color,
            digitized_samples_enabled: true,
            sound_disabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldKeys {
    /// Up, Down, Left, Right, Fire are bits 0 through 4.
    pub actions: u8,
    pub joystick: u8,
    pub f1: bool,
    pub f2: bool,
    pub f3: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputPoll {
    pub mask: u8,
    pub sound_toggled: bool,
    pub display_toggled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Controls {
    pub preferences: Preferences,
    pub sound_poll_counter: u16,
}

impl Controls {
    /// Poll once at an original input-poll site, rather than once per OS event.
    pub fn poll(&mut self, held: HeldKeys) -> InputPoll {
        let mut result = InputPoll {
            mask: held.joystick & 31,
            ..InputPoll::default()
        };
        if held.f2 {
            self.sound_poll_counter = self.sound_poll_counter.wrapping_add(1);
            if self.sound_poll_counter as i16 > 3 {
                self.sound_poll_counter = 0;
                self.preferences.sound_disabled = !self.preferences.sound_disabled;
                result.sound_toggled = true;
                return result;
            }
        }
        if held.f3 {
            self.preferences.display_mode = match self.preferences.display_mode {
                DisplayMode::Color => DisplayMode::Monochrome,
                DisplayMode::Monochrome => DisplayMode::Color,
            };
            result.mask = 0;
            result.display_toggled = true;
        } else if held.f1 {
            result.mask = 128;
        } else {
            result.mask |= held.actions & 31;
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct KeyboardState {
    scans: [u8; 128],
    last_raw_scan: u8,
    // Optional frontend policy; it does not alter reconstructed scan bytes or
    // Controls::poll. Captured-original replay retains the Original default.
    f3_mode: F3Mode,
    f3_press: u64,
    f3_consumed: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum F3Mode {
    /// Toggle only at the first accepting poll of a fresh F3 press.
    Press,
    /// Original behavior: toggle at every accepting poll while F3 is held.
    #[default]
    Original,
}

impl F3Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Original => "original",
        }
    }
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self {
            scans: [0; 128],
            last_raw_scan: 0,
            f3_mode: F3Mode::Original,
            f3_press: 0,
            f3_consumed: 0,
        }
    }
}

impl KeyboardState {
    /// The IRQ suppresses consecutive identical bytes and treats E0 as a break.
    pub fn apply_scan(&mut self, raw: u8) {
        if raw == self.last_raw_scan {
            return;
        }
        self.last_raw_scan = raw;
        if raw == 0x3d && self.scans[0x3d] == 0 {
            self.f3_press = self.f3_press.wrapping_add(1);
        }
        self.scans[usize::from(raw & 127)] = if raw & 128 == 0 { raw } else { 0 };
    }

    /// Select host QoL behavior without replaying an already-held press.
    pub fn set_f3_mode(&mut self, mode: F3Mode) {
        if self.f3_mode != mode {
            self.f3_mode = mode;
            self.f3_consumed = self.f3_press;
        }
    }

    /// Poll at the reconstructed site, preserving function-key priorities.
    /// A press is consumed only if F3 actually toggles, so an F2-priority return
    /// does not lose it. Make/break identity preserves represses between polls.
    pub fn poll_controls(&mut self, controls: &mut Controls, joystick: u8) -> InputPoll {
        let mut held = self.held_keys(joystick);
        if self.f3_mode == F3Mode::Press && self.f3_consumed == self.f3_press {
            held.f3 = false;
        }
        let poll = controls.poll(held);
        if self.f3_mode == F3Mode::Press && poll.display_toggled {
            self.f3_consumed = self.f3_press;
        }
        poll
    }

    /// Explicit frontend focus/input reset, clearing the IRQ duplicate filter too.
    pub fn clear(&mut self) {
        self.clear_table();
        self.last_raw_scan = 0;
    }

    /// Playback completion clears the table while retaining the IRQ last byte.
    pub fn clear_table(&mut self) {
        self.scans.fill(0);
    }

    pub fn held_keys(&self, joystick: u8) -> HeldKeys {
        let down = |scan: usize| self.scans[scan] != 0;
        let up = down(0x1f) || down(0x48) || down(0x7c);
        let below = down(0x2d) || down(0x50) || down(0x7b);
        let left = down(0x7a) || down(0x30) || down(0x4b);
        let right = down(0x4d) || down(0x31) || down(0x79);
        let fire = down(0x4c) || down(0x39) || down(0x77);
        HeldKeys {
            actions: u8::from(up)
                | (u8::from(below) << 1)
                | (u8::from(left) << 2)
                | (u8::from(right) << 3)
                | (u8::from(fire) << 4),
            joystick,
            f1: down(0x3b),
            f2: down(0x3c),
            f3: down(0x3d),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PauseState {
    #[default]
    Running,
    AwaitInitialRelease,
    WaitBeforePress(u8),
    AwaitPress,
    WaitBeforeDecision(u8),
    SampleDecision,
    AwaitFinalRelease,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PauseEvent {
    #[default]
    None,
    Paused,
    /// Set the game's exit flag now; the current body still completes on resume.
    EndGame,
    Resumed,
}

#[derive(Clone, Debug, Default)]
pub struct PauseControl {
    pub state: PauseState,
}

impl PauseControl {
    pub fn poll(&mut self, mask: u8) -> PauseEvent {
        self.poll_with_sound(mask, &mut |_| {})
    }

    pub fn poll_with_sound(&mut self, mask: u8, sound: &mut Foreground<'_>) -> PauseEvent {
        match self.state {
            PauseState::Running if mask & 128 != 0 => {
                sound::request(sound, 0x2699, Effect::Tone(Tone::First));
                self.state = PauseState::AwaitInitialRelease;
                PauseEvent::Paused
            }
            PauseState::AwaitInitialRelease if mask == 0 => {
                self.state = PauseState::WaitBeforePress(2);
                PauseEvent::None
            }
            PauseState::AwaitPress if mask != 0 => {
                self.state = PauseState::WaitBeforeDecision(2);
                PauseEvent::None
            }
            PauseState::SampleDecision => {
                self.state = PauseState::AwaitFinalRelease;
                if mask & 128 != 0 {
                    PauseEvent::EndGame
                } else {
                    PauseEvent::None
                }
            }
            PauseState::AwaitFinalRelease if mask == 0 => {
                self.state = PauseState::Running;
                PauseEvent::Resumed
            }
            _ => PauseEvent::None,
        }
    }

    /// Advance one original timer wait unit while world simulation is paused.
    pub fn tick(&mut self) {
        self.state = match self.state {
            PauseState::WaitBeforePress(1) => PauseState::AwaitPress,
            PauseState::WaitBeforePress(remaining) => PauseState::WaitBeforePress(remaining - 1),
            PauseState::WaitBeforeDecision(1) => PauseState::SampleDecision,
            PauseState::WaitBeforeDecision(remaining) => {
                PauseState::WaitBeforeDecision(remaining - 1)
            }
            state => state,
        };
    }
}

#[derive(Clone, Debug)]
pub struct RetainedWorld {
    pub records: [EnemyState; 29],
    pub order: [usize; 29],
    /// Special records retain extents and scale not carried by ordinary records.
    pub special: [SpecialState; 5],
    pub cached_requests: [u8; 29],
}

impl RetainedWorld {
    /// Apply only the original new-game writes; descriptor/projection state survives.
    pub fn reset_for_game(&mut self) {
        for (index, record) in self.records[..5].iter_mut().enumerate() {
            record.lateral = 0;
            record.active = index == 0;
            record.depth = match index {
                0 | 4 => 14,
                1 => 0,
                2 => 400,
                _ => 700,
            };
            record.height = if index == 1 { -115 } else { -100 };
            record.movement = match index {
                0 => 255,
                1 => 0,
                2 | 3 => 254,
                _ => 253,
            };
            record.requested_sprite = match index {
                0 => 1,
                1 => 0,
                _ => 22,
            };
        }
        for record in &mut self.records[5..15] {
            record.active = false;
            record.movement = 0;
        }
        for (index, record) in self.records[15..23].iter_mut().enumerate() {
            record.depth = 1000 + 100 * index as i16;
            record.active = false;
            record.movement = 0;
        }
        for record in &mut self.records[23..28] {
            record.active = false;
            record.movement = 1;
        }
        for record in &mut self.special {
            record.active = false;
            record.movement = 1;
        }
        self.order = std::array::from_fn(|index| index);
        self.cached_requests = self
            .records
            .map(|record| (record.requested_sprite as u8).wrapping_add(1));
    }
}

#[derive(Clone, Debug)]
pub struct GameStart {
    pub script_index: u16,
    pub random: [u16; 3],
    pub retained_world: Option<RetainedWorld>,
}

#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub random: [u16; 3],
    pub best_score: u32,
    pub retained_world: Option<RetainedWorld>,
    pub controls: Controls,
    pub keyboard: KeyboardState,
    pub pause: PauseControl,
    pub joystick: u8,
}

impl SessionState {
    /// True selects the raw DATA6 intro; false selects DATA1.
    pub fn select_intro(&mut self) -> bool {
        self.next_word() & 7 < 4
    }

    /// Choose the next initial script and reset retained world records.
    /// The caller also resets gameplay scalar state; best and preferences persist.
    pub fn begin_game(&mut self) -> GameStart {
        let script_index = self.next_word() & 3;
        if let Some(world) = &mut self.retained_world {
            world.reset_for_game();
        }
        GameStart {
            script_index,
            random: self.random,
            retained_world: self.retained_world.clone(),
        }
    }

    /// Call after returned_intro_choice when Ending already consumed the intro word.
    pub fn remember_ending(&mut self, state: &EndingSnapshot, special: [SpecialState; 5]) {
        self.random = state.random;
        self.best_score = state.best_score % 1_000_000;
        self.retained_world = Some(RetainedWorld {
            records: state.world,
            order: state.full_world_order,
            special,
            cached_requests: state
                .world
                .map(|record| (record.requested_sprite as u8).wrapping_add(1)),
        });
    }

    pub fn update_best(&mut self, score: u32) {
        self.best_score = self.best_score.max(score % 1_000_000);
    }

    /// Model only playback's input side effects, without synthesizing audio.
    pub fn finish_audio_input(&mut self, last_scan: u8) {
        self.keyboard.clear_table();
        // The 65-word clear extends through the counter's low byte at DS:1847.
        self.controls.sound_poll_counter &= 0xff00;
        if last_scan & 127 == 0x3c {
            self.controls.preferences.sound_disabled = true;
        }
    }

    /// Release frontend-held controls without cancelling the native pause handshake.
    pub fn clear_held_keys(&mut self) {
        self.joystick = 0;
        self.keyboard.clear();
    }

    fn next_word(&mut self) -> u16 {
        let mut random = Random { words: self.random };
        let word = random.next_word();
        self.random = random.state();
        word
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Rect;

    #[test]
    fn f3_press_policy_keeps_physical_state_and_repress_identity() {
        let mut keyboard = KeyboardState::default();
        keyboard.set_f3_mode(F3Mode::Press);
        let mut controls = Controls::default();
        keyboard.apply_scan(0x3d);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        assert_eq!(controls.preferences.display_mode, DisplayMode::Monochrome);
        for _ in 0..10 {
            keyboard.apply_scan(0x48);
            keyboard.apply_scan(0x3d); // Duplicate makes are not fresh presses.
            assert!(!keyboard.poll_controls(&mut controls, 0).display_toggled);
        }
        assert!(keyboard.held_keys(0).f3);
        keyboard.apply_scan(0xbd);
        keyboard.apply_scan(0x3d); // Release/repress both occur between polls.
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        assert_eq!(controls.preferences.display_mode, DisplayMode::Color);
        keyboard.apply_scan(0xbd);
        keyboard.apply_scan(0x3d);
        keyboard.apply_scan(0xbd); // A tap entirely between polls stays missed.
        assert!(!keyboard.poll_controls(&mut controls, 0).display_toggled);
        keyboard.apply_scan(0x3d);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
    }

    #[test]
    fn f3_press_waits_for_priority_and_original_polling_remains_available() {
        let mut keyboard = KeyboardState::default();
        keyboard.set_f3_mode(F3Mode::Press);
        let mut controls = Controls {
            sound_poll_counter: 3,
            ..Controls::default()
        };
        keyboard.apply_scan(0x3c);
        keyboard.apply_scan(0x3d);
        let first = keyboard.poll_controls(&mut controls, 0);
        assert!(first.sound_toggled && !first.display_toggled);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        assert!(!keyboard.poll_controls(&mut controls, 0).display_toggled);
        assert_eq!(controls.sound_poll_counter, 2);
        keyboard.set_f3_mode(F3Mode::Original);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        // F2's next priority return still takes precedence in Original mode.
        assert!(keyboard.poll_controls(&mut controls, 0).sound_toggled);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        assert_eq!(KeyboardState::default().f3_mode, F3Mode::Original);
        let held = HeldKeys {
            f3: true,
            ..HeldKeys::default()
        };
        assert!(controls.poll(held).display_toggled);
        assert!(controls.poll(held).display_toggled);
    }

    #[test]
    fn f3_policy_survives_lifecycle_and_discards_cleared_intervals() {
        let mut keyboard = KeyboardState::default();
        keyboard.set_f3_mode(F3Mode::Press);
        let mut controls = Controls::default();
        keyboard.apply_scan(0x3d);
        assert!(keyboard.poll_controls(&mut controls, 0).display_toggled);
        let mut next_phase = keyboard.clone();
        assert!(!next_phase.poll_controls(&mut controls, 0).display_toggled);
        next_phase.apply_scan(0xbd);
        next_phase.apply_scan(0x3d);
        next_phase.clear_table(); // Playback teardown discards an unpolled press.
        assert!(!next_phase.poll_controls(&mut controls, 0).display_toggled);
        next_phase.apply_scan(0xbd);
        next_phase.apply_scan(0x3d);
        assert!(next_phase.poll_controls(&mut controls, 0).display_toggled);
        next_phase.clear(); // Focus/host pause resets cannot synthesize a toggle.
        assert!(!next_phase.poll_controls(&mut controls, 0).display_toggled);
        next_phase.apply_scan(0x3d);
        assert!(next_phase.poll_controls(&mut controls, 0).display_toggled);
    }

    #[test]
    fn repeated_game_keeps_rng_best_and_control_preferences() {
        let mut session = SessionState::default();
        assert!(session.select_intro());
        assert_eq!(session.begin_game().script_index, 0);
        let first = session.random;
        session.update_best(123_456);
        session.controls.preferences.display_mode = DisplayMode::Monochrome;
        session.controls.preferences.digitized_samples_enabled = false;
        session.controls.sound_poll_counter = 2;
        session.select_intro();
        assert_eq!(session.begin_game().script_index, 3);
        assert_ne!(session.random, first);
        session.update_best(5);
        assert_eq!(session.best_score, 123_456);
        session.clear_held_keys();
        assert_eq!(
            session.controls.preferences.display_mode,
            DisplayMode::Monochrome
        );
        assert_eq!(session.controls.sound_poll_counter, 2);
        assert!(!session.controls.preferences.digitized_samples_enabled);
    }

    #[test]
    fn control_priority_and_held_poll_toggles_match_the_original() {
        let mut controls = Controls::default();
        let both = HeldKeys {
            f1: true,
            f2: true,
            f3: true,
            actions: 16,
            joystick: 4,
        };
        for mode in [
            DisplayMode::Monochrome,
            DisplayMode::Color,
            DisplayMode::Monochrome,
        ] {
            assert_eq!(controls.poll(both).mask, 0);
            assert_eq!(controls.preferences.display_mode, mode);
        }
        let fourth = controls.poll(both);
        assert_eq!(fourth.mask, 4);
        assert!(fourth.sound_toggled);
        assert!(!fourth.display_toggled);
        assert_eq!(controls.preferences.display_mode, DisplayMode::Monochrome);
        controls.poll(HeldKeys::default());
        controls.poll(HeldKeys {
            f2: true,
            ..HeldKeys::default()
        });
        controls.poll(HeldKeys::default());
        assert_eq!(controls.sound_poll_counter, 1);
        assert_eq!(
            controls
                .poll(HeldKeys {
                    f1: true,
                    actions: 16,
                    joystick: 4,
                    ..HeldKeys::default()
                })
                .mask,
            128
        );
    }

    #[test]
    fn keyboard_break_extended_prefix_and_clear_preserve_control_aliases() {
        let mut keyboard = KeyboardState::default();
        keyboard.apply_scan(0x30); // B is a Left alias.
        keyboard.apply_scan(0x39); // Space is Fire.
        assert_eq!(keyboard.held_keys(0).actions, 20);
        keyboard.apply_scan(0xb0);
        assert_eq!(keyboard.held_keys(0).actions, 16);
        keyboard.apply_scan(0xe0);
        keyboard.apply_scan(0x48);
        assert_eq!(keyboard.held_keys(0).actions, 17);
        keyboard.clear();
        assert_eq!(keyboard.held_keys(0), HeldKeys::default());
        keyboard.apply_scan(0x48);
        assert_eq!(keyboard.held_keys(0).actions, 1);
    }

    #[test]
    fn pause_requires_release_and_two_waits_and_samples_the_final_key_state() {
        let mut pause = PauseControl::default();
        assert_eq!(pause.poll(128), PauseEvent::Paused);
        pause.tick();
        assert_eq!(pause.state, PauseState::AwaitInitialRelease);
        pause.poll(0);
        pause.tick();
        assert_eq!(pause.state, PauseState::WaitBeforePress(1));
        pause.tick();
        pause.poll(16);
        pause.tick();
        pause.tick();
        assert_eq!(pause.poll(128), PauseEvent::EndGame);
        assert_eq!(pause.poll(128), PauseEvent::None);
        assert_eq!(pause.poll(0), PauseEvent::Resumed);
        assert_eq!(pause.state, PauseState::Running);
    }

    #[test]
    fn released_f1_resume_press_does_not_request_end_game_after_waits() {
        let mut pause = PauseControl::default();
        pause.poll(128);
        pause.poll(0);
        pause.tick();
        pause.tick();
        pause.poll(128);
        pause.tick();
        pause.tick();
        assert_eq!(pause.poll(0), PauseEvent::None);
        assert_eq!(pause.poll(0), PauseEvent::Resumed);
    }

    #[test]
    fn world_reset_retains_projection_and_subtypes_while_reinitializing_selected_fields() {
        let records = std::array::from_fn(|slot| EnemyState {
            slot,
            depth: -9,
            lateral: 73,
            height: 29,
            active: true,
            movement: 252,
            variant: 3,
            animation_phase: 7,
            requested_sprite: 157,
            resolved_sprite: 123,
            rect: Rect {
                x_bytes: 41,
                y: 91,
                width_bytes: 5,
                height: 9,
            },
            phase: 3,
        });
        let special = std::array::from_fn(|index| SpecialState {
            slot: index + 23,
            coordinates: [-9, 73, 29],
            depth: -9,
            lateral: 73,
            height: 29,
            extents: [28, 350, 130],
            active: true,
            movement: 4,
            requested_sprite: 157,
            resolved_sprite: 123,
            phase: 3,
            scale: 800,
            rect: Rect {
                x_bytes: 41,
                y: 91,
                width_bytes: 5,
                height: 9,
            },
        });
        let mut world = RetainedWorld {
            records,
            special,
            order: std::array::from_fn(|index| 28 - index),
            cached_requests: [0; 29],
        };
        world.reset_for_game();
        assert_eq!(world.records[0].depth, 14);
        assert_eq!(world.records[0].requested_sprite, 1);
        assert_eq!(world.records[1].depth, 0);
        assert_eq!(world.records[1].height, -115);
        assert_eq!(world.records[3].depth, 700);
        assert_eq!(world.records[4].movement, 253);
        assert_eq!(world.records[5].depth, -9);
        assert_eq!(world.records[5].requested_sprite, 157);
        assert_eq!(world.records[15].depth, 1000);
        assert_eq!(world.records[22].depth, 1700);
        assert_eq!(world.records[15].lateral, 73);
        assert_eq!(world.records[15].variant, 3);
        assert_eq!(world.records[15].animation_phase, 7);
        assert_eq!(world.records[15].phase, 3);
        assert_eq!(world.records[15].resolved_sprite, 123);
        assert_eq!(world.records[15].rect.y, 91);
        assert!(world.records[28].active);
        assert_eq!(world.records[28].movement, 252);
        assert_eq!(world.special[0].extents, [28, 350, 130]);
        assert_eq!(world.special[0].scale, 800);
        assert!(!world.special[0].active);
        assert_eq!(world.cached_requests[0], 2);
        assert_eq!(world.cached_requests[15], 158);
        assert_eq!(world.order, std::array::from_fn(|index| index));
    }

    #[test]
    fn playback_clear_preserves_last_irq_byte_and_only_clears_counter_low_byte() {
        let mut session = SessionState::default();
        session.keyboard.apply_scan(0x48);
        session.controls.sound_poll_counter = 0x1203;
        session.finish_audio_input(0xbc);
        assert!(session.controls.preferences.sound_disabled);
        assert_eq!(session.controls.sound_poll_counter, 0x1200);
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        session.keyboard.apply_scan(0x48);
        assert_eq!(session.keyboard.held_keys(0).actions, 0); // Identical IRQ byte suppressed.
        session.keyboard.apply_scan(0xc8);
        session.keyboard.apply_scan(0x48);
        assert_eq!(session.keyboard.held_keys(0).actions, 1);
    }
    #[test]
    fn native_pause_requests_its_tone_once_on_entry() {
        let mut pause = PauseControl::default();
        let mut actions = Vec::new();
        for mask in [128, 128, 0] {
            pause.poll_with_sound(mask, &mut |a| actions.push(a));
        }
        pause.tick();
        pause.tick();
        pause.poll_with_sound(16, &mut |a| actions.push(a));
        pause.tick();
        pause.tick();
        pause.poll_with_sound(0, &mut |a| actions.push(a));
        assert_eq!(
            actions,
            [sound::GameplayAction::Request {
                site: 0x2699,
                effect: Effect::Tone(Tone::First)
            }]
        );
    }
}
