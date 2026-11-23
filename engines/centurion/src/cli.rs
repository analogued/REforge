// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog-driven player arguments. Raw research tools use run_prototype instead.
use crate::assets::{LoadingProfile, VideoMode};
use reforge_inputs::ResolvedFiles;
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub(crate) struct Options {
    pub original: PathBuf,
    pub video: VideoMode,
    pub state_dir: Option<PathBuf>,
    pub load: Option<PathBuf>,
    pub scale: Option<u32>,
    pub hyllian: bool,
    pub mute: bool,
    pub original_tribute: bool,
    #[cfg(feature = "dev-tools")]
    pub smoke_frames: Option<u32>,
}

pub fn validate_arguments(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    parse(args).map(|_| ())
}

/// Read the exact resources selected and verified by the application detector.
pub fn run(
    args: impl Iterator<Item = OsString>,
    files: &ResolvedFiles,
    profile: LoadingProfile,
) -> Result<(), String> {
    let Some(mut options) = parse(args)? else {
        println!(
            "Centurion: Defender of Rome\n--video vga|ega: video mode (default VGA; EGA requires all six EGA bank pairs)\n--state-dir DIR: DOS-compatible save slots (default: beside the original installation)\n--load FILE: load a DOS save\n--mute: silent playback\n--original-tribute-menus: hide applied tribute indicators\nF10: Settings; Pause: host pause; Ctrl+Q: exit. Escape retains game back/cancel controls."
        );
        return Ok(());
    };
    let video = available_video(options.video, profile);
    if video != options.video {
        eprintln!("REforge: EGA resources are unavailable for this installation; using VGA");
        options.video = video;
    }
    #[cfg(not(feature = "sdl"))]
    {
        let _ = (options, files, profile);
        Err("Interactive Centurion playback requires the sdl feature".into())
    }
    #[cfg(feature = "sdl")]
    {
        let root = if options.original.is_dir() {
            options.original.clone()
        } else {
            options
                .original
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_owned()
        };
        let assets = crate::assets::Assets::load(files, root, profile.release, options.video)?;
        let mut session =
            crate::session::Session::new(&assets, crate::session::Phase::Intro, 0, 0)?;
        session.save_dir = Some(
            options
                .state_dir
                .clone()
                .unwrap_or_else(|| assets.original().to_owned()),
        );
        session.tribute_indicator = !options.original_tribute;
        if let Some(path) = &options.load {
            session.campaign.load(&crate::assets::read(path)?)?;
        }
        crate::player::run(&assets, &mut session, options)?;
        Ok(())
    }
}

pub(crate) fn parse_video(value: &str) -> Result<VideoMode, String> {
    match value {
        "vga" => Ok(VideoMode::Vga),
        "ega" => Ok(VideoMode::Ega),
        _ => Err("Video must be vga or ega".into()),
    }
}

fn available_video(video: VideoMode, profile: LoadingProfile) -> VideoMode {
    if video == VideoMode::Ega && !profile.ega {
        VideoMode::Vga
    } else {
        video
    }
}

fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut original = None;
    while let Some(arg) = args.next() {
        let name = arg.to_str().ok_or("Argument name must be UTF-8")?;
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("Missing value for {name}"))
        };
        match name {
            "--help" | "-h" => return Ok(None),
            "--original" => {
                if original.is_some() {
                    return Err("--original must be supplied exactly once".into());
                }
                original = Some(PathBuf::from(value()?));
            }
            "--state-dir" => options.state_dir = Some(PathBuf::from(value()?)),
            "--load" => options.load = Some(PathBuf::from(value()?)),
            "--video" => {
                options.video = parse_video(value()?.to_str().ok_or("Video must be vga or ega")?)?;
            }
            "--mute" => options.mute = true,
            "--original-tribute-menus" => options.original_tribute = true,
            "--scale" => {
                let value = value()?;
                options.scale = match value.to_str() {
                    Some("auto") => None,
                    Some(value) => Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|n| (1..=16).contains(n))
                            .ok_or("Scale must be auto or 1..16")?,
                    ),
                    None => return Err("Scale must be auto or 1..16".into()),
                };
            }
            "--presentation" => {
                options.hyllian = match value()?.to_str() {
                    Some("pixels") => false,
                    Some("hyllian") => true,
                    _ => return Err("Presentation must be pixels or hyllian".into()),
                }
            }
            #[cfg(feature = "dev-tools")]
            "--smoke-frames" => {
                options.smoke_frames = Some(
                    value()?
                        .to_str()
                        .and_then(|v| v.parse::<u32>().ok())
                        .filter(|n| *n > 0)
                        .ok_or("Smoke frames must be positive")?,
                )
            }
            _ => return Err(format!("Unknown Centurion argument {name}")),
        }
    }
    options.original = original.ok_or("Supply --original pointing to the selected installation")?;
    Ok(Some(options))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_choices_parse_and_follow_the_detected_graphics_capabilities() {
        use crate::assets::DosRelease;
        for (value, video) in [("vga", VideoMode::Vga), ("ega", VideoMode::Ega)] {
            let options = parse(
                ["--original", "synthetic", "--video", value]
                    .into_iter()
                    .map(Into::into),
            )
            .unwrap()
            .unwrap();
            assert_eq!(options.video, video);
            for release in [DosRelease::V100A, DosRelease::V100B] {
                assert_eq!(
                    available_video(video, LoadingProfile::new(release, true)),
                    video
                );
                assert_eq!(
                    available_video(video, LoadingProfile::new(release, false)),
                    VideoMode::Vga
                );
            }
        }
        assert_eq!(
            parse(["--original", "synthetic"].into_iter().map(Into::into))
                .unwrap()
                .unwrap()
                .video,
            VideoMode::Vga
        );
        for arguments in [
            vec!["--original", "synthetic", "--video"],
            vec!["--original", "synthetic", "--video", "cga"],
        ] {
            assert!(validate_arguments(arguments.into_iter().map(Into::into)).is_err());
        }
    }

    #[test]
    fn player_contract_preserves_paths_and_rejects_research_switches() {
        let parsed = parse(
            [
                "--original",
                "renamed/input",
                "--state-dir",
                "shared saves",
                "--mute",
            ]
            .into_iter()
            .map(Into::into),
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.state_dir, Some(PathBuf::from("shared saves")));
        assert!(parsed.mute);
        assert!(
            validate_arguments(
                ["--original", "data", "--naval-probe"]
                    .into_iter()
                    .map(Into::into)
            )
            .is_err()
        );
        assert!(
            validate_arguments(
                ["--original", "data", "--record", "session"]
                    .into_iter()
                    .map(Into::into)
            )
            .is_err()
        );
    }
}
