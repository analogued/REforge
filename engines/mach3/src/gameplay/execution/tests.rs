// Copyright (c) 2026 analogued. Project license has not been selected.
//! Authored fixtures for suspended completion samples and their return paths.

use super::*;
use crate::{
    gameplay::{enemy::Enemy, tests::synthetic_assets},
    sound::{ByteOrigin, Effect, MemoryByte, Resource},
};
use reforge_audio::speaker::PcSpeaker;
use std::{cell::Cell, rc::Rc};

const CS: u16 = 0x3000;
type TestClock = Clock<fn(Stamp, ClockEvent)>;

struct Memory {
    reads: Cell<usize>,
    missing: bool,
}

impl SampleMemory for Memory {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, Error> {
        self.reads.set(self.reads.get() + 1);
        assert_eq!(resource, Resource::Data8);
        assert!(offset < 9723);
        if self.missing {
            return Err(invalid("synthetic missing sample byte"));
        }
        Ok(MemoryByte {
            value: 128,
            origin: ByteOrigin::Asset,
        })
    }
}

fn program() -> ProceduralProgram {
    let mut image = vec![0; 0x78c6];
    for (anchor, base, words) in [
        (0x7541, 0x7583, 17),
        (0x762a, 0x766c, 47),
        (0x774d, 0x778c, 14),
        (0x7833, 0x7872, 42),
    ] {
        image[anchor..anchor + 6].copy_from_slice(&[
            0xd1,
            0xe0,
            0x8d,
            0x1e,
            base as u8,
            (base >> 8) as u8,
        ]);
        for i in 0..words {
            image[base + 2 * i..base + 2 * i + 2].copy_from_slice(&32u16.to_le_bytes());
        }
    }
    ProceduralProgram::from_expanded(&image).unwrap()
}

fn setup(assets: &GraphicsAssets, state: Option<GameplayState>) -> (GameplayTask, TestClock) {
    let mut game = Gameplay::test_game(assets).unwrap();
    let mut records = game.enemy.snapshots();
    records[1].active = true;
    records[1].movement = 27;
    records[1].variant = 3;
    records[1].depth = 50;
    records[1].animation_phase = 0;
    game.enemy = Enemy::restore_snapshots(records, assets).unwrap();
    let mut state = state.unwrap_or_default();
    let mut clock: TestClock = Clock::new(
        PcSpeaker::new(0, [true; 3]),
        [FarPointer::default(), keyboard_handler(CS)],
        true,
        |_, _| {},
    );
    state.procedural.request(Effect::Reset, CS, 0, &mut clock);
    state.procedural.counters.units = 1;
    (
        GameplayTask::begin(game, state, CS, 100, &clock).unwrap(),
        clock,
    )
}

#[test]
fn completion_wrapper_guards_sample_and_disabled_entry_polls_ordinary_input() {
    for (intro_enabled, disabled, extra_poll) in [(false, false, false), (true, true, true)] {
        let assets = synthetic_assets();
        let program = program();
        let memory = Memory {
            reads: Cell::new(0),
            missing: false,
        };
        let mut state = GameplayState::default();
        state.controls.preferences.digitized_samples_enabled = intro_enabled;
        state.controls.preferences.sound_disabled = disabled;
        state.controls.sound_poll_counter = 0x1234;
        state.keyboard.apply_scan(0x4d);
        let (mut task, mut clock) = setup(&assets, Some(state));
        let resources = GameplayResources {
            graphics: &assets,
            procedural: &program,
            samples: &memory,
        };
        let mut events = Vec::new();
        assert_eq!(
            task.advance_until(1, &resources, &mut clock, &mut |tick, event| events
                .push((tick, event)))
                .unwrap(),
            GameplayProgress::Pending
        );
        assert!(task.sample.is_none());
        assert_eq!(memory.reads.get(), 0);
        assert_eq!(task.state.next_playback, 0);
        assert_eq!(task.state.controls.sound_poll_counter, 0x1234);
        assert_eq!(task.state.keyboard.held_keys(0).actions, 8);
        assert_eq!(
            (
                task.game.enemy.snapshots()[1].animation_phase,
                task.game.enemy.snapshots()[1].depth
            ),
            (2, 50)
        );
        assert_eq!(
            events
                .iter()
                .any(|(_, e)| matches!(e, GameplayEvent::InputPoll { site: 0x73b2, .. })),
            extra_poll
        );
        assert!(events.iter().any(|(_, e)| matches!(
            e,
            GameplayEvent::Action(GameplayAction::PlaySample { site: 0x42bf, .. })
        )));
        assert_eq!(clock.handlers()[1], keyboard_handler(CS));
    }
}

