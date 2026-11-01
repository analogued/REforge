// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

mod input_history_cases;

use super::*;
use reforge_sdl::sdl3::keyboard::Mod;

#[test]
fn global_shortcuts_do_not_change_native_pause_or_advance_while_host_paused() {
    use reforge_sdl::sdl3::keyboard::Scancode;
    let event = |scan, key, mods, repeat| Event::KeyDown {
        timestamp: 0,
        window_id: 1,
        which: 0,
        raw: 0,
        scancode: Some(scan),
        keycode: Some(key),
        keymod: mods,
        repeat,
    };
    let mut presentation = Presentation::default();
    assert_eq!(
        event_action(&mut presentation, space(true, false), true),
        Action::Continue
    );
    assert_eq!(presentation.screen(), Screen::Splash);
    let mut session = SessionState::default();
    for (scan, key) in [(Scancode::Pause, Keycode::Pause), (Scancode::P, Keycode::P)] {
        let pause = event(scan, key, Mod::NOMOD, false);
        for host_paused in [false, true] {
            assert_eq!(
                event_action(&mut presentation, pause.clone(), host_paused),
                Action::ToggleHostPause
            );
        }
        // Both shortcuts are host actions, not F1 make/break aliases.
        control_event(&mut session, &pause, true);
        assert_eq!(
            session.keyboard.held_keys(0),
            crate::gameplay::HeldKeys::default()
        );
        assert_eq!(session.pause.state, PauseState::Running);
        for mods in [
            Mod::NOMOD,
            Mod::LSHIFTMOD,
            Mod::RSHIFTMOD,
            Mod::CAPSMOD,
            Mod::NUMMOD,
            Mod::LCTRLMOD,
            Mod::RCTRLMOD,
            Mod::LALTMOD,
            Mod::RALTMOD,
            Mod::LGUIMOD,
            Mod::RGUIMOD,
        ] {
            for repeat in [false, true] {
                assert_eq!(
                    host_shortcut(&event(scan, key, mods, repeat)),
                    shared_host_shortcut(&event(Scancode::Pause, Keycode::Pause, mods, repeat))
                );
            }
        }
    }
    assert_eq!(host_shortcut(&key_event(Keycode::P, false, false)), None);
    // P follows the letter on the active layout, as do Mach 3's other keys.
    assert_eq!(
        host_shortcut(&event(Scancode::A, Keycode::P, Mod::NOMOD, false)),
        Some(HostShortcut::TogglePause)
    );
    assert_eq!(
        host_shortcut(&event(Scancode::P, Keycode::A, Mod::NOMOD, false)),
        None
    );
    assert_eq!(
        event_action(
            &mut presentation,
            event(Scancode::A, Keycode::Q, Mod::RCTRLMOD, false),
            true
        ),
        Action::Close
    );
    assert_eq!(
        event_action(
            &mut presentation,
            event(Scancode::Q, Keycode::Q, Mod::NOMOD, false),
            false
        ),
        Action::Continue
    );
    assert_eq!(Options::default().scale, None);
}

fn space(down: bool, repeat: bool) -> Event {
    if down {
        Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: Some(Keycode::Space),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat,
            which: 0,
            raw: 0,
        }
    } else {
        Event::KeyUp {
            timestamp: 0,
            window_id: 1,
            keycode: Some(Keycode::Space),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat,
            which: 0,
            raw: 0,
        }
    }
}

fn key_event(key: Keycode, down: bool, repeat: bool) -> Event {
    let mut event = space(down, repeat);
    match &mut event {
        Event::KeyDown { keycode, .. } | Event::KeyUp { keycode, .. } => *keycode = Some(key),
        _ => unreachable!(),
    }
    event
}

#[test]
fn host_pause_and_focus_loss_preserve_every_native_pause_phase() {
    use std::time::{Duration, Instant};
    for phase in [
        PauseState::Running,
        PauseState::AwaitInitialRelease,
        PauseState::WaitBeforePress(2),
        PauseState::WaitBeforePress(1),
        PauseState::AwaitPress,
        PauseState::WaitBeforeDecision(2),
        PauseState::WaitBeforeDecision(1),
        PauseState::SampleDecision,
        PauseState::AwaitFinalRelease,
    ] {
        let mut session = SessionState::default();
        let mut input = ControlInput::default();
        input.handle(&mut session, &key_event(Keycode::F1, true, false), true);
        session.pause.state = phase;
        session.controls.sound_poll_counter = 3;
        let mut host = HostPause::default();
        let start = Instant::now();
        let deadline = start + Duration::from_millis(100);
        toggle_host_pause(&mut host, &mut session, &mut input, start);
        assert_eq!(session.pause.state, phase);
        assert_eq!(
            session.keyboard.held_keys(0),
            crate::gameplay::HeldKeys::default()
        );
        assert_eq!(host.now(start + Duration::from_secs(60)), start);
        toggle_host_pause(
            &mut host,
            &mut session,
            &mut input,
            start + Duration::from_secs(60),
        );
        assert_eq!(session.pause.state, phase);
        assert_eq!(
            deadline.duration_since(host.now(start + Duration::from_secs(60))),
            Duration::from_millis(100)
        );
        input.handle(&mut session, &key_event(Keycode::F1, true, true), true);
        assert!(!session.keyboard.held_keys(0).f1);
        input.handle(
            &mut session,
            &Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
            true,
        );
        assert_eq!(session.pause.state, phase);
        assert_eq!(session.controls.sound_poll_counter, 3);
    }
}

