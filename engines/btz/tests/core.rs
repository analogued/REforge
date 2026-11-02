// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::decode::{OperandMode, decode};
use btz::image::StoryImage;
use btz::vm::{Request, Step, Vm};
use btz::{ObjectId, ProgramAddr, VarAddr};

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn fixture(words: &[u16]) -> Vec<u8> {
    let mut bytes = vec![0; 7 * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
        (0xe0, 0x200),
        (0xe2, 6),
        (0xe4, 7),
        (0xe6, 5),
        (0xe8, 0x40),
        (0xee, 0x80),
        (0xf6, 0x180),
        (0xf8, 0x190),
        (0xfa, 4),
        (0x102, 7),
        (0x10c, 0x120),
        (0x10e, 0x140),
    ] {
        put(&mut bytes, offset, value);
    }
    for (offset, value) in [
        (4, 1),
        (8, 3),
        (10, 6),
        (12, 5),
        (14, 4),
        (16, 3),
        (20, 4),
        (22, 7),
        (24, 6),
        (26, 5),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    for (i, word) in words.iter().enumerate() {
        put(&mut bytes, 512 + 0x40 + i * 2, *word);
    }
    bytes
}

fn vm(words: &[u16]) -> Vm {
    Vm::new(StoryImage::from_dos_obj(fixture(words)).unwrap()).unwrap()
}

#[test]
fn decoding_independent_modes_and_cross_page_word() {
    let mut program = vec![0; 519];
    for (i, v) in [0x292c, 0x30, 0x32, 42].into_iter().enumerate() {
        put(&mut program, 511 + i * 2, v);
    }
    let i = decode(&program, ProgramAddr(511)).unwrap();
    assert_eq!(
        i.operands.iter().map(|o| o.mode).collect::<Vec<_>>(),
        [
            OperandMode::Variable,
            OperandMode::IndirectVariable,
            OperandMode::Literal
        ]
    );
    assert_eq!(i.next, ProgramAddr(519));
}

#[test]
fn rejects_truncated_or_unknown_wordcode() {
    for words in [vec![0x002c], vec![0x00fc, 0], vec![0x8000]] {
        let bytes: Vec<_> = words.into_iter().flat_map(u16::to_le_bytes).collect();
        assert!(decode(&bytes, ProgramAddr(0)).is_err());
    }
}

#[test]
fn forms_six_and_seven_decode_without_operand_bytes() {
    for word in [0x082eu16, 0x082f, 0x7f2e, 0x7f2f] {
        let instruction = decode(&word.to_le_bytes(), ProgramAddr(0)).unwrap();
        assert_eq!(instruction.opcode, 0x0b);
        assert_eq!(instruction.form, if word & 1 == 0 { 6 } else { 7 });
        assert!(instruction.operands.is_empty());
        assert_eq!(instruction.next, ProgramAddr(2));
    }
}

#[test]
fn forms_six_and_seven_load_previous_argument_without_consuming_next_instruction() {
    for word in [0x082e, 0x082f] {
        let mut v = vm(&[0x2c, 7, 0x40, 3, word, 0x2c, 0x1234]);
        v.step().unwrap();
        v.step().unwrap();
        assert_eq!(v.accumulator(), 10);
        assert_eq!(v.state().argument, 3);
        let operands = v.state().operands;

        v.step().unwrap();
        assert_eq!(v.accumulator(), 3);
        assert_eq!(v.state().argument, 3);
        assert_eq!(v.state().operands, operands);
        assert_eq!(v.state().ip, ProgramAddr(0x4a));

        v.step().unwrap();
        assert_eq!(v.accumulator(), 0x1234);
        assert_eq!(v.state().ip, ProgramAddr(0x4e));
    }
}

#[test]
fn loader_refuses_missing_physical_bytes() {
    let mut bytes = fixture(&[]);
    bytes.pop();
    assert!(StoryImage::from_dos_obj(bytes).is_err());
}

#[test]
fn wraparound_arithmetic_and_predicates_are_words() {
    let mut v = vm(&[0x2c, 0xffff, 0x40, 2, 0x14, 1, 0x18, 0]);
    v.step().unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 1);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 0);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 0xffff);
}

#[test]
fn reverse_subtract_unsigned_divide_and_modulo() {
    let mut v = vm(&[0x2c, 0xffff, 0x48, 1, 0x50, 3, 0x84, 10]);
    v.step().unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 2);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 0);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 0);
    let mut v = vm(&[0x2c, 0xffff, 0x50, 256, 0x84, 10]);
    v.step().unwrap();
    v.step().unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 5);
}

