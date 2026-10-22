// Copyright (c) 2026 analogued. Project license has not been selected.
//! Synthetic controller checks; no SDL window or originals. Host playback
//! checks use an isolated dummy-device child process.

use super::*;
use crate::{
    assets::{GraphicsAssets, Sprite},
    sound::{Resource, Tone},
};
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn shared_prefill_allows_startup_and_catchup_recovery() {
    const TEST: &str =
        "viewer::realtime::tests::shared_prefill_allows_startup_and_catchup_recovery";
    const MARKER: &str = "REFORGE_MACH3_PREFILL_TEST";
    if std::env::var(MARKER).as_deref() != Ok(TEST) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1"])
            .env(MARKER, TEST)
            .env("SDL_AUDIO_DRIVER", "dummy")
            .env("SDL_AUDIODRIVER", "dummy")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use reforge_sdl::audio::queue::Phase;
    let host = Playback::new(Default::default());
    let signal = signal(CHUNK as u32);
    let mut audio = PlaybackAudio::new(signal.clone(), host.clone());
    assert!(host.available());
    let at = |ms| PIT_HZ * ms / 1000;
    for ms in [10, 20, 30, 40] {
        audio.service(at(ms), at(ms));
        assert_eq!(audio.queue.phase(), Phase::Priming);
    }
    audio.service(at(60), at(60));
    assert_eq!(audio.queue.phase(), Phase::Running);
    audio.service(at(60), at(400));
    assert_eq!(audio.queue.phase(), Phase::Recovering);
    assert_eq!(host.queued_samples().unwrap(), 0);
    // Full catch-up can now prime from the nominal playback position, without
    // repeatedly limiting each submission to a 30 ms tail.
    audio.service(at(410), at(410));
    assert_eq!(audio.queue.phase(), Phase::Running);
    audio.service(at(440), at(440));
    assert_eq!(audio.queue.phase(), Phase::Running);
    assert_eq!(audio.queue.stats().recoveries, 1);
    assert!(signal.borrow().error.is_none());
}

#[test]
fn stalled_startup_and_device_reset_wait_for_current_prefill() {
    const TEST: &str =
        "viewer::realtime::tests::stalled_startup_and_device_reset_wait_for_current_prefill";
    const MARKER: &str = "REFORGE_MACH3_PREFILL_TEST";
    if std::env::var(MARKER).as_deref() != Ok(TEST) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1"])
            .env(MARKER, TEST)
            .env("SDL_AUDIO_DRIVER", "dummy")
            .env("SDL_AUDIODRIVER", "dummy")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use reforge_sdl::audio::queue::Phase;
    let at = |ms| PIT_HZ * ms / 1000;
    for reset in [false, true] {
        let host = Playback::new(Default::default());
        let signal = signal(CHUNK as u32);
        let mut audio = PlaybackAudio::new(signal.clone(), host.clone());
        assert!(host.available());
        let epoch = if reset {
            audio.service(at(60), at(60));
            assert_eq!(audio.queue.phase(), Phase::Running);
            // Reopen the endpoint as a device-setting change would, then let
            // the production adapter reset at its generated-sample boundary.
            host.start(RATE).unwrap();
            assert!(audio.sync_settings(at(60), AudioRenderer::Speaker));
            60
        } else {
            0
        };
        for ms in (10..=280).step_by(10) {
            audio.service(at(epoch + ms), at(epoch + 300));
            assert_eq!(audio.queue.phase(), Phase::Recovering);
            assert_eq!(host.queued_samples().unwrap(), 0);
        }
        audio.service(at(epoch + 320), at(epoch + 320));
        assert_eq!(audio.queue.phase(), Phase::Running);
        assert_eq!(audio.queue.stats().recoveries, 1);
        assert!(audio.queue.stats().expired >= RATE as u64 / 5);
        assert!(signal.borrow().error.is_none());
    }
}

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
    rules_with_commands(&[0])
}

fn rules_with_commands(commands: &[u8]) -> ScriptRules {
    let mut data = vec![0; 0x10000];
    for difficulty in 0..12 {
        let at = 0x04ac + difficulty * 3;
        data[at..at + 3].copy_from_slice(&[187, 188, 189]);
    }
    for index in 0..21 {
        // Empty terrain then a normal progression edge. The native renderer,
        // geometry and retained records still execute on each gameplay pass.
        let at = 0x0a3e + index * 50;
        data[at..at + commands.len()].copy_from_slice(commands);
        data[at + commands.len()] = 0xcf;
        for choice in 0..2 {
            let offset = 0x0e58 + index * 4 + choice * 2;
            data[offset..offset + 2].copy_from_slice(&(index as u16).to_le_bytes());
        }
    }
    ScriptRules::from_data_segment(&data).unwrap()
}

fn signal(chunk: u32) -> Rc<RefCell<Synthesis>> {
    let mut synthesis = Synthesis::new().unwrap();
    synthesis.renderer = CategorizedRenderer::new(
        Config {
            clock_hz: PIT_HZ as u32,
            sample_rate: RATE,
            chunk_ticks: chunk,
        },
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
        simulation.status(),
        simulation.clock.tick()
    );
}

// Isolated gameplay/ending tests start after the original splash timer return.
fn post_splash_state(simulation: &mut Simulation) -> GameplayState {
    let mut state = simulation.initial.take().unwrap();
    state
        .procedural
        .request(Effect::Reset, CS, 0, &mut simulation.clock);
    state
}

fn start(simulation: &mut Simulation, frames: &StartupFrames, resources: &Resources) {
    simulation.finish_splash(frames, resources).unwrap();
    simulation.receive(10, 0x39);
}

