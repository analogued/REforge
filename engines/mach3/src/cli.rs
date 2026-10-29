// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

#[cfg(feature = "dev-tools")]
use crate::viewer::Playback;
use crate::{
    gameplay::{DisplayMode, F3Mode},
    sound::render::AudioRenderer,
    startup::StartupFrames,
    viewer::Options,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: reforge --original <assets-directory> [--scale auto|1..16] [--presentation pixels|hyllian] [--audio-renderer speaker|enhanced-pcm] [--video cga|mono] [--f3-mode press|original] [--digitized-samples on|off]\n\n\
--original: directory containing MACH3.EXE and DATA1 through DATAH for normal animated playback.\n\
Space: advance splash; intro skip waits for the current sample/group and tail. Ctrl+Q, Escape or window close: exit.\n\
F10: open settings; F10, Escape or Close returns. Scaling and fullscreen changes apply live.\n\
Default drawable scale: auto (fit the usable display); CGA pixels are displayed at 4:3.\n\
--presentation pixels|hyllian: nearest pixels (default) or the shared Hyllian CRT shader; unavailable shaders warn and fall back to pixels.\n\
Gameplay continues until game over, then displays Game Over and High Score.\n\
Space returns to intro after the first post-game title pair.\n\
Arrows or S/X/B/N steer; Space or keypad5 fires. Gameplay has no loop cap.\n\
Normal animation runs gameplay, sound and waits on one PIT timeline.\n\
--audio-renderer speaker|enhanced-pcm: select host audio rendering (default speaker); timing is unchanged.\n\
--digitized-samples on|off: execute recorded speech/music (default on); off retains procedural effects and skips sample waits.\n\
P or Pause/Break freezes host simulation and playback. F1 retains original pause/end-game behavior; F2 toggles native sound, F3 switches color/monochrome.\n\
--video cga|mono: initial native colour or monochrome display (default cga); F3 still toggles during play.\n\
--f3-mode press|original: toggle F3 once per press (default press), or at every active original control poll.\n\
Unavailable audio warns and mutes host output; guest execution continues. Exact CPU-dependent pacing remains unmodeled.";

fn help() -> String {
    #[cfg(feature = "dev-tools")]
    {
        format!(
            "{HELP}\nF9 [dev-tools]: toggle the presentation FPS indicator (hidden by default).\n--step [dev-tools]: fresh Enter presses advance one body or title step.\n--static [dev-tools]: show initial scene only.\nThese previews use silent nominal stepping and need MACH3.EXE and DATA1 through DATA6.\n--trace-input PATH [dev-tools]: write input diagnostic trace to a new file.\n--trace-timing PATH [dev-tools]: write host execution and scheduling spans to a new file."
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
    let program = crate::executable::expand_program(&read("MACH3.EXE").map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let rules = crate::gameplay::ScriptRules::from_data_segment(
        crate::executable::data_segment(&program).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(feature = "dev-tools")]
    if options.is_preview() {
        return crate::viewer::run_preview(&frames, options, rules);
    }
    let samples = crate::sound::SoundAssets::load_with(&mut read).map_err(|e| e.to_string())?;
    let intro = crate::sound::IntroProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    let procedural =
        crate::sound::ProceduralProgram::from_expanded(&program).map_err(|e| e.to_string())?;
    crate::viewer::run_live(&frames, options, rules, samples, intro, procedural)
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
    Ok(Some((directory, options)))
}

#[cfg(test)]
mod tests {
    use super::{help, parse_args};
    #[cfg(feature = "dev-tools")]
    use crate::viewer::Playback;
    use crate::{gameplay::F3Mode, sound::render::AudioRenderer};
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn verified_file_choices_override_case_collisions_without_silent_fallback() {
        use std::{
            collections::BTreeMap,
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };
        let directory = std::env::temp_dir().join(format!(
            "mach3-selected-files-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        for (name, content) in [
            ("MACH3.EXE", b"wrong".as_slice()),
            ("mach3.exe", b"chosen program"),
            ("DATA1", b"wrong"),
            ("data1", b"chosen image"),
            ("DATA7", b"unlisted sample"),
        ] {
            fs::write(directory.join(name), content).unwrap();
        }
        let files = BTreeMap::from([
            ("mach3.exe", directory.join("mach3.exe")),
            ("DATA1", directory.join("data1")),
        ]);
        assert_eq!(
            super::read_selected(&directory, &files, "MACH3.EXE").unwrap(),
            b"chosen program"
        );
        assert_eq!(
            super::read_selected(&directory, &files, "DATA1").unwrap(),
            b"chosen image"
        );
        assert_eq!(
            super::read_selected(&directory, &files, "DATA7").unwrap(),
            b"unlisted sample"
        );
        fs::remove_file(directory.join("data1")).unwrap();
        assert!(super::read_selected(&directory, &files, "DATA1").is_err());
        fs::remove_dir_all(&directory).unwrap();
    }

    fn args(values: &[&str]) -> impl Iterator<Item = OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn playback_flags() -> Vec<Option<&'static str>> {
        let mut flags = vec![None];
        if cfg!(feature = "dev-tools") {
            flags.extend([Some("--step"), Some("--static")]);
        }
        flags
    }

    #[test]
    fn video_argument_selects_native_startup_mode_in_every_playback() {
        use crate::gameplay::DisplayMode;
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.video, DisplayMode::Color);
        for playback in playback_flags() {
            for (value, expected) in [
                ("cga", DisplayMode::Color),
                ("mono", DisplayMode::Monochrome),
            ] {
                let mut input = vec!["--original", "assets", "--video", value];
                if let Some(flag) = playback {
                    input.push(flag);
                }
                let (_, options) = parse_args(args(&input)).unwrap().unwrap();
                assert_eq!(options.video, expected);
            }
        }
        let (_, options) = parse_args(args(&[
            "--original",
            "assets",
            "--video",
            "mono",
            "--video",
            "cga",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(options.video, DisplayMode::Color);
        for value in ["", "colour", "MONO", "--scale"] {
            assert!(
                parse_args(args(&["--original", "assets", "--video", value]))
                    .unwrap_err()
                    .contains("--video")
            );
        }
        assert!(
            parse_args(args(&["--original", "assets", "--video"]))
                .unwrap_err()
                .contains("--video")
        );
    }

    #[test]
    fn removed_flags_are_rejected_without_aliases() {
        for flag in [
            "--controls",
            "--no-controls",
            "--no-input",
            "--period-ms",
            "--preview-period-ms",
        ] {
            assert!(
                parse_args(args(&["--original", "assets", flag])).is_err(),
                "{flag}"
            );
            assert!(!help().contains(flag), "{flag}");
        }
    }

    #[test]
    fn preview_flags_require_dev_tools() {
        for flag in ["--step", "--static"] {
            let result = parse_args(args(&["--original", "assets", flag]));
            assert_eq!(result.is_ok(), cfg!(feature = "dev-tools"), "{flag}");
            assert_eq!(help().contains(flag), cfg!(feature = "dev-tools"), "{flag}");
        }
        #[cfg(feature = "dev-tools")]
        for flags in [
            ["--step", "--static"],
            ["--static", "--step"],
            ["--step", "--step"],
            ["--static", "--static"],
        ] {
            assert!(parse_args(args(&["--original", "assets", flags[0], flags[1]])).is_err());
        }
    }

    #[test]
    fn presentation_defaults_to_pixels_and_reaches_all_playback_modes() {
        let (_, defaults) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert_eq!(defaults.presentation, reforge_sdl::Presentation::Pixels);
        assert!(help().contains("--presentation pixels|hyllian"));
        for value in ["pixels", "hyllian"] {
            for mode in playback_flags() {
                let mut input = vec!["--original", "assets", "--presentation", value];
                if let Some(mode) = mode {
                    input.push(mode);
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
        for value in ["", "crt", "Hyllian", "--scale"] {
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
    fn controls_are_enabled_with_original_path_and_scale_in_either_order() {
        for values in [
            &["--original", "assets with spaces", "--scale", "2"][..],
            &["--scale", "2", "--original", "assets with spaces"][..],
        ] {
            let (directory, options) = parse_args(args(values)).unwrap().unwrap();
            assert_eq!(directory, PathBuf::from("assets with spaces"));
            assert!(options.controls);
            assert_eq!(options.scale, Some(2));
        }
        let (_, options) = parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap();
        assert!(options.controls);
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
                #[cfg(feature = "dev-tools")]
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
            &["--original", "assets", "--audio-renderer", "--scale"][..],
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
            for preview in playback_flags() {
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
            &["--original", "assets", "--f3-mode", "--scale"][..],
        ] {
            assert!(
                parse_args(args(values)).unwrap_err().contains("--f3-mode"),
                "{values:?}"
            );
        }
    }

    #[cfg(feature = "dev-tools")]
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
    }

    #[test]
    fn requires_one_explicit_original_directory() {
        for values in [
            &[][..],
            &["assets"][..],
            &["--original"][..],
            &["--original", "--scale"][..],
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
            &["--original", "assets", "--trace-timing", "--scale"][..],
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
            assert!(options.controls);
        }
        for values in [
            &["--original", "assets", "--trace-input"][..],
            &["--original", "assets", "--trace-input", ""][..],
            &["--original", "assets", "--trace-input", "--scale"][..],
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
