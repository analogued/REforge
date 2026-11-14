// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later
//! Authored fixtures exercise stateful decoding without original game data.
use reforge_hnm::{Canvas, HEIGHT, Hnm1, Palette, Video, WIDTH};
use std::io::Cursor;

fn movie(palette: &[u8], frames: &[Vec<u8>], pad: bool) -> Vec<u8> {
    let header_len = 2 + palette.len() + usize::from(pad) + 4 * (frames.len() + 1);
    let mut bytes = (header_len as u16).to_le_bytes().to_vec();
    bytes.extend(palette);
    if pad {
        bytes.push(255);
    }
    let mut offset = 0u32;
    bytes.extend(offset.to_le_bytes());
    for body in frames {
        offset += (body.len() + 2) as u32;
        bytes.extend(offset.to_le_bytes());
    }
    for body in frames {
        bytes.extend(((body.len() + 2) as u16).to_le_bytes());
        bytes.extend(body);
    }
    bytes
}

fn raw(width: u16, height: u8, mode: u8, x: u16, y: u16, pixels: &[u8]) -> Vec<u8> {
    let mut frame = width.to_le_bytes().to_vec();
    frame.extend([height, mode]);
    frame.extend(x.to_le_bytes());
    frame.extend(y.to_le_bytes());
    frame.extend(pixels);
    frame
}

fn typed(tag: &[u8; 2], data: &[u8]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    bytes.extend(((data.len() + 4) as u16).to_le_bytes());
    bytes.extend(data);
    bytes
}

fn literals(data: &[u8]) -> Vec<u8> {
    assert!(data.len() < 14);
    let flags = ((1u16 << data.len()) - 1) | (1 << (data.len() + 1));
    let mut body = flags.to_le_bytes().to_vec();
    body.extend(data);
    body.extend([0, 0, 0]);
    let mut packet = (data.len() as u16).to_le_bytes().to_vec();
    packet.push(0);
    packet.extend(((body.len() + 6) as u16).to_le_bytes());
    packet.push(171u8.wrapping_sub(packet.iter().fold(0u8, |a, b| a.wrapping_add(*b))));
    packet.extend(body);
    packet
}

#[test]
fn ordered_palette_updates_metadata_and_inheritance_survive_rewind() {
    let mut first = typed(b"sd", b"VOC fragment");
    first.extend(typed(b"pl", &[1, 1, 11, 12, 13, 255, 255, 255]));
    first.extend(typed(b"pt", &[17]));
    first.extend(raw(2, 1, 255, 4, 5, &[1, 2]));
    let mut second = typed(b"kl", &[18]);
    second.extend([0, 0, 0, 255]);
    let bytes = movie(&[2, 1, 20, 21, 22, 255, 255], &[first, second], true);
    let mut inherited = Palette::default();
    inherited.colors[9] = [3, 4, 5];
    inherited.defined[9] = true;
    let mut decoder = Hnm1::new(Cursor::new(bytes), inherited).unwrap();
    assert_eq!(decoder.frame_count(), 2);
    assert!(!decoder.palette().defined[0]);
    assert_eq!(decoder.palette().colors[2], [20, 21, 22]);
    let first = decoder.next_frame().unwrap().unwrap();
    assert_eq!(first.palette.colors[1], [11, 12, 13]);
    assert_eq!(first.palette.colors[9], [3, 4, 5]);
    assert_eq!(
        first.chunks.iter().map(|c| c.tag).collect::<Vec<_>>(),
        [*b"sd", *b"pl", *b"pt"]
    );
    assert_eq!(first.chunks[0].data, b"VOC fragment");
    assert_eq!(first.video.as_ref().unwrap().pixels, [1, 2]);
    let second = decoder.next_frame().unwrap().unwrap();
    assert!(second.video.is_none());
    assert_eq!(second.palette, first.palette);
    assert!(decoder.next_frame().unwrap().is_none());
    decoder.rewind();
    assert!(!decoder.palette().defined[1]);
    assert_eq!(decoder.next_frame().unwrap().unwrap(), first);
}

#[test]
fn compressed_raw_and_row_rle_follow_the_same_coordinates_and_geometry() {
    let mut packed = vec![2, 2, 1, 255]; // 2x1 with HSQ wrapper; coordinates in expansion.
    packed.extend(literals(&[3, 0, 4, 0, 8, 9]));
    let mut rle = vec![3, 0x80, 2, 254]; // 3x2, two row-local runs, no alignment.
    rle.extend([1, 0, 2, 0, 0xfe, 6, 2, 1, 2, 3]);
    let bytes = movie(&[255, 255], &[packed, rle], false);
    let mut decoder = Hnm1::new(Cursor::new(bytes), Palette::default()).unwrap();
    let a = decoder.next_frame().unwrap().unwrap().video.unwrap();
    assert_eq!((a.width, a.height, a.x, a.y), (2, 1, 3, 4));
    assert_eq!(a.pixels, [8, 9]);
    let b = decoder.next_frame().unwrap().unwrap().video.unwrap();
    assert_eq!(b.pixels, [6, 6, 6, 1, 2, 3]);
}

