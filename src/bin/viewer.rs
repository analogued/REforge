// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{
    startup::StartupFrames,
    viewer::{Options, Playback},
};
use std::{env, path::PathBuf};

const HELP: &str = "Usage: mach3-viewer [--scale 1..6] [--step|--static] [--period-ms 1..5000] <assets-directory>\n\n\
Space: splash -> intro -> opening. Escape or window close: exit.\n\
Default window scale: 3; resizing preserves integer pixel scaling.\n\
Opening animates 270 no-input loop bodies, including the first three enemy waves, then holds.\n\
--step: fresh Space presses advance one body. --static: show initial scene only.\n\
Default pace: about 12.14 bodies/s; --period-ms overrides it.\n\
Original intro timing, player controls, later enemy behavior, and audio remain deferred.";

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1).peekable();
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
                    .filter(|scale| (1..=6).contains(scale))
                    .ok_or_else(|| "--scale must be an integer between 1 and 6".to_string())?;
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
    let directory = args.next().map(PathBuf::from);
    let (Some(directory), None) = (directory, args.next()) else {
        return Err(HELP.into());
    };
    let frames = StartupFrames::load(directory).map_err(|error| error.to_string())?;
    mach3::viewer::run_with_options(&frames, options)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach3-viewer: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
