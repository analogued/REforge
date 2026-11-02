// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{image::StoryImage, parser::Parser, text::TextCodec};

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn classes() -> [u8; 128] {
    let mut bytes = [0; 128];
    for byte in b'a'..=b'z' {
        bytes[usize::from(byte)] = 3;
    }
    for byte in b'A'..=b'Z' {
        bytes[usize::from(byte)] = 3;
    }
    for byte in b'0'..=b'9' {
        bytes[usize::from(byte)] = 3;
    }
    for byte in *b"'-" {
        bytes[usize::from(byte)] = 3;
    }
    for byte in [0, b' ', b'\r', b'\n'] {
        bytes[usize::from(byte)] = 4;
    }
    bytes[usize::from(b'!')] = 6;
    bytes
}
fn executable() -> Vec<u8> {
    let mut exe = vec![0; 32 + 0x5ed];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    exe[32 + 0x7f..32 + 0xff].copy_from_slice(&classes());
    exe
}
fn story() -> Vec<u8> {
    let mut bytes = vec![0; 6 * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
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
        put(&mut bytes, 512 + offset, value);
    }
    let dictionary = 4 * 512;
    put(&mut bytes, dictionary, 64); // Only the first three nodes are eligible input.
    for (address, left, right, text) in [
        (2, 32, 64, b"m".as_slice()),
        (32, 0, 0, b"a"),
        (64, 0, 96, b"z"),
        (96, 0, 0, b"zz"),
    ] {
        put(&mut bytes, dictionary + address, left);
        put(&mut bytes, dictionary + address + 2, right);
        bytes[dictionary + address + 4..dictionary + address + 4 + text.len()]
            .copy_from_slice(text);
    }
    bytes
}
fn setup() -> (Parser, TextCodec, StoryImage) {
    let exe = executable();
    (
        Parser::from_dos_executable(&exe).unwrap(),
        TextCodec::from_dos_executable(&exe).unwrap(),
        StoryImage::from_dos_obj(story()).unwrap(),
    )
}

#[test]
fn tokenizer_uses_external_start_and_delimiter_classes() {
    let parser = Parser::from_classes(classes());
    assert_eq!(
        parser.tokenize(b"/m/a! /z 'x'-2\r").unwrap(),
        [
            b"m/a".to_vec(),
            b"!".to_vec(),
            b"z".to_vec(),
            b"'x'-2".to_vec()
        ]
    );
    // Class selection masks bit7, but the byte itself is retained unchanged.
    assert_eq!(
        parser.tokenize(&[b'm' | 0x80, b' ', 0]).unwrap(),
        vec![vec![b'm' | 0x80]]
    );
}

#[test]
fn dictionary_search_is_exact_and_keeps_text_only_hits_distinct_from_misses() {
    let (mut parser, codec, image) = setup();
    assert_eq!(
        parser.dictionary_lookup(&image, &codec, b"a").unwrap(),
        Some(32)
    );
    assert_eq!(
        parser.dictionary_lookup(&image, &codec, b"z").unwrap(),
        Some(64)
    );
    assert_eq!(
        parser.dictionary_lookup(&image, &codec, b"zz").unwrap(),
        Some(96)
    );
    assert_eq!(
        parser.dictionary_lookup(&image, &codec, b"zzx").unwrap(),
        None
    );
    let line = parser.lex_line(&image, &codec, b"M A Z ZZ x\r").unwrap();
    assert_eq!(line.raw, b"M A Z ZZ x\r");
    assert_eq!(line.lowercase, b"m a z zz x\r");
    assert_eq!(line.recognized, [2, 32, 64]);
    assert_eq!(line.token_count, 5);
    assert!(line.unknown);
    assert_eq!(parser.unknown_word(), b"x");
    let line = parser.lex_line(&image, &codec, b"zz\r").unwrap();
    assert!(!line.unknown);
    assert!(line.recognized.is_empty());
    assert_eq!(line.token_count, 1);
    assert_eq!(parser.unknown_word(), b"x");
}

#[test]
fn last_unknown_token_wins_without_stopping_later_recognition() {
    let (mut parser, codec, image) = setup();
    let line = parser
        .lex_line(&image, &codec, b"unknown m other a\r")
        .unwrap();
    assert_eq!(line.recognized, [2, 32]);
    assert_eq!(line.token_count, 4);
    assert_eq!(parser.unknown_word(), b"other");
}

#[test]
fn completed_input_requires_cr_and_stops_copying_at_first_cr() {
    let (mut parser, codec, image) = setup();
    assert!(
        parser
            .lex_line(&image, &codec, b"m")
            .unwrap()
            .raw
            .is_empty()
    );
    let line = parser.lex_line(&image, &codec, b"M\rgarbage").unwrap();
    assert_eq!(line.raw, b"M\r");
    assert_eq!(line.lowercase, b"m\r");
    assert_eq!(line.recognized, [2]);
    assert!(parser.lex_line(&image, &codec, &[b'x'; 78]).is_err());
}

#[test]
fn c_terminators_end_lexical_input_even_before_submitted_cr() {
    let (mut parser, codec, image) = setup();
    let line = parser.lex_line(&image, &codec, b"M\0 A\r").unwrap();
    assert_eq!(line.raw, b"M\0 A\r");
    assert_eq!(line.recognized, [2]);
    assert_eq!(parser.last_input(), b"M");
    assert_eq!(parser.lower_input(), b"m");
}

#[test]
fn lexical_capacity_counts_only_recognized_dictionary_entries() {
    let (mut parser, codec, image) = setup();
    // Make punctuation a recognized root so 39 entries fit in the native line.
    let mut bytes = story();
    bytes[4 * 512 + 6] = b'!';
    let punctuation = StoryImage::from_dos_obj(bytes).unwrap();
    let mut raw = vec![b'!'; 50];
    raw.push(13);
    let line = parser.lex_line(&punctuation, &codec, &raw).unwrap();
    assert_eq!(line.recognized.len(), 39);
    assert_eq!(line.token_count, 39);
    assert_eq!(line.tokens.len(), 39);
    // Unknown punctuation never fills the recognized array.
    let line = parser.lex_line(&image, &codec, &raw).unwrap();
    assert_eq!(line.recognized.len(), 0);
    assert_eq!(line.token_count, 50);
}

#[test]
fn malformed_classes_and_dictionary_cycles_are_checked() {
    let (parser, codec, _) = setup();
    let mut bytes = story();
    put(&mut bytes, 4 * 512 + 2, 2);
    let image = StoryImage::from_dos_obj(bytes).unwrap();
    assert!(parser.dictionary_lookup(&image, &codec, b"b").is_err());
    let mut bad_classes = classes();
    bad_classes[0] = 0;
    let parser = Parser::from_classes(bad_classes);
    assert!(parser.tokenize(b"abc").is_err());
    assert!(Parser::from_dos_executable(b"MZ").is_err());
}
