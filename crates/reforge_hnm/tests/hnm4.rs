// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Cursor;

use reforge_hnm::{Hnm4, Palette};

fn chunk(tag: &[u8; 2], data: &[u8]) -> Vec<u8> {
    let mut out = ((8 + data.len()) as u32).to_le_bytes().to_vec();
    out.extend(tag);
    out.extend([0; 2]);
    out.extend(data);
    out
}

fn frame(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = ((4 + chunks.iter().map(Vec::len).sum::<usize>()) as u32)
        .to_le_bytes()
        .to_vec();
    for chunk in chunks {
        out.extend(chunk);
    }
    out
}

fn movie(frames: &[Vec<u8>]) -> Vec<u8> {
    let len = (64 + frames.iter().map(Vec::len).sum::<usize>()) as u32;
    let mut header = [0; 64];
    header[..4].copy_from_slice(b"HNM4");
    header[8..12].copy_from_slice(&[4, 0, 4, 0]);
    header[12..16].copy_from_slice(&len.to_le_bytes());
    header[16..20].copy_from_slice(&(frames.len() as u32).to_le_bytes());
    header[28..32].copy_from_slice(&16u32.to_le_bytes());
    let mut out = Vec::from(header);
    for frame in frames {
        out.extend(frame);
    }
    out
}

fn keyframe() -> Vec<u8> {
    let mut data = Vec::from([4, 0, 4, 0xfe]);
    // 16 literal bits, then a long-copy end marker.
    data.extend(0xffff_4000u32.to_le_bytes());
    data.extend(0..16);
    data.extend([0, 0, 0]);
    chunk(b"IZ", &data)
}

#[test]
fn hnm4_raster_order_and_double_buffered_interframes() {
    let movie = movie(&[
        frame(&[chunk(b"PL", &[1, 1, 9, 8, 7, 255, 255]), keyframe()]),
        frame(&[chunk(
            b"IU",
            &[
                0x00, 17, 18, // Literal.
                0x20, 3, // Leave offsets 2..8 unchanged.
                0x42, 0x00, 0x80, // Two backline pairs in normal order.
                0x41, 0x01, 0x80, // Backline pair with its pixels swapped.
                0x80,
            ],
        )]),
        frame(&[chunk(b"IU", &[0x20, 1, 0x21, 0xfe, 0x7f, 0x80])]),
    ]);
    let mut inherited = Palette::default();
    inherited.colors[2] = [1, 2, 3];
    inherited.defined[2] = true;
    let mut decoder = Hnm4::new(Cursor::new(movie), inherited.clone()).unwrap();
    assert_eq!(
        (decoder.width(), decoder.height(), decoder.frame_count()),
        (4, 4, 3)
    );
    let first = decoder.next_frame().unwrap().unwrap();
    assert_eq!(first.index, 0);
    assert_eq!(
        first.pixels,
        [0, 2, 4, 6, 1, 3, 5, 7, 8, 10, 12, 14, 9, 11, 13, 15]
    );
    assert_eq!(first.palette.colors[1], [9, 8, 7]);
    assert_eq!(first.palette.colors[2], [1, 2, 3]);
    let second = decoder.next_frame().unwrap().unwrap();
    assert_eq!(
        second.pixels,
        [17, 2, 4, 6, 18, 3, 5, 7, 18, 3, 12, 14, 8, 10, 5, 15]
    );
    let third = decoder.next_frame().unwrap().unwrap();
    assert_eq!(
        third.pixels,
        [0, 17, 4, 6, 1, 18, 5, 7, 8, 10, 12, 14, 9, 11, 13, 15]
    );
    assert!(decoder.next_frame().unwrap().is_none());

    decoder.rewind();
    assert_eq!(decoder.next_index(), 0);
    assert_eq!(decoder.palette(), &inherited);
    assert_eq!(
        decoder.next_frame().unwrap().unwrap().palette.colors[1],
        [9, 8, 7]
    );
}

#[test]
fn hnm4_rejects_invalid_packets_without_advancing() {
    for bad in [
        // Truncated long skip, invalid source offset, and no terminator.
        vec![0x40, 1],
        vec![0x01, 0, 0, 0x80],
        vec![0x00, 1, 2],
    ] {
        let movie = movie(&[frame(&[keyframe()]), frame(&[chunk(b"IU", &bad)])]);
        let mut decoder = Hnm4::new(Cursor::new(movie), Palette::default()).unwrap();
        assert!(decoder.next_frame().unwrap().is_some());
        assert!(decoder.next_frame().is_err());
        assert_eq!(decoder.next_index(), 1);
        assert!(decoder.next_frame().is_err());
        decoder.rewind();
        assert!(decoder.next_frame().unwrap().is_some());
    }
}

#[test]
fn hnm4_padded_allocation_and_partial_keyframe_follow_raster_geometry() {
    let mut partial = vec![4, 0, 2, 0xfe];
    // Eight literal bits, then the end marker.
    partial.extend(0xff40_0000u32.to_le_bytes());
    partial.extend(32..40);
    partial.extend([0, 0, 0]);
    let mut movie = movie(&[
        frame(&[keyframe()]),
        frame(&[chunk(b"IZ", &partial)]),
        frame(&[chunk(b"IU", &[0x20, 7, 0x60, 2, 15, 0x80])]),
    ]);
    movie[28..32].copy_from_slice(&24u32.to_le_bytes());
    let mut decoder = Hnm4::new(Cursor::new(&movie), Palette::default()).unwrap();
    decoder.next_frame().unwrap().unwrap();
    assert_eq!(
        decoder.next_frame().unwrap().unwrap().pixels,
        [32, 34, 36, 38, 33, 35, 37, 39, 8, 10, 12, 14, 9, 11, 13, 15]
    );
    assert_eq!(
        decoder.next_frame().unwrap().unwrap().pixels,
        [32, 34, 36, 38, 33, 35, 37, 39, 8, 10, 12, 15, 9, 11, 13, 15]
    );
    assert!(decoder.next_frame().unwrap().is_none());

    movie[28..32].copy_from_slice(&8u32.to_le_bytes());
    assert!(Hnm4::new(Cursor::new(movie), Palette::default()).is_err());
}
