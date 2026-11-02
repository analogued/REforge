// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use mach3::{
    sound::{
        Effect, Handler, NoiseState, ProceduralProgram, ProceduralState, Ramp, SampleBus,
        SoundAction, Tone,
    },
    timer::{Clock, ClockEvent, Progress},
};
use reforge_audio::speaker::PcSpeaker;

#[derive(Default)]
struct Bus {
    control: u8,
    actions: Vec<SoundAction>,
}
impl SampleBus for Bus {
    fn read_port(&mut self, port: u16) -> u8 {
        assert_eq!(port, 0x61);
        self.control
    }
    fn emit(&mut self, action: SoundAction) {
        if let SoundAction::Write {
            port: 0x61, value, ..
        } = action
        {
            self.control = value;
        }
        self.actions.push(action);
    }
}
fn writes(bus: &Bus) -> Vec<(u16, u8)> {
    bus.actions
        .iter()
        .filter_map(|a| match a {
            SoundAction::Write { port, value, .. } => Some((*port, *value)),
            _ => None,
        })
        .collect()
}
fn image() -> Vec<u8> {
    let mut b = vec![0; 0x78c6];
    for (t, (instruction, base, count)) in [
        (0x7541, 0x7583, 17),
        (0x762a, 0x766c, 47),
        (0x774d, 0x778c, 14),
        (0x7833, 0x7872, 42),
    ]
    .into_iter()
    .enumerate()
    {
        b[instruction..instruction + 6].copy_from_slice(&[
            0xd1,
            0xe0,
            0x8d,
            0x1e,
            base as u8,
            (base >> 8) as u8,
        ]);
        for index in 0..count {
            let value = 1000 + 100 * t as u16 + index as u16;
            b[base + index * 2..base + index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    b
}
fn program() -> ProceduralProgram {
    ProceduralProgram::from_expanded(&image()).unwrap()
}

#[test]
fn table_loader_reads_external_words_and_rejects_wrong_addressing_or_truncation() {
    let b = image();
    let p = program();
    for (tone, last) in Tone::ALL.into_iter().zip([16, 46, 13, 41]) {
        assert_eq!(p.divisor(tone, 0).unwrap(), 1000 + 100 * tone as u16);
        assert_eq!(
            p.divisor(tone, last).unwrap(),
            1000 + 100 * tone as u16 + last
        );
        assert!(p.divisor(tone, last + 1).is_err());
    }
    for end in [0, 0x7547, 0x75a4, 0x76c9, 0x77a7, 0x78c5] {
        assert!(ProceduralProgram::from_expanded(&b[..end]).is_err());
    }
    let mut wrong = b;
    wrong[0x774f] = 0;
    assert!(ProceduralProgram::from_expanded(&wrong).is_err());
}

#[test]
fn priority_uses_unsigned_comparison_equal_requests_and_unconditional_reset() {
    let effects = [
        (Effect::Reset, 255),
        (Effect::Tone(Tone::First), 1),
        (Effect::Tone(Tone::Second), 1),
        (Effect::Tone(Tone::Third), 5),
        (Effect::Tone(Tone::Fourth), 4),
        (Effect::Noise { divisor: 1850 }, 6),
        (Effect::Ramp(Ramp::First), 0),
        (Effect::Ramp(Ramp::Second), 2),
        (Effect::Ramp(Ramp::Third), 3),
    ];
    for current in [0, 1, 2, 3, 4, 5, 6, 255, 0xffff] {
        for (effect, priority) in effects {
            let mut state = ProceduralState {
                priority: current,
                ..ProceduralState::default()
            };
            state.noise.seed_bytes = [1, 2, 3, 4];
            state.noise.ramp_counter = 7;
            let before = state.clone();
            let mut bus = Bus::default();
            let accepted = state.request(effect, 0x1234, 0, &mut bus);
            assert_eq!(accepted, priority == 255 || current >= priority);
            if accepted {
                assert_eq!(state.priority, priority);
            } else {
                assert_eq!(state, before);
                assert!(bus.actions.is_empty());
            }
        }
    }
}

#[test]
fn setups_preserve_persistent_state_and_have_original_vector_write_order() {
    let mut state = ProceduralState::default();
    state.noise.seed_bytes = [1, 2, 3, 4];
    state.noise.ramp_counter = 3;
    state.counters.phase = 123;
    state.counters.units = 6;
    state.tone_indices = [2, 3, 4, 5];
    let mut bus = Bus {
        control: 0x60,
        ..Bus::default()
    };
    state.request(Effect::Reset, 0x1234, 0, &mut bus);
    assert!(matches!(
        bus.actions[0],
        SoundAction::SetHandler { irq: 0, .. }
    ));
    assert_eq!(
        writes(&bus),
        [(0x43, 0x36), (0x40, 0xff), (0x40, 0xff), (0x61, 0x60)]
    );
    assert_eq!(state.tone_indices, [2, 3, 4, 5]);
    bus.actions.clear();
    state.request(Effect::Tone(Tone::Fourth), 0x1234, 0, &mut bus);
    assert!(matches!(
        bus.actions[3],
        SoundAction::SetHandler { irq: 0, .. }
    ));
    assert_eq!(state.tone_indices, [2, 3, 4, 1]);
    assert_eq!(
        writes(&bus),
        [
            (0x43, 0x36),
            (0x40, 0),
            (0x40, 0x26),
            (0x43, 0xb6),
            (0x42, 0),
            (0x42, 2),
            (0x61, 0x63)
        ]
    );
    state.priority = 255;
    bus.actions.clear();
    state.request(Effect::Ramp(Ramp::Third), 0x1234, 0, &mut bus);
    assert!(matches!(
        bus.actions[0],
        SoundAction::SetHandler { irq: 0, .. }
    ));
    assert_eq!(state.tone_indices, [2, 3, 4, 0]);
    assert_eq!(
        (
            state.noise.current,
            state.noise.limit,
            state.noise.step,
            state.noise.group
        ),
        (0x4a0, 0x568, 4, 3)
    );
    assert_eq!(state.noise.cadence, 15);
    assert_eq!(state.noise.seed_bytes, [1, 2, 3, 4]);
    assert_eq!(state.noise.ramp_counter, 3);
    assert_eq!((state.counters.phase, state.counters.units), (123, 6));
}

#[test]
fn tone_terminal_entry_repeats_without_clearing_index_or_restoring_timer() {
    for (tone, last) in Tone::ALL.into_iter().zip([16, 46, 13, 41]) {
        let mut state = ProceduralState::default();
        let mut bus = Bus::default();
        state.request(Effect::Tone(tone), 0, 0, &mut bus);
        let timer = state.counters.divisor;
        for index in 1..=last + 1 {
            bus.actions.clear();
            state
                .timer_interrupt(Handler::Tone(tone), &program(), 0, &mut bus)
                .unwrap();
            let expected = 1000 + 100 * tone as u16 + index.min(last);
            assert_eq!(
                &writes(&bus)[..3],
                [
                    (0x43, 0xb6),
                    (0x42, expected as u8),
                    (0x42, (expected >> 8) as u8)
                ]
            );
        }
        assert_eq!(state.tone_indices[tone as usize], last);
        assert_eq!(state.priority, 255);
        assert_eq!(state.counters.divisor, timer);
        assert_eq!(
            bus.control & 3,
            if matches!(tone, Tone::First | Tone::Second) {
                0
            } else {
                3
            }
        );
    }
}

#[test]
fn noise_uses_overlapping_words_and_distinct_masked_and_unmasked_generators() {
    for (effect, handler, bytes) in [
        (
            Effect::Noise { divisor: 1850 },
            Handler::Noise,
            [0x88, 0x06, 0xa8, 0x48],
        ),
        (
            Effect::Ramp(Ramp::First),
            Handler::Ramp,
            [0x88, 0x46, 0x88, 0x68],
        ),
    ] {
        let mut state = ProceduralState::default();
        let mut bus = Bus::default();
        state.request(effect, 0, 0, &mut bus);
        bus.actions.clear();
        state
            .timer_interrupt(handler, &program(), 0, &mut bus)
            .unwrap();
        assert_eq!(state.noise.seed_bytes, bytes);
        assert_eq!(state.noise.cadence, 0);
        assert_eq!(&writes(&bus)[..3], [(0x43, 0xb4), (0x42, 48), (0x42, 0)]);
        assert_eq!(bus.control & 3, 3);
        for _ in 0..15 {
            bus.actions.clear();
            state
                .timer_interrupt(handler, &program(), 0, &mut bus)
                .unwrap();
        }
        assert_eq!(state.noise.cadence, 15);
        bus.actions.clear();
        state
            .timer_interrupt(handler, &program(), 0, &mut bus)
            .unwrap();
        assert_eq!(state.noise.cadence, 0);
        assert_eq!(&writes(&bus)[..3], [(0x43, 0xb4), (0x42, 48), (0x42, 0)]);
    }
}

#[test]
fn ramp_limit_frees_priority_but_handler_keeps_running_without_new_pit0_writes() {
    let mut state = ProceduralState::default();
    let mut bus = Bus::default();
    state.request(Effect::Ramp(Ramp::First), 0, 0, &mut bus);
    state.noise.current = 0x6f0;
    state.noise.ramp_counter = 3;
    let divisor = state.counters.divisor;
    bus.actions.clear();
    state
        .timer_interrupt(Handler::Ramp, &program(), 0, &mut bus)
        .unwrap();
    assert_eq!(state.noise.current, 0x6f8);
    assert_eq!(state.priority, 255);
    assert_eq!(state.handler, Some(Handler::Ramp));
    assert_eq!(state.counters.divisor, divisor);
    assert!(writes(&bus).iter().all(|(port, _)| *port != 0x40));
    for _ in 0..4 {
        state
            .timer_interrupt(Handler::Ramp, &program(), 0, &mut bus)
            .unwrap();
    }
    assert_eq!(state.noise.current, 0x700);
    assert_eq!(state.counters.divisor, divisor);
    // Signed comparison after word wrapping can enable divisor programming again.
    state.noise.current = 0x7ffc;
    state.noise.ramp_counter = 3;
    bus.actions.clear();
    state
        .timer_interrupt(Handler::Ramp, &program(), 0, &mut bus)
        .unwrap();
    assert_eq!(state.counters.divisor, 0x8004);
    assert!(writes(&bus).contains(&(0x40, 0x80)));
}

#[test]
fn native_disable_advances_counters_but_freezes_sound_state_and_clears_speaker() {
    let mut state = ProceduralState::default();
    let mut bus = Bus::default();
    state.request(Effect::Noise { divisor: 40000 }, 0, 0, &mut bus);
    let noise = state.noise;
    bus.actions.clear();
    state
        .timer_interrupt(Handler::Noise, &program(), 0xffff, &mut bus)
        .unwrap();
    assert_eq!(state.noise, noise);
    assert_eq!(state.counters.units, 1);
    assert_eq!(writes(&bus), [(0x61, 0), (0x20, 0x20)]);
    assert!(matches!(
        bus.actions.last(),
        Some(SoundAction::InterruptReturn)
    ));
    // Other nonzero words disable setup's speaker, but do not take helper's JE.
    state.priority = 255;
    state.request(Effect::Noise { divisor: 1850 }, 0, 1, &mut bus);
    assert_eq!(bus.control & 3, 0);
    state
        .timer_interrupt(Handler::Noise, &program(), 1, &mut bus)
        .unwrap();
    assert_eq!(state.noise.cadence, 0);
    assert_eq!(bus.control & 1, 1);
}

#[test]
fn early_setup_writes_are_visible_to_intervening_previous_handler_irq() {
    let mut state = ProceduralState::default();
    let mut bus = Bus::default();
    state.request(Effect::Noise { divisor: 1850 }, 0, 0, &mut bus);
    for _ in 0..3 {
        state
            .timer_interrupt(Handler::Noise, &program(), 0, &mut bus)
            .unwrap();
    }
    state.noise.cadence = 7;
    let prepared = state.prepare(Effect::Noise { divisor: 1900 }).unwrap();
    assert_eq!(state.noise.cadence, 15);
    assert_eq!(state.counters.divisor, 1850);
    state
        .timer_interrupt(Handler::Noise, &program(), 0, &mut bus)
        .unwrap();
    assert_eq!(state.noise.cadence, 0);
    prepared.finish(&mut state, 0, 0, &mut bus);
    assert_eq!(state.noise.cadence, 0); // Finish must not reapply the early reset.
    assert_eq!(state.counters.divisor, 1900);
}

fn device_run(chunks: &[u64]) -> (ProceduralState, PcSpeaker, Vec<(u64, SoundAction)>) {
    let mut actions = Vec::new();
    let mut clock = Clock::new(
        PcSpeaker::new(0, [true; 3]),
        [Default::default(); 2],
        true,
        |stamp, event| {
            if let ClockEvent::Action(a) = event {
                actions.push((stamp.tick, a));
            }
        },
    );
    let mut state = ProceduralState::default();
    state.request(Effect::Ramp(Ramp::First), 0x1234, 0, &mut clock);
    for &target in chunks {
        while let Progress::Interrupt(irq) = clock.advance_to(target).unwrap() {
            state
                .dispatch(irq, &program(), 0x1234, 0, &mut clock)
                .unwrap();
        }
    }
    let device = clock.speaker().clone();
    drop(clock);
    (state, device, actions)
}

#[test]
fn device_dispatch_and_in_handler_reprogramming_are_chunk_invariant() {
    let whole = device_run(&[20000]);
    assert_eq!(whole, device_run(&[10, 932, 2800, 5000, 7001, 20000]));
    assert!(whole.0.counters.divisor > 0x3a4);
    assert!(whole.2.iter().any(|(_, a)| matches!(
        a,
        SoundAction::Write {
            port: 0x43,
            value: 0xb4,
            ..
        }
    )));
}

#[test]
fn unknown_dispatch_or_invalid_tone_state_stops_the_entered_clock() {
    let mut clock = Clock::new(
        PcSpeaker::new(0, [true; 3]),
        [Default::default(); 2],
        true,
        |_, _| {},
    );
    let mut state = ProceduralState::default();
    state.request(Effect::Tone(Tone::Third), 0x1234, 0, &mut clock);
    let Progress::Interrupt(irq) = clock.advance_to(20000).unwrap() else {
        panic!("timer IRQ");
    };
    state.tone_indices[2] = 0xffff;
    assert!(
        state
            .dispatch(irq, &program(), 0x1234, 0, &mut clock)
            .is_err()
    );
    assert!(clock.check().is_err());
    assert!(clock.advance_to(20000).is_err());
}

#[test]
fn reset_preserves_existing_tone_cursor_and_resumes_its_terminal_writes() {
    let mut state = ProceduralState::default();
    let mut bus = Bus::default();
    state.tone_indices[0] = 16;
    state.noise = NoiseState {
        seed_bytes: [1, 2, 3, 4],
        ..NoiseState::default()
    };
    state.request(Effect::Reset, 0, 0, &mut bus);
    bus.actions.clear();
    state
        .timer_interrupt(Handler::Tone(Tone::First), &program(), 0, &mut bus)
        .unwrap();
    assert_eq!(state.tone_indices[0], 16);
    assert_eq!(state.noise.seed_bytes, [1, 2, 3, 4]);
    assert_eq!(&writes(&bus)[..3], [(0x43, 0xb6), (0x42, 0xf8), (0x42, 3)]);
}
