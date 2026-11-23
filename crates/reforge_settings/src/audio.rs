// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host audio preferences; native sound execution remains engine-owned.
pub use reforge_audio::category::{CategoryVolumes, SoundCategory};

/// When an engine can apply a selected option, independently of its storage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ApplyTiming {
    #[default]
    Live,
    NextLaunch,
}

/// Engine-owned audio choices shared by Settings, launchers and playback.
/// Defaults are explicit and do not depend on presentation order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioCapabilities {
    pub renderers: &'static [SoundRenderer],
    pub default: SoundRenderer,
    pub categories: &'static [SoundCategory],
    pub apply: ApplyTiming,
}

impl AudioCapabilities {
    pub const NONE: Self = Self {
        renderers: &[],
        default: SoundRenderer::GameDefault,
        categories: &[],
        apply: ApplyTiming::Live,
    };

    /// Resolve aliases and unsupported inherited choices without editing them.
    pub fn resolve(self, selection: SoundRenderer) -> SoundRenderer {
        if selection != SoundRenderer::GameDefault && self.renderers.contains(&selection) {
            selection
        } else {
            self.default
        }
    }

    pub fn validate(self) -> Result<(), &'static str> {
        if self.renderers.is_empty() {
            return if self.default == SoundRenderer::GameDefault && self.categories.is_empty() {
                Ok(())
            } else {
                Err("unavailable audio cannot declare a renderer or categories")
            };
        }
        if self.default == SoundRenderer::GameDefault || !self.renderers.contains(&self.default) {
            return Err("audio default must be a supported concrete renderer");
        }
        for (index, renderer) in self.renderers.iter().enumerate() {
            if *renderer == SoundRenderer::GameDefault {
                return Err("audio choices must not contain the default alias");
            }
            if self.renderers[..index].contains(renderer) {
                return Err("audio choices must be unique");
            }
        }
        for (index, category) in self.categories.iter().enumerate() {
            if self.categories[..index].contains(category) {
                return Err("audio categories must be unique");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SoundRenderer {
    #[default]
    GameDefault,
    Adlib,
    Speaker,
    EnhancedPcm,
}
impl SoundRenderer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GameDefault => "default",
            Self::Adlib => "adlib",
            Self::Speaker => "speaker",
            Self::EnhancedPcm => "enhanced-pcm",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::GameDefault => "Game default",
            Self::Adlib => "AdLib",
            Self::Speaker => "PC speaker",
            Self::EnhancedPcm => "Enhanced PCM",
        }
    }
}

impl std::str::FromStr for SoundRenderer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "default" => Ok(Self::GameDefault),
            "adlib" => Ok(Self::Adlib),
            "speaker" => Ok(Self::Speaker),
            "enhanced-pcm" => Ok(Self::EnhancedPcm),
            _ => Err("sound renderer must be default, adlib, speaker or enhanced-pcm".into()),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSettings {
    pub enabled: bool,
    pub volume: u8,
    pub categories: CategoryVolumes,
    pub renderer: SoundRenderer,
    /// None follows the system default; physical SDL IDs are never persisted.
    pub device: Option<String>,
    pub mute_unfocused: bool,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 80,
            categories: CategoryVolumes::default(),
            renderer: SoundRenderer::GameDefault,
            device: None,
            mute_unfocused: true,
        }
    }
}
impl AudioSettings {
    pub fn gain(&self, focused: bool) -> f32 {
        if !self.enabled || self.mute_unfocused && !focused {
            0.0
        } else {
            f32::from(self.volume) / 100.0
        }
    }
    pub(super) fn get(&self, key: &str) -> String {
        if let Some(category) = SoundCategory::ALL.into_iter().find(|c| c.key() == key) {
            return self.categories.get(category).to_string();
        }
        match key {
            "sound_enabled" => self.enabled.to_string(),
            "master_volume" => self.volume.to_string(),
            "audio_renderer" => self.renderer.as_str().into(),
            "output_device" => self.device.clone().unwrap_or("default".into()),
            "mute_unfocused" => self.mute_unfocused.to_string(),
            _ => String::new(),
        }
    }
    pub(super) fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if let Some(category) = SoundCategory::ALL.into_iter().find(|c| c.key() == key) {
            self.categories.0[category.index()] = value
                .parse::<u8>()
                .ok()
                .filter(|v| *v <= 100)
                .ok_or("category volume must be 0..100")?;
            return Ok(());
        }
        fn boolean(s: &str) -> Result<bool, String> {
            match s {
                "on" | "true" => Ok(true),
                "off" | "false" => Ok(false),
                _ => Err("expected on/off or true/false".into()),
            }
        }
        match key {
            "sound_enabled" => self.enabled = boolean(value)?,
            "master_volume" => {
                self.volume = value
                    .parse::<u8>()
                    .ok()
                    .filter(|v| *v <= 100)
                    .ok_or("master volume must be 0..100")?
            }
            "audio_renderer" => self.renderer = value.parse()?,
            "output_device" => {
                self.device = if value == "default" {
                    None
                } else if value.is_empty() || value.contains('\0') {
                    return Err("output device must be default or a device name".into());
                } else {
                    Some(value.into())
                }
            }
            "mute_unfocused" => self.mute_unfocused = boolean(value)?,
            _ => return Err("unknown audio setting".into()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    const AUDIO: AudioCapabilities = AudioCapabilities {
        renderers: &[SoundRenderer::Speaker, SoundRenderer::Adlib],
        default: SoundRenderer::Adlib,
        categories: &[SoundCategory::Music, SoundCategory::Effects],
        apply: ApplyTiming::NextLaunch,
    };

    #[test]
    fn explicit_default_is_independent_of_choice_order_and_keeps_aliases_unmodified() {
        assert_eq!(AUDIO.validate(), Ok(()));
        for selection in [SoundRenderer::GameDefault, SoundRenderer::EnhancedPcm] {
            let saved = selection;
            assert_eq!(AUDIO.resolve(selection), SoundRenderer::Adlib);
            assert_eq!(selection, saved);
        }
        for selection in AUDIO.renderers {
            assert_eq!(AUDIO.resolve(*selection), *selection);
        }
        let reversed = AudioCapabilities {
            renderers: &[SoundRenderer::Adlib, SoundRenderer::Speaker],
            ..AUDIO
        };
        assert_eq!(reversed.resolve(SoundRenderer::GameDefault), AUDIO.default);
        assert_eq!(AudioCapabilities::NONE.validate(), Ok(()));
        assert_eq!(
            AudioCapabilities::NONE.resolve(SoundRenderer::Adlib),
            SoundRenderer::GameDefault
        );
    }

    #[test]
    fn inconsistent_audio_capabilities_are_rejected() {
        for invalid in [
            AudioCapabilities {
                default: SoundRenderer::GameDefault,
                ..AUDIO
            },
            AudioCapabilities {
                default: SoundRenderer::EnhancedPcm,
                ..AUDIO
            },
            AudioCapabilities {
                renderers: &[SoundRenderer::Adlib, SoundRenderer::Adlib],
                ..AUDIO
            },
            AudioCapabilities {
                renderers: &[SoundRenderer::GameDefault, SoundRenderer::Adlib],
                ..AUDIO
            },
            AudioCapabilities {
                categories: &[SoundCategory::Music, SoundCategory::Music],
                ..AUDIO
            },
            AudioCapabilities {
                renderers: &[],
                ..AUDIO
            },
            AudioCapabilities {
                categories: &[SoundCategory::Effects],
                ..AudioCapabilities::NONE
            },
        ] {
            assert!(invalid.validate().is_err(), "{invalid:?}");
        }
    }
}
