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
pub use audio::{AudioSettings, CategoryVolumes, SoundCategory, SoundRenderer};

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
pub const SECTION_KEYS: [&[&str]; 4] = [DISPLAY_KEYS, CRT_KEYS, AUDIO_KEYS, CONTROL_KEYS];
pub const DIGITIZED_SAMPLES: &str = "digitized_samples";
pub const CONTROL_MODE: &str = "control_mode";
fn game_keys(game: &str) -> impl Iterator<Item = &'static str> {
    match game {
        "mach-3" => Some(DIGITIZED_SAMPLES),
        "alley-cat" => Some(CONTROL_MODE),
        _ => None,
    }
    .into_iter()
}
pub fn section_index(key: &str) -> usize {
    if key == DIGITIZED_SAMPLES {
        return 2;
    }
    if key == CONTROL_MODE {
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

fn section(key: &str) -> &'static str {
    if key == DIGITIZED_SAMPLES {
        "gameplay"
    } else if key == CONTROL_MODE || CONTROL_KEYS.contains(&key) {
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

/// Mach 3's native sample switch changes execution, rather than host output.
/// It has no global counterpart and is read by the engine only at startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mach3Settings {
    pub digitized_samples: bool,
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
/// Alley Cat input policy; it has no global or other-game counterpart.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AlleyCatSettings {
    pub control_mode: ControlMode,
}
impl Default for Mach3Settings {
    fn default() -> Self {
        Self {
            digitized_samples: true,
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
            CONTROL_MODE => self.alleycat.control_mode.as_str().into(),
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
            CONTROL_MODE => self.alleycat.control_mode = value.parse()?,
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
    pub global: Patch,
    pub game: Patch,
}
impl Draft {
    pub fn global_values(&self) -> RuntimeSettings {
        let mut values = overlay(RuntimeSettings::default(), &self.global);
        // Terminal play is an engine-specific startup selection, never a
        // graphics default inherited by unrelated games or the launcher.
        if values.renderer == Renderer::Terminal {
            values.renderer = Renderer::Auto;
        }
        values.mach3 = Mach3Settings::default();
        values.alleycat = AlleyCatSettings::default();
        values
    }
    pub fn game_values(&self) -> RuntimeSettings {
        overlay(self.global_values(), &self.game)
    }
    /// Resolve incompatible treatment preferences at menu Apply, before CLI
    /// overrides. Game edits leave global defaults alone; global edits also
    /// reconcile this game's inherited choices without touching other games.
    pub fn normalize_treatments(&mut self, saved: &Self) {
        let global = self.global_values();
        if self.global != saved.global
            && global.presentation == Presentation::Hyllian
            && !global.supports_hyllian()
        {
            self.edit(false, "presentation", "pixels".into());
        }
        let game = self.game_values();
        if game.renderer != Renderer::Terminal
            && game.presentation == Presentation::Hyllian
            && !game.supports_hyllian()
        {
            self.edit(true, "presentation", "pixels".into());
        }
    }
    pub fn reset_section(&mut self, game: bool, page: usize) {
        let patch = if game {
            &mut self.game
        } else {
            &mut self.global
        };
        patch.retain(|key, _| !SECTION_KEYS[page].contains(&key.as_str()));
        if game && page == 2 {
            patch.remove(DIGITIZED_SAMPLES);
        }
        if game && page == 3 {
            patch.remove(CONTROL_MODE);
        }
    }
    /// Native game options can share a menu page without acquiring global keys.
    pub fn has_game_overrides(&self, page: usize) -> bool {
        SECTION_KEYS[page]
            .iter()
            .any(|key| self.game.contains_key(*key))
            || page == 2 && self.game.contains_key(DIGITIZED_SAMPLES)
            || page == 3 && self.game.contains_key(CONTROL_MODE)
    }
    pub fn edit(&mut self, game: bool, key: &str, value: String) {
        if !game && matches!(key, DIGITIZED_SAMPLES | CONTROL_MODE) {
            return;
        }
        if !game && key == "renderer" && value == "terminal" {
            return;
        }
        let base = if game {
            self.global_values()
        } else {
            RuntimeSettings::default()
        };
        let patch = if game {
            &mut self.game
        } else {
            &mut self.global
        };
        if value == base.get(key) {
            patch.remove(key);
        } else {
            patch.insert(key.into(), value);
        }
    }
    /// Settings edits save differences, while returning to an original explicit
    /// override restores its presence even when it matches the current global.
    /// Pass None after an explicit section reset so cleared overrides stay clear.
    pub fn edit_game(&mut self, key: &str, value: String, original: Option<&Self>) {
        if original.and_then(|draft| draft.game.get(key)) == Some(&value) {
            self.game.insert(key.into(), value);
        } else {
            self.edit(true, key, value);
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
                    global: scale
                        .map(|n| BTreeMap::from([("scale".into(), n.to_string())]))
                        .unwrap_or_default(),
                    game: Patch::new(),
                },
                warning: None,
                blocked: false,
            },
        }
    }
    pub fn load(launch: Launch) -> Self {
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
    pub fn effective(&self, draft: &Draft) -> RuntimeSettings {
        let mut values = overlay(draft.game_values(), &self.cli);
        if self.game_id != "mindwheel" && values.renderer == Renderer::Terminal {
            values.renderer = Renderer::Auto;
        }
        if self.game_id != "mach-3" {
            values.mach3 = Mach3Settings::default();
        }
        if self.game_id != "alley-cat" {
            values.alleycat = AlleyCatSettings::default();
        }
        values
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), String> {
        if self.blocked {
            return Err("preferences were applied, but the invalid/unreadable configuration was preserved; repair it before saving".into());
        }
        if let Some(path) = &self.path {
            save_changes(
                path,
                &self.game_id,
                self.entry_id.as_deref(),
                &self.saved,
                &draft,
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
                for name in ["display", "crt", "audio", "controls"]
                    .into_iter()
                    .chain((game == "mach-3").then_some("gameplay"))
                {
                    if table.get(name).is_some_and(|v| !v.is_table_like()) {
                        *warning = Some(format!("Invalid {name} section; using inherited values"));
                    }
                }
                for key in keys().chain(game_keys(game)) {
                    if let Some(v) = table.get(section(key)).and_then(|x| x.get(key)) {
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
                        if validated.set(key, &text).is_ok()
                            && !(key == "renderer"
                                && validated.renderer == Renderer::Terminal
                                && game != "mindwheel")
                        {
                            p.insert(key.into(), validated.get(key));
                        } else {
                            *warning =
                                Some(format!("Invalid setting {key}; using inherited value"));
                        }
                    }
                }
            } else {
                *warning = Some("Invalid display section; using inherited values".into());
            }
        }
        p
    }
    let mut draft = Draft {
        global: patch(doc.get("global"), "", warning),
        game: patch(
            entry.and_then(|id| doc.get("entries").and_then(|x| x.get(id))),
            game,
            warning,
        ),
    };
    // Cross-field validation follows inheritance, so a sparse beam override is
    // checked against its inherited partner, not an unrelated built-in default.
    if draft.global_values().hyllian.validate().is_err() {
        draft.global.remove("beam_min");
        draft.global.remove("beam_max");
        *warning = Some("Invalid global beam widths; using built-in widths".into());
    }
    if draft.game_values().hyllian.validate().is_err() {
        draft.game.remove("beam_min");
        draft.game.remove("beam_max");
        *warning = Some("Invalid game beam widths; using global widths".into());
    }
    draft
}
fn save_changes(
    path: &Path,
    game: &str,
    entry: Option<&str>,
    old: &Draft,
    new: &Draft,
) -> Result<(), String> {
    edit_document(path, "settings", |doc| {
        doc["schema_version"] = toml_edit::value(2);
        for (before, after, route) in [
            (&old.global, &new.global, vec!["global"]),
            (&old.game, &new.game, vec!["entries", entry.unwrap_or("")]),
        ] {
            if route.len() == 2 && entry.is_none() {
                continue;
            }
            let scope_game = if route.len() == 2 { game } else { "" };
            for key in keys().chain(game_keys(scope_game)) {
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
                if let Some(text) = after.get(key) {
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
    fn explicit_matching_overrides_survive_unrelated_and_global_saves() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[global.display]\nwindow_mode = \"windowed\"\n[entries.alley-cat.display]\nwindow_mode = \"windowed\"\n[entries.mach-3.display]\nwindow_mode = \"fullscreen\"\n").unwrap();
        let mut current = session(path.clone(), "alley-cat");
        let original = current.saved.clone();
        let mut draft = original.clone();
        draft.edit_game("vsync", "false".into(), Some(&original));
        current.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "alley-cat");
        assert_eq!(loaded.saved.game["window_mode"], "windowed");
        assert_eq!(loaded.saved.game["vsync"], "false");
        for mode in ["fullscreen", "windowed", "fullscreen"] {
            let mut draft = loaded.saved.clone();
            draft.edit(false, "window_mode", mode.into());
            loaded.commit(draft).unwrap();
            loaded = session(path.clone(), "alley-cat");
            assert_eq!(loaded.saved.game["window_mode"], "windowed");
            assert!(!loaded.saved.game_values().fullscreen);
        }
        assert_eq!(
            session(path, "mach-3").saved.game["window_mode"],
            "fullscreen"
        );
    }

    #[test]
    fn game_edits_restore_original_presence_but_resets_do_not() {
        for (key, initial, other) in [
            ("window_mode", "windowed", "fullscreen"),
            ("master_volume", "80", "60"),
            (DIGITIZED_SAMPLES, "true", "false"),
            (CONTROL_MODE, "responsive", "original"),
        ] {
            let original = Draft {
                game: Patch::from([(key.into(), initial.into())]),
                ..Default::default()
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
            draft.reset_section(true, section_index(key));
            draft.edit_game(key, other.into(), None);
            draft.edit_game(key, initial.into(), None);
            assert!(draft.game.is_empty(), "reset {key} must not return");
        }
        let original = Draft::default();
        let mut draft = original.clone();
        draft.edit(false, "window_mode", "fullscreen".into());
        draft.edit_game("window_mode", "windowed".into(), Some(&original));
        assert_eq!(
            draft.game["window_mode"], "windowed",
            "game edits compare against the pending global, not the original resolved value"
        );
    }

    #[test]
    fn controller_dead_zone_inherits_overrides_resets_and_validates() {
        use super::*;
        let doc: DocumentMut = "[global.controls]\nstick_dead_zone = 40\n[entries.mach-3.controls]\nstick_dead_zone = 20\n[entries.alley-cat.controls]\nstick_dead_zone = 55\n".parse().unwrap();
        let mut warning = None;
        let mut mach = read_draft(&doc, "mach-3", Some("mach-3"), &mut warning);
        assert!(warning.is_none());
        assert_eq!(mach.global_values().stick_dead_zone, 40);
        assert_eq!(mach.game_values().stick_dead_zone, 20);
        assert!(mach.has_game_overrides(3));
        mach.reset_section(true, 3);
        assert_eq!(mach.game_values().stick_dead_zone, 40);
        assert!(!mach.has_game_overrides(3));
        let alley = read_draft(&doc, "alley-cat", Some("alley-cat"), &mut warning);
        assert_eq!(alley.game_values().stick_dead_zone, 55);
        assert_eq!(
            read_draft(&doc, "another-game", Some("another-game"), &mut warning)
                .game_values()
                .stick_dead_zone,
            40
        );
        let mut settings = RuntimeSettings::default();
        for valid in [5, 35, 90] {
            settings.set(STICK_DEAD_ZONE, &valid.to_string()).unwrap();
        }
        for invalid in ["0", "4", "91", "256", "-1", "NaN", "0.35"] {
            assert!(settings.set(STICK_DEAD_ZONE, invalid).is_err());
        }
    }
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
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
    #[test]
    fn terminal_renderer_is_game_scoped_and_round_trips_without_changing_graphics_preferences() {
        assert_eq!("terminal".parse::<Renderer>().unwrap(), Renderer::Terminal);
        assert_eq!(Renderer::Terminal.as_str(), "terminal");
        assert!("terminal".parse::<Backend>().is_err());
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            "[global.display]\nbackend = 'vulkan'\nscale = 3\n[global.crt]\npresentation = 'hyllian'\n",
        )
        .unwrap();
        let mut s = session(path.clone(), "mindwheel");
        let mut draft = s.saved.clone();
        draft.edit(true, "renderer", "terminal".into());
        draft.normalize_treatments(&s.saved);
        s.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "mindwheel");
        let terminal = loaded.effective(&loaded.saved);
        assert_eq!(terminal.renderer, Renderer::Terminal);
        assert_eq!(terminal.backend, Backend::Vulkan);
        assert_eq!(terminal.scale, Some(3));
        assert_eq!(terminal.presentation, Presentation::Hyllian);
        assert!(!terminal.supports_hyllian());
        loaded.cli.insert("renderer".into(), "gpu".into());
        assert_eq!(loaded.effective(&loaded.saved).renderer, Renderer::Gpu);
        assert_eq!(session(path.clone(), "mindwheel").saved, loaded.saved);
        let mut reset = loaded.saved.clone();
        reset.reset_section(true, 0);
        assert_eq!(reset.game_values().renderer, Renderer::Auto);
        assert_eq!(reset.game_values().presentation, Presentation::Hyllian);
    }

    #[test]
    fn terminal_renderer_cannot_become_a_global_default_or_affect_other_games() {
        let mut draft = Draft::default();
        draft.edit(false, "renderer", "terminal".into());
        assert!(!draft.global.contains_key("renderer"));
        // Explicitly authored configuration and CLI input are independently
        // checked even when they bypass the GUI's choice availability.
        draft.global.insert("renderer".into(), "terminal".into());
        assert_eq!(draft.global_values().renderer, Renderer::Auto);
        draft.game.insert("renderer".into(), "terminal".into());
        for game in ["alley-cat", "mach-3", "standalone"] {
            let mut session = Session::for_window(None);
            session.game_id = game.into();
            session.cli.insert("renderer".into(), "terminal".into());
            assert_eq!(session.effective(&draft).renderer, Renderer::Auto);
        }
        let doc: DocumentMut = "[global.display]\nrenderer = 'terminal'\n[entries.mindwheel.display]\nrenderer = 'terminal'\n[entries.mach-3.display]\nrenderer = 'terminal'\n".parse().unwrap();
        let mut warning = None;
        let mindwheel = read_draft(&doc, "mindwheel", Some("mindwheel"), &mut warning);
        assert!(warning.is_some());
        assert!(!mindwheel.global.contains_key("renderer"));
        assert_eq!(mindwheel.game_values().renderer, Renderer::Terminal);
        warning = None;
        let mach = read_draft(&doc, "mach-3", Some("mach-3"), &mut warning);
        assert!(warning.is_some());
        assert!(!mach.game.contains_key("renderer"));
        assert_eq!(mach.game_values().renderer, Renderer::Auto);
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
        draft.edit(false, CONTROL_MODE, "original".into());
        assert!(draft.global.is_empty());
        draft.edit(true, CONTROL_MODE, "original".into());
        assert!(draft.has_game_overrides(3));
        assert_eq!(section_index(CONTROL_MODE), 3);
        assert_eq!(
            cat.effective(&cat.saved).alleycat.control_mode,
            ControlMode::Responsive
        );
        // A concurrent change to another game must survive this game's Apply.
        let mut mach_draft = mach.saved.clone();
        mach_draft.edit(true, DIGITIZED_SAMPLES, "false".into());
        mach.commit(mach_draft).unwrap();
        cat.commit(draft).unwrap();
        let mut loaded = session(path.clone(), "alley-cat");
        assert_eq!(
            loaded.effective(&loaded.saved).alleycat.control_mode,
            ControlMode::Original
        );
        let mut reset = loaded.saved.clone();
        reset.reset_section(false, 3);
        reset.reset_section(true, 2);
        assert_eq!(
            reset.game_values().alleycat.control_mode,
            ControlMode::Original
        );
        reset.reset_section(true, 3);
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
            doc["entries"]["alley-cat"]["controls"]
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
            "[entries.alley-cat.controls]\ncontrol_mode = 'fast'\n",
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
        assert!(!draft.has_game_overrides(2));
        draft.edit(false, DIGITIZED_SAMPLES, "false".into());
        assert!(draft.global.is_empty());
        draft.edit(true, DIGITIZED_SAMPLES, "false".into());
        assert!(draft.has_game_overrides(2));
        assert_eq!(section_index(DIGITIZED_SAMPLES), 2);
        draft.edit(false, "sound_enabled", "false".into());
        assert!(!mach.effective(&draft).mach3.digitized_samples);
        assert!(!mach.effective(&draft).audio.enabled);
        mach.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert!(!loaded.effective(&loaded.saved).mach3.digitized_samples);
        assert!(!loaded.saved.game_values().audio.enabled);
        // Global resets leave the native override intact; resetting this game's
        // Audio section returns it to its built-in On value in the draft.
        let mut draft = loaded.saved.clone();
        draft.reset_section(true, 1);
        draft.reset_section(false, 2);
        assert!(!draft.game_values().mach3.digitized_samples);
        assert!(draft.game_values().audio.enabled);
        assert!(draft.has_game_overrides(2));
        let doc = read(&path).unwrap();
        assert_eq!(
            doc["entries"]["mach-3"]["gameplay"][DIGITIZED_SAMPLES].as_bool(),
            Some(false)
        );
        assert!(doc["entries"]["mach-3"].get("audio").is_none());
        draft.reset_section(true, 2);
        assert!(draft.game_values().mach3.digitized_samples);
        assert!(!draft.has_game_overrides(2));
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
        assert!(!reset.saved.has_game_overrides(2));
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
        cat_draft.edit(true, "scale", "2".into());
        cat.commit(cat_draft).unwrap();
        let mut draft = mach.saved.clone();
        draft.edit(true, "master_volume", "65".into());
        mach.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert!(loaded.effective(&loaded.saved).mach3.digitized_samples);
        assert_eq!(
            session(path.clone(), "alley-cat").saved.game_values().scale,
            Some(2)
        );
        let mut draft = mach.saved.clone();
        draft.edit(true, DIGITIZED_SAMPLES, "false".into());
        mach.commit(draft).unwrap();
        let mut draft = mach.saved.clone();
        draft.reset_section(true, 2);
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
    fn category_levels_round_trip_inherit_and_reset_without_persisting_cli() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        assert_eq!(s.saved.game_values().audio.volume, 80);
        assert_eq!(
            s.saved.game_values().audio.categories,
            CategoryVolumes([100; 3])
        );
        let mut draft = s.saved.clone();
        draft.edit(false, "music_volume", "60".into());
        draft.edit(false, "speech_volume", "70".into());
        draft.edit(true, "speech_volume", "20".into());
        draft.edit(true, "effects_volume", "0".into());
        s.cli.insert("music_volume".into(), "25".into());
        assert_eq!(
            s.effective(&draft).audio.categories,
            CategoryVolumes([25, 20, 0])
        );
        s.commit(draft.clone()).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(
            loaded.saved.game_values().audio.categories,
            CategoryVolumes([60, 20, 0])
        );
        draft.edit(true, "speech_volume", "70".into());
        assert!(!draft.game.contains_key("speech_volume"));
        draft.reset_section(false, 2);
        assert_eq!(
            draft.global_values().audio.categories,
            CategoryVolumes([100; 3])
        );
        assert_eq!(
            draft.game_values().audio.categories,
            CategoryVolumes([100, 100, 0])
        );
        draft.reset_section(true, 2);
        assert!(draft.game.is_empty());
        for category in SoundCategory::ALL {
            for bad in ["101", "-1", "NaN", "50.5"] {
                assert!(RuntimeSettings::default().set(category.key(), bad).is_err());
            }
        }
    }

    #[test]
    fn audio_inherits_sparsely_resets_independently_and_keeps_cli_temporary() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let mut d = s.saved.clone();
        d.edit(false, "scale", "4".into());
        d.edit(false, "master_volume", "65".into());
        d.edit(false, "output_device", "Test \"speakers\"".into());
        d.edit(true, "audio_renderer", "enhanced-pcm".into());
        d.edit(true, "mute_unfocused", "false".into());
        s.cli.insert("audio_renderer".into(), "speaker".into());
        assert_eq!(s.effective(&d).audio.renderer, SoundRenderer::Speaker);
        s.commit(d.clone()).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(loaded.saved, d);
        assert_eq!(
            loaded.saved.game_values().audio.renderer,
            SoundRenderer::EnhancedPcm
        );
        assert_eq!(loaded.saved.game_values().audio.volume, 65);
        d.reset_section(false, 2);
        assert_eq!(d.global_values().scale, Some(4));
        assert_eq!(d.game_values().audio.volume, 80);
        assert_eq!(d.game.len(), 2);
        d.reset_section(true, 2);
        assert!(d.game.is_empty());
        let mut value = RuntimeSettings::default();
        for bad in ["-1", "101", "NaN", "50.1"] {
            assert!(value.set("master_volume", bad).is_err());
        }
        assert!(value.set("output_device", "").is_err());
    }
    #[test]
    fn fitting_and_sampling_persist_inherit_and_reset_without_clearing_aspect() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut saved_session = session(path.clone(), "mach-3");
        let mut draft = saved_session.saved.clone();
        draft.edit(false, "scaling_policy", "fit".into());
        draft.edit(false, "sampling", "linear".into());
        draft.edit(true, "aspect_ratio", "square".into());
        draft.edit(true, "scaling_policy", "stretch".into());
        saved_session.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        let value = loaded.saved.game_values();
        assert_eq!(value.scaling_policy, ScalingPolicy::Stretch);
        assert_eq!(value.sampling, Sampling::Linear);
        assert_eq!(value.aspect_ratio, AspectRatio::Square);
        let mut draft = loaded.saved.clone();
        draft.edit(true, "scaling_policy", "fit".into());
        assert_eq!(draft.game_values().aspect_ratio, AspectRatio::Square);
        assert!(!draft.game.contains_key("scaling_policy"));
        draft.reset_section(true, 0);
        assert_eq!(draft.game_values().scaling_policy, ScalingPolicy::Fit);
        assert_eq!(draft.game_values().aspect_ratio, AspectRatio::Original);
        draft.reset_section(false, 0);
        assert_eq!(draft.game_values().scaling_policy, ScalingPolicy::Integer);
        assert_eq!(draft.game_values().sampling, Sampling::Nearest);
        let text = fs::read_to_string(path).unwrap();
        for setting in [
            "scaling_policy = \"fit\"",
            "scaling_policy = \"stretch\"",
            "sampling = \"linear\"",
        ] {
            assert!(text.contains(setting));
        }
        for (key, invalid) in [("scaling_policy", "crop"), ("sampling", "cubic")] {
            assert!(RuntimeSettings::default().set(key, invalid).is_err());
        }
    }

    #[test]
    fn aspect_preference_round_trips_without_persisting_an_engine_ratio() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        assert_eq!(s.saved.game_values().aspect_ratio, AspectRatio::Original);
        let mut d = s.saved.clone();
        d.edit(false, "aspect_ratio", "square".into());
        d.edit(true, "aspect_ratio", "original".into());
        s.commit(d).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(
            loaded.saved.global_values().aspect_ratio,
            AspectRatio::Square
        );
        assert_eq!(
            loaded.saved.game_values().aspect_ratio,
            AspectRatio::Original
        );
        assert_eq!(
            session(path.clone(), "alley-cat")
                .saved
                .game_values()
                .aspect_ratio,
            AspectRatio::Square
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("aspect_ratio = \"square\""));
        assert!(text.contains("aspect_ratio = \"original\""));
        assert!(!text.contains("4:3"));
        let mut d = loaded.saved.clone();
        d.reset_section(true, 0);
        assert_eq!(d.game_values().aspect_ratio, AspectRatio::Square);
        let mut settings = RuntimeSettings::default();
        for invalid in ["4:3", "16:9", "square-pixels", ""] {
            assert!(settings.set("aspect_ratio", invalid).is_err());
        }
    }

    #[test]
    fn sections_round_trip_sparse_parameters_and_reset_independently() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let mut d = s.saved.clone();
        d.edit(false, "frame_limit", "120".into());
        d.edit(false, "presentation", "hyllian".into());
        d.edit(false, "input_gamma", "2.60".into());
        d.edit(true, "mask_intensity", "0.20".into());
        d.edit(true, "frame_limit", "unlimited".into());
        s.commit(d.clone()).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(loaded.saved, d);
        assert_eq!(loaded.saved.game_values().get("input_gamma"), "2.60");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[global.crt]"));
        assert!(text.contains("[entries.mach-3.crt]"));
        d.reset_section(false, 0);
        assert_eq!(d.global_values().presentation, Presentation::Hyllian);
        assert_eq!(d.game_values().frame_limit, FrameLimit::Unlimited);
        d.reset_section(true, 1);
        assert_eq!(d.game.len(), 1);
        s.commit(d).unwrap();
        assert_eq!(
            session(path, "mach-3").saved.game_values().hyllian.values[5],
            35
        );
    }
    #[test]
    fn invalid_shader_fields_and_inherited_beam_pair_fall_back() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[global.crt]\nbeam_min=0.8\nbeam_max=1.1\nmask_intensity=999.0\noutput_gamma=2.6\n[entries.mach-3.crt]\nbeam_min=1.5\n").unwrap();
        let s = session(path, "mach-3");
        assert!(s.warning.is_some());
        let h = s.saved.game_values().hyllian;
        assert_eq!(h.values, [65, 80, 110, 200, 100, 35, 100, 240, 260]);
        assert!(h.validate().is_ok());
        let mut value = RuntimeSettings::default();
        for bad in ["NaN", "inf", "-1", "0.99", "4.01"] {
            assert!(value.set("input_gamma", bad).is_err());
        }
    }
    #[test]
    fn sparse_game_edits_track_global_and_remove_equal_values() {
        let mut d = Draft::default();
        d.edit(false, "scale", "4".into());
        assert_eq!(d.game_values().scale, Some(4));
        assert!(d.game.is_empty());
        d.edit(true, "scale", "8".into());
        d.edit(false, "vsync", "false".into());
        assert!(!d.game_values().vsync);
        assert_eq!(d.game.len(), 1);
        d.edit(true, "scale", "4".into());
        assert!(d.game.is_empty());
    }
    #[test]
    fn temporary_cli_is_not_saved_and_global_reset_keeps_game_entries() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            "schema_version=1\n[global.display]\nscale=4\n[entries.mach-3.display]\nvsync=false\n",
        )
        .unwrap();
        let mut s = session(path.clone(), "mach-3");
        s.cli.insert("scale".into(), "8".into());
        assert_eq!(s.effective(&s.saved).scale, Some(8));
        let mut d = s.saved.clone();
        d.global.clear();
        s.commit(d).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(loaded.saved.game_values().scale, None);
        assert!(!loaded.saved.game_values().vsync);
        assert!(!fs::read_to_string(path).unwrap().contains("scale = 8"));
    }
    #[test]
    fn treatment_reset_is_saved_per_game_and_requires_reselection() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let mut draft = s.saved.clone();
        draft.edit(false, "presentation", "hyllian".into());
        s.commit(draft.clone()).unwrap();
        draft.edit(true, "backend", "opengl".into());
        draft.normalize_treatments(&s.saved);
        assert_eq!(draft.global_values().presentation, Presentation::Hyllian);
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        assert_eq!(draft.game.get("presentation").unwrap(), "pixels");
        s.commit(draft).unwrap();

        let mut s = session(path, "mach-3");
        let mut draft = s.saved.clone();
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        draft.edit(true, "backend", "auto".into());
        draft.normalize_treatments(&s.saved);
        assert_eq!(draft.game_values().presentation, Presentation::Pixels);
        // A temporary renderer restriction must not rewrite compatible saved
        // settings. It can still cause the runtime's ordinary pixel fallback.
        draft.edit(true, "presentation", "hyllian".into());
        s.cli.insert("renderer".into(), "software".into());
        draft.normalize_treatments(&s.saved);
        assert_eq!(draft.game_values().presentation, Presentation::Hyllian);
        s.commit(draft).unwrap();
        assert_eq!(s.saved.game_values().renderer, Renderer::Auto);
    }

    #[test]
    fn global_treatment_reset_preserves_compatible_game_overrides() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path, "[global.crt]\npresentation='hyllian'\n[entries.mach-3.display]\nrenderer='gpu'\n[entries.mach-3.crt]\npresentation='hyllian'\n[entries.alley-cat.crt]\npresentation='hyllian'\n").unwrap();
        let mut s = session(path.clone(), "mach-3");
        let mut draft = s.saved.clone();
        draft.edit(false, "renderer", "sdl".into());
        draft.normalize_treatments(&s.saved);
        assert_eq!(draft.global_values().presentation, Presentation::Pixels);
        assert_eq!(draft.game_values().presentation, Presentation::Hyllian);
        s.commit(draft).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(
            loaded.saved.global_values().presentation,
            Presentation::Pixels
        );
        assert_eq!(
            loaded.saved.game_values().presentation,
            Presentation::Hyllian
        );
        let other = session(path, "alley-cat");
        assert_eq!(
            other.saved.game_values().presentation,
            Presentation::Hyllian
        );

        let mut inherited = Draft::default();
        inherited.edit(false, "presentation", "hyllian".into());
        inherited.edit(true, "renderer", "software".into());
        inherited.normalize_treatments(&Draft::default());
        assert_eq!(
            inherited.global_values().presentation,
            Presentation::Hyllian
        );
        assert_eq!(inherited.game_values().presentation, Presentation::Pixels);
    }

    #[test]
    fn merging_two_sessions_preserves_comments_other_games_and_unknown_keys() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path,"# user note\nschema_version=1\n[global.display]\nvsync=true # keep this\nfuture='keep'\n[entries.alley-cat.display]\nscale=3\n").unwrap();
        let mut a = session(path.clone(), "alley-cat");
        let mut b = session(path.clone(), "mach-3");
        let mut ad = a.saved.clone();
        ad.edit(true, "scale", "5".into());
        a.commit(ad).unwrap();
        let mut bd = b.saved.clone();
        bd.edit(false, "vsync", "false".into());
        bd.edit(true, "renderer", "software".into());
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
            "[global.display]\nscale=99\nvsync=false # valid with comment\nrenderer='software'\n",
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
        d.edit(true, "scale", "4".into());
        assert!(s.commit(d.clone()).is_err());
        assert!(!path.exists());
        drop(lock);
        s.commit(d).unwrap();
        assert_eq!(session(path, "mach-3").saved.game_values().scale, Some(4));
    }
}
