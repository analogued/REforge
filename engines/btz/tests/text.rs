// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::text::{TextCodec, TextOutput};
use btz::{ObjectId, image::StoryImage};

fn put(bytes: &mut [u8], offset: usize, word: u16) {
    bytes[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
}
fn executable() -> Vec<u8> {
    let mut exe = vec![0; 32 + 0x5ed];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    for (offset, byte) in [
        (0x36d + 128, b'x'),
        (0x46d + 128, b'y'),
        (0x36d + 127, b'z'),
        (0x46d + 127, b'q'),
        (0x56d + 127, b'r'),
    ] {
        exe[32 + offset] = byte;
    }
    exe[32 + 0xff..32 + 0xff + 4].copy_from_slice(b"KEY\0");
    exe
}
fn story() -> Vec<u8> {
    let mut bytes = vec![0; 6 * 512];
    put(&mut bytes, 0, 512);
    for (offset, word) in [
        (4, 1),
        (14, 2),
        (12, 3),
        (8, 4),
        (10, 5),
        (16, 2),
        (26, 3),
        (24, 4),
        (20, 5),
        (22, 6),
    ] {
        put(&mut bytes, 512 + offset, word);
    }
    bytes[4 * 512 + 6..4 * 512 + 11].copy_from_slice(b"ab\x80\x7f\0");
    put(&mut bytes, 3 * 512 + 2, 4);
    for (index, word) in [2, 2, 0xffff, 0xfffe].into_iter().enumerate() {
        put(&mut bytes, 5 * 512 + index * 2, word);
    }
    bytes
}
fn tokens(words: &[&[u8]]) -> Vec<Vec<u8>> {
    words.iter().map(|s| s.to_vec()).collect()
}
fn row(output: &TextOutput, row: usize) -> String {
    output.screen().dos_cells()[row * 160 + 2..row * 160 + 156]
        .iter()
        .step_by(2)
        .map(|&b| char::from(b))
        .collect::<String>()
        .trim_end()
        .to_string()
}
#[test]
fn external_codec_expands_pairs_triples_and_word_indexed_messages() {
    let codec = TextCodec::from_dos_executable(&executable()).unwrap();
    let image = StoryImage::from_dos_obj(story()).unwrap();
    assert_eq!(codec.pause_prompt(), b"KEY");
    assert_eq!(codec.word(&image, 2).unwrap(), b"abxyzqr");
    assert_eq!(
        codec.message(&image, 0x8000).unwrap(),
        tokens(&[b"abxyzqr", b"abxyzqr"])
    );
    assert_eq!(
        codec.message(&image, 0).unwrap(),
        codec.message(&image, 0x8000).unwrap()
    );
    assert!(codec.message(&image, 0xffff).unwrap().is_empty());
    assert!(codec.message(&image, 0xc180).is_err());
}
#[test]
fn decoding_checks_external_extents_and_malformed_words() {
    let mut exe = executable();
    exe.truncate(100);
    assert!(TextCodec::from_dos_executable(&exe).is_err());
    assert!(TextCodec::from_dos_executable(&[0; 100]).is_err());
    let codec = TextCodec::from_dos_executable(&executable()).unwrap();
    let mut bytes = story();
    put(&mut bytes, 3 * 512 + 2, 257);
    let image = StoryImage::from_dos_obj(bytes).unwrap();
    assert!(codec.message(&image, 0x8000).is_err());
    assert!(codec.word(&image, 511).is_err());
    let mut bytes = story();
    bytes[4 * 512 + 6..4 * 512 + 47].fill(b'a');
    assert!(
        codec
            .word(&StoryImage::from_dos_obj(bytes).unwrap(), 2)
            .is_err()
    );
}
#[test]
fn punctuation_quotes_and_sentence_state_cross_messages() {
    let mut out = TextOutput::new();
    out.message(tokens(&[b"hello", b",", b"world", b"."]), b"KEY")
        .unwrap();
    out.message(
        tokens(&[
            b"next", b"[", b"test", b"]", b"\"", b"quoted", b"\"", b"(", b"more", b")", b"!",
        ]),
        b"KEY",
    )
    .unwrap();
    assert_eq!(
        row(&out, 1),
        "Hello, world.  Next [test] \"quoted\" (more)!"
    );
}
#[test]
fn case_indentation_and_paragraph_controls_are_not_printed() {
    let mut out = TextOutput::new();
    out.message(
        tokens(&[
            b"\x06", b"\x14", b"\x02", b"\x17", b"heading", b"@", b"\x10", b"alpha", b".", b"\x15",
            b"beta", b"\x0e", b"\x03", b"gamma",
        ]),
        b"KEY",
    )
    .unwrap();
    assert_eq!(row(&out, 1), format!("{}HEADING", " ".repeat(30)));
    assert_eq!(row(&out, 2), "");
    assert_eq!(row(&out, 3), "  Alpha.  beta Gamma");
}
#[test]
fn pause_keeps_pending_message_and_clears_only_after_key() {
    let mut out = TextOutput::new();
    out.set_pagination(15, 15);
    out.message(tokens(&[b"before", b"\x0c", b"after"]), b"KEY")
        .unwrap();
    assert!(out.waiting_for_key());
    assert_eq!(row(&out, 1), "Before");
    assert_eq!(
        &out.screen().dos_cells()[3840..3846],
        &[b'K', 7, b'E', 7, b'Y', 7]
    );
    assert!(out.message(tokens(&[b"wrong"]), b"KEY").is_err());
    out.resume_key(b"KEY").unwrap();
    assert!(!out.waiting_for_key());
    assert_eq!(row(&out, 1), "after");
    assert_eq!(out.pagination_remaining(), 15);
    assert_eq!(out.screen().dos_cells()[(19 * 80 + 1) * 2], 0xdb);
    assert_eq!(out.screen().dos_cells()[18 * 160], 0xcc);
    assert!(out.resume_key(b"KEY").is_err());
}
#[test]
fn pagination_suspends_before_newline_without_clearing_the_page() {
    let mut out = TextOutput::new();
    out.set_pagination(1, 3);
    out.message(tokens(&[b"first", b"@", b"second"]), b"KEY")
        .unwrap();
    assert!(out.waiting_for_key());
    assert_eq!(row(&out, 1), "First");
    assert_eq!(row(&out, 2), "");
    assert_eq!(out.pagination_remaining(), 3);
    out.resume_key(b"KEY").unwrap();
    assert_eq!(row(&out, 1), "First");
    assert_eq!(row(&out, 2), "second");
}
#[test]
fn wrapping_moves_whole_word_and_scrolls_only_the_upper_pane() {
    let mut out = TextOutput::new();
    out.message(vec![b"abcdefghij".to_vec(); 7], b"KEY")
        .unwrap();
    out.message(tokens(&[b"last"]), b"KEY").unwrap();
    assert_eq!(
        row(&out, 1),
        "Abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij"
    );
    assert_eq!(row(&out, 2), "last");
    for _ in 0..20 {
        out.message(tokens(&[b"@", b"x"]), b"KEY").unwrap();
    }
    assert_eq!(row(&out, 17), "x");
    assert_eq!(out.screen().dos_cells()[18 * 160], 0xcc);
    assert_eq!(out.screen().dos_cells()[(19 * 80 + 1) * 2], 0xdb);
}
#[test]
fn sentence_separator_does_not_indent_after_wrapping() {
    let mut out = TextOutput::new();
    let mut words = vec![b"abcdefghij".to_vec(); 6];
    words.extend(tokens(&[b"abcdefgh", b".", b"next"]));
    out.message(words, b"KEY").unwrap();
    assert_eq!(row(&out, 2), "Next");
}
#[test]
fn invalid_public_tokens_fail_without_changing_the_screen() {
    let mut out = TextOutput::new();
    let before = out.screen().dos_cells().to_vec();
    assert!(out.message(vec![vec![b'x'; 40]], b"KEY").is_err());
    assert!(out.message(tokens(&[b"bad\0word"]), b"KEY").is_err());
    assert_eq!(out.screen().dos_cells(), before);
    assert_eq!(out.screen().unicode().lines().count(), 25);
    out.message(tokens(&[b"one", b"", b"two"]), b"KEY").unwrap();
    assert_eq!(row(&out, 1), "One  two");
}

#[test]
fn object_name_uses_big_endian_phrase_addresses_and_first_separator() {
    let codec = TextCodec::from_dos_executable(&executable()).unwrap();
    let mut bytes = story();
    put(&mut bytes, 0xf0, 0x80);
    put(&mut bytes, 0x102, 1);
    put(&mut bytes, 512 + 0x80, 0);
    put(&mut bytes, 512 + 0x82, 5);
    bytes[2 * 512..2 * 512 + 5].copy_from_slice(&[0, 2, 0xff, 0xff, 0xff]);
    let image = StoryImage::from_dos_obj(bytes).unwrap();
    assert_eq!(
        codec.object_words(&image, ObjectId(1)).unwrap(),
        tokens(&[b"abxyzqr"])
    );
    assert!(
        codec
            .object_words(&image, ObjectId(0xffff))
            .unwrap()
            .is_empty()
    );
    assert!(codec.object_words(&image, ObjectId(2)).is_err());
}

#[test]
fn native_input_token_can_exceed_dictionary_word_limit_without_splitting() {
    let mut output = TextOutput::new();
    let token = format!("{} {}\r", "a".repeat(35), "b".repeat(35));
    output.native_token(token.as_bytes(), b"KEY").unwrap();
    assert_eq!(
        row(&output, 1),
        format!("A{} {}", "a".repeat(34), "b".repeat(35))
    );
    assert!(output.native_token(&[b'x'; 78], b"KEY").is_err());
}

#[test]
fn keyboard_polling_exposes_each_wrap_erase_newline_and_replay() {
    // The next word begins in column77, wraps, is erased there and replayed
    // on the next row. Every native upper-pane write must service input.
    let words = vec![vec![b'a'; 37], vec![b'b'; 37], b"cdef".to_vec()];
    let mut baseline = TextOutput::new();
    baseline.message(words.clone(), b"KEY").unwrap();

    let mut output = TextOutput::new();
    output.enable_keyboard_polling();
    output.message(words, b"KEY").unwrap();
    let mut boundaries = Vec::new();
    while output.keyboard_poll_pending() {
        boundaries.push((
            output.screen().upper_cursor(),
            output.screen().dos_cells()[(80 + 77) * 2],
        ));
        assert!(boundaries.len() <= 83, "output failed to finish");
        output.resume_keyboard_poll(b"KEY");
    }
    assert_eq!(boundaries.len(), 83);
    assert_eq!(boundaries[76], ((1, 78), b'c'));
    assert_eq!(boundaries[77], ((1, 77), b' '));
    assert_eq!(boundaries[78].0, (2, 1));
    assert_eq!(boundaries[79].0, (2, 2));
    assert_eq!(
        row(&output, 1),
        format!("A{} {}", "a".repeat(36), "b".repeat(37))
    );
    assert_eq!(row(&output, 2), "cdef");
    assert_eq!(output.screen().dos_cells(), baseline.screen().dos_cells());
    assert_eq!(
        output.take_transcript_events().collect::<Vec<_>>(),
        baseline.take_transcript_events().collect::<Vec<_>>()
    );
}

#[test]
fn keyboard_boundaries_and_pagination_wait_resume_independently() {
    let mut output = TextOutput::new();
    output.enable_keyboard_polling();
    output.set_pagination(1, 3);
    output.message(tokens(&[b"a", b"@", b"b"]), b"KEY").unwrap();
    assert!(output.keyboard_poll_pending());
    assert!(!output.waiting_for_key());
    assert_eq!(row(&output, 1), "A");
    output.resume_keyboard_poll(b"KEY");
    assert!(!output.keyboard_poll_pending());
    assert!(output.waiting_for_key());
    assert_eq!(row(&output, 2), "");

    output.resume_key(b"KEY").unwrap();
    assert!(!output.waiting_for_key());
    assert!(output.keyboard_poll_pending()); // newline, before the next letter
    assert_eq!(output.screen().upper_cursor(), (2, 1));
    output.resume_keyboard_poll(b"KEY");
    assert!(output.keyboard_poll_pending());
    assert_eq!(row(&output, 2), "b");
    output.resume_keyboard_poll(b"KEY");
    assert!(!output.keyboard_poll_pending());
    assert!(!output.waiting_for_key());
}
