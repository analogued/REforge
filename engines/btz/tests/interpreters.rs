// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{
    ProgramAddr, VarAddr,
    bookmark::Bookmark,
    image::StoryImage,
    input::Key,
    interpreter::{
        BRIMSTONE_DOS, BRIMSTONE_RESOURCES, ESSEX_DOS, MINDWHEEL_DOS, NativeService, SubmittedRead,
    },
    parser::Parser,
    random::{DosRandom, RandomState},
    services::{HostReply, HostRequest},
    session::{Session, SessionStep},
    text::TextCodec,
    timing::{DosReadTiming, ReadPoll},
    vm::{Request, Step, Vm},
};

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn fixture(code: &[u16], messages: &[&[&[u8]]]) -> Vec<u8> {
    let mut bytes = vec![0; 7 * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
        (0xe8, 0x40),
        (0xe2, 1),
        (0xe4, 2),
        (0xe6, 0xffff),
        (0xee, 0x90),
        (0xf0, 0x80),
        (0xf6, 0x1e0),
        (0xf8, 0x1e2),
        (0xfa, 2),
        (0x102, 0),
        (0x10c, 0x180),
        (0x110, 0x1e4),
        (0x112, 0xffff),
        (0x114, 10),
        (0x116, 12),
        (0x134, 0xffff),
        (0x136, 0xffff),
        (0x138, 0x150),
        (0x150, 0x100),
        (0x152, 0x140),
        (0xdc, 0xffff),
        (0xde, 0xffff),
        (0x1e2, 100),
        (0x1e4, 1),
    ] {
        put(&mut bytes, offset, value);
    }
    for (offset, value) in [
        (4, 1),
        (16, 3),
        (14, 3),
        (26, 4),
        (12, 4),
        (24, 5),
        (8, 5),
        (20, 6),
        (10, 6),
        (22, 7),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    program(&mut bytes, 0x40, code);
    // Object zero's first phrase is the single recognized word "go".
    put(&mut bytes, 512 + 0x7e, 0);
    put(&mut bytes, 512 + 0x80, 3);
    bytes[3 * 512..3 * 512 + 3].copy_from_slice(&[0, 2, 0xff]);
    put(&mut bytes, 5 * 512, 2);
    bytes[5 * 512 + 6..5 * 512 + 9].copy_from_slice(b"go\0");
    let mut address = 32;
    let mut index = 0;
    for (message, tokens) in messages.iter().enumerate() {
        put(&mut bytes, 4 * 512 + message * 2, index);
        for token in *tokens {
            bytes[5 * 512 + address + 4..5 * 512 + address + 4 + token.len()]
                .copy_from_slice(token);
            put(&mut bytes, 6 * 512 + usize::from(index) * 2, address as u16);
            index += 1;
            address += token.len() + 5;
        }
        put(&mut bytes, 4 * 512 + (message + 1) * 2, index);
    }
    bytes
}
fn program(bytes: &mut [u8], address: usize, words: &[u16]) {
    for (index, &word) in words.iter().enumerate() {
        put(bytes, 512 + address + index * 2, word);
    }
}
fn image(bytes: Vec<u8>) -> StoryImage {
    StoryImage::from_dos_obj(bytes).unwrap()
}
fn codec() -> TextCodec {
    let mut exe = vec![0; 32 + 0x5ed];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    let classes = &mut exe[32 + 0x7f..32 + 0xff];
    classes.fill(4);
    for &byte in b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-" {
        classes[usize::from(byte)] = 3;
    }
    TextCodec::from_dos_executable(&exe).unwrap()
}
fn session(bytes: Vec<u8>) -> Session {
    session_with(bytes, &ESSEX_DOS)
}
fn session_with(bytes: Vec<u8>, definition: &'static btz::interpreter::Interpreter) -> Session {
    Session::with_interpreter(
        image(bytes),
        codec(),
        DosRandom::from_state(
            RandomState {
                seed: 334,
                counter: 0,
            },
            0,
        ),
        definition,
    )
    .unwrap()
}
fn first_row(session: &Session) -> String {
    session.screen().dos_cells()[162..316]
        .iter()
        .step_by(2)
        .map(|&byte| char::from(byte))
        .collect::<String>()
        .trim_end()
        .into()
}

