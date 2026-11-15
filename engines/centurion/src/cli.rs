// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog-driven player arguments. Raw research tools use run_prototype instead.
use reforge_inputs::ResolvedFiles;
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub(crate) struct Options {
    pub original: PathBuf,
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
pub fn run(args: impl Iterator<Item = OsString>, files: &ResolvedFiles) -> Result<(), String> {
    let Some(options) = parse(args)? else {
        println!(
            "Centurion: Defender of Rome\n--state-dir DIR: DOS-compatible save slots (default: beside the original installation)\n--load FILE: load a DOS save\n--mute: silent playback\n--original-tribute-menus: hide applied tribute indicators\nF10: Settings; Pause: host pause; Ctrl+Q: exit. Escape retains game back/cancel controls."
        );
        return Ok(());
    };
    #[cfg(not(feature = "sdl"))]
    {
        let _ = (options, files);
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
        let assets = crate::assets::Assets::load(files, root)?;
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
