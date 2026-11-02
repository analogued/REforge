// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

mod detection;
#[cfg(feature = "sdl")]
mod launcher;
mod options;
mod profile;

use detection::{Detected, Game};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Default)]
struct Input {
    original: Option<PathBuf>,
    remaining: Vec<OsString>,
    warnings: Vec<String>,
    help: bool,
    list: bool,
    version: bool,
}

fn extract(args: impl IntoIterator<Item = OsString>) -> Input {
    let mut input = Input::default();
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        let (name, inline) = options::split_option(&arg);
        match name.as_str() {
            "--original" => {
                let value = inline.or_else(|| args.next_if(|next| !options::is_option(next)));
                match value.filter(|value| !value.is_empty()) {
                    Some(value) => {
                        if input.original.is_some() {
                            input
                                .warnings
                                .push("repeated --original; using the last supplied path".into());
                        }
                        input.original = Some(value.into());
                    }
                    None => input
                        .warnings
                        .push("--original requires a file or directory; ignored".into()),
                }
            }
            "--help" | "-h" | "--list" | "--version" | "-V" => {
                if inline.is_some() {
                    input
                        .warnings
                        .push(format!("{name} is a flag and takes no value; ignored"));
                } else if name == "--list" {
                    input.list = true;
                } else if matches!(name.as_str(), "--version" | "-V") {
                    input.version = true;
                } else {
                    input.help = true;
                }
            }
            _ => {
                input.remaining.push(arg);
                if inline.is_none() && options::takes_value(&name) == Some(true) {
                    input
                        .remaining
                        .extend(args.next_if(|next| !options::is_option(next)));
                }
            }
        }
    }
    input
}

struct Plan {
    detected: Detected,
    args: Vec<OsString>,
    warnings: Vec<String>,
    help: bool,
}

fn plan(
    input: Input,
    detector: impl FnOnce(&Path) -> Result<Detected, String>,
) -> Result<Plan, String> {
    let path = input
        .original
        .ok_or("Supply --original <file-or-directory>; use --help for usage")?;
    let detected = detector(&path)?;
    let mut prepared = options::prepare(input.remaining, detected.game);
    let mut warnings = input.warnings;
    warnings.append(&mut prepared.warnings);
    Ok(Plan {
        detected,
        args: prepared.args,
        warnings,
        help: input.help,
    })
}

fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("REforge: warning: {warning}");
    }
}

fn launch_settings(values: Vec<OsString>, game: Game) -> (reforge_settings::Launch, Vec<OsString>) {
    let mut args = Vec::new();
    let mut prefs = reforge_settings::Launch {
        game_id: game.id().into(),
        entry_id: None,
        path: reforge_settings::default_path(),
        cli: Default::default(),
    };
    let mut no_config = false;
    let mut remaining = values.into_iter();
    while let Some(arg) = remaining.next() {
        match arg.to_str() {
            Some("--no-config") => no_config = true,
            Some("--config") => prefs.path = remaining.next().map(PathBuf::from),
            Some(
                name @ ("--window-mode"
                | "--renderer"
                | "--backend"
                | "--vsync"
                | "--scale"
                | "--frame-limit"
                | "--presentation"
                | "--audio-renderer"
                | "--digitized-samples"),
            ) => {
                if let Some(value) = remaining.next() {
                    let key = match name {
                        "--window-mode" => "window_mode",
                        "--scale" => "scale",
                        "--renderer" => "renderer",
                        "--backend" => "backend",
                        "--frame-limit" => "frame_limit",
                        "--presentation" => "presentation",
                        "--audio-renderer" => "audio_renderer",
                        "--digitized-samples" => "digitized_samples",
                        _ => "vsync",
                    };
                    if let Some(text) = value.to_str() {
                        let mut normalized = reforge_settings::RuntimeSettings::default();
                        if normalized.set(key, text).is_ok() {
                            prefs.cli.insert(key.into(), normalized.get(key));
                        }
                    }
                    if matches!(
                        name,
                        "--scale" | "--presentation" | "--audio-renderer" | "--digitized-samples"
                    ) {
                        args.push(arg);
                        args.push(value);
                    }
                }
            }
            _ => args.push(arg),
        }
    }
    if no_config {
        prefs.path = None;
    }
    (prefs, args)
}