#[test]
fn native_service_slots_are_mapped_without_changing_shared_meanings() {
    assert_eq!(MINDWHEEL_DOS.service(7), Some(NativeService::EchoUnknown));
    assert_eq!(ESSEX_DOS.service(7), Some(NativeService::EchoUnknown));
    assert_eq!(MINDWHEEL_DOS.service(10), Some(NativeService::EchoInput));
    assert_eq!(ESSEX_DOS.service(10), Some(NativeService::EchoInput));
    assert_eq!(ESSEX_DOS.service(18), Some(NativeService::DrawFrame));
    assert_eq!(ESSEX_DOS.service(17), Some(NativeService::NoOp));
    assert_eq!(MINDWHEEL_DOS.service(15), None);
    assert_eq!(ESSEX_DOS.service(0), None);
    assert_eq!(BRIMSTONE_DOS.service(7), Some(NativeService::EchoUnknown));
    assert_eq!(BRIMSTONE_DOS.service(10), Some(NativeService::EchoInput));
    assert_eq!(BRIMSTONE_DOS.service(18), Some(NativeService::DrawFrame));
    assert_eq!(BRIMSTONE_DOS.service(19), None);
}

#[test]
fn extended_bindings_execute_byte_cursor_and_cancelled_bookmark_services() {
    let code = [
        0x2c,
        0x1234,
        0x3c,
        0x191,
        0x38,
        0x191,
        0x30,
        0x192,
        0x2c,
        3,
        0x30,
        0x118,
        0x2c,
        2,
        0x30,
        0x11a,
        0x64,
        16,
        0x34,
        b'Q' as u16,
        0x64,
        15,
        0x64,
        5,
        0x64,
        6,
        0x64,
        3,
    ];
    for definition in [&ESSEX_DOS, &BRIMSTONE_DOS] {
        let mut session = session_with(fixture(&code, &[]), definition);
        let mut requests = Vec::new();
        let mut waited = false;
        for _ in 0..1000 {
            match session.advance().unwrap() {
                SessionStep::Host(HostRequest::SaveBookmark { data }) => {
                    let bookmark = Bookmark::decode(&data, 512).unwrap();
                    assert_eq!(bookmark.variables[0x191], 0x34);
                    assert_eq!(bookmark.variables[0x192], 0x34);
                    assert_eq!(bookmark.variables[0x118], 4);
                    assert_eq!(bookmark.variables[0x11a], 2);
                    requests.push("save");
                    session.complete_host(HostReply::Cancelled).unwrap();
                }
                SessionStep::Host(HostRequest::LoadBookmark) => {
                    requests.push("restore");
                    session.complete_host(HostReply::Cancelled).unwrap();
                }
                SessionStep::WaitKey => {
                    assert_eq!(requests, ["save", "restore"]);
                    waited = true;
                    break;
                }
                SessionStep::Unsupported(request) => {
                    panic!("unexpected request: {request:?}")
                }
                SessionStep::Host(request) => panic!("unexpected host request: {request:?}"),
                _ => {}
            }
        }
        assert_eq!(requests, ["save", "restore"]);
        assert!(waited, "expected the native key wait");
    }
}

#[test]
fn byte_opcodes_use_unaligned_bytes_and_preserve_the_other_byte() {
    let bytes = fixture(
        &[0x2c, 0x1234, 0x3c, 0x191, 0x38, 0x191, 0x34, b'!' as u16],
        &[],
    );
    let mut vm = Vm::with_interpreter(image(bytes), &ESSEX_DOS).unwrap();
    vm.write_variable(VarAddr(0x190), 0xabcd).unwrap();
    vm.step().unwrap();
    vm.step().unwrap();
    assert_eq!(vm.variable(VarAddr(0x190)).unwrap(), 0x34cd);
    vm.step().unwrap();
    assert_eq!(vm.accumulator(), 0x34);
    assert_eq!(
        vm.step().unwrap(),
        Step::Suspended(Request::Character { byte: b'!' })
    );
}

#[test]
fn byte_write_protects_constants_and_checks_actual_byte_extent() {
    for address in [0u16, 2, 3, 9, 512, 0xffff] {
        let mut vm =
            Vm::with_interpreter(image(fixture(&[0x3c, address], &[])), &ESSEX_DOS).unwrap();
        assert!(vm.step().is_err(), "{address:x}");
    }
    for address in [1u16, 10, 511] {
        let mut vm = Vm::with_interpreter(
            image(fixture(&[0x2c, 0x42, 0x3c, address], &[])),
            &ESSEX_DOS,
        )
        .unwrap();
        vm.step().unwrap();
        vm.step().unwrap();
        assert_eq!(vm.state().variables[usize::from(address)], 0x42);
    }
}

#[test]
fn extension_slots_remain_noops_in_mindwheel() {
    let mut vm = Vm::new(image(fixture(
        &[0x2c, 99, 0x34, 1, 0x38, 0xffff, 0x3c, 0],
        &[],
    )))
    .unwrap();
    for _ in 0..4 {
        assert!(matches!(vm.step().unwrap(), Step::Executed { .. }));
    }
    assert_eq!(vm.accumulator(), 99);
}

