// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine declarations for the launcher's small set of immediate-save options.
//!
//! Storage keys and command-line values remain separate from player-facing
//! labels. A palette is optional metadata for Symbolic artwork; producing it
//! does not require opening original files or generating a game preview.

#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
    pub short_label: &'static str,
    /// None omits this choice from the active-options preview badge.
    pub badge: Option<&'static str>,
    pub palette: Option<&'static [[u8; 3]]>,
}

#[derive(Clone, Copy, Debug)]
pub enum Storage {
    /// An existing preference managed by the shared Settings transaction.
    Shared(&'static str),
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
}

impl OptionDefinition {
    pub const fn key(&self) -> &'static str {
        match self.storage {
            Storage::Shared(key) | Storage::Native { key, .. } => key,
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
        },
        Choice {
            value: "hyllian",
            label: "Hyllian CRT",
            short_label: "Hyllian CRT",
            badge: Some("CRT"),
            palette: None,
        },
    ],
    default: "pixels",
    storage: Storage::Shared("presentation"),
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
        },
        Choice {
            value: "fullscreen",
            label: "Fullscreen",
            short_label: "Fullscreen",
            badge: None,
            palette: None,
        },
    ],
    default: "windowed",
    storage: Storage::Shared("window_mode"),
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
