// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use mach3::{
    sound::{
        ByteOrigin, FarPointer, MemoryByte, Resource, SampleBus, SampleMemory, SamplePlayer,
        SampleRequest, SampleSoundState, SoundAction, StartResult,
    },
    timer::{
        Clock, ClockEvent, Counters, Progress, SampleProgress, SampleStart, SampleTask, Stamp,
        Wait, return_interrupt,
    },
};
use reforge_audio::speaker::PcSpeaker;

const HANDLERS: [FarPointer; 2] = [
    FarPointer {
        segment: 0x1234,
        offset: 0x7532,
    },
    FarPointer {
        segment: 0x1234,
        offset: 0x744a,
    },
];

fn write(bus: &mut impl SampleBus, port: u16, value: u8) {
    bus.emit(SoundAction::Write {
        port,
        value,
        source: None,
    });
}

fn program(bus: &mut impl SampleBus, divisor: u16) {
    write(bus, 0x43, 0x36);
    write(bus, 0x40, divisor as u8);
    write(bus, 0x40, (divisor >> 8) as u8);
}

struct Memory;
impl SampleMemory for Memory {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, mach3::Error> {
        assert_eq!(resource, Resource::DataA);
        assert!(offset < 3);
        Ok(MemoryByte {
            value: [0, 128, 255][usize::from(offset)],
            origin: ByteOrigin::Asset,
        })
    }
}

#[test]
fn sample_suspends_foreground_without_advancing_game_counters_then_restores_real_writes() {
    let mut events = Vec::new();
    let mut clock = Clock::new(
        PcSpeaker::new(0x60, [true; 3]),
        HANDLERS,
        true,
        |stamp, event| events.push((stamp, event)),
    );
    let mut counters = Counters {
        divisor: 0xffff,
        phase: 0x1234,
        units: 6,
    };
    let before = counters;
    program(&mut clock, counters.divisor);
    let environment = clock.sample_environment(0x1234, 0);
    let mut state = SampleSoundState::default();
    let StartResult::Playing(mut player) = SamplePlayer::begin(
        1,
        SampleRequest {
            resource: Resource::DataA,
            offset: 0,
            count: 3,
            reverse: false,
        },
        &mut state,
        environment,
        &mut clock,
    ) else {
        panic!("enabled sample");
    };
    let mut interrupts = 0;
    while !player.completed() {
        match clock.advance_to(1000).unwrap() {
            Progress::Interrupt(irq) => {
                assert_eq!(irq.handler.offset, 0x57a4);
                assert!(!clock.interrupts_enabled());
                player.timer_interrupt(&Memory, &mut clock).unwrap();
                interrupts += 1;
                clock.check().unwrap();
                assert!(clock.interrupts_enabled());
            }
            Progress::Reached => panic!("sample did not complete"),
        }
    }
    assert_eq!(interrupts, 6);
    assert_eq!(clock.tick(), 361);
    assert_eq!(counters, before);
    assert_eq!(player.finish(&mut state, &mut clock).unwrap(), 0);
    clock.check().unwrap();
    assert_eq!(clock.handlers(), HANDLERS);
    assert_eq!(
        clock.speaker().pit().channel(0).unwrap().programmed_count,
        Some(19560)
    );
    assert_eq!(counters.divisor, 0xffff);
    let Progress::Interrupt(irq) = clock.advance_to(30000).unwrap() else {
        panic!("restored timer");
    };
    assert_eq!(irq.handler, HANDLERS[0]);
    assert_eq!(clock.tick(), 19922);
    assert_eq!(counters.interrupt(0, &mut clock), 0xffff);
    return_interrupt(&mut clock);
    assert_eq!((counters.phase, counters.units), (0x1232, 8));
    // The foreground wait drops every accumulated unit, retaining both phases.
    assert!(Wait::from_call(1, 1).poll(&mut counters));
    assert_eq!((counters.phase, counters.units), (0x1232, 0));
    drop(clock);
    let end = events
        .iter()
        .position(|(_, e)| matches!(e, ClockEvent::Action(SoundAction::SourceEnd { .. })))
        .unwrap();
    assert!(events[end + 1..].iter().any(|(_, e)| matches!(e,
        ClockEvent::Device(d) if d.pit.iter().any(|p| p.channel == 2 && p.high))));
}

