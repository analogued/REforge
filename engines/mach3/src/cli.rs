// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use crate::{
    gameplay::F3Mode,
    sound::render::AudioRenderer,
    startup::StartupFrames,
    viewer::{Options, Playback},
};
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Usage: reforge --original <assets-directory> [--scale auto|1..16] [--presentation pixels|hyllian] [--step|--static] [--controls] [--period-ms 1..5000] [--audio-renderer speaker|enhanced-pcm] [--f3-mode press|original] [--digitized-samples on|off]\n\n\
--original: directory containing MACH3.EXE and DATA1 through DATAH for normal animated playback.\n\
Space: advance splash; intro skip waits for the current sample/group and tail. Ctrl+Q, Escape or window close: exit.\n\
F10: open settings; F10, Escape or Close returns. Scaling and fullscreen changes apply live.\n\
Default drawable scale: auto (fit the usable display); CGA pixels are displayed at 4:3.\n\
--presentation pixels|hyllian: nearest pixels (default) or the shared Hyllian CRT shader; unavailable shaders warn and fall back to pixels.\n\
Gameplay continues until game over, then displays Game Over and High Score.\n\
Space returns to intro after the first post-game title pair.\n\
--step: fresh Enter presses advance one body or title step. --static: show initial scene only.\n\
--controls: flag with no value; arrows or S/X/B/N steer; Space or keypad5 fires. Gameplay has no loop cap.\n\
Normal animation runs gameplay, sound and waits on one PIT timeline.\n\
--audio-renderer speaker|enhanced-pcm: select host audio rendering (default speaker); timing is unchanged.\n\
--digitized-samples on|off: execute recorded speech/music (default on); off retains procedural effects and skips sample waits.\n\
--step, --static and custom --period-ms previews use silent nominal stepping and need DATA1 through DATA6.\n\
P or Pause/Break freezes host simulation and playback. F1 retains original pause/end-game behavior; F2 toggles native sound, F3 switches color/monochrome.\n\
--f3-mode press|original: toggle F3 once per press (default press), or at every active original control poll.\n\
Unavailable audio warns and mutes host output; guest execution continues. Exact CPU-dependent pacing remains unmodeled.";

fn help() -> String {
    #[cfg(feature = "dev-tools")]
    {
        format!(
            "{HELP}\n--trace-input PATH [dev-tools]: write input diagnostic trace to a new file.\n--trace-timing PATH [dev-tools]: write host execution and scheduling spans to a new file."
        )
    }
    #[cfg(not(feature = "dev-tools"))]
    {
        HELP.into()
    }
}

