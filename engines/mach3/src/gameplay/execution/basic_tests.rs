// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::sound::{ByteOrigin, Effect, MemoryByte, Resource, SoundAction};
use reforge_audio::speaker::PcSpeaker;
use std::{cell::RefCell, rc::Rc};

type Log = Rc<RefCell<Vec<(Stamp, ClockEvent)>>>;

#[test]
fn joystick_taps_between_polls_are_missed_and_never_generate_keyboard_irqs() {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let log = Log::default();
    let (state, mut clock) = context(log.clone());
    let mut task = GameplayTask::begin(
        Gameplay::test_game(&assets).unwrap(),
        state,
        0x1234,
        64,
        &clock,
    )
    .unwrap();
    task.schedule_joystick(10, 4, &clock).unwrap();
    task.schedule_joystick(20, 0, &clock).unwrap();
    assert!(task.schedule_joystick(19, 4, &clock).is_err());
    let mut polls = Vec::new();
    while clock.tick() < 100_000 {
        task.advance_until(100_000, &resources, &mut clock, &mut |_, e| {
            if let GameplayEvent::InputPoll { mask, .. } = e {
                polls.push(mask);
            }
        })
        .unwrap();
    }
    assert!(!polls.is_empty());
    assert!(polls.iter().all(|&mask| mask == 0));
    assert!(
        !log.borrow()
            .iter()
            .any(|(_, e)| matches!(e, ClockEvent::Interrupt(irq) if irq.irq == 1))
    );
    assert!(task.schedule_joystick(99_999, 4, &clock).is_err());
    task.schedule_joystick(100_010, 4, &clock).unwrap();
    task.clear_input(&clock).unwrap();
    assert!(task.take_scheduled_joysticks().is_empty());
    assert_eq!(task.state().joystick, 0);
}

pub(super) fn program() -> ProceduralProgram {
    let mut bytes = vec![0; 0x78c6];
    for (instruction, base, count) in [
        (0x7541, 0x7583, 17),
        (0x762a, 0x766c, 47),
        (0x774d, 0x778c, 14),
        (0x7833, 0x7872, 42),
    ] {
        bytes[instruction..instruction + 6].copy_from_slice(&[
            0xd1,
            0xe0,
            0x8d,
            0x1e,
            base as u8,
            (base >> 8) as u8,
        ]);
        for index in 0..count {
            bytes[base + index * 2..base + index * 2 + 2]
                .copy_from_slice(&(1000 + index as u16).to_le_bytes());
        }
    }
    ProceduralProgram::from_expanded(&bytes).unwrap()
}

pub(super) struct Memory;
impl SampleMemory for Memory {
    fn read(&self, _resource: Resource, offset: u16) -> Result<MemoryByte, Error> {
        Ok(MemoryByte {
            value: offset as u8,
            origin: ByteOrigin::Asset,
        })
    }
}

fn context(log: Log) -> (GameplayState, Clock<impl FnMut(Stamp, ClockEvent)>) {
    let cs = 0x1234;
    let mut clock = Clock::new(
        PcSpeaker::new(0x60, [true; 3]),
        [FarPointer::default(), keyboard_handler(cs)],
        true,
        move |stamp, event| log.borrow_mut().push((stamp, event)),
    );
    let mut state = GameplayState::default();
    state
        .procedural
        .request(Effect::Noise { divisor: 1850 }, cs, 0, &mut clock);
    (state, clock)
}

#[derive(Debug, PartialEq)]
struct ResultLog {
    clock: Vec<(Stamp, ClockEvent)>,
    game: Vec<(u64, GameplayEvent)>,
    checkpoints: Vec<(String, crate::frame::Frame)>,
    state: String,
    final_game: String,
}

fn run(chunk: u64, end: u64) -> ResultLog {
    run_observed(chunk, end, &mut |_, _| {})
}

fn run_observed(chunk: u64, end: u64, observe: &mut dyn FnMut(Stamp, &ClockEvent)) -> ResultLog {
    run_diagnostics(chunk, end, observe, true)
}

