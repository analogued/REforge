// Copyright (c) 2026 analogued. Project license has not yet been selected.

mod detection;
mod options;

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
            "--help" | "-h" | "--list" => {
                if inline.is_some() {
                    input
                        .warnings
                        .push(format!("{name} is a flag and takes no value; ignored"));
                } else if name == "--list" {
                    input.list = true;
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

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let input = extract(args);
    if input.list {
        warn(&input.warnings);
        for release in detection::RELEASES {
            println!(
                "{}: {} — {} (engine {})",
                release.game.id(),
                release.game.title(),
                release.id,
                release.game.engine_id()
            );
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
    args.extend(plan.args);
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
        Game::Mach3 => {
            #[cfg(feature = "sdl")]
            {
                mach3::cli::run(args.into_iter())
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
            game,
            release: detection::RELEASES.iter().find(|r| r.game == game).unwrap(),
            original: path.into(),
        })
    }
    #[test]
    fn help_and_catalog_do_not_require_originals() {
        for values in [&[][..], &["--help"][..], &["--list"][..]] {
            assert!(run(args(values)).is_ok());
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
    }
}
