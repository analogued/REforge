// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

#[cfg(feature = "dev-tools")]
use crate::platform::Playback;
use crate::{
    gameplay::{DisplayMode, F3Mode},
    platform::{MotionMode, Options},
    sound::render::AudioRenderer,
    startup::StartupFrames,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: reforge --original <assets-directory> [--scale auto|1..16] [--presentation pixels|hyllian] [--audio-renderer speaker|enhanced-pcm] [--video cga|mono] [--f3-mode press|original] [--digitized-samples on|off] [--motion reforged|enhanced|original]\n\n\
--original: directory containing MACH3.EXE or MACH3.OV2, plus DATA1 through DATAH for normal animated playback.\n\
Space: advance splash; intro skip waits for the current sample/group and tail. Ctrl+Q or window close: exit.\n\
Escape or F10: open settings; F10, Escape or Close returns. Scaling and fullscreen changes apply live.\n\
Default drawable scale: auto (fit the usable display); CGA pixels are displayed at 4:3.\n\
--presentation pixels|hyllian: nearest pixels (default) or the shared Hyllian CRT shader; unavailable shaders warn and fall back to pixels.\n\
Gameplay continues until game over, then displays Game Over and High Score.\n\
Space returns to intro after the first post-game title pair.\n\
Arrows or S/X/B/N steer; Space or keypad5 fires. Gameplay has no loop cap.\n\
Normal animation runs gameplay, sound and waits on one PIT timeline.\n\
--audio-renderer speaker|enhanced-pcm: select host audio rendering (default speaker); timing is unchanged.\n\
--digitized-samples on|off: execute recorded speech/music (default on); off retains procedural effects and skips sample waits.\n\
--motion reforged|enhanced|original: use Reforged scene movement (reforged, default), craft/shadow interpolation (enhanced), or use original motion; gameplay and collisions are unchanged.\n\
P or Pause/Break freezes host simulation and playback. F1 retains original pause/end-game behavior; F2 toggles native sound, F3 switches color/monochrome.\n\
--video cga|mono: initial native colour or monochrome display (default cga); F3 still toggles during play.\n\
--f3-mode press|original: toggle F3 once per press (default press), or at every active original control poll.\n\
Unavailable audio warns and mutes host output; guest execution continues. Exact CPU-dependent pacing remains unmodeled.";

fn help() -> String {
    #[cfg(feature = "dev-tools")]
    {
        format!(
            "{HELP}\nF9 [dev-tools]: toggle the presentation FPS indicator (hidden by default).\n--step [dev-tools]: fresh Enter presses advance one body or title step.\n--static [dev-tools]: show initial scene only.\nThese previews use silent nominal stepping and need MACH3.EXE or MACH3.OV2 and DATA1 through DATA6.\n--trace-input PATH [dev-tools]: write input diagnostic trace to a new file.\n--trace-timing PATH [dev-tools]: write host execution and scheduling spans to a new file.\n--trace-motion PATH [dev-tools]: optional motion diagnostics; failures disable logging only.\n--motion-experiment PATH [dev-tools]: compatibility alias for --motion enhanced --trace-motion PATH."
        )
    }
    #[cfg(not(feature = "dev-tools"))]
    {
        HELP.into()
    }
}

/// Run the Mach 3 windowed frontend with arguments excluding the executable name.
///
/// The `sdl` feature provides this entry point for REforge and the standalone
/// `mach-3` binary.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    run_with_files(args, &BTreeMap::new())
}

/// Run using the physical files selected by REforge's release detector.
/// Unlisted resources retain the standalone loader's ordinary resolution.
pub fn run_with_files(
    args: impl Iterator<Item = OsString>,
    files: &BTreeMap<&str, PathBuf>,
) -> Result<(), String> {
    let Some((directory, options)) = parse_args(args)? else {
        println!("{}", help());
        return Ok(());
    };
    let mut read = |name: &str| read_selected(&directory, files, name);
    let frames = StartupFrames::load_with(&mut read).map_err(|error| error.to_string())?;
    let program =
        crate::executable::load_program_with_files(&directory, files).map_err(|e| e.to_string())?;
    let rules = crate::gameplay::ScriptRules::from_data_segment(
        crate::executable::data_segment(&program).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(feature = "dev-tools")]
    if options.is_preview() {
        return crate::platform::run_preview(&frames, options, rules);
    }
    let samples = crate::sound::SoundAssets::load_with(&mut read).map_err(|e| e.to_string())?;
    let intro = crate::sound::IntroProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    let procedural =
        crate::sound::ProceduralProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    crate::platform::run_live(&frames, options, rules, samples, intro, procedural)
}