#[test]
fn settings_pause_restores_host_state_and_preserves_native_deadlines() {
    use std::time::{Duration, Instant};
    for already_paused in [false, true] {
        for phase in [
            PauseState::Running,
            PauseState::AwaitInitialRelease,
            PauseState::WaitBeforePress(2),
            PauseState::AwaitPress,
            PauseState::WaitBeforeDecision(2),
            PauseState::SampleDecision,
            PauseState::AwaitFinalRelease,
        ] {
            let start = Instant::now();
            let deadline = start + Duration::from_millis(30);
            let mut game = GamePlayback::default();
            game.session.pause.state = phase;
            game.next_frame = Some(deadline);
            game.pause_tick = Some(deadline);
            let mut input = ControlInput::default();
            input.handle(
                &mut game.session,
                &key_event(Keycode::Right, true, false),
                true,
            );
            let mut host = HostPause::default();
            if already_paused {
                host.toggle(start);
            }
            let mut menu = MenuPause::default();
            assert!(menu.update(true, &mut host, start));
            game.reset_host_input(&mut input, &host, start);
            assert!(host.is_paused());
            assert!(!menu.update(true, &mut host, start + Duration::from_secs(5)));
            assert_eq!(host.now(start + Duration::from_secs(5)), start);
            let close = start + Duration::from_secs(10);
            assert!(menu.update(false, &mut host, close));
            game.reset_host_input(&mut input, &host, close);
            assert_eq!(host.is_paused(), already_paused);
            assert_eq!(host.now(close), start);
            assert_eq!(game.next_frame, Some(deadline));
            assert_eq!(game.pause_tick, Some(deadline));
            assert_eq!(game.session.pause.state, phase);
            assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
            input.handle(
                &mut game.session,
                &key_event(Keycode::Right, true, true),
                true,
            );
            assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
            input.handle(
                &mut game.session,
                &key_event(Keycode::Right, true, false),
                true,
            );
            assert_eq!(game.session.keyboard.held_keys(0).actions, 8);
        }
    }
}

#[test]
fn cleared_native_pause_handshake_observes_releases_and_fresh_presses() {
    let mut session = SessionState::default();
    let mut input = ControlInput::default();
    session.pause.state = PauseState::AwaitInitialRelease;
    input.clear(&mut session);
    let result = session.controls.poll(session.keyboard.held_keys(0));
    session.pause.poll(result.mask);
    assert_eq!(session.pause.state, PauseState::WaitBeforePress(2));
    session.pause.tick();
    session.pause.tick();
    assert_eq!(session.pause.state, PauseState::AwaitPress);
    input.handle(&mut session, &key_event(Keycode::F1, true, true), true);
    let result = session.controls.poll(session.keyboard.held_keys(0));
    session.pause.poll(result.mask);
    assert_eq!(session.pause.state, PauseState::AwaitPress);
    input.handle(&mut session, &key_event(Keycode::F1, false, false), true);
    input.handle(&mut session, &key_event(Keycode::F1, true, false), true);
    let result = session.controls.poll(session.keyboard.held_keys(0));
    session.pause.poll(result.mask);
    assert_eq!(session.pause.state, PauseState::WaitBeforeDecision(2));
    // Clearing keys is a release, not a reset or a synthetic F1 press.
    input.clear(&mut session);
    session.pause.tick();
    session.pause.tick();
    assert_eq!(session.pause.poll(0), PauseEvent::None);
    assert_eq!(session.pause.state, PauseState::AwaitFinalRelease);
    assert_eq!(session.pause.poll(0), PauseEvent::Resumed);
}

#[test]
fn game_repeats_require_a_fresh_press_after_input_reset() {
    for key in [
        Keycode::S,
        Keycode::X,
        Keycode::B,
        Keycode::N,
        Keycode::Up,
        Keycode::Down,
        Keycode::Left,
        Keycode::Right,
        Keycode::Space,
        Keycode::Kp5,
        Keycode::F1,
        Keycode::F2,
        Keycode::F3,
    ] {
        let mut session = SessionState::default();
        let mut input = ControlInput::default();
        input.handle(&mut session, &key_event(key, true, false), true);
        let held = session.keyboard.held_keys(0);
        assert_ne!(held, crate::gameplay::HeldKeys::default());
        input.clear(&mut session);
        input.handle(&mut session, &key_event(key, true, true), true);
        assert_eq!(
            session.keyboard.held_keys(0),
            crate::gameplay::HeldKeys::default()
        );
        input.handle(&mut session, &key_event(key, false, false), true);
        input.handle(&mut session, &key_event(key, true, false), true);
        input.handle(&mut session, &key_event(key, true, true), true);
        assert_eq!(session.keyboard.held_keys(0), held);
        input.handle(&mut session, &key_event(key, false, false), true);
        assert_eq!(
            session.keyboard.held_keys(0),
            crate::gameplay::HeldKeys::default()
        );
    }
}

#[test]
fn sdl_events_advance_once_and_close() {
    let mut state = Presentation::default();
    assert!(handle_event(&mut state, space(true, false)));
    assert_eq!(state.screen(), Screen::Intro);
    assert!(handle_event(&mut state, space(true, true)));
    assert_eq!(state.screen(), Screen::Intro);
    assert!(handle_event(
        &mut state,
        Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event: WindowEvent::FocusLost
        }
    ));
    assert!(handle_event(&mut state, space(true, false)));
    assert_eq!(state.screen(), Screen::InitialScene);
    assert!(handle_event(&mut state, space(false, false)));
    assert!(handle_event(&mut state, space(true, false)));
    assert_eq!(state.screen(), Screen::InitialScene);
    assert!(!handle_event(&mut state, Event::Quit { timestamp: 0 }));
    assert!(!handle_event(
        &mut state,
        Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event: WindowEvent::CloseRequested
        }
    ));
}

fn synthetic_assets() -> GraphicsAssets {
    GraphicsAssets {
        primary: vec![0x55; 28_960],
        alternate: vec![0; 16_000],
        sprites: (0..190)
            .map(|_| crate::assets::Sprite {
                width_bytes: 14,
                height: 37,
                descriptor: [0; 7],
                color: vec![0; 14 * 37],
                mask: vec![255; 14 * 37],
            })
            .collect(),
    }
}

