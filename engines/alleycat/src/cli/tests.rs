// Copyright (c) 2026 analogued. Project license has not yet been selected.

#[cfg(feature = "dev-tools")]
use super::diagnostics::{TIMING_HEADER, timing_row};
#[cfg(feature = "sdl")]
use super::frontend::{CATCH_UP_MAX_PASSES, catch_up_batch};
use super::*;

#[cfg(feature = "sdl")]
#[test]
fn title_sound_toggles_remain_valid_after_committed_drawing_work() {
    use alleycat::assets::{DATA_LENGTH, set_word};
    use alleycat::sound::render::Renderer;
    use reforge_audio::category::CategoryVolumes;

    for machine in [Machine::Cga, Machine::Pcjr] {
        for repeated in [false, true] {
            let mut data = vec![0; DATA_LENGTH];
            set_word(&mut data, 0x2ae5, 0xfa59);
            set_word(&mut data, 0x28a0, 0x0802);
            set_word(&mut data, 0x28a2, 0xffff);
            set_word(&mut data, 0x56da, 846);
            for address in (0x0f7a..0x0faa).step_by(2) {
                set_word(&mut data, address, 0x1800);
            }
            data[0] = 0xff;
            data[0x538c..0x540c].fill(2);
            set_word(&mut data, 0x5326, 1000);
            let mut splash = if repeated {
                Splash::after_game_over(&data, machine, 0).unwrap()
            } else {
                Splash::new(&data, machine).unwrap()
            };
            splash.sound.record(true);
            let mut clock = VirtualClock::new(RetraceProfile::for_machine(machine));
            let mut renderer = Renderer::new(CategoryVolumes::default()).unwrap();

            // Service the renderer before processing the next host key, as in
            // the live loop. A toggle must also work without another guest pass
            // (for example while the settings panel has paused the title).
            for enabled in [true, false, true, false, true] {
                if (splash.data()[0] != 0) != enabled {
                    splash.toggle_sound();
                }
                renderer
                    .render(splash.sound.drain(), clock.elapsed_pit_ticks() as u64)
                    .unwrap();
                let end = clock.elapsed_nanos() + 150_000_000;
                let mut pcm = Vec::new();
                while clock.elapsed_nanos() < end {
                    advance_splash(&mut splash, &mut clock, false).unwrap();
                    pcm.extend(
                        renderer
                            .render(splash.sound.drain(), clock.elapsed_pit_ticks() as u64)
                            .unwrap(),
                    );
                }
                let settled = &pcm[pcm.len() / 2..];
                if enabled {
                    assert!(settled.iter().any(|sample| sample.abs() > 100));
                } else {
                    // Allow the band-limited renderer's residual filter decay.
                    let peak = settled.iter().map(|sample| sample.abs()).max().unwrap();
                    assert!(peak < 100, "muted title peak: {peak}");
                }
            }
        }
    }
}

