// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-owned video choices, scene previews and display geometry.
use crate::assets::DosRelease;
use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, PaletteHint, Storage, WINDOW};
#[path = "launcher_palette.rs"]
mod palette;

/// DOS 1.00A's supported distribution contains VGA artwork only.
pub const OPTIONS: &[OptionDefinition] = &[DISPLAY, WINDOW];
const OPTIONS_100B: &[OptionDefinition] = &[
    OptionDefinition {
        label: "Video",
        choices: &[
            Choice {
                value: "vga",
                label: "VGA",
                short_label: "VGA",
                badge: Some("VGA"),
                palette: None,
                palette_hint: PaletteHint::Auto,
            },
            Choice {
                value: "ega",
                label: "EGA",
                short_label: "EGA",
                badge: Some("EGA"),
                palette: None,
                palette_hint: PaletteHint::Auto,
            },
        ],
        default: "vga",
        storage: Storage::Native {
            key: "video",
            argument: "--video",
        },
    },
    DISPLAY,
    WINDOW,
];

pub const fn options(release: DosRelease) -> &'static [OptionDefinition] {
    match release {
        DosRelease::V100A => OPTIONS,
        DosRelease::V100B => OPTIONS_100B,
    }
}

pub const PREVIEW_OPTION: &str = "video";
pub const PREVIEW_VARIANT: &str = "vga";
pub const PREVIEW_VARIANTS: &[&str] = &[PREVIEW_VARIANT];
const PREVIEW_VARIANTS_100B: &[&str] = &["vga", "ega"];
pub const PREVIEW_REVISION: u32 = 2;

pub const fn preview_variants(release: DosRelease) -> &'static [&'static str] {
    match release {
        DosRelease::V100A => PREVIEW_VARIANTS,
        DosRelease::V100B => PREVIEW_VARIANTS_100B,
    }
}

pub fn geometry(_: &std::collections::BTreeMap<String, String>) -> reforge_sdl::DisplayGeometry {
    reforge_sdl::DisplayGeometry::new((320, 200), (320, 240))
}

pub fn preview(
    files: &reforge_inputs::ResolvedFiles,
    variant: &str,
    release: DosRelease,
) -> Result<reforge_sdl::preview::Frame, String> {
    if !preview_variants(release).contains(&variant) {
        return Err("Unknown Centurion preview variant for this release".into());
    }
    let video = crate::cli::parse_video(variant)?;
    let assets = crate::assets::Assets::load(files, std::path::PathBuf::new(), release, video)?;
    let session = crate::session::Session::new(&assets, crate::session::Phase::Title, 0, 0)?;
    let frame = session.render(&assets, false)?;
    reforge_sdl::preview::Frame::new(geometry(&Default::default()), rgba(&frame, &assets))?
        .with_palette(palette::extract(&assets)?)
}

pub(crate) fn rgba(frame: &crate::graphics::Frame, assets: &crate::assets::Assets) -> Vec<u8> {
    let palette = frame.palette(assets);
    frame
        .pixels
        .iter()
        .flat_map(|index| {
            let [r, g, b] = palette[usize::from(*index)];
            [r, g, b, 255]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_choices_and_previews_follow_the_detected_release() {
        assert_eq!(preview_variants(DosRelease::V100A), ["vga"]);
        assert_eq!(preview_variants(DosRelease::V100B), ["vga", "ega"]);
        assert!(
            options(DosRelease::V100A)
                .iter()
                .all(|option| option.key() != "video")
        );
        let video = options(DosRelease::V100B)
            .iter()
            .find(|option| option.key() == "video")
            .unwrap();
        assert_eq!(video.default, "vga");
        assert_eq!(
            video
                .choices
                .iter()
                .map(|choice| choice.value)
                .collect::<Vec<_>>(),
            ["vga", "ega"]
        );
        for choice in video.choices {
            crate::cli::validate_arguments(
                ["--original", "synthetic", "--video", choice.value]
                    .into_iter()
                    .map(Into::into),
            )
            .unwrap();
        }
    }

    #[test]
    fn both_video_modes_keep_the_original_display_geometry() {
        for mode in ["vga", "ega"] {
            let native = std::collections::BTreeMap::from([("video".into(), mode.into())]);
            let geometry = geometry(&native);
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Original), (4, 3));
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Square), (8, 5));
        }
        assert_eq!(
            crate::cli::parse_video("ega").unwrap(),
            crate::assets::VideoMode::Ega
        );
    }
}
