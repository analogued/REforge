// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Rasterization of the engine's DOS character cells, independent of SDL.

mod ascii;

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 200;

const COLOURS: [[u8; 3]; 16] = [
    [0, 0, 0],
    [0, 0, 170],
    [0, 170, 0],
    [0, 170, 170],
    [170, 0, 0],
    [170, 0, 170],
    [170, 85, 0],
    [170, 170, 170],
    [85, 85, 85],
    [85, 85, 255],
    [85, 255, 85],
    [85, 255, 255],
    [255, 85, 85],
    [255, 85, 255],
    [255, 255, 85],
    [255, 255, 255],
];

/// Background and foreground of the interpreter's ordinary attribute-7 text.
pub const TEXT_PALETTE: [[u8; 3]; 2] = [COLOURS[0], COLOURS[7]];

/// Paint an 80x25 character/attribute buffer. Bit7 of an attribute is the
/// original text-mode blink bit; the caller supplies the current blink phase.
pub fn render(cells: &[u8], blink_on: bool) -> Result<Vec<u8>, String> {
    if cells.len() != 80 * 25 * 2 {
        return Err("BTZ text screen must contain 80 by 25 DOS cells".into());
    }
    let mut pixels = vec![0; WIDTH * HEIGHT * 3];
    for (index, cell) in cells.as_chunks::<2>().0.iter().enumerate() {
        let rows = glyph(cell[0]);
        let foreground = COLOURS[usize::from(cell[1] & 15)];
        let background = COLOURS[usize::from((cell[1] >> 4) & 7)];
        let visible = blink_on || cell[1] & 0x80 == 0;
        for (y, row) in rows.into_iter().enumerate() {
            for x in 0..8 {
                let colour = if visible && row & (1 << x) != 0 {
                    foreground
                } else {
                    background
                };
                let offset = ((index / 80 * 8 + y) * WIDTH + index % 80 * 8 + x) * 3;
                pixels[offset..offset + 3].copy_from_slice(&colour);
            }
        }
    }
    Ok(pixels)
}

fn glyph(character: u8) -> [u8; 8] {
    if let Some(rows) = ascii::glyph(character) {
        return rows;
    }
    // Authored geometric DOS border/block glyphs. No BIOS font rows are used.
    // Pixel bit0 is the left edge, matching the imported ASCII row orientation.
    match character {
        0 | 0xff => [0; 8],
        0xb0 => [0x11, 0x44, 0x11, 0x44, 0x11, 0x44, 0x11, 0x44],
        0xb1 => [0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa],
        0xb2 => [0xee, 0xbb, 0xee, 0xbb, 0xee, 0xbb, 0xee, 0xbb],
        0xb3 => [0x08; 8],
        0xba => [0x24; 8],
        0xc4 => [0, 0, 0, 0xff, 0, 0, 0, 0],
        0xcd => [0, 0, 0xff, 0, 0, 0xff, 0, 0],
        0xc9 => [0, 0, 0xfc, 0x04, 0x04, 0xe4, 0x24, 0x24],
        0xbb => mirror([0, 0, 0xfc, 0x04, 0x04, 0xe4, 0x24, 0x24]),
        0xc8 => [0x24, 0x24, 0xe4, 0x04, 0x04, 0xfc, 0, 0],
        0xbc => mirror([0x24, 0x24, 0xe4, 0x04, 0x04, 0xfc, 0, 0]),
        0xcc => [0x24, 0x24, 0xe4, 0x04, 0x04, 0xe4, 0x24, 0x24],
        0xb9 => mirror([0x24, 0x24, 0xe4, 0x04, 0x04, 0xe4, 0x24, 0x24]),
        0xdb => [0xff; 8],
        0xdc => [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff],
        0xdf => [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0],
        0xdd => [0x0f; 8],
        0xde => [0xf0; 8],
        _ => ascii::glyph(b'?').expect("printable ASCII fallback"),
    }
}

fn mirror(rows: [u8; 8]) -> [u8; 8] {
    rows.map(u8::reverse_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_and_attributes_are_applied_to_every_cell() {
        assert!(render(&[], true).is_err());
        let mut cells = [b' ', 7].repeat(2000);
        cells[..2].copy_from_slice(&[0xdb, 0x1e]);
        cells[3998..].copy_from_slice(&[0xdb, 0x2c]);
        let pixels = render(&cells, true).unwrap();
        assert_eq!(pixels.len(), WIDTH * HEIGHT * 3);
        assert_eq!(&pixels[..3], &COLOURS[14]);
        assert_eq!(&pixels[pixels.len() - 3..], &COLOURS[12]);
        assert_eq!(&pixels[8 * 3..8 * 3 + 3], &COLOURS[0]);
    }

    #[test]
    fn blink_hides_only_foreground_and_borders_join_at_cell_edges() {
        let mut cells = [b' ', 7].repeat(2000);
        cells[..2].copy_from_slice(&[0xdb, 0x9e]);
        assert_eq!(&render(&cells, false).unwrap()[..3], &COLOURS[1]);
        assert_eq!(&render(&cells, true).unwrap()[..3], &COLOURS[14]);
        assert_eq!(glyph(0xcd)[2], 0xff);
        assert_eq!(glyph(0xcd)[5], 0xff);
        assert_eq!(glyph(0xba)[0], glyph(0xba)[7]);
        assert_eq!(glyph(0xbb), mirror(glyph(0xc9)));
        assert_eq!(glyph(0xbc), mirror(glyph(0xc8)));
    }
}
