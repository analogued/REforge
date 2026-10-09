// Copyright (c) 2026 analogued. Project license has not been selected.

use super::*;

#[test]
fn overdue_polls_use_received_history_and_match_punctual_execution() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let unit = options.body_period / 4;
    for lateness in [
        Duration::from_micros(100),
        Duration::from_millis(1),
        Duration::from_millis(10),
    ] {
        for pass in [1, 3] {
            for (press, release, expected) in [(-2, -1, 0), (-2, 2, 8), (2, 4, 0)] {
                let start = Instant::now();
                let poll = start + unit * pass;
                let late = poll + lateness;
                let at = |delta: i32| {
                    if delta < 0 {
                        poll - lateness * delta.unsigned_abs() / 4
                    } else {
                        poll + lateness * delta as u32 / 4
                    }
                };
                let events = [
                    (at(press), key_event(Keycode::Right, true, false)),
                    (at(release), key_event(Keycode::Right, false, false)),
                ];
                let mut punctual = running_playback(&assets, start, &options);
                let mut delayed = running_playback(&assets, start, &options);
                let mut punctual_input = ControlInput::default();
                let mut delayed_input = ControlInput::default();
                for (time, event) in &events {
                    if *time <= poll {
                        receive_at(&mut punctual, &mut punctual_input, event, *time, *time);
                    }
                    if *time <= late {
                        receive_at(&mut delayed, &mut delayed_input, event, *time, late);
                    }
                }
                punctual.advance(&assets, &options, poll).unwrap();
                delayed.advance(&assets, &options, late).unwrap();
                let reference = punctual.opening.as_ref().unwrap();
                let actual = delayed.opening.as_ref().unwrap();
                assert_eq!(actual.snapshot().player.effective_input, expected);
                assert_eq!(
                    format!("{:?}", actual.snapshot()),
                    format!("{:?}", reference.snapshot())
                );
                assert_eq!(actual.frame(), reference.frame());
                assert_eq!(
                    format!("{:?}", delayed.session),
                    format!("{:?}", punctual.session)
                );
                // No tap is latched until use: both paths see the final
                // released state at the following poll.
                for (time, event) in &events {
                    if *time > poll {
                        receive_at(
                            &mut punctual,
                            &mut punctual_input,
                            event,
                            *time,
                            poll + unit,
                        );
                    }
                    if *time > late {
                        receive_at(&mut delayed, &mut delayed_input, event, *time, poll + unit);
                    }
                }
                let next_poll = poll + unit * if pass == 1 { 2 } else { 1 };
                punctual.advance(&assets, &options, next_poll).unwrap();
                delayed.advance(&assets, &options, next_poll).unwrap();
                assert_eq!(
                    delayed
                        .opening
                        .as_ref()
                        .unwrap()
                        .snapshot()
                        .player
                        .effective_input,
                    0
                );
                assert_eq!(
                    delayed.opening.as_ref().unwrap().frame(),
                    punctual.opening.as_ref().unwrap().frame()
                );
            }
        }
    }
}

#[test]
fn late_delivery_is_prospective_and_catch_up_repolls_keep_history_order() {
    use std::time::Instant;
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    let mut game = running_playback(&assets, start, &options);
    let mut input = ControlInput::default();
    game.advance(&assets, &options, start + unit).unwrap();
    let first = game.opening.as_ref().unwrap().frame().clone();
    // The event is dated before an already-executed poll. It affects the
    // next unexecuted poll without rolling back or inserting a delay.
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, true, false),
        start,
        start + unit,
    );
    assert_eq!(game.opening.as_ref().unwrap().frame(), &first);
    game.advance(&assets, &options, start + unit * 3).unwrap();
    assert_eq!(
        game.opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        8
    );

    let mut game = running_playback(&assets, start, &options);
    let mut input = ControlInput::default();
    let late = start + unit * 100;
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, true, false),
        start + unit * 63,
        late,
    );
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, false, false),
        start + unit * 65,
        late,
    );
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 48);
    assert_eq!(
        game.opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        8
    );
    assert!(game.ordinary_due(&options, late));
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 75);
    assert_eq!(
        game.opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        0
    );
    assert_eq!(game.next_frame, Some(start + unit * 101));
}