#[test]
fn splash_automatically_returns_at_the_bios_clock_deadline_without_game_ticks() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    assert_eq!(simulation.clock.handlers()[0], FarPointer::default());
    assert_eq!(
        simulation
            .clock
            .speaker()
            .pit()
            .channel(0)
            .unwrap()
            .programmed_count,
        Some(crate::startup::SPLASH_DIVISOR)
    );
    let log = logged_ending_clock(&mut simulation);
    // PIT loading takes one clock, then 219 accelerated BIOS IRQs reach DOS
    // second 12. Assets are already decoded; no guest file-loading delay.
    let deadline = 1 + 219 * u64::from(crate::startup::SPLASH_DIVISOR);
    advance(
        &mut simulation,
        deadline - 1,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(matches!(simulation.phase, Phase::Splash));
    assert_eq!(simulation.visible, *frames.frame(Screen::Splash));
    assert_eq!(simulation.state().procedural.counters.units, 0);
    assert_eq!(simulation.state().next_playback, 0);
    assert!(
        !log.borrow()
            .iter()
            .any(|(_, event)| matches!(event, ClockEvent::Action(SoundAction::SetHandler { .. })))
    );
    advance(
        &mut simulation,
        deadline,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(matches!(simulation.phase, Phase::Intro(_)));
    assert_eq!(simulation.clock.tick(), deadline);
    assert_eq!(
        simulation.visible,
        *frames.return_intro(simulation.intro_choice)
    );
    assert_eq!(simulation.state().procedural.counters.units, 0);
    assert_eq!(simulation.state().next_playback, 0);
    let log = log.borrow();
    assert_eq!(
        log.iter()
            .filter(|(_, e)| matches!(e, ClockEvent::Interrupt(_)))
            .count(),
        219
    );
    let actions = log
        .iter()
        .filter_map(|(stamp, event)| {
            if let ClockEvent::Action(action) = event {
                if matches!(
                    action,
                    SoundAction::InterruptReturn | SoundAction::Write { port: 0x20, .. }
                ) {
                    return None;
                }
                Some((stamp.tick, action))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(actions.iter().all(|(tick, _)| *tick == deadline));
    assert_eq!(actions.len(), 6);
    assert!(
        matches!(actions[0].1, SoundAction::SetHandler { irq: 0, handler } if handler.offset == 0x7532)
    );
    assert!(matches!(
        actions[1].1,
        SoundAction::Write {
            port: 0x43,
            value: 0x36,
            ..
        }
    ));
    for action in &actions[2..4] {
        assert!(matches!(
            action.1,
            SoundAction::Write {
                port: 0x40,
                value: 0xff,
                ..
            }
        ));
    }
    assert!(matches!(
        actions[4].1,
        SoundAction::Read {
            port: 0x61,
            value: 0x60
        }
    ));
    assert!(matches!(
        actions[5].1,
        SoundAction::Write {
            port: 0x61,
            value: 0x60,
            ..
        }
    ));
}

#[test]
fn splash_return_is_independent_of_host_budget_and_renderer_chunks() {
    fn run(budget: u64, chunk: u32) -> (u64, String, Frame, Vec<(u64, ClockEvent)>) {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(chunk)).unwrap();
        let log = logged_ending_clock(&mut simulation);
        while matches!(simulation.phase, Phase::Splash) {
            let target = simulation.clock.tick() + budget;
            advance(
                &mut simulation,
                target,
                &frames,
                &resources,
                &mut Vec::new(),
            );
        }
        let transactions = log
            .borrow()
            .iter()
            .filter(|(_, event)| !matches!(event, ClockEvent::Device(_)))
            .map(|(stamp, event)| (stamp.tick, event.clone()))
            .collect();
        (
            simulation.clock.tick(),
            format!("{:?}", simulation.state()),
            simulation.visible,
            transactions,
        )
    }
    assert_eq!(run(3 * PIT_HZ, CHUNK as u32), run(731, 137));
}

#[test]
fn space_skip_enters_intro_immediately_before_or_during_the_wait() {
    let frames = frames();
    let resources = resources();
    for tick in [0, 1_000_000] {
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        advance(&mut simulation, tick, &frames, &resources, &mut Vec::new());
        simulation.finish_splash(&frames, &resources).unwrap();
        assert_eq!(simulation.clock.tick(), tick);
        assert!(matches!(simulation.phase, Phase::Intro(_)));
        assert_eq!(simulation.state().procedural.counters.units, 0);
        assert_eq!(simulation.state().keyboard.held_keys(0).actions, 0);
        assert!(simulation.scans.is_empty());
        assert_eq!(simulation.state().next_playback, 0);
        assert!(simulation.finish_splash(&frames, &resources).is_err());
    }
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
    transactions: Vec<(u64, ClockEvent)>,
}

#[derive(Default)]
struct QueueSink {
    queued: usize,
}
impl reforge_sdl::audio::queue::Sink for QueueSink {
    fn queued(&self) -> Result<usize, String> {
        Ok(self.queued)
    }
    fn submit(&mut self, samples: &[i16]) -> Result<(), String> {
        self.queued += samples.len();
        Ok(())
    }
    fn pause(&mut self, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.queued = 0;
        Ok(())
    }
}

fn gameplay_run(budget: u64, render_chunk: u32, discard_output: bool) -> (Outcome, Vec<i16>) {
    gameplay_run_mode(budget, render_chunk, discard_output, AudioRenderer::Speaker)
}

fn logged_initial_signal_clock(
    simulation: &mut Simulation,
    signal: Rc<RefCell<Synthesis>>,
) -> Rc<RefCell<Vec<(u64, ClockEvent)>>> {
    // Install observation at the initial clock boundary: no IRQ is pending.
    // The cloned devices retain the splash reload already programmed at tick 0.
    let log = Rc::new(RefCell::new(Vec::new()));
    let observe = log.clone();
    simulation.clock = Clock::new(
        simulation.clock.speaker().clone(),
        simulation.clock.handlers(),
        true,
        Box::new(move |stamp, event| {
            observe.borrow_mut().push((stamp.tick, event.clone()));
            signal.borrow_mut().observe(stamp, event);
        }),
    );
    log
}

fn gameplay_run_mode(
    budget: u64,
    render_chunk: u32,
    discard_output: bool,
    mode: AudioRenderer,
) -> (Outcome, Vec<i16>) {
    gameplay_run_switching(budget, render_chunk, discard_output, mode, false)
}
fn gameplay_run_switching(
    budget: u64,
    render_chunk: u32,
    discard_output: bool,
    mode: AudioRenderer,
    switch: bool,
) -> (Outcome, Vec<i16>) {
    gameplay_run_levels(
        budget,
        render_chunk,
        discard_output,
        mode,
        switch,
        reforge_audio::category::CategoryVolumes::default(),
    )
}
fn gameplay_run_levels(
    budget: u64,
    render_chunk: u32,
    discard_output: bool,
    mode: AudioRenderer,
    switch: bool,
    volumes: reforge_audio::category::CategoryVolumes,
) -> (Outcome, Vec<i16>) {
    let frames = frames();
    let resources = resources();
    let signal = signal(render_chunk);
    signal.borrow_mut().policy = RenderingPolicy::new(mode, false);
    signal.borrow_mut().renderer.set_volumes(volumes).unwrap();
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    let log = logged_initial_signal_clock(&mut simulation, signal.clone());
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
        host: Playback::new(Default::default()),
        generation: 0,
        queue: Queue::new(RATE, Policy::default()).unwrap(),
    };
    let mut queued = Queue::new(RATE, Policy::default()).unwrap();
    let mut sink = QueueSink::default();
    let mut pcm = Vec::new();
    let mut events = Vec::new();
    let end = start + 500_000;
    while simulation.clock.tick() < end {
        let target = (simulation.clock.tick() + budget).min(end);
        advance(&mut simulation, target, &frames, &resources, &mut events);
        if switch {
            let mode = if (simulation.clock.tick() - start) / 50_000 % 2 == 0 {
                AudioRenderer::Speaker
            } else {
                AudioRenderer::EnhancedPcm
            };
            signal.borrow_mut().set_mode(mode, simulation.clock.tick());
        }
        if discard_output {
            // Exercise recovery with this same production-generated PCM, not
            // just absent output. The target jumps by250ms once while native
            // execution continues unchanged; controlled device input is empty.
            let mut synthesis = signal.borrow_mut();
            synthesis.advance(simulation.clock.tick(), None);
            for block in &synthesis.blocks {
                queued.push(block.start, block.samples.clone()).unwrap();
            }
            drop(synthesis);
            sink.queued = 0;
            let jump = if simulation.clock.tick() >= start + 200_000 {
                RATE as u64 / 4
            } else {
                0
            };
            queued
                .service(&mut sink, samples_at(simulation.clock.tick()) + jump)
                .unwrap();
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
    if discard_output {
        assert!(queued.stats().recoveries > 0);
    }
    let outcome = Outcome {
        tick: simulation.clock.tick(),
        state: format!("{:?}", simulation.state()),
        world: format!("{:?}", runtime.game().snapshot()),
        session: format!("{:?}", simulation.session),
        visible: simulation.visible.clone(),
        events,
        generated: signal.borrow().generated,
        transactions: log.borrow().clone(),
    };
    (outcome, pcm)
}

#[test]
fn category_muting_changes_only_host_audio_not_gameplay_or_device_execution() {
    use reforge_audio::category::CategoryVolumes;
    for mode in [AudioRenderer::Speaker, AudioRenderer::EnhancedPcm] {
        let (normal, audible) = gameplay_run_mode(10_000, CHUNK as u32, false, mode);
        let (muted, silent) = gameplay_run_levels(
            10_000,
            CHUNK as u32,
            false,
            mode,
            false,
            CategoryVolumes([0; 3]),
        );
        assert_eq!(normal, muted);
        assert_eq!(audible.len(), silent.len());
        assert!(audible.iter().any(|s| *s != 0));
        assert!(silent.iter().all(|s| *s == 0));
    }
}

#[test]
fn changing_host_renderer_preserves_device_execution_and_gameplay() {
    let (baseline, _) = gameplay_run_mode(10_000, CHUNK as u32, false, AudioRenderer::Speaker);
    let (switched, _) =
        gameplay_run_switching(10_000, CHUNK as u32, false, AudioRenderer::Speaker, true);
    assert_eq!(baseline, switched);
}

#[test]
fn enhanced_pcm_changes_presentation_while_preserving_execution_and_chunk_invariance() {
    let (reference, speaker_pcm) = gameplay_run(500_000, CHUNK as u32, false);
    let (enhanced, enhanced_pcm) =
        gameplay_run_mode(500_000, CHUNK as u32, false, AudioRenderer::EnhancedPcm);
    assert_eq!(reference, enhanced);
    assert_ne!(speaker_pcm, enhanced_pcm);
    assert_eq!(speaker_pcm.len(), enhanced_pcm.len());
    let (fine, fine_pcm) = gameplay_run_mode(731, 137, false, AudioRenderer::EnhancedPcm);
    assert_eq!(enhanced, fine);
    assert_eq!(enhanced_pcm, fine_pcm);
    let (discarded, pcm) = gameplay_run_mode(10_000, 731, true, AudioRenderer::EnhancedPcm);
    assert_eq!(enhanced, discarded);
    assert!(pcm.is_empty());
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
fn retained_intro_direction_updates_initial_scene_before_pregame_playback() {
    for (intro_enabled, sound_disabled, steering_enabled) in [
        (false, false, true),
        (true, true, true),
        (false, false, false),
    ] {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let state = simulation.initial.as_mut().unwrap();
        state.controls.preferences.digitized_samples_enabled = intro_enabled;
        state.controls.preferences.sound_disabled = sound_disabled;
        simulation.finish_splash(&frames, &resources).unwrap();
        let tick = simulation.clock.tick();
        simulation.receive(tick, 0x4d);
        simulation.receive(tick, 0x39);
        let mut events = Vec::new();
        for _ in 0..100 {
            if matches!(simulation.phase, Phase::Pregame { .. }) {
                break;
            }
            simulation
                .advance(
                    simulation.clock.tick() + 100_000,
                    &frames,
                    &rules(),
                    &resources,
                    steering_enabled,
                    &mut events,
                )
                .unwrap();
        }
        let Phase::Pregame { game, task } = &simulation.phase else {
            panic!()
        };
        let state = game.snapshot();
        assert_eq!(task.state().keyboard.held_keys(0).actions, 24);
        assert_eq!(state.player.velocity, [i16::from(steering_enabled), 0]);
        assert_eq!(
            state.player.coordinates[1],
            if steering_enabled { 4 } else { 0 }
        );
        assert_eq!(
            state.player.effective_input,
            if steering_enabled { 24 } else { 0 }
        );
        assert_eq!(state.static_lateral, if steering_enabled { -8 } else { 0 });
        assert_eq!((state.body, state.render_pass), (0, 0));
        assert_eq!(simulation.visible, *game.frame());
        assert!(events.iter().all(|(_, event)| !matches!(
            event,
            GameplayEvent::Published { .. } | GameplayEvent::Action(_)
        )));
    }
}

#[test]
fn native_pause_return_continues_clock_through_ending_and_restart() {
    assert_eq!(
        native_pause_return_run(AudioRenderer::Speaker),
        native_pause_return_run(AudioRenderer::EnhancedPcm)
    );
}

fn native_pause_return_run(mode: AudioRenderer) -> Outcome {
    let frames = frames();
    let resources = resources();
    let signal = signal(CHUNK as u32);
    signal.borrow_mut().policy = RenderingPolicy::new(mode, false);
    let mut simulation = Simulation::new(&frames, signal.clone()).unwrap();
    let log = logged_initial_signal_clock(&mut simulation, signal.clone());
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
    let Phase::Playing(runtime) = &simulation.phase else {
        unreachable!()
    };
    Outcome {
        tick: simulation.clock.tick(),
        state: format!("{:?}", simulation.state()),
        world: format!("{:?}", runtime.game().snapshot()),
        session: format!("{:?}", simulation.session),
        visible: simulation.visible.clone(),
        events,
        generated: signal.borrow().generated,
        transactions: log.borrow().clone(),
    }
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
    let mut state = post_splash_state(&mut simulation);
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
    let mut state = post_splash_state(&mut simulation);
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

fn ending_simulation(frames: &StartupFrames) -> Simulation {
    ending_simulation_with_best(frames, 0)
}

fn ending_simulation_with_best(frames: &StartupFrames, best_score: u32) -> Simulation {
    let mut simulation = Simulation::new(frames, signal(CHUNK as u32)).unwrap();
    simulation.session.best_score = best_score;
    let state = post_splash_state(&mut simulation);
    let start = simulation.session.begin_game();
    let game = Gameplay::for_play(frames.graphics(), rules(), start).unwrap();
    simulation.visible = game.frame().clone();
    simulation.phase = Phase::Ending {
        ending: Box::new(Ending::pending_with_best(
            &game.snapshot(),
            game.frame(),
            best_score,
        )),
        state: Box::new(state),
        step: EndingStep::Entry {
            initial: true,
            poll_at: 0,
        },
    };
    simulation
}

#[test]
fn postgame_intro_executes_one_reset_and_preserves_retained_execution_state() {
    for ending_samples in [true, false] {
        for sound_disabled in [true, false] {
            let frames = frames();
            let resources = resources();
            let mut simulation = ending_simulation_with_best(&frames, 12_345);
            let Phase::Ending { state, .. } = &mut simulation.phase else {
                unreachable!()
            };
            state.controls.preferences.digitized_samples_enabled = ending_samples;
            until(
                &mut simulation,
                &frames,
                &resources,
                &mut Vec::new(),
                |s| matches!(&s.phase, Phase::Ending { ending, .. } if ending.can_return()),
            );
            assert_eq!(
                simulation
                    .clock
                    .speaker()
                    .pit()
                    .channel(0)
                    .unwrap()
                    .programmed_count,
                Some(if ending_samples { 19_560 } else { 65_535 })
            );
            let Phase::Ending {
                state,
                ending,
                step,
            } = &mut simulation.phase
            else {
                unreachable!()
            };
            // Test the optional poll/return boundary, after the preceding wait;
            // that wait has its own counter clear, distinct from the reset.
            *step = EndingStep::Poll;
            state.controls.preferences.sound_disabled = sound_disabled;
            state.controls.preferences.display_mode = DisplayMode::Monochrome;
            state.controls.sound_poll_counter = 0x1200;
            state.procedural.counters.phase = 0x1234;
            state.procedural.counters.units = 7;
            state.procedural.priority = 1;
            state.procedural.tone_indices = [14, 2, 3, 4];
            state.procedural.noise.seed_bytes = [1, 2, 3, 4];
            let preferences = state.controls.preferences;
            let procedural = state.procedural.clone();
            let ending_state = ending.snapshot();
            let mut expected_session = SessionState {
                random: ending_state.random,
                ..SessionState::default()
            };
            let expected_choice = expected_session.select_intro();
            let tick = simulation.clock.tick();
            let speaker_control = simulation.clock.speaker().control();
            let log = logged_ending_clock(&mut simulation);
            simulation.receive(tick, 0x39);
            simulation.receive(tick + 100, 0x4b);
            until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
                matches!(s.phase, Phase::Intro(_))
            });
            assert_eq!(simulation.clock.tick(), tick);
            assert_eq!(
                simulation.clock.handlers()[0],
                FarPointer {
                    segment: CS,
                    offset: 0x7532
                }
            );
            let channel = simulation.clock.speaker().pit().channel(0).unwrap();
            assert_eq!(channel.programmed_count, Some(0xffff));
            assert!(
                channel.pending_load,
                "reload even when count already matches"
            );
            assert_eq!(simulation.clock.speaker().control() & 3, 0);
            let after = simulation.state();
            assert_eq!(after.procedural.priority, 255);
            assert_eq!(after.procedural.counters, procedural.counters);
            assert_eq!(after.procedural.tone_indices, procedural.tone_indices);
            assert_eq!(after.procedural.noise, procedural.noise);
            assert_eq!(after.controls.preferences, preferences);
            assert_eq!(after.controls.sound_poll_counter, 0x1200);
            assert_ne!(after.keyboard.held_keys(0).actions & 16, 0);
            assert_eq!(simulation.scans, [(tick + 100, 0x4b)]);
            assert_eq!(simulation.session.best_score, 12_345);
            assert_eq!(simulation.session.random, expected_session.random);
            assert_eq!(simulation.intro_choice, expected_choice);
            assert_eq!(simulation.visible, *frames.return_intro(expected_choice));
            assert_eq!(simulation.palette(), 1);
            let retained = simulation.session.retained_world.as_ref().unwrap();
            assert_eq!(
                format!("{:?}", retained.records),
                format!("{:?}", ending_state.world)
            );
            assert_eq!(retained.order, ending_state.full_world_order);
            let actions = log
                .borrow()
                .iter()
                .filter_map(|(stamp, event)| {
                    if let ClockEvent::Action(action) = event {
                        Some((stamp.tick, *action))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            // IRQ1 acknowledges the return key before the caller's reset.
            // Assert the reset's complete ordered suffix and exactly one call.
            let reset_sites = actions
                .iter()
                .enumerate()
                .filter_map(|(index, (_, action))| {
                    matches!(action, SoundAction::SetHandler { irq: 0, .. }).then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(reset_sites.len(), 1);
            assert_eq!(
                actions[reset_sites[0]..],
                [
                    SoundAction::SetHandler {
                        irq: 0,
                        handler: FarPointer {
                            segment: CS,
                            offset: 0x7532
                        }
                    },
                    SoundAction::Write {
                        port: 0x43,
                        value: 0x36,
                        source: None
                    },
                    SoundAction::Write {
                        port: 0x40,
                        value: 0xff,
                        source: None
                    },
                    SoundAction::Write {
                        port: 0x40,
                        value: 0xff,
                        source: None
                    },
                    SoundAction::Read {
                        port: 0x61,
                        value: speaker_control
                    },
                    SoundAction::Write {
                        port: 0x61,
                        value: speaker_control & 0xfc,
                        source: None
                    },
                ]
                .map(|action| (tick, action))
            );
            let next_game = expected_session.begin_game();
            simulation.receive(tick + 200, 0xb9);
            simulation.receive(tick + 210, 0x39);
            until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
                matches!(s.phase, Phase::Playing(_))
            });
            let Phase::Playing(runtime) = &simulation.phase else {
                unreachable!()
            };
            assert_eq!(runtime.game().snapshot().body, 0);
            assert_eq!(
                runtime.game().snapshot().script_index,
                next_game.script_index
            );
            assert_eq!(runtime.game().snapshot().random, next_game.random);
            assert_eq!(simulation.session.best_score, 12_345);
            assert_eq!(simulation.state().controls.preferences, preferences);
        }
    }
}

#[test]
fn frontend_return_executes_exactly_one_reset_transaction() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let state = post_splash_state(&mut simulation);
    let mut game =
        Gameplay::for_play(frames.graphics(), rules(), simulation.session.begin_game()).unwrap();
    game.request_game_over();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let log = actions.clone();
    // Clone only the explicit bootstrap device/vector context, before input or
    // interrupts have been delivered. Capture the full frontend return path.
    simulation.clock = Clock::new(
        simulation.clock.speaker().clone(),
        simulation.clock.handlers(),
        true,
        Box::new(move |_, event| {
            if let ClockEvent::Action(action) = event {
                log.borrow_mut().push(action);
            }
        }),
    );
    simulation.phase = Phase::Playing(Box::new(
        GameplayRuntime::begin(game, state, CS, 64, &simulation.clock).unwrap(),
    ));
    advance(
        &mut simulation,
        100_000,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(matches!(simulation.phase, Phase::Ending { .. }));
    assert_eq!(
        *actions.borrow(),
        [
            SoundAction::SetHandler {
                irq: 0,
                handler: FarPointer {
                    segment: CS,
                    offset: 0x7532
                }
            },
            SoundAction::Write {
                port: 0x43,
                value: 0x36,
                source: None
            },
            SoundAction::Write {
                port: 0x40,
                value: 0xff,
                source: None
            },
            SoundAction::Write {
                port: 0x40,
                value: 0xff,
                source: None
            },
            SoundAction::Read {
                port: 0x61,
                value: 0x60
            },
            SoundAction::Write {
                port: 0x61,
                value: 0x60,
                source: None
            },
        ]
    );
}

#[test]
fn title_entry_waits_for_zero_mask_before_setup_or_drawing() {
    let frames = frames();
    let resources = resources();
    let mut simulation = ending_simulation(&frames);
    let visible = simulation.visible.clone();
    let before = match &simulation.phase {
        Phase::Ending { ending, .. } => ending.snapshot(),
        _ => unreachable!(),
    };
    simulation.receive(0, 0x39);
    advance(
        &mut simulation,
        10_000,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    let Phase::Ending { ending, step, .. } = &simulation.phase else {
        unreachable!()
    };
    assert!(matches!(step, EndingStep::Entry { initial: true, .. }));
    assert_eq!(format!("{:?}", ending.snapshot()), format!("{before:?}"));
    assert_eq!(simulation.visible, visible);
    simulation.receive(simulation.clock.tick(), 0xb9);
    while simulation.clock.tick() < 11_000 {
        advance(
            &mut simulation,
            11_000,
            &frames,
            &resources,
            &mut Vec::new(),
        );
    }
    assert!(matches!(
        &simulation.phase,
        Phase::Ending {
            step: EndingStep::Wait(_),
            ..
        }
    ));
    assert_eq!(simulation.visible, visible); // First draw still waits for two units.
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.visible != visible
    });
}

#[test]
fn mandatory_animation_skips_function_polls_but_next_title_entry_processes_them() {
    use crate::gameplay::{EndingStage, EndingTitle};
    let frames = frames();
    let resources = resources();
    let mut simulation = ending_simulation(&frames);
    let Phase::Ending { state, .. } = &mut simulation.phase else {
        unreachable!()
    };
    // This check isolates ordinary animation polls from sample key clearing.
    state.controls.preferences.digitized_samples_enabled = false;
    let mut events = Vec::new();
    advance(&mut simulation, 100, &frames, &resources, &mut events);
    assert_eq!(simulation.state().controls.sound_poll_counter, 0);
    // Make both keys after the first entry poll. Holding them does not poll on
    // each mandatory approach, hold or departure; the next entry is different.
    simulation.receive(simulation.clock.tick(), 0x3c);
    simulation.receive(simulation.clock.tick(), 0x3d);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        matches!(&s.phase, Phase::Ending { ending, .. }
            if ending.snapshot().title == EndingTitle::GameOver && ending.phase() == EndingStage::Departure)
    });
    assert_eq!(simulation.state().controls.sound_poll_counter, 0);
    assert_eq!(simulation.display_mode(), DisplayMode::Color);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        matches!(&s.phase, Phase::Ending { ending, .. }
            if ending.snapshot().title == EndingTitle::HighScore && ending.phase() == EndingStage::Departure)
    });
    assert_eq!(simulation.state().controls.sound_poll_counter, 1);
    assert_eq!(simulation.display_mode(), DisplayMode::Monochrome);
    let sites = events
        .iter()
        .filter_map(|(_, e)| match e {
            GameplayEvent::InputPoll { site, .. } => Some(*site),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sites, [0x26f1, 0x2864]);
}

#[test]
fn pending_native_pause_f3_marks_unchanged_frame_for_repaint() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    enter_playing(&mut simulation, &frames, &resources);
    simulation.receive(simulation.clock.tick(), 0x3b);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.state().pause.state == PauseState::AwaitInitialRelease
    });
    let visible = simulation.visible.clone();
    simulation.receive(simulation.clock.tick(), 0x3d);
    for _ in 0..128 {
        let target = simulation.clock.tick() + 1;
        let mut events = Vec::new();
        let dirty = simulation
            .advance(target, &frames, &rules(), &resources, true, &mut events)
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|(_, e)| matches!(e, GameplayEvent::Published { .. }))
        );
        if simulation.display_mode() == DisplayMode::Monochrome {
            assert!(dirty);
            assert_eq!(simulation.visible, visible);
            assert_ne!(simulation.state().pause.state, PauseState::Running);
            return;
        }
    }
    panic!("native pause did not consume F3");
}

