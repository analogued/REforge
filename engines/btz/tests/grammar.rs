// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{ObjectId, VarAddr, image::StoryImage, parser::grammar::GrammarState, vm::Vm};

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn fixture(phrases: &[Vec<Vec<u16>>], flags: &[u16]) -> (StoryImage, Vm) {
    let mut encoded = Vec::new();
    let mut ends = Vec::new();
    for patterns in phrases {
        for pattern in patterns {
            for reference in pattern {
                encoded.extend(reference.to_be_bytes());
            }
            encoded.push(0xff);
        }
        ends.push(encoded.len() as u16);
    }
    let grammar_blocks = encoded.len().div_ceil(512).max(1) as u16;
    let mut bytes = vec![0; usize::from(6 + grammar_blocks) * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
        (0xe8, 0x40),
        (0xe2, 6),
        (0xe4, 7),
        (0xe6, 5),
        (0x102, phrases.len() as u16 - 1),
        (0x10c, 0x120),
        (0xee, 0x80),
        (0x10e, 0x140),
        (0xe0, 0x200),
        (0xf0, 0x100),
        (0xf6, 0x180),
        (0xf8, 0x190),
        (0xfa, phrases.len() as u16 * 2),
        (0x110, 0x1a0),
    ] {
        put(&mut bytes, offset, value);
    }
    for (offset, value) in [
        (4, 1),
        (14, 3),
        (12, 3 + grammar_blocks),
        (8, 4 + grammar_blocks),
        (10, 5 + grammar_blocks),
        (16, 3),
        (26, 3 + grammar_blocks),
        (24, 4 + grammar_blocks),
        (20, 5 + grammar_blocks),
        (22, 6 + grammar_blocks),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    put(&mut bytes, 512 + 0xfe, 0);
    for (object, &end) in ends.iter().enumerate() {
        put(&mut bytes, 512 + 0x100 + object * 2, end);
        put(&mut bytes, 0x180 + object * 2, object as u16);
        put(&mut bytes, 0x190 + object * 2, object as u16);
        put(
            &mut bytes,
            0x1a0 + object * 2,
            flags.get(object).copied().unwrap_or(0),
        );
    }
    bytes[3 * 512..3 * 512 + encoded.len()].copy_from_slice(&encoded);
    let image = StoryImage::from_dos_obj(bytes).unwrap();
    let vm = Vm::new(image.clone()).unwrap();
    (image, vm)
}
fn one(words: &[u16]) -> Vec<Vec<u16>> {
    vec![words.to_vec()]
}

#[test]
fn big_endian_phrase_words_become_ordered_object_output() {
    let (image, mut vm) = fixture(
        &[one(&[0x1234]), one(&[0x2345, 0x3456]), one(&[0x3456])],
        &[0, 0, 0],
    );
    let mut grammar = GrammarState::new();
    let result = grammar
        .parse(&image, &mut vm, &[0x1234, 0x2345, 0x3456])
        .unwrap();
    assert_eq!(result.output, [0, 1]);
    assert_eq!(result.objects, [0, 1, 0xffff]);
    assert_eq!(result.used, [1, 1, 1]);
    assert_eq!(result.remaining_count, 0);
}
#[test]
fn longer_phrase_displaces_a_previously_selected_single_word() {
    let (image, mut vm) = fixture(&[one(&[20]), one(&[10, 20])], &[0, 0]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20])
        .unwrap();
    assert_eq!(result.output, [1]);
    assert_eq!(result.objects, [1, 0xffff]);
}
#[test]
fn gap_flag_keeps_independent_objects_inside_the_gap() {
    let (image, mut vm) = fixture(&[one(&[20]), one(&[10, 30])], &[0, 2]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 30])
        .unwrap();
    assert_eq!(result.output, [1, 0]);
    assert_eq!(result.objects, [1, 0, 0xffff]);
    assert_eq!(result.remaining_count, 0);
    let (image, mut vm) = fixture(&[one(&[20]), one(&[10, 30])], &[0, 0]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 30])
        .unwrap();
    assert_eq!(result.output, [0]);
    assert_eq!(result.remaining_count, 2);
}
#[test]
fn free_matches_append_alternatives_without_consuming_tokens() {
    let (image, mut vm) = fixture(&[one(&[10]), one(&[10]), one(&[10])], &[1, 1, 0]);
    let result = GrammarState::new().parse(&image, &mut vm, &[10]).unwrap();
    assert_eq!(result.output, [2, 0, 1]);
    assert_eq!(result.alternatives, [(0, 0), (0, 1)]);
    assert_eq!(result.used, [1]);
}
#[test]
fn remembered_objects_take_priority_and_ring_retains_a_terminator() {
    let (image, mut vm) = fixture(&[one(&[10]), one(&[10])], &[0, 0]);
    let mut grammar = GrammarState::new();
    assert_eq!(&grammar.history()[..2], &[0xffff, 0xffff]);
    assert!(grammar.history()[2..].iter().all(|&object| object == 0));
    grammar.remember_object(ObjectId(1), 1).unwrap();
    assert_eq!(grammar.parse(&image, &mut vm, &[10]).unwrap().output, [1]);
    for _ in 0..105 {
        grammar.remember_object(ObjectId(0), 1).unwrap();
    }
    assert_eq!(
        grammar.history().iter().filter(|&&o| o == 0xffff).count(),
        1
    );
    assert_eq!(
        grammar.history()[(grammar.history_head() + 1) % 100],
        0xffff
    );
    let head = grammar.history_head();
    grammar.remember_object(ObjectId(0xffff), 1).unwrap();
    assert_eq!(grammar.history_head(), head);
    assert!(grammar.remember_object(ObjectId(2), 1).is_err());
    let inactive = grammar.history()[2..].to_vec();
    grammar.reset();
    assert_eq!(grammar.history_head(), 0);
    assert_eq!(&grammar.history()[..2], &[0xffff, 0xffff]);
    assert_eq!(&grammar.history()[2..], inactive);
}
#[test]
fn alternate_phrases_and_repeated_occurrences_keep_native_order() {
    let (image, mut vm) = fixture(&[vec![vec![10], vec![20]]], &[0]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 10])
        .unwrap();
    assert_eq!(result.output, [0, 0, 0]);
    assert_eq!(result.used, [1, 1, 1]);
    assert_eq!(result.remaining_count, 0);
}
#[test]
fn empty_and_unmatched_input_preserve_vm_registers() {
    let (image, mut vm) = fixture(&[one(&[10])], &[0]);
    vm.write_variable(VarAddr(0xce), 123).unwrap();
    let before = vm.state().clone();
    let mut grammar = GrammarState::new();
    assert!(
        grammar
            .parse(&image, &mut vm, &[])
            .unwrap()
            .output
            .is_empty()
    );
    let result = grammar.parse(&image, &mut vm, &[20]).unwrap();
    assert!(result.output.is_empty());
    assert_eq!(result.remaining_count, 1);
    assert_eq!(vm.state(), &before);
    assert!(grammar.parse(&image, &mut vm, &[0xffff]).is_err());
    assert!(grammar.parse(&image, &mut vm, &[10; 40]).is_err());
}
#[test]
fn crossing_spans_are_rejected_but_contiguous_covering_spans_replace_gaps() {
    let (image, mut vm) = fixture(&[one(&[10, 30]), one(&[20, 40])], &[2, 2]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 30, 40])
        .unwrap();
    assert_eq!(result.output, [0]);
    assert_eq!(result.used, [1, 0, 1, 0]);
    assert_eq!(result.remaining_count, 2);
    let (image, mut vm) = fixture(&[one(&[10, 30]), one(&[10, 20, 30])], &[2, 0]);
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 30])
        .unwrap();
    assert_eq!(result.output, [1]);
    assert_eq!(result.used, [1, 1, 1]);
}
#[test]
fn a_contained_subphrase_does_not_replace_an_existing_longer_match() {
    let (image, mut vm) = fixture(&[one(&[10, 20, 30]), one(&[20])], &[0, 0]);
    // Keep a token unmatched so that the full scan reaches the second object.
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 20, 30, 40])
        .unwrap();
    assert_eq!(result.output, [0]);
    assert_eq!(result.remaining_count, 1);
}
#[test]
fn gap_prefix_state_persists_for_later_occurrences_in_the_same_phrase() {
    let (image, mut vm) = fixture(&[one(&[10, 30]), one(&[10, 30])], &[2, 0]);
    // The native prefix flag is reset per alternate phrase, rather than per
    // occurrence. After the first gapped occurrence, a later contiguous one
    // still carries that prefix end and can be replaced by object1.
    let result = GrammarState::new()
        .parse(&image, &mut vm, &[10, 99, 30, 10, 30])
        .unwrap();
    assert_eq!(result.output, [0, 1]);
    assert_eq!(result.objects, [0, 0xffff, 0xffff, 1, 0xffff]);
    assert_eq!(result.remaining_count, 1);
}
#[test]
fn duplicate_free_alternatives_are_preserved_in_phrase_encounter_order() {
    let (image, mut vm) = fixture(&[vec![vec![10], vec![10]]], &[1]);
    let result = GrammarState::new().parse(&image, &mut vm, &[10]).unwrap();
    assert_eq!(result.output, [0, 0]);
    assert_eq!(result.alternatives, [(0, 0), (0, 0)]);
    assert_eq!(result.used, [0]);
    assert_eq!(result.remaining_count, 1);
}
#[test]
fn output_buffer_reserves_a_slot_for_its_terminating_word() {
    let patterns = [one(&[10]), one(&[10]), one(&[10]), one(&[20])];
    let (image, mut vm) = fixture(&patterns, &[1, 1, 1, 0]);
    let mut tokens = vec![10; 22];
    tokens.extend([20; 13]);
    let result = GrammarState::new().parse(&image, &mut vm, &tokens).unwrap();
    assert_eq!(result.output.len(), 79);
    tokens.push(20);
    let error = GrammarState::new()
        .parse(&image, &mut vm, &tokens)
        .unwrap_err();
    assert_eq!(error.0, "parsed output exceeds DOS variable buffer");
}
#[test]
fn phrase_staging_reserves_a_slot_for_its_terminating_word() {
    // Each authored pattern also has one decoded alternate-phrase separator.
    let (image, mut vm) = fixture(&[one(&[10; 398])], &[0]);
    assert!(GrammarState::new().parse(&image, &mut vm, &[20]).is_ok());
    let (image, mut vm) = fixture(&[one(&[10; 399])], &[0]);
    let error = GrammarState::new()
        .parse(&image, &mut vm, &[20])
        .unwrap_err();
    assert_eq!(error.0, "grammar phrase buffer exhausted");
}