#[test]
fn numeric_objects_preserve_order_and_native_decimal_prefix_rules() {
    let image = image(fixture(&[], &[]));
    let codec = codec();
    let mut parser = Parser::from_classes(*codec.input_classes());
    let mut vm = Vm::with_interpreter(image.clone(), &ESSEX_DOS).unwrap();
    let result = parser
        .parse_line(&image, &codec, &mut vm, b"-2 go 65537tail\r")
        .unwrap();
    assert!(!result.unknown);
    assert_eq!(parser.last_grammar().unwrap().output, [10, 0, 11]);
    assert_eq!(vm.variable(VarAddr(0x194)).unwrap(), 0xfffe);
    assert_eq!(vm.variable(VarAddr(0x196)).unwrap(), 1);
    assert_eq!(vm.variable(VarAddr(0xd8)).unwrap(), 3);
    assert_eq!(result.output_end, VarAddr(0x34));
}

#[test]
fn numeric_slot_exhaustion_and_disabled_range_fall_back_to_unknown_words() {
    for (definition, range, input, count) in [
        (&ESSEX_DOS, (10, 12), b"1 2 3\r".as_slice(), 2),
        (&ESSEX_DOS, (0xffff, 0xffff), b"1\r", 0),
        (&BRIMSTONE_DOS, (0xffff, 0xffff), b"1\r", 0),
        (&MINDWHEEL_DOS, (10, 12), b"1\r", 0),
    ] {
        let mut bytes = fixture(&[], &[]);
        put(&mut bytes, 0x114, range.0);
        put(&mut bytes, 0x116, range.1);
        let image = image(bytes);
        let codec = codec();
        let mut parser = Parser::from_classes(*codec.input_classes());
        let mut vm = Vm::with_interpreter(image.clone(), definition).unwrap();
        assert!(
            parser
                .parse_line(&image, &codec, &mut vm, input)
                .unwrap()
                .unknown
        );
        assert_eq!(parser.last_lexical().numbers.len(), count);
    }
}

#[test]
fn submitted_read_formulas_cover_all_scratch_words_and_keep_deadline_rules() {
    for word in 0u16..=u16::MAX {
        assert_eq!(
            SubmittedRead::ShiftEight.value(word),
            (word / 256).saturating_sub(1)
        );
        assert_eq!(
            SubmittedRead::Divide85.value(word),
            (word / 85).saturating_sub(1)
        );
    }
    for value in 0..=770 {
        let word = SubmittedRead::Divide85.representative_word(value).unwrap();
        assert_eq!(SubmittedRead::Divide85.value(word), value);
    }
    assert_eq!(SubmittedRead::Divide85.representative_word(771), None);
    let timing = DosReadTiming::with_formula(2, 100, 42585, SubmittedRead::Divide85);
    assert_eq!(timing.poll(103, true), ReadPoll::Submitted { value: 500 });
    assert_eq!(timing.poll(103, false), ReadPoll::TimedOut { value: 2 });
}

#[test]
fn nested_message_callbacks_keep_native_frames_and_resume_in_order() {
    let mut bytes = fixture(
        &[0x58, 0x8000, 0x64, 4],
        &[
            &[b"before", b"|12a|", b"after"],
            &[b"inner", b"|3b|", b"tail"],
            &[b"deep"],
        ],
    );
    program(&mut bytes, 0x100, &[0x58, 0x8001, 0, 7]);
    program(&mut bytes, 0x140, &[0x64, 5, 0x58, 0x8002, 0, 9]);
    let mut session = session(bytes);
    let mut saved = false;
    for _ in 0..2000 {
        match session.advance().unwrap() {
            SessionStep::Host(HostRequest::SaveBookmark { data }) => {
                let bookmark = Bookmark::decode(&data, 512).unwrap();
                assert_eq!(bookmark.return_depth, 8);
                assert_eq!(
                    &bookmark.return_slots[..8],
                    &[2, 0x8000, 0x44, 0, 5, 0x8001, 0x104, 0]
                );
                assert_eq!(&bookmark.value_slots[..3], &[1, 2, 3]);
                assert_eq!(bookmark.encode().unwrap(), data);
                saved = true;
                // Cancellation avoids the normal save dialog's screen rebuild.
                session.complete_host(HostReply::Cancelled).unwrap();
            }
            SessionStep::Host(HostRequest::Exit { .. }) => break,
            SessionStep::WaitKey => session.key(Key::Enter).unwrap(),
            SessionStep::Unsupported(request) => panic!("{request:?}"),
            _ => {}
        }
    }
    assert!(saved);
    assert!(session.vm().state().returns.is_empty());
    assert_eq!(session.vm().variable(VarAddr(0x130)).unwrap(), 0xffff);
    assert_eq!(session.vm().accumulator(), 7);
}