#[test]
fn division_by_zero_is_an_explicit_fault() {
    assert!(vm(&[0x50, 0]).step().is_err());
}

#[test]
fn call_return_and_value_stack_are_separate() {
    let mut v = vm(&[0x04, 0x48, 0x2c, 99, 0x6c, 7, 0x00, 42]);
    v.step().unwrap();
    assert_eq!(v.state().returns, [0x44]);
    v.step().unwrap();
    assert_eq!(v.state().values, [7]);
    v.step().unwrap();
    assert_eq!(v.state().ip, ProgramAddr(0x44));
    assert_eq!(v.accumulator(), 42);
    assert!(v.state().returns.is_empty());
    assert_eq!(v.state().values, [7]);
}

#[test]
fn special_zero_and_constant_writes() {
    let mut v = vm(&[]);
    v.write_variable(VarAddr(0), 123).unwrap();
    assert_eq!(v.variable(VarAddr(0)).unwrap(), 0xffff);
    assert!(v.write_variable(VarAddr(2), 9).is_err());
    assert!(v.write_variable(VarAddr(0x31), 9).is_err());
    v.write_variable(VarAddr(10), 9).unwrap();
    assert_eq!(v.variable(VarAddr(10)).unwrap(), 9);
}

#[test]
fn each_operand_space_resolves_at_fetch() {
    let mut bytes = fixture(&[0x112c, 0x30, 0x102c, 0x32, 0x012c, 0x34]);
    put(&mut bytes, 0x30, 0x60);
    put(&mut bytes, 512 + 0x60, 123);
    put(&mut bytes, 0x32, 0x36);
    put(&mut bytes, 0x36, 456);
    put(&mut bytes, 0x34, 789);
    let mut v = Vm::new(StoryImage::from_dos_obj(bytes).unwrap()).unwrap();
    for expected in [123, 456, 789] {
        v.step().unwrap();
        assert_eq!(v.accumulator(), expected);
    }
}

#[test]
fn accumulator_form_and_three_operand_form_preserve_abi() {
    let mut v = vm(&[0x2c, 7, 0x43, 0x082c, 11, 12, 13]);
    v.step().unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 14);
    v.step().unwrap();
    assert_eq!(v.state().operands, [11, 12, 13]);
    assert_eq!(v.accumulator(), 7); // argument from preceding add was 7
}

#[test]
fn non_boolean_value_does_not_take_either_conditional() {
    let mut v = vm(&[0x2c, 1, 0x0c, 0x40, 0x10, 0x40]);
    for _ in 0..3 {
        v.step().unwrap();
    }
    assert_eq!(v.state().ip, ProgramAddr(0x4c));
}

#[test]
fn valid_accumulator_branch_does_not_push_return_stack() {
    let mut v = vm(&[0x2c, 7, 0x78, 0x40]);
    v.step().unwrap();
    v.step().unwrap();
    assert_eq!(v.state().ip, ProgramAddr(0x40));
    assert!(v.state().returns.is_empty());
}

#[test]
fn host_request_must_be_completed_and_preserves_accumulator() {
    let mut v = vm(&[0x2c, 55, 0x58, 0x8021, 0x40, 1]);
    v.step().unwrap();
    assert_eq!(
        v.step().unwrap(),
        Step::Suspended(Request::Message { id: 0x8021 })
    );
    assert!(v.step().is_err());
    assert!(v.resume(Some(0)).is_err());
    v.resume(None).unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 56);
}

#[test]
fn random_requires_the_interpreter_algorithm_result() {
    let mut v = vm(&[0x54, 10]);
    assert_eq!(
        v.step().unwrap(),
        Step::Suspended(Request::Random { maximum: 10 })
    );
    assert!(v.resume(None).is_err());
    assert!(v.resume(Some(11)).is_err());
    v.resume(Some(10)).unwrap();
    assert_eq!(v.accumulator(), 10);
}

fn properties(mutable_key: u16, mutable_value: u16) -> Vm {
    let mut bytes = fixture(&[0x2d, 1, 2]); // form 1 property value, object 1/property 2
    // Object 1's ranges: one mutable and one immutable property pair.
    for (offset, value) in [(0x84, 0), (0x86, 0), (0x88, 4), (0x8a, 4)] {
        put(&mut bytes, 512 + offset, value);
    }
    put(&mut bytes, 0x140, mutable_key);
    put(&mut bytes, 0x142, mutable_value);
    put(&mut bytes, 512 + 0x200, 2);
    put(&mut bytes, 512 + 0x202, 222);
    Vm::new(StoryImage::from_dos_obj(bytes).unwrap()).unwrap()
}

