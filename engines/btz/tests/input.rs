// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::input::{InputAction, InputEditor, Key};

fn type_bytes(editor: &mut InputEditor, text: &[u8]) -> Vec<InputAction> {
    text.iter()
        .flat_map(|&b| editor.process_key(Key::from_dos_byte(b)))
        .collect()
}

#[test]
fn first_typing_uses_the_painted_cursor_and_breakers_scrolls_only_later_rows() {
    use btz::{
        interpreter::{BREAKERS_DOS, BRIMSTONE_DOS, ESSEX_DOS, MINDWHEEL_DOS},
        text::TextScreen,
    };
    for definition in [&MINDWHEEL_DOS, &ESSEX_DOS, &BRIMSTONE_DOS, &BREAKERS_DOS] {
        let mut editor = InputEditor::with_layout(definition.layout);
        let mut screen = TextScreen::with_layout(definition.layout);
        for action in type_bytes(&mut editor, b"no") {
            screen.apply_input(&action, &[]).unwrap();
        }
        assert_eq!(editor.cursor(), (19, 3));
        let row = |screen: &TextScreen, y: usize| -> Vec<u8> {
            screen.dos_cells()[y * 160..(y + 1) * 160]
                .iter()
                .step_by(2)
                .copied()
                .collect()
        };
        assert_eq!(&row(&screen, 19)[1..4], b"no\xdb");
        assert_eq!(row(&screen, 20)[1], b' ');
        for action in editor.process_key(Key::Enter) {
            screen.apply_input(&action, &[]).unwrap();
        }
        assert_eq!(editor.cursor(), (20, 1));
        assert_eq!(&row(&screen, 19)[1..4], b"no ");
        assert_eq!(row(&screen, 20)[1], 0xdb);
        editor.acknowledge_line().unwrap();
        for command in [b"look\r".as_slice(), b"inventory\r", b"wait\r"] {
            for action in type_bytes(&mut editor, command) {
                screen.apply_input(&action, &[]).unwrap();
            }
            editor.acknowledge_line().unwrap();
        }
        assert_eq!(editor.cursor(), (22, 1));
        if std::ptr::eq(definition, &BREAKERS_DOS) {
            assert_eq!(&row(&screen, 19)[1..4], b"no ");
            assert!(row(&screen, 20).windows(9).any(|s| s == b"inventory"));
        } else {
            assert!(row(&screen, 19).windows(4).any(|s| s == b"look"));
        }
        editor.rebuild_frame();
        let screen = TextScreen::with_layout(definition.layout);
        assert_eq!(editor.cursor(), (19, 1));
        assert_eq!(row(&screen, 19)[1], 0xdb);
    }
}

#[test]
fn submitted_raw_line_preserves_case_and_waits_for_parser_acknowledgment() {
    let mut editor = InputEditor::new();
    let actions = type_bytes(&mut editor, b"HeLLo\r");
    assert_eq!(editor.submitted_line(), Some(b"HeLLo\r".as_slice()));
    assert_eq!(editor.cursor(), (20, 1));
    assert!(actions.contains(&InputAction::ClearSubmitFlag));
    assert!(editor.process_key(Key::Character(b'x')).is_empty());
    assert_eq!(editor.acknowledge_line().unwrap(), b"HeLLo\r");
    assert!(!editor.line_ready());
    assert!(editor.buffer().is_empty());
    assert!(editor.acknowledge_line().is_err());
}

#[test]
fn editor_backspace_erases_block_then_moves_cursor_over_deleted_byte() {
    let mut editor = InputEditor::new();
    type_bytes(&mut editor, b"ab");
    assert_eq!(
        editor.process_key(Key::Backspace),
        [
            InputAction::Put {
                row: 19,
                column: 3,
                byte: b' '
            },
            InputAction::Put {
                row: 19,
                column: 2,
                byte: 0xdb
            }
        ]
    );
    assert_eq!(editor.buffer(), b"a");
    type_bytes(&mut editor, b"\x1d\x08");
    assert!(editor.buffer().is_empty());
    assert_eq!(editor.cursor(), (19, 1));
}

#[test]
fn ordinary_character_cap_is_76_but_return_is_still_accepted() {
    let mut editor = InputEditor::new();
    type_bytes(&mut editor, &[b'x'; 100]);
    assert_eq!(editor.buffer().len(), 76);
    assert_eq!(editor.cursor(), (19, 77));
    editor.process_key(Key::Enter);
    assert_eq!(editor.buffer().len(), 77);
    assert_eq!(editor.buffer().last(), Some(&13));
    assert!(editor.line_ready());
}