fn run_diagnostics(
    chunk: u64,
    end: u64,
    observe: &mut dyn FnMut(Stamp, &ClockEvent),
    capture: bool,
) -> ResultLog {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let log = Log::default();
    let captured = log.clone();
    let mut clock = Clock::new(
        PcSpeaker::new(0x60, [true; 3]),
        [FarPointer::default(), keyboard_handler(0x1234)],
        true,
        |stamp, event| {
            observe(stamp, &event);
            captured.borrow_mut().push((stamp, event));
        },
    );
    let mut state = GameplayState::default();
    state
        .procedural
        .request(Effect::Noise { divisor: 1850 }, 0x1234, 0, &mut clock);
    let mut gameplay = Gameplay::test_game(&assets).unwrap();
    gameplay.capture_pass_snapshots(capture);
    let mut task = GameplayTask::begin(gameplay, state, 0x1234, 64, &clock).unwrap();
    for (tick, raw) in [
        (20_000, 0x4b),
        (33_300, 0xcb),
        (33_300, 0x4d),
        (65_000, 0xcd),
        (72_000, 0x39),
        (800_000, 0xb9),
        (910_000, 0x3c),
        (1_100_000, 0xbc),
        (1_200_000, 0x3d),
        (1_210_000, 0xbd),
    ] {
        if tick <= end {
            task.schedule_scan(tick, raw, &clock).unwrap();
        }
    }
    let mut game = Vec::new();
    let mut checkpoints = Vec::new();
    while clock.tick() < end {
        let budget = (clock.tick() + chunk).min(end);
        loop {
            match task
                .advance_until(budget, &resources, &mut clock, &mut |tick, event| {
                    game.push((tick, event))
                })
                .unwrap()
            {
                GameplayProgress::Pending => break,
                GameplayProgress::Published { .. } => checkpoints.push((
                    format!("{:?}", task.game().pass_snapshot()),
                    task.game().pass_frame().clone(),
                )),
                GameplayProgress::Returned => panic!("bounded prefix returned early"),
                GameplayProgress::TransitionPublished { .. } => panic!("untimed task"),
            }
        }
    }
    let state = format!("{:?}", task.state());
    let final_game = format!("{:?}", task.game().snapshot());
    drop(clock);
    drop(captured);
    let clock = Rc::try_unwrap(log).unwrap().into_inner();
    ResultLog {
        clock,
        game,
        checkpoints,
        state,
        final_game,
    }
}

#[test]
fn chunk_sizes_and_host_stalls_preserve_state_pixels_and_ordered_device_transactions() {
    let coarse = run(2_000_000, 2_000_000);
    assert_eq!(coarse, run(731, 2_000_000));
    assert_eq!(coarse, run(10_000, 2_000_000));
    // Tight partitioning also straddles input/IRQ deadlines by one PIT clock.
    assert_eq!(run(100_000, 100_000), run(1, 100_000));
    assert!(!coarse.checkpoints.is_empty());
    assert!(
        coarse
            .game
            .iter()
            .any(|(_, event)| matches!(event, GameplayEvent::InputPoll { mask: 8, .. }))
    );
    assert!(coarse.clock.iter().any(|(_, event)| matches!(
        event,
        ClockEvent::Action(SoundAction::Write {
            source: None,
            port: 0x42,
            ..
        })
    )));
}

#[cfg(feature = "dev-tools")]
#[test]
fn audio_rendering_rates_and_chunks_leave_gameplay_and_device_execution_identical() {
    use reforge_audio::{
        PIT_HZ,
        render::{BlipRenderer, Config, SignalRenderer},
    };
    let end = 2_000_000;
    let baseline = run(2_000_000, end);
    let render = |rate, chunk| {
        let mut renderer = BlipRenderer::new(
            Config {
                clock_hz: PIT_HZ as u32,
                sample_rate: rate,
                chunk_ticks: chunk,
            },
            0,
            0,
        )
        .unwrap();
        let mut pcm = Vec::new();
        let game = run_observed(731, end, &mut |_, event| {
            if let ClockEvent::Device(events) = event {
                if let Some(edge) = events.drive {
                    renderer
                        .level_at(
                            edge.tick,
                            if edge.high { 8192 } else { 0 },
                            &mut |samples| {
                                pcm.extend_from_slice(samples);
                                Ok(())
                            },
                        )
                        .unwrap();
                }
            }
        });
        renderer
            .finish_until(end, &mut |samples| {
                pcm.extend_from_slice(samples);
                Ok(())
            })
            .unwrap();
        assert_eq!(game, baseline);
        assert!(pcm.iter().any(|sample| *sample != 0));
        pcm
    };
    assert_eq!(render(48000, 11931), render(48000, 137));
    render(44100, 10000);
    render(96000, 4096);
}