/// Run the Mach 3 viewer with arguments excluding the executable name.
///
/// The `viewer` feature provides this entry point for REforge and the standalone
/// `mach3-viewer` binary.
pub fn run(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let Some((directory, options)) = parse_args(args)? else {
        println!("{}", help());
        return Ok(());
    };
    let frames = StartupFrames::load(&directory).map_err(|error| error.to_string())?;
    let program = crate::executable::load_program(&directory).map_err(|error| error.to_string())?;
    let rules = crate::gameplay::ScriptRules::from_data_segment(
        crate::executable::data_segment(&program).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if options.is_preview() {
        return crate::viewer::run_preview(&frames, options, rules);
    }
    let samples = crate::sound::SoundAssets::load(&directory).map_err(|e| e.to_string())?;
    let intro = crate::sound::IntroProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    let procedural =
        crate::sound::ProceduralProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    crate::viewer::run_live(&frames, options, rules, samples, intro, procedural)
}

/// Validate the viewer argument contract without loading original data or SDL.
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
            Some("--controls") => {
                options.controls = true;
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
            Some("--f3-mode") => {
                options.f3_mode = match args.next().as_deref().and_then(|arg| arg.to_str()) {
                    Some("press") => F3Mode::Press,
                    Some("original") => F3Mode::Original,
                    _ => {
                        return Err("--f3-mode must be press or original".into());
                    }
                };
            }
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
            Some("--period-ms") => {
                let period = args
                    .next()
                    .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u64>().ok()))
                    .filter(|period| (1..=5000).contains(period))
                    .ok_or_else(|| {
                        "--period-ms must be an integer between 1 and 5000".to_string()
                    })?;
                options.preview_period = Some(std::time::Duration::from_millis(period));
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
            Some(value) if value.starts_with('-') => return Err(help()),
            _ => {
                return Err(format!(
                    "Unexpected positional argument: {}. Supply the data directory with --original",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    if options.controls && options.playback == Playback::Static {
        return Err("--controls cannot be combined with --static".into());
    }
    let directory =
        directory.ok_or_else(|| "Missing required --original <assets-directory>".to_string())?;
    Ok(Some((directory, options)))
}

#[cfg(test)]
mod tests {
    use super::{help, parse_args};
    use crate::{gameplay::F3Mode, sound::render::AudioRenderer, viewer::Playback};
    use std::{ffi::OsString, path::PathBuf};

    fn args(values: &[&str]) -> impl Iterator<Item = OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn presentation_defaults_to_pixels_and_reaches_all_playback_modes() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.presentation, reforge_sdl::Presentation::Pixels);
        assert!(help().contains("--presentation pixels|hyllian"));
        for value in ["pixels", "hyllian"] {
            for mode in [None, Some("--step"), Some("--static"), Some("--period-ms")] {
                let mut input = vec!["--original", "assets", "--presentation", value];
                if let Some(mode) = mode {
                    input.push(mode);
                    if mode == "--period-ms" {
                        input.push("12");
                    }
                }
                let (_, options) = parse_args(args(&input)).unwrap().unwrap();
                assert_eq!(options.presentation, value.parse().unwrap());
            }
        }
        let (_, options) = parse_args(args(&[
            "--original",
            "assets",
            "--presentation",
            "hyllian",
            "--presentation",
            "pixels",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.presentation, reforge_sdl::Presentation::Pixels);
        for value in ["", "crt", "Hyllian", "--controls"] {
            assert!(parse_args(args(&["--original", "assets", "--presentation", value])).is_err());
        }
        assert!(parse_args(args(&["--original", "assets", "--presentation"])).is_err());
    }

    #[test]
    fn scale_defaults_to_auto_and_last_explicit_selection_wins() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.scale, None);
        assert!(help().contains("--scale auto|1..16"));
        for (value, expected) in [("auto", None), ("1", Some(1)), ("16", Some(16))] {
            let (_, options) = parse_args(args(&["--original", "assets", "--scale", value]))
                .unwrap()
                .unwrap();
            assert_eq!(options.scale, expected);
        }
        for (first, last, expected) in [("auto", "3", Some(3)), ("3", "auto", None)] {
            let (_, options) = parse_args(args(&[
                "--original",
                "assets",
                "--scale",
                first,
                "--scale",
                last,
            ]))
            .unwrap()
            .unwrap();
            assert_eq!(options.scale, expected);
        }
        for value in ["0", "17", "-1", "1.5", "Auto", "", "4294967296"] {
            assert!(
                parse_args(args(&["--original", "assets", "--scale", value]))
                    .unwrap_err()
                    .contains("--scale"),
                "{value:?}"
            );
        }
        assert!(parse_args(args(&["--original", "assets", "--scale"])).is_err());
    }

    #[test]
    fn original_path_and_controls_are_independent_in_either_order() {
        for values in [
            &[
                "--original",
                "assets with spaces",
                "--controls",
                "--scale",
                "2",
            ][..],
            &[
                "--controls",
                "--scale",
                "2",
                "--original",
                "assets with spaces",
            ][..],
        ] {
            let (directory, options) = parse_args(args(values)).unwrap().unwrap();
            assert_eq!(directory, PathBuf::from("assets with spaces"));
            assert!(options.controls);
            assert_eq!(options.scale, Some(2));
        }
        let (_, options) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert!(!options.controls);
    }

    #[test]
    fn native_sample_option_defaults_to_preferences_and_accepts_original_spellings() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.digitized_samples, None);
        for (extra, expected) in [
            (vec!["S"], false),
            (vec!["s"], false),
            (vec!["--digitized-samples", "off"], false),
            (vec!["--digitized-samples", "on"], true),
        ] {
            let mut values = vec!["--original", "assets"];
            values.extend(extra);
            let (_, options) = parse_args(args(&values)).unwrap().unwrap();
            assert_eq!(options.digitized_samples, Some(expected));
        }
        for bad in [
            vec!["--digitized-samples"],
            vec!["--digitized-samples", "yes"],
        ] {
            let mut values = vec!["--original", "assets"];
            values.extend(bad);
            assert!(
                parse_args(args(&values))
                    .unwrap_err()
                    .contains("--digitized-samples")
            );
        }
        let (directory, options) = parse_args(args(&["--original", "s"])).unwrap().unwrap();
        assert_eq!(directory, PathBuf::from("s"));
        assert_eq!(options.digitized_samples, None);
    }

    #[test]
    fn audio_renderer_defaults_to_speaker_and_accepts_both_policies() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.audio_renderer, AudioRenderer::Speaker);
        for (value, renderer) in [
            ("speaker", AudioRenderer::Speaker),
            ("enhanced-pcm", AudioRenderer::EnhancedPcm),
        ] {
            for values in [
                vec!["--original", "assets", "--audio-renderer", value],
                vec!["--audio-renderer", value, "--original", "assets"],
            ] {
                let (_, options) = parse_args(args(&values)).unwrap().unwrap();
                assert_eq!(options.audio_renderer, renderer);
                assert_eq!(options.playback, Playback::Animated);
                assert_eq!(options.preview_period, None);
                assert!(!options.is_preview());
            }
        }
        let (_, options) = parse_args(args(&[
            "--original",
            "assets",
            "--audio-renderer",
            "enhanced-pcm",
            "--audio-renderer",
            "speaker",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.audio_renderer, AudioRenderer::Speaker);
        assert!(help().contains("--audio-renderer speaker|enhanced-pcm"));
        assert!(help().contains("default speaker"));
    }

    #[test]
    fn invalid_audio_renderer_values_are_rejected() {
        for values in [
            &["--original", "assets", "--audio-renderer"][..],
            &["--original", "assets", "--audio-renderer", ""][..],
            &["--original", "assets", "--audio-renderer", "pcm"][..],
            &["--original", "assets", "--audio-renderer", "Speaker"][..],
            &["--original", "assets", "--audio-renderer", "--controls"][..],
        ] {
            assert!(
                parse_args(args(values))
                    .unwrap_err()
                    .contains("--audio-renderer"),
                "{values:?}"
            );
        }
    }

    #[test]
    fn f3_mode_defaults_to_press_and_is_available_in_all_viewer_modes() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.f3_mode, F3Mode::Press);
        for (value, mode) in [("press", F3Mode::Press), ("original", F3Mode::Original)] {
            for preview in [None, Some("--step"), Some("--static")] {
                let mut values = vec!["--f3-mode", value, "--original", "assets"];
                if let Some(preview) = preview {
                    values.push(preview);
                }
                let (_, options) = parse_args(args(&values)).unwrap().unwrap();
                assert_eq!(options.f3_mode, mode);
            }
        }
        let (_, options) = parse_args(args(&[
            "--original",
            "assets",
            "--f3-mode",
            "original",
            "--f3-mode",
            "press",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.f3_mode, F3Mode::Press);
        assert!(help().contains("--f3-mode press|original"));
        assert!(help().contains("default press"));
    }

    #[test]
    fn invalid_f3_mode_values_are_rejected() {
        for values in [
            &["--original", "assets", "--f3-mode"][..],
            &["--original", "assets", "--f3-mode", ""][..],
            &["--original", "assets", "--f3-mode", "repeat"][..],
            &["--original", "assets", "--f3-mode", "Press"][..],
            &["--original", "assets", "--f3-mode", "--controls"][..],
        ] {
            assert!(
                parse_args(args(values)).unwrap_err().contains("--f3-mode"),
                "{values:?}"
            );
        }
    }

    #[test]
    fn audio_renderer_selection_preserves_silent_preview_modes() {
        for (flag, playback) in [("--step", Playback::Step), ("--static", Playback::Static)] {
            let (_, options) = parse_args(args(&[
                "--original",
                "assets",
                "--audio-renderer",
                "enhanced-pcm",
                flag,
            ]))
            .unwrap()
            .unwrap();
            assert_eq!(options.audio_renderer, AudioRenderer::EnhancedPcm);
            assert_eq!(options.playback, playback);
        }
        let (_, options) = parse_args(args(&[
            "--original",
            "assets",
            "--audio-renderer",
            "enhanced-pcm",
            "--period-ms",
            "12",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.audio_renderer, AudioRenderer::EnhancedPcm);
        assert_eq!(
            options.preview_period,
            Some(std::time::Duration::from_millis(12))
        );
        assert!(options.is_preview());
    }

    #[test]
    fn requires_one_explicit_original_directory() {
        for values in [
            &[][..],
            &["--controls"][..],
            &["assets"][..],
            &["--controls", "assets"][..],
            &["--original"][..],
            &["--original", "--controls"][..],
            &["--original", "assets", "extra"][..],
            &["--original", "assets", "--original", "other"][..],
        ] {
            assert!(
                parse_args(args(values)).unwrap_err().contains("--original"),
                "{values:?}"
            );
        }
    }

    #[test]
    fn help_does_not_require_or_load_original_data() {
        for flag in ["--help", "-h"] {
            assert!(parse_args(args(&[flag])).unwrap().is_none());
        }
        assert_eq!(
            help().contains("--trace-input"),
            cfg!(feature = "dev-tools")
        );
    }

    #[cfg(not(feature = "dev-tools"))]
    #[test]
    fn input_trace_is_excluded_from_normal_parser() {
        assert!(
            parse_args(args(&[
                "--original",
                "assets",
                "--trace-input",
                "trace.tsv"
            ]))
            .is_err()
        );
    }

    #[test]
    fn timing_trace_option_is_feature_gated_and_requires_a_path() {
        let valid = parse_args(args(&[
            "--original",
            "assets",
            "--trace-timing",
            "timing trace.tsv",
        ]));
        #[cfg(feature = "dev-tools")]
        {
            let (_, options) = valid.unwrap().unwrap();
            assert_eq!(
                options.timing_trace,
                Some(PathBuf::from("timing trace.tsv"))
            );
        }
        #[cfg(not(feature = "dev-tools"))]
        assert!(valid.is_err());
        assert_eq!(
            help().contains("--trace-timing"),
            cfg!(feature = "dev-tools")
        );
        for values in [
            &["--original", "assets", "--trace-timing"][..],
            &["--original", "assets", "--trace-timing", ""][..],
            &["--original", "assets", "--trace-timing", "--controls"][..],
        ] {
            assert!(parse_args(args(values)).is_err());
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn trace_path_is_optional_and_independent_of_controls() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.input_trace, None);
        for values in [
            &["--original", "assets", "--trace-input", "input trace.tsv"][..],
            &["--trace-input", "input trace.tsv", "--original", "assets"][..],
        ] {
            let (_, options) = parse_args(args(values)).unwrap().unwrap();
            assert_eq!(options.input_trace, Some(PathBuf::from("input trace.tsv")));
            assert!(!options.controls);
        }
        for values in [
            &["--original", "assets", "--trace-input"][..],
            &["--original", "assets", "--trace-input", ""][..],
            &["--original", "assets", "--trace-input", "--controls"][..],
        ] {
            assert!(
                parse_args(args(values))
                    .unwrap_err()
                    .contains("--trace-input")
            );
        }
        assert!(
            parse_args(args(&["--trace-input", "trace.tsv"]))
                .unwrap_err()
                .contains("Missing required --original")
        );
    }

    #[cfg(all(feature = "dev-tools", unix))]
    #[test]
    fn trace_path_preserves_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"trace-\xff.tsv".to_vec());
        let (_, options) = parse_args(
            [
                OsString::from("--original"),
                OsString::from("assets"),
                OsString::from("--trace-input"),
                path.clone(),
            ]
            .into_iter(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(options.input_trace.unwrap().into_os_string(), path);
    }

    #[cfg(unix)]
    #[test]
    fn original_directory_preserves_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"assets-\xff".to_vec());
        let (directory, _) = parse_args([OsString::from("--original"), path.clone()].into_iter())
            .unwrap()
            .unwrap();
        assert_eq!(directory.into_os_string(), path);
    }
}
