// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine declarations for the launcher's small set of immediate-save options.
//!
//! Storage keys and command-line values remain separate from player-facing
//! labels. Choices can provide static palettes; engine preview frames can attach
//! runtime palette metadata. Presentation uses a separate display copy, retaining
//! the engine's original palette order and colour values for game rendering.

mod palette;
pub use palette::{
    MAX_PALETTE_COLOURS, Palette, PaletteDisplay, PaletteHint, prepare_palette,
    prepare_palette_for_rows,
};

#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
    pub short_label: &'static str,
    /// None omits this choice from the active-options preview badge.
    pub badge: Option<&'static str>,
    pub palette: Option<&'static [[u8; 3]]>,
    pub palette_hint: PaletteHint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Storage {
    /// An existing preference managed by the shared Settings transaction.
    Shared(&'static str),
    /// BTZ's startup frontend switch, retaining its graphical renderer choice.
    TerminalMode,
    /// Engine-specific preference passed through its ordinary argument parser.
    Native {
        key: &'static str,
        argument: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct OptionDefinition {
    pub label: &'static str,
    pub choices: &'static [Choice],
    pub default: &'static str,
    pub storage: Storage,
    pub apply: reforge_settings::ApplyTiming,
}

impl OptionDefinition {
    pub const fn key(&self) -> &'static str {
        match self.storage {
            Storage::Shared(key) | Storage::Native { key, .. } => key,
            Storage::TerminalMode => "renderer",
        }
    }
}

pub const DISPLAY: OptionDefinition = OptionDefinition {
    label: "Display",
    choices: &[
        Choice {
            value: "pixels",
            label: "Sharp pixels",
            short_label: "Sharp pixels",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
        Choice {
            value: "hyllian",
            label: "Hyllian CRT",
            short_label: "Hyllian CRT",
            badge: Some("CRT"),
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
    ],
    default: "pixels",
    storage: Storage::Shared("presentation"),
    apply: reforge_settings::ApplyTiming::Live,
};

pub const WINDOW: OptionDefinition = OptionDefinition {
    label: "Window",
    choices: &[
        Choice {
            value: "windowed",
            label: "Windowed",
            short_label: "Windowed",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
        Choice {
            value: "fullscreen",
            label: "Fullscreen",
            short_label: "Fullscreen",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
    ],
    default: "windowed",
    storage: Storage::Shared("window_mode"),
    apply: reforge_settings::ApplyTiming::Live,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_choices_follow_settings_values() {
        let defaults = reforge_settings::RuntimeSettings::default();
        for option in [DISPLAY, WINDOW] {
            assert_eq!(option.default, defaults.get(option.key()));
            for choice in option.choices {
                let mut values = defaults.clone();
                values.set(option.key(), choice.value).unwrap();
                assert_eq!(values.get(option.key()), choice.value);
            }
        }
    }
}
