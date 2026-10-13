// Copyright (c) 2026 analogued. Project license has not been selected.
//! Synthetic controller checks; no SDL window, output device or originals.

use super::*;
use crate::{
    assets::{GraphicsAssets, Sprite},
    sound::{Resource, Tone},
};
use std::sync::atomic::{AtomicU64, Ordering};

fn frames() -> StartupFrames {
    StartupFrames::from_test_graphics(GraphicsAssets {
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
    })
}

fn resources() -> Resources {
    resources_with_count_one(false)
}

fn resources_with_count_one(count_one: bool) -> Resources {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "mach3-audio-controller-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    for resource in Resource::ALL {
        let samples = (0..resource.bytes())
            .map(|i| (i * 37 + 128) as u8)
            .collect::<Vec<_>>();
        std::fs::write(path.join(resource.name()), samples).unwrap();
    }
    let samples = SoundAssets::load(&path).unwrap();
    std::fs::remove_dir_all(path).unwrap();
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
            // Authored active entries exercise the DOS-compatible mode-3
            // count-one path. These are not copied game table values.
            let divisor =
                if count_one && matches!((base, index), (0x7583, 3) | (0x766c, 4) | (0x7872, 2)) {
                    1
                } else {
                    1000 + index as u16
                };
            image[base + index * 2..base + index * 2 + 2].copy_from_slice(&divisor.to_le_bytes());
        }
    }
    Resources {
        samples,
        intro: IntroProgram::synthetic(),
        procedural: ProceduralProgram::from_expanded(&image).unwrap(),
    }
}

fn rules() -> ScriptRules {
    let mut data = vec![0; 0x10000];
    for difficulty in 0..12 {
        let at = 0x04ac + difficulty * 3;
        data[at..at + 3].copy_from_slice(&[187, 188, 189]);
    }
    for index in 0..21 {
        // Empty terrain then a normal progression edge. The native renderer,
        // geometry and retained records still execute on each gameplay pass.
        data[0x0a3e + index * 50..0x0a3e + index * 50 + 2].copy_from_slice(&[0, 0xcf]);
        for choice in 0..2 {
            let offset = 0x0e58 + index * 4 + choice * 2;
            data[offset..offset + 2].copy_from_slice(&(index as u16).to_le_bytes());
        }
    }
    ScriptRules::from_data_segment(&data).unwrap()
}

fn signal(chunk: u32) -> Rc<RefCell<Synthesis>> {
    let mut synthesis = Synthesis::new().unwrap();
    synthesis.renderer = BlipRenderer::new(
        Config {
            clock_hz: PIT_HZ as u32,
            sample_rate: RATE,
            chunk_ticks: chunk,
        },
        0,
        0,
    )
    .unwrap();
    Rc::new(RefCell::new(synthesis))
}

fn advance(
    simulation: &mut Simulation,
    target: u64,
    frames: &StartupFrames,
    resources: &Resources,
    events: &mut Vec<(u64, GameplayEvent)>,
) {
    simulation
        .advance(target, frames, &rules(), resources, true, events)
        .unwrap();
    simulation.clock.check().unwrap();
}

fn until(
    simulation: &mut Simulation,
    frames: &StartupFrames,
    resources: &Resources,
    events: &mut Vec<(u64, GameplayEvent)>,
    predicate: impl Fn(&Simulation) -> bool,
) {
    for _ in 0..20_000 {
        if predicate(simulation) {
            return;
        }
        let target = simulation.clock.tick() + 100_000;
        advance(simulation, target, frames, resources, events);
    }
    panic!(
        "controller did not reach expected phase: {} tick={}",
        simulation.caption(),
        simulation.clock.tick()
    );
}

fn start(simulation: &mut Simulation, frames: &StartupFrames, resources: &Resources) {
    let state = simulation.initial.take().unwrap();
    simulation.start_intro(state, frames, resources).unwrap();
    simulation.receive(10, 0x39);
}

