// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{MotionMode, help, parse_args};
#[cfg(feature = "dev-tools")]
use crate::platform::Playback;
use crate::{gameplay::F3Mode, sound::render::AudioRenderer};
use std::{ffi::OsString, path::PathBuf};

#[test]
fn motion_experiment_requires_a_path_and_is_feature_gated() {
    let parsed = parse_args(args(&[
        "--original",
        "assets",
        "--motion-experiment",
        "motion.tsv",
    ]));
    #[cfg(feature = "dev-tools")]
    assert_eq!(
        parsed.unwrap().unwrap().1.motion_trace,
        Some(PathBuf::from("motion.tsv"))
    );
    #[cfg(not(feature = "dev-tools"))]
    assert!(parsed.is_err());
    for suffix in [
        vec!["--motion-experiment"],
        vec!["--motion-experiment", ""],
        vec!["--motion-experiment", "--scale"],
    ] {
        let mut argv = vec!["--original", "assets"];
        argv.extend(suffix);
        assert!(parse_args(args(&argv)).is_err());
    }
    #[cfg(feature = "dev-tools")]
    assert!(
        parse_args(args(&[
            "--original",
            "assets",
            "--motion-experiment",
            "motion.tsv",
            "--step"
        ]))
        .is_err()
    );
}

#[test]
fn motion_is_available_in_normal_builds_and_defaults_to_saved_preferences() {
    for (name, mode) in [
        ("enhanced", MotionMode::Enhanced),
        ("original", MotionMode::Original),
    ] {
        let options = parse_args(args(&["--original", "assets", "--motion", name]))
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(options.motion, Some(mode));
        #[cfg(feature = "dev-tools")]
        assert!(options.motion_trace.is_none());
    }
    assert!(
        parse_args(args(&["--original", "assets"]))
            .unwrap()
            .unwrap()
            .1
            .motion
            .is_none()
    );
    for suffix in [
        vec!["--motion"],
        vec!["--motion", "fast"],
        vec!["--motion", "faithful"],
    ] {
        let mut argv = vec!["--original", "assets"];
        argv.extend(suffix);
        assert!(parse_args(args(&argv)).is_err());
    }
    #[cfg(feature = "dev-tools")]
    assert!(
        parse_args(args(&[
            "--original",
            "assets",
            "--motion",
            "enhanced",
            "--static"
        ]))
        .is_err()
    );
}

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
fn f3_mode_defaults_to_press_and_is_available_in_all_playback_modes() {
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
