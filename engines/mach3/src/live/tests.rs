// Copyright (c) 2026 analogued. Project license has not been selected.

use super::*;
use crate::{
    assets::{GraphicsAssets, Sprite},
    opening::{PauseEvent, PauseState},
    sound::{ByteOrigin, Effect, FarPointer, MemoryByte, ProceduralProgram, Resource},
};
use reforge_audio::speaker::PcSpeaker;
use std::{cell::RefCell, rc::Rc};

const CS: u16 = 0x1234;
type Log = Rc<RefCell<Vec<(Stamp, ClockEvent)>>>;

struct Memory;
impl SampleMemory for Memory {
    fn read(&self, _: Resource, offset: u16) -> Result<MemoryByte, Error> {
        Ok(MemoryByte {
            value: offset as u8,
            origin: ByteOrigin::Asset,
        })
    }
}

fn graphics() -> GraphicsAssets {
    GraphicsAssets {
        primary: vec![0; 28_960],
        alternate: vec![0; 16_000],
        sprites: (0..190)
            .map(|index| Sprite {
                width_bytes: 14,
                height: 18,
                descriptor: if index == 13 {
                    [0, 0, 15, 0, 0, 0, 0]
                } else {
                    [0; 7]
                },
                color: vec![0x55; 14 * 18],
                mask: vec![0; 14 * 18],
            })
            .collect(),
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
        for index in 0..words {
            image[base + 2 * index..base + 2 * index + 2]
                .copy_from_slice(&(1000 + index as u16).to_le_bytes());
        }
    }
    ProceduralProgram::from_expanded(&image).unwrap()
}

fn begin(
    assets: &GraphicsAssets,
    log: Log,
    steering_enabled: bool,
) -> (GameplayRuntime, Clock<impl FnMut(Stamp, ClockEvent)>) {
    let mut clock = Clock::new(
        PcSpeaker::new(0x60, [true; 3]),
        [
            FarPointer::default(),
            FarPointer {
                segment: CS,
                offset: 0x744a,
            },
        ],
        true,
        move |stamp, event| log.borrow_mut().push((stamp, event)),
    );
    let mut state = GameplayState {
        steering_enabled,
        ..GameplayState::default()
    };
    state
        .procedural
        .request(Effect::Noise { divisor: 1850 }, CS, 0, &mut clock);
    (
        GameplayRuntime::begin(Opening::new(assets).unwrap(), state, CS, 64, &clock).unwrap(),
        clock,
    )
}

#[derive(Debug, PartialEq)]
struct Run {
    device: Vec<(Stamp, ClockEvent)>,
    gameplay: Vec<(u64, GameplayEvent)>,
    images: Vec<Frame>,
    state: String,
    world: String,
}

fn run(chunk: u64) -> Run {
    let assets = graphics();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let log = Log::default();
    let (mut runtime, mut clock) = begin(&assets, log.clone(), true);
    for (tick, raw) in [
        (20_000, 0x4b),
        (33_300, 0xcb),
        (33_300, 0x4d),
        (65_000, 0xcd),
    ] {
        runtime.schedule_scan(tick, raw, &clock).unwrap();
    }
    let mut gameplay = Vec::new();
    let mut images = Vec::new();
    while clock.tick() < 500_000 {
        let target = (clock.tick() + chunk).min(500_000);
        loop {
            match runtime
                .advance_until(target, &resources, &mut clock, &mut |tick, event| {
                    gameplay.push((tick, event))
                })
                .unwrap()
            {
                GameplayProgress::Pending => break,
                GameplayProgress::Published { .. } => images.push(runtime.visible_frame().clone()),
                GameplayProgress::Returned => panic!("prefix must remain in gameplay"),
            }
        }
    }
    let state = format!("{:?}", runtime.state());
    let world = format!("{:?}", runtime.game().snapshot());
    drop(clock);
    let device = Rc::try_unwrap(log).unwrap().into_inner();
    Run {
        device,
        gameplay,
        images,
        state,
        world,
    }
}

#[test]
fn received_scans_and_publications_are_identical_across_host_stalls() {
    let stalled = run(500_000);
    assert!(!stalled.images.is_empty());
    assert_eq!(stalled, run(731));
    assert_eq!(stalled, run(10_000));
}

#[test]
fn visible_frame_changes_only_at_publication() {
    let assets = graphics();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (mut runtime, mut clock) = begin(&assets, Log::default(), true);
    let initial = runtime.visible_frame().clone();
    assert_eq!(
        runtime
            .advance_until(33_301, &resources, &mut clock, &mut |_, _| {})
            .unwrap(),
        GameplayProgress::Pending
    );
    assert_eq!(runtime.visible_frame(), &initial);
    assert_eq!(
        runtime
            .advance_until(100_000, &resources, &mut clock, &mut |_, _| {})
            .unwrap(),
        GameplayProgress::Published { body: 1, pass: 1 }
    );
    assert_eq!(clock.tick(), 66_601);
    assert_eq!(runtime.visible_frame(), runtime.game().pass_frame());
    assert_ne!(runtime.visible_frame(), &initial);
    let published = runtime.visible_frame().clone();
    assert_eq!(
        runtime
            .advance_until(clock.tick(), &resources, &mut clock, &mut |_, _| {})
            .unwrap(),
        GameplayProgress::Pending
    );
    assert_eq!(runtime.visible_frame(), &published);
}

