// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{Error, assets::GraphicsAssets, opening::ScriptRules, rules, session_capture};
use std::{env, path::PathBuf};

const HELP: &str = "Usage: mach3-replay <assets-directory> <recording-directory> <new-output-directory>\n\
       mach3-replay --hook-signature <assets-directory>\n\n\
Replay sampled manual inputs and an optional session timeline into packed CGA frames.\n\
Reads games.tsv, inputs.tsv, frame stop metadata, and optional controls.tsv; never reads reference pixels.\n\
A timeline extends replay through ending titles, returned intro and subsequent games.\n\
Display mode defaults to color (mode 4); audio is deferred.\n\
--hook-signature prints the external executable bytes used to validate the capture hook.";

fn run() -> Result<bool, Error> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{HELP}");
        return Ok(true);
    }
    if args.len() == 2 && args[0] == "--hook-signature" {
        let program = rules::load_program(&PathBuf::from(&args[1]))?;
        let signature = program
            .get(0x044a..0x044a + 32)
            .ok_or_else(|| Error::InvalidData("truncated executable hook signature".into()))?;
        for byte in signature {
            print!("{byte:02x}");
        }
        println!();
        return Ok(true);
    }
    let [assets_directory, recording, destination] = args.as_slice() else {
        return Err(Error::InvalidData(HELP.into()));
    };
    let assets_directory = PathBuf::from(assets_directory);
    let assets = GraphicsAssets::load(&assets_directory)?;
    let program = rules::load_program(&assets_directory)?;
    let data = program
        .get(rules::DATA_SEGMENT_OFFSET..)
        .ok_or_else(|| Error::InvalidData("truncated program data segment".into()))?;
    let rules = ScriptRules::from_data_segment(data)?;
    let startup = if PathBuf::from(recording).join("timeline.tsv").exists() {
        Some(mach3::startup::StartupFrames::load(&assets_directory)?)
    } else {
        None
    };
    let report = session_capture::replay_session(
        &assets,
        rules,
        startup.as_ref(),
        &PathBuf::from(recording),
        &PathBuf::from(destination),
    )?;
    println!(
        "Replayed {} polls into {} frames",
        report.inputs, report.frames
    );
    if let Some(diagnostic) = report.diagnostic {
        eprintln!("mach3-replay: {diagnostic}; partial frames remain available");
        Ok(false)
    } else {
        Ok(true)
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("mach3-replay: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
