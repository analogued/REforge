// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::{image::StoryImage, media::TextStores};
fn put(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn primary() -> Vec<u8> {
    let mut bytes = vec![0; 7 * 512];
    put(&mut bytes, 0, 512);
    for (at, value) in [
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
        put(&mut bytes, 512 + at, value);
    }
    bytes[4 * 512..5 * 512].fill(0x22);
    bytes[5 * 512..6 * 512].fill(0x33);
    bytes[6 * 512..7 * 512].fill(0x44);
    bytes
}
fn secondary() -> Vec<u8> {
    let mut bytes = vec![0; 4 * 512];
    for (at, value) in [(12, 1), (24, 2), (8, 2), (20, 3), (10, 3), (22, 4)] {
        put(&mut bytes, at, value);
    }
    bytes[512..1024].fill(0x52);
    bytes[1024..1536].fill(0x53);
    bytes[1536..2048].fill(0x54);
    bytes
}
#[test]
fn primary_media_extracts_text_without_exposing_wordcode_or_initial_variables() {
    let bytes = primary();
    let story = StoryImage::from_dos_obj(bytes.clone()).unwrap();
    let stores = TextStores::from_primary_dos_obj(bytes).unwrap();
    assert_eq!(stores, TextStores::from_story_image(&story).unwrap());
    for (id, byte) in [(2, 0x22), (3, 0x33), (4, 0x44)] {
        assert_eq!(stores.store(id).unwrap(), &[byte; 512]);
    }
    for id in [0, 1, 5, usize::MAX] {
        assert!(stores.store(id).is_err());
    }
}
#[test]
fn secondary_media_reads_its_directory_at_block_zero_without_a_variable_header() {
    let stores = TextStores::from_secondary_dos_obj(secondary()).unwrap();
    for (id, byte) in [(2, 0x52), (3, 0x53), (4, 0x54)] {
        assert_eq!(stores.store(id).unwrap(), &[byte; 512]);
    }
    assert!(StoryImage::from_dos_obj(secondary()).is_err());
}
#[test]
fn secondary_ranges_may_share_bytes_and_leave_unreferenced_blocks() {
    let mut bytes = secondary();
    put(&mut bytes, 10, 2);
    put(&mut bytes, 22, 3);
    bytes.extend([0x77; 512]);
    let stores = TextStores::from_secondary_dos_obj(bytes).unwrap();
    assert_eq!(stores.store(3).unwrap(), stores.store(4).unwrap());
    assert_eq!(stores.store(4).unwrap(), &[0x53; 512]);
}
#[test]
fn malformed_media_extents_fail_without_producing_partial_stores() {
    assert!(TextStores::from_secondary_dos_obj(vec![0; 511]).is_err());
    let mut bytes = secondary();
    put(&mut bytes, 12, 3);
    put(&mut bytes, 24, 2);
    assert!(TextStores::from_secondary_dos_obj(bytes).is_err());
    let mut bytes = secondary();
    put(&mut bytes, 22, 5);
    assert!(TextStores::from_secondary_dos_obj(bytes).is_err());
    let mut bytes = primary();
    bytes.pop();
    assert!(TextStores::from_primary_dos_obj(bytes).is_err());
}