#[test]
fn waits_are_driven_by_delivered_helpers_and_native_disable_does_not_freeze_time() {
    let mut clock = Clock::new(PcSpeaker::new(3, [true; 3]), HANDLERS, true, |_, _| {});
    let mut counters = Counters::default();
    write(&mut clock, 0x43, 0x36);
    counters.program_divisor(1850, &mut clock);
    let wait = Wait::from_call(1, 1);
    let mut clocks = Vec::new();
    for _ in 0..12 {
        while !wait.poll(&mut counters) {
            let Progress::Interrupt(irq) = clock.advance_to(clock.tick() + 40000).unwrap() else {
                panic!("periodic timer");
            };
            assert_eq!(irq.handler, HANDLERS[0]);
            counters.interrupt(0xffff, &mut clock);
            return_interrupt(&mut clock);
            clock.check().unwrap();
        }
        clocks.push(clock.tick());
    }
    assert_eq!(
        clocks,
        vec![
            33301, 66601, 99901, 131351, 164651, 197951, 229401, 262701, 296001, 329301, 360751,
            394051
        ]
    );
    assert_eq!(clock.speaker().control() & 3, 0);
    assert_eq!(counters.phase, (2 * 1850_u32 * 213) as u16);
}

#[test]
fn immediate_out_rise_is_pending_through_cli_and_repeated_blocked_edges_coalesce() {
    let mut clock = Clock::new(
        PcSpeaker::new(0, [false, true, true]),
        HANDLERS,
        false,
        |_, _| {},
    );
    program(&mut clock, 60); // Mode control immediately raises OUT0.
    assert_eq!(clock.advance_to(181).unwrap(), Progress::Reached);
    clock.emit(SoundAction::InterruptsEnabled(true));
    let Progress::Interrupt(irq) = clock.advance_to(181).unwrap() else {
        panic!("pending edge");
    };
    assert_eq!(irq.irq, 0);
    return_interrupt(&mut clock);
    assert_eq!(clock.advance_to(181).unwrap(), Progress::Reached);
    assert!(matches!(
        clock.advance_to(241).unwrap(),
        Progress::Interrupt(_)
    ));
}

#[test]
fn if_iret_eoi_and_irq_priority_have_distinct_effects() {
    let mut clock = Clock::new(PcSpeaker::new(0, [true; 3]), HANDLERS, false, |_, _| {});
    program(&mut clock, 60);
    clock.keyboard_input(0x39).unwrap();
    assert!(clock.keyboard_input(0xb9).is_err());
    clock.advance_to(61).unwrap();
    clock.emit(SoundAction::InterruptsEnabled(true));
    let Progress::Interrupt(first) = clock.advance_to(61).unwrap() else {
        panic!("IRQ0 priority");
    };
    assert_eq!(first.irq, 0);
    // IRET alone restores IF but leaves the PIC in-service block in place.
    clock.emit(SoundAction::InterruptReturn);
    assert!(clock.interrupts_enabled());
    assert_eq!(clock.advance_to(61).unwrap(), Progress::Reached);
    write(&mut clock, 0x20, 0x20);
    let Progress::Interrupt(second) = clock.advance_to(61).unwrap() else {
        panic!("IRQ1 pending");
    };
    assert_eq!(second.irq, 1);
    assert_eq!(clock.read_port(0x60), 0x39);
    clock.emit(SoundAction::InterruptsEnabled(true));
    // Higher-priority IRQ0 can nest in IRQ1 after STI.
    let Progress::Interrupt(nested) = clock.advance_to(121).unwrap() else {
        panic!("nested IRQ0");
    };
    assert_eq!(nested.irq, 0);
    return_interrupt(&mut clock);
    assert!(clock.interrupts_enabled());
    return_interrupt(&mut clock);
    assert!(clock.interrupts_enabled());
    clock.check().unwrap();
}

#[test]
fn sample_keyboard_handler_runs_on_same_clock_and_return_disables_sound() {
    let mut clock = Clock::new(PcSpeaker::new(0x60, [true; 3]), HANDLERS, true, |_, _| {});
    let environment = clock.sample_environment(0x1234, 0);
    let mut state = SampleSoundState::default();
    let StartResult::Playing(mut player) = SamplePlayer::begin(
        1,
        SampleRequest {
            resource: Resource::DataA,
            offset: 0,
            count: 1,
            reverse: false,
        },
        &mut state,
        environment,
        &mut clock,
    ) else {
        panic!("enabled sample");
    };
    clock.advance_to(30).unwrap();
    clock.keyboard_input(0xbc).unwrap();
    let Progress::Interrupt(irq) = clock.advance_to(30).unwrap() else {
        panic!("keyboard");
    };
    assert_eq!(irq.handler.offset, 0x57cc);
    player.keyboard_interrupt(&mut state, &mut clock).unwrap();
    for _ in 0..2 {
        let Progress::Interrupt(irq) = clock.advance_to(200).unwrap() else {
            panic!("sample IRQ0");
        };
        assert_eq!(irq.irq, 0);
        player.timer_interrupt(&Memory, &mut clock).unwrap();
    }
    assert_eq!(player.finish(&mut state, &mut clock).unwrap(), 0xbc);
    assert!(state.disabled);
    assert_eq!(clock.handlers(), HANDLERS);
    clock.check().unwrap();
}

