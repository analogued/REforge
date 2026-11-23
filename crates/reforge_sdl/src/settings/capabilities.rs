// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine declarations consumed by both launcher and player settings hosts.
//! Availability of host facilities is supplied separately, never saved here.

use super::VideoCapabilities;
use crate::{
    DisplayGeometry,
    launcher::{OptionDefinition, Storage},
};
use reforge_settings::{ApplyTiming, AudioCapabilities, SoundRenderer};

/// The typed engine option panels available to the shared Settings UI.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EngineOptions {
    #[default]
    None,
    AlleyCat,
    Mach3,
    Centurion,
    Btz,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControllerLayouts {
    #[default]
    None,
    AlleyCat,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlCapabilities {
    pub controller_layouts: ControllerLayouts,
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayCapabilities {
    /// Initial geometry for the selected mode. A live window retains its
    /// current geometry separately when the engine changes video mode.
    pub geometry: DisplayGeometry,
    pub video: VideoCapabilities,
    pub machine: Option<&'static OptionDefinition>,
    pub mode: Option<&'static OptionDefinition>,
    pub frontend: Option<&'static OptionDefinition>,
}

#[derive(Clone, Copy, Debug)]
pub struct SettingsCapabilities {
    /// Fallback scope for a standalone engine host without a launch context.
    pub game_id: &'static str,
    pub audio: AudioCapabilities,
    pub display: DisplayCapabilities,
    pub controls: ControlCapabilities,
    pub options: EngineOptions,
    pub quick_options: &'static [OptionDefinition],
}

/// Current host availability, independent of an engine's declared support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsAvailability {
    pub terminal: bool,
    /// This host executes audio, independently of declared device support.
    /// Silent development previews leave capabilities intact and set this false.
    pub audio_playback: bool,
}
impl Default for SettingsAvailability {
    fn default() -> Self {
        Self {
            terminal: false,
            audio_playback: true,
        }
    }
}

impl SettingsCapabilities {
    pub const fn basic(geometry: DisplayGeometry) -> Self {
        Self {
            game_id: "standalone",
            audio: AudioCapabilities::NONE,
            display: DisplayCapabilities {
                geometry,
                video: VideoCapabilities::DEFAULT,
                machine: None,
                mode: None,
                frontend: None,
            },
            controls: ControlCapabilities {
                controller_layouts: ControllerLayouts::None,
            },
            options: EngineOptions::None,
            quick_options: &[],
        }
    }

    /// Check declarations, not the availability of terminals or audio devices.
    pub fn validate(self) -> Result<(), &'static str> {
        self.audio.validate()?;
        if self.game_id.is_empty() {
            return Err("settings scope must not be empty");
        }
        if self.display.video.min_scale == 0
            || self.display.video.max_scale < self.display.video.min_scale
        {
            return Err("display scale limits must be positive and ordered");
        }
        for (index, option) in self.quick_options.iter().enumerate() {
            validate_option(option)?;
            if self.quick_options[..index]
                .iter()
                .any(|previous| previous.key() == option.key())
            {
                return Err("quick option storage keys must be unique");
            }
            if matches!(option.storage, Storage::Shared("audio_renderer")) {
                if option.default != self.audio.default.as_str() || option.apply != self.audio.apply
                {
                    return Err("audio quick choice must share its capability default and timing");
                }
                if option.choices.len() != self.audio.renderers.len()
                    || option.choices.iter().any(|choice| {
                        match choice.value.parse::<SoundRenderer>() {
                            Ok(renderer) => {
                                renderer == SoundRenderer::GameDefault
                                    || !self.audio.renderers.contains(&renderer)
                            }
                            Err(_) => true,
                        }
                    })
                {
                    return Err("audio quick choices must match supported renderers");
                }
            }
        }
        for option in [
            self.display.machine,
            self.display.mode,
            self.display.frontend,
        ]
        .into_iter()
        .flatten()
        {
            validate_option(option)?;
            if !self.quick_options.iter().any(|quick| {
                quick.storage == option.storage
                    && quick.default == option.default
                    && quick.apply == option.apply
                    && quick.choices.len() == option.choices.len()
                    && quick
                        .choices
                        .iter()
                        .zip(option.choices)
                        .all(|(a, b)| a.value == b.value)
            }) {
                return Err("display choices must reference matching quick options");
            }
        }
        if let Some(frontend) = self.display.frontend
            && (!matches!(frontend.storage, Storage::TerminalMode)
                || frontend.apply != ApplyTiming::NextLaunch)
        {
            return Err("frontend selection must use next-launch terminal storage");
        }
        Ok(())
    }
}

