// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

use mach3::{Error, assets::GraphicsAssets, scene::initial_frame};

const HELP: &str = "Usage: mach3-frame [--format ppm|cga] <assets-directory> <output-file>\n\n\
Render the verified initial CGA gameplay scene. Default format: ppm.\n\
Reads DATA2, DATA3, DATA4, DATA5; file names are case-insensitive.\n\
Output must be a new file. This renderer has no audio or gameplay loop.";

fn run() -> Result<(), Error> {
    let mut args = env::args_os().skip(1).peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{HELP}");
        return Ok(());
    }
    let mut cga = false;
    if args.peek().is_some_and(|arg| arg == "--format") {
        args.next();
        match args.next().as_deref().and_then(|arg| arg.to_str()) {
            Some("ppm") => {}
            Some("cga") => cga = true,
            _ => return Err(Error::InvalidData("--format must be ppm or cga".into())),
        }
    }
    let directory = args.next().map(PathBuf::from);
    let destination = args.next().map(PathBuf::from);
    let (Some(directory), Some(destination), None) = (directory, destination, args.next()) else {
        return Err(Error::InvalidData(HELP.into()));
    };
    let frame = initial_frame(&GraphicsAssets::load(directory)?)?;
    // create_new also protects original assets from accidental output aliasing.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    if cga {
        output.write_all(&frame.to_cga())?;
    } else {
        frame.write_ppm(&mut output)?;
    }
    output.flush()?;
    println!("Wrote 320x200 initial scene to {}", destination.display());
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach3-frame: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