fn running_playback(assets: &GraphicsAssets, start: std::time::Instant) -> GamePlayback {
    GamePlayback {
        visible_frame: Some(Gameplay::test_game(assets).unwrap().frame().clone()),
        gameplay: Some(Gameplay::test_game(assets).unwrap()),
        next_frame: Some(start + NOMINAL_PREVIEW_BODY_PERIOD / 4),
        ..GamePlayback::default()
    }
}

fn pass_count(game: &GamePlayback) -> u32 {
    let state = game.gameplay.as_ref().unwrap().snapshot();
    if state.body == 0 {
        0
    } else {
        u32::from(state.body - 1) * 3 + u32::from(state.render_pass)
    }
}

fn receive_at(
    game: &mut GamePlayback,
    input: &mut ControlInput,
    event: &Event,
    at: std::time::Instant,
    received: std::time::Instant,
) -> &'static str {
    input.receive(
        &mut game.session,
        &mut game.history,
        event,
        true,
        EventTime {
            host: at,
            timeline: at,
            now: received,
        },
    )
}

#[test]
fn f3_restores_palette_at_body_steering_and_native_pause_polls() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let green_game = || {
        let mut data = vec![0; 65536];
        data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
        for index in 0..21 {
            data[0x0a3e + 50 * index..0x0a41 + 50 * index].copy_from_slice(&[0xce, 0xce, 0xcf]);
        }
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let start_state = SessionState::default().begin_game();
        let mut gameplay = Gameplay::for_play(&assets, rules, start_state).unwrap();
        for _ in 0..21 {
            gameplay.advance(&assets).unwrap();
        }
        assert_eq!(gameplay.palette_family(), 0);
        GamePlayback {
            gameplay: Some(gameplay),
            color_family: 0,
            next_frame: Some(start + unit),
            ..GamePlayback::default()
        }
    };
    for body_already_polled in [false, true] {
        let mut game = green_game();
        game.body_polled = body_already_polled;
        game.session.keyboard.apply_scan(0x3d);
        game.advance(&assets, &options, start + unit).unwrap();
        assert_eq!(game.color_family, 1);
        assert_eq!(game.gameplay.as_ref().unwrap().palette_family(), 1);
        if !body_already_polled {
            // A held F3 switches at body entry and again before steering.
            assert_eq!(
                game.session.controls.preferences.display_mode,
                DisplayMode::Color
            );
            game.session.keyboard.apply_scan(0xbd);
        }
        game.advance(&assets, &options, start + unit * 3).unwrap();
        assert_eq!(
            game.session.controls.preferences.display_mode,
            DisplayMode::Color
        );
        assert_eq!(game.color_family, 1);
        game.session.keyboard.apply_scan(0xbd);
        game.advance(&assets, &options, start + unit * 3).unwrap();
        assert_eq!(game.color_family, 1);
    }
    let mut game = green_game();
    let passes = pass_count(&game);
    game.session.pause.state = PauseState::AwaitPress;
    game.session.keyboard.apply_scan(0x3d);
    for now in [start, start + unit] {
        game.advance(&assets, &options, now).unwrap();
        assert_eq!(game.session.pause.state, PauseState::AwaitPress);
        assert_eq!(pass_count(&game), passes);
        assert_eq!(game.color_family, 1);
        assert_eq!(game.gameplay.as_ref().unwrap().palette_family(), 1);
    }
    assert_eq!(
        game.session.controls.preferences.display_mode,
        DisplayMode::Color
    );
    // A script write later in the same pass wins over the earlier F3 reset.
    let mut game = green_game();
    let gameplay = game.gameplay.as_mut().unwrap();
    for _ in 0..19 {
        gameplay.advance(&assets).unwrap();
    }
    for _ in 0..2 {
        gameplay
            .advance_subframe(&assets, Steering::default())
            .unwrap();
    }
    game.body_polled = true;
    game.session.controls.preferences.display_mode = DisplayMode::Monochrome;
    game.session.keyboard.apply_scan(0x3d);
    game.advance(&assets, &options, start + unit).unwrap();
    assert_eq!(game.gameplay.as_ref().unwrap().snapshot().body, 41);
    assert_eq!(
        game.session.controls.preferences.display_mode,
        DisplayMode::Color
    );
    assert_eq!(game.color_family, 0);
    // F2's fourth active poll takes priority and performs no BIOS mode set.
    let mut game = green_game();
    game.body_polled = true;
    game.session.controls.sound_poll_counter = 3;
    game.session.keyboard.apply_scan(0x3c);
    game.session.keyboard.apply_scan(0x3d);
    game.advance(&assets, &options, start + unit).unwrap();
    assert!(game.session.controls.preferences.sound_disabled);
    assert_eq!(
        game.session.controls.preferences.display_mode,
        DisplayMode::Color
    );
    assert_eq!(game.color_family, 0);
}

#[test]
fn publication_wait_retains_pixels_and_consumes_no_input_poll() {
    use std::time::{Duration, Instant};
    let mut assets = synthetic_assets();
    // Different terrain strips guarantee a changed prepared framebuffer.
    for (index, strip) in assets.primary[16_000..].chunks_mut(72 * 15).enumerate() {
        strip.fill(index as u8 * 17);
    }
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let mut game = running_playback(&assets, start);
    let initial = game.visible_frame.clone();
    game.advance(&assets, &options, start + unit).unwrap();
    assert!(game.pending_publication);
    assert_eq!(game.visible_frame, initial);
    let prepared = game.gameplay.as_ref().unwrap().pass_frame().clone();
    assert_ne!(Some(&prepared), initial.as_ref());
    let state = format!("{:?}", game.gameplay.as_ref().unwrap().snapshot());
    let session = format!("{:?}", game.session);
    let mut input = ControlInput::default();
    // A tap wholly inside the wait is not steering or a synthetic poll.
    for (offset, down) in [
        (Duration::from_millis(2), true),
        (Duration::from_millis(5), false),
    ] {
        receive_at(
            &mut game,
            &mut input,
            &key_event(Keycode::Right, down, false),
            start + unit + offset,
            start + unit * 2,
        );
    }
    assert!(
        !game
            .advance(
                &assets,
                &options,
                start + unit * 2 - Duration::from_nanos(1)
            )
            .unwrap()
    );
    assert_eq!(game.visible_frame, initial);
    assert!(game.advance(&assets, &options, start + unit * 2).unwrap());
    assert!(!game.pending_publication);
    assert_eq!(game.visible_frame.as_ref(), Some(&prepared));
    assert_eq!(format!("{:?}", game.session), session);
    assert_eq!(
        format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
        state
    );
    assert_eq!(game.next_frame, Some(start + unit * 3));
    game.advance(&assets, &options, start + unit * 3).unwrap();
    assert_eq!(
        game.gameplay
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        0
    );
    assert_eq!(
        game.visible_frame.as_ref(),
        Some(game.gameplay.as_ref().unwrap().pass_frame())
    );
}

