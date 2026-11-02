// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player entry point; diagnostic replay remains in the development binary.

use reforge_settings::{Renderer, RuntimeSettings, Session};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

const HELP: &str = "Usage: reforge --original <game-directory> [--renderer auto|gpu|sdl|software|terminal]\n\nMindwheel uses the normal windowed text display by default.\n--renderer terminal plays in the terminal already attached to the process.\nBOOKMARK and RESUME NOVEL save/load; SCRIPT and UNSCRIPT control transcripts.\n--save-dir directory sets the base for prompted filenames (default: game data directory).\n--bookmark-path file and --transcript-path file select fixed destinations.\n--side-map file supplies an alternate SIDE.TXT.\nEnter begins play; Ctrl+Q exits. Escape and Ctrl-S retain game pause controls.";

struct Options {
    original: PathBuf,
    side_map: Option<PathBuf>,
    save_dir: Option<PathBuf>,
    bookmark: Option<PathBuf>,
    transcript: Option<PathBuf>,
    settings: RuntimeSettings,
}

pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    run_with_files(args, &BTreeMap::new())
}

pub fn validate_arguments(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    parse(args).map(|_| ())
}

fn parse(args: impl Iterator<Item = OsString>) -> Result<Option<Options>, String> {
    let prefs = Session::for_window(None);
    let mut options = Options {
        original: PathBuf::new(),
        side_map: None,
        save_dir: None,
        bookmark: None,
        transcript: None,
        settings: prefs.effective(&prefs.saved),
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let flag = arg.to_str().ok_or("invalid option encoding")?;
        if matches!(flag, "--help" | "-h") {
            return Ok(None);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--original" => {
                if !options.original.as_os_str().is_empty() {
                    return Err("--original must be supplied exactly once".into());
                }
                options.original = value.into();
            }
            "--side-map" => options.side_map = Some(value.into()),
            "--save-dir" => options.save_dir = Some(value.into()),
            "--bookmark-path" => options.bookmark = Some(value.into()),
            "--transcript-path" => options.transcript = Some(value.into()),
            _ => {
                let key = flag
                    .strip_prefix("--")
                    .ok_or_else(|| format!("unexpected argument: {flag}"))?
                    .replace('-', "_");
                if !reforge_settings::keys().any(|known| known == key) {
                    return Err(format!("unknown option: {flag}"));
                }
                let text = value.to_str().ok_or("invalid setting encoding")?;
                let text = match (key.as_str(), text) {
                    ("vsync" | "sound_enabled" | "mute_unfocused", "on") => "true",
                    ("vsync" | "sound_enabled" | "mute_unfocused", "off") => "false",
                    _ => text,
                };
                options.settings.set(&key, text)?;
            }
        }
    }
    if options.original.as_os_str().is_empty() {
        return Err("Supply --original <game-directory>".into());
    }
    Ok(Some(options))
}

pub fn run_with_files(
    args: impl Iterator<Item = OsString>,
    files: &BTreeMap<&str, PathBuf>,
) -> Result<(), String> {
    let Some(options) = parse(args)? else {
        println!("{HELP}");
        return Ok(());
    };
    if options.settings.renderer == Renderer::Terminal && !crate::player::terminal_available() {
        return Err("Terminal rendering requires an attached input/output terminal. Start reforge --original <game> --renderer terminal from a terminal.".into());
    }
    let directory = if options.original.is_dir() {
        options.original.clone()
    } else {
        options
            .original
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    };
    let selected = |name: &str| {
        files
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| directory.join(name))
    };
    let data = selected("ADISK.OBJ");
    let interpreter = selected("GO.EXE");
    let map = options
        .side_map
        .unwrap_or_else(|| directory.join("SIDE.TXT"));
    let mut loaded = crate::player::load(&data, &interpreter, &map)?;
    if let Some(path) = options.save_dir {
        loaded.files.save_directory = std::path::absolute(path).map_err(|e| e.to_string())?;
    }
    loaded.files.bookmark_path = options.bookmark;
    loaded.files.transcript_path = options.transcript;
    if options.settings.renderer == Renderer::Terminal {
        return crate::player::run_terminal(loaded);
    }
    #[cfg(feature = "sdl")]
    {
        crate::windowed::run(loaded, options.settings)
    }
    #[cfg(not(feature = "sdl"))]
    {
        Err(
            "Windowed BTZ play requires the sdl feature; use --renderer terminal from a terminal."
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> impl Iterator<Item = OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }
    #[test]
    fn terminal_and_save_paths_are_normal_player_options() {
        let options = parse(args(&[
            "--original",
            "game",
            "--renderer",
            "terminal",
            "--bookmark-path",
            "my save",
            "--save-dir",
            "saves",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.settings.renderer, Renderer::Terminal);
        assert_eq!(options.bookmark, Some("my save".into()));
        assert_eq!(options.save_dir, Some("saves".into()));
    }
    #[test]
    fn invalid_and_diagnostic_arguments_are_rejected() {
        for flags in [
            vec!["--original", "game", "--snapshot"],
            vec!["--original", "game", "--renderer", "bogus"],
            vec!["--original", "game", "--original", "other"],
        ] {
            assert!(validate_arguments(args(&flags)).is_err());
        }
    }
}
