// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{
    VarAddr,
    image::StoryImage,
    input::Key,
    random::{DosRandom, RandomState},
    services::{HostReply, HostRequest, SideMap},
    session::{Session, SessionStep},
    text::TextCodec,
};

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn executable() -> Vec<u8> {
    let mut exe = vec![0; 32 + 0x6cb];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    exe[32 + 0x7f..32 + 0xff].fill(4);
    for byte in b'a'..=b'z' {
        exe[32 + 0x7f + usize::from(byte)] = 2;
    }
    exe[32 + 0xff..32 + 0x103].copy_from_slice(b"KEY\0");
    exe[32 + 0x6c7..32 + 0x6cb].copy_from_slice(&123u32.to_le_bytes());
    exe
}
fn image(program: &[u16]) -> StoryImage {
    let mut bytes = vec![0; 7 * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
        (0xdc, 0xffff),
        (0xde, 0xffff),
        (0xe8, 0x40),
        (0xea, 0x60),
        (0xe2, 6),
        (0xe4, 7),
        (0xe6, 5),
        (0x102, 7),
        (0x10c, 0x120),
        (0xee, 0x80),
        (0x10e, 0x140),
        (0xe0, 0x200),
        (0x112, 0x8000),
        (0xda, 42),
        (0xce, 0xffff),
    ] {
        put(&mut bytes, offset, value);
    }
    for (offset, value) in [
        (4, 1),
        (14, 3),
        (12, 4),
        (8, 5),
        (10, 6),
        (16, 3),
        (26, 4),
        (24, 5),
        (20, 6),
        (22, 7),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    for (i, word) in program.iter().enumerate() {
        put(&mut bytes, 512 + 0x40 + i * 2, *word);
    }
    // Authored bootstrap tokens: heading, then pause/clear.
    bytes[5 * 512 + 6..5 * 512 + 14].copy_from_slice(b"heading\0");
    bytes[5 * 512 + 36..5 * 512 + 38].copy_from_slice(b"\x0c\0");
    for (i, word) in [0, 2].into_iter().enumerate() {
        put(&mut bytes, 4 * 512 + i * 2, word);
    }
    for (i, word) in [2, 32].into_iter().enumerate() {
        put(&mut bytes, 6 * 512 + i * 2, word);
    }
    StoryImage::from_dos_obj(bytes).unwrap()
}
fn session(program: &[u16]) -> Session {
    let exe = executable();
    Session::new(
        image(program),
        TextCodec::from_dos_executable(&exe).unwrap(),
        DosRandom::from_dos_executable(&exe).unwrap(),
    )
    .unwrap()
}
fn row(session: &Session, row: usize) -> String {
    session.screen().dos_cells()[row * 160 + 2..row * 160 + 156]
        .iter()
        .step_by(2)
        .map(|&byte| char::from(byte))
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn raw_generator_agrees_with_independent_wide_product_fold() {
    for adjustment in [0u32, 1, 123, 0x80000000, u32::MAX] {
        for initial in [0u32, 1, 0x8000, 0xffff, 0x80000000, u32::MAX] {
            let mut random = DosRandom::from_state(
                RandomState {
                    seed: initial,
                    counter: 17,
                },
                adjustment,
            );
            let mut seed = initial;
            for _ in 0..100 {
                let product = u64::from(seed) * 16807;
                let folded = ((product & 0x7fffffff) + (product >> 31)) as u32;
                seed = folded.wrapping_sub(adjustment);
                if (seed as i32) > (adjustment as i32) {
                    seed = seed.wrapping_add(adjustment);
                }
                assert_eq!(random.idle_step(), (seed as i16).wrapping_abs() as u16);
                assert_eq!(random.state(), RandomState { seed, counter: 17 });
            }
        }
    }
}
#[test]
fn bounded_random_rejects_outside_mask_and_wraps_counter() {
    let mut random = DosRandom::from_state(
        RandomState {
            seed: 0,
            counter: 6,
        },
        0,
    );
    assert_eq!(random.bounded(5).unwrap(), 0);
    assert_eq!(random.state().counter, 9); // Reject6,7 then accept8&7.
    let mut random = DosRandom::from_state(
        RandomState {
            seed: 0,
            counter: 0xffff,
        },
        0,
    );
    assert_eq!(random.bounded(0).unwrap(), 0);
    assert_eq!(random.state().counter, 1);
    let mut random = DosRandom::from_state(
        RandomState {
            seed: 0,
            counter: 0xffff,
        },
        0,
    );
    assert_eq!(random.bounded(0xffff).unwrap(), 0xffff);
    assert_eq!(random.state().counter, 0);
    let mut random = DosRandom::from_state(
        RandomState {
            seed: 0,
            counter: 0x8000,
        },
        0,
    );
    assert_eq!(random.bounded(0x8000).unwrap(), 0x8000);
    assert!(DosRandom::from_dos_executable(b"MZ").is_err());
}
#[test]
fn bootstrap_wait_does_not_advance_vm_or_submit_enter() {
    let mut session = session(&[0x64, 1]);
    let initial_ip = session.vm().state().ip;
    assert_eq!(session.advance().unwrap(), SessionStep::WaitKey);
    assert_eq!(session.vm().state().ip, initial_ip);
    assert_eq!(row(&session, 1), "Heading");
    session.key(Key::Enter).unwrap();
    assert!(!session.input().line_ready());
    assert!(session.input().buffer().is_empty());
    assert_eq!(row(&session, 1), "");
    assert_eq!(session.advance().unwrap(), SessionStep::Progress);
    assert_eq!(
        session.advance().unwrap(),
        SessionStep::ReadLine { timeout: 0xffff }
    );
}
#[test]
fn title_letter_is_forwarded_and_unknown_line_calls_story_handler() {
    let mut session = session(&[0x64, 1]);
    session.key(Key::Character(b'A')).unwrap();
    assert_eq!(session.input().buffer(), b"A");
    session.advance().unwrap();
    session.key(Key::Character(b'b')).unwrap();
    session.key(Key::Enter).unwrap();
    assert_eq!(session.vm().variable(VarAddr(0xda)).unwrap(), 0);
    let interrupted = session.vm().state().ip;
    assert_eq!(
        session.advance().unwrap(),
        SessionStep::ParsedLine {
            line: b"Ab\r".to_vec(),
            unknown: true
        }
    );
    assert_eq!(session.vm().state().ip.0, 0x60);
    assert_eq!(session.vm().state().returns, vec![interrupted.0]);
    assert_eq!(session.vm().variable(VarAddr(0xd8)).unwrap(), 1);
    assert_eq!(session.vm().variable(VarAddr(0x2e)).unwrap(), 0xffff);
    assert_eq!(session.parser().last_input(), b"Ab\r");
    assert!(!session.input().line_ready());
}
#[test]
fn decimal_output_is_signed_and_native_wait_does_not_clear_upper_pane() {
    let mut session = session(&[0x7c, 0xffff, 0x64, 3, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    assert_eq!(session.advance().unwrap(), SessionStep::Progress); // decimal
    session.advance().unwrap(); // finish output request
    session.advance().unwrap(); // service3 starts wait
    assert_eq!(session.advance().unwrap(), SessionStep::WaitKey);
    assert_eq!(row(&session, 1), "-1");
    session.key(Key::Enter).unwrap();
    assert_eq!(row(&session, 1), "-1");
    session.advance().unwrap(); // finish service3
    session.advance().unwrap(); // service1 starts
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::ReadLine { .. }
    ));
}
#[test]
fn bookmark_request_remains_pending_until_host_completes_it() {
    let mut session = session(&[0x64, 5]);
    session.key(Key::Enter).unwrap();
    assert_eq!(session.advance().unwrap(), SessionStep::Progress);
    let expected = session.advance().unwrap();
    assert!(matches!(
        expected,
        SessionStep::Host(HostRequest::SaveBookmark { .. })
    ));
    let ip = session.vm().state().ip;
    assert_eq!(session.advance().unwrap(), expected);
    assert_eq!(session.vm().state().ip, ip);
}
#[test]
fn editor_pause_and_bell_preserve_input_and_display_boundaries() {
    let mut session = session(&[0x64, 1]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap();
    session.key(Key::Pause).unwrap();
    assert_eq!(session.advance().unwrap(), SessionStep::WaitKey);
    session.key(Key::Enter).unwrap();
    assert!(!session.input().line_ready());
    session.key(Key::Character(7)).unwrap();
    assert_eq!(session.take_bells(), 1);
    assert_eq!(session.take_bells(), 0);
    assert_eq!(session.input().buffer(), &[7]);
    // BIOS BEL does not paint over the old block; the logical editor cursor
    // still advances and draws a second block at its new position.
    assert_eq!(session.screen().dos_cells()[(19 * 80 + 1) * 2], 0xdb);
    assert_eq!(session.screen().dos_cells()[(19 * 80 + 2) * 2], 0xdb);
}

#[test]
fn blank_line_completes_parser_and_end_service_returns_variable_address() {
    let mut session = session(&[0x64, 1, 0x64, 2]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap();
    session.set_read_return_value(11);
    let before = session.random_state();
    session.key_batch(&[Key::Enter]).unwrap();
    assert_ne!(session.random_state().seed, before.seed);
    assert_eq!(session.random_state().counter, before.counter);
    assert_eq!(
        session.advance().unwrap(),
        SessionStep::ParsedLine {
            line: b"\r".to_vec(),
            unknown: false
        }
    );
    assert_eq!(session.vm().accumulator(), 11);
    assert_eq!(session.vm().variable(VarAddr(0xd8)).unwrap(), 0);
    assert_eq!(session.vm().variable(VarAddr(0x2e)).unwrap(), 0xffff);
    assert!(session.vm().state().returns.is_empty());
    session.advance().unwrap();
    assert_eq!(session.vm().accumulator(), 0x2e);
}

#[test]
fn completed_input_service_keeps_raw_case_and_finishes_before_next_read() {
    let mut session = session(&[0x64, 1, 0x64, 10, 0x2c, 0xffff, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap();
    let keys: Vec<_> = b"HeAdInG\r"
        .iter()
        .copied()
        .map(Key::from_dos_byte)
        .collect();
    session.key_batch(&keys).unwrap();
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::ParsedLine { unknown: false, .. }
    ));
    session.advance().unwrap(); // start service10
    assert_eq!(row(&session, 1), "HeAdInG");
    session.advance().unwrap(); // finish service10
    session.advance().unwrap(); // load infinite timeout
    session.advance().unwrap(); // start service1
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::ReadLine { .. }
    ));
    assert_eq!(session.parser().last_input(), b"HeAdInG\r");
}

fn all_present_side_map() -> SideMap {
    let mut bytes = b"0\na\n".repeat(10);
    bytes.extend_from_slice(b"100\n");
    SideMap::from_bytes(&bytes).unwrap()
}
fn next_host(session: &mut Session) -> HostRequest {
    for _ in 0..100 {
        match session.advance().unwrap() {
            SessionStep::Host(request) => return request,
            SessionStep::Executed { .. } | SessionStep::Progress => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("host request not reached")
}

#[test]
fn bookmark_restore_keeps_load_ip_and_rng_but_restores_variables_and_stacks() {
    let mut session = session(&[0x6c, 99, 0x64, 5, 0x2c, 7, 0x30, 0x120, 0x64, 6, 0x64, 4]);
    session.set_side_map(all_present_side_map());
    session.key(Key::Enter).unwrap();
    let HostRequest::SaveBookmark { data } = next_host(&mut session) else {
        panic!()
    };
    session.complete_host(HostReply::Done).unwrap();
    assert!(matches!(next_host(&mut session), HostRequest::LoadBookmark));
    assert_eq!(session.vm().variable(VarAddr(0x120)).unwrap(), 7);
    let load_ip = session.vm().state().ip;
    let random = session.random_state();
    session.complete_host(HostReply::Bytes(data)).unwrap();
    assert_eq!(session.vm().state().ip, load_ip);
    assert_eq!(session.vm().state().values, vec![99]);
    assert_eq!(session.vm().variable(VarAddr(0x120)).unwrap(), 0);
    assert_eq!(session.random_state(), random);
    assert_eq!(session.input().cursor(), (19, 1));
    assert_eq!(next_host(&mut session), HostRequest::Exit { code: 0 });
    assert!(
        session
            .screen()
            .dos_cells()
            .chunks_exact(2)
            .all(|cell| cell == [b' ', 7])
    );
    session.complete_host(HostReply::Done).unwrap();
    assert_eq!(session.advance().unwrap(), SessionStep::Exited { code: 0 });
}

#[test]
fn transcript_open_retains_partial_formatter_line_and_close_does_not_flush_it() {
    let mut session = session(&[0x64, 8, 0x2c, 0xffff, 0x64, 1, 0x64, 10, 0x64, 9, 0x64, 4]);
    session.key(Key::Enter).unwrap();
    assert!(matches!(
        next_host(&mut session),
        HostRequest::TranscriptName { max_bytes: 28, .. }
    ));
    session
        .complete_host(HostReply::Name(b"OUT.TXT".to_vec()))
        .unwrap();
    assert_eq!(
        next_host(&mut session),
        HostRequest::OpenTranscript {
            path: b"OUT.TXT".to_vec()
        }
    );
    session.complete_host(HostReply::Done).unwrap();
    assert_eq!(
        next_host(&mut session),
        HostRequest::WriteTranscript {
            bytes: b"Heading \r\n".to_vec()
        }
    );
    session.complete_host(HostReply::Done).unwrap();
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::ReadLine { .. }
    ));
    let keys: Vec<_> = b"HeAdInG\r"
        .iter()
        .copied()
        .map(Key::from_dos_byte)
        .collect();
    session.key_batch(&keys).unwrap();
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::ParsedLine { unknown: false, .. }
    ));
    assert_eq!(next_host(&mut session), HostRequest::CloseTranscript);
    session.complete_host(HostReply::Done).unwrap();
    // Closing is not a partial-line flush, and exit does not close twice.
    assert_eq!(next_host(&mut session), HostRequest::Exit { code: 0 });
}