#[test]
fn historical_f1_tap_enters_native_pause_without_replaying_its_release() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    let poll = start + unit;
    let late = poll + Duration::from_millis(10);
    let mut game = running_playback(&assets, start, &options);
    let mut input = ControlInput::default();
    for (at, down) in [
        (poll - Duration::from_millis(1), true),
        (poll + Duration::from_millis(1), false),
    ] {
        receive_at(
            &mut game,
            &mut input,
            &key_event(Keycode::F1, down, false),
            at,
            late,
        );
    }
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 0);
    assert_eq!(game.session.pause.state, PauseState::AwaitInitialRelease);
    assert!(!game.session.keyboard.held_keys(0).f1);
    game.advance(&assets, &options, late).unwrap();
    assert_eq!(pass_count(&game), 0);
    assert_eq!(game.session.pause.state, PauseState::WaitBeforePress(2));
}

#[test]
fn historical_function_keys_keep_four_poll_priority_and_counter_rules() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    let mut delayed = running_playback(&assets, start, &options);
    let mut punctual = running_playback(&assets, start, &options);
    let mut delayed_input = ControlInput::default();
    let mut punctual_input = ControlInput::default();
    for key in [Keycode::Right, Keycode::F1, Keycode::F2, Keycode::F3] {
        let event = key_event(key, true, false);
        receive_at(
            &mut delayed,
            &mut delayed_input,
            &event,
            start,
            start + unit * 4,
        );
        receive_at(&mut punctual, &mut punctual_input, &event, start, start);
    }
    for pass in 1..=4 {
        punctual
            .advance(&assets, &options, start + unit * pass)
            .unwrap();
    }
    delayed
        .advance(
            &assets,
            &options,
            start + unit * 4 + Duration::from_millis(10),
        )
        .unwrap();
    assert_eq!(
        format!("{:?}", delayed.session),
        format!("{:?}", punctual.session)
    );
    assert_eq!(
        delayed.opening.as_ref().unwrap().frame(),
        punctual.opening.as_ref().unwrap().frame()
    );
    assert_eq!(delayed.session.controls.sound_poll_counter, 0);
    assert!(delayed.session.controls.preferences.sound_disabled);
    // Three F3 toggles; the fourth F2 sample returns before F3 or F1.
    assert_eq!(
        delayed.session.controls.preferences.display_mode,
        DisplayMode::Monochrome
    );
    assert_eq!(delayed.session.pause.state, PauseState::Running);
    assert_eq!(
        delayed
            .opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        0
    );
}

#[test]
fn frontend_resets_discard_history_and_reject_buffered_or_repeat_keys() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    for focus in [false, true] {
        let mut game = running_playback(&assets, start, &options);
        let mut input = ControlInput::default();
        game.session.pause.state = PauseState::AwaitPress;
        game.session.controls.sound_poll_counter = 3;
        receive_at(
            &mut game,
            &mut input,
            &key_event(Keycode::F1, true, false),
            start,
            start + unit,
        );
        let reset = if focus {
            let time = start + unit;
            receive_at(
                &mut game,
                &mut input,
                &Event::Window {
                    timestamp: 0,
                    window_id: 1,
                    win_event: WindowEvent::FocusLost,
                },
                time,
                time,
            );
            time
        } else {
            let mut pause = HostPause::default();
            game.toggle_host_pause(&mut pause, &mut input, start + unit);
            let resumed = start + Duration::from_secs(60);
            game.toggle_host_pause(&mut pause, &mut input, resumed);
            assert_eq!(game.next_frame, Some(start + unit));
            assert_eq!(pause.now(resumed), start + unit);
            resumed
        };
        assert_eq!(game.session.pause.state, PauseState::AwaitPress);
        assert_eq!(game.session.controls.sound_poll_counter, 3);
        assert_eq!(
            receive_at(
                &mut game,
                &mut input,
                &key_event(Keycode::F1, true, false),
                reset - Duration::from_micros(1),
                reset
            ),
            "ignored_before_reset"
        );
        assert_eq!(
            receive_at(
                &mut game,
                &mut input,
                &key_event(Keycode::F1, true, true),
                reset,
                reset
            ),
            "ignored_repeat"
        );
        let timeline = start + unit;
        game.history.apply_until(&mut game.session, timeline);
        assert!(!game.session.keyboard.held_keys(0).f1);
        assert_eq!(
            input.receive(
                &mut game.session,
                &mut game.history,
                &key_event(Keycode::F1, true, false),
                true,
                EventTime {
                    host: reset,
                    timeline,
                    now: timeline
                },
            ),
            "queued"
        );
        game.history.apply_until(&mut game.session, timeline);
        assert!(game.session.keyboard.held_keys(0).f1);
    }
}

