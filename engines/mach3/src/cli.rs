// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use crate::{
    startup::StartupFrames,
    viewer::{Options, Playback},
};
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Usage: reforge mach-3 [--scale 1..16] [--step|--static] [--controls] [--period-ms 1..5000] <assets-directory>\n\n\
Space: splash -> intro -> opening. Escape or window close: exit.\n\
Default window scale: 3 (960x720); CGA pixels are displayed at 4:3.\n\
Gameplay continues until game over, then displays Game Over and High Score.\n\
Space returns to intro after the first post-game title pair.\n\
--step: fresh Enter presses advance one body or title step. --static: show initial scene only.\n\
--controls: arrows or S/X/B/N steer; Space or keypad5 fires. Gameplay has no loop cap.\n\
Default pace: about 36.41 displayed passes/s (12.14 bodies/s); --period-ms sets the body interval.\n\
F1 pauses/ends game, F2 toggles sound preference, F3 switches color/monochrome.\n\
Original intro timing, exact CPU-dependent pacing, and audio remain deferred.";

/// Run the Mach 3 viewer with arguments excluding the executable name.
///
/// The `viewer` feature provides this entry point for REforge and the standalone
/// `mach3-viewer` binary.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let mut args = args.peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{HELP}");
        return Ok(());
    }
    let mut options = Options::default();
    let mut playback_set = false;
    while let Some(arg) = args.peek().and_then(|arg| arg.to_str()) {
        match arg {
            "--scale" => {
                args.next();
                options.scale = args
                    .next()
                    .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u32>().ok()))
                    .filter(|scale| (1..=16).contains(scale))
                    .ok_or_else(|| "--scale must be an integer between 1 and 16".to_string())?;
            }
            "--controls" => {
                args.next();
                options.controls = true;
            }
            "--step" | "--static" => {
                if playback_set {
                    return Err("--step and --static cannot be combined or repeated".into());
                }
                options.playback = if args.next().unwrap() == "--step" {
                    Playback::Step
                } else {
                    Playback::Static
                };
                playback_set = true;
            }
            "--period-ms" => {
                args.next();
                let period = args
                    .next()
                    .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u64>().ok()))
                    .filter(|period| (1..=5000).contains(period))
                    .ok_or_else(|| {
                        "--period-ms must be an integer between 1 and 5000".to_string()
                    })?;
                options.body_period = std::time::Duration::from_millis(period);
            }
            "--" => {
                args.next();
                break;
            }
            arg if arg.starts_with('-') => return Err(HELP.into()),
            _ => break,
        }
    }
    if options.controls && options.playback == Playback::Static {
        return Err("--controls cannot be combined with --static".into());
    }
    let directory = args.next().map(PathBuf::from);
    let (Some(directory), None) = (directory, args.next()) else {
        return Err(HELP.into());
    };
    let frames = StartupFrames::load(&directory).map_err(|error| error.to_string())?;
    let program = crate::rules::load_program(&directory).map_err(|error| error.to_string())?;
    let rules = crate::opening::ScriptRules::from_data_segment(
        &program[crate::rules::DATA_SEGMENT_OFFSET..],
    )
    .map_err(|error| error.to_string())?;
    crate::viewer::run_with_rules(&frames, options, rules)
}