#[test]
fn publication_wait_retains_the_old_image_and_defers_noise_until_after_publication() {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (state, mut clock) = context(Log::default());
    let mut task = GameplayTask::begin(
        Gameplay::test_game(&assets).unwrap(),
        state,
        0x1234,
        64,
        &clock,
    )
    .unwrap();
    let mut events = Vec::new();
    assert_eq!(
        task.advance_until(33_301, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Pending
    );
    assert!(task.game().pass_snapshot().is_none());
    assert_eq!(task.game().snapshot().player.phase, 0);
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        GameplayEvent::Action(GameplayAction::Wait { site: 0x0550, .. })
    )));
    assert!(!events.iter().any(|(_, event)| matches!(
        event,
        GameplayEvent::Action(GameplayAction::Request { site: 0x067b, .. })
    )));
    assert_eq!(
        task.advance_until(66_601, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Published { body: 1, pass: 1 }
    );
    assert_eq!(clock.tick(), 66_601);
    assert!(task.game().pass_snapshot().is_some());
    assert_eq!(
        task.advance_until(clock.tick(), &resources, &mut clock, &mut |tick, event| {
            events.push((tick, event))
        })
        .unwrap(),
        GameplayProgress::Pending
    );
    let published = events
        .iter()
        .position(|(_, event)| matches!(event, GameplayEvent::Published { .. }))
        .unwrap();
    let noise = events
        .iter()
        .position(|(_, event)| {
            matches!(
                event,
                GameplayEvent::Action(GameplayAction::Request { site: 0x067b, .. })
            )
        })
        .unwrap();
    assert!(published < noise);
}

#[test]
fn death_waits_suspend_gameplay_and_only_then_commit_the_life_reset() {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (state, mut clock) = context(Log::default());
    let mut gameplay = Gameplay::test_game(&assets).unwrap();
    gameplay.player.hit(2);
    let mut task = GameplayTask::begin(gameplay, state, 0x1234, 64, &clock).unwrap();
    let mut events = Vec::new();
    let mut death_started = false;
    for _ in 0..50_000 {
        let budget = clock.tick() + 100;
        task.advance_until(budget, &resources, &mut clock, &mut |tick, event| {
            events.push((tick, event))
        })
        .unwrap();
        if events.iter().any(|(_, event)| {
            matches!(
                event,
                GameplayEvent::Action(GameplayAction::Wait { site: 0x2602, .. })
            )
        }) {
            death_started = true;
            break;
        }
    }
    assert!(death_started);
    let frozen = format!("{:?}", task.game().snapshot());
    let old_phase = task.state().procedural.counters.phase;
    let budget = clock.tick() + 10_000;
    task.advance_until(budget, &resources, &mut clock, &mut |tick, event| {
        events.push((tick, event))
    })
    .unwrap();
    assert_eq!(frozen, format!("{:?}", task.game().snapshot()));
    assert_ne!(old_phase, task.state().procedural.counters.phase);
    assert_eq!(task.game().snapshot().player.lives, 6);
    let end = clock.tick() + 2_000_000;
    while task.game().snapshot().player.lives == 6 {
        task.advance_until(end, &resources, &mut clock, &mut |tick, event| {
            events.push((tick, event))
        })
        .unwrap();
        assert!(clock.tick() < end);
    }
    assert_eq!(task.game().snapshot().player.lives, 5);
    assert_eq!(task.game().snapshot().player.phase, 0);
    let waits: Vec<_> = events
        .iter()
        .filter_map(|(_, event)| match event {
            GameplayEvent::Action(GameplayAction::Wait {
                site: 0x25f9 | 0x2602,
                units,
            }) => Some(*units),
            _ => None,
        })
        .collect();
    assert_eq!(waits, [1, 20]);
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        GameplayEvent::Action(GameplayAction::Request {
            site: 0x2607,
            effect: Effect::Reset
        })
    )));
}