#[test]
fn callback_output_rejoins_the_outer_message_without_extra_spaces() {
    let mut bytes = fixture(
        &[0x58, 0x8000, 0x64, 3],
        &[&[b"before", b"|a|", b"after"], &[b"inside"]],
    );
    program(&mut bytes, 0x100, &[0x58, 0x8001, 0, 7]);
    let mut session = session(bytes);
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    assert_eq!(first_row(&session), "Before inside after");
    assert_eq!(session.vm().state().ip, ProgramAddr(0x48));
}

#[test]
fn cursor_services_and_region_bounds_share_the_same_formatter_state() {
    let mut session = session(fixture(
        &[
            0x2c,
            5,
            0x30,
            0x118,
            0x2c,
            3,
            0x30,
            0x11a,
            0x64,
            16,
            0x34,
            b'X' as u16,
            0x64,
            15,
            0x64,
            18,
            0x64,
            3,
        ],
        &[],
    ));
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    assert_eq!(session.vm().variable(VarAddr(0x118)).unwrap(), 6);
    assert_eq!(session.vm().variable(VarAddr(0x11a)).unwrap(), 3);
    assert_eq!(session.screen().dos_cells()[(3 * 80 + 5) * 2], b'X');
    assert_eq!(session.screen().upper_cursor(), (3, 6));
}

#[test]
fn a_restored_fetch_state_can_resume_native_text_frames() {
    let image = image(fixture(&[0, 7], &[]));
    let mut state = Vm::with_interpreter(image.clone(), &ESSEX_DOS)
        .unwrap()
        .state()
        .clone();
    state.returns = vec![2, 0x8000, 0x88, 0];
    put(&mut state.variables, 0x130, 0x8000);
    let mut vm = Vm::from_state_with_interpreter(image, state, &ESSEX_DOS).unwrap();
    assert_eq!(
        vm.step().unwrap(),
        Step::Suspended(Request::ResumeText {
            argument: 0x8000,
            position: 2
        })
    );
    assert_eq!(vm.accumulator(), 7);
    assert_eq!(vm.state().argument, 0x8000);
    assert_eq!(vm.state().ip, ProgramAddr(0x88));
    assert_eq!(vm.variable(VarAddr(0x130)).unwrap(), 0xffff);
    assert!(vm.state().returns.is_empty());
}

#[test]
fn object_phrase_callbacks_resume_at_byte_offsets() {
    let mut bytes = fixture(
        &[0x68, 0, 0x64, 3],
        &[&[b"before", b"|a|", b"after"], &[b"inside"]],
    );
    program(&mut bytes, 0x100, &[0x58, 0x8001, 0, 7]);
    // Reuse authored dictionary addresses from message zero in object zero.
    for index in 0..3 {
        let address = u16::from_le_bytes(
            bytes[6 * 512 + index * 2..6 * 512 + index * 2 + 2]
                .try_into()
                .unwrap(),
        );
        bytes[3 * 512 + index * 2..3 * 512 + index * 2 + 2].copy_from_slice(&address.to_be_bytes());
    }
    bytes[3 * 512 + 6] = 0xff;
    put(&mut bytes, 512 + 0x80, 7);
    let mut session = session(bytes);
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    assert_eq!(first_row(&session), "Before inside after");
    assert!(session.vm().state().returns.is_empty());
}

#[test]
fn raw_character_controls_and_cursor_outside_story_region_remain_safe() {
    let mut session = session(fixture(
        &[
            0x2c,
            0,
            0x30,
            0x118,
            0x2c,
            24,
            0x30,
            0x11a,
            0x64,
            16,
            0x34,
            11,
            0x34,
            12,
            0x34,
            13,
            0x34,
            7,
            0x34,
            b'Z' as u16,
            0x64,
            15,
            0x64,
            3,
        ],
        &[],
    ));
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    assert_eq!(session.take_bells(), 1);
    assert_eq!(session.vm().variable(VarAddr(0x118)).unwrap(), 2);
    assert_eq!(session.vm().variable(VarAddr(0x11a)).unwrap(), 24);
    assert_eq!(session.screen().upper_cursor(), (24, 2));
}

