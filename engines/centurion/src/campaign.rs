// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

fn coordinate(state: &Campaign, region: usize) -> Result<(i32, i32)> {
    let offset = 0x1ab8 + region * 4;
    Ok((
        word(&state.data, offset)? as i32,
        word(&state.data, offset + 2)? as i32,
    ))
}

pub fn render(
    assets: &Assets,
    state: &Campaign,
    background: &Bitmap,
    sprites: &[Bitmap],
) -> Result<Frame> {
    let mut frame = Frame::new();
    // Cached backdrop expansion at 0000:88a7 ignores the sprite transparent pen.
    frame.pixels.copy_from_slice(&background.pixels);
    for province in 0..27 {
        if state.data[0x5285 + province * 32] != 0 {
            let x = word(&state.data, 0x3c52 + province * 4)? as i32;
            let y = word(&state.data, 0x3c54 + province * 4)? as i32;
            frame.flood_replace(x, y, 9);
        }
    }
    for offset in (0x1116..0x115e).step_by(12) {
        if state.data[offset] != 255 {
            let region = state.data[offset - 2] as usize;
            let (x, y) = coordinate(state, region)?;
            frame.blit(&sprites[if offset < 0x113a { 6 } else { 8 }], x, y);
        }
    }
    for offset in (0x1924..0x1a14).step_by(20) {
        if state.data[offset + 1] != 255 {
            let region = state.data[offset] as usize;
            let (x, y) = coordinate(state, region)?;
            frame.blit(&sprites[if region < 27 { 2 } else { 4 }], x, y);
        }
    }
    for offset in (0xc113..0xc1ad).step_by(14) {
        if word(&state.data, offset + 7)? != 0 {
            let region = state.data[offset] as usize;
            let (x, y) = coordinate(state, region)?;
            frame.blit(&sprites[if region < 27 { 5 } else { 8 }], x, y);
        }
    }
    for address in [0x3c42, 0x3c4a] {
        let left = word(&state.data, address)? as i32;
        let top = word(&state.data, address + 2)? as i32;
        let right = word(&state.data, address + 4)? as i32;
        let bottom = word(&state.data, address + 6)? as i32;
        frame.bevel(left, top, right, bottom, word(&state.data, 0x6346)? as u8);
    }
    frame.text(assets, "End Turn", 232, 6, 15)?;
    let delta = 275 - word(&state.data, 0x634c)? as i32;
    let year = format!("{} {}", delta.abs(), if delta > 0 { "BC" } else { "AD" });
    frame.center_text(assets, &year, 297, 6, 15)?;
    Ok(frame)
}