#[test]
fn sample_suspends_gameplay_retains_software_counters_and_clears_input_at_return() {
    let assets = synthetic_assets();
    let program = program();
    let memory = Memory {
        reads: Cell::new(0),
        missing: false,
    };
    let mut state = GameplayState::default();
    state.controls.sound_poll_counter = 0x1234;
    state.keyboard.apply_scan(0x4d);
    let (mut task, mut clock) = setup(&assets, Some(state));
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &memory,
    };
    let mut events = Vec::new();
    task.schedule_scan(60, 0xbc, &clock).unwrap();
    assert_eq!(
        task.advance_until(1, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Pending
    );
    assert!(task.sample.is_some());
    assert_eq!(
        (
            task.game.enemy.snapshots()[1].animation_phase,
            task.game.enemy.snapshots()[1].depth
        ),
        (0, 22)
    );
    assert!(task.game.pass_snapshot().is_none());
    assert_eq!(
        clock.handlers()[0],
        FarPointer {
            segment: CS,
            offset: 0x57a4
        }
    );
    assert_eq!(
        clock.handlers()[1],
        FarPointer {
            segment: CS,
            offset: 0x57cc
        }
    );
    let counters = task.state.procedural.counters;
    let random = task.game.snapshot().random;
    assert_eq!(
        task.advance_until(1_100_000, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Pending
    );
    assert_eq!(task.state.procedural.counters, counters);
    assert_eq!(task.game.snapshot().random, random);
    assert_eq!(task.game.enemy.snapshots()[1].animation_phase, 0);
    assert!(!task.state.controls.preferences.sound_disabled);
    assert_eq!(task.state.keyboard.held_keys(0).actions, 8);
    assert!(matches!(
        task.advance_until(1_300_000, &resources, &mut clock, &mut |tick, event| events
            .push((tick, event)))
            .unwrap(),
        GameplayProgress::Published { body: 1, pass: 1 }
    ));
    assert!(task.sample.is_none());
    assert_eq!(memory.reads.get(), 2 * 9723);
    assert_eq!(task.state.next_playback, 1);
    assert!(task.state.controls.preferences.sound_disabled);
    assert_eq!(task.state.controls.sound_poll_counter, 0x1200);
    assert_eq!(task.state.keyboard.held_keys(0).actions, 0);
    assert_eq!(task.state.di, SamplePlayer::return_di());
    assert_eq!(
        clock.handlers()[0],
        FarPointer {
            segment: CS,
            offset: 0x7532
        }
    );
    assert_eq!(clock.handlers()[1], keyboard_handler(CS));
    assert!(clock.interrupts_enabled());
    assert_eq!(task.state.procedural.counters.divisor, 0x9000);
    assert_eq!(
        clock.speaker().pit().channel(0).unwrap().programmed_count,
        Some(0x4c68)
    );
    assert_eq!(task.game.enemy.snapshots()[1].animation_phase, 2);
    assert!(!events.iter().any(|(_, e)| matches!(
        e,
        GameplayEvent::Action(GameplayAction::Request { site: 0x067b, .. })
    )));
}

#[test]
fn missing_sample_byte_leaves_pre_return_gameplay_and_a_sticky_clock_failure() {
    let assets = synthetic_assets();
    let program = program();
    let memory = Memory {
        reads: Cell::new(0),
        missing: true,
    };
    let (mut task, mut clock) = setup(&assets, None);
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &memory,
    };
    let observed = Rc::new(Cell::new(0));
    let observed_events = observed.clone();
    let mut observe = move |_, _| observed_events.set(observed_events.get() + 1);
    let error = task
        .advance_until(100, &resources, &mut clock, &mut observe)
        .unwrap_err();
    assert!(error.to_string().contains("synthetic missing sample byte"));
    assert!(clock.check().is_err());
    assert_eq!(memory.reads.get(), 1);
    assert_eq!(
        (
            task.game.enemy.snapshots()[1].animation_phase,
            task.game.enemy.snapshots()[1].depth
        ),
        (0, 22)
    );
    let count = observed.get();
    let tick = clock.tick();
    assert!(
        task.advance_until(1000, &resources, &mut clock, &mut observe)
            .is_err()
    );
    assert_eq!(clock.tick(), tick);
    assert_eq!(memory.reads.get(), 1);
    assert_eq!(observed.get(), count);
}

#[test]
fn completion_sample_and_same_tick_keyboard_delivery_are_partition_invariant() {
    let run = |chunk: u64| {
        let assets = synthetic_assets();
        let program = program();
        let memory = Memory {
            reads: Cell::new(0),
            missing: false,
        };
        let (mut task, mut clock) = setup(&assets, None);
        let resources = GameplayResources {
            graphics: &assets,
            procedural: &program,
            samples: &memory,
        };
        // Sample IRQ0 is due at 61. IRQ1 observes both bytes before foreground
        // execution resumes, preserving the raw F2 break as the last scan.
        for (tick, scan) in [(61, 0x3c), (61, 0xbc)] {
            task.schedule_scan(tick, scan, &clock).unwrap();
        }
        let end = 1_300_000;
        let mut events = Vec::new();
        while clock.tick() < end {
            let budget = (clock.tick() + chunk).min(end);
            while matches!(
                task.advance_until(budget, &resources, &mut clock, &mut |tick, event| events
                    .push((tick, event)))
                    .unwrap(),
                GameplayProgress::Published { .. }
            ) {}
        }
        assert!(task.state.controls.preferences.sound_disabled);
        (
            events,
            format!("{:?}", task.state),
            format!("{:?}", task.game.snapshot()),
            task.game.pass_frame().clone(),
            clock.speaker().clone(),
            memory.reads.get(),
        )
    };
    assert_eq!(run(1_300_000), run(149));
}