#[test]
fn executable_resource_offsets_are_independent_of_behavior_selection() {
    use btz::{interpreter::DosResources, services::ServiceMessages};
    let resources = DosResources {
        pairs: 64,
        thirds: 640,
        pause: 800,
        classes: 832,
        random_adjustment: 1024,
        service_strings: [1040, 1056, 1072, 1088],
    };
    // Both an arbitrary relocation and the third DOS layout must load their
    // own tables, including the separately relocated printer-error message.
    for resources in [resources, BRIMSTONE_RESOURCES] {
        let mut exe = vec![0; 32 + 2048];
        exe[..2].copy_from_slice(b"MZ");
        put(&mut exe, 8, 2);
        exe[32 + resources.pairs + 1] = b'A';
        exe[32 + resources.pairs + 257] = b'B';
        exe[32 + resources.pairs + 128] = b'X';
        exe[32 + resources.pairs + 384] = b'Y';
        exe[32 + resources.thirds + 1] = b'Z';
        exe[32 + resources.pause..32 + resources.pause + 4].copy_from_slice(b"KEY\0");
        exe[32 + resources.classes..32 + resources.classes + 128].fill(3);
        exe[32 + resources.random_adjustment..32 + resources.random_adjustment + 4]
            .copy_from_slice(&123u32.to_le_bytes());
        for (address, text) in resources.service_strings.into_iter().zip([
            b"printer".as_slice(),
            b"error",
            b"disk",
            b"wrong",
        ]) {
            exe[32 + address..32 + address + text.len()].copy_from_slice(text);
        }
        let codec = TextCodec::from_dos_resources(&exe, resources).unwrap();
        assert_eq!(codec.pause_prompt(), b"KEY");
        assert_eq!(codec.input_classes(), &[3; 128]);
        let mut bytes = fixture(&[], &[]);
        bytes[5 * 512 + 6..5 * 512 + 9].copy_from_slice(&[128, 1, 0]);
        assert_eq!(codec.word(&image(bytes), 2).unwrap(), b"XYABZ");
        let messages = ServiceMessages::from_dos_resources(&exe, resources).unwrap();
        assert_eq!(messages.printer_prompt, b"printer");
        assert_eq!(messages.printer_error, b"error");
        assert_eq!(messages.disk_prompt, b"disk");
        assert_eq!(messages.wrong_disk, b"wrong");
        let mut random = DosRandom::from_dos_resources(&exe, resources).unwrap();
        let mut expected = DosRandom::from_state(
            RandomState {
                seed: 334,
                counter: 0,
            },
            123,
        );
        assert_eq!(random.idle_step(), expected.idle_step());
        let invalid = DosResources {
            pairs: usize::MAX,
            ..resources
        };
        assert!(TextCodec::from_dos_resources(&exe, invalid).is_err());
    }
}

#[test]
fn essex_accepts_property_keys_beyond_object_ids_but_still_checks_the_object() {
    let mut bytes = fixture(&[], &[]);
    for (offset, value) in [(0x1a0, 99), (0x1a2, 7)] {
        put(&mut bytes, offset, value);
    }
    program(&mut bytes, 0x90, &[0x1a0, 0, 0x1a4, 0]);
    let image = image(bytes);
    let mut essex = Vm::with_interpreter(image.clone(), &ESSEX_DOS).unwrap();
    assert_eq!(
        essex
            .property(btz::ObjectId(0), btz::ObjectId(99), false)
            .unwrap(),
        7
    );
    assert_eq!(
        essex
            .property(btz::ObjectId(0), btz::ObjectId(99), true)
            .unwrap(),
        0x1a2
    );
    assert!(
        essex
            .property(btz::ObjectId(1), btz::ObjectId(99), false)
            .is_err()
    );
    let mut mindwheel = Vm::new(image).unwrap();
    assert!(
        mindwheel
            .property(btz::ObjectId(0), btz::ObjectId(99), false)
            .is_err()
    );
}

