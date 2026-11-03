// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Common,
    Game(Game),
    Btz,
}
impl Scope {
    fn applies(self, game: Game) -> bool {
        match self {
            Self::Common => true,
            Self::Game(owner) => owner == game,
            Self::Btz => game.is_btz(),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Common => "all games",
            Self::Game(game) => game.title(),
            Self::Btz => "BTZ games",
        }
    }
}

struct Spec {
    name: &'static str,
    scope: Scope,
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
        scope: Scope::Common,
        value: Value::Path,
        dev: false,
        description: "Original game file or directory; identify a verified release automatically",
    },
    Spec {
        name: "--scale",
        scope: Scope::Common,
        value: Value::Scale,
        dev: false,
        description: "Initial drawable pixel multiplier, auto|1..16 (default auto)",
    },
    Spec {
        name: "--config",
        scope: Scope::Common,
        value: Value::Path,
        dev: false,
        description: "Read/write preferences at this path",
    },
    Spec {
        name: "--no-config",
        scope: Scope::Common,
        value: Value::Flag,
        dev: false,
        description: "Run without reading or saving preferences",
    },
    Spec {
        name: "--window-mode",
        scope: Scope::Common,
        value: Value::Choice(&["windowed", "fullscreen"]),
        dev: false,
        description: "Temporary window mode override",
    },
    Spec {
        name: "--renderer",
        scope: Scope::Common,
        value: Value::Choice(&["auto", "gpu", "sdl", "software"]),
        dev: false,
        description: "Temporary renderer override",
    },
    Spec {
        name: "--backend",
        scope: Scope::Common,
        value: Value::Choice(&[
            "auto",
            "vulkan",
            "opengl",
            "opengles2",
            "direct3d11",
            "direct3d12",
            "metal",
        ]),
        dev: false,
        description: "Temporary renderer backend override",
    },
    Spec {
        name: "--vsync",
        scope: Scope::Common,
        value: Value::Choice(&["on", "off"]),
        dev: false,
        description: "Temporary VSync override",
    },
    Spec {
        name: "--frame-limit",
        scope: Scope::Common,
        value: Value::Choice(&["display", "60", "120", "unlimited"]),
        dev: false,
        description: "Temporary presentation ceiling (display refresh, 60, 120 or unlimited)",
    },
    Spec {
        name: "--presentation",
        scope: Scope::Common,
        value: Value::Choice(&["pixels", "hyllian"]),
        dev: false,
        description: "Temporary window presentation override: pixels or Hyllian CRT",
    },
    Spec {
        name: "--help",
        scope: Scope::Common,
        value: Value::Flag,
        dev: false,
        description: "Show common help, or detected-game help with --original",
    },
    Spec {
        name: "--list",
        scope: Scope::Common,
        value: Value::Flag,
        dev: false,
        description: "List supported games",
    },
    Spec {
        name: "--version",
        scope: Scope::Common,
        value: Value::Flag,
        dev: false,
        description: "Show the REforge version (-V)",
    },
    Spec {
        name: "--video",
        scope: Scope::Game(Game::Mach3),
        value: Value::Choice(&["cga", "mono"]),
        dev: false,
        description: "Initial video mode (default cga); F3 switches during play",
    },
    Spec {
        name: "--save-dir",
        scope: Scope::Btz,
        value: Value::Path,
        dev: false,
        description: "Directory for prompted bookmark and transcript filenames (default game directory)",
    },
    Spec {
        name: "--bookmark-path",
        scope: Scope::Btz,
        value: Value::Path,
        dev: false,
        description: "Use this bookmark file for save and restore without filename prompts",
    },
    Spec {
        name: "--transcript-path",
        scope: Scope::Btz,
        value: Value::Path,
        dev: false,
        description: "Use this transcript filename without prompting",
    },
    Spec {
        name: "--side-map",
        scope: Scope::Btz,
        value: Value::Path,
        dev: true,
        description: "Use this disk mapping file (default SIDE.TXT beside the game data)",
    },
    Spec {
        name: "--machine",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Choice(&["cga", "pcjr"]),
        dev: false,
        description: "Hardware path (default cga)",
    },
    Spec {
        name: "--difficulty",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Integer(0, 7),
        dev: false,
        description: "Initial difficulty, 0..7; skip skill question",
    },
    Spec {
        name: "--scene",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Choice(SCENES),
        dev: true,
        description: "Start in the named scene",
    },
    Spec {
        name: "--seed",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Integer(2, 65535),
        dev: false,
        description: "Explicit startup seed, 2..65535 or 0xHEX (default timer-derived)",
    },
    Spec {
        name: "--step-hz",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Rate,
        dev: true,
        description: "Diagnostic accepted updates/sec, 0.1..1000",
    },
    Spec {
        name: "--headless",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Run without a window",
    },
    Spec {
        name: "--timed",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Use the reference clock in headless runs",
    },
    Spec {
        name: "--steps",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Integer(0, usize::MAX as u64),
        dev: true,
        description: "Number of accepted updates (default 60 headless)",
    },
    Spec {
        name: "--script",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Headless input CSV: x,y[,action]",
    },
    Spec {
        name: "--trace",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write accepted player states as CSV",
    },
    Spec {
        name: "--trace-timing",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write timed outer-loop events as CSV",
    },
    Spec {
        name: "--dump-cga",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write final packed framebuffer",
    },
    Spec {
        name: "--dump-ppm",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Path,
        dev: true,
        description: "Write final 320x200 image",
    },
    Spec {
        name: "--disable-damage",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Damage,
        dev: true,
        description: "all, none or comma-separated damage identifiers",
    },
    Spec {
        name: "--disable-actors",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Actors,
        dev: true,
        description: "all, none or comma-separated actor identifiers",
    },
    Spec {
        name: "--infinite-lives",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Retain lives while preserving death effects",
    },
    Spec {
        name: "--original-window-timing",
        scope: Scope::Game(Game::AlleyCat),
        value: Value::Flag,
        dev: true,
        description: "Restore original difficulty-6 window hold",
    },
    Spec {
        name: "--audio-renderer",
        scope: Scope::Game(Game::Mach3),
        value: Value::Choice(&["speaker", "enhanced-pcm"]),
        dev: false,
        description: "Host audio rendering for normal animation (default speaker); previews stay silent",
    },
    Spec {
        name: "--digitized-samples",
        scope: Scope::Game(Game::Mach3),
        value: Value::Choice(&["on", "off"]),
        dev: false,
        description: "Enable recorded speech/music (default on); off retains procedural effects",
    },
    Spec {
        name: "--f3-mode",
        scope: Scope::Game(Game::Mach3),
        value: Value::Choice(&["press", "original"]),
        dev: false,
        description: "F3 once per press (default press), or every active original control poll",
    },
    Spec {
        name: "--step",
        scope: Scope::Game(Game::Mach3),
        value: Value::Flag,
        dev: true,
        description: "Advance one body/title step per fresh Enter press",
    },
    Spec {
        name: "--static",
        scope: Scope::Game(Game::Mach3),
        value: Value::Flag,
        dev: true,
        description: "Show initial scene only",
    },
    Spec {
        name: "--trace-input",
        scope: Scope::Game(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Write input diagnostic trace to a new file",
    },
    Spec {
        name: "--trace-timing",
        scope: Scope::Game(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Write simulation and presentation timing to a new file",
    },
    Spec {
        name: "--motion",
        scope: Scope::Game(Game::Mach3),
        value: Value::Choice(&["scene", "enhanced", "original"]),
        dev: false,
        description: "Select interpolated scene motion (default), craft-only interpolation or original presentation",
    },
    Spec {
        name: "--trace-motion",
        scope: Scope::Game(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Write optional motion diagnostics to a new file",
    },
    Spec {
        name: "--motion-experiment",
        scope: Scope::Game(Game::Mach3),
        value: Value::Path,
        dev: true,
        description: "Alias for --motion enhanced --trace-motion PATH",
    },
];

pub fn takes_value(name: &str) -> Option<bool> {
    if matches!(name, "-h" | "-V") {
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
        if spec.scope == Scope::Game(Game::Mach3)
            && matches!(
                spec.name,
                "--trace-input" | "--trace-timing" | "--trace-motion" | "--motion-experiment"
            )
            && is_option(&value)
        {
            return Ok(std::path::PathBuf::from(".").join(value).into_os_string());
        }
        return Ok(value);
    }
    let text = value.to_str().ok_or("value must be UTF-8")?;
    if spec.name == "--renderer" && text == "terminal" && game.is_btz() {
        return Ok(value);
    }
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
            let conflict = [("--static", "--step")]
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
        if game.is_btz() {
            let terminal = active("--renderer")
                .and_then(|i| entries[i].value.as_ref())
                .is_some_and(|v| v == "terminal");
            if terminal
                && let Some(index) = [
                    "--backend",
                    "--window-mode",
                    "--scale",
                    "--vsync",
                    "--frame-limit",
                    "--presentation",
                ]
                .into_iter()
                .find_map(active)
            {
                remove(
                    entries,
                    index,
                    "does not apply to the terminal renderer",
                    warnings,
                );
                continue;
            }
            break;
        }
        if active("--headless").is_some()
            && let Some(index) = active("--presentation").or_else(|| active("--frame-limit"))
        {
            remove(entries, index, "does not apply to --headless", warnings);
            continue;
        }
        if active("--headless").is_none()
            && let Some(index) = active("--script").or_else(|| active("--timed"))
        {
            remove(entries, index, "requires --headless", warnings);
            continue;
        }
        let scene = active("--scene")
            .and_then(|i| entries[i].value.as_ref())
            .and_then(|v| v.to_str())
            .unwrap_or("alley");
        if scene == "splash"
            && let Some(index) = ["--script", "--trace", "--trace-timing", "--step-hz"]
                .into_iter()
                .find_map(active)
        {
            remove(entries, index, "does not apply to --scene splash", warnings);
            continue;
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
        if let Some(step) = active("--step-hz")
            && let Some(other) = active("--timed").or_else(|| active("--trace-timing"))
        {
            let index = step.max(other);
            remove(
                entries,
                index,
                "conflicts with the selected timing mode",
                warnings,
            );
            continue;
        }
        if active("--headless").is_some()
            && active("--timed").is_none()
            && let Some(index) = active("--trace-timing")
        {
            remove(
                entries,
                index,
                "requires --timed for headless execution",
                warnings,
            );
            continue;
        }
        break;
    }
}

pub fn prepare(args: impl IntoIterator<Item = OsString>, game: Game) -> Prepared {
    let mut args = args.into_iter().peekable();
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    while let Some(arg) = args.next() {
        // Hidden original Mach 3 spelling. Normalize only bare arguments, never
        // option values or paths; ordinary ownership checks ignore it elsewhere.
        let arg = if arg == "S" || arg == "s" {
            OsString::from("--digitized-samples=off")
        } else {
            arg
        };
        let (name, inline) = split_option(&arg);
        let Some(spec) = SPECS
            .iter()
            .find(|spec| spec.name == name && spec.scope.applies(game))
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
        if !spec.scope.applies(game) {
            warnings.push(format!(
                "{} applies to {}, not {}; ignored",
                spec.name,
                spec.scope.label(),
                game.title()
            ));
            continue;
        }
        if spec.dev && !cfg!(feature = "dev-tools") {
            warnings.push(format!("{} requires a dev-tools build; ignored", spec.name));
            continue;
        }
        if matches!(spec.name, "--presentation" | "--frame-limit") && !cfg!(feature = "sdl") {
            warnings.push(format!("{} requires an SDL build; ignored", spec.name));
            continue;
        }
        if matches!(spec.name, "--original" | "--help" | "--list" | "--version") {
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
        "REforge\n\nUsage: reforge\n       reforge --original <file-or-directory> [options]\n       reforge --help\n       reforge --list\n       reforge --version\n\nWithout arguments, open the launcher and saved game library.\nDetection checks filenames, then sizes, then SHA-256 hashes.\nInvalid, unavailable and other-game options are warned about and ignored.\nOriginal data is required and is never included.\n\nCommon options:\n",
    );
    for owner in [None, game]
        .into_iter()
        .take(if game.is_some() { 2 } else { 1 })
    {
        if let Some(owner) = owner {
            text.push_str(&format!("\n{} options:\n", owner.title()));
        }
        for spec in SPECS.iter().filter(|spec| {
            (if let Some(game) = owner {
                spec.scope != Scope::Common && spec.scope.applies(game)
            } else {
                spec.scope == Scope::Common
            }) && (!spec.dev || cfg!(feature = "dev-tools"))
        }) {
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
                let suffix = if spec.name == "--renderer" && game.is_some_and(Game::is_btz) {
                    ", terminal (attached terminal)"
                } else {
                    ""
                };
                text.push_str(&format!("    Choices: {}{suffix}\n", choices.join(", ")));
            }
        }
    }
    if game == Some(Game::AlleyCat) && cfg!(feature = "dev-tools") {
        text.push_str("\nDamage identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, air.\nActor identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, bin-cat, alley-mice, broom, hearts-arrows, hearts-rivals.\nDisable lists replace the previous selection; damage suppression leaves actors active.\n");
    }
    #[cfg(all(feature = "sdl", feature = "dev-tools"))]
    text.push_str("\nF9 toggles the presentation FPS indicator (hidden by default).\n");
    if game.is_none() {
        text.push_str("\nUse --original <path> --help for detected-game options.\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_renderer_is_a_mindwheel_player_option() {
        let result = prepare(args(&["--renderer", "terminal"]), Game::Mindwheel);
        assert!(result.warnings.is_empty());
        assert_eq!(result.args, args(&["--renderer", "terminal"]));
        accepted(&result, Game::Mindwheel);
        assert!(help(Some(Game::Mindwheel)).contains("software, terminal"));
        for game in [Game::AlleyCat, Game::Mach3] {
            let result = prepare(args(&["--renderer", "terminal"]), game);
            assert!(result.args.is_empty());
            assert_eq!(result.warnings.len(), 1);
            assert!(!help(Some(game)).contains("software, terminal"));
        }
    }

    #[test]
    fn terminal_renderer_ignores_window_only_overrides() {
        let result = prepare(
            args(&[
                "--renderer",
                "terminal",
                "--backend",
                "opengl",
                "--scale",
                "3",
                "--window-mode",
                "fullscreen",
                "--presentation",
                "pixels",
                "--save-dir",
                "saves",
            ]),
            Game::Mindwheel,
        );
        assert_eq!(
            result.args,
            args(&["--renderer", "terminal", "--save-dir", "saves"])
        );
        assert!(result.warnings.len() >= 4);
    }

    #[test]
    fn mindwheel_file_options_preserve_paths_and_reject_other_games() {
        for option in ["--save-dir", "--bookmark-path", "--transcript-path"] {
            let result = prepare(args(&[option, "a path with spaces"]), Game::Mindwheel);
            assert!(result.warnings.is_empty());
            assert_eq!(result.args, args(&[option, "a path with spaces"]));
            for game in [Game::AlleyCat, Game::Mach3] {
                let result = prepare(args(&[option, "a path with spaces"]), game);
                assert!(result.args.is_empty());
                assert_eq!(result.warnings.len(), 1);
            }
        }
    }

    #[test]
    fn side_map_override_requires_development_build() {
        let input = args(&["--side-map", "a path with spaces"]);
        let result = prepare(input.clone(), Game::Mindwheel);
        assert_eq!(
            help(Some(Game::Mindwheel)).contains("--side-map"),
            cfg!(feature = "dev-tools")
        );
        if cfg!(feature = "dev-tools") {
            assert!(result.warnings.is_empty());
            assert_eq!(result.args, input);
            accepted(&result, Game::Mindwheel);
        } else {
            assert!(result.args.is_empty());
            assert_eq!(
                result.warnings,
                ["--side-map requires a dev-tools build; ignored"]
            );
        }
        for game in [Game::AlleyCat, Game::Mach3] {
            let result = prepare(input.clone(), game);
            assert!(result.args.is_empty());
            assert_eq!(result.warnings.len(), 1);
        }
    }

    #[test]
    fn fps_shortcut_help_requires_graphical_development_build() {
        assert_eq!(
            help(None).contains("F9"),
            cfg!(all(feature = "sdl", feature = "dev-tools"))
        );
    }

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
        if game.is_btz() {
            btz::cli::validate_arguments(args.clone().into_iter()).unwrap();
        }
        #[cfg(feature = "sdl")]
        if game == Game::Mach3 {
            mach3::cli::validate_arguments(args.into_iter()).unwrap();
        }
    }
    #[test]
    fn digitized_samples_and_hidden_original_spellings_are_mach3_only() {
        for (input, value) in [
            (vec!["S"], "off"),
            (vec!["s"], "off"),
            (vec!["--digitized-samples=off"], "off"),
            (vec!["--digitized-samples", "on"], "on"),
            (vec!["S", "--digitized-samples=on"], "on"),
            (vec!["--digitized-samples=on", "s"], "off"),
        ] {
            let result = prepare(args(&input), Game::Mach3);
            assert!(result.warnings.is_empty());
            assert_eq!(result.args, args(&["--digitized-samples", value]));
            accepted(&result, Game::Mach3);
            let other = prepare(args(&input), Game::AlleyCat);
            assert!(other.args.is_empty());
            assert!(!other.warnings.is_empty());
        }
        let result = prepare(args(&["--config", "S", "--scale", "2"]), Game::Mach3);
        assert!(result.warnings.is_empty());
        assert_eq!(result.args, args(&["--config", "S", "--scale", "2"]));
        let result = prepare(
            args(&["--digitized-samples=off", "--digitized-samples=bad"]),
            Game::Mach3,
        );
        assert_eq!(result.args, args(&["--digitized-samples", "off"]));
        assert_eq!(result.warnings.len(), 1);
        assert!(help(Some(Game::Mach3)).contains("--digitized-samples"));
        assert!(!help(Some(Game::AlleyCat)).contains("--digitized-samples"));
        assert!(!help(None).contains("--digitized-samples"));
    }
    #[test]
    fn frame_limit_choices_are_shared_and_invalid_values_warn() {
        for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex] {
            for value in ["display", "60", "120", "unlimited"] {
                let result = prepare(args(&["--frame-limit", value]), game);
                if cfg!(feature = "sdl") {
                    assert_eq!(result.args, args(&["--frame-limit", value]));
                    assert!(result.warnings.is_empty());
                } else {
                    assert!(result.args.is_empty());
                }
            }
            let result = prepare(args(&["--frame-limit", "60", "--frame-limit", "bad"]), game);
            assert!(!result.warnings.is_empty());
            if cfg!(feature = "sdl") {
                assert_eq!(result.args, args(&["--frame-limit", "60"]));
            }
        }
    }
    #[test]
    fn shared_presentation_choices_and_build_availability() {
        assert_eq!(takes_value("--presentation"), Some(true));
        for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex] {
            let help = help(Some(game));
            assert!(help.contains("--presentation VALUE"));
            assert!(help.contains("Choices: pixels, hyllian"));
            assert!(help.contains("Temporary window presentation override"));
            for value in ["pixels", "hyllian"] {
                for input in [
                    vec!["--presentation", value],
                    vec![&format!("--presentation={value}")],
                ] {
                    let result = prepare(args(&input), game);
                    if cfg!(feature = "sdl") {
                        assert_eq!(result.args, args(&["--presentation", value]));
                        assert!(result.warnings.is_empty());
                    } else {
                        assert!(result.args.is_empty());
                        assert_eq!(result.warnings.len(), 1);
                        assert!(result.warnings[0].contains("requires an SDL build"));
                    }
                    accepted(&result, game);
                }
            }
        }
    }

    #[cfg(feature = "sdl")]
    #[test]
    fn presentation_validation_retains_previous_value_and_following_options() {
        for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex] {
            for value in ["", "crt", "Hyllian", "-1"] {
                let result = prepare(
                    args(&[
                        "--presentation=hyllian",
                        &format!("--presentation={value}"),
                        "--presentation",
                        "--scale=2",
                    ]),
                    game,
                );
                assert_eq!(
                    result.args,
                    args(&["--presentation", "hyllian", "--scale", "2"])
                );
                assert_eq!(result.warnings.len(), 2);
                assert!(result.warnings[0].contains("use pixels|hyllian"));
                assert!(result.warnings[1].contains("requires a value"));
                accepted(&result, game);
            }
            let result = prepare(
                args(&["--presentation=hyllian", "--presentation=pixels"]),
                game,
            );
            assert_eq!(result.args, args(&["--presentation", "pixels"]));
            assert!(result.warnings.is_empty());
            accepted(&result, game);
        }
    }

    #[cfg(all(feature = "sdl", feature = "dev-tools"))]
    #[test]
    fn headless_presentation_warns_and_is_ignored_in_either_order() {
        for input in [
            vec!["--presentation=hyllian", "--headless"],
            vec!["--headless", "--presentation=pixels"],
        ] {
            let result = prepare(args(&input), Game::AlleyCat);
            assert_eq!(result.args, args(&["--headless"]));
            assert_eq!(result.warnings.len(), 1);
            assert!(result.warnings[0].contains("does not apply to --headless"));
            accepted(&result, Game::AlleyCat);
        }
    }

    #[test]
    fn scale_defaults_are_left_to_engines_and_auto_is_forwarded() {
        assert_eq!(takes_value("--scale"), Some(true));
        assert!(help(None).contains("auto|1..16 (default auto)"));
        for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex] {
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
        for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex] {
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
                "--audio-renderer",
                "NaN",
                "--machine",
                "pcjr",
                "--unknown",
                "extra",
                "--step=true",
            ]),
            Game::Mach3,
        );
        assert_eq!(result.args, args(&["--scale", "3"]));
        assert_eq!(result.warnings.len(), 7);
        accepted(&result, Game::Mach3);
    }
    #[test]
    fn inline_values_and_following_options_are_preserved() {
        let result = prepare(
            args(&["--scale", "--f3-mode=original", "--scale=0x4"]),
            Game::Mach3,
        );
        assert_eq!(
            result.args,
            args(&["--f3-mode", "original", "--scale", "4"])
        );
        assert_eq!(result.warnings.len(), 1);
        accepted(&result, Game::Mach3);
        let result = prepare(
            args(&[
                "--audio-renderer=speaker",
                "--difficulty=2",
                "--machine=pcjr",
            ]),
            Game::AlleyCat,
        );
        assert_eq!(
            result.args,
            args(&["--difficulty", "2", "--machine", "pcjr"])
        );
        assert_eq!(result.warnings.len(), 1);
        accepted(&result, Game::AlleyCat);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn incompatible_preview_modes_ignore_later_conflict_and_collapse_repeats() {
        for (input, expected) in [
            (vec!["--step", "--static", "--step"], vec!["--step"]),
            (vec!["--static", "--step", "--static"], vec!["--static"]),
        ] {
            let result = prepare(args(&input), Game::Mach3);
            assert_eq!(result.args, args(&expected));
            assert_eq!(result.warnings.len(), 2);
            accepted(&result, Game::Mach3);
        }
        let result = prepare(args(&["--scale", "--step", "--scale=4"]), Game::Mach3);
        assert_eq!(result.args, args(&["--step", "--scale", "4"]));
        assert_eq!(result.warnings.len(), 1);
        accepted(&result, Game::Mach3);
    }

    #[test]
    fn removed_mach3_flags_have_no_aliases() {
        for flag in [
            "--controls",
            "--no-controls",
            "--no-input",
            "--period-ms",
            "--preview-period-ms",
        ] {
            assert_eq!(takes_value(flag), None);
            assert!(!help(Some(Game::Mach3)).contains(flag));
            let result = prepare(args(&[flag, "--scale=3"]), Game::Mach3);
            assert_eq!(result.args, args(&["--scale", "3"]));
            assert_eq!(result.warnings.len(), 1);
            assert!(result.warnings[0].contains("unrecognized argument"));
            accepted(&result, Game::Mach3);
        }
    }

    #[test]
    fn preview_flags_are_development_only() {
        for flag in ["--step", "--static"] {
            assert_eq!(takes_value(flag), Some(false));
            let result = prepare(args(&[flag]), Game::Mach3);
            #[cfg(feature = "dev-tools")]
            {
                assert_eq!(result.args, args(&[flag]));
                assert!(result.warnings.is_empty());
                assert!(help(Some(Game::Mach3)).contains(&format!("{flag} [dev-tools]")));
            }
            #[cfg(not(feature = "dev-tools"))]
            {
                assert!(result.args.is_empty());
                assert_eq!(result.warnings.len(), 1);
                assert!(result.warnings[0].contains("requires a dev-tools build"));
                assert!(!help(Some(Game::Mach3)).contains(flag));
            }
            accepted(&result, Game::Mach3);
        }
    }

    fn preview_flags() -> Vec<Option<&'static str>> {
        let mut flags = vec![None];
        if cfg!(feature = "dev-tools") {
            flags.extend([Some("--step"), Some("--static")]);
        }
        flags
    }

    #[test]
    fn mach3_audio_renderer_choices_reach_engine_in_normal_and_preview_modes() {
        assert_eq!(takes_value("--audio-renderer"), Some(true));
        for renderer in ["speaker", "enhanced-pcm"] {
            for preview in preview_flags() {
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
                "--f3-mode=press",
            ]),
            Game::Mach3,
        );
        assert_eq!(
            result.args,
            args(&["--audio-renderer", "enhanced-pcm", "--f3-mode", "press"])
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
            for preview in preview_flags() {
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
                "--scale=2",
            ]),
            Game::Mach3,
        );
        assert_eq!(
            result.args,
            args(&["--f3-mode", "original", "--scale", "2"])
        );
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
                    assert!(spec.scope != Scope::Common && other.scope != Scope::Common);
                    assert_ne!(spec.scope, other.scope);
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
                for quoted in
                    line.split('"').skip(1).step_by(2).filter(|word| {
                        word.starts_with("--") && !word.chars().any(char::is_whitespace)
                    })
                {
                    if quoted == "--" {
                        continue;
                    }
                    let spec = SPECS
                        .iter()
                        .find(|spec| spec.name == quoted && spec.scope.applies(game))
                        .unwrap_or_else(|| panic!("uncataloged {quoted}"));
                    assert!(spec.scope.applies(game), "{quoted}");
                }
            }
        }
        for spec in SPECS
            .iter()
            .filter(|s| s.scope != Scope::Common && (!s.dev || cfg!(feature = "dev-tools")))
        {
            for game in [Game::AlleyCat, Game::Mach3, Game::Mindwheel, Game::Essex]
                .into_iter()
                .filter(|&game| spec.scope.applies(game))
            {
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
    }
    #[test]
    fn alleycat_seed_is_a_player_option_and_is_rejected_for_other_games() {
        let result = prepare(args(&["--seed", "0xfa59"]), Game::AlleyCat);
        assert!(result.warnings.is_empty());
        assert_eq!(result.args, args(&["--seed", "64089"]));
        assert!(help(Some(Game::AlleyCat)).contains("--seed"));
        accepted(&result, Game::AlleyCat);
        let other = prepare(args(&["--seed", "0xfa59"]), Game::Mach3);
        assert!(other.args.is_empty());
        assert_eq!(other.warnings.len(), 1);
        for value in ["0", "1", "65536"] {
            let invalid = prepare(args(&["--seed", value]), Game::AlleyCat);
            assert!(invalid.args.is_empty());
            assert_eq!(invalid.warnings.len(), 1);
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
            assert_eq!(result.args, args(&["--seed", "12"]));
            assert_eq!(result.warnings.len(), 2);
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
        for option in ["--trace-input", "--trace-timing", "--trace-motion"] {
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
            if option != "--trace-timing" {
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
    #[test]
    fn motion_selection_is_game_scoped_in_normal_builds() {
        for mode in ["scene", "enhanced", "original"] {
            let selected = prepare(args(&["--motion", mode]), Game::Mach3);
            assert_eq!(selected.args, args(&["--motion", mode]));
            assert!(selected.warnings.is_empty());
        }
        let invalid = prepare(
            args(&[
                "--motion",
                "original",
                "--motion",
                "fast",
                "--f3-mode",
                "press",
            ]),
            Game::Mach3,
        );
        assert_eq!(
            invalid.args,
            args(&["--motion", "original", "--f3-mode", "press"])
        );
        assert_eq!(invalid.warnings.len(), 1);
        let foreign = prepare(args(&["--motion", "original"]), Game::AlleyCat);
        assert!(foreign.args.is_empty());
        assert_eq!(foreign.warnings.len(), 1);
    }
    #[cfg(feature = "dev-tools")]
    #[test]
    fn inline_trace_filename_starting_with_dash_reaches_engine_as_a_path() {
        for option in ["--trace-input", "--trace-timing", "--trace-motion"] {
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
        for option in ["--trace-input", "--trace-timing", "--trace-motion"] {
            let result = prepare(
                args(&[
                    &format!("{option}=trace.tsv"),
                    &format!("{option}="),
                    option,
                    "--f3-mode=press",
                ]),
                Game::Mach3,
            );
            assert_eq!(
                result.args,
                args(&[option, "trace.tsv", "--f3-mode", "press"])
            );
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
        for option in ["--trace-input", "--trace-timing", "--trace-motion"] {
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
