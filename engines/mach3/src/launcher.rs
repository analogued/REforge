// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-owned launcher choices and clean, static native scene previews.

use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};
use reforge_sdl::settings::{
    ApplyTiming, AudioCapabilities, ControlCapabilities, ControllerLayouts, DisplayCapabilities,
    EngineOptions, SettingsCapabilities, SoundCategory, SoundRenderer, VideoCapabilities,
};

// Mode 6 uses one-bit pixels, not the synthetic four-grey mockup palette.
const MONO: [[u8; 3]; 2] = [[0, 0, 0], [255, 255, 255]];

/// Audio devices, default and application policy shared by launcher and player.
pub const AUDIO: AudioCapabilities = AudioCapabilities {
    renderers: &[SoundRenderer::Speaker, SoundRenderer::EnhancedPcm],
    default: SoundRenderer::Speaker,
    categories: &SoundCategory::ALL,
    apply: ApplyTiming::Live,
};

pub const OPTIONS: &[OptionDefinition] = &[
    OptionDefinition {
        label: "Video",
        choices: &[
            Choice {
                value: "cga",
                label: "CGA colour",
                short_label: "CGA",
                badge: Some("CGA"),
                palette: Some(&crate::frame::PALETTE),
                palette_hint: reforge_sdl::launcher::PaletteHint::Auto,
            },
            Choice {
                value: "mono",
                label: "Monochrome",
                short_label: "Mono",
                badge: Some("Mono"),
                palette: Some(&MONO),
                palette_hint: reforge_sdl::launcher::PaletteHint::Auto,
            },
        ],
        default: "cga",
        storage: Storage::Native {
            key: "video",
            argument: "--video",
        },
        apply: ApplyTiming::Live,
    },
    DISPLAY,
    WINDOW,
    OptionDefinition {
        label: "Audio",
        choices: &[
            Choice {
                value: "speaker",
                label: "PC speaker",
                short_label: "Speaker",
                badge: None,
                palette: None,
                palette_hint: reforge_sdl::launcher::PaletteHint::Auto,
            },
            Choice {
                value: "enhanced-pcm",
                label: "Enhanced PCM",
                short_label: "PCM",
                badge: Some("PCM"),
                palette: None,
                palette_hint: reforge_sdl::launcher::PaletteHint::Auto,
            },
        ],
        default: AUDIO.default.as_str(),
        storage: Storage::Shared("audio_renderer"),
        apply: AUDIO.apply,
    },
];

/// Native dimensions for Settings' aspect labels; both modes target a 4:3 CRT.
pub fn geometry(
    native: &std::collections::BTreeMap<String, String>,
) -> reforge_sdl::DisplayGeometry {
    let width = if native.get("video").is_some_and(|value| value == "mono") {
        crate::frame::WIDTH as u32 * 2
    } else {
        crate::frame::WIDTH as u32
    };
    reforge_sdl::DisplayGeometry::new((width, crate::frame::HEIGHT as u32), (320, 240))
}

/// Declare settings once for every host of this engine and detected release.
pub fn settings_capabilities(
    native: &std::collections::BTreeMap<String, String>,
) -> SettingsCapabilities {
    let quick_options = OPTIONS;
    SettingsCapabilities {
        game_id: "mach-3",
        audio: AUDIO,
        display: DisplayCapabilities {
            geometry: geometry(native),
            video: VideoCapabilities::DEFAULT,
            machine: None,
            mode: Some(&quick_options[0]),
            frontend: None,
        },
        controls: ControlCapabilities {
            controller_layouts: ControllerLayouts::None,
        },
        options: EngineOptions::Mach3,
        quick_options,
    }
}

pub const PREVIEW_OPTION: &str = "video";
/// Change when the representative scene recipe or native rendering changes.
pub const PREVIEW_REVISION: u32 = 2;
pub const PREVIEW_VARIANTS: &[&str] = &["cga", "mono"];

/// Read exactly the detector-selected files, including case-colliding names.
/// This static scene neither starts gameplay nor opens audio or a window.
pub fn preview(
    files: &reforge_inputs::ResolvedFiles,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    let frames =
        crate::startup::StartupFrames::load_with(|name| files.read(name).map_err(crate::Error::Io))
            .map_err(|e| e.to_string())?;
    let frame = frames.frame(crate::startup::Screen::Splash);
    let (width, rgb) = match variant {
        "cga" => (320, frame.rgb()),
        "mono" => (
            640,
            crate::platform::display_rgb(frame, crate::gameplay::DisplayMode::Monochrome, 1),
        ),
        _ => return Err("Unknown preview video mode".into()),
    };
    let geometry = reforge_sdl::DisplayGeometry::new((width, 200), (320, 240));
    let rgba = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|c| [c[0], c[1], c[2], 255])
        .collect();
    if variant == "mono" {
        reforge_sdl::preview::Frame::new_monochrome(geometry, rgba)
    } else {
        reforge_sdl::preview::Frame::new(geometry, rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_share_live_audio_and_selected_video_geometry() {
        for mode in ["cga", "mono"] {
            let native = std::collections::BTreeMap::from([("video".into(), mode.into())]);
            let caps = settings_capabilities(&native);
            assert_eq!(caps.validate(), Ok(()));
            assert_eq!(caps.display.geometry, geometry(&native));
            assert_eq!(caps.display.mode.unwrap().key(), "video");
            assert_eq!(caps.display.mode.unwrap().apply, ApplyTiming::Live);
            assert_eq!(caps.audio.default, SoundRenderer::Speaker);
            assert_eq!(caps.audio.categories, SoundCategory::ALL);
            assert_eq!(caps.audio.apply, ApplyTiming::Live);
            assert_eq!(
                caps.audio.resolve(SoundRenderer::Adlib),
                SoundRenderer::Speaker
            );
            assert_eq!(
                caps.audio.resolve(SoundRenderer::EnhancedPcm),
                SoundRenderer::EnhancedPcm
            );
            assert_eq!(
                caps.quick_options
                    .iter()
                    .map(|option| (option.key(), option.default))
                    .collect::<Vec<_>>(),
                OPTIONS
                    .iter()
                    .map(|option| (option.key(), option.default))
                    .collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn video_modes_keep_corrected_aspect_but_distinct_pixel_geometry() {
        for (mode, square) in [("cga", (8, 5)), ("mono", (16, 5))] {
            let native = std::collections::BTreeMap::from([("video".into(), mode.into())]);
            let geometry = geometry(&native);
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Original), (4, 3));
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Square), square);
        }
    }

    #[test]
    fn native_choices_are_accepted_by_engine_parser() {
        for option in OPTIONS {
            let Storage::Native { argument, .. } = option.storage else {
                continue;
            };
            for choice in option.choices {
                crate::cli::validate_arguments(
                    ["--original", "synthetic", argument, choice.value]
                        .into_iter()
                        .map(std::ffi::OsString::from),
                )
                .unwrap();
            }
        }
    }
}
