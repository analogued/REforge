// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent preferences and temporary launch overrides, independent of SDL.
pub mod launcher;
pub mod library;

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item};
mod audio;
pub use audio::{
    ApplyTiming, AudioCapabilities, AudioSettings, CategoryVolumes, SoundCategory, SoundRenderer,
};
mod controller;
pub use controller::{ControllerButtons, ControllerMovement, FaceButton};
mod centurion;
pub use centurion::{
    AppliedValueIndicator, CenturionOptions, FleetInformation, MapHighlight, MapHoverInfo,
    MenuDecorations, PopupCursor,
};
mod btz;
pub use btz::{BtzSettings, FileCursorStyle};

pub const DISPLAY_KEYS: &[&str] = &[
    "window_mode",
    "scale",
    "aspect_ratio",
    "scaling_policy",
    "sampling",
    "renderer",
    "backend",
    "vsync",
    "frame_limit",
];
pub const CRT_KEYS: &[&str] = &[
    "presentation",
    "horizontal_filter",
    "scanline_strength",
    "beam_min",
    "beam_max",
    "colour_boost",
    "sharpness",
    "mask_intensity",
    "anti_ringing",
    "input_gamma",
    "output_gamma",
];
pub const AUDIO_KEYS: &[&str] = &[
    "sound_enabled",
    "master_volume",
    "music_volume",
    "speech_volume",
    "effects_volume",
    "audio_renderer",
    "output_device",
    "mute_unfocused",
];
pub const STICK_DEAD_ZONE: &str = "stick_dead_zone";
pub const CONTROL_KEYS: &[&str] = &[STICK_DEAD_ZONE];
/// UI section containing game-specific enhancements.
pub const ENHANCEMENTS_SECTION: usize = 4;
pub const SECTION_KEYS: [&[&str]; 5] = [DISPLAY_KEYS, CRT_KEYS, AUDIO_KEYS, CONTROL_KEYS, &[]];
pub const DIGITIZED_SAMPLES: &str = "digitized_samples";
pub const MOTION: &str = "motion";
/// Internal key separating Alley Cat from Mach 3 in the flat runtime patch.
/// Both games persist their own value as `enhancements.motion`.
pub const ALLEYCAT_MOTION: &str = "alleycat_motion";
pub const BTZ_FILE_CURSOR: &str = "file_cursor";
/// Internal key for Centurion; stored as `enhancements.fleet_information`.
pub const CENTURION_FLEET_INFORMATION: &str = "centurion_fleet_information";
/// Internal key for Centurion; stored as `enhancements.popup_cursor`.
pub const CENTURION_POPUP_CURSOR: &str = "centurion_popup_cursor";
/// Internal key for Centurion; stored as `enhancements.menu_decorations`.
pub const CENTURION_MENU_DECORATIONS: &str = "centurion_menu_decorations";
/// Internal key for Centurion; stored as `enhancements.map_highlight`.
pub const CENTURION_MAP_HIGHLIGHT: &str = "centurion_map_highlight";
/// Internal key for Centurion; stored as `enhancements.map_hover_info`.
pub const CENTURION_MAP_HOVER_INFO: &str = "centurion_map_hover_info";
/// Internal key for Centurion; stored as `enhancements.applied_value_indicator`.
pub const CENTURION_APPLIED_VALUE_INDICATOR: &str = "centurion_applied_value_indicator";
pub const CONTROL_MODE: &str = "control_mode";
/// Alley Cat startup machine, shared with the launcher's native option store.
pub const ALLEYCAT_MACHINE: &str = "machine";
pub const CONTROLLER_MOVEMENT: &str = "controller_movement";
pub const CONTROLLER_ACTION: &str = "controller_action";
pub const CONTROLLER_BUTTONS: &str = "controller_buttons";
const ALLEYCAT_CONTROL_KEYS: &[&str] = &[
    CONTROL_MODE,
    CONTROLLER_MOVEMENT,
    CONTROLLER_ACTION,
    CONTROLLER_BUTTONS,
];
// Dormant per-entry renderer while Terminal is selected. Absence uses Auto.
const GRAPHICAL_RENDERER: &str = "graphical_renderer";
/// Game identities with an attached-terminal player host.
pub fn supports_terminal(game: &str) -> bool {
    matches!(game, "mindwheel" | "essex" | "brimstone" | "breakers")
}

fn game_keys(game: &str) -> impl Iterator<Item = &'static str> {
    let keys: &[&str] = match game {
        "mach-3" => &[DIGITIZED_SAMPLES, MOTION],
        "centurion-defender-of-rome" => &[
            CENTURION_FLEET_INFORMATION,
            CENTURION_POPUP_CURSOR,
            CENTURION_MENU_DECORATIONS,
            CENTURION_MAP_HIGHLIGHT,
            CENTURION_MAP_HOVER_INFO,
            CENTURION_APPLIED_VALUE_INDICATOR,
        ],
        "alley-cat" => &[
            ALLEYCAT_MACHINE,
            ALLEYCAT_MOTION,
            CONTROL_MODE,
            CONTROLLER_MOVEMENT,
            CONTROLLER_ACTION,
            CONTROLLER_BUTTONS,
        ],
        "mindwheel" | "essex" | "brimstone" | "breakers" => &[GRAPHICAL_RENDERER, BTZ_FILE_CURSOR],
        _ => &[],
    };
    keys.iter().copied()
}
pub fn section_index(key: &str) -> usize {
    if matches!(
        key,
        MOTION
            | ALLEYCAT_MOTION
            | CONTROL_MODE
            | CENTURION_FLEET_INFORMATION
            | CENTURION_POPUP_CURSOR
            | CENTURION_MENU_DECORATIONS
            | CENTURION_MAP_HIGHLIGHT
            | CENTURION_MAP_HOVER_INFO
            | CENTURION_APPLIED_VALUE_INDICATOR
    ) {
        return ENHANCEMENTS_SECTION;
    }
    if key == DIGITIZED_SAMPLES {
        return 2;
    }
    if ALLEYCAT_CONTROL_KEYS.contains(&key) {
        return 3;
    }
    SECTION_KEYS
        .iter()
        .position(|keys| keys.contains(&key))
        .unwrap_or(0)
}
pub fn keys() -> impl Iterator<Item = &'static str> {
    SECTION_KEYS.into_iter().flatten().copied()
}

fn stored_key(key: &str) -> &str {
    match key {
        ALLEYCAT_MOTION => MOTION,
        CENTURION_FLEET_INFORMATION => "fleet_information",
        CENTURION_POPUP_CURSOR => "popup_cursor",
        CENTURION_MENU_DECORATIONS => "menu_decorations",
        CENTURION_MAP_HIGHLIGHT => "map_highlight",
        CENTURION_MAP_HOVER_INFO => "map_hover_info",
        CENTURION_APPLIED_VALUE_INDICATOR => "applied_value_indicator",
        _ => key,
    }
}

fn section(key: &str) -> &'static str {
    if matches!(
        key,
        MOTION
            | ALLEYCAT_MOTION
            | CONTROL_MODE
            | CENTURION_FLEET_INFORMATION
            | CENTURION_POPUP_CURSOR
            | CENTURION_MENU_DECORATIONS
            | CENTURION_MAP_HIGHLIGHT
            | CENTURION_MAP_HOVER_INFO
            | CENTURION_APPLIED_VALUE_INDICATOR
    ) {
        "enhancements"
    } else if key == ALLEYCAT_MACHINE {
        "launcher"
    } else if key == DIGITIZED_SAMPLES {
        "gameplay"
    } else if ALLEYCAT_CONTROL_KEYS.contains(&key) || CONTROL_KEYS.contains(&key) {
        "controls"
    } else if CRT_KEYS.contains(&key) {
        "crt"
    } else if AUDIO_KEYS.contains(&key) {
        "audio"
    } else {
        "display"
    }
}
/// Image fitting inside the available client area, independent of window size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScalingPolicy {
    #[default]
    Integer,
    Fit,
    Stretch,
}
impl ScalingPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Fit => "fit",
            Self::Stretch => "stretch",
        }
    }
}
impl std::str::FromStr for ScalingPolicy {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "integer" => Ok(Self::Integer),
            "fit" => Ok(Self::Fit),
            "stretch" => Ok(Self::Stretch),
            _ => Err("scaling policy must be integer, fit or stretch".into()),
        }
    }
}

/// Texture reconstruction for ordinary pixels. CRT treatments use their own filter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Sampling {
    #[default]
    Nearest,
    Linear,
}
impl Sampling {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Linear => "linear",
        }
    }
}
impl std::str::FromStr for Sampling {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "nearest" => Ok(Self::Nearest),
            "linear" => Ok(Self::Linear),
            _ => Err("sampling must be nearest or linear".into()),
        }
    }
}

/// Semantic preference; engines supply the numeric display/pixel ratios.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AspectRatio {
    #[default]
    Original,
    Square,
}
impl AspectRatio {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Square => "square",
        }
    }
}
impl std::str::FromStr for AspectRatio {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "original" => Ok(Self::Original),
            "square" => Ok(Self::Square),
            _ => Err("aspect ratio must be original or square".into()),
        }
    }
}

