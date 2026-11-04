// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! BTZ-only presentation preferences; independent of the player host.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FileCursorStyle {
    #[default]
    PulsingUnderline,
    BlinkingUnderline,
    SteadyBlock,
}
impl FileCursorStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PulsingUnderline => "pulsing-underline",
            Self::BlinkingUnderline => "blinking-underline",
            Self::SteadyBlock => "steady-block",
        }
    }
}
impl std::str::FromStr for FileCursorStyle {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "pulsing-underline" => Ok(Self::PulsingUnderline),
            "blinking-underline" => Ok(Self::BlinkingUnderline),
            "steady-block" => Ok(Self::SteadyBlock),
            _ => Err(
                "file cursor must be pulsing-underline, blinking-underline or steady-block".into(),
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BtzSettings {
    /// Retain the existing preference name for command input and file dialogs.
    pub file_cursor: FileCursorStyle,
}