#[test]
fn opaque_overlay_and_direct_updates_preserve_different_background_regions() {
    let mut canvas = Canvas::from_pixels(vec![9; WIDTH * HEIGHT]).unwrap();
    let mut frame = Video {
        width: 2,
        height: 2,
        flags: 0,
        mode: 255,
        x: 1,
        y: 1,
        pixels: vec![0, 2, 3, 0],
    };
    canvas.apply(&frame).unwrap();
    assert_eq!(&canvas.pixels()[321..323], [9, 2]);
    assert_eq!(&canvas.pixels()[641..643], [3, 9]);
    frame.mode = 254;
    canvas.apply(&frame).unwrap();
    assert_eq!(&canvas.pixels()[321..323], [0, 2]);
    frame.flags = 0x600;
    frame.mode = 255;
    canvas.apply(&frame).unwrap();
    assert_eq!(&canvas.pixels()[..5], [0, 2, 3, 0, 9]);
    assert_eq!(canvas.pixels()[321], 0);
}

#[test]
fn direct_compressed_update_decodes_without_coordinates() {
    let mut packet = vec![2, 6, 1, 255];
    packet.extend(literals(&[0, 7]));
    let bytes = movie(&[255, 255], &[packet], false);
    let mut decoder = Hnm1::new(Cursor::new(bytes), Palette::default()).unwrap();
    let update = decoder.next_frame().unwrap().unwrap().video.unwrap();
    assert_eq!((update.x, update.y), (0, 0));
    assert_eq!(update.pixels, [0, 7]);
}

#[test]
fn clipped_composition_and_bad_public_updates_cannot_escape_the_canvas() {
    let mut canvas = Canvas::default();
    let mut frame = Video {
        width: 2,
        height: 2,
        flags: 0,
        mode: 254,
        x: u16::MAX,
        y: 199,
        pixels: vec![1, 2, 3, 4],
    };
    canvas.apply(&frame).unwrap();
    assert_eq!(canvas.pixels()[199 * WIDTH], 2);
    assert_eq!(canvas.pixels().iter().filter(|p| **p != 0).count(), 1);
    let before = canvas.clone();
    frame.pixels.clear();
    assert!(canvas.apply(&frame).is_err());
    assert_eq!(canvas, before);
}

#[test]
fn malformed_indices_palette_lengths_and_frame_chunks_fail_cleanly() {
    let good = movie(&[255, 255], &[raw(2, 1, 255, 0, 0, &[1, 2])], false);
    for end in 0..good.len() {
        assert!(Hnm1::new(Cursor::new(good[..end].to_vec()), Palette::default()).is_err());
    }
    let mut bad = good.clone();
    bad[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert!(Hnm1::new(Cursor::new(bad), Palette::default()).is_err());
    assert!(
        Hnm1::new(
            Cursor::new(movie(&[255, 2, 1, 2, 3], &[vec![0; 4]], false)),
            Palette::default()
        )
        .is_err()
    );
    let mut bad = good.clone();
    let header = u16::from_le_bytes([bad[0], bad[1]]) as usize;
    bad[header] ^= 1;
    let mut decoder = Hnm1::new(Cursor::new(bad), Palette::default()).unwrap();
    assert!(decoder.next_frame().is_err());
    assert!(decoder.next_frame().is_err());
    assert_eq!(decoder.next_index(), 0);
    decoder.rewind();
    assert!(decoder.next_frame().is_err());
    for frame in [
        vec![b's', b'd', 3, 0],
        raw(2, 1, 3, 0, 0, &[1, 2]),
        raw(3, 1, 255, 0, 0, &[1]),
    ] {
        let mut decoder = Hnm1::new(
            Cursor::new(movie(&[255, 255], &[frame], false)),
            Palette::default(),
        )
        .unwrap();
        assert!(decoder.next_frame().is_err());
    }
}

#[test]
fn palette_count_zero_updates_all_colors_and_either_zero_dimension_is_unchanged() {
    let mut palette = vec![0, 0];
    palette.extend((0..768).map(|i| (i % 64) as u8));
    palette.extend([255, 255]);
    let bytes = movie(&palette, &[vec![0, 0, 17, 255], vec![2, 0, 0, 255]], false);
    let mut decoder = Hnm1::new(Cursor::new(bytes), Palette::default()).unwrap();
    assert!(decoder.palette().defined.iter().all(|v| *v));
    assert_eq!(decoder.palette().colors[255], [61, 62, 63]);
    assert!(decoder.next_frame().unwrap().unwrap().video.is_none());
    assert!(decoder.next_frame().unwrap().unwrap().video.is_none());
}

#[test]
fn direct_output_bypasses_the_compositor_rle_flag() {
    let mut packet = vec![2, 0x86, 1, 255];
    packet.extend(literals(&[0, 7]));
    let mut movie = Hnm1::new(
        Cursor::new(movie(&[255, 255], &[packet], false)),
        Palette::default(),
    )
    .unwrap();
    let update = movie.next_frame().unwrap().unwrap().video.unwrap();
    assert_eq!(update.pixels, [0, 7]);
}