#[test]
fn press_mode_f3_stays_monochrome_during_native_pause_until_fresh_press() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    simulation
        .initial
        .as_mut()
        .unwrap()
        .keyboard
        .set_f3_mode(crate::gameplay::F3Mode::Press);
    enter_playing(&mut simulation, &frames, &resources);
    simulation.receive(simulation.clock.tick(), 0x3b);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.state().pause.state == PauseState::AwaitInitialRelease
    });
    simulation.receive(simulation.clock.tick(), 0xbb);
    simulation.receive(simulation.clock.tick(), 0x3d);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.display_mode() == DisplayMode::Monochrome
    });
    let visible = simulation.visible.clone();
    let mut events = Vec::new();
    for _ in 0..10 {
        let tick = simulation.clock.tick() + 100_000;
        advance(&mut simulation, tick, &frames, &resources, &mut events);
        assert_eq!(simulation.display_mode(), DisplayMode::Monochrome);
        assert_ne!(simulation.state().pause.state, PauseState::Running);
        assert_eq!(simulation.visible, visible);
    }
    assert!(
        !events
            .iter()
            .any(|(_, event)| matches!(event, GameplayEvent::Published { .. }))
    );
    simulation.receive(simulation.clock.tick(), 0xbd);
    simulation.receive(simulation.clock.tick(), 0x3d);
    until(&mut simulation, &frames, &resources, &mut events, |s| {
        s.display_mode() == DisplayMode::Color
    });
    assert_ne!(simulation.state().pause.state, PauseState::Running);
}

