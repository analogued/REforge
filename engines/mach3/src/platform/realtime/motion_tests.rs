// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Real simulation continuations and frontend receipt/catch-up policy, with
//! synthetic assets. These do not claim to exercise SDL's outer event loop.

use super::*;

fn attach(sim: &mut Simulation) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mach3-motion-lifecycle-{}-{}.tsv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    sim.motion = Some(MotionPresenter::create(&path).unwrap());
    path
}

fn finish(sim: &mut Simulation, path: std::path::PathBuf) -> String {
    sim.motion.as_mut().unwrap().flush();
    let trace = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    trace
}

fn catch_up(
    sim: &mut Simulation,
    wanted: u64,
    f: &StartupFrames,
    r: &Resources,
    events: &mut Vec<(u64, GameplayEvent)>,
) -> usize {
    let mut budgets = 0;
    while sim.clock.tick() < wanted {
        let budget = wanted.min(sim.clock.tick() + CHUNK);
        advance(sim, budget, f, r, events);
        if let Some(m) = &mut sim.motion {
            m.commit(wanted).unwrap();
        }
        budgets += 1;
    }
    if let Some(m) = &mut sim.motion {
        let _ = m.render(wanted, f.graphics()).unwrap();
        m.presented(wanted, wanted, 0);
    }
    budgets
}

fn reach(
    sim: &mut Simulation,
    f: &StartupFrames,
    r: &Resources,
    events: &mut Vec<(u64, GameplayEvent)>,
    condition: impl Fn(&Simulation) -> bool,
) {
    for _ in 0..20_000 {
        if condition(sim) {
            return;
        }
        catch_up(sim, sim.clock.tick() + 10_000, f, r, events);
    }
    panic!("unreached motion lifecycle condition: {}", sim.status());
}

fn lifecycle(mode: Option<MotionMode>) -> (Outcome, Vec<i16>) {
    let f = frames();
    let r = resources();
    let signal = signal(CHUNK as u32);
    let mut sim = Simulation::new(&f, signal.clone()).unwrap();
    let log = logged_initial_signal_clock(&mut sim, signal.clone());
    enter_playing(&mut sim, &f, &r);
    let path = mode.map(|mode| {
        let path = attach(&mut sim);
        sim.motion.as_mut().unwrap().set_mode(mode);
        path
    });
    let mut events = Vec::new();
    let start = sim.clock.tick();
    sim.receive(start, 0x4d);
    sim.receive(start + 400_000, 0xcd);
    // A real multi-budget recovery: every intervening continuation executes,
    // observations arrive at one host target, only the final scene is presented.
    assert!(catch_up(&mut sim, start + 600_000, &f, &r, &mut events) > 40);
    for end_game in [false, true] {
        sim.receive(sim.clock.tick(), 0x3b);
        reach(&mut sim, &f, &r, &mut events, |s| {
            s.state().pause.state == PauseState::AwaitInitialRelease
        });
        sim.receive(sim.clock.tick(), 0xbb);
        reach(&mut sim, &f, &r, &mut events, |s| {
            s.state().pause.state == PauseState::AwaitPress
        });
        let frozen = sim
            .motion
            .as_ref()
            .and_then(|m| m.render(sim.clock.tick(), f.graphics()).unwrap());
        let tick = sim.clock.tick();
        catch_up(&mut sim, tick + 100_000, &f, &r, &mut events);
        assert!(sim.clock.tick() > tick); // PIT continues during native pause.
        assert_eq!(
            frozen,
            sim.motion
                .as_ref()
                .and_then(|m| m.render(sim.clock.tick(), f.graphics()).unwrap())
        );
        sim.receive(sim.clock.tick(), if end_game { 0x3b } else { 0x39 });
        reach(&mut sim, &f, &r, &mut events, |s| {
            s.state().pause.state == PauseState::AwaitFinalRelease
        });
        sim.receive(sim.clock.tick(), if end_game { 0xbb } else { 0xb9 });
        reach(&mut sim, &f, &r, &mut events, |s| {
            if end_game {
                matches!(s.phase, Phase::Ending { .. })
            } else {
                s.state().pause.state == PauseState::Running
            }
        });
    }
    assert!(
        sim.motion
            .as_ref()
            .is_none_or(|m| m.render(sim.clock.tick(), f.graphics()).unwrap().is_none())
    );
    reach(
        &mut sim,
        &f,
        &r,
        &mut events,
        |s| matches!(&s.phase, Phase::Ending { ending, .. } if ending.can_return()),
    );
    sim.receive(sim.clock.tick(), 0x39);
    reach(&mut sim, &f, &r, &mut events, |s| {
        matches!(s.phase, Phase::Intro(_))
    });
    let at = sim.clock.tick();
    sim.receive(at, 0xb9);
    sim.receive(at + 10, 0x39);
    reach(&mut sim, &f, &r, &mut events, |s| {
        matches!(s.phase, Phase::Playing(_))
    });
    assert!(
        sim.motion
            .as_ref()
            .is_none_or(|m| m.render(sim.clock.tick(), f.graphics()).unwrap().is_none())
    );
    sim.clear_input().unwrap();
    reach(
        &mut sim,
        &f,
        &r,
        &mut events,
        |s| matches!(&s.phase, Phase::Playing(runtime) if runtime.game().snapshot().body >= 2),
    );
    assert!(
        sim.motion
            .as_ref()
            .is_none_or(|m| m.render(sim.clock.tick(), f.graphics()).unwrap().is_some())
    );
    if let Some(path) = path {
        let trace = finish(&mut sim, path);
        for reason in ["native_pause", "ending", "intro", "new_game", "rebase"] {
            assert!(
                trace
                    .lines()
                    .any(|line| line.split('\t').nth(11) == Some(reason)),
                "missing {reason}"
            );
        }
    }
    signal.borrow_mut().advance(sim.clock.tick(), None);
    let pcm = signal
        .borrow_mut()
        .blocks
        .drain(..)
        .flat_map(|b| b.samples)
        .collect();
    let Phase::Playing(runtime) = &sim.phase else {
        unreachable!()
    };
    (
        Outcome {
            tick: sim.clock.tick(),
            state: format!("{:?}", sim.state()),
            world: format!("{:?}", runtime.game().snapshot()),
            session: format!("{:?}", sim.session),
            visible: sim.visible.clone(),
            events,
            generated: signal.borrow().generated,
            transactions: log.borrow().clone(),
        },
        pcm,
    )
}