fn enter_playing(simulation: &mut Simulation, frames: &StartupFrames, resources: &Resources) {
    start(simulation, frames, resources);
    until(simulation, frames, resources, &mut Vec::new(), |s| {
        matches!(s.phase, Phase::Playing(_))
    });
    simulation.clear_input().unwrap();
}

#[derive(Debug, PartialEq)]
struct Outcome {
    tick: u64,
    state: String,
    world: String,
    session: String,
    visible: Frame,
    events: Vec<(u64, GameplayEvent)>,
    generated: u64,
}

fn gameplay_run(budget: u64, render_chunk: u32, discard_output: bool) -> (Outcome, Vec<i16>) {
    let frames = frames();
    let resources = resources();
    let signal = signal(render_chunk);
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    enter_playing(&mut simulation, &frames, &resources);
    let start = simulation.clock.tick();
    for (offset, raw) in [
        (20_000, 0x4b),
        (33_300, 0xcb),
        (33_300, 0x4d),
        (65_000, 0xcd),
        (72_000, 0x39),
        (350_000, 0xb9),
    ] {
        simulation.receive(start + offset, raw);
    }
    let mut audio = PlaybackAudio {
        signal: signal.clone(),
        output: None,
        started: false,
        recovering: false,
    };
    let mut pcm = Vec::new();
    let mut events = Vec::new();
    let end = start + 500_000;
    while simulation.clock.tick() < end {
        let target = (simulation.clock.tick() + budget).min(end);
        advance(&mut simulation, target, &frames, &resources, &mut events);
        if discard_output {
            audio.service(simulation.clock.tick(), end);
        } else {
            let mut synthesis = signal.borrow_mut();
            synthesis.advance(simulation.clock.tick(), None);
            for block in synthesis.blocks.drain(..) {
                pcm.extend(block.samples);
            }
        }
    }
    let Phase::Playing(runtime) = &simulation.phase else {
        panic!("bounded gameplay returned")
    };
    assert!(runtime.game().snapshot().body > 0);
    assert!(signal.borrow().error.is_none());
    let outcome = Outcome {
        tick: simulation.clock.tick(),
        state: format!("{:?}", simulation.state()),
        world: format!("{:?}", runtime.game().snapshot()),
        session: format!("{:?}", simulation.session),
        visible: simulation.visible.clone(),
        events,
        generated: signal.borrow().generated,
    };
    (outcome, pcm)
}

#[test]
fn simulation_budget_render_chunks_and_absent_output_preserve_gameplay() {
    let (baseline, pcm) = gameplay_run(500_000, CHUNK as u32, false);
    assert!(pcm.iter().any(|sample| *sample != 0));
    let (fine, fine_pcm) = gameplay_run(731, 137, false);
    assert_eq!(baseline, fine);
    assert_eq!(pcm, fine_pcm);
    let (silent, discarded) = gameplay_run(10_000, CHUNK as u32, true);
    assert_eq!(baseline, silent);
    assert!(discarded.is_empty());
}

#[test]
fn first_scene_is_published_before_pregame_sample_and_initial_body() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    start(&mut simulation, &frames, &resources);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        matches!(s.phase, Phase::Pregame { .. })
    });
    let Phase::Pregame { task, game } = &simulation.phase else {
        unreachable!()
    };
    assert_eq!(simulation.visible, *game.frame());
    assert_eq!(game.snapshot().body, 0);
    assert_eq!(task.state().next_playback, 6);
    let entry_tick = simulation.clock.tick();
    advance(
        &mut simulation,
        entry_tick,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(matches!(simulation.phase, Phase::Pregame { .. }));
    assert_eq!(simulation.state().next_playback, 7);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        matches!(s.phase, Phase::Playing(_))
    });
    assert!(simulation.clock.tick() > entry_tick);
    assert_eq!(simulation.clock.handlers()[0].offset, 0x7532);
    assert_eq!(simulation.state().procedural.counters.divisor, 0xffff);
    let Phase::Playing(runtime) = &simulation.phase else {
        unreachable!()
    };
    assert_eq!(runtime.game().snapshot().body, 0);
    assert_eq!(simulation.visible, *runtime.game().frame());
}

