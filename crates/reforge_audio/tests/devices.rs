// Copyright (c) 2026 analogued. Project license has not been selected.
//! Authored synthetic cases with expected sequences from Intel's 8254 diagrams.
use reforge_audio::{
    pit::{Error, Mode, Pit},
    speaker::{DriveEdge, PcSpeaker},
};

fn advance(pit: &mut Pit, target: u64) -> Vec<(u64, usize, bool)> {
    let mut edges = Vec::new();
    while pit.tick() < target {
        let step = pit.advance_to(target).unwrap();
        edges.extend(step.edges.iter().map(|e| (e.tick, e.channel, e.high)));
    }
    edges
}

fn configured(control: u8, count: u8) -> Pit {
    let mut pit = Pit::new([true, true, false], [false; 3]);
    pit.write_port(0x43, control).unwrap();
    pit.write_port(0x40, count).unwrap();
    pit
}

#[test]
fn mode_zero_load_clock_terminal_and_post_terminal_wrap() {
    let mut pit = configured(0x10, 3);
    assert_eq!(pit.read_port(0x40), Err(Error::UnknownCount(0)));
    let mut observed = Vec::new();
    for tick in 1..=5 {
        advance(&mut pit, tick);
        let state = pit.channel(0).unwrap();
        observed.push((state.counter.unwrap(), state.output));
    }
    assert_eq!(
        observed,
        [(3, false), (2, false), (1, false), (0, true), (65535, true)]
    );
    // Reloading the SAME value lowers OUT and incurs a new load clock.
    let edges = pit.write_port(0x40, 3).unwrap();
    assert_eq!(
        edges.iter().map(|e| (e.tick, e.high)).collect::<Vec<_>>(),
        [(5, false)]
    );
    assert_eq!(advance(&mut pit, 9), [(9, 0, true)]);
}

#[test]
fn count_one_and_sixty_four_are_not_deduplicated_pwm_levels() {
    let mut pit = configured(0x10, 1);
    assert_eq!(advance(&mut pit, 2), [(2, 0, true)]);
    pit.write_port(0x40, 64).unwrap();
    advance(&mut pit, 62);
    assert!(!pit.channel(0).unwrap().output);
    // A repeated count64 write sixty clocks later prevents terminal at67.
    pit.write_port(0x40, 64).unwrap();
    assert!(advance(&mut pit, 126).is_empty());
    assert_eq!(advance(&mut pit, 127), [(127, 0, true)]);
}

#[test]
fn mode_zero_gate_pauses_and_low_high_first_byte_stops() {
    let mut pit = Pit::new([false, true, false], [true; 3]);
    pit.write_port(0x43, 0x30).unwrap();
    pit.write_port(0x40, 4).unwrap();
    assert!(pit.channel(0).unwrap().pending_high_byte);
    advance(&mut pit, 20);
    assert_eq!(pit.channel(0).unwrap().counter, None);
    pit.write_port(0x40, 0).unwrap();
    advance(&mut pit, 30);
    assert_eq!(pit.channel(0).unwrap().counter, Some(4));
    pit.set_gate(0, true).unwrap();
    advance(&mut pit, 32);
    assert_eq!(pit.channel(0).unwrap().counter, Some(2));
    pit.set_gate(0, false).unwrap();
    advance(&mut pit, 50);
    assert_eq!(pit.channel(0).unwrap().counter, Some(2));
    pit.set_gate(0, true).unwrap();
    assert_eq!(advance(&mut pit, 52), [(52, 0, true)]);
    assert!(!pit.write_port(0x40, 8).unwrap()[0].high);
    advance(&mut pit, 70);
    assert_eq!(pit.channel(0).unwrap().counter, Some(0));
    pit.write_port(0x40, 0).unwrap();
    assert_eq!(advance(&mut pit, 79), [(79, 0, true)]);
}

#[test]
fn one_shot_requires_trigger_ignores_falling_gate_and_uses_new_count_only_on_retrigger() {
    let mut pit = Pit::new([false; 3], [false; 3]);
    pit.write_port(0x43, 0x12).unwrap();
    pit.write_port(0x40, 3).unwrap();
    assert!(advance(&mut pit, 10).is_empty());
    assert!(pit.channel(0).unwrap().output);
    pit.set_gate(0, true).unwrap();
    assert_eq!(advance(&mut pit, 11), [(11, 0, false)]);
    pit.set_gate(0, false).unwrap();
    pit.write_port(0x40, 5).unwrap();
    assert_eq!(pit.channel(0).unwrap().programmed_count, Some(5));
    assert_eq!(pit.channel(0).unwrap().counter, Some(3));
    assert_eq!(advance(&mut pit, 14), [(14, 0, true)]);
    pit.set_gate(0, true).unwrap();
    assert_eq!(advance(&mut pit, 15), [(15, 0, false)]);
    advance(&mut pit, 17);
    pit.set_gate(0, false).unwrap();
    pit.set_gate(0, true).unwrap();
    assert!(advance(&mut pit, 22).is_empty());
    assert_eq!(advance(&mut pit, 23), [(23, 0, true)]);
    advance(&mut pit, 24);
    assert_eq!(pit.channel(0).unwrap().counter, Some(65535));
}