#[test]
fn native_pause_runs_irqs_and_four_separate_waits_before_resuming() {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (state, mut clock) = context(Log::default());
    let mut task = GameplayTask::begin(
        Gameplay::test_game(&assets).unwrap(),
        state,
        0x1234,
        64,
        &clock,
    )
    .unwrap();
    for (tick, raw) in [
        (10_000, 0x3b),
        (40_000, 0xbb),
        (160_000, 0x4d),
        (280_000, 0xcd),
    ] {
        task.schedule_scan(tick, raw, &clock).unwrap();
    }
    let mut events = Vec::new();
    assert_eq!(
        task.advance_until(150_000, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Pending
    );
    assert_eq!(task.game().snapshot().body, 0);
    assert!(task.game().pass_snapshot().is_none());
    assert_eq!(task.state().pause.state, PauseState::AwaitPress);
    assert_ne!(task.state().procedural.counters.phase, 0);
    while task.game().pass_snapshot().is_none() {
        task.advance_until(500_000, &resources, &mut clock, &mut |tick, event| {
            events.push((tick, event))
        })
        .unwrap();
        assert!(clock.tick() < 500_000);
    }
    let waits: Vec<_> = events
        .iter()
        .filter_map(|(_, event)| match event {
            GameplayEvent::Action(GameplayAction::Wait {
                site: site @ (0x26a7 | 0x26b0 | 0x26c0 | 0x26c9),
                ..
            }) => Some(*site),
            _ => None,
        })
        .collect();
    assert_eq!(waits, [0x26a7, 0x26b0, 0x26c0, 0x26c9]);
    assert_eq!(task.state().pause.state, PauseState::Running);
    assert!(!task.game().snapshot().player.game_over);
}

#[test]
fn late_input_is_prospective_and_invalid_context_does_not_start_execution() {
    let assets = crate::gameplay::tests::synthetic_assets();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (state, mut clock) = context(Log::default());
    assert!(
        GameplayTask::begin(
            Gameplay::test_game(&assets).unwrap(),
            state.clone(),
            0x9999,
            64,
            &clock
        )
        .is_err()
    );
    assert!(
        GameplayTask::begin(
            Gameplay::test_game(&assets).unwrap(),
            state.clone(),
            0x1234,
            0,
            &clock
        )
        .is_err()
    );
    let mut task = GameplayTask::begin(
        Gameplay::test_game(&assets).unwrap(),
        state,
        0x1234,
        64,
        &clock,
    )
    .unwrap();
    task.advance_until(33_301, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    let before = format!("{:?}", task.game().snapshot());
    assert!(task.schedule_scan(33_300, 0x4d, &clock).is_err());
    task.schedule_scan(clock.tick(), 0x4d, &clock).unwrap();
    task.advance_until(clock.tick(), &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert_eq!(before, format!("{:?}", task.game().snapshot()));
    assert_eq!(task.state().keyboard.held_keys(0).actions, 8);
    // A caller cannot advance the device independently and then resume a stale
    // continuation, even if it did not cross a deliverable IRQ.
    clock.advance_to(clock.tick() + 1).unwrap();
    assert!(
        task.advance_until(clock.tick(), &resources, &mut clock, &mut |_, _| {})
            .is_err()
    );
    assert!(task.schedule_scan(clock.tick(), 0xcd, &clock).is_err());
}

#[test]
fn optional_pass_snapshots_preserve_execution_events_and_every_frame() {
    let captured = run_diagnostics(10_000, 2_000_000, &mut |_, _| {}, true);
    let plain = run_diagnostics(10_000, 2_000_000, &mut |_, _| {}, false);
    assert!(!captured.checkpoints.is_empty());
    assert!(
        captured
            .checkpoints
            .iter()
            .all(|(state, _)| state.starts_with("Some("))
    );
    assert!(plain.checkpoints.iter().all(|(state, _)| state == "None"));
    assert_eq!(captured.clock, plain.clock);
    assert_eq!(captured.game, plain.game);
    assert_eq!(captured.state, plain.state);
    assert_eq!(captured.final_game, plain.final_game);
    assert_eq!(
        captured
            .checkpoints
            .into_iter()
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>(),
        plain
            .checkpoints
            .into_iter()
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>(),
    );
}