#[test]
fn tabs_insert_ten_literal_spaces_with_the_same_buffer_cap() {
    let mut editor = InputEditor::new();
    type_bytes(&mut editor, b"a\tb");
    assert_eq!(editor.buffer(), b"a          b");
    for _ in 0..10 {
        editor.process_key(Key::Tab);
    }
    assert_eq!(editor.buffer().len(), 76);
    assert_eq!(editor.cursor(), (19, 77));
}

#[test]
fn history_recall_uses_cursor_position_and_preserves_original_case() {
    let mut editor = InputEditor::new();
    type_bytes(&mut editor, b"AbCd\r");
    editor.acknowledge_line().unwrap();
    type_bytes(&mut editor, b"x\x1c");
    assert_eq!(editor.buffer(), b"xb");
    editor.process_key(Key::RecallLine);
    assert_eq!(editor.buffer(), b"xbCd");
    editor.process_key(Key::RecallCharacter);
    assert_eq!(editor.buffer(), b"xbCd");
    editor.process_key(Key::EraseLine);
    assert_eq!(editor.buffer(), b"");
    assert_eq!(editor.cursor(), (20, 1));
    editor.process_key(Key::RecallLine);
    assert_eq!(editor.buffer(), b"AbCd");
}

#[test]
fn completed_lines_scroll_only_the_observed_input_rectangle() {
    let mut editor = InputEditor::new();
    for _ in 0..3 {
        type_bytes(&mut editor, b"x\r");
        editor.acknowledge_line().unwrap();
    }
    assert_eq!(editor.cursor(), (22, 1));
    let actions = type_bytes(&mut editor, b"x\r");
    assert_eq!(editor.cursor(), (22, 1));
    assert!(actions.contains(&InputAction::Scroll {
        top: 19,
        bottom: 22,
        left: 1,
        right: 77
    }));
    assert_eq!(
        actions.last(),
        Some(&InputAction::Put {
            row: 22,
            column: 1,
            byte: 0xdb
        })
    );
}

#[test]
fn pause_consumes_return_but_forwards_other_dismissal_keys() {
    let mut editor = InputEditor::new();
    assert_eq!(
        editor.process_key(Key::Pause),
        [InputAction::PauseStatus { active: true }]
    );
    assert!(editor.process_key(Key::NoKey).is_empty());
    assert!(editor.process_key(Key::Pause).is_empty());
    assert!(editor.paused());
    assert_eq!(
        editor.process_key(Key::Enter),
        [InputAction::PauseStatus { active: false }]
    );
    assert!(!editor.line_ready());
    editor.process_key(Key::Pause);
    let actions = editor.process_key(Key::Character(b'X'));
    assert_eq!(
        actions.first(),
        Some(&InputAction::PauseStatus { active: false })
    );
    assert_eq!(editor.buffer(), b"X");
    editor.process_key(Key::Pause);
    editor.process_key(Key::Ignored);
    assert!(!editor.paused());
}

#[test]
fn native_key_aliases_distinguish_empty_polls_from_ignored_keys() {
    assert_eq!(Key::from_dos_byte(0), Key::NoKey);
    for byte in [0xc8, 0xd6] {
        assert_eq!(Key::from_dos_byte(byte), Key::EraseLine);
    }
    for byte in [0xc9, 0xd4] {
        assert_eq!(Key::from_dos_byte(byte), Key::RecallLine);
    }
    for byte in [0x1c, 0xd2] {
        assert_eq!(Key::from_dos_byte(byte), Key::RecallCharacter);
    }
    for byte in [0x1e, 0x1f, 0xd3, 0xdb] {
        assert_eq!(Key::from_dos_byte(byte), Key::Ignored);
    }
    let mut editor = InputEditor::new();
    assert!(editor.process_key(Key::NoKey).is_empty());
    type_bytes(&mut editor, b"A\nB");
    assert_eq!(editor.buffer(), b"AB");
    let bell = editor.process_key(Key::from_dos_byte(7));
    assert_eq!(editor.buffer(), b"AB\x07");
    assert_eq!(editor.cursor(), (19, 4));
    assert_eq!(
        bell,
        [
            InputAction::Bell,
            InputAction::Put {
                row: 19,
                column: 4,
                byte: 0xdb
            }
        ]
    );
}