#[test]
fn changed_layer_publishes_immediately_then_restores_the_extra_wait() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let mut data = vec![0; 65536];
    data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
    for index in 0..21 {
        data[0x0a3e + 50 * index..0x0a41 + 50 * index].copy_from_slice(&[0xcb, 0x85, 0xcf]);
    }
    let rules = ScriptRules::from_data_segment(&data).unwrap();
    let mut gameplay =
        Gameplay::for_play(&assets, rules, SessionState::default().begin_game()).unwrap();
    for _ in 0..21 {
        gameplay.advance(&assets).unwrap();
    }
    assert!(!gameplay.take_transitions().is_empty());
    let mut game = GamePlayback {
        visible_frame: Some(gameplay.frame().clone()),
        gameplay: Some(gameplay),
        next_frame: Some(start + unit),
        ..GamePlayback::default()
    };
    assert!(game.advance(&assets, &options, start + unit).unwrap());
    assert!(!game.pending_publication);
    assert_eq!(
        game.visible_frame.as_ref(),
        Some(game.gameplay.as_ref().unwrap().pass_frame())
    );
    assert_eq!(game.next_frame, Some(start + unit * 2));
    game.advance(&assets, &options, start + unit * 3).unwrap();
    assert_eq!(game.gameplay.as_ref().unwrap().snapshot().render_pass, 3);
    let old = game.visible_frame.clone();
    game.advance(&assets, &options, start + unit * 4).unwrap();
    assert!(game.pending_publication);
    assert_eq!(game.visible_frame, old);
    assert_eq!(game.next_frame, Some(start + unit * 5));
}

#[test]
fn f3_repaints_previous_pixels_while_first_pass_publication_is_pending() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let mut game = running_playback(&assets, start);
    let old = game.visible_frame.clone();
    // Skip the body's function-key poll to exercise one F3 mode switch at
    // the steering poll before composition and its additional wait.
    game.body_polled = true;
    game.session.keyboard.apply_scan(0x3d);
    assert!(game.advance(&assets, &options, start + unit).unwrap());
    assert_eq!(
        game.session.controls.preferences.display_mode,
        DisplayMode::Monochrome
    );
    assert!(game.pending_publication);
    assert_eq!(game.visible_frame, old);
    assert_eq!(game.color_family, 1);
    let session = format!("{:?}", game.session);
    game.advance(&assets, &options, start + unit * 2).unwrap();
    assert_eq!(format!("{:?}", game.session), session);
}

#[test]
fn pending_publication_survives_focus_and_host_pause_and_is_reset_by_new_game() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    for focus in [false, true] {
        let mut game = running_playback(&assets, start);
        game.advance(&assets, &options, start + unit).unwrap();
        let old = game.visible_frame.clone();
        let prepared = game.gameplay.as_ref().unwrap().pass_frame().clone();
        let mut input = ControlInput::default();
        if focus {
            receive_at(
                &mut game,
                &mut input,
                &Event::Window {
                    timestamp: 0,
                    window_id: 1,
                    win_event: WindowEvent::FocusLost,
                },
                start + unit,
                start + unit,
            );
        } else {
            let mut pause = HostPause::default();
            game.toggle_host_pause(&mut pause, &mut input, start + unit);
            game.toggle_host_pause(
                &mut pause,
                &mut input,
                start + unit + Duration::from_secs(60),
            );
            assert_eq!(
                pause.now(start + unit + Duration::from_secs(60)),
                start + unit
            );
        }
        assert_eq!(game.visible_frame, old);
        assert!(game.pending_publication);
        game.advance(&assets, &options, start + unit * 2).unwrap();
        assert_eq!(game.visible_frame.as_ref(), Some(&prepared));
        game.advance(&assets, &options, start + unit * 5).unwrap();
        assert!(game.pending_publication);
        game.begin_game(
            &assets,
            &crate::gameplay::test_support::diagnostic_rules(),
            &options,
            start + unit * 5,
        )
        .unwrap();
        assert!(!game.pending_publication);
        assert_eq!(
            game.visible_frame.as_ref(),
            Some(game.gameplay.as_ref().unwrap().frame())
        );
        assert_eq!(game.next_frame, Some(start + unit * 6));
    }
}