#[test]
fn read_timeout_preserves_partial_editor_for_the_next_read() {
    let mut session = session(&[0x2c, 5, 0x64, 1, 0x2c, 0xffff, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap();
    session.advance().unwrap();
    session.key_batch(&[Key::Character(b'H')]).unwrap();
    session.set_bios_ticks(5);
    assert_eq!(
        session.advance().unwrap(),
        SessionStep::TimedOut { value: 5 }
    );
    assert_eq!(session.vm().accumulator(), 5);
    assert_eq!(session.input().buffer(), b"H");
    assert!(!session.input().line_ready());
    assert_eq!(session.parser().last_input(), b"");
    assert_eq!(session.vm().variable(VarAddr(0xda)).unwrap(), 42);
    session.advance().unwrap();
    session.advance().unwrap();
    session.key_batch(&[Key::Enter]).unwrap();
    assert_eq!(
        session.advance().unwrap(),
        SessionStep::ParsedLine {
            line: b"H\r".to_vec(),
            unknown: true
        }
    );
}

#[test]
fn form_poll_submits_typeahead_before_an_immediately_expired_read() {
    // A harmless property-form no-op provides the keyboard opportunity before
    // the story enters read0. Submitted input must win over that deadline.
    let mut session = session(&[0x35, 0xffff, 0xffff, 0x2c, 0, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    let random = session.random_state();
    let mut calls = 0;
    let mut keyboard = || {
        calls += 1;
        assert_eq!(calls, 1, "the submitted read must not poll again");
        Ok(vec![Key::Enter])
    };
    assert_eq!(
        session.advance_with_keyboard(&mut keyboard).unwrap(),
        SessionStep::Progress
    );
    assert!(session.input().line_ready());
    for _ in 0..3 {
        session.advance_with_keyboard(&mut keyboard).unwrap();
    }
    assert_eq!(
        session.advance_with_keyboard(&mut keyboard).unwrap(),
        SessionStep::ParsedLine {
            line: b"\r".to_vec(),
            unknown: false,
        }
    );
    assert_eq!(calls, 1);
    assert_eq!(session.random_state(), random);
}

#[test]
fn form_poll_pause_defers_the_property_instruction_until_resume() {
    let mut session = session(&[0x35, 0xffff, 0xffff, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    let random = session.random_state();
    session
        .advance_with_keyboard(&mut || Ok(vec![Key::Pause]))
        .unwrap();
    let paused_ip = session.vm().state().ip;
    assert!(session.input().paused());
    assert_eq!(
        session
            .advance_with_keyboard(&mut || panic!("paused execution must not poll"))
            .unwrap(),
        SessionStep::WaitKey
    );
    assert_eq!(session.vm().state().ip, paused_ip);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap(); // finish the interrupted operand evaluation
    assert!(matches!(
        session.advance().unwrap(),
        SessionStep::Executed { .. }
    ));
    assert_eq!(session.random_state(), random);
}

#[test]
fn expired_reads_leave_new_keys_queued_and_partial_input_intact() {
    for ticks in [2, 3] {
        let mut session = session(&[0x2c, 2, 0x64, 1, 0x2c, 0xffff, 0x64, 1]);
        session.key(Key::Enter).unwrap();
        session.advance().unwrap(); // load2
        session.advance().unwrap(); // begin read at tick0
        session.key(Key::Character(b'H')).unwrap();
        let random = session.random_state();
        session.set_bios_ticks(ticks);
        assert_eq!(
            session
                .advance_with_keyboard(&mut || panic!("expired read must not consume a key"))
                .unwrap(),
            SessionStep::TimedOut { value: 2 }
        );
        assert_eq!(session.random_state(), random);
        assert_eq!(session.input().buffer(), b"H");
        assert!(!session.input().line_ready());

        session.advance().unwrap(); // infinite timeout
        session.advance().unwrap(); // new read
        session
            .advance_with_keyboard(&mut || Ok(vec![Key::Enter]))
            .unwrap();
        assert_eq!(
            session
                .advance_with_keyboard(&mut || panic!("submitted line must not repoll"))
                .unwrap(),
            SessionStep::ParsedLine {
                line: b"H\r".to_vec(),
                unknown: true,
            }
        );
    }
}

#[test]
fn key_polled_before_deadline_keeps_submission_priority_after_deadline() {
    let mut session = session(&[0x2c, 2, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap();
    session.advance().unwrap();
    let mut expected = DosRandom::from_state(session.random_state(), 123);
    expected.idle_step();
    session.set_bios_ticks(1);
    assert_eq!(
        session
            .advance_with_keyboard(&mut || Ok(vec![Key::Enter]))
            .unwrap(),
        SessionStep::ReadLine { timeout: 2 }
    );
    assert!(session.input().line_ready());
    assert_eq!(session.random_state(), expected.state());
    session.set_bios_ticks(3);
    assert_eq!(
        session
            .advance_with_keyboard(&mut || panic!("already submitted input takes priority"))
            .unwrap(),
        SessionStep::ParsedLine {
            line: b"\r".to_vec(),
            unknown: false,
        }
    );
    assert_eq!(session.random_state(), expected.state());
}

#[test]
fn only_read_loop_keyboard_polls_advance_the_idle_generator() {
    let program = [0x35, 0xffff, 0xffff, 0x7c, 123, 0x64, 1];
    let mut observed = session(&program);
    let mut baseline = session(&program);
    observed.key(Key::Enter).unwrap();
    baseline.key(Key::Enter).unwrap();
    let before = observed.random_state();
    let mut polls = 0;
    let mut keyboard = || {
        polls += 1;
        Ok(Vec::new())
    };
    // Property poll, its no-op, decimal output, output completion, begin read.
    for _ in 0..5 {
        assert_eq!(
            observed.advance_with_keyboard(&mut keyboard).unwrap(),
            baseline.advance().unwrap()
        );
    }
    assert_eq!(polls, 4); // one property poll and three printed digits
    assert_eq!(observed.screen().dos_cells(), baseline.screen().dos_cells());
    assert_eq!(observed.random_state(), before);
    let mut expected = DosRandom::from_state(before, 123);
    for _ in 0..2 {
        expected.idle_step();
        assert!(matches!(
            observed
                .advance_with_keyboard(&mut || Ok(Vec::new()))
                .unwrap(),
            SessionStep::ReadLine { .. }
        ));
        assert_eq!(observed.random_state(), expected.state());
    }
}

#[test]
fn output_pause_stops_mid_number_without_completing_the_vm_request() {
    let mut session = session(&[0x7c, 12345, 0x64, 1]);
    session.key(Key::Enter).unwrap();
    let random = session.random_state();
    let mut polls = 0;
    session
        .advance_with_keyboard(&mut || {
            polls += 1;
            Ok(if polls == 2 {
                vec![Key::Pause]
            } else {
                Vec::new()
            })
        })
        .unwrap();
    assert_eq!(polls, 2);
    assert_eq!(row(&session, 1), "12");
    assert!(session.input().paused());
    let paused_ip = session.vm().state().ip;
    assert_eq!(
        session
            .advance_with_keyboard(&mut || panic!("paused output must not poll"))
            .unwrap(),
        SessionStep::WaitKey
    );
    assert_eq!(session.vm().state().ip, paused_ip);
    assert_eq!(session.random_state(), random);

    session.key(Key::Enter).unwrap(); // dismiss pause, without submitting a line
    assert!(!session.input().line_ready());
    session
        .advance_with_keyboard(&mut || {
            polls += 1;
            Ok(Vec::new())
        })
        .unwrap();
    assert_eq!(polls, 5);
    assert_eq!(row(&session, 1), "12345");
    assert_eq!(session.vm().state().ip, paused_ip);
    assert_eq!(session.random_state(), random);
}

#[test]
fn bootstrap_output_services_keyboard_before_the_title_wait() {
    let exe = executable();
    let mut polls = 0;
    let mut session = Session::new_with_keyboard(
        image(&[0x64, 1]),
        TextCodec::from_dos_executable(&exe).unwrap(),
        DosRandom::from_dos_executable(&exe).unwrap(),
        &mut || {
            polls += 1;
            Ok(if polls == 1 {
                vec![Key::Pause]
            } else {
                Vec::new()
            })
        },
    )
    .unwrap();
    assert_eq!(polls, 1);
    assert_eq!(row(&session, 1), "H");
    assert!(session.input().paused());
    let initial_ip = session.vm().state().ip;
    let random = session.random_state();
    session.key(Key::Enter).unwrap();
    assert_eq!(
        session
            .advance_with_keyboard(&mut || {
                polls += 1;
                Ok(Vec::new())
            })
            .unwrap(),
        SessionStep::WaitKey
    );
    assert_eq!(polls, 8); // seven letters and the separator before pause/clear
    assert_eq!(row(&session, 1), "Heading");
    assert_eq!(session.vm().state().ip, initial_ip);
    assert_eq!(session.random_state(), random);
}

#[test]
fn keyboard_enabled_advances_preserve_wordcode_pagination_updates_without_output() {
    let mut session = session(&[0x002c, 7, 0x0030, 0xdc, 0x0064, 1]);
    session.key(Key::Enter).unwrap();
    session.advance().unwrap(); // load7
    session.advance().unwrap(); // store pagination variable
    assert_eq!(session.vm().variable(VarAddr(0xdc)).unwrap(), 7);
    session
        .advance_with_keyboard(&mut || Ok(Vec::new()))
        .unwrap(); // next operation has no output
    assert_eq!(session.vm().variable(VarAddr(0xdc)).unwrap(), 7);
}