#[test]
fn press_mode_f3_release_repress_and_pause_are_budget_partition_invariant() {
    let mut outcomes = Vec::new();
    for budget in [64, CHUNK, 100_000] {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        simulation
            .initial
            .as_mut()
            .unwrap()
            .keyboard
            .set_f3_mode(crate::gameplay::F3Mode::Press);
        enter_playing(&mut simulation, &frames, &resources);
        simulation.receive(simulation.clock.tick(), 0x3b);
        until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
            s.state().pause.state == PauseState::AwaitInitialRelease
        });
        let start = simulation.clock.tick();
        for (offset, raw) in [
            (0, 0xbb),
            (2, 0x3d),
            (200_000, 0xbd),
            (200_001, 0x3d),
            (400_000, 0xbd),
        ] {
            simulation.receive(start + offset, raw);
        }
        let end = start + 500_000;
        let mut events = Vec::new();
        while simulation.clock.tick() < end {
            let target = (simulation.clock.tick() + budget).min(end);
            advance(&mut simulation, target, &frames, &resources, &mut events);
        }
        assert_eq!(simulation.display_mode(), DisplayMode::Color);
        assert_ne!(simulation.state().pause.state, PauseState::Running);
        outcomes.push((
            simulation.clock.tick(),
            format!("{:?}", simulation.state()),
            simulation.visible,
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1], outcomes[2]);
}

