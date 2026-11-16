// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Lossless logical input format shared by diagnostic scripts and recordings.
use crate::assets::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Quit,
    #[cfg(feature = "dev-tools")]
    Mark(String),
    Enter,
    Space,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Backspace,
    Text(String),
    Shortcut(u8),
    Mouse(i32, i32),
    MouseDelta(i32, i32),
    ReleaseButton(u8),
    Click(i32, i32),
    RightClick(i32, i32),
    Release(i32, i32),
    Tick(u32),
    Random(u16),
    ReleaseKey(u8),
    TurnCheck(u8),
    RaceStack(u16),
}

impl Input {
    /// Menu decoration follows navigation or actual mouse activity, separately
    /// from continue prompts (which deliberately ignore mouse movement).
    pub(super) fn menu_input(
        &self,
        previous_mouse: Option<(i32, i32)>,
    ) -> Option<crate::state::ContinueInput> {
        use crate::state::ContinueInput::{Keyboard, Mouse};
        match self {
            Self::Up | Self::Down | Self::Left | Self::Right => Some(Keyboard),
            Self::Click(..) | Self::RightClick(..) => Some(Mouse),
            Self::Mouse(x, y) if previous_mouse != Some((*x, *y)) => Some(Mouse),
            Self::MouseDelta(x, y) if *x != 0 || *y != 0 => Some(Mouse),
            _ => None,
        }
    }
    /// Continue prompts follow intentional controls, never polling or pointer drift.
    pub(super) fn continue_input(&self) -> Option<crate::state::ContinueInput> {
        use crate::state::ContinueInput::{Keyboard, Mouse};
        match self {
            Self::Click(..) | Self::RightClick(..) => Some(Mouse),
            Self::Enter
            | Self::Space
            | Self::Escape
            | Self::Up
            | Self::Down
            | Self::Left
            | Self::Right
            | Self::Backspace => Some(Keyboard),
            Self::Shortcut(key) if *key != 0 => Some(Keyboard),
            Self::Text(text) if !text.is_empty() => Some(Keyboard),
            _ => None,
        }
    }
    pub fn parse(line: &str) -> Result<Self> {
        let fields: Vec<_> = line.split_whitespace().collect();
        Ok(match fields.as_slice() {
            ["quit"] => Input::Quit,
            #[cfg(feature = "dev-tools")]
            ["mark", rest @ ..] => Input::Mark(rest.join(" ")),
            #[cfg(feature = "dev-tools")]
            ["text-hex"] => Input::Text(String::new()),
            #[cfg(feature = "dev-tools")]
            ["text-hex", value] => Input::Text(decode_hex(value)?),
            #[cfg(feature = "dev-tools")]
            ["mark-hex"] => Input::Mark(String::new()),
            #[cfg(feature = "dev-tools")]
            ["mark-hex", value] => Input::Mark(decode_hex(value)?),
            #[cfg(feature = "dev-tools")]
            ["key-code", code] => Input::Shortcut(code.parse::<u8>().map_err(|e| e.to_string())?),
            #[cfg(feature = "dev-tools")]
            ["turn-check", code] => {
                Input::TurnCheck(code.parse::<u8>().map_err(|e| e.to_string())?)
            }
            ["enter"] => Input::Enter,
            ["space"] => Input::Space,
            ["escape"] => Input::Escape,
            ["up"] => Input::Up,
            ["down"] => Input::Down,
            ["left"] => Input::Left,
            ["right"] => Input::Right,
            ["backspace"] => Input::Backspace,
            ["text", rest @ ..] => Input::Text(rest.join(" ")),
            ["key", key] if key.len() == 1 => Input::Shortcut(key.as_bytes()[0]),
            ["race-stack", value] => {
                Input::RaceStack(value.parse::<u16>().map_err(|e| e.to_string())?)
            }
            ["turn-recover"] => Input::TurnCheck(255),
            ["turn-province", province] => {
                Input::TurnCheck(province.parse::<u8>().map_err(|e| e.to_string())?)
            }
            ["turn-finish"] => Input::TurnCheck(254),
            ["turn-enemies"] => Input::TurnCheck(253),
            ["random", bound] => Input::Random(bound.parse::<u16>().map_err(|e| e.to_string())?),
            ["keyup", bits] => Input::ReleaseKey(bits.parse::<u8>().map_err(|e| e.to_string())?),
            ["tick", count] => Input::Tick(count.parse::<u32>().map_err(|e| e.to_string())?),
            ["mouse-delta", x, y] => Input::MouseDelta(
                x.parse::<i32>().map_err(|e| e.to_string())?,
                y.parse::<i32>().map_err(|e| e.to_string())?,
            ),
            ["button-up", mask] => {
                Input::ReleaseButton(mask.parse::<u8>().map_err(|e| e.to_string())?)
            }
            ["mouse", x, y] => Input::Mouse(
                x.parse::<i32>().map_err(|e| e.to_string())?,
                y.parse::<i32>().map_err(|e| e.to_string())?,
            ),
            ["right-click", x, y] => Input::RightClick(
                x.parse::<i32>().map_err(|e| e.to_string())?,
                y.parse::<i32>().map_err(|e| e.to_string())?,
            ),
            ["click", x, y] => Input::Click(
                x.parse::<i32>().map_err(|e| e.to_string())?,
                y.parse::<i32>().map_err(|e| e.to_string())?,
            ),
            ["release", x, y] => Input::Release(
                x.parse::<i32>().map_err(|e| e.to_string())?,
                y.parse::<i32>().map_err(|e| e.to_string())?,
            ),
            _ => return Err(format!("Invalid script command: {line}")),
        })
    }