#[test]
fn mode_two_pending_reload_waits_for_period_end() {
    let mut pit = configured(0x14, 3);
    advance(&mut pit, 2);
    pit.write_port(0x40, 5).unwrap();
    assert_eq!(pit.channel(0).unwrap().counter, Some(2));
    assert_eq!(
        advance(&mut pit, 9),
        [(3, 0, false), (4, 0, true), (8, 0, false), (9, 0, true)]
    );
    // Low GATE forces high immediately and rising GATE restarts on next clock.
    advance(&mut pit, 13);
    assert!(pit.set_gate(0, false).unwrap()[0].high);
    advance(&mut pit, 20);
    pit.set_gate(0, true).unwrap();
    assert_eq!(advance(&mut pit, 26), [(25, 0, false), (26, 0, true)]);
}

#[test]
fn mode_three_even_and_8254_odd_counter_sequences() {
    for (n, expected) in [
        (
            4,
            vec![(4, true), (2, true), (4, false), (2, false), (4, true)],
        ),
        (
            5,
            vec![
                (4, true),
                (2, true),
                (0, true),
                (4, false),
                (2, false),
                (4, true),
            ],
        ),
    ] {
        let mut pit = configured(0x16, n);
        let mut observed = Vec::new();
        for tick in 1..=expected.len() as u64 {
            advance(&mut pit, tick);
            let s = pit.channel(0).unwrap();
            observed.push((s.counter.unwrap(), s.output));
        }
        assert_eq!(observed, expected);
    }
    let mut pit = configured(0x16, 4);
    advance(&mut pit, 2);
    pit.write_port(0x40, 6).unwrap();
    // Current HIGH phase finishes using4; new LOW phase lasts6/2 clocks.
    assert_eq!(
        advance(&mut pit, 9),
        [(3, 0, false), (6, 0, true), (9, 0, false)]
    );
}

#[test]
fn zero_means_sixty_five_thousand_and_high_only_zeroes_low_byte() {
    let mut pit = configured(0x10, 0);
    advance(&mut pit, 1);
    assert_eq!(pit.channel(0).unwrap().counter, Some(0));
    assert!(advance(&mut pit, 65536).is_empty());
    assert_eq!(advance(&mut pit, 65537), [(65537, 0, true)]);
    pit.write_port(0x43, 0x20).unwrap();
    pit.write_port(0x40, 1).unwrap();
    advance(&mut pit, 65538);
    assert_eq!(pit.channel(0).unwrap().counter, Some(256));
}

#[test]
fn latches_hold_both_bytes_without_stopping_and_duplicate_latches_are_ignored() {
    let mut pit = Pit::new([true; 3], [false; 3]);
    pit.write_port(0x43, 0x30).unwrap();
    pit.write_port(0x40, 0x34).unwrap();
    pit.write_port(0x40, 0x12).unwrap();
    advance(&mut pit, 1);
    pit.write_port(0x43, 0).unwrap();
    advance(&mut pit, 100);
    pit.write_port(0x43, 0).unwrap();
    assert_eq!(pit.read_port(0x40).unwrap(), 0x34);
    // Unrelated channel programming between reads must not alter the latch.
    pit.write_port(0x43, 0x54).unwrap();
    pit.write_port(0x41, 3).unwrap();
    advance(&mut pit, 300);
    assert_eq!(pit.read_port(0x40).unwrap(), 0x12);
    assert_eq!(pit.read_port(0x40).unwrap(), 0x09); // 0x1234 -299 =0x1109
    assert_eq!(pit.read_port(0x40).unwrap(), 0x11);
}

#[test]
fn unsupported_controls_and_invalid_periodic_count_do_not_change_device_state() {
    let mut pit = configured(0x16, 4);
    advance(&mut pit, 2);
    let before = pit.clone();
    for control in [0xc0, 0x19, 0x18, 0x1a, 0x17] {
        assert!(pit.write_port(0x43, control).is_err());
        assert_eq!(pit, before);
    }
    assert_eq!(pit.write_port(0x40, 1), Err(Error::InvalidPeriodicCount));
    assert_eq!(pit, before);
    assert_eq!(pit.advance_to(1), Err(Error::TimeBackwards));
    assert_eq!(pit, before);
}