#[test]
fn palette_zero_survives_gameplay_return_and_title_animation() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    // Retain F3 until the entry poll; active sample teardown clears held keys.
    state.controls.preferences.digitized_samples_enabled = false;
    let mut game = Gameplay::for_play(
        frames.graphics(),
        rules_with_commands(&[0xce, 0xce]),
        simulation.session.begin_game(),
    )
    .unwrap();
    for _ in 0..21 {
        game.advance(frames.graphics()).unwrap();
    }
    assert_eq!(game.palette_family(), 0);
    let retained = game.frame().clone();
    game.request_game_over();
    simulation.visible = retained.clone();
    simulation.palette_family = 0;
    simulation.phase = Phase::Playing(Box::new(
        GameplayRuntime::begin(game, state, CS, 64, &simulation.clock).unwrap(),
    ));
    advance(
        &mut simulation,
        100_000,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(matches!(simulation.phase, Phase::Ending { .. }));
    assert_eq!(simulation.palette(), 0);
    assert_eq!(simulation.visible, retained);
    advance(
        &mut simulation,
        500_000,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert_eq!(simulation.palette(), 0);
    // The original mode reset (F3) changes the palette at the next title entry.
    simulation.receive(simulation.clock.tick(), 0x3d);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.display_mode() == DisplayMode::Monochrome
    });
    assert_eq!(simulation.palette(), 1);
}