fn read_selected(
    directory: &Path,
    files: &BTreeMap<&str, PathBuf>,
    name: &str,
) -> Result<Vec<u8>, crate::Error> {
    if let Some((_, path)) = files
        .iter()
        .find(|(catalog_name, _)| catalog_name.eq_ignore_ascii_case(name))
    {
        std::fs::read(path).map_err(crate::Error::Io)
    } else {
        crate::assets::read_resource(directory, name)
    }
}

/// Validate the windowed argument contract without loading original data or SDL.
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
    #[cfg(feature = "dev-tools")]
    let mut playback_set = false;
    let mut directory = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("S" | "s") => options.digitized_samples = Some(false),
            Some("--digitized-samples") => {
                options.digitized_samples =
                    Some(match args.next().as_deref().and_then(|arg| arg.to_str()) {
                        Some("on") => true,
                        Some("off") => false,
                        _ => {
                            return Err("--digitized-samples must be on or off".into());
                        }
                    });
            }
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
                let value = args.next().ok_or_else(|| {
                    "--scale must be auto or an integer between 1 and 16".to_string()
                })?;
                options.scale = match value.to_str() {
                    Some("auto") => None,
                    value => Some(
                        value
                            .and_then(|value| value.parse::<u32>().ok())
                            .filter(|scale| (1..=16).contains(scale))
                            .ok_or_else(|| {
                                "--scale must be auto or an integer between 1 and 16".to_string()
                            })?,
                    ),
                };
            }
            Some("--presentation") => {
                options.presentation = args
                    .next()
                    .as_deref()
                    .and_then(|arg| arg.to_str())
                    .ok_or("--presentation must be pixels or hyllian")?
                    .parse()?;
            }
            Some("--audio-renderer") => {
                options.audio_renderer = match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("speaker") => AudioRenderer::Speaker,
                    Some("enhanced-pcm") => AudioRenderer::EnhancedPcm,
                    _ => {
                        return Err("--audio-renderer must be speaker or enhanced-pcm".into());
                    }
                };
            }
            Some("--video") => {
                options.video = match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("cga") => DisplayMode::Color,
                    Some("mono") => DisplayMode::Monochrome,
                    _ => return Err("--video must be cga or mono".into()),
                };
            }
            Some("--f3-mode") => {
                options.f3_mode = match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("press") => F3Mode::Press,
                    Some("original") => F3Mode::Original,
                    _ => {
                        return Err("--f3-mode must be press or original".into());
                    }
                };
            }
            #[cfg(feature = "dev-tools")]
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
            #[cfg(feature = "dev-tools")]
            Some("--trace-input") => {
                let path = args
                    .next_if(|arg| !arg.to_string_lossy().starts_with('-'))
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| "--trace-input requires a nonempty output path".to_string())?;
                options.input_trace = Some(PathBuf::from(path));
            }
            #[cfg(feature = "dev-tools")]
            Some("--trace-timing") => {
                let path = args
                    .next_if(|arg| !arg.to_string_lossy().starts_with('-'))
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| "--trace-timing requires a nonempty output path".to_string())?;
                options.timing_trace = Some(PathBuf::from(path));
            }
            Some("--motion") => {
                options.motion = Some(match args.next().as_deref().and_then(|v| v.to_str()) {
                    Some("reforged") => MotionMode::Reforged,
                    Some("original") => MotionMode::Original,
                    Some("enhanced") => MotionMode::Enhanced,
                    _ => return Err("--motion must be reforged, enhanced or original".into()),
                });
            }
            #[cfg(feature = "dev-tools")]
            Some("--trace-motion" | "--motion-experiment") => {
                let path = args
                    .next_if(|arg| !arg.to_string_lossy().starts_with('-'))
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| {
                        format!("{} requires a new trace path", arg.to_string_lossy())
                    })?;
                if arg == "--motion-experiment" {
                    options.motion = Some(MotionMode::Enhanced);
                }
                options.motion_trace = Some(PathBuf::from(path));
            }
            Some(value) if value.starts_with('-') => return Err(help()),
            _ => {
                return Err(format!(
                    "Unexpected positional argument: {}. Supply the data directory with --original",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    let directory =
        directory.ok_or_else(|| "Missing required --original <assets-directory>".to_string())?;
    #[cfg(feature = "dev-tools")]
    if (options.motion.is_some() || options.motion_trace.is_some()) && options.is_preview() {
        return Err("motion observation requires animated playback".into());
    }
    Ok(Some((directory, options)))
}

#[cfg(test)]
mod tests;