fn opens_launcher(args: &[OsString]) -> bool {
    args.is_empty()
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args: Vec<_> = args.into_iter().collect();
    if opens_launcher(&args) {
        #[cfg(feature = "sdl")]
        return launcher::run();
        #[cfg(not(feature = "sdl"))]
        return Err(
            "The graphical launcher requires the sdl feature; use --help for command-line usage"
                .into(),
        );
    }
    let input = extract(args);
    if input.version {
        warn(&input.warnings);
        println!("REforge {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if input.list {
        warn(&input.warnings);
        let mut listed = std::collections::HashSet::new();
        for release in detection::RELEASES {
            let game = release.game();
            if listed.insert(game.id()) {
                println!("{}: {}", game.id(), game.title());
            }
        }
        return Ok(());
    }
    if input.original.is_none() && (input.help || input.remaining.is_empty()) {
        warn(&input.warnings);
        if !input.help && !input.warnings.is_empty() {
            return Err("Supply --original <file-or-directory>".into());
        }
        println!("{}", options::help(None));
        return Ok(());
    }
    // Print input diagnostics even when essential data cannot be identified.
    warn(&input.warnings);
    let mut input = input;
    input.warnings.clear();
    let plan = plan(input, detection::detect)?;
    eprintln!(
        "REforge: detected {} ({})",
        plan.detected.game.title(),
        plan.detected.release.id
    );
    warn(&plan.warnings);
    if plan.help {
        println!("{}", options::help(Some(plan.detected.game)));
        return Ok(());
    }
    let mut args = vec![
        OsString::from("--original"),
        std::fs::canonicalize(&plan.detected.original)
            .map_err(|error| format!("Cannot resolve original path: {error}"))?
            .into_os_string(),
    ];
    let (mut prefs, mut remaining) = launch_settings(plan.args, plan.detected.game);
    if let Err(error) = profile::bind(&mut prefs, &plan.detected, &mut remaining) {
        eprintln!("REforge: {error}; using global defaults where possible");
    }
    args.extend(remaining);
    let _preferences = prefs.install();
    match plan.detected.game {
        Game::AlleyCat => {
            let args = args
                .into_iter()
                .map(|arg| {
                    arg.into_string()
                        .map_err(|_| "Alley Cat requires UTF-8 paths".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            alleycat::cli::run(args)
        }
        Game::Mindwheel => btz::cli::run_with_files(args.into_iter(), &plan.detected.files),
        Game::Mach3 => {
            #[cfg(feature = "sdl")]
            {
                mach3::cli::run_with_files(args.into_iter(), &plan.detected.files)
            }
            #[cfg(not(feature = "sdl"))]
            {
                Err("Mach 3 windowed play requires the sdl feature".into())
            }
        }
    }
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("REforge: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    fn synthetic_detect(path: &Path) -> Result<Detected, String> {
        let game = if path == Path::new("cat") {
            Game::AlleyCat
        } else {
            Game::Mach3
        };
        Ok(Detected {
            files: Default::default(),
            game,
            release: detection::RELEASES
                .iter()
                .find(|r| r.game() == game)
                .unwrap(),
            original: path.into(),
        })
    }
    #[test]
    fn mindwheel_terminal_renderer_reaches_launch_settings_without_opening_launcher() {
        let original = args(&["--original", "mindwheel", "--renderer", "terminal"]);
        assert!(!opens_launcher(&original));
        let prepared = options::prepare(
            args(&["--renderer", "terminal", "--save-dir", "bookmarks"]),
            Game::Mindwheel,
        );
        assert!(prepared.warnings.is_empty());
        let (launch, forward) = launch_settings(prepared.args, Game::Mindwheel);
        assert_eq!(launch.game_id, "mindwheel");
        assert_eq!(
            launch.cli.get("renderer").map(String::as_str),
            Some("terminal")
        );
        assert_eq!(forward, args(&["--save-dir", "bookmarks"]));
    }

    #[test]
    fn original_sample_switch_is_detected_before_game_specific_normalization() {
        for spelling in ["S", "s"] {
            let mach = plan(
                extract(args(&[spelling, "--original", "mach"])),
                synthetic_detect,
            )
            .unwrap();
            let (launch, forward) = launch_settings(mach.args, Game::Mach3);
            assert_eq!(
                launch.cli.get("digitized_samples").map(String::as_str),
                Some("false")
            );
            assert_eq!(forward, args(&["--digitized-samples", "off"]));
            let cat = plan(
                extract(args(&["--original", "cat", spelling])),
                synthetic_detect,
            )
            .unwrap();
            let (launch, forward) = launch_settings(cat.args, Game::AlleyCat);
            assert!(!launch.cli.contains_key("digitized_samples"));
            assert!(forward.is_empty());
        }
        // A path named S is an original-data path, not the hidden switch.
        assert_eq!(
            extract(args(&["--original", "S"])).original,
            Some(PathBuf::from("S"))
        );
    }
    #[test]
    fn audio_renderer_override_is_locked_but_forwarded_to_the_engine() {
        let (launch, args) =
            launch_settings(args(&["--audio-renderer", "enhanced-pcm"]), Game::Mach3);
        assert_eq!(
            launch.cli.get("audio_renderer").map(String::as_str),
            Some("enhanced-pcm")
        );
        assert_eq!(args.len(), 2);
    }
    #[test]
    fn frame_limit_and_presentation_are_temporary_preferences() {
        let (launch, forward) = launch_settings(
            args(&[
                "--no-config",
                "--frame-limit",
                "120",
                "--presentation",
                "hyllian",
            ]),
            Game::Mach3,
        );
        assert_eq!(
            launch.cli.get("frame_limit").map(String::as_str),
            Some("120")
        );
        assert_eq!(
            launch.cli.get("presentation").map(String::as_str),
            Some("hyllian")
        );
        assert_eq!(forward, args(&["--presentation", "hyllian"]));
        assert!(launch.path.is_none());
    }
    #[test]
    fn preference_options_are_temporary_and_only_engine_arguments_are_forwarded() {
        let prepared = options::prepare(
            args(&[
                "--scale",
                "4",
                "--renderer",
                "software",
                "--vsync",
                "off",
                "--config",
                "custom.toml",
                "--no-config",
                "--difficulty",
                "2",
            ]),
            Game::AlleyCat,
        );
        assert!(prepared.warnings.is_empty());
        let (launch, forward) = launch_settings(prepared.args, Game::AlleyCat);
        assert_eq!(launch.game_id, "alley-cat");
        assert!(launch.path.is_none());
        assert_eq!(launch.cli.get("scale").map(String::as_str), Some("4"));
        assert_eq!(launch.cli.get("vsync").map(String::as_str), Some("false"));
        assert_eq!(forward, args(&["--scale", "4", "--difficulty", "2"]));
        let (launch, forward) = launch_settings(args(&["--config", "test.toml"]), Game::Mach3);
        assert_eq!(launch.path, Some(PathBuf::from("test.toml")));
        assert!(launch.cli.is_empty());
        assert!(forward.is_empty());
    }

    #[test]
    fn informational_flags_do_not_require_originals() {
        for values in [
            &["--help"][..],
            &["--list"][..],
            &["--version"][..],
            &["-V"][..],
        ] {
            assert!(run(args(values)).is_ok());
        }
    }
    #[test]
    fn version_exits_before_original_detection_and_rejects_inline_values() {
        for flag in ["--version", "-V"] {
            assert_eq!(options::takes_value(flag), Some(false));
            assert!(run(args(&["--original", "missing", flag])).is_ok());
            let input = extract(args(&[&format!("{flag}=unexpected")]));
            assert!(!input.version);
            assert_eq!(input.warnings.len(), 1);
        }
    }
    #[test]
    fn launcher_requires_exactly_zero_arguments() {
        assert!(opens_launcher(&[]));
        for values in [
            &["--help"][..],
            &["--list"][..],
            &["--version"][..],
            &["--original", "cat"][..],
            &["--unknown"][..],
            &["--original"][..],
            &[""][..],
        ] {
            assert!(!opens_launcher(&args(values)));
        }
    }
    #[test]
    fn optional_errors_do_not_prevent_detected_game_selection() {
        let plan = plan(
            extract(args(&[
                "--scene",
                "cheese",
                "--original",
                "mach",
                "--scale",
                "2",
                "--scale",
                "bad",
                "--unknown",
                "extra",
            ])),
            synthetic_detect,
        )
        .unwrap();
        assert_eq!(plan.detected.game, Game::Mach3);
        assert_eq!(plan.args, args(&["--scale", "2"]));
        assert_eq!(plan.warnings.len(), 4);
    }
    #[test]
    fn missing_optional_value_does_not_swallow_source_or_help() {
        let input = extract(args(&["--scale", "--original", "cat", "--help"]));
        let plan = plan(input, synthetic_detect).unwrap();
        assert_eq!(plan.detected.game, Game::AlleyCat);
        assert!(plan.help);
        assert!(plan.args.is_empty());
        assert_eq!(plan.warnings.len(), 1);
    }
    #[test]
    fn original_repeats_keep_last_nonempty_value_and_essential_failures_stop() {
        let input = extract(args(&[
            "--original=cat",
            "--original",
            "mach",
            "--original=",
            "--help",
        ]));
        assert_eq!(input.original, Some(PathBuf::from("mach")));
        assert_eq!(input.warnings.len(), 2);
        assert!(plan(extract(args(&["--scale", "2"])), synthetic_detect).is_err());
        assert!(
            plan(extract(args(&["--original", "missing"])), |_| Err(
                "unrecognized data".into()
            ))
            .is_err()
        );
    }
    #[test]
    fn option_values_are_not_interpreted_as_application_flags() {
        let input = extract(args(&["--trace=--help", "--original", "cat"]));
        assert!(!input.help);
        assert_eq!(input.remaining, args(&["--trace=--help"]));
        let input = extract(args(&["--trace=--version", "--original=--version"]));
        assert!(!input.version);
        assert_eq!(input.original, Some(PathBuf::from("--version")));
        assert_eq!(input.remaining, args(&["--trace=--version"]));
    }
}
