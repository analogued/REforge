// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Host audio preferences; native sound execution remains engine-owned.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SoundRenderer {
    #[default]
    GameDefault,
    Speaker,
    EnhancedPcm,
}
impl SoundRenderer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GameDefault => "default",
            Self::Speaker => "speaker",
            Self::EnhancedPcm => "enhanced-pcm",
        }
    }
}
impl std::str::FromStr for SoundRenderer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "default" => Ok(Self::GameDefault),
            "speaker" => Ok(Self::Speaker),
            "enhanced-pcm" => Ok(Self::EnhancedPcm),
            _ => Err("sound renderer must be default, speaker or enhanced-pcm".into()),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSettings {
    pub enabled: bool,
    pub volume: u8,
    pub renderer: SoundRenderer,
    /// None follows the system default; physical SDL IDs are never persisted.
    pub device: Option<String>,
    pub mute_unfocused: bool,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 100,
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