#[test]
fn property_prefers_mutable_value_and_returns_writable_slot() {
    let mut v = properties(2, 111);
    assert_eq!(v.property(ObjectId(1), ObjectId(2), false).unwrap(), 111);
    assert_eq!(v.property(ObjectId(1), ObjectId(2), true).unwrap(), 0x142);
    assert!(v.property_found());
}

#[test]
fn immutable_property_value_has_no_writable_address() {
    let mut v = properties(4, 111);
    assert_eq!(v.property(ObjectId(1), ObjectId(2), false).unwrap(), 222);
    assert_eq!(v.property(ObjectId(1), ObjectId(2), true).unwrap(), 0);
    assert!(v.property_found());
    assert_eq!(v.variable(VarAddr(0)).unwrap(), 0xffff);
}

#[test]
fn missing_property_value_and_missing_address_have_different_sentinels() {
    let mut v = properties(4, 111);
    assert_eq!(v.property(ObjectId(1), ObjectId(3), false).unwrap(), 0xffff);
    assert!(!v.property_found());
    assert_eq!(v.property(ObjectId(1), ObjectId(3), true).unwrap(), 0);
    assert!(!v.property_found());
}

#[test]
fn property_value_form_suspends_before_native_input_poll() {
    let mut v = properties(2, 111);
    assert_eq!(v.step().unwrap(), Step::Suspended(Request::PollInput));
    assert!(v.resume(Some(65)).is_err());
    v.resume(None).unwrap();
    v.step().unwrap();
    assert_eq!(v.accumulator(), 111);
}

#[test]
fn property_address_form_and_interval_search() {
    let mut bytes = fixture(&[0x082d, 1, 2, 0x74, 15, 0x74, 35]);
    for (offset, value) in [(0x84, 0), (0x86, 0), (0x88, 4), (0x8a, 0)] {
        put(&mut bytes, 512 + offset, value);
    }
    for (offset, value) in [
        (0x140, 2),
        (0x142, 111),
        (0x180, 10),
        (0x182, 30),
        (0x190, 20),
        (0x192, 40),
    ] {
        put(&mut bytes, offset, value);
    }
    let mut v = Vm::new(StoryImage::from_dos_obj(bytes).unwrap()).unwrap();
    assert!(matches!(v.step().unwrap(), Step::Executed { .. }));
    assert_eq!(v.accumulator(), 0x142);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 0);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 2);
}

#[test]
fn direct_property_and_next_related_object() {
    let mut bytes = fixture(&[0x2e, 11, 0]); // form 2: find next whose direct value is 11
    put(&mut bytes, 0x122, 44);
    put(&mut bytes, 0x124, 11);
    let mut v = Vm::new(StoryImage::from_dos_obj(bytes).unwrap()).unwrap();
    assert_eq!(v.property(ObjectId(1), ObjectId(6), false).unwrap(), 44);
    assert_eq!(v.property(ObjectId(1), ObjectId(7), true).unwrap(), 0x122);
    v.step().unwrap();
    assert_eq!(v.accumulator(), 2);
}

#[test]
fn fallback_chain_and_cycle_detection() {
    let mut bytes = fixture(&[]);
    // Object 1 and 2 each have a single mutable pair, with no program pairs.
    for (offset, value) in [
        (0x84, 0),
        (0x86, 0),
        (0x88, 4),
        (0x8a, 0),
        (0x8c, 8),
        (0x8e, 0),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    for (offset, value) in [(0x140, 5), (0x142, 2), (0x144, 2), (0x146, 333)] {
        put(&mut bytes, offset, value);
    }
    let mut v = Vm::new(StoryImage::from_dos_obj(bytes.clone()).unwrap()).unwrap();
    assert_eq!(v.property(ObjectId(1), ObjectId(2), false).unwrap(), 333);
    put(&mut bytes, 0x144, 5);
    put(&mut bytes, 0x146, 1);
    let mut v = Vm::new(StoryImage::from_dos_obj(bytes).unwrap()).unwrap();
    assert!(v.property(ObjectId(1), ObjectId(2), false).is_err());
}

#[test]
fn empty_stacks_and_invalid_property_ids_fault_explicitly() {
    assert!(vm(&[0x00, 7]).step().is_err());
    assert!(vm(&[0x70, 0x30]).step().is_err());
    assert!(vm(&[]).property(ObjectId(8), ObjectId(2), false).is_err());
}