fn optional_ending(frames: &StartupFrames) -> Ending {
    let mut session = SessionState::default();
    let game = Gameplay::for_play(frames.graphics(), rules(), session.begin_game()).unwrap();
    let mut ending = Ending::new(frames.graphics(), &game.snapshot(), game.frame()).unwrap();
    while !ending.can_return() {
        ending.advance(frames.graphics()).unwrap();
    }
    ending.prepare_next_step().unwrap();
    assert_eq!(ending.snapshot().world[5].depth, 972);
    ending
}

#[test]
fn optional_poll_precedes_wait_and_draw_and_uses_any_nonzero_mask() {
    fn run(budget: u64, break_at: Option<u64>) -> (String, Vec<(u64, GameplayEvent)>, bool, u64) {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let mut state = post_splash_state(&mut simulation);
        let mut ending = optional_ending(&frames);
        simulation.visible = ending.frame().clone();
        simulation.receive(80_000, 0x4b); // Left: mask4, not Fire.
        if let Some(at) = break_at {
            simulation.receive(at, 0xcb);
        }
        let mut step = EndingStep::Poll;
        let mut events = Vec::new();
        let mut returned = false;
        let mut dirty = false;
        while simulation.clock.tick() < 140_000 {
            let target = (simulation.clock.tick() + budget).min(140_000);
            let (next, repaint, choice) = simulation
                .advance_ending(
                    &mut ending,
                    &mut state,
                    step,
                    target,
                    &EndingResources {
                        frames: &frames,
                        sound: &resources,
                    },
                    &mut events,
                )
                .unwrap();
            step = next;
            dirty |= repaint;
            if choice.is_some() {
                returned = true;
                break;
            }
        }
        // Input arriving during the wait cannot suppress its prepared draw.
        assert!(dirty);
        assert_eq!(ending.snapshot().world[5].depth, 944);
        (
            format!("{:?}", ending.snapshot()),
            events,
            returned,
            simulation.clock.tick(),
        )
    }
    let coarse = run(140_000, None);
    assert!(coarse.2);
    assert_eq!(coarse.3, 131_071);
    assert_eq!(
        coarse.1,
        [
            (
                0,
                GameplayEvent::InputPoll {
                    site: 0x2736,
                    mask: 0
                }
            ),
            (
                131_071,
                GameplayEvent::InputPoll {
                    site: 0x2736,
                    mask: 4
                }
            ),
        ]
    );
    assert_eq!(run(137, None), coarse);
    let tap = run(140_000, Some(100_000));
    assert!(!tap.2);
    assert_eq!(
        tap.1,
        [
            (
                0,
                GameplayEvent::InputPoll {
                    site: 0x2736,
                    mask: 0
                }
            ),
            (
                131_071,
                GameplayEvent::InputPoll {
                    site: 0x2736,
                    mask: 0
                }
            ),
        ]
    );
}

#[test]
fn intertitle_executes_separate_waits_and_release_before_setup() {
    use crate::gameplay::EndingStage;
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    let mut ending = optional_ending(&frames);
    ending.finish_prepared_step(frames.graphics()).unwrap();
    while ending.phase() != EndingStage::Pause {
        ending.advance(frames.graphics()).unwrap();
    }
    state.procedural.counters.units = 3;
    let mut events = Vec::new();
    let (_, _, returned) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            EndingStep::Intertitle {
                second: false,
                wait: Wait::from_call(1, 1),
            },
            200_000,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut events,
        )
        .unwrap();
    assert!(returned.is_none());
    // Wait1 clears the initial excess3. Wait5 then needs three IRQ0 calls;
    // a merged Wait6 would consume the excess and return after two instead.
    assert_eq!(
        events,
        [
            (
                196_606,
                GameplayEvent::InputPoll {
                    site: 0x2864,
                    mask: 0
                }
            ),
            (
                196_606,
                GameplayEvent::InputPoll {
                    site: 0x28fd,
                    mask: 0
                }
            ),
        ]
    );
    assert_eq!(ending.snapshot().world[5].depth, 1000);
}

#[test]
fn following_title_release_gate_defers_best_score_and_preserves_previous_image() {
    use crate::gameplay::{EndingStage, EndingTitle};
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    let mut game = Gameplay::for_play(
        frames.graphics(),
        rules_with_commands(&[0xc3]),
        simulation.session.begin_game(),
    )
    .unwrap();
    for _ in 0..21 {
        game.advance(frames.graphics()).unwrap();
    }
    let score = game.snapshot().hud.score;
    assert!(score > 0);
    let mut ending = Ending::new(frames.graphics(), &game.snapshot(), game.frame()).unwrap();
    while ending.phase() != EndingStage::Departure || ending.snapshot().draw_index != 33 {
        ending.advance(frames.graphics()).unwrap();
    }
    let visible = ending.frame().clone();
    simulation.visible = visible.clone();
    simulation.receive(0, 0x39);
    let (step, dirty, returned) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            EndingStep::Finish,
            10_000,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut Vec::new(),
        )
        .unwrap();
    assert!(!dirty);
    assert!(returned.is_none());
    assert!(matches!(step, EndingStep::Entry { initial: false, .. }));
    assert_eq!(ending.snapshot().title, EndingTitle::GameOver);
    assert_eq!(ending.snapshot().best_score, 0);
    assert_eq!(simulation.visible, visible);
    simulation.receive(simulation.clock.tick(), 0xb9);
    let (_, dirty, returned) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            step,
            11_000,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut Vec::new(),
        )
        .unwrap();
    assert!(!dirty);
    assert!(returned.is_none());
    assert_eq!(ending.snapshot().title, EndingTitle::HighScore);
    assert_eq!(ending.snapshot().best_score, score);
    assert_eq!(simulation.visible, visible);
}

#[test]
fn score_overlay_is_drawn_before_hold_poll_without_consuming_timer_units() {
    use crate::gameplay::{EndingStage, EndingTitle};
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    let mut ending = optional_ending(&frames);
    ending.finish_prepared_step(frames.graphics()).unwrap();
    while ending.snapshot().title != EndingTitle::HighScore
        || ending.phase() != EndingStage::Approach
        || ending.snapshot().draw_index != 34
    {
        ending.advance(frames.graphics()).unwrap();
    }
    ending.prepare_next_step().unwrap();
    assert_eq!(ending.phase(), EndingStage::ScoreOverlay);
    state.procedural.counters.units = 17;
    state.keyboard.apply_scan(0x3b); // F1 returns at the later hold poll.
    let mut events = Vec::new();
    let (_, dirty, returned) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            EndingStep::Poll,
            1,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut events,
        )
        .unwrap();
    assert!(dirty);
    assert!(returned.is_some());
    assert_eq!(state.procedural.counters.units, 17);
    assert_eq!(
        events,
        [(
            0,
            GameplayEvent::InputPoll {
                site: 0x29fa,
                mask: 128
            }
        )]
    );
    assert_eq!(simulation.visible, *ending.frame());
    assert_eq!(ending.phase(), EndingStage::Hold);
}