#[cfg(feature = "sdl")]
#[test]
fn catch_up_yields_for_slow_passes_and_retains_pending_work() {
    use std::cell::Cell;
    use std::time::Duration;

    let host_elapsed = Cell::new(Duration::ZERO);
    let mut guest_nanos = 0;
    let target = 100_000_000;
    let mut batches = 0;
    while guest_nanos < target {
        host_elapsed.set(Duration::ZERO);
        let passes = catch_up_batch(
            || host_elapsed.get(),
            || {
                if guest_nanos >= target {
                    return Ok(false);
                }
                // Expensive host work advances only a short guest interval.
                host_elapsed.set(host_elapsed.get() + Duration::from_millis(2));
                guest_nanos += 50_000;
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(passes, 2);
        batches += 1;
        if batches == 1 {
            assert_eq!(guest_nanos, 100_000);
            assert!(guest_nanos < target);
        }
    }
    assert_eq!(batches, 1000);
    assert_eq!(guest_nanos, target);
    // Even a single over-budget pass must complete before yielding.
    let passes = catch_up_batch(
        || Duration::from_secs(1),
        || {
            guest_nanos += 1;
            Ok(true)
        },
    )
    .unwrap();
    assert_eq!(passes, 1);
    assert_eq!(guest_nanos, target + 1);
}

#[cfg(feature = "sdl")]
#[test]
fn catch_up_pass_limit_stops_work_and_propagates_errors() {
    use std::time::Duration;

    let mut passes = 0;
    assert_eq!(
        catch_up_batch(
            || Duration::ZERO,
            || {
                passes += 1;
                Ok(true)
            }
        )
        .unwrap(),
        CATCH_UP_MAX_PASSES
    );
    assert_eq!(passes, CATCH_UP_MAX_PASSES);
    assert_eq!(catch_up_batch(|| Duration::ZERO, || Ok(false)), Ok(0));
    assert_eq!(
        catch_up_batch(|| Duration::ZERO, || Err("failed guest pass".into())),
        Err("failed guest pass".into())
    );
}

#[cfg(feature = "sdl")]
#[test]
fn catch_up_batches_preserve_guest_passes_and_blocking_fence_wait() {
    use std::time::Duration;

    for machine in [Machine::Cga, Machine::Pcjr] {
        for falling in [false, true] {
            let replay = |bounded: bool| {
                let mut alley = Alley::synthetic();
                if falling {
                    alley.player.band_state = 2;
                    alley.player.y = 108;
                    alley.player.vertical_coordinate = 158;
                    alley.player.vertical_direction = 1;
                }
                let mut runtime = Runtime::new(machine);
                let target = 600_000_000;
                let mut trace = Vec::new();
                let mut fence_waits = 0;
                let mut batches = 0;
                while runtime.clock.elapsed_nanos() < target {
                    let mut advance = || {
                        if runtime.clock.elapsed_nanos() >= target {
                            return Ok(false);
                        }
                        let event = runtime.advance_pass_with_action(
                            &mut alley,
                            Controls { x: 1, y: 0 },
                            false,
                        )?;
                        fence_waits += usize::from(event.fence_wait);
                        trace.push(format!("{event:?};{:?}", alley.player));
                        Ok(true)
                    };
                    if bounded {
                        catch_up_batch(|| Duration::ZERO, advance).unwrap();
                    } else {
                        while advance().unwrap() {}
                    }
                    batches += 1;
                }
                (
                    trace,
                    alley.framebuffer().bytes().to_vec(),
                    runtime.clock.elapsed_nanos(),
                    fence_waits,
                    batches,
                )
            };
            let reference = replay(false);
            let bounded = replay(true);
            assert_eq!(bounded.0, reference.0);
            assert_eq!(bounded.1, reference.1);
            assert_eq!(bounded.2, reference.2);
            assert_eq!(bounded.3, reference.3);
            if falling {
                assert!(bounded.3 > CATCH_UP_MAX_PASSES);
                assert!(bounded.4 > 1);
            }
        }
    }
}

fn parse(args: &[&str]) -> Result<Option<Options>, String> {
    parse_options(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn presentation_defaults_to_pixels_and_accepts_valid_choices() {
    let defaults = parse(&["--original", "example.exe"]).unwrap().unwrap();
    #[cfg(feature = "sdl")]
    assert_eq!(defaults.presentation, reforge_sdl::Presentation::Pixels);
    #[cfg(not(feature = "sdl"))]
    let _ = defaults;
    assert!(HELP.contains("--presentation pixels|hyllian"));
    for value in ["pixels", "hyllian"] {
        let options = parse(&["--original", "example.exe", "--presentation", value])
            .unwrap()
            .unwrap();
        #[cfg(feature = "sdl")]
        assert_eq!(options.presentation, value.parse().unwrap());
        #[cfg(not(feature = "sdl"))]
        let _ = options;
    }
    #[cfg(feature = "sdl")]
    {
        let options = parse(&[
            "--original",
            "example.exe",
            "--presentation",
            "hyllian",
            "--presentation",
            "pixels",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.presentation, reforge_sdl::Presentation::Pixels);
    }
    for value in ["", "crt", "Hyllian", "--help"] {
        assert!(parse(&["--original", "example.exe", "--presentation", value]).is_err());
    }
    assert!(parse(&["--original", "example.exe", "--presentation"]).is_err());
}

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
#[test]
fn headless_ignores_selected_presentation_in_either_order() {
    for input in [
        vec![
            "--original",
            "example.exe",
            "--presentation",
            "hyllian",
            "--headless",
        ],
        vec![
            "--original",
            "example.exe",
            "--headless",
            "--presentation",
            "hyllian",
        ],
    ] {
        let options = parse(&input).unwrap().unwrap();
        assert_eq!(options.presentation, reforge_sdl::Presentation::Pixels);
        assert!(options.dev.headless);
    }
}

#[test]
fn scale_defaults_to_auto_and_last_explicit_selection_wins() {
    assert_eq!(
        parse(&["--original", "example.exe"])
            .unwrap()
            .unwrap()
            .scale,
        None
    );
    assert!(HELP.contains("--scale auto|1..16"));
    for (value, expected) in [
        ("auto", None),
        ("1", Some(1)),
        ("16", Some(16)),
        ("0x4", Some(4)),
    ] {
        let options = parse(&["--original", "example.exe", "--scale", value])
            .unwrap()
            .unwrap();
        assert_eq!(options.scale, expected);
    }
    for (first, last, expected) in [("auto", "3", Some(3)), ("3", "auto", None)] {
        let options = parse(&[
            "--original",
            "example.exe",
            "--scale",
            first,
            "--scale",
            last,
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.scale, expected);
    }
    for value in ["0", "17", "-1", "1.5", "Auto", "", "65536"] {
        assert!(
            parse(&["--original", "example.exe", "--scale", value]).is_err(),
            "{value:?}"
        );
    }
    assert!(parse(&["--original", "example.exe", "--scale"]).is_err());
}

#[test]
fn player_options_remain_available() {
    let options = parse(&[
        "--original",
        "example.exe",
        "--machine",
        "pcjr",
        "--scale",
        "3",
        "--difficulty",
        "7",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(options.original, PathBuf::from("example.exe"));
    assert_eq!(options.machine, Machine::Pcjr);
    assert_eq!((options.scale, options.difficulty), (Some(3), 7));
    assert!(options.difficulty_selected);
    assert!(parse(&["--help"]).unwrap().is_none());
}

#[cfg(feature = "dev-tools")]
#[test]
fn timing_trace_keeps_selection_state_without_an_accepted_player_update() {
    let event = PassOutcome {
        start_nanos: 100,
        end_nanos: 200,
        tick_low: 18,
        pit_counter: 65530,
        retrace: false,
        control_polled: true,
        acceptance: None,
        actor_group_due: true,
        fence_wait: false,
        mouse_wait: false,
        room_wait: false,
        scene: 7,
        scene_changed: true,
    };
    let selection = alleycat::alley::SelectionTraceState {
        last_room: 5,
        previous_room: 5,
        random_state: 64089,
        difficulty: 6,
        pending_hearts: false,
    };
    let mut output = Vec::new();
    timing_row(&mut output, 9, &event, selection).unwrap();
    let row = String::from_utf8(output).unwrap();
    let fields: Vec<_> = row.trim_end().split(',').collect();
    let columns: Vec<_> = TIMING_HEADER.split(',').collect();
    assert_eq!(fields.len(), columns.len());
    let value = |name| fields[columns.iter().position(|column| *column == name).unwrap()];
    for (name, expected) in [
        ("event", "9"),
        ("accepted", "0"),
        ("scene", "7"),
        ("scene_changed", "1"),
        ("last_ordinary_room", "5"),
        ("previous_ordinary_room", "5"),
        ("random_state", "64089"),
        ("difficulty", "6"),
        ("pending_hearts", "0"),
    ] {
        assert_eq!(value(name), expected, "{name}");
    }
}

#[test]
fn explicit_seed_is_available_in_every_build_and_last_value_wins() {
    let defaults = parse(&["--original", "example.exe"]).unwrap().unwrap();
    assert_eq!(
        defaults.seed, None,
        "ordinary play must not assume a fixed seed"
    );
    let options = parse(&[
        "--original",
        "example.exe",
        "--seed",
        "4660",
        "--seed",
        "0xfa59",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(options.seed, Some(0xfa59));
    assert!(HELP.contains("--seed"));
    for value in ["65536", "-1", "nonsense"] {
        assert!(parse(&["--original", "example.exe", "--seed", value]).is_err());
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn deterministic_diagnostics_keep_their_default_without_overriding_explicit_seed() {
    for extra in [
        vec!["--headless"],
        vec!["--scene", "splash"],
        vec!["--scene", "alley"],
        vec!["--step-hz", "36"],
    ] {
        let mut args = vec!["--original", "example.exe"];
        args.extend(extra);
        let options = parse(&args).unwrap().unwrap();
        assert_eq!(seed::resolve(&options).unwrap(), 0xfa59);
        args.extend(["--seed", "0x1234"]);
        let options = parse(&args).unwrap().unwrap();
        assert_eq!(seed::resolve(&options).unwrap(), 0x1234);
    }
}

#[test]
fn invalid_player_options_are_rejected() {
    for args in [
        vec!["--original", "example.exe", "--scale", "0"],
        vec!["--original", "example.exe", "--scale", "17"],
        vec!["--original", "example.exe", "--difficulty", "8"],
        vec!["--original", "example.exe", "--machine", "other"],
        vec!["--original"],
        vec!["--unknown"],
    ] {
        assert!(parse(&args).is_err());
    }
}

#[cfg(not(feature = "dev-tools"))]
#[test]
fn normal_parser_rejects_every_development_flag() {
    for flag in [
        "--scene",
        "--step-hz",
        "--headless",
        "--timed",
        "--steps",
        "--script",
        "--trace",
        "--trace-timing",
        "--dump-cga",
        "--dump-ppm",
        "--disable-damage",
        "--disable-actors",
        "--infinite-lives",
        "--original-window-timing",
    ] {
        assert_eq!(
            parse(&[flag]).err(),
            Some(format!("Unknown option: {flag}"))
        );
        assert!(!HELP.contains(flag));
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn development_parser_retains_diagnostics_and_switches() {
    let options = parse(&[
        "--original",
        "example.exe",
        "--seed",
        "0x1234",
        "--scene",
        "underwater",
        "--headless",
        "--timed",
        "--steps",
        "12",
        "--script",
        "controls.csv",
        "--trace",
        "states.csv",
        "--trace-timing",
        "timing.csv",
        "--dump-cga",
        "screen.bin",
        "--dump-ppm",
        "screen.ppm",
        "--disable-damage",
        "eels,air",
        "--disable-actors",
        "spider,broom",
        "--infinite-lives",
        "--original-window-timing",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(options.seed, Some(0x1234));
    let dev = options.dev;
    assert!(dev.scene == StartScene::Underwater);
    assert!(dev.headless && dev.timed && dev.switches.infinite_lives);
    assert!(dev.switches.original_window_timing);
    assert_eq!(dev.steps, Some(12));
    assert_eq!(dev.script, Some(PathBuf::from("controls.csv")));
    assert_eq!(dev.trace, Some(PathBuf::from("states.csv")));
    assert_eq!(dev.timing_trace, Some(PathBuf::from("timing.csv")));
    assert_eq!(dev.cga, Some(PathBuf::from("screen.bin")));
    assert_eq!(dev.ppm, Some(PathBuf::from("screen.ppm")));
}

#[cfg(feature = "dev-tools")]
#[test]
fn repeated_disable_flags_replace_launcher_defaults() {
    let options = parse(&[
        "--original",
        "example.exe",
        "--disable-damage",
        "all",
        "--disable-damage",
        "none",
        "--disable-actors",
        "all",
        "--disable-actors",
        "none",
    ])
    .unwrap()
    .unwrap();
    assert!(
        !options
            .dev
            .switches
            .actor_disabled(alleycat::dev_tools::Actor::AlleyDog)
    );
    assert!(
        !options
            .dev
            .switches
            .damage_disabled(alleycat::dev_tools::Damage::AlleyDog)
    );
}

#[cfg(feature = "dev-tools")]
#[test]
fn milk_scene_is_available_for_timed_development_play() {
    let options = parse(&[
        "--original",
        "example.exe",
        "--scene",
        "milk",
        "--headless",
        "--timed",
    ])
    .unwrap()
    .unwrap();
    assert!(options.dev.scene == StartScene::Milk);
}

#[cfg(feature = "dev-tools")]
#[test]
fn explicit_scenes_bypass_startup_and_splash_accepts_frame_exports() {
    let default = parse(&["--original", "example.exe"]).unwrap().unwrap();
    assert!(!default.dev.scene_selected);
    for scene in ["alley", "splash"] {
        let options = parse(&[
            "--original",
            "example.exe",
            "--scene",
            scene,
            "--headless",
            "--steps",
            "72",
            "--dump-cga",
            "title.cga",
        ])
        .unwrap()
        .unwrap();
        assert!(options.dev.scene_selected);
        assert_eq!(options.dev.steps, Some(72));
        assert_eq!(options.dev.cga, Some(PathBuf::from("title.cga")));
    }
    for extra in [
        vec!["--script", "controls.csv"],
        vec!["--trace", "player.csv"],
        vec!["--trace-timing", "timing.csv"],
        vec!["--step-hz", "36"],
    ] {
        let mut args = vec![
            "--original",
            "example.exe",
            "--scene",
            "splash",
            "--headless",
        ];
        args.extend(extra);
        assert_eq!(
            parse(&args).err(),
            Some("Splash preview does not use gameplay scripts, traces or --step-hz".into())
        );
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn diagnostic_constraints_remain_enforced() {
    for extra in [
        vec!["--script", "controls.csv"],
        vec!["--timed"],
        vec!["--headless", "--trace-timing", "timing.csv"],
        vec!["--headless", "--timed", "--step-hz", "10"],
        vec!["--step-hz", "NaN"],
        vec!["--step-hz", "0"],
        vec!["--disable-damage", "all,eels"],
        vec!["--disable-actors", "unknown"],
    ] {
        let mut args = vec!["--original", "example.exe"];
        args.extend(extra);
        assert!(parse(&args).is_err());
    }
}