#[test]
fn ordinary_simulation_matches_at_15_30_60_and_120_presentations_per_second() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let end = start + Duration::from_secs(1);
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let mut reference = running_playback(&assets, start);
    for scan in [0x4d, 0x39, 0x3c] {
        reference.session.keyboard.apply_scan(scan);
    }
    // Reference processes exactly one due original pass per call.
    let units = (Duration::from_secs(1).as_nanos() / unit.as_nanos()) as u32;
    let passes = units - (units + 2) / 4;
    for pass in 1..=units {
        reference
            .advance(&assets, &options, start + unit * pass)
            .unwrap();
    }
    assert_eq!(pass_count(&reference), passes);
    assert!(passes > 25);
    for fps in [15, 30, 60, 120] {
        let mut game = running_playback(&assets, start);
        for scan in [0x4d, 0x39, 0x3c] {
            game.session.keyboard.apply_scan(scan);
        }
        // Simulate blocking presentation by advancing the host clock between
        // calls to the exact same scheduling path used by the SDL frontend.
        for frame in 1..=fps {
            let now = start + Duration::from_nanos(1_000_000_000 * frame / fps);
            game.advance(&assets, &options, now).unwrap();
            assert!(!game.transitions.active());
        }
        assert_eq!(pass_count(&game), passes, "presentation rate {fps}");
        // Snapshot has no PartialEq; its Debug covers every reconstructed field.
        assert_eq!(
            format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
            format!("{:?}", reference.gameplay.as_ref().unwrap().snapshot())
        );
        assert_eq!(
            game.gameplay.as_ref().unwrap().frame(),
            reference.gameplay.as_ref().unwrap().frame()
        );
        assert_eq!(
            game.session.controls.preferences,
            reference.session.controls.preferences
        );
        assert_eq!(
            game.session.controls.sound_poll_counter,
            reference.session.controls.sound_poll_counter
        );
        assert_eq!(game.visible_frame, reference.visible_frame);
        assert_eq!(game.pending_publication, reference.pending_publication);
        assert_eq!(game.next_frame, reference.next_frame);
        assert!(game.next_frame.unwrap() > end);
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn timing_spans_observe_catch_up_and_blocking_waits_without_changing_state() {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    let assets = synthetic_assets();
    let options = Options::default();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let origin = Instant::now();
    let path = std::env::temp_dir().join(format!(
        "mach3-timing-platform-{}-{}.tsv",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut ticks = 0;
    let mut trace =
        TimingTrace::create_with_clock(&path, NOMINAL_PREVIEW_BODY_PERIOD, origin, move || {
            ticks += 1;
            origin + Duration::from_micros(ticks)
        })
        .unwrap();
    trace.begin_game();
    let mut game = running_playback(&assets, origin);
    let mut plain = running_playback(&assets, origin);
    game.timing = Some(trace);
    let late = origin + unit * 100;
    game.advance(&assets, &options, late).unwrap();
    plain.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 48);
    assert_eq!(game.next_frame, plain.next_frame);
    assert_eq!(
        format!("{:?}", game.gameplay.as_ref().unwrap().snapshot()),
        format!("{:?}", plain.gameplay.as_ref().unwrap().snapshot())
    );
    let frame = game.gameplay.as_ref().unwrap().frame().clone();
    let steps = [2, 5]
        .into_iter()
        .map(|wait_count| TransitionFrame {
            frame: frame.clone(),
            clip: crate::frame::Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: 1,
                height: 1,
            },
            wait_count,
            before_pass: false,
            palette_family: 1,
            state: None,
        })
        .collect::<Vec<_>>();
    game.transitions.append(steps.clone(), late, unit);
    plain.transitions.append(steps, late, unit);
    for at in [late + unit * 20, late + unit * 25] {
        game.advance(&assets, &options, at).unwrap();
        plain.advance(&assets, &options, at).unwrap();
        assert_eq!(pass_count(&game), 48);
        assert_eq!(game.transitions.deadline, plain.transitions.deadline);
        assert_eq!(game.next_frame, plain.next_frame);
        assert_eq!(
            game.gameplay.as_ref().unwrap().frame(),
            plain.gameplay.as_ref().unwrap().frame()
        );
    }
    game.begin_game(
        &assets,
        &crate::gameplay::test_support::diagnostic_rules(),
        &options,
        late + unit * 26,
    )
    .unwrap();
    game.timing.as_mut().unwrap().flush().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Vec<_>> = text
        .lines()
        .skip(2)
        .map(|line| line.split('\t').collect())
        .collect();
    let passes: Vec<_> = rows.iter().filter(|row| row[2] == "pass").collect();
    assert_eq!(passes.len(), 48);
    let publications: Vec<_> = rows.iter().filter(|row| row[2] == "pass_publish").collect();
    assert_eq!(publications.len(), 48);
    for (prepare, publish) in passes.iter().zip(&publications) {
        assert_eq!(&prepare[3..5], &publish[3..5]);
        let deferred = prepare[4] == "1";
        assert_eq!(
            publish[5].parse::<u128>().unwrap(),
            prepare[5].parse::<u128>().unwrap() + if deferred { unit.as_nanos() } else { 0 }
        );
        assert_eq!(
            publish[21],
            if deferred {
                "delayed=true"
            } else {
                "delayed=false"
            }
        );
    }
    assert_eq!(passes[0][3], "1");
    assert_eq!(passes[0][4], "1");
    assert_eq!(passes[0][5].parse::<u128>().unwrap(), unit.as_nanos());
    assert_eq!(
        passes[0][6].parse::<u128>().unwrap(),
        (unit * 100).as_nanos()
    );
    for pass in passes {
        assert_eq!(
            pass[8].parse::<i128>().unwrap() - pass[7].parse::<i128>().unwrap(),
            1000
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row[2] == "transition_wait")
            .count(),
        2
    );
    assert_eq!(rows.last().unwrap()[1], "1");
    assert_eq!(rows.last().unwrap()[2], "game_start");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn bounded_catch_up_keeps_debt_without_changing_step_or_static_playback() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let late = start + NOMINAL_PREVIEW_BODY_PERIOD / 4 * 100;
    let mut game = running_playback(&assets, start);
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 48);
    assert!(game.ordinary_due(&options, late));
    assert!(game.next_frame.unwrap() <= late);
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 75);
    assert!(!game.ordinary_due(&options, late));
    assert!(game.next_frame.unwrap() > late);
    for playback in [Playback::Step, Playback::Static] {
        let options = Options {
            playback,
            ..options.clone()
        };
        let mut game = running_playback(&assets, start);
        assert!(!game.advance(&assets, &options, late).unwrap());
        assert_eq!(pass_count(&game), 0);
        if playback == Playback::Step {
            game.pending_step = true;
            assert!(game.advance(&assets, &options, late).unwrap());
            assert_eq!(pass_count(&game), 3);
            assert!(!game.advance(&assets, &options, late).unwrap());
            assert_eq!(pass_count(&game), 3);
        }
    }
}

