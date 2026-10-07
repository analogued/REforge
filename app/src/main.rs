// Copyright (c) 2026 analogued. Project license has not yet been selected.

use std::{ffi::OsString, process::ExitCode};

const HELP: &str = "REforge

Usage: reforge <game> [game arguments...]
       reforge --list

Games:
  alley-cat    Alley Cat (requires an external CAT.EXE)
  mach-3       Mach 3 (requires an external resource directory)

Use reforge <game> --help for game-specific options.
Original game files are required and are not included.";

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let mut args = args.into_iter();
    let Some(game) = args.next() else {
        println!("{HELP}");
        return Ok(());
    };
    match game.to_str() {
        Some("--help" | "-h" | "--list") => {
            if args.next().is_some() {
                return Err("Help and --list do not accept extra arguments".into());
            }
            println!("{HELP}");
            Ok(())
        }
        Some("alley-cat") => {
            let args = args
                .map(|arg| {
                    arg.into_string()
                        .map_err(|_| "Alley Cat arguments must be UTF-8".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            alleycat::cli::run(args)
        }
        Some("mach-3") => {
            #[cfg(feature = "sdl")]
            {
                mach3::cli::run(args)
            }
            #[cfg(not(feature = "sdl"))]
            {
                Err("Mach 3 windowed play requires the sdl feature; use its headless export/replay tools for diagnostics".into())
            }
        }
        _ => Err(format!(
            "Unknown game: {}\nUse --list to see supported games",
            game.to_string_lossy()
        )),
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
    use super::run;
    use std::ffi::OsString;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn selection_and_help_do_not_require_original_files() {
        for values in [
            &[][..],
            &["--list"][..],
            &["--help"][..],
            &["alley-cat", "--help"][..],
        ] {
            assert!(run(args(values)).is_ok());
        }
        #[cfg(feature = "sdl")]
        assert!(run(args(&["mach-3", "--help"])).is_ok());
    }

    #[test]
    fn selection_errors_are_reported_before_loading_originals() {
        assert!(
            run(args(&["unknown"]))
                .unwrap_err()
                .contains("Unknown game")
        );
        assert!(run(args(&["--list", "alley-cat"])).is_err());
        assert!(
            run(args(&[
                "alley-cat",
                "--original",
                "missing-synthetic-input",
                "--scale",
                "0"
            ]))
            .unwrap_err()
            .contains("Scale")
        );
        #[cfg(feature = "sdl")]
        assert!(
            run(args(&["mach-3", "--scale", "0"]))
                .unwrap_err()
                .contains("--scale")
        );
        #[cfg(not(feature = "sdl"))]
        assert!(
            run(args(&["mach-3"]))
                .unwrap_err()
                .contains("requires the sdl feature")
        );
    }

    #[test]
    fn development_arguments_follow_the_application_feature() {
        let error = run(args(&["alley-cat", "--scene", "alley"])).unwrap_err();
        #[cfg(feature = "dev-tools")]
        assert!(error.contains("--original"));
        #[cfg(not(feature = "dev-tools"))]
        assert!(error.contains("--scene"));
    }
}
