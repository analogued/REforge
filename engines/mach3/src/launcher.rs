// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Engine-owned launcher choices and clean, static native scene previews.

use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};

// Mode 6 uses one-bit pixels, not the synthetic four-grey mockup palette.
const MONO: [[u8; 3]; 2] = [[0, 0, 0], [255, 255, 255]];

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
            },
            Choice {
                value: "mono",
                label: "Monochrome",
                short_label: "Mono",
                badge: Some("Mono"),
                palette: Some(&MONO),
            },
        ],
        default: "cga",
        storage: Storage::Native {
            key: "video",
            argument: "--video",
        },
    },
    OptionDefinition {
        label: "Audio",
        choices: &[
            Choice {
                value: "speaker",
                label: "PC speaker",
                short_label: "Speaker",
                badge: Some("Speaker"),
                palette: None,
            },
            Choice {
                value: "enhanced-pcm",
                label: "Enhanced PCM",
                short_label: "PCM",
                badge: Some("PCM"),
                palette: None,
            },
        ],
        default: "speaker",
        storage: Storage::Shared("audio_renderer"),
    },
    OptionDefinition {
        label: "Samples",
        choices: &[
            Choice {
                value: "true",
                label: "On",
                short_label: "On",
                badge: None,
                palette: None,
            },
            Choice {
                value: "false",
                label: "Off",
                short_label: "Off",
                badge: None,
                palette: None,
            },
        ],
        default: "true",
        storage: Storage::Shared("digitized_samples"),
    },
    DISPLAY,
    WINDOW,
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

pub const PREVIEW_OPTION: &str = "video";
/// Change when the representative scene recipe or native rendering changes.
pub const PREVIEW_REVISION: u32 = 1;
pub const PREVIEW_VARIANTS: &[&str] = &["cga", "mono"];

/// Read exactly the detector-selected files, including case-colliding names.
/// This static scene neither starts gameplay nor opens audio or a window.
pub fn preview(
    files: &std::collections::BTreeMap<&str, std::path::PathBuf>,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    let frames = crate::startup::StartupFrames::load_with(|name| {
        let path = files.get(name).ok_or_else(|| {
            crate::Error::InvalidData(format!("Missing verified preview resource {name}"))
        })?;
        std::fs::read(path).map_err(crate::Error::Io)
    })
    .map_err(|e| e.to_string())?;
    let frame = frames.frame(crate::startup::Screen::InitialScene);
    let (width, rgb) = match variant {
        "cga" => (320, frame.rgb()),
        "mono" => (
            640,
            crate::viewer::display_rgb(frame, crate::gameplay::DisplayMode::Monochrome, 1),
        ),
        _ => return Err("Unknown preview video mode".into()),
    };
    reforge_sdl::preview::Frame::new(
        reforge_sdl::DisplayGeometry::new((width, 200), (320, 240)),
        rgb.chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