type Execution = (Counters, PcSpeaker, Vec<(Stamp, ClockEvent)>);

fn run_partitioned(boundaries: &[u64]) -> Execution {
    let mut events = Vec::new();
    let mut clock = Clock::new(
        PcSpeaker::new(0, [true; 3]),
        HANDLERS,
        true,
        |stamp, event| events.push((stamp, event)),
    );
    let mut counters = Counters::default();
    write(&mut clock, 0x43, 0x36);
    counters.program_divisor(100, &mut clock);
    let mut delivered = 0;
    for &target in boundaries {
        while let Progress::Interrupt(_) = clock.advance_to(target).unwrap() {
            counters.interrupt(0, &mut clock);
            delivered += 1;
            if delivered == 2 {
                // Reprogramming during an ISR changes the next actual deadline.
                write(&mut clock, 0x43, 0x36);
                counters.program_divisor(60, &mut clock);
            }
            return_interrupt(&mut clock);
        }
    }
    let speaker = clock.speaker().clone();
    clock.check().unwrap();
    drop(clock);
    // Empty steps are caller observation boundaries, not device operations.
    events.retain(
        |(_, e)| !matches!(e, ClockEvent::Device(d) if d.pit.is_empty() && d.drive.is_none()),
    );
    (counters, speaker, events)
}

#[test]
fn changing_advancement_chunks_or_observation_preserves_execution() {
    let (whole_counter, whole_device, whole_events) = run_partitioned(&[1000]);
    let (chunk_counter, chunk_device, chunk_events) =
        run_partitioned(&[13, 60, 201, 202, 227, 900, 1000]);
    assert_eq!(whole_counter, chunk_counter);
    assert_eq!(whole_device, chunk_device);
    // Observation ordering numbers include empty steps, so compare event times
    // and content while keeping their order intact.
    let normalize = |events: Vec<(Stamp, ClockEvent)>| {
        events
            .into_iter()
            .map(|(stamp, event)| (stamp.tick, event))
            .collect::<Vec<_>>()
    };
    assert_eq!(normalize(whole_events), normalize(chunk_events));
}

#[test]
fn invalid_execution_remains_failed_and_backwards_time_is_rejected() {
    let mut clock = Clock::new(PcSpeaker::new(0, [true; 3]), HANDLERS, true, |_, _| {});
    clock.advance_to(10).unwrap();
    assert!(clock.advance_to(9).is_err());
    clock.emit(SoundAction::InterruptReturn);
    assert!(clock.check().is_err());
    assert!(clock.advance_to(10).is_err());
}

fn run_sample_task(chunks: &[u64], observe: bool) -> (PcSpeaker, Vec<(u64, ClockEvent)>) {
    let mut events = Vec::new();
    let mut clock = Clock::new(
        PcSpeaker::new(0x60, [true; 3]),
        HANDLERS,
        true,
        |stamp, event| {
            if observe {
                events.push((stamp.tick, event));
            }
        },
    );
    let mut state = SampleSoundState::default();
    let SampleStart::Playing(mut task) = SampleTask::begin(
        1,
        SampleRequest {
            resource: Resource::DataA,
            offset: 0,
            count: 3,
            reverse: false,
        },
        &mut state,
        0x1234,
        0,
        &mut clock,
    )
    .unwrap() else {
        panic!("sample start");
    };
    assert_eq!(
        task.advance_until(30, &Memory, &mut state, &mut clock)
            .unwrap(),
        SampleProgress::Pending
    );
    clock.keyboard_input(0xb9).unwrap();
    for &target in chunks {
        assert_eq!(
            task.advance_until(target, &Memory, &mut state, &mut clock)
                .unwrap(),
            SampleProgress::Pending
        );
    }
    assert_eq!(
        task.advance_until(1000, &Memory, &mut state, &mut clock)
            .unwrap(),
        SampleProgress::Returned(0xb9)
    );
    assert_eq!(clock.tick(), 361); // Do not spend remaining budget before foreground resumes.
    assert_eq!(clock.handlers(), HANDLERS);
    assert_eq!(state.last_scan, 0xb9);
    assert!(state.keyboard_words.iter().all(|word| *word == 0));
    assert!(
        task.advance_until(1000, &Memory, &mut state, &mut clock)
            .is_err()
    );
    let speaker = clock.speaker().clone();
    drop(clock);
    events.retain(
        |(_, e)| !matches!(e, ClockEvent::Device(d) if d.pit.is_empty() && d.drive.is_none()),
    );
    (speaker, events)
}

