// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! SDL gamepad ownership and digital left-stick/D-pad conversion. No game clocks,
//! keyboard emulation, input buffering or movement assistance live here.

use sdl3::{
    event::{Event, WindowEvent},
    gamepad::{Axis, Button, Gamepad},
    joystick::JoystickId,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Actions {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub action: bool,
}
impl Actions {
    /// Four independent directions and the bottom face button; no conflict policy.
    pub fn bits(self) -> u8 {
        u8::from(self.up)
            | (u8::from(self.down) << 1)
            | (u8::from(self.left) << 2)
            | (u8::from(self.right) << 3)
            | (u8::from(self.action) << 4)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PadEvent {
    pub actions: Actions,
    pub pressed: Actions,
    pub pause: bool,
    pub settings: bool,
}

#[derive(Default)]
struct Digital {
    axes: [i16; 2],
    directions: [i8; 2],
    buttons: u8,
    armed: bool,
    focused: bool,
    dead_zone: u8,
}
fn button_bit(button: Button) -> u8 {
    match button {
        Button::DPadUp => 1,
        Button::DPadDown => 2,
        Button::DPadLeft => 4,
        Button::DPadRight => 8,
        Button::South => 16,
        Button::Start => 32,
        Button::Back => 64,
        _ => 0,
    }
}
impl Digital {
    fn new(dead_zone: u8) -> Self {
        Self {
            focused: true,
            dead_zone: dead_zone.clamp(5, 90),
            ..Self::default()
        }
    }
    fn axis_direction(&self, value: i16, previous: i8) -> i8 {
        let value = i32::from(value);
        let engage = i32::from(self.dead_zone) * 32767 / 100;
        // At the default 35%, release at 25%. Integer arithmetic handles -32768.
        let release = engage * 5 / 7;
        if value <= -engage {
            -1
        } else if value >= engage {
            1
        } else if previous < 0 && value < -release {
            -1
        } else if previous > 0 && value > release {
            1
        } else {
            0
        }
    }
    fn neutral(&self) -> bool {
        self.buttons == 0 && self.axes.iter().all(|&v| self.axis_direction(v, 0) == 0)
    }
    fn reset(&mut self) {
        self.directions = [0; 2];
        self.armed = self.focused && self.neutral();
    }
    fn actions(&self) -> Actions {
        if !self.focused || !self.armed {
            return Actions::default();
        }
        Actions {
            up: self.directions[1] < 0 || self.buttons & 1 != 0,
            down: self.directions[1] > 0 || self.buttons & 2 != 0,
            left: self.directions[0] < 0 || self.buttons & 4 != 0,
            right: self.directions[0] > 0 || self.buttons & 8 != 0,
            action: self.buttons & 16 != 0,
        }
    }
    fn update(&mut self, axis: Option<(Axis, i16)>, button: Option<(Button, bool)>) -> PadEvent {
        let before = self.actions();
        let old_buttons = self.buttons;
        if let Some((axis, value)) = axis {
            let index = match axis {
                Axis::LeftX => Some(0),
                Axis::LeftY => Some(1),
                _ => None,
            };
            if let Some(index) = index {
                self.axes[index] = value;
                self.directions[index] = self.axis_direction(value, self.directions[index]);
            }
        }
        if let Some((button, pressed)) = button {
            let bit = button_bit(button);
            if pressed {
                self.buttons |= bit;
            } else {
                self.buttons &= !bit;
            }
        }
        if !self.armed && self.focused && self.neutral() {
            self.armed = true;
            self.directions = [0; 2];
        }
        let actions = self.actions();
        let pressed = actions.bits() & !before.bits();
        let buttons = self.buttons & !old_buttons;
        PadEvent {
            actions,
            pressed: Actions {
                up: pressed & 1 != 0,
                down: pressed & 2 != 0,
                left: pressed & 4 != 0,
                right: pressed & 8 != 0,
                action: pressed & 16 != 0,
            },
            pause: self.focused && buttons & 32 != 0,
            settings: self.focused && buttons & 64 != 0,
        }
    }
}

/// One active SDL-recognized gamepad. Device errors leave keyboard play available.
pub struct Gamepads {
    pad: Option<(JoystickId, Gamepad)>,
    subsystem: Option<sdl3::GamepadSubsystem>,
    digital: Digital,
    epoch: u64,
}
impl Gamepads {
    pub fn new(dead_zone: u8) -> Self {
        let subsystem = sdl3::init().and_then(|sdl| sdl.gamepad());
        let subsystem = match subsystem {
            Ok(subsystem) => Some(subsystem),
            Err(error) => {
                eprintln!("REforge: gamepad input unavailable: {error}");
                None
            }
        };
        let mut result = Self {
            pad: None,
            subsystem,
            digital: Digital::new(dead_zone),
            epoch: 0,
        };
        result.open_first();
        result
    }
    fn open_first(&mut self) {
        if self.pad.is_some() {
            return;
        }
        let Some(subsystem) = &self.subsystem else {
            return;
        };
        match subsystem.gamepads() {
            Ok(ids) => {
                for id in ids {
                    match subsystem.open(id) {
                        Ok(pad) => {
                            eprintln!(
                                "REforge: controller connected: {}",
                                pad.name().unwrap_or_else(|| "unnamed gamepad".into())
                            );
                            self.pad = Some((id, pad));
                            self.reset();
                            break;
                        }
                        Err(error) => eprintln!("REforge: cannot open controller {id:?}: {error}"),
                    }
                }
            }
            Err(error) => eprintln!("REforge: cannot enumerate controllers: {error}"),
        }
    }
    pub fn actions(&self) -> Actions {
        self.digital.actions()
    }
    /// Discard old input across host boundaries; held controls need neutral first.
    /// Host pause/settings buttons retain their own release-to-press edge state.
    pub fn reset(&mut self) {
        if let Some((_, pad)) = &self.pad {
            self.digital.axes = [pad.axis(Axis::LeftX), pad.axis(Axis::LeftY)];
            self.digital.buttons = [
                Button::DPadUp,
                Button::DPadDown,
                Button::DPadLeft,
                Button::DPadRight,
                Button::South,
                Button::Start,
                Button::Back,
            ]
            .into_iter()
            .filter(|&b| pad.button(b))
            .fold(0, |bits, b| bits | button_bit(b));
        } else {
            self.digital.axes = [0; 2];
            self.digital.buttons = 0;
        }
        self.digital.reset();
        self.epoch = crate::event_clock_sample().sdl_ns;
    }
    pub fn set_dead_zone(&mut self, percent: u8) {
        let percent = percent.clamp(5, 90);
        if self.digital.dead_zone != percent {
            self.digital.dead_zone = percent;
            self.reset();
        }
    }
    /// Call in SDL queue order. Use the source event's timestamp for engine history.
    pub fn handle(&mut self, event: &Event) -> Option<PadEvent> {
        match *event {
            Event::GamepadAdded { .. } => {
                self.open_first();
                None
            }
            Event::GamepadRemoved { which, .. }
                if self.pad.as_ref().is_some_and(|(id, _)| *id == which) =>
            {
                self.pad = None;
                self.reset();
                self.open_first();
                Some(PadEvent::default())
            }
            Event::GamepadRemapped { which, .. }
                if self.pad.as_ref().is_some_and(|(id, _)| *id == which) =>
            {
                self.reset();
                Some(PadEvent::default())
            }
            Event::Window {
                win_event: WindowEvent::FocusLost | WindowEvent::FocusGained,
                ..
            } => {
                self.digital.focused = matches!(
                    event,
                    Event::Window {
                        win_event: WindowEvent::FocusGained,
                        ..
                    }
                );
                self.reset();
                Some(PadEvent::default())
            }
            _ if event.get_timestamp() != 0 && event.get_timestamp() < self.epoch => None,
            Event::GamepadAxisMotion {
                which, axis, value, ..
            } if matches!(axis, Axis::LeftX | Axis::LeftY)
                && self.pad.as_ref().is_some_and(|(id, _)| *id == which) =>
            {
                Some(self.digital.update(Some((axis, value)), None))
            }
            Event::GamepadButtonDown { which, button, .. }
            | Event::GamepadButtonUp { which, button, .. }
                if button_bit(button) != 0
                    && self.pad.as_ref().is_some_and(|(id, _)| *id == which) =>
            {
                Some(self.digital.update(
                    None,
                    Some((button, matches!(event, Event::GamepadButtonDown { .. }))),
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdl_virtual_controller_delivers_events_resets_and_disconnects() {
        const NAME: &str =
            "gamepad::tests::sdl_virtual_controller_delivers_events_resets_and_disconnects";
        if std::env::var("REFORGE_GAMEPAD_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_GAMEPAD_TEST", NAME)
                .env("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use sdl3::joystick::{JoystickType, VirtualJoystickDescription};
        let sdl = sdl3::init().unwrap();
        let joysticks = sdl.joystick().unwrap();
        let mut events = sdl.event_pump().unwrap();
        let mut pads = Gamepads::new(35);
        let description = || {
            VirtualJoystickDescription::new()
                .name("REforge authored virtual pad")
                .joystick_type(JoystickType::Gamepad)
                .with_axes([Axis::LeftX, Axis::LeftY])
                .with_buttons([
                    Button::South,
                    Button::East,
                    Button::West,
                    Button::North,
                    Button::Back,
                    Button::Guide,
                    Button::Start,
                    Button::LeftStick,
                    Button::RightStick,
                    Button::LeftShoulder,
                    Button::RightShoulder,
                    Button::DPadUp,
                    Button::DPadDown,
                    Button::DPadLeft,
                    Button::DPadRight,
                ])
        };
        let connection = joysticks.attach_virtual_joystick(description()).unwrap();
        let id = connection.id();
        let device = joysticks.open(id).unwrap();
        // Explicitly select only our virtual device, even on a developer desktop
        // with physical controllers. Never change their input or configuration.
        pads.pad = None;
        for event in events.poll_iter() {
            pads.handle(&event);
        }
        pads.pad = Some((id, pads.subsystem.as_ref().unwrap().open(id).unwrap()));
        pads.reset();
        device.set_virtual_axis(Axis::LeftX as u32, 20000).unwrap();
        device.set_virtual_axis(Axis::LeftY as u32, -20000).unwrap();
        let mut received = Vec::new();
        for event in events.poll_iter() {
            if let Some(pad) = pads.handle(&event) {
                received.push((event.get_timestamp(), pad));
            }
        }
        assert_eq!(pads.actions().bits(), 9);
        assert!(received.iter().all(|(stamp, _)| *stamp > 0));
        pads.reset();
        assert_eq!(pads.actions().bits(), 0);
        device.set_virtual_axis(Axis::LeftX as u32, 0).unwrap();
        device.set_virtual_axis(Axis::LeftY as u32, 0).unwrap();
        for event in events.poll_iter() {
            pads.handle(&event);
        }
        device
            .set_virtual_button(Button::South as u32, true)
            .unwrap();
        let notifications = events
            .poll_iter()
            .filter_map(|e| pads.handle(&e))
            .collect::<Vec<_>>();
        assert!(notifications.iter().any(|p| p.pressed.action));
        let second = joysticks.attach_virtual_joystick(description()).unwrap();
        let other = joysticks.open(second.id()).unwrap();
        other.set_virtual_axis(Axis::LeftX as u32, -20000).unwrap();
        for event in events.poll_iter() {
            pads.handle(&event);
        }
        assert_eq!(
            pads.actions().bits(),
            16,
            "a second device cannot steer the active one"
        );
        device
            .set_virtual_button(Button::Start as u32, true)
            .unwrap();
        let notifications = events
            .poll_iter()
            .filter_map(|e| pads.handle(&e))
            .collect::<Vec<_>>();
        assert!(notifications.iter().any(|p| p.pause));
        pads.set_dead_zone(60);
        assert_eq!(pads.actions().bits(), 0);
        drop(connection);
        let mut removed = false;
        for event in events.poll_iter() {
            removed |= pads.handle(&event).is_some_and(|p| p.actions.bits() == 0);
        }
        assert!(removed);
        assert!(pads.pad.as_ref().is_none_or(|(active, _)| *active != id));
        assert_eq!(pads.actions().bits(), 0);
    }
    #[test]
    fn axes_use_hysteresis_and_keep_diagonals_without_combination_delay() {
        let mut d = Digital::new(35);
        d.reset();
        assert_eq!(d.update(Some((Axis::LeftX, 11000)), None).actions.bits(), 0);
        assert_eq!(d.update(Some((Axis::LeftX, 12000)), None).actions.bits(), 8);
        assert_eq!(d.update(Some((Axis::LeftX, 9000)), None).actions.bits(), 8);
        assert_eq!(
            d.update(Some((Axis::LeftY, -20000)), None).actions.bits(),
            9
        );
        assert_eq!(d.update(Some((Axis::LeftX, 8000)), None).actions.bits(), 1);
        assert_eq!(
            d.update(Some((Axis::LeftX, i16::MIN)), None).actions.bits(),
            5
        );
        assert_eq!(
            d.update(Some((Axis::LeftX, i16::MAX)), None).actions.bits(),
            9
        );
    }
    #[test]
    fn dpad_and_stick_are_independent_sources_and_action_has_fresh_edges() {
        let mut d = Digital::new(90);
        d.reset();
        d.update(Some((Axis::LeftX, 32767)), None);
        d.update(None, Some((Button::DPadRight, true)));
        assert_eq!(
            d.update(None, Some((Button::DPadRight, false)))
                .actions
                .bits(),
            8
        );
        d.update(None, Some((Button::DPadLeft, true)));
        assert_eq!(d.actions().bits(), 12);
        assert!(d.update(None, Some((Button::South, true))).pressed.action);
        assert!(!d.update(None, Some((Button::South, true))).pressed.action);
        assert!(!d.update(None, Some((Button::South, false))).actions.action);
    }
    #[test]
    fn boundaries_require_neutral_but_host_pause_stays_available() {
        let mut d = Digital::new(35);
        d.reset();
        d.update(Some((Axis::LeftY, -20000)), None);
        d.reset();
        assert_eq!(
            d.update(None, Some((Button::South, true))).actions.bits(),
            0
        );
        assert!(d.update(None, Some((Button::Start, true))).pause);
        assert!(!d.update(None, Some((Button::Start, true))).pause);
        d.update(None, Some((Button::Start, false)));
        d.update(None, Some((Button::South, false)));
        d.update(Some((Axis::LeftY, 0)), None);
        assert!(d.update(None, Some((Button::South, true))).pressed.action);
        d.focused = false;
        d.reset();
        assert!(!d.update(None, Some((Button::Back, true))).settings);
        assert_eq!(d.actions().bits(), 0);
    }
}
