// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::timing::{DosReadTiming, ReadPoll};

#[test]
fn submission_has_priority_even_when_deadline_has_expired() {
    let timing = DosReadTiming::new(5, 100, 0x1200);
    assert_eq!(timing.poll(104, false), ReadPoll::Wait);
    assert_eq!(timing.poll(105, false), ReadPoll::TimedOut { value: 5 });
    assert_eq!(timing.poll(106, true), ReadPoll::Submitted { value: 17 });
}

#[test]
fn zero_negative_and_unlimited_timeouts_follow_native_signed_rules() {
    assert_eq!(
        DosReadTiming::new(0, 20, 0).poll(20, false),
        ReadPoll::TimedOut { value: 0 }
    );
    let negative = DosReadTiming::new(0xfffe, 20, 0);
    assert_eq!(negative.deadline(), 18);
    assert_eq!(
        negative.poll(20, false),
        ReadPoll::TimedOut { value: 0xfffe }
    );
    let unlimited = DosReadTiming::new(0xffff, 20, 0);
    assert_eq!(unlimited.deadline(), 19);
    for now in [0, 19, 20, 0x7fffffff, 0x80000000, u32::MAX] {
        assert_eq!(unlimited.poll(now, false), ReadPoll::Wait);
    }
}

#[test]
fn deadline_uses_wrapping_add_but_signed_comparison() {
    let timing = DosReadTiming::new(4, u32::MAX - 1, 0);
    assert_eq!(timing.deadline(), 2);
    assert_eq!(timing.poll(u32::MAX, false), ReadPoll::Wait);
    assert_eq!(timing.poll(2, false), ReadPoll::TimedOut { value: 4 });
    // Crossing the signed boundary does not become modular elapsed arithmetic.
    let timing = DosReadTiming::new(4, 0x7ffffffe, 0);
    assert_eq!(timing.deadline(), 0x80000002);
    assert_eq!(
        timing.poll(0x7ffffffe, false),
        ReadPoll::TimedOut { value: 4 }
    );
}

#[test]
fn bios_midnight_reset_is_not_normalized_into_a_completed_timeout() {
    let timing = DosReadTiming::new(10, 0x1800ad, 0);
    assert_eq!(timing.deadline(), 0x1800b7);
    assert_eq!(timing.poll(1, false), ReadPoll::Wait);
}

#[test]
fn submission_result_uses_only_stale_word_high_byte() {
    for (word, value) in [
        (0, 0),
        (0xff, 0),
        (0x100, 0),
        (0x1ff, 0),
        (0x200, 1),
        (0x1234, 17),
        (0xffff, 254),
    ] {
        let timing = DosReadTiming::new(10, 0, word);
        assert_eq!(timing.counter_word(), word);
        assert_eq!(timing.timeout(), 10);
        assert_eq!(timing.submitted_value(), value);
        assert_eq!(timing.poll(0, true), ReadPoll::Submitted { value });
    }
}

#[test]
fn explicit_observed_results_must_fit_the_submission_path() {
    for value in 0..=254 {
        let word = DosReadTiming::counter_word_for_value(value).unwrap();
        assert_eq!(DosReadTiming::new(0, 0, word).submitted_value(), value);
    }
    assert!(DosReadTiming::counter_word_for_value(255).is_err());
    assert!(DosReadTiming::counter_word_for_value(0xffff).is_err());
}