#[test]
fn native_pause_return_continues_clock_through_ending_and_restart() {
    let frames = frames();
    let resources = resources();
    let signal = signal(CHUNK as u32);
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    enter_playing(&mut simulation, &frames, &resources);
    let game_tick = simulation.clock.tick();
    let mut events = Vec::new();
    simulation.receive(game_tick, 0x3b);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitInitialRelease
    });
    simulation.receive(simulation.clock.tick(), 0xbb);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitPress
    });
    simulation.receive(simulation.clock.tick(), 0x3b);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitFinalRelease
    });
    simulation.receive(simulation.clock.tick(), 0xbb);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        matches!(s.phase, Phase::Ending { .. })
    });
    let return_tick = simulation.clock.tick();
    assert!(return_tick > game_tick);
    assert_eq!(simulation.clock.handlers()[0].offset, 0x7532);
    assert_eq!(simulation.state().procedural.counters.divisor, 0xffff);
    assert_eq!(
        simulation
            .clock
            .speaker()
            .pit()
            .channel(0)
            .unwrap()
            .programmed_count,
        Some(0xffff)
    );
    assert_eq!(simulation.clock.speaker().control() & 3, 0);
    assert_eq!(
        events
            .iter()
            .filter(|(_, e)| matches!(e, GameplayEvent::Published { .. }))
            .count(),
        3
    );
    assert!(events.iter().any(|(_, e)| *e == GameplayEvent::Returned));
    until(
        &mut simulation,
        &frames,
        &resources,
        &mut events,
        |s| matches!(&s.phase, Phase::Ending { ending, .. } if ending.can_return()),
    );
    simulation.receive(simulation.clock.tick(), 0x39);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        matches!(s.phase, Phase::Intro(_))
    });
    assert!(simulation.session.retained_world.is_some());
    let ending_random = simulation.session.random;
    let ending_tick = simulation.clock.tick();
    assert!(ending_tick > return_tick);
    simulation.receive(ending_tick, 0xb9);
    simulation.receive(ending_tick + 10, 0x39);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        matches!(s.phase, Phase::Playing(_))
    });
    assert!(simulation.clock.tick() > ending_tick);
    assert_ne!(simulation.session.random, ending_random);
    assert_eq!(simulation.state().pause.state, PauseState::Running);
    assert!(simulation.state().next_playback > 7);
    signal.borrow_mut().advance(simulation.clock.tick(), None);
    assert!(signal.borrow().error.is_none());
    assert!(signal.borrow().generated > samples_at(return_tick));
}

#[test]
fn received_future_scan_survives_intro_and_pregame_boundaries() {
    let frames = frames();
    let resources = resources();
    let mut reference = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    enter_playing(&mut reference, &frames, &resources);
    let playing_tick = reference.clock.tick();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    start(&mut simulation, &frames, &resources);
    simulation.receive(playing_tick + 10, 0x4d);
    let end = playing_tick + 100_000;
    while simulation.clock.tick() < end {
        advance(&mut simulation, end, &frames, &resources, &mut Vec::new());
    }
    assert!(matches!(simulation.phase, Phase::Playing(_)));
    assert_eq!(simulation.state().keyboard.held_keys(0).actions, 8);
    simulation.clear_input().unwrap();
    assert_eq!(simulation.state().keyboard.held_keys(0).actions, 0);
}

#[test]
fn background_timer_priority_does_not_overwrite_same_tick_keyboard_bytes() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = simulation.initial.take().unwrap();
    // Mode 0 lowers OUT0; reselecting mode 3 raises it immediately, leaving an
    // IRQ0 request pending at the same tick as two received keyboard bytes.
    crate::sound::write(&mut simulation.clock, 0x43, 0x30);
    crate::sound::write(&mut simulation.clock, 0x43, 0x36);
    simulation.receive(simulation.clock.tick(), 0x39);
    simulation.receive(simulation.clock.tick(), 0xb9);
    simulation
        .drain_background(&mut state, &resources.procedural)
        .unwrap();
    assert!(simulation.scans.is_empty());
    assert_eq!(state.keyboard.held_keys(0).actions, 0);
    simulation.clock.check().unwrap();
    assert_eq!(
        simulation
            .clock
            .advance_to(simulation.clock.tick())
            .unwrap(),
        Progress::Reached
    );
}