#[test]
fn deliberate_waits_fold_taps_without_replaying_discarded_deadlines() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    let late = start + unit * 20;
    for boundary in ["transition", "native_resume", "ending", "step", "static"] {
        let mut options = options.clone();
        let mut game = running_playback(&assets, start, &options);
        let mut input = ControlInput::default();
        match boundary {
            "transition" => game.transitions.append(
                vec![TransitionFrame {
                    frame: game.opening.as_ref().unwrap().frame().clone(),
                    clip: crate::frame::Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: 1,
                        height: 1,
                    },
                    wait_count: 1,
                    before_pass: false,
                    palette_family: 1,
                    state: None,
                }],
                start,
                unit,
            ),
            "native_resume" => game.session.pause.state = PauseState::AwaitFinalRelease,
            "ending" => {
                let opening = game.opening.as_ref().unwrap();
                game.ending = Some(
                    Ending::new_with_best(&assets, &opening.snapshot(), opening.frame(), 0)
                        .unwrap(),
                );
            }
            "step" => {
                options.playback = Playback::Step;
                game.pending_step = true;
            }
            "static" => options.playback = Playback::Static,
            _ => unreachable!(),
        }
        receive_at(
            &mut game,
            &mut input,
            &key_event(Keycode::Right, true, false),
            late - Duration::from_millis(2),
            late,
        );
        receive_at(
            &mut game,
            &mut input,
            &key_event(Keycode::Right, false, false),
            late - Duration::from_millis(1),
            late,
        );
        game.advance(&assets, &options, late).unwrap();
        assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
        assert_eq!(
            game.opening
                .as_ref()
                .unwrap()
                .snapshot()
                .player
                .effective_input,
            0
        );
        if boundary == "transition" {
            assert_eq!(pass_count(&game), 0);
            assert_eq!(game.next_frame, Some(late + unit));
            game.advance(&assets, &options, late + unit).unwrap();
            assert_eq!(
                game.opening
                    .as_ref()
                    .unwrap()
                    .snapshot()
                    .player
                    .effective_input,
                0
            );
        }
        if boundary == "native_resume" {
            assert_eq!(game.session.pause.state, PauseState::Running);
            assert_eq!(pass_count(&game), 1);
            assert_eq!(game.next_frame, Some(late + unit));
        }
    }
}

#[test]
fn transition_holds_and_new_session_preserve_current_scans_not_old_taps() {
    use std::time::{Duration, Instant};
    let assets = synthetic_assets();
    let options = Options {
        controls: true,
        ..Options::default()
    };
    let start = Instant::now();
    let unit = options.body_period / 4;
    let now = start + unit * 10;
    let mut game = running_playback(&assets, start, &options);
    let mut input = ControlInput::default();
    game.transitions.append(
        vec![TransitionFrame {
            frame: game.opening.as_ref().unwrap().frame().clone(),
            clip: crate::frame::Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: 1,
                height: 1,
            },
            wait_count: 1,
            before_pass: false,
            palette_family: 1,
            state: None,
        }],
        start,
        unit,
    );
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, true, false),
        now - Duration::from_millis(1),
        now,
    );
    game.advance(&assets, &options, now).unwrap();
    game.advance(&assets, &options, now + unit).unwrap();
    assert_eq!(
        game.opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        8
    );
    // A startup/return Space press is not fire. A completed Left tap before
    // the next game must not be latched, while the held Right stays held.
    input.receive(
        &mut game.session,
        &mut game.history,
        &key_event(Keycode::Space, true, false),
        false,
        EventTime {
            host: now + unit,
            timeline: now + unit,
            now: now + unit,
        },
    );
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Left, true, false),
        now + unit,
        now + unit * 2,
    );
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Left, false, false),
        now + unit * 2,
        now + unit * 2,
    );
    game.begin_game(&assets, None, &options, now + unit * 2)
        .unwrap();
    assert_eq!(game.session.keyboard.held_keys(0).actions, 8);
    game.advance(&assets, &options, now + unit * 3).unwrap();
    assert_eq!(
        game.opening
            .as_ref()
            .unwrap()
            .snapshot()
            .player
            .effective_input,
        8
    );
    assert_eq!(game.session.keyboard.held_keys(0).actions & 16, 0);
    // Engine clears retain the last scan filter and do not reset frontend
    // repeat eligibility. Repeated identical bytes stay suppressed.
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, true, true),
        now + unit * 3,
        now + unit * 3,
    );
    game.history.apply_until(&mut game.session, now + unit * 3);
    game.session.finish_audio_input(0);
    receive_at(
        &mut game,
        &mut input,
        &key_event(Keycode::Right, true, true),
        now + unit * 4,
        now + unit * 4,
    );
    game.history.apply_until(&mut game.session, now + unit * 4);
    assert_eq!(game.session.keyboard.held_keys(0).actions, 0);
}