/// The renderer reaches the final approach or pending score overlay without
/// advancing the device. Live execution then selects the sample at Finish.
fn hold_entry(
    frames: &StartupFrames,
    title: crate::gameplay::EndingTitle,
    score: u32,
    best: u32,
    optional: bool,
) -> Ending {
    use crate::gameplay::{EndingStage, EndingTitle};
    let mut session = SessionState::default();
    let game = Gameplay::for_play(frames.graphics(), rules(), session.begin_game()).unwrap();
    let mut snapshot = game.snapshot();
    snapshot.hud.score = score;
    let mut ending =
        Ending::new_with_best(frames.graphics(), &snapshot, game.frame(), best).unwrap();
    while ending.snapshot().title != title
        || ending.phase() != EndingStage::Approach
        || ending.snapshot().draw_index != 34
        || (optional && ending.snapshot().cycle == 0)
    {
        ending.advance(frames.graphics()).unwrap();
    }
    if title == EndingTitle::HighScore {
        ending.prepare_next_step().unwrap();
        assert_eq!(ending.phase(), EndingStage::ScoreOverlay);
    }
    ending
}

fn logged_ending_clock(simulation: &mut Simulation) -> Rc<RefCell<Vec<(Stamp, ClockEvent)>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let observe = log.clone();
    simulation.clock = Clock::new(
        simulation.clock.speaker().clone(),
        simulation.clock.handlers(),
        true,
        Box::new(move |stamp, event| observe.borrow_mut().push((stamp, event))),
    );
    log
}

#[test]
fn mandatory_hold_samples_select_original_resources_and_preserve_publication() {
    use crate::gameplay::{EndingStage, EndingTitle};
    for (title, resource, count, site) in [
        (EndingTitle::GameOver, Resource::Data9, 10_874, 0x27a4),
        (EndingTitle::HighScore, Resource::Data7, 11_872, 0x29e6),
    ] {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let mut state = post_splash_state(&mut simulation);
        let mut ending = hold_entry(&frames, title, 123_456, 0, false);
        let overlay_pending = ending.has_prepared_draw();
        simulation.visible = ending.frame().clone();
        let log = logged_ending_clock(&mut simulation);
        let mut events = Vec::new();
        let (step, dirty, returned) = simulation
            .advance_ending(
                &mut ending,
                &mut state,
                EndingStep::Finish,
                0,
                &EndingResources {
                    frames: &frames,
                    sound: &resources,
                },
                &mut events,
            )
            .unwrap();
        assert!(matches!(step, EndingStep::Sample { .. }));
        assert_eq!(dirty, overlay_pending);
        assert_eq!(returned, None);
        assert_eq!(simulation.visible, *ending.frame());
        assert_eq!(ending.phase(), EndingStage::Hold);
        assert_eq!(ending.snapshot().hold_tick, 1);
        assert_eq!(state.next_playback, 1);
        let request = SampleRequest {
            resource,
            offset: 0,
            count,
            reverse: false,
        };
        assert_eq!(
            events,
            [(
                0,
                GameplayEvent::Action(crate::sound::GameplayAction::IntroSample { site, request })
            )]
        );
        assert!(log.borrow().iter().any(|(_, event)| *event
            == ClockEvent::Action(SoundAction::SourceBegin {
                playback: 0,
                request
            })));
        assert_eq!(
            simulation.clock.handlers(),
            [
                FarPointer {
                    segment: CS,
                    offset: 0x57a4
                },
                FarPointer {
                    segment: CS,
                    offset: 0x57cc
                }
            ]
        );
    }
}

#[test]
fn ending_sample_guards_distinguish_equal_lower_optional_and_intro_disabled() {
    use crate::gameplay::EndingTitle;
    for (title, score, best, optional, intro, sound, expected) in [
        (
            EndingTitle::HighScore,
            123_456,
            123_456,
            false,
            true,
            false,
            false,
        ),
        (
            EndingTitle::HighScore,
            123_455,
            123_456,
            false,
            true,
            false,
            false,
        ),
        (
            EndingTitle::HighScore,
            123_457,
            123_456,
            false,
            true,
            false,
            true,
        ),
        (EndingTitle::HighScore, 999_999, 0, true, true, false, false),
        (EndingTitle::GameOver, 999_999, 0, true, true, false, false),
        (EndingTitle::GameOver, 0, 0, false, false, false, false),
        (
            EndingTitle::HighScore,
            999_999,
            0,
            false,
            false,
            true,
            false,
        ),
    ] {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let mut state = post_splash_state(&mut simulation);
        state.controls.preferences.digitized_samples_enabled = intro;
        state.controls.preferences.sound_disabled = sound;
        let mut ending = hold_entry(&frames, title, score, best, optional);
        let mut events = Vec::new();
        let (step, _, _) = simulation
            .advance_ending(
                &mut ending,
                &mut state,
                EndingStep::Finish,
                0,
                &EndingResources {
                    frames: &frames,
                    sound: &resources,
                },
                &mut events,
            )
            .unwrap();
        assert_eq!(matches!(step, EndingStep::Sample { .. }), expected);
        assert_eq!(
            events.iter().any(|(_, event)| matches!(
                event,
                GameplayEvent::Action(crate::sound::GameplayAction::IntroSample { .. })
            )),
            expected
        );
        assert!(
            !events
                .iter()
                .any(|(_, event)| matches!(event, GameplayEvent::InputPoll { site: 0x73b2, .. }))
        );
    }
}

#[test]
fn ordinary_sound_bypass_polls_once_and_ignores_mask_without_sample_teardown() {
    use crate::gameplay::EndingTitle;
    for title in [EndingTitle::GameOver, EndingTitle::HighScore] {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let mut state = post_splash_state(&mut simulation);
        state.controls.preferences.sound_disabled = true;
        state.keyboard.apply_scan(0x3b);
        let mut ending = hold_entry(&frames, title, 1, 0, false);
        let log = logged_ending_clock(&mut simulation);
        let mut events = Vec::new();
        let (step, _, returned) = simulation
            .advance_ending(
                &mut ending,
                &mut state,
                EndingStep::Finish,
                0,
                &EndingResources {
                    frames: &frames,
                    sound: &resources,
                },
                &mut events,
            )
            .unwrap();
        assert!(matches!(step, EndingStep::Poll));
        assert_eq!(returned, None);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1],
            (
                0,
                GameplayEvent::InputPoll {
                    site: 0x73b2,
                    mask: 128
                }
            )
        );
        assert!(log.borrow().is_empty());
        assert_eq!(state.next_playback, 0);
        assert!(state.keyboard.held_keys(0).f1);
    }
}

#[test]
fn ending_sample_matches_direct_sample_task_hardware_transactions() {
    use crate::gameplay::EndingTitle;
    let frames = frames();
    let resources = resources();
    let memory = DosSampleMemory::new(&resources.samples, *b"MACH3\0\0\0");
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    state.di = 1; // Inherited ISR cursor parity changes the number of writes.
    let speaker = simulation.clock.speaker().clone();
    let handlers = simulation.clock.handlers();
    let actual = logged_ending_clock(&mut simulation);
    let expected = Rc::new(RefCell::new(Vec::new()));
    let log = expected.clone();
    let mut reference = Clock::new(speaker, handlers, true, move |stamp, event| {
        log.borrow_mut().push((stamp, event))
    });
    let request = SampleRequest {
        resource: Resource::Data9,
        offset: 0,
        count: 10_874,
        reverse: false,
    };
    let mut reference_state = SampleSoundState::default();
    let SampleStart::Playing(mut task) =
        SampleTask::begin(0, request, &mut reference_state, CS, 1, &mut reference).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        task.advance_until(2_000_000, &memory, &mut reference_state, &mut reference)
            .unwrap(),
        crate::timer::SampleProgress::Returned(0)
    ));
    let mut ending = hold_entry(&frames, EndingTitle::GameOver, 0, 0, false);
    let (step, _, _) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            EndingStep::Finish,
            reference.tick(),
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut Vec::new(),
        )
        .unwrap();
    assert!(!matches!(step, EndingStep::Sample { .. }));
    assert_eq!(simulation.clock.tick(), reference.tick());
    assert_eq!(*actual.borrow(), *expected.borrow());
    assert_eq!(state.di, SamplePlayer::return_di());
    assert_eq!(simulation.clock.handlers(), handlers);
}