#[test]
fn native_pause_catchup_ending_and_new_game_preserve_execution_and_pcm() {
    let baseline = lifecycle(None);
    for mode in [MotionMode::Enhanced, MotionMode::Scene] {
        assert_eq!(baseline, lifecycle(Some(mode)));
    }
}

fn partial_pass_transition(mode: MotionMode) {
    let f = frames();
    let r = resources();
    let mut sim = Simulation::new(&f, signal(CHUNK as u32)).unwrap();
    enter_playing(&mut sim, &f, &r);
    let game = Gameplay::from_game_start(
        f.graphics(),
        rules_with_commands(&[0xd6, 0xd5, 0xcb, 0xcc]),
        SessionState::default().begin_game(),
        90,
    )
    .unwrap();
    let state = sim.state().clone();
    sim.phase = Phase::Playing(Box::new(
        GameplayRuntime::begin(game, state, CS, 64, &sim.clock).unwrap(),
    ));
    let path = attach(&mut sim);
    sim.motion.as_mut().unwrap().set_mode(mode);
    let mut events = Vec::new();
    reach(
        &mut sim,
        &f,
        &r,
        &mut events,
        |s| matches!(&s.phase, Phase::Playing(runtime) if runtime.next_transition_deadline().is_some()),
    );
    let Phase::Playing(runtime) = &sim.phase else {
        unreachable!()
    };
    assert_ne!(
        runtime.game().snapshot().render_pass,
        1,
        "fixture must transition during a partial pass"
    );
    assert!(
        sim.motion
            .as_ref()
            .unwrap()
            .render(sim.clock.tick(), f.graphics())
            .unwrap()
            .is_none()
    );
    let body = runtime.game().snapshot().body;
    for _ in 0..5000 {
        let wanted = sim.clock.tick() + 10_000;
        catch_up(&mut sim, wanted, &f, &r, &mut events);
        if events
            .iter()
            .any(|(_, e)| matches!(e, GameplayEvent::Published { body: b, pass: 1 } if *b > body))
        {
            break;
        }
    }
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, GameplayEvent::Published { body: b, pass: 1 } if *b > body))
    );
    assert!(
        sim.motion
            .as_ref()
            .unwrap()
            .render(sim.clock.tick(), f.graphics())
            .unwrap()
            .is_some()
    );
    let trace = finish(&mut sim, path);
    let mut reset = false;
    for line in trace.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] == "boundary" && fields[11] == "transition" {
            reset = true;
        }
        if reset && fields[0] == "observation" {
            assert_eq!(fields[29], "1");
            assert_eq!(fields[11], "first");
            return;
        }
    }
    panic!("no complete first pass recovered after transition");
}

#[test]
fn partial_pass_transition_falls_back_until_a_complete_first_pass() {
    for mode in [MotionMode::Enhanced, MotionMode::Scene] {
        partial_pass_transition(mode);
    }
}
