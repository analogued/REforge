// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Pack shared bitmap glyphs for the engine's CGA drawing routines.

/// Pack one 8x8 character as CGA color three on color zero.
pub(crate) fn character(character: u8) -> Result<[u8; 16], String> {
    let rows = reforge_graphics::font8x8::glyph(character)
        .ok_or("Scoring text contains a non-printable ASCII character")?;
    let mut packed = [0; 16];
    for (y, row) in rows.iter().enumerate() {
        for x in 0..8 {
            if row & (1 << x) != 0 {
                packed[y * 2 + x / 4] |= 3 << (6 - 2 * (x % 4));
            }
        }
    }
    Ok(packed)
}