    #[cfg(feature = "dev-tools")]
    pub fn command(&self) -> String {
        match self {
            Self::Quit => "quit".into(),
            Self::Mark(text) => format!("mark-hex {}", hex(text)),
            Self::Text(text) => format!("text-hex {}", hex(text)),
            Self::Shortcut(key) => format!("key-code {key}"),
            Self::Mouse(x, y) => format!("mouse {x} {y}"),
            Self::MouseDelta(x, y) => format!("mouse-delta {x} {y}"),
            Self::Click(x, y) => format!("click {x} {y}"),
            Self::RightClick(x, y) => format!("right-click {x} {y}"),
            Self::Release(x, y) => format!("release {x} {y}"),
            Self::Tick(n) => format!("tick {n}"),
            Self::Random(n) => format!("random {n}"),
            Self::ReleaseKey(n) => format!("keyup {n}"),
            Self::ReleaseButton(n) => format!("button-up {n}"),
            Self::TurnCheck(n) => format!("turn-check {n}"),
            Self::RaceStack(n) => format!("race-stack {n}"),
            Self::Enter => "enter".into(),
            Self::Space => "space".into(),
            Self::Escape => "escape".into(),
            Self::Up => "up".into(),
            Self::Down => "down".into(),
            Self::Left => "left".into(),
            Self::Right => "right".into(),
            Self::Backspace => "backspace".into(),
        }
    }

    #[cfg(feature = "dev-tools")]
    pub fn is_motion(&self) -> bool {
        matches!(self, Self::Tick(_) | Self::Mouse(..) | Self::MouseDelta(..))
    }
}

#[cfg(test)]
mod continue_tests {
    use super::*;
    use crate::state::ContinueInput::{Keyboard, Mouse};

    #[test]
    fn decoration_navigation_ignores_stationary_polling_and_other_keys() {
        for input in [Input::Up, Input::Down, Input::Left, Input::Right] {
            assert_eq!(input.menu_input(Some((12, 20))), Some(Keyboard));
        }
        for input in [
            Input::Mouse(13, 20),
            Input::MouseDelta(1, 0),
            Input::Click(12, 20),
            Input::RightClick(12, 20),
        ] {
            assert_eq!(input.menu_input(Some((12, 20))), Some(Mouse));
        }
        for input in [
            Input::Mouse(12, 20),
            Input::MouseDelta(0, 0),
            Input::Tick(1),
            Input::Enter,
            Input::Space,
            Input::Shortcut(b's'),
            Input::Release(13, 20),
            Input::ReleaseButton(1),
            Input::ReleaseKey(1),
        ] {
            assert_eq!(input.menu_input(Some((12, 20))), None);
        }
        // Motion changes decoration, but continue wording waits for a click/key.
        assert_eq!(Input::Mouse(13, 20).continue_input(), None);
    }

    #[test]
    fn only_deliberate_controls_change_continue_wording() {
        for input in [Input::Click(10, 20), Input::RightClick(10, 20)] {
            assert_eq!(input.continue_input(), Some(Mouse));
        }
        for input in [
            Input::Enter,
            Input::Space,
            Input::Escape,
            Input::Up,
            Input::Shortcut(b'f'),
            Input::Text("a".into()),
        ] {
            assert_eq!(input.continue_input(), Some(Keyboard));
        }
        for input in [
            Input::Mouse(10, 20),
            Input::MouseDelta(1, 2),
            Input::Tick(3),
            Input::Release(10, 20),
            Input::ReleaseButton(1),
            Input::ReleaseKey(1),
            Input::Random(10),
            Input::Text(String::new()),
            Input::Shortcut(0),
        ] {
            assert_eq!(input.continue_input(), None);
        }
    }
}

#[cfg(feature = "dev-tools")]
fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[cfg(feature = "dev-tools")]
fn decode_hex(value: &str) -> Result<String> {
    if !value.len().is_multiple_of(2) || !value.is_ascii() {
        return Err("Invalid input hex".into());
    }
    let bytes: Result<Vec<u8>> = value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).map_err(|e| e.to_string()))
        .collect();
    String::from_utf8(bytes?).map_err(|e| e.to_string())
}

#[cfg(all(test, feature = "dev-tools"))]
mod tests {
    use super::*;
    #[test]
    fn recorded_input_round_trips_without_text_or_coordinate_loss() {
        let inputs = vec![
            Input::Quit,
            Input::Mark("frame\nwhy?".into()),
            Input::Text("  name\t\nα  ".into()),
            Input::Text(String::new()),
            Input::Shortcut(0),
            Input::Shortcut(255),
            Input::Mouse(-20, 203),
            Input::MouseDelta(-1, 0),
            Input::Click(319, 199),
            Input::RightClick(0, 0),
            Input::Release(25, 42),
            Input::Tick(u32::MAX),
            Input::Random(0),
            Input::ReleaseKey(15),
            Input::ReleaseButton(2),
            Input::TurnCheck(255),
            Input::RaceStack(65535),
            Input::Enter,
            Input::Space,
            Input::Escape,
            Input::Up,
            Input::Down,
            Input::Left,
            Input::Right,
            Input::Backspace,
        ];
        for input in inputs {
            assert_eq!(Input::parse(&input.command()).unwrap(), input);
        }
        for line in ["text-hex f", "text-hex gg", "text-hex ff", "key-code 256"] {
            assert!(Input::parse(line).is_err());
        }
    }
}