#[test]
fn focus_reset_discards_future_scans_and_preserves_native_pause() {
    let assets = graphics();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (mut runtime, mut clock) = begin(&assets, Log::default(), false);
    for (tick, raw) in [(10_000, 0x3b), (40_000, 0xbb), (160_000, 0x3b)] {
        runtime.schedule_scan(tick, raw, &clock).unwrap();
    }
    runtime
        .advance_until(33_301, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert_eq!(runtime.state().pause.state, PauseState::AwaitInitialRelease);
    runtime.clear_input(&clock).unwrap();
    assert_eq!(runtime.state().pause.state, PauseState::AwaitInitialRelease);
    runtime
        .advance_until(200_000, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert_eq!(runtime.state().pause.state, PauseState::AwaitPress);
    assert_eq!(runtime.state().keyboard.held_keys(0).actions, 0);
    // Resetting the duplicate filter allows the same F1 make byte again.
    runtime.schedule_scan(200_000, 0x3b, &clock).unwrap();
    runtime
        .advance_until(200_000, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert!(runtime.state().keyboard.held_keys(0).f1);
    let mut session = SessionState {
        random: [1, 2, 3],
        ..SessionState::default()
    };
    runtime.synchronize_session(&mut session);
    assert_eq!(session.random, [1, 2, 3]);
    assert!(session.keyboard.held_keys(0).f1);
    assert_eq!(session.pause.state, runtime.state().pause.state);
}

#[test]
fn native_pause_end_game_returns_only_after_three_published_passes() {
    let assets = graphics();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (mut runtime, mut clock) = begin(&assets, Log::default(), false);
    for (tick, raw) in [
        (10_000, 0x3b),
        (40_000, 0xbb),
        (160_000, 0x3b),
        (280_000, 0xbb),
    ] {
        runtime.schedule_scan(tick, raw, &clock).unwrap();
    }
    let mut publications = Vec::new();
    let mut events = Vec::new();
    loop {
        match runtime
            .advance_until(1_000_000, &resources, &mut clock, &mut |tick, event| {
                events.push((tick, event))
            })
            .unwrap()
        {
            GameplayProgress::Published { body, pass } => publications.push((body, pass)),
            GameplayProgress::Returned => break,
            GameplayProgress::Pending => panic!("game must return within budget"),
        }
    }
    assert_eq!(publications, [(1, 1), (1, 2), (1, 3)]);
    assert!(
        events
            .iter()
            .any(|(_, event)| *event == GameplayEvent::Pause(PauseEvent::EndGame))
    );
    assert!(clock.tick() < 1_000_000);
    let (game, state) = runtime.into_parts().unwrap();
    assert!(game.complete());
    assert_eq!(game.snapshot().render_pass, 3);
    assert_eq!(state.pause.state, PauseState::Running);
    clock.check().unwrap();
}

#[test]
fn disabled_steering_keeps_function_keys_and_pause_resume_input() {
    let assets = graphics();
    let program = program();
    let resources = GameplayResources {
        graphics: &assets,
        procedural: &program,
        samples: &Memory,
    };
    let (mut baseline, mut baseline_clock) = begin(&assets, Log::default(), true);
    let (mut disabled, mut clock) = begin(&assets, Log::default(), false);
    disabled.schedule_scan(10_000, 0x4d, &clock).unwrap();
    for (runtime, clock) in [
        (&mut baseline, &mut baseline_clock),
        (&mut disabled, &mut clock),
    ] {
        runtime
            .advance_until(100_000, &resources, clock, &mut |_, _| {})
            .unwrap();
    }
    assert_eq!(
        format!("{:?}", baseline.game().snapshot().player),
        format!("{:?}", disabled.game().snapshot().player)
    );
    assert_eq!(disabled.state().keyboard.held_keys(0).actions, 8);
    disabled.schedule_scan(clock.tick(), 0x3d, &clock).unwrap();
    disabled
        .advance_until(100_000, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert_eq!(
        disabled.state().controls.preferences.display_mode,
        crate::opening::DisplayMode::Monochrome
    );

    let (mut paused, mut clock) = begin(&assets, Log::default(), false);
    for (tick, raw) in [
        (10_000, 0x3b),
        (40_000, 0xbb),
        (160_000, 0x4d),
        (280_000, 0xcd),
    ] {
        paused.schedule_scan(tick, raw, &clock).unwrap();
    }
    paused
        .advance_until(150_000, &resources, &mut clock, &mut |_, _| {})
        .unwrap();
    assert_eq!(paused.state().pause.state, PauseState::AwaitPress);
    assert_eq!(paused.game().snapshot().body, 0);
    assert!(matches!(
        paused
            .advance_until(500_000, &resources, &mut clock, &mut |_, _| {})
            .unwrap(),
        GameplayProgress::Published { body: 1, pass: 1 }
    ));
    assert_eq!(paused.state().pause.state, PauseState::Running);
    assert!(!paused.game().snapshot().player.game_over);
}
