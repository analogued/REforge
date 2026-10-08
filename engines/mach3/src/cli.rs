// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use crate::{
    startup::StartupFrames,
    viewer::{Options, Playback},
};
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Usage: reforge --original <assets-directory> [--scale 1..16] [--step|--static] [--controls] [--period-ms 1..5000]\n\n\
--original: directory containing MACH3.EXE and DATA1 through DATA6.\n\
Space: splash -> intro -> opening. Ctrl+Q, Escape or window close: exit.\n\
Default drawable scale: 4 (1280x960 pixels); CGA pixels are displayed at 4:3.\n\
Gameplay continues until game over, then displays Game Over and High Score.\n\
Space returns to intro after the first post-game title pair.\n\
--step: fresh Enter presses advance one body or title step. --static: show initial scene only.\n\
--controls: flag with no value; arrows or S/X/B/N steer; Space or keypad5 fires. Gameplay has no loop cap.\n\
Default pace: about 36.41 displayed passes/s (12.14 bodies/s); --period-ms sets the body interval.\n\
Pause/Break toggles host pause. F1 retains original pause/end-game behavior; F2 toggles sound preference, F3 switches color/monochrome.\n\
Original intro timing, exact CPU-dependent pacing, and audio remain deferred.";

/// Run the Mach 3 viewer with arguments excluding the executable name.
///
/// The `viewer` feature provides this entry point for REforge and the standalone
/// `mach3-viewer` binary.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let Some((directory, options)) = parse_args(args)? else {
        println!("{HELP}");
        return Ok(());
    };
    let frames = StartupFrames::load(&directory).map_err(|error| error.to_string())?;
    let program = crate::rules::load_program(&directory).map_err(|error| error.to_string())?;
    let rules = crate::opening::ScriptRules::from_data_segment(
        &program[crate::rules::DATA_SEGMENT_OFFSET..],
    )
    .map_err(|error| error.to_string())?;
    crate::viewer::run_with_rules(&frames, options, rules)
}

/// Validate the viewer argument contract without loading original data or SDL.
pub fn validate_arguments(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    parse_args(args).map(|_| ())
}

fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Option<(PathBuf, Options)>, String> {
    let mut args = args.peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(None);
    }
    let mut options = Options::default();
    let mut playback_set = false;
    let mut directory = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--original") => {
                if directory.is_some() {
                    return Err("--original must be supplied exactly once".into());
                }
                let path = args
                    .next_if(|arg| !arg.to_string_lossy().starts_with('-'))
                    .ok_or_else(|| "--original requires a data directory".to_string())?;
                directory = Some(PathBuf::from(path));
            }
            Some("--scale") => {
                options.scale = args
                    .next()
                    .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u32>().ok()))
                    .filter(|scale| (1..=16).contains(scale))
                    .ok_or_else(|| "--scale must be an integer between 1 and 16".to_string())?;
            }
            Some("--controls") => {
                options.controls = true;
            }
            Some("--step" | "--static") => {
                if playback_set {
                    return Err("--step and --static cannot be combined or repeated".into());
                }
                options.playback = if arg == "--step" {
                    Playback::Step
                } else {
                    Playback::Static
                };
                playback_set = true;
            }
            Some("--period-ms") => {
                let period = args
                    .next()
                    .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u64>().ok()))
                    .filter(|period| (1..=5000).contains(period))
                    .ok_or_else(|| {
                        "--period-ms must be an integer between 1 and 5000".to_string()
                    })?;
                options.body_period = std::time::Duration::from_millis(period);
            }
            Some(value) if value.starts_with('-') => return Err(HELP.into()),
            _ => {
                return Err(format!(
                    "Unexpected positional argument: {}. Supply the data directory with --original",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    if options.controls && options.playback == Playback::Static {
        return Err("--controls cannot be combined with --static".into());
    }
    let directory =
        directory.ok_or_else(|| "Missing required --original <assets-directory>".to_string())?;
    Ok(Some((directory, options)))
}

#[cfg(test)]
mod tests {
    use super::parse_args;
    use std::{ffi::OsString, path::PathBuf};

    fn args(values: &[&str]) -> impl Iterator<Item = OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn original_path_and_controls_are_independent_in_either_order() {
        for values in [
            &[
                "--original",
                "assets with spaces",
                "--controls",
                "--scale",
                "2",
            ][..],
            &[
                "--controls",
                "--scale",
                "2",
                "--original",
                "assets with spaces",
            ][..],
        ] {
            let (directory, options) = parse_args(args(values)).unwrap().unwrap();
            assert_eq!(directory, PathBuf::from("assets with spaces"));
            assert!(options.controls);
            assert_eq!(options.scale, 2);
        }
        let (_, options) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert!(!options.controls);
    }

    #[test]
    fn requires_one_explicit_original_directory() {
        for values in [
            &[][..],
            &["--controls"][..],
            &["assets"][..],
            &["--controls", "assets"][..],
            &["--original"][..],
            &["--original", "--controls"][..],
            &["--original", "assets", "extra"][..],
            &["--original", "assets", "--original", "other"][..],
        ] {
            assert!(
                parse_args(args(values)).unwrap_err().contains("--original"),
                "{values:?}"
            );
        }
    }

    #[test]
    fn help_does_not_require_or_load_original_data() {
        for flag in ["--help", "-h"] {
            assert!(parse_args(args(&[flag])).unwrap().is_none());
        }
    }

    #[cfg(unix)]
    #[test]
    fn original_directory_preserves_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"assets-\xff".to_vec());
        let (directory, _) = parse_args([OsString::from("--original"), path.clone()].into_iter())
            .unwrap()
            .unwrap();
        assert_eq!(directory.into_os_string(), path);
    }
}