/// Shared host presentation, independent of game palettes and framebuffer storage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Presentation {
    #[default]
    Pixels,
    Hyllian,
}
impl Presentation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pixels => "pixels",
            Self::Hyllian => "hyllian",
        }
    }
}
impl std::str::FromStr for Presentation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pixels" => Ok(Self::Pixels),
            "hyllian" => Ok(Self::Hyllian),
            _ => Err("presentation must be pixels or hyllian".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FrameLimit {
    #[default]
    Display,
    Fps60,
    Fps120,
    Unlimited,
}
impl FrameLimit {
    pub const ALL: [Self; 4] = [Self::Display, Self::Fps60, Self::Fps120, Self::Unlimited];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Fps60 => "60",
            Self::Fps120 => "120",
            Self::Unlimited => "unlimited",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Display => "Display refresh rate",
            Self::Fps60 => "60 fps",
            Self::Fps120 => "120 fps",
            Self::Unlimited => "Unlimited",
        }
    }
}
impl std::str::FromStr for FrameLimit {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| "frame limit must be display, 60, 120 or unlimited".into())
    }
}
/// Parameters use hundredths for stable equality, sparse patches and serialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HyllianSettings {
    pub values: [u16; 9],
    pub catmull_rom: bool,
}
impl Default for HyllianSettings {
    fn default() -> Self {
        Self {
            values: [65, 70, 100, 200, 100, 35, 100, 240, 240],
            catmull_rom: false,
        }
    }
}
#[derive(Clone, Copy)]
pub struct ShaderParameter {
    pub key: &'static str,
    pub label: &'static str,
    pub min: u16,
    pub max: u16,
    pub help: &'static str,
}
pub const SHADER_PARAMETERS: [ShaderParameter; 9] = [
    ShaderParameter {
        key: "scanline_strength",
        label: "Scanline strength",
        min: 0,
        max: 100,
        help: "Strength of the Gaussian beam falloff. Scanlines follow the native 200 source rows.",
    },
    ShaderParameter {
        key: "beam_min",
        label: "Minimum beam width",
        min: 10,
        max: 200,
        help: "Beam width in dark areas. Must not exceed maximum beam width.",
    },
    ShaderParameter {
        key: "beam_max",
        label: "Maximum beam width",
        min: 10,
        max: 200,
        help: "Beam width in bright areas. Must not be below minimum beam width.",
    },
    ShaderParameter {
        key: "colour_boost",
        label: "Colour boost",
        min: 100,
        max: 300,
        help: "Compensates for the brightness lost between scanlines.",
    },
    ShaderParameter {
        key: "sharpness",
        label: "Horizontal sharpness",
        min: 100,
        max: 500,
        help: "Horizontal sampling sharpness; 1.00 retains the native source width.",
    },
    ShaderParameter {
        key: "mask_intensity",
        label: "Mask intensity",
        min: 0,
        max: 100,
        help: "Intensity of the RGB aperture mask, anchored to physical output pixels.",
    },
    ShaderParameter {
        key: "anti_ringing",
        label: "Anti-ringing",
        min: 0,
        max: 100,
        help: "Suppresses overshoot from horizontal filtering.",
    },
    ShaderParameter {
        key: "input_gamma",
        label: "Input gamma",
        min: 100,
        max: 400,
        help: "Gamma used before beam and filter calculations.",
    },
    ShaderParameter {
        key: "output_gamma",
        label: "Output gamma",
        min: 100,
        max: 400,
        help: "Gamma used to encode the final output; this is not a monitor colour profile.",
    },
];
impl HyllianSettings {
    pub fn validate(self) -> Result<(), String> {
        if self
            .values
            .iter()
            .zip(SHADER_PARAMETERS)
            .any(|(v, p)| !(p.min..=p.max).contains(v))
        {
            return Err("Hyllian parameter outside its supported range".into());
        }
        if self.values[1] > self.values[2] {
            return Err("minimum beam width must not exceed maximum beam width".into());
        }
        Ok(())
    }
}

/// Mach 3-only preferences: startup sample execution and live visual motion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mach3Settings {
    pub digitized_samples: bool,
    pub motion: MotionMode,
}

/// Mach 3 presentation only; authoritative game updates stay unchanged.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MotionMode {
    /// Interpolate the moving scene while retaining native mountain updates.
    #[default]
    Reforged,
    /// Legacy craft/shadow-only interpolation; persisted as `enhanced`.
    Enhanced,
    /// Native placement and publication cadence.
    Original,
}
impl MotionMode {
    pub const ALL: [Self; 3] = [Self::Reforged, Self::Enhanced, Self::Original];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reforged => "reforged",
            Self::Enhanced => "enhanced",
            Self::Original => "original",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Reforged => "Reforged scene",
            Self::Enhanced => "Enhanced craft",
            Self::Original => "Original",
        }
    }
}
impl std::str::FromStr for MotionMode {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "reforged" => Ok(Self::Reforged),
            "enhanced" => Ok(Self::Enhanced),
            "original" => Ok(Self::Original),
            _ => Err("motion must be reforged, enhanced or original".into()),
        }
    }
}

/// Alley Cat presentation only; input policy and native updates are independent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlleyCatMotion {
    #[default]
    Reforged,
    Original,
}
impl AlleyCatMotion {
    pub const ALL: [Self; 2] = [Self::Reforged, Self::Original];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reforged => "reforged",
            Self::Original => "original",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Reforged => "Reforged",
            Self::Original => "Original",
        }
    }
}
impl std::str::FromStr for AlleyCatMotion {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "reforged" => Ok(Self::Reforged),
            "original" => Ok(Self::Original),
            _ => Err("Alley Cat motion must be reforged or original".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControlMode {
    #[default]
    Responsive,
    Original,
}
impl ControlMode {
    pub const ALL: [Self; 2] = [Self::Responsive, Self::Original];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Responsive => "responsive",
            Self::Original => "original",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Responsive => "Responsive",
            Self::Original => "Original",
        }
    }
}
impl std::str::FromStr for ControlMode {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "responsive" => Ok(Self::Responsive),
            "original" => Ok(Self::Original),
            _ => Err("control mode must be responsive or original".into()),
        }
    }
}
/// Stored Alley Cat startup choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlleyCatMachine {
    Cga,
    Pcjr,
}
impl AlleyCatMachine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cga => "cga",
            Self::Pcjr => "pcjr",
        }
    }
}
impl std::str::FromStr for AlleyCatMachine {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cga" => Ok(Self::Cga),
            "pcjr" | "tandy" => Ok(Self::Pcjr),
            _ => Err("Alley Cat machine must be cga or pcjr".into()),
        }
    }
}
/// Per-entry Alley Cat preferences.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AlleyCatSettings {
    /// None inherits the input-specific machine default. Changes apply on launch.
    pub machine: Option<AlleyCatMachine>,
    pub control_mode: ControlMode,
    pub motion: AlleyCatMotion,
    pub controller_movement: ControllerMovement,
    pub controller_action: FaceButton,
    pub controller_buttons: ControllerButtons,
}
impl Default for Mach3Settings {
    fn default() -> Self {
        Self {
            digitized_samples: true,
            motion: MotionMode::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSettings {
    pub fullscreen: bool,
    pub scale: Option<u32>,
    pub aspect_ratio: AspectRatio,
    pub scaling_policy: ScalingPolicy,
    pub sampling: Sampling,
    pub renderer: Renderer,
    pub backend: Backend,
    pub vsync: bool,
    pub frame_limit: FrameLimit,
    pub presentation: Presentation,
    pub hyllian: HyllianSettings,
    pub audio: AudioSettings,
    pub mach3: Mach3Settings,
    pub alleycat: AlleyCatSettings,
    pub btz: BtzSettings,
    pub centurion: CenturionOptions,
    /// Percentage of each left-stick axis required to activate a direction.
    pub stick_dead_zone: u8,
}
impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            scale: None,
            aspect_ratio: AspectRatio::default(),
            scaling_policy: ScalingPolicy::default(),
            sampling: Sampling::default(),
            renderer: Renderer::Auto,
            backend: Backend::Auto,
            vsync: true,
            frame_limit: FrameLimit::default(),
            presentation: Presentation::default(),
            hyllian: HyllianSettings::default(),
            audio: AudioSettings::default(),
            mach3: Mach3Settings::default(),
            alleycat: AlleyCatSettings::default(),
            btz: BtzSettings::default(),
            centurion: CenturionOptions::default(),
            stick_dead_zone: 35,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Renderer {
    #[default]
    Auto,
    Gpu,
    Sdl,
    Software,
    Terminal,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Backend {
    #[default]
    Auto,
    Vulkan,
    OpenGl,
    OpenGlEs,
    Direct3d11,
    Direct3d12,
    Metal,
}
impl Renderer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gpu => "gpu",
            Self::Sdl => "sdl",
            Self::Software => "software",
            Self::Terminal => "terminal",
        }
    }
}
impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vulkan => "vulkan",
            Self::OpenGl => "opengl",
            Self::OpenGlEs => "opengles2",
            Self::Direct3d11 => "direct3d11",
            Self::Direct3d12 => "direct3d12",
            Self::Metal => "metal",
        }
    }
}
impl std::str::FromStr for Renderer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "gpu" => Ok(Self::Gpu),
            "sdl" => Ok(Self::Sdl),
            "software" => Ok(Self::Software),
            "terminal" => Ok(Self::Terminal),
            _ => Err("renderer must be auto, gpu, sdl, software or terminal".into()),
        }
    }
}
impl std::str::FromStr for Backend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "vulkan" => Ok(Self::Vulkan),
            "opengl" => Ok(Self::OpenGl),
            "opengles2" => Ok(Self::OpenGlEs),
            "direct3d11" => Ok(Self::Direct3d11),
            "direct3d12" => Ok(Self::Direct3d12),
            "metal" => Ok(Self::Metal),
            _ => Err("unknown renderer backend".into()),
        }
    }
}
impl RuntimeSettings {
    /// Whether the selected renderer/backend can execute our custom GPU shader.
    /// Automatic selection can still fall back if GPU initialization fails.
    pub fn supports_hyllian(&self) -> bool {
        matches!(self.renderer, Renderer::Auto | Renderer::Gpu)
            && matches!(
                self.backend,
                Backend::Auto | Backend::Vulkan | Backend::Direct3d12 | Backend::Metal
            )
    }