#[test]
fn narrow_regions_split_long_words_without_replaying_them_over_the_border() {
    let mut session = session(fixture(
        &[0x2c, 5, 0x30, 0x120, 0x58, 0x8000, 0x64, 3],
        &[&[b"abcdefghij"]],
    ));
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    let cells = session.screen().dos_cells();
    let row = |row: usize| {
        cells[row * 160 + 2..row * 160 + 12]
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(row(1), b"Abcde");
    assert_eq!(row(2), b"fghij");
    assert_eq!(cells[(2 * 80 + 78) * 2], 0xba);
}

#[test]
fn numeric_layout_uses_the_header_cached_at_initialization() {
    let image = image(fixture(&[], &[]));
    let codec = codec();
    let mut parser = Parser::from_classes(*codec.input_classes());
    let mut vm = Vm::with_interpreter(image.clone(), &ESSEX_DOS).unwrap();
    vm.write_variable(VarAddr(0x114), 0xffff).unwrap();
    vm.write_variable(VarAddr(0x116), 0xffff).unwrap();
    vm.write_variable(VarAddr(0x10c), 0x1a0).unwrap();
    assert!(
        !parser
            .parse_line(&image, &codec, &mut vm, b"-2\r")
            .unwrap()
            .unknown
    );
    assert_eq!(vm.variable(VarAddr(0x194)).unwrap(), 0xfffe);
    assert_eq!(vm.variable(VarAddr(0x1b4)).unwrap(), 0);
}

#[test]
fn pause_control_skips_display_waits_without_disabling_ordinary_input() {
    let mut session = session(fixture(
        &[
            0x2c, 0, 0x30, 0x134, 0x64, 3, 0x58, 0x8000, 0x2c, 0xffff, 0x64, 1,
        ],
        &[&[b"\x0c", b"ready"]],
    ));
    for _ in 0..1000 {
        match session.advance().unwrap() {
            SessionStep::ReadLine { .. } => break,
            SessionStep::WaitKey => panic!("disabled pause requested a key"),
            _ => {}
        }
    }
    session
        .key_batch(&[Key::Pause, Key::Escape, Key::Character(b'x')])
        .unwrap();
    assert!(!session.input().paused());
    assert_eq!(session.input().buffer(), &[0x13, 0x1b, b'x']);
    assert_eq!(first_row(&session), "Ready");
}

#[test]
fn restored_extended_cursor_and_region_are_reinitialized_after_loading() {
    let mut session = session(fixture(&[0x64, 6, 0x64, 3], &[]));
    let mut bookmark = Bookmark::from_vm(session.vm().state(), [0xffff; 100], 0, 1).unwrap();
    for (address, value) in [
        (0x118, 21),
        (0x11a, 13),
        (0x11c, 5),
        (0x120, 40),
        (0x126, 22),
        (0x1e, 0),
    ] {
        put(&mut bookmark.variables, address, value);
    }
    session.set_side_map(btz::services::SideMap {
        entries: [btz::services::SideEntry {
            physical_disk: 0,
            drive: b'A',
        }; 10],
        trailing_value: 100,
    });
    for _ in 0..1000 {
        if matches!(
            session.advance().unwrap(),
            SessionStep::Host(HostRequest::LoadBookmark)
        ) {
            break;
        }
    }
    session
        .complete_host(HostReply::Bytes(bookmark.encode().unwrap()))
        .unwrap();
    assert_eq!(session.vm().variable(VarAddr(0x118)).unwrap(), 1);
    assert_eq!(session.vm().variable(VarAddr(0x11a)).unwrap(), 1);
    assert_eq!(session.vm().variable(VarAddr(0x11c)).unwrap(), 1);
    assert_eq!(session.vm().variable(VarAddr(0x120)).unwrap(), 77);
    assert_eq!(session.vm().variable(VarAddr(0x126)).unwrap(), 24);
}

#[test]
fn transcript_control_suppresses_output_without_closing_the_transcript() {
    let mut session = session(fixture(
        &[
            0x64, 8, 0x58, 0x8000, 0x2c, 0, 0x30, 0x136, 0x58, 0x8001, 0x2c, 0xffff, 0x30, 0x136,
            0x58, 0x8002, 0x2c, 0xffff, 0x64, 1,
        ],
        &[&[b"visible"], &[b"hidden", b"@"], &[b"again", b"@"]],
    ));
    let mut output = Vec::new();
    for _ in 0..2000 {
        match session.advance().unwrap() {
            SessionStep::Host(HostRequest::TranscriptName { .. }) => session
                .complete_host(HostReply::Name(b"test.txt".to_vec()))
                .unwrap(),
            SessionStep::Host(HostRequest::OpenTranscript { .. }) => {
                session.complete_host(HostReply::Done).unwrap()
            }
            SessionStep::Host(HostRequest::WriteTranscript { bytes }) => {
                output.extend(bytes);
                session.complete_host(HostReply::Done).unwrap();
            }
            SessionStep::ReadLine { .. } => break,
            _ => {}
        }
    }
    assert_eq!(output, b"Visibleagain\r\n\r\n");
}

#[test]
fn essex_echoes_unknown_words_and_original_input_through_the_correct_slots() {
    let mut bytes = fixture(
        &[
            0x2c, 0xffff, 0x64, 1, 0x64, 7, 0x58, 0x8000, 0x64, 10, 0x2c, 0, 0x64, 3,
        ],
        &[&[b"@"]],
    );
    put(&mut bytes, 0xea, 0xffff);
    let mut session = session(bytes);
    for _ in 0..2000 {
        match session.advance().unwrap() {
            SessionStep::ReadLine { .. } if !session.input().line_ready() => {
                session
                    .key_batch(
                        &b"GO XYZ\r"
                            .iter()
                            .copied()
                            .map(Key::from_dos_byte)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            }
            SessionStep::WaitKey => break,
            _ => {}
        }
    }
    assert_eq!(first_row(&session), "Xyz");
    let row = session.screen().dos_cells()[322..476]
        .iter()
        .step_by(2)
        .copied()
        .collect::<Vec<_>>();
    assert!(row.starts_with(b"GO XYZ"));
}

#[test]
fn display_extent_metadata_does_not_move_the_physical_pause_line() {
    let mut bytes = fixture(
        &[
            0x2c,
            5,
            0x30,
            0x11a,
            0x64,
            16,
            0x34,
            b'X' as u16,
            0x2c,
            10,
            0x30,
            0x124,
            0x2c,
            5,
            0x30,
            0x126,
            0x64,
            3,
        ],
        &[],
    );
    put(&mut bytes, 0x128, 0x1234);
    put(&mut bytes, 0x12a, 0x5678);
    let mut session = session(bytes);
    for _ in 0..1000 {
        if matches!(session.advance().unwrap(), SessionStep::WaitKey) {
            break;
        }
    }
    assert_eq!(session.screen().dos_cells()[(5 * 80 + 1) * 2], b'X');
    assert_eq!(session.vm().variable(VarAddr(0x126)).unwrap(), 5);
    assert_eq!(session.vm().variable(VarAddr(0x128)).unwrap(), 0x1234);
    assert_eq!(session.vm().variable(VarAddr(0x12a)).unwrap(), 0x5678);
}

#[test]
fn native_restore_preserves_saved_bytes_outside_normal_wordcode_write_rules() {
    for definition in [&MINDWHEEL_DOS, &ESSEX_DOS, &BRIMSTONE_DOS] {
        let image = image(fixture(&[0x64, 6, 0x64, 3], &[]));
        let mut session = Session::with_interpreter(
            image,
            codec(),
            DosRandom::from_state(
                RandomState {
                    seed: 334,
                    counter: 0,
                },
                0,
            ),
            definition,
        )
        .unwrap();
        let mut bookmark = Bookmark::from_vm(session.vm().state(), [0xffff; 100], 0, 1).unwrap();
        let saved = [0x42ff, 0x21, 0x43, 0x65, 0x87];
        for (index, value) in saved.into_iter().enumerate() {
            put(&mut bookmark.variables, index * 2, value);
        }
        session.set_side_map(btz::services::SideMap {
            entries: [btz::services::SideEntry {
                physical_disk: 0,
                drive: b'A',
            }; 10],
            trailing_value: 100,
        });
        for _ in 0..1000 {
            if matches!(
                session.advance().unwrap(),
                SessionStep::Host(HostRequest::LoadBookmark)
            ) {
                break;
            }
        }
        session
            .complete_host(HostReply::Bytes(bookmark.encode().unwrap()))
            .unwrap();
        for (index, value) in saved.into_iter().enumerate() {
            assert_eq!(
                session.vm().variable(VarAddr(index as u16 * 2)).unwrap(),
                value
            );
        }
    }
}

#[test]
fn breakers_constant_forms_comparisons_and_recall_preserve_native_variables() {
    use btz::interpreter::BREAKERS_DOS;
    let code = [
        0x082c, 0x30, 0x180, // form4 is ffff, with no operands
        0x082e, 0x30, 0x182, // form6 is zero, with no operands
        0x2c, 55, 0x082f, 0x30, 0x184, // form7 retains the previous argument
        0x2c, 0x1234, 0x18, 0x1234, 0x5f, 0x30, 0x186,
    ];
    let mut vm = Vm::with_interpreter(image(fixture(&code, &[])), &BREAKERS_DOS).unwrap();
    let previous_ea = vm.variable(VarAddr(0xea)).unwrap();
    for _ in 0..11 {
        vm.step().unwrap();
    }
    for (address, value) in [
        (0x180, 0xffff),
        (0x182, 0),
        (0x184, 55),
        (0x186, 0x1234),
        (0xec, 0x1234),
    ] {
        assert_eq!(vm.variable(VarAddr(address)).unwrap(), value, "{address:x}");
    }
    assert_eq!(vm.variable(VarAddr(0xea)).unwrap(), previous_ea);
    for opcode in 5..=10 {
        let mut vm = Vm::with_interpreter(
            image(fixture(&[0x2c, 123, opcode * 4, 1, 0x5f], &[])),
            &BREAKERS_DOS,
        )
        .unwrap();
        vm.step().unwrap();
        vm.step().unwrap();
        assert_eq!(vm.variable(VarAddr(0xec)).unwrap(), 123);
        vm.step().unwrap();
        assert_eq!(vm.accumulator(), 123);
    }
}

#[test]
fn breakers_form4_does_not_consume_the_following_instruction() {
    let bytes: Vec<_> = [0x082cu16, 0x2c, 9, 11, 12]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let native =
        btz::decode::decode_with_rules(&bytes, ProgramAddr(0), btz::interpreter::CONSTANT_FORMS)
            .unwrap();
    assert!(native.operands.is_empty());
    assert_eq!(native.next, ProgramAddr(2));
    let older = btz::decode::decode(&bytes, ProgramAddr(0)).unwrap();
    assert_eq!(older.operands.len(), 3);
    assert_eq!(older.next, ProgramAddr(8));
}

fn wait(session: &mut Session) {
    for _ in 0..2000 {
        match session.advance().unwrap() {
            SessionStep::WaitKey => return,
            SessionStep::Unsupported(r) => panic!("{r:?}"),
            _ => {}
        }
    }
    panic!("expected a key wait");
}

#[test]
fn breakers_character_opcode_uses_the_formatter_and_old_slot_is_empty() {
    let mut s = session_with(
        fixture(&[0x34, b'X' as u16, 0x60, b'q' as u16, 0x64, 3], &[]),
        &btz::interpreter::BREAKERS_DOS,
    );
    wait(&mut s);
    assert_eq!(first_row(&s), "Q");
    assert_eq!(s.input().cursor(), (20, 1));
    assert_eq!(s.vm().variable(VarAddr(0x120)).unwrap(), 78);
    assert_eq!(s.screen().dos_cells()[79 * 2], 0xbb);
    for byte in 0..=255u16 {
        let mut vm = Vm::with_interpreter(
            image(fixture(&[0x60, byte], &[])),
            &btz::interpreter::BREAKERS_DOS,
        )
        .unwrap();
        assert_eq!(
            vm.step().unwrap(),
            Step::Suspended(Request::CharacterToken { byte: byte as u8 })
        );
    }
}

#[test]
fn breakers_screen_services_keep_character_buffers_inside_native_bookmarks() {
    let mut code = Vec::new();
    for (address, value) in [
        (0x11c, 10),
        (0x11e, 4),
        (0x120, 14),
        (0x122, 6),
        (0x118, 10),
        (0x11a, 4),
    ] {
        code.extend([0x2c, value, 0x30, address]);
    }
    code.extend([
        0x64,
        18,
        0x64,
        16,
        0x60,
        b'A' as u16,
        0x2c,
        0x180,
        0x64,
        17,
        0x64,
        16,
        0x60,
        b'B' as u16,
        0x2c,
        0x180,
        0x64,
        19,
        0x64,
        3,
    ]);
    let mut bytes = fixture(&[], &[]);
    put(&mut bytes, 0xe8, 0x200);
    program(&mut bytes, 0x200, &code);
    let mut s = session_with(bytes, &btz::interpreter::BREAKERS_DOS);
    wait(&mut s);
    assert_eq!(s.screen().dos_cells()[(4 * 80 + 10) * 2], b'A');
    assert_eq!(s.screen().dos_cells()[(3 * 80 + 9) * 2], 0xda);
    assert_eq!(s.screen().dos_cells()[(7 * 80 + 15) * 2], 0xd9);
    assert_eq!(s.vm().state().variables[0x180], 0xda);
    assert_eq!(s.vm().state().variables[0x180 + 8], b'A');
    assert_eq!(s.screen().upper_cursor().0, 4);
}

#[test]
fn only_breakers_clears_accumulator_when_a_transcript_is_cancelled() {
    for definition in [
        &MINDWHEEL_DOS,
        &ESSEX_DOS,
        &BRIMSTONE_DOS,
        &btz::interpreter::BREAKERS_DOS,
    ] {
        let mut s = session_with(fixture(&[0x2c, 0x1234, 0x64, 8, 0x64, 3], &[]), definition);
        let mut cancelled = false;
        for _ in 0..2000 {
            match s.advance().unwrap() {
                SessionStep::Host(HostRequest::TranscriptName { .. }) => {
                    s.complete_host(HostReply::Cancelled).unwrap();
                    cancelled = true;
                }
                SessionStep::WaitKey => break,
                _ => {}
            }
        }
        assert!(cancelled);
        assert_eq!(
            s.vm().accumulator(),
            if definition.transcript_cancel_zero {
                0
            } else {
                0x1234
            }
        );
    }
}

#[test]
fn standalone_character_tokens_collect_controls_without_dispatching_a_message_callback() {
    let mut bytes = fixture(
        &[
            0x60,
            b'|' as u16,
            0x60,
            b'a' as u16,
            0x60,
            b'3' as u16,
            0x60,
            b'|' as u16,
            0x64,
            3,
        ],
        &[],
    );
    program(&mut bytes, 0x100, &[0x2c, 99, 0x30, 0x180, 0, 0]);
    let mut s = session_with(bytes, &btz::interpreter::BREAKERS_DOS);
    wait(&mut s);
    assert_eq!(s.vm().variable(VarAddr(0x180)).unwrap(), 0);
    assert_eq!(s.vm().state().values, [3]);
    assert!(s.vm().state().returns.is_empty());
    assert_eq!(first_row(&s), "");
}

#[test]
fn zero_character_token_retains_the_empty_c_strings_separator() {
    let mut s = session_with(
        fixture(
            &[0x60, b'Q' as u16, 0x60, 0, 0x60, b'R' as u16, 0x64, 3],
            &[],
        ),
        &btz::interpreter::BREAKERS_DOS,
    );
    wait(&mut s);
    assert_eq!(first_row(&s), "Q  R");
}