#[test]
fn scheduler_cannot_advance_past_an_irq_that_changes_the_next_deadline() {
    let mut pit = configured(0x16, 4);
    assert_eq!(pit.advance_to(100).unwrap().tick, 1); // Load, no decrement.
    assert_eq!(pit.advance_to(100).unwrap().tick, 3); // Falling OUT0.
    let rising = pit.advance_to(100).unwrap();
    assert_eq!((rising.tick, rising.edges[0].high), (5, true));
    // The dispatched handler changes the timer NOW, before a later edge executes.
    pit.write_port(0x43, 0x10).unwrap();
    pit.write_port(0x40, 2).unwrap();
    assert_eq!(advance(&mut pit, 100), [(8, 0, true)]);
    assert_eq!(pit.channel(0).unwrap().mode, Some(Mode::TerminalCount));
}

#[test]
fn speaker_data_bit_is_independent_of_gate_and_keyboard_ack_keeps_phase() {
    let mut speaker = PcSpeaker::new(0, [true; 3]);
    assert!(!speaker.drive());
    let edge = speaker.write_port(0x61, 2).unwrap(); // GATE remains low, OUT2 high.
    assert_eq!(
        edge.drive,
        Some(DriveEdge {
            tick: 0,
            high: true
        })
    );
    speaker.write_port(0x43, 0x96).unwrap();
    speaker.write_port(0x42, 4).unwrap();
    speaker.write_port(0x61, 3).unwrap();
    while speaker.pit().tick() < 3 {
        speaker.advance_to(3).unwrap();
    }
    assert!(!speaker.drive());
    let before = speaker.pit().clone();
    let ack = speaker.write_port(0x61, 0x83).unwrap();
    assert!(ack.pit.is_empty() && ack.drive.is_none());
    let restore = speaker.write_port(0x61, 3).unwrap();
    assert!(restore.pit.is_empty() && restore.drive.is_none());
    assert_eq!(*speaker.pit(), before);
    let disable_gate = speaker.write_port(0x61, 2).unwrap();
    assert_eq!(
        disable_gate.drive,
        Some(DriveEdge {
            tick: 3,
            high: true
        })
    );
    assert_eq!(speaker.read_port(0x61).unwrap(), 2);
    assert!(!speaker.write_port(0x61, 0).unwrap().drive.unwrap().high);
}

#[test]
fn chunk_partitioning_and_speaker_enable_do_not_change_pit_execution() {
    fn run(quantum: u64, control: u8) -> (Pit, Vec<(u64, usize, bool)>) {
        let mut s = PcSpeaker::new(control, [true; 3]);
        s.write_port(0x43, 0x36).unwrap();
        s.write_port(0x40, 60).unwrap();
        s.write_port(0x40, 0).unwrap();
        s.write_port(0x43, 0x96).unwrap();
        s.write_port(0x42, 7).unwrap();
        let mut edges = Vec::new();
        for target in (quantum..=300).step_by(quantum as usize) {
            while s.pit().tick() < target {
                let e = s.advance_to(target).unwrap();
                edges.extend(e.pit.iter().map(|e| (e.tick, e.channel, e.high)));
            }
        }
        (s.pit().clone(), edges)
    }
    assert_eq!(run(1, 1), run(10, 1));
    assert_eq!(run(10, 1), run(100, 1));
    assert_eq!(run(1, 1), run(1, 3)); // Output enable never gates IRQ0 or channel2.
}

#[test]
fn new_latch_between_live_byte_reads_is_explicitly_unsupported_without_resetting_phase() {
    let mut pit = Pit::new([true; 3], [false; 3]);
    pit.write_port(0x43, 0x30).unwrap();
    pit.write_port(0x40, 0x34).unwrap();
    pit.write_port(0x40, 0x12).unwrap();
    advance(&mut pit, 1);
    assert_eq!(pit.read_port(0x40).unwrap(), 0x34);
    let before = pit.clone();
    assert_eq!(pit.write_port(0x43, 0), Err(Error::IncompleteRead(0)));
    assert_eq!(pit, before);
    assert_eq!(pit.read_port(0x40).unwrap(), 0x12);
    pit.write_port(0x43, 0).unwrap();
    assert_eq!(pit.read_port(0x40).unwrap(), 0x34);
    // Existing latch command during a half-read latch remains ignored.
    pit.write_port(0x43, 0).unwrap();
    assert_eq!(pit.read_port(0x40).unwrap(), 0x12);
    assert_eq!(pit.read_port(0x40).unwrap(), 0x34);
}
