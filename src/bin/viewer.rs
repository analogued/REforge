// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::startup::StartupFrames;
use std::{env, path::PathBuf};

const HELP: &str = "Usage: mach3-viewer [--scale 1..6] <assets-directory>\n\n\
Space: splash -> intro -> initial scene. Escape or window close: exit.\n\
Default window scale: 3; resizing preserves integer pixel scaling.\n\
Manual screen stepping; original timing, gameplay updates, and audio are deferred.";

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1).peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{HELP}");
        return Ok(());
    }
    let mut scale = 3;
    if args.peek().is_some_and(|arg| arg == "--scale") {
        args.next();
        scale = args
            .next()
            .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u32>().ok()))
            .filter(|scale| (1..=6).contains(scale))
            .ok_or_else(|| "--scale must be an integer between 1 and 6".to_string())?;
    }
    let directory = args.next().map(PathBuf::from);
    let (Some(directory), None) = (directory, args.next()) else {
        return Err(HELP.into());
    };
    let frames = StartupFrames::load(directory).map_err(|error| error.to_string())?;
    mach3::viewer::run(&frames, scale)
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