#[test]
fn catch_up_stops_for_native_pause_and_preserves_transition_waits() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let late = start + unit * 100;
    let mut paused = running_playback(&assets, start);
    paused.session.keyboard.apply_scan(0x3b);
    paused.advance(&assets, &options, late).unwrap();
    assert_eq!(paused.session.pause.state, PauseState::AwaitInitialRelease);
    assert_eq!(pass_count(&paused), 0);
    let mut game = running_playback(&assets, start);
    let frame = game.gameplay.as_ref().unwrap().frame().clone();
    game.transitions.append(
        [2, 5]
            .into_iter()
            .map(|wait_count| TransitionFrame {
                frame: frame.clone(),
                clip: crate::frame::Rect {
                    x_bytes: 0,
                    y: 0,
                    width_bytes: 1,
                    height: 1,
                },
                wait_count,
                before_pass: false,
                palette_family: 1,
                state: None,
            })
            .collect(),
        start,
        unit,
    );
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(game.transitions.frames.len(), 1);
    assert_eq!(game.transitions.deadline, Some(late + unit * 5));
    assert_eq!(pass_count(&game), 0);
    game.advance(&assets, &options, late + unit * 4).unwrap();
    assert_eq!(game.transitions.frames.len(), 1);
    game.advance(&assets, &options, late + unit * 5).unwrap();
    assert!(!game.transitions.active());
    assert_eq!(game.next_frame, Some(late + unit * 6));
    assert_eq!(pass_count(&game), 0);
    game.advance(&assets, &options, late + unit * 6).unwrap();
    assert_eq!(pass_count(&game), 1);
}