    pub fn get(&self, key: &str) -> String {
        match key {
            STICK_DEAD_ZONE => self.stick_dead_zone.to_string(),
            DIGITIZED_SAMPLES => self.mach3.digitized_samples.to_string(),
            ALLEYCAT_MACHINE => self
                .alleycat
                .machine
                .map_or("default", AlleyCatMachine::as_str)
                .into(),
            CONTROL_MODE => self.alleycat.control_mode.as_str().into(),
            CONTROLLER_MOVEMENT => self.alleycat.controller_movement.as_str().into(),
            CONTROLLER_ACTION => self.alleycat.controller_action.as_str().into(),
            CONTROLLER_BUTTONS => self.alleycat.controller_buttons.as_str().into(),
            MOTION => self.mach3.motion.as_str().into(),
            ALLEYCAT_MOTION => self.alleycat.motion.as_str().into(),
            BTZ_FILE_CURSOR => self.btz.file_cursor.as_str().into(),
            CENTURION_FLEET_INFORMATION => self.centurion.fleet_information.as_str().into(),
            CENTURION_POPUP_CURSOR => self.centurion.popup_cursor.as_str().into(),
            CENTURION_MENU_DECORATIONS => self.centurion.menu_decorations.as_str().into(),
            CENTURION_MAP_HIGHLIGHT => self.centurion.map_highlight.as_str().into(),
            CENTURION_MAP_HOVER_INFO => self.centurion.map_hover_info.as_str().into(),
            CENTURION_APPLIED_VALUE_INDICATOR => {
                self.centurion.applied_value_indicator.as_str().into()
            }
            "window_mode" => if self.fullscreen {
                "fullscreen"
            } else {
                "windowed"
            }
            .into(),
            "scale" => self.scale.map_or("auto".into(), |x| x.to_string()),
            "aspect_ratio" => self.aspect_ratio.as_str().into(),
            "scaling_policy" => self.scaling_policy.as_str().into(),
            "sampling" => self.sampling.as_str().into(),
            "renderer" => self.renderer.as_str().into(),
            "backend" => self.backend.as_str().into(),
            "vsync" => self.vsync.to_string(),
            "frame_limit" => self.frame_limit.as_str().into(),
            "presentation" => self.presentation.as_str().into(),
            "horizontal_filter" => if self.hyllian.catmull_rom {
                "catmull-rom"
            } else {
                "hermite"
            }
            .into(),
            _ if AUDIO_KEYS.contains(&key) => self.audio.get(key),
            _ => SHADER_PARAMETERS
                .iter()
                .position(|p| p.key == key)
                .map(|i| format!("{:.2}", f64::from(self.hyllian.values[i]) / 100.0))
                .unwrap_or_default(),
        }
    }
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            STICK_DEAD_ZONE => {
                self.stick_dead_zone = value
                    .parse::<u8>()
                    .ok()
                    .filter(|v| (5..=90).contains(v))
                    .ok_or("stick dead zone must be 5..90 percent")?;
            }
            ALLEYCAT_MACHINE => {
                self.alleycat.machine = if value == "default" {
                    None
                } else {
                    Some(value.parse()?)
                };
            }
            CONTROL_MODE => self.alleycat.control_mode = value.parse()?,
            CONTROLLER_MOVEMENT => self.alleycat.controller_movement = value.parse()?,
            CONTROLLER_ACTION => self.alleycat.controller_action = value.parse()?,
            CONTROLLER_BUTTONS => self.alleycat.controller_buttons = value.parse()?,
            MOTION => self.mach3.motion = value.parse()?,
            ALLEYCAT_MOTION => self.alleycat.motion = value.parse()?,
            BTZ_FILE_CURSOR => self.btz.file_cursor = value.parse()?,
            CENTURION_FLEET_INFORMATION => self.centurion.fleet_information = value.parse()?,
            CENTURION_POPUP_CURSOR => self.centurion.popup_cursor = value.parse()?,
            CENTURION_MENU_DECORATIONS => self.centurion.menu_decorations = value.parse()?,
            CENTURION_MAP_HIGHLIGHT => self.centurion.map_highlight = value.parse()?,
            CENTURION_MAP_HOVER_INFO => self.centurion.map_hover_info = value.parse()?,
            CENTURION_APPLIED_VALUE_INDICATOR => {
                self.centurion.applied_value_indicator = value.parse()?
            }
            DIGITIZED_SAMPLES => {
                self.mach3.digitized_samples = match value {
                    "on" | "true" => true,
                    "off" | "false" => false,
                    _ => return Err("digitized samples must be on or off".into()),
                };
            }
            "window_mode" => {
                self.fullscreen = match value {
                    "windowed" => false,
                    "fullscreen" => true,
                    _ => return Err("window mode must be windowed or fullscreen".into()),
                }
            }
            "scale" => {
                self.scale = if value == "auto" {
                    None
                } else {
                    Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|x| (1..=16).contains(x))
                            .ok_or("scale must be auto or 1..16")?,
                    )
                }
            }
            "renderer" => self.renderer = value.parse()?,
            "aspect_ratio" => self.aspect_ratio = value.parse()?,
            "scaling_policy" => self.scaling_policy = value.parse()?,
            "sampling" => self.sampling = value.parse()?,
            "backend" => self.backend = value.parse()?,
            "vsync" => {
                self.vsync = match value {
                    "true" | "on" => true,
                    "false" | "off" => false,
                    _ => return Err("vsync must be on or off".into()),
                }
            }
            "frame_limit" => self.frame_limit = value.parse()?,
            "presentation" => self.presentation = value.parse()?,
            "horizontal_filter" => {
                self.hyllian.catmull_rom = match value {
                    "hermite" => false,
                    "catmull-rom" => true,
                    _ => return Err("filter must be hermite or catmull-rom".into()),
                }
            }
            _ if AUDIO_KEYS.contains(&key) => self.audio.set(key, value)?,
            _ => {
                let i = SHADER_PARAMETERS
                    .iter()
                    .position(|p| p.key == key)
                    .ok_or("unknown video setting")?;
                let n: f64 = value
                    .parse()
                    .map_err(|_| "expected a decimal shader parameter")?;
                let p = SHADER_PARAMETERS[i];
                if !n.is_finite() || n < f64::from(p.min) / 100.0 || n > f64::from(p.max) / 100.0 {
                    return Err(format!(
                        "{} must be between {:.2} and {:.2}",
                        p.label,
                        f64::from(p.min) / 100.0,
                        f64::from(p.max) / 100.0
                    ));
                }
                self.hyllian.values[i] = (n * 100.0).round() as u16;
            }
        };
        Ok(())
    }
}
pub type Patch = BTreeMap<String, String>;
fn overlay(mut base: RuntimeSettings, patch: &Patch) -> RuntimeSettings {
    for (k, v) in patch {
        let _ = base.set(k, v);
    }
    base
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Draft {
    /// Saved choices for one library entry, applied over built-in game defaults.
    pub game: Patch,
}
impl Draft {
    pub fn game_values(&self) -> RuntimeSettings {
        overlay(RuntimeSettings::default(), &self.game)
    }
    /// Switch BTZ's startup frontend without replacing graphical preferences.
    pub fn set_terminal(&mut self, terminal: bool) {
        if terminal {
            if self.game_values().renderer != Renderer::Terminal {
                if let Some(renderer) = self.game.get("renderer").cloned() {
                    self.game.insert(GRAPHICAL_RENDERER.into(), renderer);
                } else {
                    self.game.remove(GRAPHICAL_RENDERER);
                }
                self.game.insert("renderer".into(), "terminal".into());
            }
        } else if self.game_values().renderer == Renderer::Terminal {
            if let Some(renderer) = self.game.remove(GRAPHICAL_RENDERER) {
                self.game.insert("renderer".into(), renderer);
            } else {
                self.game.remove("renderer");
            }
        }
    }
    /// Resolve incompatible treatments at Apply, before temporary CLI values.
    pub fn normalize_treatments(&mut self) {
        let game = self.game_values();
        if game.renderer != Renderer::Terminal
            && game.presentation == Presentation::Hyllian
            && !game.supports_hyllian()
        {
            self.edit("presentation", "pixels".into());
        }
    }
    pub fn reset_section(&mut self, page: usize) {
        let patch = &mut self.game;
        patch.retain(|key, _| !SECTION_KEYS[page].contains(&key.as_str()));
        if page == 2 {
            patch.remove(DIGITIZED_SAMPLES);
        }
        if page == 3 {
            patch.retain(|key, _| {
                !ALLEYCAT_CONTROL_KEYS.contains(&key.as_str()) || key == CONTROL_MODE
            });
        }
        if page == ENHANCEMENTS_SECTION {
            patch.remove(MOTION);
            patch.remove(ALLEYCAT_MOTION);
            patch.remove(CONTROL_MODE);
            patch.remove(CENTURION_FLEET_INFORMATION);
            patch.remove(CENTURION_POPUP_CURSOR);
            patch.remove(CENTURION_MENU_DECORATIONS);
            patch.remove(CENTURION_MAP_HIGHLIGHT);
            patch.remove(CENTURION_MAP_HOVER_INFO);
            patch.remove(CENTURION_APPLIED_VALUE_INDICATOR);
        }
        if page == 0 {
            patch.remove(GRAPHICAL_RENDERER);
            patch.remove(BTZ_FILE_CURSOR);
            patch.remove(ALLEYCAT_MACHINE);
        }
    }
    /// Whether this entry has explicit choices in a settings section.
    pub fn has_custom_settings(&self, page: usize) -> bool {
        SECTION_KEYS[page]
            .iter()
            .any(|key| self.game.contains_key(*key))
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(MOTION)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(ALLEYCAT_MOTION)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CONTROL_MODE)
            || page == 0 && self.game.contains_key(BTZ_FILE_CURSOR)
            || page == 0 && self.game.contains_key(ALLEYCAT_MACHINE)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CENTURION_FLEET_INFORMATION)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CENTURION_POPUP_CURSOR)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CENTURION_MENU_DECORATIONS)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CENTURION_MAP_HIGHLIGHT)
            || page == ENHANCEMENTS_SECTION && self.game.contains_key(CENTURION_MAP_HOVER_INFO)
            || page == ENHANCEMENTS_SECTION
                && self.game.contains_key(CENTURION_APPLIED_VALUE_INDICATOR)
            || page == 2 && self.game.contains_key(DIGITIZED_SAMPLES)
            || page == 3
                && ALLEYCAT_CONTROL_KEYS
                    .iter()
                    .any(|key| *key != CONTROL_MODE && self.game.contains_key(*key))
    }
    /// Store entry choices relative to the game's built-in defaults.
    pub fn edit(&mut self, key: &str, value: String) {
        if value == RuntimeSettings::default().get(key) {
            self.game.remove(key);
        } else {
            self.game.insert(key.into(), value);
        }
    }
    /// Settings edits save differences, while returning to an original explicit
    /// choice restores its presence even when it matches the built-in default.
    /// Pass None after an explicit section reset so cleared overrides stay clear.
    pub fn edit_game(&mut self, key: &str, value: String, original: Option<&Self>) {
        if original.and_then(|draft| draft.game.get(key)) == Some(&value) {
            self.game.insert(key.into(), value);
        } else {
            self.edit(key, value);
        }
    }
}
#[derive(Clone, Debug)]
pub struct Launch {
    pub game_id: String,
    pub entry_id: Option<String>,
    pub path: Option<PathBuf>,
    pub cli: Patch,
}
thread_local! {static LAUNCH:RefCell<Option<Launch>>=const{RefCell::new(None)};}
pub struct LaunchGuard(Option<Launch>);
impl Launch {
    pub fn install(self) -> LaunchGuard {
        LaunchGuard(LAUNCH.with(|v| v.replace(Some(self))))
    }
}
impl Drop for LaunchGuard {
    fn drop(&mut self) {
        LAUNCH.with(|v| v.replace(self.0.take()));
    }
}