#[test]
fn ending_sample_input_boundaries_and_teardown_are_budget_partition_invariant() {
    use crate::gameplay::EndingTitle;
    type SampleOutcome = (Vec<(Stamp, ClockEvent)>, String, Vec<(u64, GameplayEvent)>);
    fn run(budget: u64) -> SampleOutcome {
        let frames = frames();
        let resources = resources();
        let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
        let mut state = post_splash_state(&mut simulation);
        state.keyboard.apply_scan(0x4b);
        state.controls.sound_poll_counter = 0xabcd;
        let mut ending = hold_entry(&frames, EndingTitle::GameOver, 0, 0, false);
        let log = logged_ending_clock(&mut simulation);
        // Same-tick input also exercises the one-byte keyboard latch.
        simulation.receive(60, 0x39);
        simulation.receive(60, 0xb9);
        simulation.receive(120, 0x3c);
        let mut step = EndingStep::Finish;
        let mut events = Vec::new();
        while simulation.clock.tick() < 1_400_000 {
            let target = (simulation.clock.tick() + budget).min(1_400_000);
            let (next, _, returned) = simulation
                .advance_ending(
                    &mut ending,
                    &mut state,
                    step,
                    target,
                    &EndingResources {
                        frames: &frames,
                        sound: &resources,
                    },
                    &mut events,
                )
                .unwrap();
            step = next;
            assert_eq!(returned, None);
        }
        assert!(!matches!(step, EndingStep::Sample { .. }));
        assert_eq!(state.next_playback, 1);
        assert!(state.controls.preferences.sound_disabled);
        assert_eq!(state.controls.sound_poll_counter, 0xab00);
        assert_eq!(state.di, SamplePlayer::return_di());
        assert_eq!(state.keyboard.held_keys(0).actions, 0);
        assert!(
            events
                .iter()
                .all(|(_, event)| !matches!(event, GameplayEvent::InputPoll { .. }))
        );
        let output = log.borrow().clone();
        (output, format!("{state:?}"), events)
    }
    assert_eq!(run(1_400_000), run(37));
}

#[test]
fn sample_irq0_priority_services_all_same_tick_scans_before_foreground() {
    use crate::gameplay::EndingTitle;
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    let mut ending = hold_entry(&frames, EndingTitle::GameOver, 0, 0, false);
    let (step, _, _) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            EndingStep::Finish,
            0,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut Vec::new(),
        )
        .unwrap();
    let log = logged_ending_clock(&mut simulation);
    // Re-selecting mode3 after mode0 generates a pending timer request.
    crate::sound::write(&mut simulation.clock, 0x43, 0x30);
    crate::sound::write(&mut simulation.clock, 0x43, 0x36);
    simulation.receive(0, 0x39);
    simulation.receive(0, 0xb9);
    let (step, _, _) = simulation
        .advance_ending(
            &mut ending,
            &mut state,
            step,
            0,
            &EndingResources {
                frames: &frames,
                sound: &resources,
            },
            &mut Vec::new(),
        )
        .unwrap();
    let EndingStep::Sample {
        state: sample_state,
        ..
    } = step
    else {
        panic!()
    };
    assert_eq!(sample_state.last_scan, 0xb9);
    assert!(simulation.scans.is_empty());
    let irqs = log
        .borrow()
        .iter()
        .filter_map(|(_, event)| {
            if let ClockEvent::Interrupt(irq) = event {
                Some(irq.irq)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(irqs, [0, 1, 1]);
    assert_eq!(simulation.clock.advance_to(0).unwrap(), Progress::Reached);
}

#[test]
fn focus_reset_during_sample_discards_future_input_and_preserves_received_return_scan() {
    use crate::gameplay::EndingTitle;
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    let mut state = post_splash_state(&mut simulation);
    state.keyboard.apply_scan(0x4b);
    simulation.phase = Phase::Ending {
        ending: Box::new(hold_entry(&frames, EndingTitle::GameOver, 0, 0, false)),
        state: Box::new(state),
        step: EndingStep::Finish,
    };
    simulation.receive(60, 0x3c);
    simulation.receive(120, 0x39);
    advance(&mut simulation, 60, &frames, &resources, &mut Vec::new());
    let Phase::Ending {
        step: EndingStep::Sample { state, .. },
        ..
    } = &simulation.phase
    else {
        panic!()
    };
    assert_eq!(state.last_scan, 0x3c);
    simulation.clear_input().unwrap();
    assert!(simulation.scans.is_empty());
    assert_eq!(simulation.state().keyboard.held_keys(0).actions, 0);
    let Phase::Ending {
        step: EndingStep::Sample { state, .. },
        ..
    } = &simulation.phase
    else {
        panic!()
    };
    assert_eq!(state.last_scan, 0x3c);
    advance(
        &mut simulation,
        1_400_000,
        &frames,
        &resources,
        &mut Vec::new(),
    );
    assert!(simulation.state().controls.preferences.sound_disabled);
    assert_eq!(simulation.state().di, SamplePlayer::return_di());
}

#[test]
fn settings_boundary_preserves_live_native_pause_and_device_clock() {
    let frames = frames();
    let resources = resources();
    let mut simulation = Simulation::new(&frames, signal(CHUNK as u32)).unwrap();
    enter_playing(&mut simulation, &frames, &resources);
    simulation.receive(simulation.clock.tick(), 0x3b);
    until(&mut simulation, &frames, &resources, &mut Vec::new(), |s| {
        s.state().pause.state == PauseState::AwaitInitialRelease
    });
    let phase = simulation.state().pause.state;
    let visible = simulation.visible.clone();
    let tick = simulation.clock.tick();
    let speaker = simulation.clock.speaker().clone();
    let random = simulation.session.random;
    let start = Instant::now();
    let mut host = HostPause::default();
    let mut menu = MenuPause::default();
    assert!(menu.update(true, &mut host, start));
    simulation.clear_input().unwrap();
    simulation.receive(tick + 100, 0x4d);
    assert!(menu.update(false, &mut host, start + Duration::from_secs(30)));
    simulation.clear_input().unwrap();
    assert!(!host.is_paused());
    assert_eq!(host.now(start + Duration::from_secs(30)), start);
    assert_eq!(simulation.clock.tick(), tick);
    assert_eq!(simulation.clock.speaker(), &speaker);
    assert_eq!(simulation.session.random, random);
    assert_eq!(simulation.state().pause.state, phase);
    assert_eq!(simulation.state().keyboard.held_keys(0).actions, 0);
    assert!(simulation.scans.is_empty());
    assert_eq!(simulation.visible, visible);
}