#[test]
fn ending_frames_keep_their_visible_waits_instead_of_catching_up() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options::default();
    let start = Instant::now();
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let mut game = running_playback(&assets, start);
    #[cfg(feature = "dev-tools")]
    let path = std::env::temp_dir().join(format!(
        "mach3-ending-timing-{}-{}.tsv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    #[cfg(feature = "dev-tools")]
    {
        let mut trace =
            TimingTrace::create_with_clock(&path, NOMINAL_PREVIEW_BODY_PERIOD, start, move || {
                start
            })
            .unwrap();
        trace.begin_game();
        game.timing = Some(trace);
    }
    let gameplay = game.gameplay.as_mut().unwrap();
    gameplay.request_game_over();
    for _ in 0..3 {
        gameplay
            .advance_subframe(&assets, Steering::default())
            .unwrap();
    }
    let mut expected =
        Ending::new_with_best(&assets, &gameplay.snapshot(), gameplay.frame(), 0).unwrap();
    game.advance(&assets, &options, start + unit * 100).unwrap();
    assert!(game.ending.is_some());
    let next = game.next_frame.unwrap();
    game.advance(&assets, &options, next - std::time::Duration::from_nanos(1))
        .unwrap();
    assert_eq!(
        format!("{:?}", game.ending.as_ref().unwrap().snapshot()),
        format!("{:?}", expected.snapshot())
    );
    expected.advance(&assets).unwrap();
    let late = next + unit * 100;
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(
        format!("{:?}", game.ending.as_ref().unwrap().snapshot()),
        format!("{:?}", expected.snapshot())
    );
    assert_eq!(
        game.next_frame,
        Some(late + NOMINAL_PREVIEW_BODY_PERIOD * expected.wait_units() / 4)
    );
    #[cfg(feature = "dev-tools")]
    {
        game.timing.as_mut().unwrap().flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let kinds: Vec<_> = text
            .lines()
            .skip(2)
            .map(|row| row.split('\t').nth(2).unwrap())
            .collect();
        assert_eq!(kinds, ["ending_start", "ending_step"]);
        assert!(text.contains("previous_wait_units=2"));
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn deferred_pause_exit_finishes_the_committed_body_and_four_input_polls() {
    let assets = GraphicsAssets {
        primary: vec![0; 28_960],
        alternate: vec![0; 16_000],
        sprites: (0..190)
            .map(|_| crate::assets::Sprite {
                width_bytes: 14,
                height: 18,
                descriptor: [0; 7],
                color: vec![0; 14 * 18],
                mask: vec![255; 14 * 18],
            })
            .collect(),
    };
    let mut gameplay = Gameplay::test_controlled_game(&assets).unwrap();
    let mut session = SessionState::default();
    session.keyboard.apply_scan(0x3c); // Held F2 at all four input sites.
    session.controls.poll(session.keyboard.held_keys(0)); // Body-entry poll.
    let mut quit = true; // Pause's second F1 press passed the exit gate.
    advance_game_pass(&mut gameplay, &assets, &mut session, true, &mut quit).unwrap();
    assert!(gameplay.snapshot().player.game_over);
    assert!(!gameplay.complete());
    assert_eq!(gameplay.snapshot().render_pass, 1);
    for _ in 0..2 {
        advance_game_pass(&mut gameplay, &assets, &mut session, true, &mut quit).unwrap();
    }
    assert_eq!(gameplay.snapshot().body, 1);
    assert_eq!(gameplay.snapshot().render_pass, 3);
    assert!(gameplay.complete());
    assert!(session.controls.preferences.sound_disabled);
    assert_eq!(session.controls.sound_poll_counter, 0);
}

#[test]
fn startup_space_does_not_fire_and_gameplay_aliases_release_on_focus_loss() {
    let mut session = SessionState::default();
    control_event(&mut session, &space(true, false), false);
    assert_eq!(session.keyboard.held_keys(0).actions, 0);
    control_event(&mut session, &space(false, false), false);
    control_event(&mut session, &space(true, false), true);
    let poll = session.controls.poll(session.keyboard.held_keys(0));
    assert!(steering_from_mask(poll.mask, true).fire);
    assert!(!steering_from_mask(poll.mask, false).fire);
    control_event(
        &mut session,
        &Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event: WindowEvent::FocusLost,
        },
        true,
    );
    assert_eq!(session.keyboard.held_keys(0).actions, 0);
    assert_eq!(scan_for_key(Keycode::B), Some(0x30));
    assert_eq!(scan_for_key(Keycode::Kp5), Some(0x4c));
    assert_eq!(scan_for_key(Keycode::Return), None);
}

#[test]
fn monochrome_uses_each_raw_bit_while_color_preserves_two_bit_pixels() {
    let mut packed = vec![0; crate::frame::FRAME_BYTES];
    packed[0] = 0x81;
    packed[crate::frame::ROW_BYTES] = 0x40;
    let frame = Frame::from_packed(&packed).unwrap();
    let mono = display_rgb(&frame, DisplayMode::Monochrome, 1);
    assert_eq!(mono.len(), 640 * 200 * 3);
    for x in 0..8 {
        assert_eq!(
            &mono[x * 3..x * 3 + 3],
            &[if x == 0 || x == 7 { 255 } else { 0 }; 3]
        );
    }
    assert_eq!(&mono[(640 + 1) * 3..(640 + 2) * 3], &[255; 3]);
    let color = display_rgb(&frame, DisplayMode::Color, 1);
    let source = frame.rgb();
    for (index, pixel) in source.chunks_exact(3).enumerate() {
        assert_eq!(&color[index * 6..index * 6 + 3], pixel);
        assert_eq!(&color[index * 6 + 3..index * 6 + 6], pixel);
    }
}

#[test]
fn color_palette_families_use_the_original_high_intensity_colors() {
    let mut packed = vec![0; crate::frame::FRAME_BYTES];
    packed[0] = 0x1b; // All four two-bit indices in one byte.
    let frame = Frame::from_packed(&packed).unwrap();
    let green = display_rgb(&frame, DisplayMode::Color, 0);
    let cyan = display_rgb(&frame, DisplayMode::Color, 1);
    for (index, (family0, family1)) in [
        ([0, 0, 0], [0, 0, 0]),
        ([85, 255, 85], [85, 255, 255]),
        ([255, 85, 85], [255, 85, 255]),
        ([255, 255, 85], [255, 255, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        for duplicate in 0..2 {
            let offset = index * 6 + duplicate * 3;
            assert_eq!(green[offset..offset + 3], family0);
            assert_eq!(cyan[offset..offset + 3], family1);
        }
    }
    assert_eq!(
        display_rgb(&frame, DisplayMode::Monochrome, 0),
        display_rgb(&frame, DisplayMode::Monochrome, 1)
    );
}

#[test]
fn transition_queue_displays_each_frame_for_its_wait_and_then_releases_simulation() {
    let now = std::time::Instant::now();
    let unit = std::time::Duration::from_millis(10);
    let frame = Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap();
    let mut playback = TransitionPlayback::default();
    playback.append(
        [2, 5]
            .into_iter()
            .map(|wait_count| TransitionFrame {
                frame: frame.clone(),
                clip: crate::frame::Rect {
                    x_bytes: 0,
                    y: 0,
                    width_bytes: 1,
                    height: 1,
                },
                wait_count,
                before_pass: false,
                palette_family: 1,
                state: None,
            })
            .collect(),
        now,
        unit,
    );
    assert!(playback.active());
    assert!(!playback.advance(now + unit, unit));
    assert!(playback.advance(now + unit * 2, unit));
    assert_eq!(playback.frames.front().unwrap().wait_count, 5);
    assert!(!playback.advance(now + unit * 6, unit));
    assert!(playback.advance(now + unit * 7, unit));
    assert!(!playback.active());
    assert_eq!(playback.deadline, None);
}
#[cfg(feature = "dev-tools")]
#[test]
fn input_tracing_preserves_historical_poll_results() {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    // Only this test initializes SDL in this test binary. No video device
    // or synthetic SDL device-state update is needed; ControlInput consumes
    // these authored events exactly as it consumes the frontend's events.
    let _sdl = reforge_sdl::sdl3::init().unwrap();
    let folder = std::env::temp_dir().join(format!(
        "reforge-input-trace-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&folder).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(folder.clone());
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let unit = NOMINAL_PREVIEW_BODY_PERIOD / 4;
    let read_rows = |game: &mut GamePlayback, path: &std::path::Path| {
        game.trace.as_mut().unwrap().flush().unwrap();
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .skip(2)
            .map(|line| line.split('\t').map(str::to_owned).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let assert_same = |left: &GamePlayback, right: &GamePlayback| {
        assert_eq!(
            format!("{:?}", left.session),
            format!("{:?}", right.session)
        );
        assert_eq!(
            format!("{:?}", left.gameplay.as_ref().unwrap().snapshot()),
            format!("{:?}", right.gameplay.as_ref().unwrap().snapshot())
        );
        assert_eq!(
            left.gameplay.as_ref().unwrap().frame(),
            right.gameplay.as_ref().unwrap().frame()
        );
        assert_eq!(left.next_frame, right.next_frame);
        assert_eq!(left.pause_tick, right.pause_tick);
        assert_eq!(left.body_polled, right.body_polled);
        assert_eq!(left.quit_after_body_entry, right.quit_after_body_entry);
    };
    // All events are handled at P+10ms, after a deliberately overdue poll P.
    // Expected-at-P is a temporal reference, not a tap-retention policy.
    for (label, press_delta, release_delta, expected_at_poll) in [
        ("before", -5, -1, 0),
        ("spanning", -5, 5, 8),
        ("after", 5, 20, 0),
    ] {
        let start = Instant::now();
        let path = folder.join(format!("{label}.tsv"));
        let mut game = running_playback(&assets, start);
        let mut plain = running_playback(&assets, start);
        game.trace = Some(InputTrace::create_at(&path, start).unwrap());
        let mut input = ControlInput::default();
        let mut plain_input = ControlInput::default();
        let clock = reforge_sdl::event_clock_sample();
        for (down, delta) in [(true, press_delta), (false, release_delta)] {
            if delta > 10 {
                continue;
            }
            let mut event = key_event(Keycode::Right, down, false);
            let timestamp = clock.sdl_ns + (unit.as_nanos() as i64 + delta * 1_000_000) as u64;
            match &mut event {
                Event::KeyDown {
                    timestamp: value, ..
                }
                | Event::KeyUp {
                    timestamp: value, ..
                } => *value = timestamp,
                _ => unreachable!(),
            }
            let at = if delta < 0 {
                start + unit - Duration::from_millis((-delta) as u64)
            } else {
                start + unit + Duration::from_millis(delta as u64)
            };
            let received = start + unit + Duration::from_millis(10);
            receive_at(&mut game, &mut input, &event, at, received);
            receive_at(&mut plain, &mut plain_input, &event, at, received);
            game.trace.as_mut().unwrap().event_with_input_time(
                &event,
                "queued",
                &HostPause::default(),
                &game.session,
                game.history.queued_at(),
            );
        }
        let late = start + unit + Duration::from_millis(10);
        game.advance(&assets, &options, late).unwrap();
        plain.advance(&assets, &options, late).unwrap();
        assert_same(&game, &plain);
        let rows = read_rows(&mut game, &path);
        assert!(rows.iter().all(|row| row.len() == 13));
        assert!(rows.iter().filter(|row| row[1] == "event").all(|row| {
            row[12].contains("disposition=queued") && row[12].contains("effective_input_ns=")
        }));
        let polls = rows
            .iter()
            .filter(|row| row[1] == "poll")
            .collect::<Vec<_>>();
        assert_eq!(polls.len(), 2);
        assert_eq!(polls[0][2], "body_entry");
        assert_eq!(polls[1][2], "steering");
        for row in polls {
            assert_eq!(row[8], unit.as_nanos().to_string());
            assert_eq!(
                row[4],
                (unit + Duration::from_millis(10)).as_nanos().to_string()
            );
            assert_eq!(row[9], expected_at_poll.to_string());
            assert_eq!(row[11], expected_at_poll.to_string());
        }
    }

    // A full late catch-up batch, function-key handling, native waits and
    // transition waits must produce the same state with tracing enabled.
    let start = Instant::now();
    let path = folder.join("boundaries.tsv");
    let mut game = running_playback(&assets, start);
    let mut plain = running_playback(&assets, start);
    game.trace = Some(InputTrace::create_at(&path, start).unwrap());
    for scan in [0x4d, 0x39, 0x3c] {
        game.session.keyboard.apply_scan(scan);
        plain.session.keyboard.apply_scan(scan);
    }
    let late = start + unit * 100;
    for _ in 0..2 {
        game.advance(&assets, &options, late).unwrap();
        plain.advance(&assets, &options, late).unwrap();
        assert_same(&game, &plain);
    }
    let mut input = ControlInput::default();
    let mut plain_input = ControlInput::default();
    for key in [Keycode::F2, Keycode::F3, Keycode::F1] {
        let event = key_event(key, true, false);
        input.handle(&mut game.session, &event, true);
        plain_input.handle(&mut plain.session, &event, true);
        game.trace
            .as_mut()
            .unwrap()
            .event(&event, "handled", &HostPause::default(), &game.session);
    }
    let next = game.next_frame.unwrap();
    game.advance(&assets, &options, next).unwrap();
    plain.advance(&assets, &options, next).unwrap();
    assert_same(&game, &plain);
    game.session.clear_held_keys();
    plain.session.clear_held_keys();
    game.session.pause.state = PauseState::AwaitInitialRelease;
    plain.session.pause.state = PauseState::AwaitInitialRelease;
    for now in [next, next + unit, next + unit * 2] {
        game.advance(&assets, &options, now).unwrap();
        plain.advance(&assets, &options, now).unwrap();
        assert_same(&game, &plain);
    }
    game.session.pause.state = PauseState::AwaitFinalRelease;
    plain.session.pause.state = PauseState::AwaitFinalRelease;
    game.advance(&assets, &options, next + unit * 3).unwrap();
    plain.advance(&assets, &options, next + unit * 3).unwrap();
    assert_same(&game, &plain);
    let frame = game.gameplay.as_ref().unwrap().frame().clone();
    let steps = [2, 5]
        .into_iter()
        .map(|wait_count| TransitionFrame {
            frame: frame.clone(),
            clip: crate::frame::Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: 1,
                height: 1,
            },
            wait_count,
            before_pass: false,
            palette_family: 1,
            state: None,
        })
        .collect::<Vec<_>>();
    game.transitions.append(steps.clone(), late, unit);
    plain.transitions.append(steps, late, unit);
    for now in [late + unit * 20, late + unit * 25] {
        game.advance(&assets, &options, now).unwrap();
        plain.advance(&assets, &options, now).unwrap();
        assert_same(&game, &plain);
        assert_eq!(game.transitions.deadline, plain.transitions.deadline);
    }
    let rows = read_rows(&mut game, &path);
    assert!(
        rows.iter()
            .any(|row| row[1] == "poll" && row[12].contains("sound_toggled=true"))
    );
    assert!(
        rows.iter()
            .any(|row| row[1] == "poll" && row[12].contains("display_toggled=true"))
    );
    assert!(rows.iter().any(|row| row[1] == "event" && row[10] == "7"));
    for label in [
        "native_pause_wait",
        "native_pause_tick",
        "native_pause_resumed",
        "transition_frame_wait",
        "transition_finished",
    ] {
        assert!(rows.iter().any(|row| row[2] == label), "missing {label}");
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row[2] == "transition_frame_wait")
            .count(),
        2
    );
    for pair in rows.windows(2) {
        assert!(pair[1][0].parse::<u64>().unwrap() > pair[0][0].parse::<u64>().unwrap());
        assert!(pair[1][3].parse::<i128>().unwrap() >= pair[0][3].parse::<i128>().unwrap());
        assert!(pair[1][5].parse::<u64>().unwrap() >= pair[0][5].parse::<u64>().unwrap());
    }
}
