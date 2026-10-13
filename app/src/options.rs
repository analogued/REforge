// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Player CLI catalog. Engine-specific parsers receive only validated options.

use crate::detection::Game;
use std::ffi::OsString;

#[derive(Clone, Copy)]
enum Value {
    Flag,
    Path,
    Integer(u64, u64),
    Scale,
    Rate,
    Choice(&'static [&'static str]),
    Actors,
    Damage,
}

struct Spec {
    name: &'static str,
    game: Option<Game>,
    value: Value,
    dev: bool,
    description: &'static str,
}

const SCENES: &[&str] = &[
    "splash",
    "alley",
    "fishbowl",
    "underwater",
    "bookcase",
    "cheese",
    "birdcage",
    "milk",
    "hearts",
    "completion",
];
const SPECS: &[Spec] = &[
    Spec {
        name: "--original",
        game: None,
        value: Value::Path,
        dev: false,
        description: "Original game file or directory; identify a verified release automatically",
    },
    Spec {
        name: "--scale",
        game: None,
        value: Value::Scale,
        dev: false,
        description: "Initial drawable pixel multiplier, auto|1..16 (default auto for both games)",
    },
    Spec {
        name: "--help",
        game: None,
        value: Value::Flag,
        dev: false,
        description: "Show common help, or detected-game help with --original",
    },
    Spec {
        name: "--list",
        game: None,
        value: Value::Flag,
        dev: false,
        description: "List supported games and verified releases",
    },
    Spec {
        name: "--machine",
        game: Some(Game::AlleyCat),
        value: Value::Choice(&["cga", "pcjr"]),
        dev: false,
        description: "Hardware path (default cga)",
    },
    Spec {
        name: "--difficulty",
        game: Some(Game::AlleyCat),
        value: Value::Integer(0, 7),
        dev: false,
        description: "Initial difficulty, 0..7; skip skill question",
    },
    Spec {
        name: "--scene",
        game: Some(Game::AlleyCat),
        value: Value::Choice(SCENES),
        dev: true,
        description: "Start in the named scene",
    },
    Spec {
        name: "--seed",
        game: Some(Game::AlleyCat),
        value: Value::Integer(0, 65535),
        dev: true,
        description: "Random seed, decimal or 0xHEX (default 0xfa59)",
    },
    Spec {
        name: "--step-hz",
        game: Some(Game::AlleyCat),
        value: Value::Rate,
        dev: true,
        description: "Diagnostic accepted updates/sec, 0.1..1000",
    },
    Spec {
        name: "--headless",
        game: Some(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Run without a window",
    },
    Spec {
        name: "--timed",
        game: Some(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Use the reference clock in headless runs",
    },
    Spec {
        name: "--steps",
        game: Some(Game::AlleyCat),
        value: Value::Integer(0, usize::MAX as u64),
        dev: true,
        description: "Number of accepted updates (default 60 headless)",
    },
    Spec {
        name: "--script",
        game: Some(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Headless input CSV: x,y[,action]",
    },
    Spec {
        name: "--trace",
        game: Some(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write accepted player states as CSV",
    },
    Spec {
        name: "--trace-timing",
        game: Some(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write timed outer-loop events as CSV",
    },
    Spec {
        name: "--dump-cga",
        game: Some(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write final packed framebuffer",
    },
    Spec {
        name: "--dump-ppm",
        game: Some(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write final 320x200 image",
    },
    Spec {
        name: "--disable-damage",
        game: Some(Game::AlleyCat),
        value: Value::Damage,
        dev: true,
        description: "all, none or comma-separated damage identifiers",
    },
    Spec {
        name: "--disable-actors",
        game: Some(Game::AlleyCat),
        value: Value::Actors,
        dev: true,
        description: "all, none or comma-separated actor identifiers",
    },
    Spec {
        name: "--infinite-lives",
        game: Some(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Retain lives while preserving death effects",
    },
    Spec {
        name: "--original-window-timing",
        game: Some(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Restore original difficulty-6 window hold",
    },
    Spec {
        name: "--audio-renderer",
        game: Some(Game::Mach3),
        value: Value::Choice(&["speaker", "enhanced-pcm"]),
        dev: false,
        description: "Host audio rendering for normal animation (default speaker); previews stay silent",
    },
    Spec {
        name: "--f3-mode",
        game: Some(Game::Mach3),
        value: Value::Choice(&["press", "original"]),
        dev: false,
        description: "F3 once per press (default press), or every active original control poll",
    },
    Spec {
        name: "--controls",
        game: Some(Game::Mach3),
        value: Value::Flag,
        dev: false,
        description: "Enable steering and shooting; takes no value",
    },
    Spec {
        name: "--step",
        game: Some(Game::Mach3),
        value: Value::Flag,
        dev: false,
        description: "Advance one body/title step per fresh Enter press",
    },
    Spec {
        name: "--static",
        game: Some(Game::Mach3),
        value: Value::Flag,
        dev: false,
        description: "Show initial scene only",
    },
    Spec {
        name: "--period-ms",
        game: Some(Game::Mach3),
        value: Value::Integer(1, 5000),
        dev: false,
        description: "Whole-body interval in milliseconds, 1..5000",
    },
    Spec {
        name: "--trace-input",
        game: Some(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Write input diagnostic trace to a new file",
    },
    Spec {
        name: "--trace-timing",
        game: Some(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Write simulation and presentation timing to a new file",
    },
];

pub fn takes_value(name: &str) -> Option<bool> {
    if name == "-h" {
        return Some(false);
    }
    SPECS
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| !matches!(spec.value, Value::Flag))
}

pub fn split_option(arg: &OsString) -> (String, Option<OsString>) {
    match arg.to_str().and_then(|text| text.split_once('=')) {
        Some((name, value)) if name.starts_with('-') => (name.into(), Some(value.into())),
        _ => (arg.to_string_lossy().into_owned(), None),
    }
}

pub fn is_option(arg: &OsString) -> bool {
    arg.to_string_lossy().starts_with('-')
}

pub struct Prepared {
    pub args: Vec<OsString>,
    pub warnings: Vec<String>,
}

struct Entry {
    spec: &'static Spec,
    value: Option<OsString>,
}

fn validate(spec: &Spec, value: OsString, game: Game) -> Result<OsString, String> {
    if matches!(spec.value, Value::Path) {
        if value.is_empty() {
            return Err("path must not be empty".into());
        }
        if game == Game::AlleyCat && value.to_str().is_none() {
            return Err("this engine requires UTF-8 diagnostic paths".into());
        }
        // The engine receives separate name/value arguments. Protect an
        // explicitly inline filename beginning with '-' from its option gate.
        if spec.game == Some(Game::Mach3)
            && matches!(spec.name, "--trace-input" | "--trace-timing")
            && is_option(&value)
        {
            return Ok(std::path::PathBuf::from(".").join(value).into_os_string());
        }
        return Ok(value);
    }
    let text = value.to_str().ok_or("value must be UTF-8")?;
    match spec.value {
        Value::Integer(min, max) => {
            let number = if let Some(hex) = text.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()
            } else {
                text.parse::<u64>().ok()
            };
            match number.filter(|number| (min..=max).contains(number)) {
                Some(number) => return Ok(number.to_string().into()),
                None => return Err(format!("expected an integer between {min} and {max}")),
            }
        }
        Value::Scale => {
            if text == "auto" {
                return Ok(value);
            }
            let number = if let Some(hex) = text.strip_prefix("0x") {
                u32::from_str_radix(hex, 16).ok()
            } else {
                text.parse::<u32>().ok()
            };
            return number
                .filter(|number| (1..=16).contains(number))
                .map(|number| number.to_string().into())
                .ok_or_else(|| "expected auto or an integer between 1 and 16".into());
        }
        Value::Rate => {
            if !text
                .parse::<f64>()
                .is_ok_and(|rate| rate.is_finite() && (0.1..=1000.0).contains(&rate))
            {
                return Err("expected a finite rate between 0.1 and 1000".into());
            }
        }
        Value::Choice(choices) => {
            if !choices.contains(&text) {
                return Err(format!("use {}", choices.join("|")));
            }
        }
        Value::Actors | Value::Damage => {
            #[cfg(feature = "dev-tools")]
            {
                let mut options = alleycat::dev_tools::DevOptions::default();
                if matches!(spec.value, Value::Actors) {
                    options.set_disabled_actors(text)?;
                } else {
                    options.set_disabled_damage(text)?;
                }
            }
        }
        Value::Flag | Value::Path => unreachable!(),
    }
    Ok(value)
}

fn last(entries: &[Entry], name: &str) -> Option<usize> {
    entries.iter().rposition(|entry| entry.spec.name == name)
}

fn remove(entries: &mut Vec<Entry>, index: usize, reason: &str, warnings: &mut Vec<String>) {
    let entry = entries.remove(index);
    warnings.push(format!("{} {reason}; ignored", entry.spec.name));
}

fn resolve_constraints(entries: &mut Vec<Entry>, game: Game, warnings: &mut Vec<String>) {
    loop {
        let active = |name| last(entries, name);
        if game == Game::Mach3 {
            let conflict = [("--static", "--controls"), ("--static", "--step")]
                .into_iter()
                .find_map(|(a, b)| Some((active(a)?, active(b)?, a, b)));
            if let Some((a, b, aname, bname)) = conflict {
                remove(
                    entries,
                    a.max(b),
                    &format!("conflicts with {}", if a > b { bname } else { aname }),
                    warnings,
                );
                continue;
            }
            break;
        }
        if active("--headless").is_none() {
            if let Some(index) = active("--script").or_else(|| active("--timed")) {
                remove(entries, index, "requires --headless", warnings);
                continue;
            }
        }
        let scene = active("--scene")
            .and_then(|i| entries[i].value.as_ref())
            .and_then(|v| v.to_str())
            .unwrap_or("alley");
        if scene == "splash" {
            if let Some(index) = ["--script", "--trace", "--trace-timing", "--step-hz"]
                .into_iter()
                .find_map(active)
            {
                remove(entries, index, "does not apply to --scene splash", warnings);
                continue;
            }
        }
        if !["alley", "birdcage", "splash"].contains(&scene) {
            if let Some(index) = active("--step-hz") {
                remove(
                    entries,
                    index,
                    "does not apply to this timed scene",
                    warnings,
                );
                continue;
            }
            if active("--headless").is_some() && active("--timed").is_none() {
                let index = active("--scene").expect("non-default scene has an option");
                remove(
                    entries,
                    index,
                    "requires --timed for headless execution",
                    warnings,
                );
                continue;
            }
        }
        if let Some(step) = active("--step-hz") {
            if let Some(other) = active("--timed").or_else(|| active("--trace-timing")) {
                let index = step.max(other);
                remove(
                    entries,
                    index,
                    "conflicts with the selected timing mode",
                    warnings,
                );
                continue;
            }
        }
        if active("--headless").is_some() && active("--timed").is_none() {
            if let Some(index) = active("--trace-timing") {
                remove(
                    entries,
                    index,
                    "requires --timed for headless execution",
                    warnings,
                );
                continue;
            }
        }
        break;
    }
}

pub fn prepare(args: impl IntoIterator<Item = OsString>, game: Game) -> Prepared {
    let mut args = args.into_iter().peekable();
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    while let Some(arg) = args.next() {
        let (name, inline) = split_option(&arg);
        let Some(spec) = SPECS
            .iter()
            .find(|spec| spec.name == name && (spec.game.is_none() || spec.game == Some(game)))
            .or_else(|| SPECS.iter().find(|spec| spec.name == name))
        else {
            warnings.push(format!("unrecognized argument {arg:?}; ignored"));
            continue;
        };
        let value = if matches!(spec.value, Value::Flag) {
            inline
        } else {
            inline.or_else(|| args.next_if(|next| !is_option(next)))
        };
        if let Some(owner) = spec.game.filter(|owner| *owner != game) {
            warnings.push(format!(
                "{} applies to {}, not {}; ignored",
                spec.name,
                owner.title(),
                game.title()
            ));
            continue;
        }
        if spec.dev && !cfg!(feature = "dev-tools") {
            warnings.push(format!("{} requires a dev-tools build; ignored", spec.name));
            continue;
        }
        if matches!(spec.name, "--original" | "--help" | "--list") {
            continue;
        }
        if matches!(spec.value, Value::Flag) {
            if value.is_some() {
                warnings.push(format!(
                    "{} is a flag and takes no value; ignored",
                    spec.name
                ));
                continue;
            }
            entries.push(Entry { spec, value: None });
        } else {
            let Some(value) = value else {
                warnings.push(format!(
                    "{} requires a value; ignored (previous value or default retained)",
                    spec.name
                ));
                continue;
            };
            match validate(spec, value, game) {
                Ok(value) => entries.push(Entry {
                    spec,
                    value: Some(value),
                }),
                Err(error) => warnings.push(format!(
                    "{}: {error}; ignored (previous value or default retained)",
                    spec.name
                )),
            }
        }
    }
    resolve_constraints(&mut entries, game, &mut warnings);
    let mut result = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if last(&entries, entry.spec.name) != Some(index) {
            continue;
        }
        result.push(entry.spec.name.into());
        result.extend(entry.value.iter().cloned());
    }
    Prepared {
        args: result,
        warnings,
    }
}

pub fn help(game: Option<Game>) -> String {
    let mut text = String::from(
        "REforge\n\nUsage: reforge --original <file-or-directory> [options]\n       reforge --help\n       reforge --list\n\nDetection checks filenames, then sizes, then SHA-256 hashes.\nInvalid, unavailable and other-game options are warned about and ignored.\nOriginal data is required and is never included.\n\nCommon options:\n",
    );
    for owner in [None, game]
        .into_iter()
        .take(if game.is_some() { 2 } else { 1 })
    {
        if let Some(owner) = owner {
            text.push_str(&format!("\n{} options:\n", owner.title()));
        }
        for spec in SPECS
            .iter()
            .filter(|spec| spec.game == owner && (!spec.dev || cfg!(feature = "dev-tools")))
        {
            let value = match spec.value {
                Value::Flag => "",
                Value::Path => " PATH",
                _ => " VALUE",
            };
            text.push_str(&format!(
                "  {}{}{}  {}\n",
                spec.name,
                value,
                if spec.dev { " [dev-tools]" } else { "" },
                spec.description
            ));
            if let Value::Choice(choices) = spec.value {
                text.push_str(&format!("    Choices: {}\n", choices.join(", ")));
            }
        }
    }
    if game == Some(Game::AlleyCat) && cfg!(feature = "dev-tools") {
        text.push_str("\nDamage identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, air.\nActor identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, bin-cat, alley-mice, broom, hearts-arrows, hearts-rivals.\nDisable lists replace the previous selection; damage suppression leaves actors active.\n");
    }
    if game.is_none() {
        text.push_str("\nUse --original <path> --help for detected-game options.\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    fn accepted(result: &Prepared, game: Game) {
        let mut args = args(&["--original", "synthetic-missing-original"]);
        args.extend(result.args.iter().cloned());
        if game == Game::AlleyCat {
            alleycat::cli::validate_arguments(
                args.iter().map(|arg| arg.to_str().unwrap().to_owned()),
            )
            .unwrap();
        }
        #[cfg(feature = "sdl")]
        if game == Game::Mach3 {
            mach3::cli::validate_arguments(args.into_iter()).unwrap();
        }
    }
    #[test]
    fn scale_defaults_are_left_to_engines_and_auto_is_forwarded() {
        assert_eq!(takes_value("--scale"), Some(true));
        assert!(help(None).contains("auto|1..16 (default auto for both games)"));
        for game in [Game::AlleyCat, Game::Mach3] {
            let defaults = prepare(args(&[]), game);
            assert!(defaults.args.is_empty());
            assert!(defaults.warnings.is_empty());
            accepted(&defaults, game);
            for input in [vec!["--scale", "auto"], vec!["--scale=auto"]] {
                let result = prepare(args(&input), game);
                assert_eq!(result.args, args(&["--scale", "auto"]));
                assert!(result.warnings.is_empty());
                accepted(&result, game);
            }
            for (value, expected) in [("1", "1"), ("16", "16"), ("0x4", "4")] {
                let result = prepare(args(&["--scale", value]), game);
                assert_eq!(result.args, args(&["--scale", expected]));
                assert!(result.warnings.is_empty());
                accepted(&result, game);
            }
            for (first, last) in [("auto", "3"), ("3", "auto")] {
                let result = prepare(args(&["--scale", first, "--scale", last]), game);
                assert_eq!(result.args, args(&["--scale", last]));
                assert!(result.warnings.is_empty());
                accepted(&result, game);
            }
        }
    }

    #[test]
    fn invalid_scale_warns_and_retains_auto_fixed_or_engine_default() {
        for game in [Game::AlleyCat, Game::Mach3] {
            for value in ["0", "17", "-1", "1.5", "Auto", "", "0x", "4294967296"] {
                for previous in [None, Some("auto"), Some("3")] {
                    let mut input = Vec::new();
                    if let Some(previous) = previous {
                        input.extend(["--scale", previous]);
                    }
                    let invalid = format!("--scale={value}");
                    input.push(&invalid);
                    let result = prepare(args(&input), game);
                    let expected = previous
                        .map(|value| args(&["--scale", value]))
                        .unwrap_or_default();
                    assert_eq!(result.args, expected, "{value:?}");
                    assert_eq!(result.warnings.len(), 1, "{value:?}");
                    accepted(&result, game);
                }
            }
        }
    }

    #[test]
    fn invalid_values_and_foreign_options_warn_and_preserve_previous_values() {
        let result = prepare(
            args(&[
                "--scale",
                "3",
                "--scale",
                "0",
                "--scene",
                "cheese",
                "--period-ms",
                "NaN",
                "--machine",
                "pcjr",
                "--unknown",
                "extra",
                "--controls=true",
            ]),
            Game::Mach3,
        );
        assert_eq!(result.args, args(&["--scale", "3"]));
        assert_eq!(result.warnings.len(), 7);
        accepted(&result, Game::Mach3);
    }
    #[test]
    fn flags_do_not_consume_following_options_and_inline_values_work() {
        let result = prepare(
            args(&["--scale", "--controls", "--period-ms=12", "--scale=0x4"]),
            Game::Mach3,
        );
        assert_eq!(
            result.args,
            args(&["--controls", "--period-ms", "12", "--scale", "4"])
        );
        assert_eq!(result.warnings.len(), 1);
        accepted(&result, Game::Mach3);
        let result = prepare(
            args(&["--controls", "--difficulty=2", "--machine=pcjr"]),
            Game::AlleyCat,
        );
        assert_eq!(
            result.args,
            args(&["--difficulty", "2", "--machine", "pcjr"])
        );
        assert_eq!(result.warnings.len(), 1);
        accepted(&result, Game::AlleyCat);
    }
    #[test]
    fn incompatible_mach_modes_ignore_later_conflict_and_collapse_repeats() {
        for (input, expected) in [
            (
                vec!["--controls", "--static", "--step", "--step"],
                vec!["--controls", "--step"],
            ),
            (vec!["--static", "--controls", "--step"], vec!["--static"]),
        ] {
            let result = prepare(args(&input), Game::Mach3);
            assert_eq!(result.args, args(&expected));
            assert!(!result.warnings.is_empty());
            accepted(&result, Game::Mach3);
        }
    }
    #[test]
    fn mach3_audio_renderer_choices_reach_engine_in_normal_and_preview_modes() {
        assert_eq!(takes_value("--audio-renderer"), Some(true));
        for renderer in ["speaker", "enhanced-pcm"] {
            for preview in [
                None,
                Some("--step"),
                Some("--static"),
                Some("--period-ms=12"),
            ] {
                let mut input = vec![format!("--audio-renderer={renderer}")];
                if let Some(preview) = preview {
                    input.push(preview.into());
                }
                let result = prepare(input.into_iter().map(OsString::from), Game::Mach3);
                assert!(result.warnings.is_empty());
                assert_eq!(&result.args[..2], args(&["--audio-renderer", renderer]));
                accepted(&result, Game::Mach3);
            }
        }
    }
    #[test]
    fn audio_renderer_validation_retains_previous_value_and_following_options() {
        let result = prepare(
            args(&[
                "--audio-renderer",
                "enhanced-pcm",
                "--audio-renderer=pcm",
                "--audio-renderer=",
                "--audio-renderer",
                "--controls",
            ]),
            Game::Mach3,
        );
        assert_eq!(
            result.args,
            args(&["--audio-renderer", "enhanced-pcm", "--controls"])
        );
        assert_eq!(result.warnings.len(), 3);
        assert!(
            result.warnings[..2]
                .iter()
                .all(|warning| warning.contains("use speaker|enhanced-pcm"))
        );
        assert!(result.warnings[2].contains("requires a value"));
        accepted(&result, Game::Mach3);

        let repeated = prepare(
            args(&["--audio-renderer=enhanced-pcm", "--audio-renderer=speaker"]),
            Game::Mach3,
        );
        assert_eq!(repeated.args, args(&["--audio-renderer", "speaker"]));
        assert!(repeated.warnings.is_empty());
        accepted(&repeated, Game::Mach3);
    }
    #[test]
    fn audio_renderer_is_mach3_only_and_appears_in_its_help() {
        let foreign = prepare(
            args(&["--audio-renderer", "enhanced-pcm", "--scale", "2"]),
            Game::AlleyCat,
        );
        assert_eq!(foreign.args, args(&["--scale", "2"]));
        assert_eq!(foreign.warnings.len(), 1);
        assert!(foreign.warnings[0].contains("applies to Mach 3"));
        accepted(&foreign, Game::AlleyCat);
        let help = help(Some(Game::Mach3));
        assert!(help.contains("--audio-renderer VALUE"));
        assert!(help.contains("Choices: speaker, enhanced-pcm"));
        assert!(help.contains("default speaker"));
        assert!(!super::help(None).contains("--audio-renderer"));
        assert!(!super::help(Some(Game::AlleyCat)).contains("--audio-renderer"));
    }
    #[test]
    fn mach3_f3_mode_choices_reach_engine_in_normal_and_preview_modes() {
        assert_eq!(takes_value("--f3-mode"), Some(true));
        for mode in ["press", "original"] {
            for preview in [
                None,
                Some("--step"),
                Some("--static"),
                Some("--period-ms=12"),
            ] {
                let mut input = vec![format!("--f3-mode={mode}")];
                if let Some(preview) = preview {
                    input.push(preview.into());
                }
                let result = prepare(input.into_iter().map(OsString::from), Game::Mach3);
                assert!(result.warnings.is_empty());
                assert_eq!(&result.args[..2], args(&["--f3-mode", mode]));
                accepted(&result, Game::Mach3);
            }
        }
    }
    #[test]
    fn f3_mode_validation_preserves_previous_value_and_following_options() {
        let result = prepare(
            args(&[
                "--f3-mode=original",
                "--f3-mode=repeat",
                "--f3-mode=",
                "--f3-mode",
                "--controls",
            ]),
            Game::Mach3,
        );
        assert_eq!(result.args, args(&["--f3-mode", "original", "--controls"]));
        assert_eq!(result.warnings.len(), 3);
        assert!(
            result.warnings[..2]
                .iter()
                .all(|warning| warning.contains("use press|original"))
        );
        assert!(result.warnings[2].contains("requires a value"));
        accepted(&result, Game::Mach3);
        let repeated = prepare(
            args(&["--f3-mode=original", "--f3-mode=press"]),
            Game::Mach3,
        );
        assert_eq!(repeated.args, args(&["--f3-mode", "press"]));
        assert!(repeated.warnings.is_empty());
        accepted(&repeated, Game::Mach3);
    }
    #[test]
    fn f3_mode_is_mach3_only_and_present_in_normal_help() {
        let foreign = prepare(args(&["--f3-mode=original", "--scale=2"]), Game::AlleyCat);
        assert_eq!(foreign.args, args(&["--scale", "2"]));
        assert_eq!(foreign.warnings.len(), 1);
        assert!(foreign.warnings[0].contains("applies to Mach 3"));
        accepted(&foreign, Game::AlleyCat);
        let help = help(Some(Game::Mach3));
        assert!(help.contains("--f3-mode VALUE"));
        assert!(help.contains("Choices: press, original"));
        assert!(help.contains("default press"));
        assert!(!super::help(None).contains("--f3-mode"));
        assert!(!super::help(Some(Game::AlleyCat)).contains("--f3-mode"));
    }
    #[test]
    fn shared_option_names_have_consistent_value_arity() {
        // Essential argument extraction runs before game detection.
        for spec in SPECS {
            for other in SPECS.iter().filter(|other| other.name == spec.name) {
                assert_eq!(
                    matches!(spec.value, Value::Flag),
                    matches!(other.value, Value::Flag)
                );
                if !std::ptr::eq(spec, other) {
                    assert!(spec.game.is_some() && other.game.is_some());
                    assert_ne!(spec.game, other.game);
                }
            }
        }
    }
    #[test]
    fn catalog_option_names_match_engine_parsers() {
        for (source, game) in [
            (
                include_str!("../../engines/alleycat/src/cli.rs"),
                Game::AlleyCat,
            ),
            (include_str!("../../engines/mach3/src/cli.rs"), Game::Mach3),
        ] {
            for line in source.lines() {
                let line = line.trim();
                if !line.contains("=>") || !line.contains("\"--") {
                    continue;
                }
                for quoted in line
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .filter(|word| word.starts_with("--"))
                {
                    if quoted == "--" {
                        continue;
                    }
                    let spec = SPECS
                        .iter()
                        .find(|spec| {
                            spec.name == quoted && (spec.game.is_none() || spec.game == Some(game))
                        })
                        .unwrap_or_else(|| panic!("uncataloged {quoted}"));
                    assert!(spec.game.is_none() || spec.game == Some(game), "{quoted}");
                }
            }
        }
        for spec in SPECS
            .iter()
            .filter(|s| s.game.is_some() && (!s.dev || cfg!(feature = "dev-tools")))
        {
            let game = spec.game.unwrap();
            let mut values = vec![spec.name];
            let numeric;
            match spec.value {
                Value::Flag => {}
                Value::Path => values.push("synthetic-path"),
                Value::Integer(min, _) => {
                    numeric = min.to_string();
                    values.push(&numeric);
                }
                Value::Rate => values.push("1"),
                Value::Scale => values.push("auto"),
                Value::Choice(choices) => values.push(choices[0]),
                Value::Actors | Value::Damage => values.push("none"),
            }
            if spec.name == "--script" || spec.name == "--timed" {
                values.push("--headless");
            }
            let result = prepare(args(&values), game);
            assert!(
                result.warnings.is_empty(),
                "{}: {:?}",
                spec.name,
                result.warnings
            );
            accepted(&result, game);
        }
    }
    #[test]
    fn unavailable_development_options_are_recognized_and_excluded() {
        let result = prepare(
            args(&["--scene", "cheese", "--seed", "12", "--infinite-lives"]),
            Game::AlleyCat,
        );
        #[cfg(not(feature = "dev-tools"))]
        {
            assert!(result.args.is_empty());
            assert_eq!(result.warnings.len(), 3);
            assert!(result.warnings.iter().all(|w| w.contains("dev-tools")));
            assert!(!help(Some(Game::AlleyCat)).contains("--scene"));
        }
        #[cfg(feature = "dev-tools")]
        {
            assert!(result.warnings.is_empty());
            assert!(help(Some(Game::AlleyCat)).contains("--scene"));
        }
        accepted(&result, Game::AlleyCat);
    }
    #[test]
    fn mach3_traces_are_development_only() {
        for option in ["--trace-input", "--trace-timing"] {
            assert_eq!(takes_value(option), Some(true));
            let result = prepare(args(&[option, "trace.tsv"]), Game::Mach3);
            #[cfg(not(feature = "dev-tools"))]
            {
                assert!(result.args.is_empty());
                assert_eq!(result.warnings.len(), 1);
                assert!(result.warnings[0].contains("requires a dev-tools build"));
                assert!(!help(Some(Game::Mach3)).contains(option));
            }
            #[cfg(feature = "dev-tools")]
            {
                assert_eq!(result.args, args(&[option, "trace.tsv"]));
                assert!(result.warnings.is_empty());
                assert!(help(Some(Game::Mach3)).contains(&format!("{option} PATH [dev-tools]")));
            }
            accepted(&result, Game::Mach3);
            let foreign = prepare(args(&[option, "trace.tsv"]), Game::AlleyCat);
            if option == "--trace-input" {
                assert!(foreign.args.is_empty());
                assert_eq!(foreign.warnings.len(), 1);
                assert!(foreign.warnings[0].contains("applies to Mach 3"));
                assert!(!help(Some(Game::AlleyCat)).contains(option));
            } else {
                #[cfg(feature = "dev-tools")]
                {
                    assert_eq!(foreign.args, args(&[option, "trace.tsv"]));
                    assert!(foreign.warnings.is_empty());
                    assert!(
                        help(Some(Game::AlleyCat)).contains("Write timed outer-loop events as CSV")
                    );
                    assert!(
                        help(Some(Game::Mach3))
                            .contains("Write simulation and presentation timing")
                    );
                }
                #[cfg(not(feature = "dev-tools"))]
                {
                    assert!(foreign.args.is_empty());
                    assert_eq!(foreign.warnings.len(), 1);
                    assert!(foreign.warnings[0].contains("requires a dev-tools build"));
                }
            }
            assert!(!help(None).contains(option));
            accepted(&foreign, Game::AlleyCat);
        }
    }
    #[cfg(feature = "dev-tools")]
    #[test]
    fn inline_trace_filename_starting_with_dash_reaches_engine_as_a_path() {
        for option in ["--trace-input", "--trace-timing"] {
            let result = prepare(args(&[&format!("{option}=-trace.tsv")]), Game::Mach3);
            assert_eq!(
                result.args,
                vec![
                    OsString::from(option),
                    std::path::PathBuf::from(".")
                        .join("-trace.tsv")
                        .into_os_string()
                ]
            );
            assert!(result.warnings.is_empty());
            accepted(&result, Game::Mach3);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn invalid_trace_paths_preserve_previous_path_and_following_options() {
        for option in ["--trace-input", "--trace-timing"] {
            let result = prepare(
                args(&[
                    &format!("{option}=trace.tsv"),
                    &format!("{option}="),
                    option,
                    "--controls",
                ]),
                Game::Mach3,
            );
            assert_eq!(result.args, args(&[option, "trace.tsv", "--controls"]));
            assert_eq!(result.warnings.len(), 2);
            assert!(result.warnings[0].contains("path must not be empty"));
            assert!(result.warnings[1].contains("requires a value"));
            accepted(&result, Game::Mach3);
        }
    }
    #[cfg(all(feature = "dev-tools", unix))]
    #[test]
    fn mach3_traces_preserve_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"trace-\xff.tsv".to_vec());
        for option in ["--trace-input", "--trace-timing"] {
            let result = prepare([OsString::from(option), path.clone()], Game::Mach3);
            assert_eq!(result.args, vec![OsString::from(option), path.clone()]);
            assert!(result.warnings.is_empty());
            accepted(&result, Game::Mach3);
        }
    }
    #[cfg(feature = "dev-tools")]
    #[test]
    fn damage_lists_validate_atomically_and_timing_constraints_are_recoverable() {
        let result = prepare(
            args(&[
                "--disable-damage",
                "all",
                "--disable-damage",
                "eels,bogus",
                "--scene",
                "cheese",
                "--headless",
                "--step-hz",
                "12",
                "--trace-timing",
                "trace",
            ]),
            Game::AlleyCat,
        );
        assert_eq!(
            result.args,
            args(&["--disable-damage", "all", "--headless"])
        );
        assert_eq!(result.warnings.len(), 4);
        accepted(&result, Game::AlleyCat);
        let result = prepare(
            args(&[
                "--headless",
                "--timed",
                "--scene",
                "splash",
                "--script",
                "script",
                "--trace",
                "trace",
            ]),
            Game::AlleyCat,
        );
        assert_eq!(result.warnings.len(), 2);
        accepted(&result, Game::AlleyCat);
    }
}