#[test]
fn foreground_sample_continuation_is_invariant_under_chunking_and_disabled_observation() {
    let (whole_device, whole_events) = run_sample_task(&[], true);
    let (chunk_device, chunk_events) = run_sample_task(&[30, 31, 59, 61, 140, 300, 360], true);
    let (unobserved_device, unobserved_events) = run_sample_task(&[], false);
    assert_eq!(whole_device, chunk_device);
    assert_eq!(whole_device, unobserved_device);
    assert_eq!(whole_events, chunk_events);
    assert!(unobserved_events.is_empty());
}

#[test]
fn already_disabled_sample_requires_ordinary_poll_and_changes_no_device_state() {
    let mut clock = Clock::new(PcSpeaker::new(0x60, [true; 3]), HANDLERS, true, |_, _| {});
    let before = clock.speaker().clone();
    let mut state = SampleSoundState {
        disabled: true,
        last_scan: 7,
        ..SampleSoundState::default()
    };
    assert!(matches!(
        SampleTask::begin(
            1,
            SampleRequest {
                resource: Resource::DataA,
                offset: 0,
                count: 3,
                reverse: false
            },
            &mut state,
            0x1234,
            0,
            &mut clock
        )
        .unwrap(),
        SampleStart::RegularKeyboardPoll
    ));
    assert_eq!(clock.speaker(), &before);
    assert_eq!(clock.handlers(), HANDLERS);
    assert_eq!(state.last_scan, 7);
}

#[test]
fn failed_source_read_cannot_abandon_an_entered_handler_and_continue_silently() {
    struct Missing;
    impl SampleMemory for Missing {
        fn read(&self, _: Resource, _: u16) -> Result<MemoryByte, mach3::Error> {
            Err(mach3::Error::InvalidData("missing boundary byte".into()))
        }
    }
    let mut clock = Clock::new(PcSpeaker::new(0, [true; 3]), HANDLERS, true, |_, _| {});
    let mut state = SampleSoundState::default();
    let SampleStart::Playing(mut task) = SampleTask::begin(
        1,
        SampleRequest {
            resource: Resource::DataA,
            offset: 0,
            count: 1,
            reverse: false,
        },
        &mut state,
        0x1234,
        0,
        &mut clock,
    )
    .unwrap() else {
        panic!("sample start");
    };
    let error = task
        .advance_until(1000, &Missing, &mut state, &mut clock)
        .unwrap_err();
    assert_eq!(error.to_string(), "missing boundary byte");
    assert_eq!(clock.tick(), 61);
    assert!(clock.check().is_err());
    assert!(clock.advance_to(1000).is_err());
    assert_eq!(
        task.advance_until(1000, &Memory, &mut state, &mut clock)
            .unwrap_err()
            .to_string(),
        error.to_string()
    );
    assert_eq!(clock.tick(), 61);
}

#[test]
fn pending_keyboard_is_delivered_before_completion_teardown_at_same_boundary() {
    for count in [0, 1] {
        let mut clock = Clock::new(
            PcSpeaker::new(0, [false, true, true]),
            HANDLERS,
            true,
            |_, _| {},
        );
        clock.keyboard_input(0x39).unwrap();
        let mut state = SampleSoundState::default();
        let SampleStart::Playing(mut task) = SampleTask::begin(
            1,
            SampleRequest {
                resource: Resource::DataA,
                offset: 0,
                count,
                reverse: false,
            },
            &mut state,
            0x1234,
            1,
            &mut clock,
        )
        .unwrap() else {
            panic!("sample start");
        };
        // OUT0's immediate rise requests IRQ0, whose odd DI completes one byte.
        // IRQ1 remains deliverable afterward, before foreground observes CX.
        assert_eq!(
            task.advance_until(0, &Memory, &mut state, &mut clock)
                .unwrap(),
            SampleProgress::Returned(0x39)
        );
        assert_eq!(state.last_scan, 0x39);
        assert_eq!(clock.handlers(), HANDLERS);
        assert_eq!(clock.advance_to(0).unwrap(), Progress::Reached);
        clock.check().unwrap();
    }
}