fn validate_option(option: &OptionDefinition) -> Result<(), &'static str> {
    if option.label.is_empty() || option.key().is_empty() {
        return Err("option label and storage key must not be empty");
    }
    if option.choices.is_empty()
        || !option
            .choices
            .iter()
            .any(|choice| choice.value == option.default)
    {
        return Err("option default must be a supported choice");
    }
    for (index, choice) in option.choices.iter().enumerate() {
        if choice.value.is_empty()
            || option.choices[..index]
                .iter()
                .any(|previous| previous.value == choice.value)
        {
            return Err("option choice values must be nonempty and unique");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::{Choice, PaletteHint};

    const CHOICES: &[Choice] = &[
        Choice {
            value: "speaker",
            label: "PC speaker",
            short_label: "PC speaker",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
        Choice {
            value: "adlib",
            label: "AdLib",
            short_label: "AdLib",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
    ];
    const AUDIO: AudioCapabilities = AudioCapabilities {
        renderers: &[SoundRenderer::Speaker, SoundRenderer::Adlib],
        default: SoundRenderer::Adlib,
        categories: &[],
        apply: ApplyTiming::NextLaunch,
    };
    const QUICK: &[OptionDefinition] = &[OptionDefinition {
        label: "Sound",
        choices: CHOICES,
        default: "adlib",
        storage: Storage::Shared("audio_renderer"),
        apply: ApplyTiming::NextLaunch,
    }];
    const CAPABILITIES: SettingsCapabilities = SettingsCapabilities {
        audio: AUDIO,
        quick_options: QUICK,
        ..SettingsCapabilities::basic(DisplayGeometry::new((320, 200), (320, 240)))
    };

    #[test]
    fn shared_audio_declarations_keep_explicit_defaults_and_application_policy() {
        assert_eq!(CAPABILITIES.validate(), Ok(()));
        assert_eq!(
            CAPABILITIES.audio.resolve(SoundRenderer::GameDefault),
            SoundRenderer::Adlib
        );
        assert!(
            SettingsCapabilities {
                audio: AudioCapabilities {
                    default: SoundRenderer::Speaker,
                    ..AUDIO
                },
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
        assert!(
            SettingsCapabilities {
                audio: AudioCapabilities {
                    apply: ApplyTiming::Live,
                    ..AUDIO
                },
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn absent_support_and_host_availability_are_separate() {
        let basic = SettingsCapabilities::basic(DisplayGeometry::new((320, 200), (320, 240)));
        assert_eq!(basic.validate(), Ok(()));
        for terminal in [false, true] {
            let availability = SettingsAvailability {
                terminal,
                ..SettingsAvailability::default()
            };
            assert_eq!(availability.terminal, terminal);
            assert!(basic.display.frontend.is_none());
            assert_eq!(CAPABILITIES.validate(), Ok(()));
        }
    }

    #[test]
    fn invalid_display_limits_and_mismatched_option_metadata_are_rejected() {
        let video = VideoCapabilities {
            min_scale: 3,
            max_scale: 2,
            ..VideoCapabilities::DEFAULT
        };
        assert!(
            SettingsCapabilities {
                display: DisplayCapabilities {
                    video,
                    ..CAPABILITIES.display
                },
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
        assert!(
            SettingsCapabilities {
                display: DisplayCapabilities {
                    mode: Some(&QUICK[0]),
                    ..CAPABILITIES.display
                },
                quick_options: &[],
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
        assert!(
            SettingsCapabilities {
                display: DisplayCapabilities {
                    frontend: Some(&QUICK[0]),
                    ..CAPABILITIES.display
                },
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
        const FULL_MODE: OptionDefinition = OptionDefinition {
            label: "Video",
            choices: CHOICES,
            default: "adlib",
            storage: Storage::Native {
                key: "video",
                argument: "--video",
            },
            apply: ApplyTiming::NextLaunch,
        };
        const LIMITED_MODE: &[OptionDefinition] = &[OptionDefinition {
            choices: &[CHOICES[1]],
            ..FULL_MODE
        }];
        assert!(
            SettingsCapabilities {
                display: DisplayCapabilities {
                    mode: Some(&FULL_MODE),
                    ..CAPABILITIES.display
                },
                quick_options: LIMITED_MODE,
                ..CAPABILITIES
            }
            .validate()
            .is_err()
        );
    }
}