#[derive(Clone)]
pub struct Session {
    pub game_id: String,
    pub entry_id: Option<String>,
    pub path: Option<PathBuf>,
    pub cli: Patch,
    pub saved: Draft,
    pub warning: Option<String>,
    blocked: bool,
}
impl Session {
    pub fn for_window(scale: Option<u32>) -> Self {
        let launch = LAUNCH.with(|v| v.borrow().clone());
        match launch {
            Some(x) => Self::load(x),
            None => Self {
                game_id: "standalone".into(),
                entry_id: None,
                path: None,
                cli: Patch::new(),
                saved: Draft {
                    game: scale
                        .map(|n| BTreeMap::from([("scale".into(), n.to_string())]))
                        .unwrap_or_default(),
                },
                warning: None,
                blocked: false,
            },
        }
    }
    pub fn load(mut launch: Launch) -> Self {
        // The application exposes one --motion argument; patches need distinct
        // internal keys because RuntimeSettings contains both games' settings.
        if launch.game_id == "alley-cat"
            && let Some(motion) = launch.cli.remove(MOTION)
        {
            launch.cli.insert(ALLEYCAT_MOTION.into(), motion);
        }
        let mut s = Self {
            game_id: launch.game_id,
            entry_id: launch.entry_id,
            path: launch.path,
            cli: launch.cli,
            saved: Draft::default(),
            warning: None,
            blocked: false,
        };
        if let Some(path) = &s.path {
            match read(path) {
                Ok(doc) => {
                    s.saved = read_draft(&doc, &s.game_id, s.entry_id.as_deref(), &mut s.warning);
                }
                Err(e) => {
                    s.warning = Some(e);
                    s.blocked = true;
                }
            }
        }
        s
    }
    /// Launcher appearance preferences have storage when configuration is enabled.
    pub fn has_launcher_storage(&self) -> bool {
        self.path.is_some()
    }
    /// Per-copy edits require both configuration and a registered library entry.
    /// Unregistered CLI launches retain game edits only in this session.
    pub fn has_entry_storage(&self) -> bool {
        self.has_launcher_storage() && self.entry_id.is_some()
    }
    pub fn effective(&self, draft: &Draft) -> RuntimeSettings {
        let mut values = overlay(draft.game_values(), &self.cli);
        if !supports_terminal(&self.game_id) && values.renderer == Renderer::Terminal {
            values.renderer = Renderer::Auto;
        }
        if self.game_id != "mach-3" {
            values.mach3 = Mach3Settings::default();
        }
        if self.game_id != "alley-cat" {
            values.alleycat = AlleyCatSettings::default();
        }
        if !supports_terminal(&self.game_id) {
            values.btz = BtzSettings::default();
        }
        if self.game_id != "centurion-defender-of-rome" {
            values.centurion = CenturionOptions::default();
        }
        values
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), String> {
        self.commit_with_launcher(draft, launcher::Edits::default())
    }
    /// Apply entry edits and launcher-only preference edits in one
    /// locked, atomic configuration update. Unchanged launcher values are omitted.
    pub fn commit_with_launcher(
        &mut self,
        mut draft: Draft,
        launcher_edits: launcher::Edits,
    ) -> Result<(), String> {
        if let Some(height) = launcher_edits.split_preview_height {
            launcher::validate_split_preview_height(height)?;
        }
        if self.blocked {
            return Err("preferences were applied, but the invalid/unreadable configuration was preserved; repair it before saving".into());
        }
        // Settings can choose Terminal through the full Renderer selector too.
        // Preserve the graphical choice across either entry point.
        if supports_terminal(&self.game_id) {
            if draft.game_values().renderer == Renderer::Terminal {
                let key = if self.saved.game_values().renderer == Renderer::Terminal {
                    GRAPHICAL_RENDERER
                } else {
                    "renderer"
                };
                if let Some(renderer) = self.saved.game.get(key).cloned() {
                    draft.game.insert(GRAPHICAL_RENDERER.into(), renderer);
                } else {
                    draft.game.remove(GRAPHICAL_RENDERER);
                }
            } else {
                draft.game.remove(GRAPHICAL_RENDERER);
            }
        }
        if let Some(path) = &self.path {
            save_changes(
                path,
                &self.game_id,
                self.entry_id.as_deref(),
                &self.saved,
                &draft,
                launcher_edits,
            )?;
        }
        self.saved = draft;
        self.warning = None;
        Ok(())
    }
}
fn read(path: &Path) -> Result<DocumentMut, String> {
    read_document(path, "settings")
}
fn read_document(path: &Path, kind: &str) -> Result<DocumentMut, String> {
    match fs::read_to_string(path) {
        Ok(s) => {
            let doc = s
                .parse::<DocumentMut>()
                .map_err(|e| format!("Cannot parse {}: {e}", path.display()))?;
            if doc
                .get("schema_version")
                .is_some_and(|v| !matches!(v.as_integer(), Some(1 | 2)))
            {
                return Err(format!("unsupported {kind} schema_version; file preserved"));
            }
            Ok(doc)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}
fn read_draft(
    doc: &DocumentMut,
    game: &str,
    entry: Option<&str>,
    warning: &mut Option<String>,
) -> Draft {
    fn patch(item: Option<&Item>, game: &str, warning: &mut Option<String>) -> Patch {
        let mut p = Patch::new();
        if let Some(item) = item {
            if let Some(table) = item.as_table_like() {
                for name in ["display", "crt", "audio", "controls", "enhancements"]
                    .into_iter()
                    .chain((game == "mach-3").then_some("gameplay"))
                    .chain((game == "alley-cat").then_some("launcher"))
                {
                    if table.get(name).is_some_and(|v| !v.is_table_like()) {
                        *warning = Some(format!("Invalid {name} section; using built-in defaults"));
                    }
                }
                for key in keys().chain(game_keys(game)) {
                    if let Some(v) = table.get(section(key)).and_then(|x| x.get(stored_key(key))) {
                        let text: String = if let Some(s) = v.as_str() {
                            s.into()
                        } else if let Some(n) = v.as_integer() {
                            n.to_string()
                        } else if let Some(n) = v.as_float() {
                            n.to_string()
                        } else if let Some(b) = v.as_bool() {
                            b.to_string()
                        } else {
                            String::new()
                        };
                        let mut validated = RuntimeSettings::default();
                        if key == GRAPHICAL_RENDERER {
                            match text.parse::<Renderer>() {
                                Ok(renderer) if renderer != Renderer::Terminal => {
                                    p.insert(key.into(), renderer.as_str().into());
                                }
                                _ => {
                                    *warning = Some(
                                        "Invalid graphical renderer; using built-in default".into(),
                                    )
                                }
                            }
                            continue;
                        }
                        if !(key == ALLEYCAT_MACHINE && text == "default")
                            && validated.set(key, &text).is_ok()
                            && !(key == "renderer"
                                && validated.renderer == Renderer::Terminal
                                && !supports_terminal(game))
                        {
                            p.insert(key.into(), validated.get(key));
                        } else {
                            *warning = Some(format!(
                                "Invalid setting {}; using built-in default",
                                stored_key(key)
                            ));
                        }
                    }
                }
            } else {
                *warning = Some("Invalid display section; using built-in defaults".into());
            }
        }
        p
    }
    let mut draft = Draft {
        game: patch(
            entry.and_then(|id| doc.get("entries").and_then(|x| x.get(id))),
            game,
            warning,
        ),
    };
    // Validate dependent fields together against this entry's defaults.
    if draft.game_values().hyllian.validate().is_err() {
        draft.game.remove("beam_min");
        draft.game.remove("beam_max");
        *warning = Some("Invalid beam widths; using built-in widths".into());
    }
    draft
}
fn save_changes(
    path: &Path,
    game: &str,
    entry: Option<&str>,
    old: &Draft,
    new: &Draft,
    launcher_edits: launcher::Edits,
) -> Result<(), String> {
    if entry.is_none()
        && launcher_edits.split_preview_height.is_none()
        && launcher_edits.gallery_preview_aspect.is_none()
    {
        return Ok(());
    }
    edit_document(path, "settings", |doc| {
        launcher::write_edits(doc, launcher_edits)?;
        doc["schema_version"] = toml_edit::value(2);
        if let Some(entry) = entry {
            let (before, after) = (&old.game, &new.game);
            let route = ["entries", entry];
            for key in keys().chain(game_keys(game)) {
                if before.get(key) == after.get(key) {
                    continue;
                }
                let mut item = doc.as_item_mut();
                for part in route.iter().copied().chain(std::iter::once(section(key))) {
                    if item.is_none() {
                        *item = Item::Table(toml_edit::Table::new());
                    }
                    if !item.is_table_like() {
                        return Err("configuration section is not a table; file preserved".into());
                    }
                    item = &mut item[part];
                }
                if item.is_none() {
                    *item = Item::Table(toml_edit::Table::new());
                }
                if !item.is_table_like() {
                    return Err("configuration section is not a table; file preserved".into());
                }
                let text = after.get(key);
                let key = stored_key(key);
                if let Some(text) = text {
                    let replacement = match key {
                        "vsync" | "sound_enabled" | "mute_unfocused" | DIGITIZED_SAMPLES => {
                            toml_edit::Value::from(text == "true" || text == "on")
                        }
                        "scale" | STICK_DEAD_ZONE | "master_volume" | "music_volume"
                        | "speech_volume" | "effects_volume"
                            if text != "auto" =>
                        {
                            toml_edit::Value::from(text.parse::<i64>().map_err(|e| e.to_string())?)
                        }
                        _ if SHADER_PARAMETERS.iter().any(|p| p.key == key) => {
                            toml_edit::Value::from(text.parse::<f64>().map_err(|e| e.to_string())?)
                        }
                        _ => toml_edit::Value::from(text.as_str()),
                    };
                    let mut replacement = replacement;
                    if let Some(old) = item.get(key).and_then(Item::as_value) {
                        *replacement.decor_mut() = old.decor().clone();
                    }
                    item[key] = Item::Value(replacement);
                } else if let Some(table) = item.as_table_like_mut() {
                    table.remove(key);
                }
            }
        }
        Ok(())
    })
}

struct DocumentLock(fs::File);
impl Drop for DocumentLock {
    fn drop(&mut self) {
        // Unix subprocess startup can briefly inherit the open-file description.
        // Closing only our fd leaves its flock held until the child execs.
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

pub(crate) fn edit_document<T>(
    path: &Path,
    kind: &str,
    edit: impl FnOnce(&mut DocumentMut) -> Result<T, String>,
) -> Result<T, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut directories = fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directories.mode(0o700);
    }
    directories.create(parent).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|e| format!("{kind} file is busy; retry: {e}"))?;
    let _lock = DocumentLock(lock);
    let mut doc = read_document(path, kind)?;
    let value = edit(&mut doc)?;
    let temporary = parent.join(format!(
        ".reforge-{kind}-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temporary).map_err(|e| e.to_string())?;
        f.write_all(doc.to_string().as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        Ok(value)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn default_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("REforge/config.toml"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/REforge/config.toml"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|x| !x.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|x| PathBuf::from(x).join(".config")))
            .map(|p| p.join("reforge/config.toml"))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn game_edits_restore_original_presence_but_resets_do_not() {
        for (key, initial, other) in [
            ("window_mode", "windowed", "fullscreen"),
            ("master_volume", "80", "60"),
            (DIGITIZED_SAMPLES, "true", "false"),
            (CONTROL_MODE, "responsive", "original"),
            (CONTROLLER_MOVEMENT, "directional", "button"),
            (CONTROLLER_ACTION, "south", "north"),
            (
                CONTROLLER_BUTTONS,
                "west-jump-south-action",
                "east-action-south-jump",
            ),
        ] {
            let original = Draft {
                game: Patch::from([(key.into(), initial.into())]),
            };
            let mut draft = original.clone();
            draft.edit_game(key, other.into(), Some(&original));
            draft.edit_game(key, initial.into(), Some(&original));
            assert_eq!(draft, original, "round trip must preserve explicit {key}");
            let inherited = Draft::default();
            let mut draft = inherited.clone();
            draft.edit_game(key, other.into(), Some(&inherited));
            draft.edit_game(key, initial.into(), Some(&inherited));
            assert!(
                draft.game.is_empty(),
                "round trip must preserve inheritance for {key}"
            );
            let mut draft = original.clone();
            draft.reset_section(section_index(key));
            draft.edit_game(key, other.into(), None);
            draft.edit_game(key, initial.into(), None);
            assert!(draft.game.is_empty(), "reset {key} must not return");
        }
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    #[test]
    fn persistence_destinations_match_registered_unregistered_and_no_config_runs() {
        let dir = TestDir::new();
        for (name, registered, configured) in [
            ("registered", true, true),
            ("unregistered", false, true),
            ("no-config", true, false),
            ("temporary", false, false),
        ] {
            let path = dir.0.join(format!("{name}.toml"));
            let launch = Launch {
                game_id: "mach-3".into(),
                entry_id: registered.then(|| "test-copy".into()),
                path: configured.then(|| path.clone()),
                cli: Patch::new(),
            };
            let mut session = Session::load(launch.clone());
            assert_eq!(session.has_launcher_storage(), configured);
            assert_eq!(session.has_entry_storage(), configured && registered);
            let mut draft = session.saved.clone();
            draft.game.insert("master_volume".into(), "75".into());
            draft.game.insert(MOTION.into(), "original".into());
            session.commit(draft).unwrap();
            assert_eq!(session.saved.game[MOTION], "original");
            let reloaded = Session::load(launch);
            assert_eq!(
                reloaded.saved.game.contains_key("master_volume"),
                configured && registered
            );
            assert_eq!(
                reloaded.saved.game.contains_key(MOTION),
                configured && registered
            );
            assert_eq!(path.exists(), configured && registered);
        }
    }
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "reforge-prefs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn file(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn session(path: PathBuf, game: &str) -> Session {
        Session::load(Launch {
            game_id: game.into(),
            entry_id: Some(game.into()),
            path: Some(path),
            cli: Patch::new(),
        })
    }

    #[cfg(unix)]
    #[test]
    fn document_lock_release_does_not_wait_for_inherited_handles() {
        let directory = TestDir::new();
        let path = directory.file();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&file).unwrap();
        let guard = DocumentLock(file);
        // A duplicate shares the open-file description, just like an fd
        // inherited while another thread starts a subprocess on Unix.
        let inherited = guard.0.try_clone().unwrap();
        drop(guard);
        edit_document(&path, "settings", |document| {
            document["written"] = toml_edit::value(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            read_document(&path, "settings").unwrap()["written"].as_bool(),
            Some(true)
        );
        drop(inherited);
    }
    #[test]
    fn btz_cursor_is_game_scoped_persistent_and_retained_in_terminal_play() {
        assert_eq!(
            FileCursorStyle::default(),
            FileCursorStyle::PulsingUnderline
        );
        assert!("blink".parse::<FileCursorStyle>().is_err());
        for game in ["mindwheel", "essex", "brimstone", "breakers"] {
            for style in [
                FileCursorStyle::SteadyBlock,
                FileCursorStyle::BlinkingUnderline,
            ] {
                let dir = TestDir::new();
                let path = dir.file();
                let mut settings = session(path.clone(), game);
                let mut draft = settings.saved.clone();

                draft.edit(BTZ_FILE_CURSOR, style.as_str().into());
                assert!(draft.has_custom_settings(0));
                assert_eq!(
                    settings.effective(&settings.saved).btz.file_cursor,
                    FileCursorStyle::PulsingUnderline
                );
                settings.commit(draft).unwrap();
                let mut loaded = session(path.clone(), game);
                assert_eq!(loaded.effective(&loaded.saved).btz.file_cursor, style);
                let mut draft = loaded.saved.clone();
                draft.set_terminal(true);
                loaded.commit(draft).unwrap();
                let loaded = session(path.clone(), game);
                assert_eq!(loaded.effective(&loaded.saved).renderer, Renderer::Terminal);
                assert_eq!(loaded.effective(&loaded.saved).btz.file_cursor, style);
                for other in ["alley-cat", "mach-3"] {
                    assert_eq!(
                        session(path.clone(), other)
                            .effective(&loaded.saved)
                            .btz
                            .file_cursor,
                        FileCursorStyle::PulsingUnderline
                    );
                }
                let mut reset = loaded.saved.clone();
                reset.reset_section(1);
                assert_eq!(reset.game_values().btz.file_cursor, style);
                reset.reset_section(0);
                assert!(!reset.game.contains_key(BTZ_FILE_CURSOR));
                assert_eq!(
                    reset.game_values().btz.file_cursor,
                    FileCursorStyle::PulsingUnderline
                );
            }
        }
    }

    #[test]
    fn terminal_renderer_is_game_scoped_and_round_trips_without_changing_graphics_preferences() {
        assert_eq!("terminal".parse::<Renderer>().unwrap(), Renderer::Terminal);
        assert_eq!(Renderer::Terminal.as_str(), "terminal");
        assert!("terminal".parse::<Backend>().is_err());
        for game in ["mindwheel", "essex", "brimstone", "breakers"] {
            let dir = TestDir::new();
            let path = dir.file();
            fs::write(
            &path,
            format!("[entries.{game}.display]\nbackend = 'vulkan'\nscale = 3\n[entries.{game}.crt]\npresentation = 'hyllian'\n"),
        )
        .unwrap();
            let mut s = session(path.clone(), game);
            let mut draft = s.saved.clone();
            draft.edit("renderer", "terminal".into());
            draft.normalize_treatments();
            s.commit(draft).unwrap();
            let mut loaded = session(path.clone(), game);
            let terminal = loaded.effective(&loaded.saved);
            assert_eq!(terminal.renderer, Renderer::Terminal);
            assert_eq!(terminal.backend, Backend::Vulkan);
            assert_eq!(terminal.scale, Some(3));
            assert_eq!(terminal.presentation, Presentation::Hyllian);
            assert!(!terminal.supports_hyllian());
            loaded.cli.insert("renderer".into(), "gpu".into());
            assert_eq!(loaded.effective(&loaded.saved).renderer, Renderer::Gpu);
            assert_eq!(session(path.clone(), game).saved, loaded.saved);
            let mut reset = loaded.saved.clone();
            reset.reset_section(0);
            assert_eq!(reset.game_values().renderer, Renderer::Auto);
            assert_eq!(reset.game_values().presentation, Presentation::Hyllian);
        }
    }

    #[test]
    fn frontend_switch_restores_explicit_renderer_and_retains_inheritance() {
        for renderer in [
            None,
            Some("auto"),
            Some("gpu"),
            Some("sdl"),
            Some("software"),
        ] {
            let dir = TestDir::new();
            let path = dir.file();
            let mut s = session(path.clone(), "mindwheel");
            let mut d = s.saved.clone();
            d.game.insert("window_mode".into(), "fullscreen".into());
            d.game.insert("presentation".into(), "hyllian".into());
            if let Some(renderer) = renderer {
                d.game.insert("renderer".into(), renderer.into());
            }
            s.commit(d).unwrap();
            let graphical = s.saved.clone();
            // Use the same direct setting edit as the full Renderer selector.
            let mut d = s.saved.clone();
            d.edit("renderer", "terminal".into());
            s.commit(d).unwrap();
            s = session(path.clone(), "mindwheel");
            assert_eq!(s.effective(&s.saved).renderer, Renderer::Terminal);
            let mut d = s.saved.clone();
            d.set_terminal(false);
            s.commit(d).unwrap();
            assert_eq!(session(path.clone(), "mindwheel").saved, graphical);

            let mut d = s.saved.clone();
            d.set_terminal(true);
            s.commit(d).unwrap();
            // The built-in renderer stays the fallback while terminal play is active.
            let d = s.saved.clone();
            s.commit(d).unwrap();
            s = session(path, "mindwheel");
            let mut d = s.saved.clone();
            d.set_terminal(false);
            s.commit(d).unwrap();
            assert_eq!(s.saved.game.get("renderer").map(String::as_str), renderer);
            assert_eq!(
                s.saved.game_values().renderer.as_str(),
                renderer.unwrap_or("auto")
            );
            assert!(s.saved.game_values().fullscreen);
            assert_eq!(s.saved.game_values().presentation, Presentation::Hyllian);
        }
    }

    #[test]
    fn alleycat_motion_is_per_copy_and_independent_of_input_and_mach3() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            concat!(
                "[global.display]\nmotion='original'\n",
                "[entries.other.enhancements]\nmotion='original'\n",
                "[entries.mach-3.enhancements]\nmotion='enhanced'\n"
            ),
        )
        .unwrap();
        let launch = |entry: &str| Launch {
            game_id: "alley-cat".into(),
            entry_id: Some(entry.into()),
            path: Some(path.clone()),
            cli: Patch::new(),
        };
        let mut first = Session::load(launch("first"));
        assert_eq!(
            first.effective(&first.saved).alleycat.motion,
            AlleyCatMotion::Reforged
        );
        assert_eq!(
            first.effective(&first.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
        let mut draft = first.saved.clone();

        draft.edit(ALLEYCAT_MOTION, "original".into());
        assert!(draft.has_custom_settings(ENHANCEMENTS_SECTION));
        assert!(!draft.has_custom_settings(3));
        assert_eq!(draft.game_values().get(ALLEYCAT_MOTION), "original");
        assert_eq!(draft.game_values().get(MOTION), "reforged");
        first.commit(draft).unwrap();
        let loaded = Session::load(launch("first"));
        assert_eq!(
            loaded.effective(&loaded.saved).alleycat.motion,
            AlleyCatMotion::Original
        );
        assert_eq!(
            loaded.effective(&loaded.saved).mach3.motion,
            MotionMode::Reforged
        );
        assert_eq!(
            Session::load(launch("second"))
                .saved
                .game_values()
                .alleycat
                .motion,
            AlleyCatMotion::Reforged
        );
        assert_eq!(
            Session::load(launch("other"))
                .saved
                .game_values()
                .alleycat
                .motion,
            AlleyCatMotion::Original
        );
        assert_eq!(
            session(path.clone(), "mach-3")
                .effective(&session(path.clone(), "mach-3").saved)
                .mach3
                .motion,
            MotionMode::Enhanced
        );
        let text = fs::read_to_string(&path).unwrap();
        let doc = text.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["entries"]["first"]["enhancements"][MOTION].as_str(),
            Some("original")
        );
        assert!(
            doc["entries"]["first"]["enhancements"]
                .get(ALLEYCAT_MOTION)
                .is_none()
        );
        assert_eq!(
            doc["entries"]["mach-3"]["enhancements"][MOTION].as_str(),
            Some("enhanced")
        );
        let mut controls = loaded.saved.clone();
        controls.edit(CONTROL_MODE, "original".into());
        controls.reset_section(ENHANCEMENTS_SECTION);
        assert_eq!(
            controls.game_values().alleycat.motion,
            AlleyCatMotion::Reforged
        );
        assert_eq!(
            controls.game_values().alleycat.control_mode,
            ControlMode::Responsive
        );
        assert!(!controls.has_custom_settings(3));
        assert!(!controls.has_custom_settings(ENHANCEMENTS_SECTION));
    }

    #[test]
    fn alleycat_motion_cli_is_temporary_and_rejects_mach3_only_modes() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[entries.copy.enhancements]\nmotion='reforged'\n").unwrap();
        let launch = Launch {
            game_id: "alley-cat".into(),
            entry_id: Some("copy".into()),
            path: Some(path.clone()),
            cli: Patch::from([(MOTION.into(), "original".into())]),
        };
        let mut loaded = Session::load(launch);
        assert!(!loaded.cli.contains_key(MOTION));
        assert_eq!(
            loaded.cli.get(ALLEYCAT_MOTION).map(String::as_str),
            Some("original")
        );
        assert_eq!(loaded.saved.game_values().get(ALLEYCAT_MOTION), "reforged");
        assert_eq!(
            loaded.effective(&loaded.saved).get(ALLEYCAT_MOTION),
            "original"
        );
        let saved = loaded.saved.clone();
        loaded.commit(saved).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "schema_version = 2\n[entries.copy.enhancements]\nmotion='reforged'\n"
        );
        for invalid in ["enhanced", "scene", "faithful"] {
            assert!(
                RuntimeSettings::default()
                    .set(ALLEYCAT_MOTION, invalid)
                    .is_err()
            );
            fs::write(
                &path,
                format!("[entries.alley-cat.enhancements]\nmotion='{invalid}' # preserve\n"),
            )
            .unwrap();
            let original = fs::read_to_string(&path).unwrap();
            let s = session(path.clone(), "alley-cat");
            assert!(s.warning.is_some());
            assert!(!s.saved.game.contains_key(ALLEYCAT_MOTION));
            assert_eq!(
                s.effective(&s.saved).alleycat.motion,
                AlleyCatMotion::Reforged
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    fn mach3_motion_is_game_only_persistent_and_defaults_to_reforged() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            "[global.display]\nmotion='original'\n[entries.alley-cat.enhancements]\nmotion='original'\n",
        )
        .unwrap();
        let mut mach = session(path.clone(), "mach-3");
        assert_eq!(
            mach.effective(&mach.saved).mach3.motion,
            MotionMode::Reforged
        );
        assert!(
            !session(path.clone(), "alley-cat")
                .saved
                .game
                .contains_key(MOTION)
        );
        let mut draft = mach.saved.clone();

        draft.edit(MOTION, "original".into());
        assert!(draft.has_custom_settings(ENHANCEMENTS_SECTION));
        assert_eq!(section_index(MOTION), ENHANCEMENTS_SECTION);
        assert_eq!(
            mach.effective(&mach.saved).mach3.motion,
            MotionMode::Reforged
        );
        mach.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "mach-3");
        assert_eq!(
            loaded.effective(&loaded.saved).mach3.motion,
            MotionMode::Original
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("motion = \"original\"")
        );
        loaded.cli.insert(MOTION.into(), "enhanced".into());
        assert_eq!(
            loaded.effective(&loaded.saved).mach3.motion,
            MotionMode::Enhanced
        );
        assert_eq!(
            session(path.clone(), "mach-3")
                .saved
                .game
                .get(MOTION)
                .map(String::as_str),
            Some("original")
        );
        let mut reset = loaded.saved.clone();
        reset.reset_section(2);
        assert_eq!(reset.game_values().mach3.motion, MotionMode::Original);
        reset.reset_section(ENHANCEMENTS_SECTION);
        assert!(!reset.has_custom_settings(ENHANCEMENTS_SECTION));
        assert_eq!(reset.game_values().mach3.motion, MotionMode::Reforged);
        assert_eq!(
            session(path, "alley-cat")
                .effective(&loaded.saved)
                .mach3
                .motion,
            MotionMode::Reforged
        );
        assert!("faithful".parse::<MotionMode>().is_err());
    }

    #[test]
    fn mach3_motion_preserves_explicit_legacy_modes_and_round_trips_reforged() {
        let dir = TestDir::new();
        let path = dir.file();
        for (name, mode) in [
            ("enhanced", MotionMode::Enhanced),
            ("original", MotionMode::Original),
            ("reforged", MotionMode::Reforged),
        ] {
            fs::write(
                &path,
                format!("[entries.mach-3.enhancements]\nmotion='{name}'\n"),
            )
            .unwrap();
            let mut loaded = session(path.clone(), "mach-3");
            assert_eq!(loaded.effective(&loaded.saved).mach3.motion, mode);
            assert_eq!(name.parse::<MotionMode>().unwrap(), mode);
            assert_eq!(mode.as_str(), name);
            // Saving another section must not migrate a deliberate legacy choice.
            let mut draft = loaded.saved.clone();
            draft.edit(DIGITIZED_SAMPLES, "false".into());
            loaded.commit(draft).unwrap();
            let saved = session(path.clone(), "mach-3");
            assert_eq!(saved.saved.game.get(MOTION).map(String::as_str), Some(name));
            assert_eq!(saved.effective(&saved.saved).mach3.motion, mode);
            let mut reset = saved.saved.clone();

            reset.reset_section(2);
            reset.reset_section(3);
            assert_eq!(reset.game_values().mach3.motion, mode);
            reset.reset_section(ENHANCEMENTS_SECTION);
            assert_eq!(reset.game_values().mach3.motion, MotionMode::Reforged);
        }
    }

    #[test]
    fn alleycat_controls_are_game_only_sparse_and_apply_scoped() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            r#"
schema_version = 1
[global.controls]
control_mode = "original"
[entries.mach-3.controls]
control_mode = "original"
"#,
        )
        .unwrap();
        let mut cat = session(path.clone(), "alley-cat");
        let mut mach = session(path.clone(), "mach-3");
        assert_eq!(
            cat.effective(&cat.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
        assert_eq!(
            mach.effective(&mach.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
        assert!(!mach.saved.game.contains_key(CONTROL_MODE));
        let mut draft = cat.saved.clone();

        draft.edit(CONTROL_MODE, "original".into());
        assert!(draft.has_custom_settings(ENHANCEMENTS_SECTION));
        assert_eq!(section_index(CONTROL_MODE), ENHANCEMENTS_SECTION);
        assert_eq!(
            cat.effective(&cat.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
        // A concurrent change to another game must survive this game's Apply.
        let mut mach_draft = mach.saved.clone();
        mach_draft.edit(DIGITIZED_SAMPLES, "false".into());
        mach.commit(mach_draft).unwrap();
        cat.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "alley-cat");
        assert_eq!(
            loaded.effective(&loaded.saved).alleycat.control_mode,
            ControlMode::Original
        );
        let mut reset = loaded.saved.clone();

        reset.reset_section(2);
        assert_eq!(
            reset.game_values().alleycat.control_mode,
            ControlMode::Original
        );
        reset.reset_section(ENHANCEMENTS_SECTION);
        assert_eq!(
            reset.game_values().alleycat.control_mode,
            ControlMode::Responsive
        );
        assert_eq!(
            session(path.clone(), "alley-cat").saved,
            loaded.saved,
            "reset is draft-only until Apply"
        );
        loaded.commit(reset).unwrap();
        let doc = read(&path).unwrap();
        assert!(
            doc["entries"]["alley-cat"]["enhancements"]
                .get(CONTROL_MODE)
                .is_none()
        );
        assert_eq!(
            doc["entries"]["mach-3"]["gameplay"][DIGITIZED_SAMPLES].as_bool(),
            Some(false)
        );
        assert_eq!(
            doc["global"]["controls"][CONTROL_MODE].as_str(),
            Some("original"),
            "unrecognized global data is preserved"
        );
        // Invalid values warn and fall back without rewriting the user's file.
        fs::write(
            &path,
            "[entries.alley-cat.enhancements]\ncontrol_mode = 'fast'\n",
        )
        .unwrap();
        let invalid = session(path.clone(), "alley-cat");
        assert!(invalid.warning.is_some());
        assert_eq!(
            invalid.effective(&invalid.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
    }

    #[test]
    fn controller_templates_are_entry_only_and_keep_each_modes_layout() {
        let dir = TestDir::new();
        let path = dir.file();
        let selected = [
            (CONTROLLER_MOVEMENT, "button"),
            (CONTROLLER_ACTION, "north"),
            (CONTROLLER_BUTTONS, "east-action-south-jump"),
        ];
        let values = selected
            .map(|(key, value)| format!("{key} = '{value}'\n"))
            .join("");
        fs::write(
            &path,
            format!("[global.controls]\n{values}[entries.mach-3.controls]\n{values}"),
        )
        .unwrap();
        let mut cat = session(path.clone(), "alley-cat");
        let defaults = AlleyCatSettings::default();
        assert_eq!(cat.saved.game_values().alleycat, defaults);
        let mut draft = cat.saved.clone();
        for (key, value) in selected {
            assert_eq!(section_index(key), 3);
            draft.edit(key, value.into());
            draft.edit(key, value.into());
        }
        assert!(draft.has_custom_settings(3));
        cat.commit(draft).unwrap();

        let mut loaded = session(path.clone(), "alley-cat");
        let saved_layout = loaded.saved.game_values().alleycat;
        assert_eq!(saved_layout.controller_movement, ControllerMovement::Button);
        assert_eq!(saved_layout.controller_action, FaceButton::North);
        assert_eq!(
            saved_layout.controller_buttons,
            ControllerButtons::EastActionSouthJump
        );
        let other = Session::load(Launch {
            game_id: "alley-cat".into(),
            entry_id: Some("alley-cat-other".into()),
            path: Some(path.clone()),
            cli: Patch::new(),
        });
        assert_eq!(other.saved.game_values().alleycat, defaults);
        let mach = session(path.clone(), "mach-3");
        assert_eq!(mach.effective(&mach.saved).alleycat, defaults);
        for (key, _) in selected {
            assert!(!mach.saved.game.contains_key(key));
        }

        // A mode switch leaves both independent layout choices stored.
        let mut draft = loaded.saved.clone();
        draft.edit(CONTROLLER_MOVEMENT, "directional".into());
        loaded.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "alley-cat");
        let directional = loaded.saved.game_values().alleycat;
        assert_eq!(
            directional.controller_movement,
            ControllerMovement::Directional
        );
        assert_eq!(
            directional.controller_action,
            saved_layout.controller_action
        );
        assert_eq!(
            directional.controller_buttons,
            saved_layout.controller_buttons
        );
        assert!(!loaded.saved.game.contains_key(CONTROLLER_MOVEMENT));
        assert!(loaded.saved.has_custom_settings(3));

        let mut reset = loaded.saved.clone();
        reset.reset_section(0);
        assert_eq!(reset.game_values().alleycat, directional);
        reset.reset_section(3);
        assert_eq!(reset.game_values().alleycat, defaults);
        assert!(!reset.has_custom_settings(3));
        loaded.commit(reset).unwrap();
        let doc = read(&path).unwrap();
        for (key, value) in selected {
            assert!(doc["entries"]["alley-cat"]["controls"].get(key).is_none());
            assert_eq!(doc["global"]["controls"][key].as_str(), Some(value));
            assert_eq!(
                doc["entries"]["mach-3"]["controls"][key].as_str(),
                Some(value)
            );
        }
    }

    #[test]
    fn invalid_controller_templates_warn_without_rewriting_the_file() {
        let dir = TestDir::new();
        let path = dir.file();
        for key in [CONTROLLER_MOVEMENT, CONTROLLER_ACTION, CONTROLLER_BUTTONS] {
            let text = format!("[entries.alley-cat.controls]\n{key} = 'unknown'\n");
            fs::write(&path, &text).unwrap();
            let invalid = session(path.clone(), "alley-cat");
            assert!(invalid.warning.is_some(), "invalid {key} must warn");
            assert_eq!(
                invalid.saved.game_values().alleycat,
                AlleyCatSettings::default()
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn alleycat_machine_shares_launcher_pins_and_resets_to_input_default() {
        let dir = TestDir::new();
        let path = dir.file();
        launcher::save_native_option(&path, "alley-cat", "machine", Some("pcjr")).unwrap();
        launcher::save_native_option(&path, "alley-cat", "future_option", Some("keep")).unwrap();
        launcher::save_native_option(&path, "another-copy", "machine", Some("cga")).unwrap();
        let mut current = session(path.clone(), "alley-cat");
        assert_eq!(
            current.saved.game_values().alleycat.machine,
            Some(AlleyCatMachine::Pcjr)
        );
        assert!(current.saved.has_custom_settings(0));
        assert_eq!(section_index(ALLEYCAT_MACHINE), 0);
        let mut draft = current.saved.clone();
        draft.edit_game(ALLEYCAT_MACHINE, "cga".into(), Some(&current.saved));
        current.commit(draft).unwrap();
        assert_eq!(
            launcher::native_options(&path, "alley-cat").unwrap()["machine"],
            "cga"
        );
        let mut current = session(path.clone(), "alley-cat");
        let mut draft = current.saved.clone();
        // Choosing an engine's concrete default remains an explicit pin.
        draft.edit(ALLEYCAT_MACHINE, "cga".into());
        current.commit(draft).unwrap();
        assert_eq!(
            launcher::native_options(&path, "alley-cat").unwrap()["machine"],
            "cga"
        );
        let mut draft = current.saved.clone();

        assert_eq!(
            draft.game_values().alleycat.machine,
            Some(AlleyCatMachine::Cga)
        );
        draft.reset_section(0);
        assert!(!draft.has_custom_settings(0));
        assert_eq!(draft.game_values().alleycat.machine, None);
        current.commit(draft).unwrap();
        let native = launcher::native_options(&path, "alley-cat").unwrap();
        assert!(!native.contains_key("machine"));
        assert_eq!(native["future_option"], "keep");
        assert_eq!(
            launcher::native_options(&path, "another-copy").unwrap()["machine"],
            "cga"
        );
    }

    #[test]
    fn unrelated_settings_save_preserves_concurrent_launcher_machine_edits() {
        let dir = TestDir::new();
        let path = dir.file();
        launcher::save_native_option(&path, "alley-cat", "machine", Some("cga")).unwrap();
        let mut current = session(path.clone(), "alley-cat");
        launcher::save_native_option(&path, "alley-cat", "machine", Some("pcjr")).unwrap();
        launcher::save_native_option(&path, "alley-cat", "future_option", Some("new")).unwrap();
        let mut draft = current.saved.clone();
        draft.edit("vsync", "false".into());
        current.commit(draft).unwrap();
        let latest = session(path.clone(), "alley-cat");
        assert_eq!(
            latest.saved.game_values().alleycat.machine,
            Some(AlleyCatMachine::Pcjr)
        );
        assert!(!latest.saved.game_values().vsync);
        assert_eq!(
            launcher::native_options(&path, "alley-cat").unwrap()["future_option"],
            "new"
        );
    }

    #[test]
    fn invalid_alleycat_machine_values_and_malformed_sections_remain_intact() {
        let dir = TestDir::new();
        let path = dir.file();
        for invalid in ["unknown", "default"] {
            fs::write(
                &path,
                format!(
                    "[entries.alley-cat.launcher]\nmachine = '{invalid}'\nfuture_flag = true\n"
                ),
            )
            .unwrap();
            let mut current = session(path.clone(), "alley-cat");
            assert!(current.warning.is_some());
            assert_eq!(current.saved.game_values().alleycat.machine, None);
            let mut draft = current.saved.clone();
            draft.edit("vsync", "false".into());
            current.commit(draft).unwrap();
            let doc = read(&path).unwrap();
            assert_eq!(
                doc["entries"]["alley-cat"]["launcher"]["machine"].as_str(),
                Some(invalid)
            );
            assert_eq!(
                doc["entries"]["alley-cat"]["launcher"]["future_flag"].as_bool(),
                Some(true)
            );
        }
        let original = "[entries.alley-cat]\nlauncher = 'broken'\n";
        fs::write(&path, original).unwrap();
        let mut current = session(path.clone(), "alley-cat");
        assert!(current.warning.is_some());
        let mut draft = current.saved.clone();
        draft.edit("vsync", "false".into());
        draft.edit(ALLEYCAT_MACHINE, "pcjr".into());
        assert!(current.commit(draft).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn native_samples_are_mach3_only_persistent_and_not_inherited_from_global_audio() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            r#"
schema_version = 1
[global.gameplay]
digitized_samples = false
[entries.alley-cat.gameplay]
digitized_samples = false
"#,
        )
        .unwrap();
        let mut mach = session(path.clone(), "mach-3");
        let cat = session(path.clone(), "alley-cat");
        assert!(mach.effective(&mach.saved).mach3.digitized_samples);
        assert!(cat.effective(&cat.saved).mach3.digitized_samples);
        assert!(!cat.saved.game.contains_key(DIGITIZED_SAMPLES));
        let mut draft = mach.saved.clone();
        assert!(!draft.has_custom_settings(2));

        draft.edit(DIGITIZED_SAMPLES, "false".into());
        assert!(draft.has_custom_settings(2));
        assert_eq!(section_index(DIGITIZED_SAMPLES), 2);
        draft.edit("sound_enabled", "false".into());
        assert!(!mach.effective(&draft).mach3.digitized_samples);
        assert!(!mach.effective(&draft).audio.enabled);
        mach.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert!(!loaded.effective(&loaded.saved).mach3.digitized_samples);
        assert!(!loaded.saved.game_values().audio.enabled);
        // Other sections leave Audio intact; resetting Audio restores On.
        let mut draft = loaded.saved.clone();
        draft.reset_section(1);

        assert!(!draft.game_values().mach3.digitized_samples);
        assert!(!draft.game_values().audio.enabled);
        assert!(draft.has_custom_settings(2));
        let doc = read(&path).unwrap();
        assert_eq!(
            doc["entries"]["mach-3"]["gameplay"][DIGITIZED_SAMPLES].as_bool(),
            Some(false)
        );
        assert_eq!(
            doc["entries"]["mach-3"]["audio"]["sound_enabled"].as_bool(),
            Some(false)
        );
        draft.reset_section(2);
        assert!(draft.game_values().mach3.digitized_samples);
        assert!(!draft.has_custom_settings(2));
        // Cancel leaves the saved Off value intact. Apply removes only the
        // game override, without creating a global digitized-samples setting.
        assert!(
            !session(path.clone(), "mach-3")
                .saved
                .game_values()
                .mach3
                .digitized_samples
        );
        mach.commit(draft).unwrap();
        let reset = session(path.clone(), "mach-3");
        assert!(reset.saved.game_values().mach3.digitized_samples);
        assert!(!reset.saved.has_custom_settings(2));
    }

    #[test]
    fn sample_cli_overrides_do_not_persist_and_concurrent_game_edits_survive() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut mach = session(path.clone(), "mach-3");
        let mut cat = session(path.clone(), "alley-cat");
        mach.cli.insert(DIGITIZED_SAMPLES.into(), "false".into());
        assert!(!mach.effective(&mach.saved).mach3.digitized_samples);
        let mut cat_draft = cat.saved.clone();
        cat_draft.edit("scale", "2".into());
        cat.commit(cat_draft).unwrap();
        let mut draft = mach.saved.clone();
        draft.edit("master_volume", "65".into());
        mach.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert!(loaded.effective(&loaded.saved).mach3.digitized_samples);
        assert_eq!(
            session(path.clone(), "alley-cat").saved.game_values().scale,
            Some(2)
        );
        let mut draft = mach.saved.clone();
        draft.edit(DIGITIZED_SAMPLES, "false".into());
        mach.commit(draft).unwrap();
        let mut draft = mach.saved.clone();
        draft.reset_section(2);
        assert!(
            !mach.effective(&draft).mach3.digitized_samples,
            "S overrides this run even after resetting the saved preference"
        );
        mach.commit(draft).unwrap();
        assert!(
            !read(&path).unwrap()["entries"]["mach-3"]["gameplay"]
                .as_table_like()
                .unwrap()
                .contains_key(DIGITIZED_SAMPLES)
        );
        for bad in ["yes", "0", "", "mute"] {
            assert!(
                RuntimeSettings::default()
                    .set(DIGITIZED_SAMPLES, bad)
                    .is_err()
            );
        }
    }

    #[test]
    fn treatment_reset_is_saved_per_game_and_requires_reselection() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let mut draft = s.saved.clone();
        draft.edit("presentation", "hyllian".into());
        s.commit(draft.clone()).unwrap();
        draft.edit("backend", "opengl".into());
        draft.normalize_treatments();
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        assert!(!draft.game.contains_key("presentation"));
        s.commit(draft).unwrap();

        let mut s = session(path, "mach-3");
        let mut draft = s.saved.clone();
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        draft.edit("backend", "auto".into());
        draft.normalize_treatments();
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        // A temporary renderer restriction must not rewrite compatible saved
        // settings. It can still cause the runtime's ordinary pixel fallback.
        draft.edit("presentation", "hyllian".into());
        s.cli.insert("renderer".into(), "software".into());
        draft.normalize_treatments();
        assert_eq!(draft.game_values().presentation, Presentation::Hyllian);
        s.commit(draft).unwrap();
        assert_eq!(s.saved.game_values().renderer, Renderer::Auto);
    }

    #[test]
    fn merging_two_sessions_preserves_comments_other_games_and_unknown_keys() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path,"# user note\nschema_version=1\n[global.display]\nvsync=true # keep this\nfuture='keep'\n[entries.alley-cat.display]\nscale=3\n").unwrap();
        let mut a = session(path.clone(), "alley-cat");
        let mut b = session(path.clone(), "mach-3");
        let mut ad = a.saved.clone();
        ad.edit("scale", "5".into());
        a.commit(ad).unwrap();
        let mut bd = b.saved.clone();
        bd.edit("vsync", "false".into());
        bd.edit("renderer", "software".into());
        b.commit(bd).unwrap();
        let result = fs::read_to_string(&path).unwrap();
        assert!(
            result.contains("# user note")
                && result.contains("# keep this")
                && result.contains("future='keep'")
        );
        assert_eq!(
            session(path.clone(), "alley-cat").saved.game_values().scale,
            Some(5)
        );
        assert_eq!(
            session(path, "mach-3").saved.game_values().renderer,
            Renderer::Software
        );
    }
    #[test]
    fn malformed_files_are_preserved_and_invalid_fields_do_not_hide_valid_fields() {
        let dir = TestDir::new();
        let path = dir.file();
        let bad = "[broken\n";
        fs::write(&path, bad).unwrap();
        let mut s = session(path.clone(), "mach-3");
        assert!(s.warning.is_some());
        assert!(s.commit(Draft::default()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), bad);
        fs::write(
            &path,
            "[entries.mach-3.display]\nscale=99\nvsync=false # valid with comment\nrenderer='software'\n",
        )
        .unwrap();
        let s = session(path, "mach-3");
        assert!(s.warning.is_some());
        assert_eq!(s.saved.game_values().scale, None);
        assert!(!s.saved.game_values().vsync);
        assert_eq!(s.saved.game_values().renderer, Renderer::Software);
    }
    #[test]
    fn writer_contention_is_reported_and_can_be_retried() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        let mut d = s.saved.clone();
        d.edit("scale", "4".into());
        assert!(s.commit(d.clone()).is_err());
        assert!(!path.exists());
        drop(lock);
        s.commit(d).unwrap();
        assert_eq!(session(path, "mach-3").saved.game_values().scale, Some(4));
    }
    #[test]
    fn loading_ignores_globals_and_old_sections_without_rewriting() {
        let dir = TestDir::new();
        let path = dir.file();
        let text = concat!(
            "# Old settings are left for manual cleanup, not migrated.\n",
            "[global.display]\nscale=8\nwindow_mode='fullscreen'\n",
            "[global.audio]\nmaster_volume=15\n",
            "[entries.copy.display]\nvsync=false\nmotion='original'\n",
            "[entries.copy.controls]\ncontrol_mode='original'\n",
            "[entries.copy.launcher]\nsound='pc-speaker'\n",
        );
        fs::write(&path, text).unwrap();
        for (game, entry) in [
            ("alley-cat", Some("copy")),
            ("centurion-defender-of-rome", Some("copy")),
            ("mach-3", None),
        ] {
            let current = Session::load(Launch {
                game_id: game.into(),
                entry_id: entry.map(str::to_owned),
                path: Some(path.clone()),
                cli: Patch::new(),
            });
            let value = current.effective(&current.saved);
            assert_eq!(value.scale, None);
            assert!(!value.fullscreen);
            assert_eq!(value.audio.volume, 80);
            assert_eq!(value.audio.renderer, SoundRenderer::GameDefault);
            assert_eq!(value.alleycat.motion, AlleyCatMotion::Reforged);
            assert_eq!(value.alleycat.control_mode, ControlMode::Responsive);
            assert_eq!(value.vsync, entry.is_none());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn copies_use_their_own_preferences_and_reset_to_built_in_defaults() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[global.display]\nscale=8\n[entries.first.display]\nscale=3\n[entries.second.display]\nscale=5\n").unwrap();
        let load = |entry: &str| {
            Session::load(Launch {
                game_id: "alley-cat".into(),
                entry_id: Some(entry.into()),
                path: Some(path.clone()),
                cli: Patch::new(),
            })
        };
        let mut first = load("first");
        assert_eq!(first.saved.game_values().scale, Some(3));
        assert_eq!(load("second").saved.game_values().scale, Some(5));
        assert_eq!(load("new-copy").saved.game_values().scale, None);
        let mut draft = first.saved.clone();
        draft.edit("scale", "6".into());
        draft.edit("master_volume", "45".into());
        draft.edit(STICK_DEAD_ZONE, "55".into());
        assert_eq!(
            load("first").saved.game_values().scale,
            Some(3),
            "draft is not saved"
        );
        first.commit(draft).unwrap();
        let mut draft = first.saved.clone();
        draft.reset_section(0);
        first.commit(draft).unwrap();
        let value = load("first").saved.game_values();
        assert_eq!(value.scale, None);
        assert_eq!(value.audio.volume, 45);
        assert_eq!(value.stick_dead_zone, 55);
        assert_eq!(load("second").saved.game_values().scale, Some(5));
    }

    #[test]
    fn audio_display_and_crt_round_trip_with_temporary_cli_values() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut current = session(path.clone(), "mach-3");
        let mut draft = current.saved.clone();
        for (key, value) in [
            ("scale", "4"),
            ("aspect_ratio", "square"),
            ("scaling_policy", "fit"),
            ("sampling", "linear"),
            ("master_volume", "65"),
            ("music_volume", "60"),
            ("speech_volume", "20"),
            ("effects_volume", "0"),
            ("mute_unfocused", "false"),
            ("output_device", "Test \"speakers\""),
            ("audio_renderer", "enhanced-pcm"),
            ("presentation", "hyllian"),
            ("input_gamma", "2.60"),
            ("mask_intensity", "0.20"),
        ] {
            draft.edit(key, value.into());
        }
        current.cli.insert("scale".into(), "8".into());
        current.cli.insert("music_volume".into(), "25".into());
        assert_eq!(current.effective(&draft).scale, Some(8));
        assert_eq!(
            current.effective(&draft).audio.categories,
            CategoryVolumes([25, 20, 0])
        );
        current.commit(draft.clone()).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(loaded.saved, draft);
        assert_eq!(loaded.saved.game_values().scale, Some(4));
        assert_eq!(
            loaded.saved.game_values().audio.categories,
            CategoryVolumes([60, 20, 0])
        );
        draft.reset_section(2);
        assert_eq!(draft.game_values().audio, RuntimeSettings::default().audio);
        assert_eq!(draft.game_values().scale, Some(4));
        draft.reset_section(0);
        assert_eq!(draft.game_values().aspect_ratio, AspectRatio::Original);
        assert_eq!(draft.game_values().presentation, Presentation::Hyllian);
        draft.reset_section(1);
        assert_eq!(draft, Draft::default());
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("[global"));
        assert!(!text.contains("scale = 8"));
    }

    #[test]
    fn invalid_entry_fields_fall_back_without_hiding_valid_choices() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[entries.mach-3.crt]\nbeam_min=1.5\nbeam_max=1.1\nmask_intensity=999.0\noutput_gamma=2.6\n[entries.mach-3.controls]\nstick_dead_zone=91\n").unwrap();
        let original = fs::read(&path).unwrap();
        let current = session(path.clone(), "mach-3");
        assert!(current.warning.is_some());
        let value = current.saved.game_values();
        assert!(value.hyllian.validate().is_ok());
        assert_eq!(value.get("output_gamma"), "2.60");
        assert_eq!(
            value.stick_dead_zone,
            RuntimeSettings::default().stick_dead_zone
        );
        assert_eq!(fs::read(path).unwrap(), original);
        for (key, bad) in [
            (STICK_DEAD_ZONE, "4"),
            ("scaling_policy", "crop"),
            ("sampling", "cubic"),
            ("aspect_ratio", "4:3"),
            ("input_gamma", "NaN"),
            ("music_volume", "101"),
        ] {
            assert!(RuntimeSettings::default().set(key, bad).is_err());
        }
    }
}
