// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-owned video/sound choices, scene previews and display geometry.
use crate::assets::LoadingProfile;
use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, PaletteHint, Storage, WINDOW};
use reforge_sdl::settings::{
    ApplyTiming, ControlCapabilities, ControllerLayouts, DisplayCapabilities, EngineOptions,
    SettingsCapabilities, VideoCapabilities,
};
#[path = "launcher_palette.rs"]
mod palette;
pub use crate::audio::AUDIO;

/// Options for an installation with only verified VGA artwork.
pub const OPTIONS: &[OptionDefinition] = &[SOUND, DISPLAY, WINDOW];
const SOUND: OptionDefinition = OptionDefinition {
    label: "Sound",
    choices: &[
        Choice {
            value: "adlib",
            label: "AdLib",
            short_label: "AdLib",
            badge: None,
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
        Choice {
            value: "speaker",
            label: "PC speaker",
            short_label: "PC speaker",
            badge: Some("PC speaker"),
            palette: None,
            palette_hint: PaletteHint::Auto,
        },
    ],
    default: AUDIO.default.as_str(),
    storage: Storage::Shared("audio_renderer"),
    apply: AUDIO.apply,
};
const OPTIONS_VGA_EGA: &[OptionDefinition] = &[
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
        apply: ApplyTiming::NextLaunch,
    },
    SOUND,
    DISPLAY,
    WINDOW,
];

pub const fn options(profile: LoadingProfile) -> &'static [OptionDefinition] {
    if profile.ega {
        OPTIONS_VGA_EGA
    } else {
        OPTIONS
    }
}

/// Declare settings once for every host of this engine and detected release.
pub fn settings_capabilities(
    profile: LoadingProfile,
    native: &std::collections::BTreeMap<String, String>,
) -> SettingsCapabilities {
    let quick_options = options(profile);
    SettingsCapabilities {
        game_id: "centurion-defender-of-rome",
        audio: AUDIO,
        display: DisplayCapabilities {
            geometry: geometry(native),
            video: VideoCapabilities::DEFAULT,
            machine: None,
            mode: quick_options.iter().find(|option| option.key() == "video"),
            frontend: None,
        },
        controls: ControlCapabilities {
            gamepad: false,
            controller_layouts: ControllerLayouts::None,
        },
        options: EngineOptions::Centurion,
        quick_options,
    }
}

pub const PREVIEW_OPTION: &str = "video";
pub const PREVIEW_VARIANT: &str = "vga";
pub const PREVIEW_VARIANTS: &[&str] = &[PREVIEW_VARIANT];
const PREVIEW_VARIANTS_VGA_EGA: &[&str] = &["vga", "ega"];
pub const PREVIEW_REVISION: u32 = 2;

pub const fn preview_variants(profile: LoadingProfile) -> &'static [&'static str] {
    if profile.ega {
        PREVIEW_VARIANTS_VGA_EGA
    } else {
        PREVIEW_VARIANTS
    }
}

pub fn geometry(_: &std::collections::BTreeMap<String, String>) -> reforge_sdl::DisplayGeometry {
    reforge_sdl::DisplayGeometry::new((320, 200), (320, 240))
}

pub fn preview(
    files: &reforge_inputs::ResolvedFiles,
    variant: &str,
    profile: LoadingProfile,
) -> Result<reforge_sdl::preview::Frame, String> {
    if !preview_variants(profile).contains(&variant) {
        return Err("Unknown Centurion preview variant for this installation".into());
    }
    let video = crate::cli::parse_video(variant)?;
    let assets =
        crate::assets::Assets::load(files, std::path::PathBuf::new(), profile.release, video)?;
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
    fn release_declarations_require_verified_ega_and_restart_for_device_changes() {
        for release in [
            crate::assets::DosRelease::V100A,
            crate::assets::DosRelease::V100B,
        ] {
            for ega in [false, true] {
                let profile = LoadingProfile::new(release, ega);
                let caps = settings_capabilities(profile, &Default::default());
                assert_eq!(caps.validate(), Ok(()));
                assert!(!caps.controls.gamepad);
                assert_eq!(caps.display.mode.is_some(), ega);
                if let Some(video) = caps.display.mode {
                    assert_eq!(video.key(), "video");
                    assert_eq!(video.default, "vga");
                    assert_eq!(video.apply, ApplyTiming::NextLaunch);
                }
                assert_eq!(
                    caps.quick_options
                        .iter()
                        .map(|option| (option.key(), option.default))
                        .collect::<Vec<_>>(),
                    options(profile)
                        .iter()
                        .map(|option| (option.key(), option.default))
                        .collect::<Vec<_>>(),
                );
                assert_eq!(caps.audio.default, reforge_settings::SoundRenderer::Adlib);
                assert_eq!(caps.audio.apply, ApplyTiming::NextLaunch);
                assert_eq!(
                    caps.audio.categories,
                    [
                        reforge_settings::SoundCategory::Music,
                        reforge_settings::SoundCategory::Effects
                    ]
                );
            }
        }
    }

    #[test]
    fn video_choices_and_previews_follow_the_detected_graphics_capabilities() {
        use crate::assets::DosRelease;
        for release in [DosRelease::V100A, DosRelease::V100B] {
            let vga = LoadingProfile::new(release, false);
            let complete = LoadingProfile::new(release, true);
            assert_eq!(preview_variants(vga), ["vga"]);
            assert_eq!(preview_variants(complete), ["vga", "ega"]);
            assert!(options(vga).iter().all(|option| option.key() != "video"));
            let video = options(complete)
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
            assert!(
                preview(&reforge_inputs::ResolvedFiles::default(), "ega", vga)
                    .err()
                    .unwrap()
                    .contains("Unknown Centurion preview variant")
            );
        }
    }

    #[test]
    fn sound_choices_apply_to_every_release_and_graphics_profile() {
        use crate::assets::DosRelease;
        for release in [DosRelease::V100A, DosRelease::V100B] {
            for ega in [false, true] {
                let sound = options(LoadingProfile::new(release, ega))
                    .iter()
                    .find(|option| option.key() == "audio_renderer")
                    .unwrap();
                assert_eq!(sound.default, "adlib");
                assert_eq!(
                    sound
                        .choices
                        .iter()
                        .map(|choice| choice.value)
                        .collect::<Vec<_>>(),
                    ["adlib", "speaker"]
                );
                for choice in sound.choices {
                    crate::cli::validate_arguments(
                        [
                            "--original",
                            "synthetic",
                            "--sound",
                            if choice.value == "speaker" {
                                "pc-speaker"
                            } else {
                                choice.value
                            },
                        ]
                        .into_iter()
                        .map(Into::into),
                    )
                    .unwrap();
                }
            }
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