#[test]
fn native_pause_and_player_fire_survive_active_mode_three_count_one() {
    let frames = frames();
    let resources = resources_with_count_one(true);
    assert_eq!(resources.procedural.divisor(Tone::First, 3).unwrap(), 1);
    assert_eq!(resources.procedural.divisor(Tone::Fourth, 2).unwrap(), 1);
    let signal = signal(CHUNK as u32);
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    enter_playing(&mut simulation, &frames, &resources);
    let mut events = Vec::new();
    simulation.receive(simulation.clock.tick(), 0x3b);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitInitialRelease
            && s.state().procedural.tone_indices[0] == 16
            && s.state().procedural.priority == 255
    });
    assert_eq!(simulation.state().procedural.priority, 255);
    let Phase::Playing(runtime) = &simulation.phase else {
        unreachable!()
    };
    assert_eq!(runtime.game().snapshot().body, 0);
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        GameplayEvent::Action(crate::sound::GameplayAction::Request {
            effect: Effect::Tone(Tone::First),
            ..
        })
    )));
    simulation.receive(simulation.clock.tick(), 0xbb);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitPress
    });
    simulation.receive(simulation.clock.tick(), 0x39);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::AwaitFinalRelease
    });
    simulation.receive(simulation.clock.tick(), 0xb9);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().pause.state == PauseState::Running
    });
    simulation.receive(simulation.clock.tick(), 0x39);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.state().procedural.tone_indices[3] >= 3
    });
    simulation.receive(simulation.clock.tick(), 0xb9);
    assert!(events.iter().any(|(_, event)| matches!(
        event,
        GameplayEvent::Action(crate::sound::GameplayAction::Request {
            site: 0x20d3 | 0x2128,
            effect: Effect::Tone(Tone::Fourth),
        })
    )));
    let end = simulation.clock.tick() + 1_000_000;
    while simulation.clock.tick() < end {
        let target = (simulation.clock.tick() + 10_000).min(end);
        advance(&mut simulation, target, &frames, &resources, &mut events);
    }
    assert!(matches!(simulation.phase, Phase::Playing(_)));
    assert_eq!(simulation.state().procedural.tone_indices[3], 0);
    assert_eq!(
        simulation.state().procedural.handler,
        Some(crate::sound::Handler::Noise)
    );
    signal.borrow_mut().advance(simulation.clock.tick(), None);
    assert!(signal.borrow().error.is_none());
}

#[test]
fn second_tone_with_active_count_one_completes_on_controller_background_clock() {
    let frames = frames();
    let resources = resources_with_count_one(true);
    assert_eq!(resources.procedural.divisor(Tone::Second, 4).unwrap(), 1);
    let signal = signal(CHUNK as u32);
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    let mut state = simulation.initial.take().unwrap();
    assert!(
        state
            .procedural
            .request(Effect::Tone(Tone::Second), CS, 0, &mut simulation.clock)
    );
    for _ in 0..100 {
        if state.procedural.tone_indices[1] == 46 && state.procedural.priority == 255 {
            break;
        }
        let target = simulation.clock.tick() + 100_000;
        simulation
            .advance_background(target, &mut state, &resources.procedural)
            .unwrap();
        simulation
            .drain_background(&mut state, &resources.procedural)
            .unwrap();
    }
    assert_eq!(state.procedural.tone_indices[1], 46);
    assert_eq!(state.procedural.priority, 255);
    assert_eq!(
        simulation.clock.handlers()[0].offset,
        Tone::Second.handler()
    );
    simulation.clock.check().unwrap();
    signal.borrow_mut().advance(simulation.clock.tick(), None);
    assert!(signal.borrow().error.is_none());
    assert!(signal.borrow().generated > 0);
}
