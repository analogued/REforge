// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{
    ProgramAddr,
    bookmark::{Bookmark, HEADER_BYTES},
    vm::VmState,
};

fn state() -> VmState {
    VmState {
        ip: ProgramAddr(0x1234),
        argument: 0x5678,
        operands: [9, 10, 11],
        variables: (0..512).map(|i| (i % 251) as u8).collect(),
        returns: vec![0x1010, 0x2020],
        values: vec![0xabcd, 0x1234, 0xffff],
    }
}
fn bookmark() -> Bookmark {
    let mut history = [0xffff; 100];
    history[1] = 7;
    Bookmark::from_vm(&state(), history, 1, 3).unwrap()
}
fn at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn fixed_dos_field_order_and_word_endianness() {
    let book = bookmark();
    let bytes = book.encode().unwrap();
    assert_eq!(bytes.len(), HEADER_BYTES + 512);
    assert_eq!(at(&bytes, 0), 512);
    assert_eq!(&bytes[2..8], &[0xcd, 0xab, 0x34, 0x12, 0xff, 0xff]);
    assert_eq!(at(&bytes, 258), 3); // value depth
    assert_eq!(at(&bytes, 260), 0x1010); // first return
    assert_eq!(at(&bytes, 516), 2); // return depth
    assert_eq!(at(&bytes, 518), 1); // history head
    assert_eq!(at(&bytes, 520), 0xffff);
    assert_eq!(at(&bytes, 522), 7);
    assert_eq!(at(&bytes, 720), 3); // selected physical disk
    assert_eq!(&bytes[722..], state().variables);
    assert_eq!(Bookmark::decode(&bytes, 512).unwrap(), book);
}
#[test]
fn inactive_slots_are_preserved_on_decode_but_zero_when_constructed() {
    let mut bytes = bookmark().encode().unwrap();
    put(&mut bytes, 2 + 127 * 2, 0xdead);
    put(&mut bytes, 260 + 127 * 2, 0xbeef);
    let restored = Bookmark::decode(&bytes, 512).unwrap();
    assert_eq!(restored.values().unwrap(), [0xabcd, 0x1234, 0xffff]);
    assert_eq!(restored.returns().unwrap(), [0x1010, 0x2020]);
    assert_eq!(restored.encode().unwrap(), bytes);
    let clean = bookmark();
    assert!(clean.value_slots[3..].iter().all(|&x| x == 0));
    assert!(clean.return_slots[2..].iter().all(|&x| x == 0));
}
#[test]
fn instruction_pointer_operands_and_argument_are_not_serialized() {
    let a = state();
    let mut b = a.clone();
    b.ip = ProgramAddr(2);
    b.argument = 3;
    b.operands = [4, 5, 6];
    let history = [0xffff; 100];
    assert_eq!(
        Bookmark::from_vm(&a, history, 0, 1)
            .unwrap()
            .encode()
            .unwrap(),
        Bookmark::from_vm(&b, history, 0, 1)
            .unwrap()
            .encode()
            .unwrap()
    );
}
#[test]
fn all_truncations_and_trailing_bytes_fail_before_restore() {
    let bytes = bookmark().encode().unwrap();
    for len in 0..bytes.len() {
        assert!(
            Bookmark::decode(&bytes[..len], 512).is_err(),
            "length {len}"
        );
    }
    let mut extra = bytes;
    extra.push(0);
    assert!(Bookmark::decode(&extra, 512).is_err());
}
#[test]
fn extent_depth_and_history_bounds_are_checked() {
    let bytes = bookmark().encode().unwrap();
    assert!(Bookmark::decode(&bytes, 514).is_err());
    for extent in [0, 1, 0x113, 0x115, 65536] {
        assert!(Bookmark::decode(&bytes, extent).is_err());
    }
    for (offset, value) in [(0, 511), (258, 129), (516, 129), (518, 100)] {
        let mut broken = bytes.clone();
        put(&mut broken, offset, value);
        assert!(Bookmark::decode(&broken, 512).is_err());
    }
    let mut book = bookmark();
    book.value_depth = 129;
    assert!(book.encode().is_err());
    assert!(book.values().is_err());
    book = bookmark();
    book.return_depth = 129;
    assert!(book.encode().is_err());
    assert!(book.returns().is_err());
    book = bookmark();
    book.history_head = 100;
    assert!(book.encode().is_err());
}
#[test]
fn full_native_capacities_are_accepted_without_overflow() {
    let mut s = state();
    s.returns = vec![1; 128];
    s.values = vec![2; 128];
    let book = Bookmark::from_vm(&s, [0xffff; 100], 99, 0xffff).unwrap();
    assert_eq!(
        Bookmark::decode(&book.encode().unwrap(), 512).unwrap(),
        book
    );
    s.values.push(3);
    assert!(Bookmark::from_vm(&s, [0xffff; 100], 0, 1).is_err());
}
