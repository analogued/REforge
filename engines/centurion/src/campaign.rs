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
    render_highlighted(assets, state, background, sprites, None)
}

/// Hover follows the same region mask as map selection, excluding the controls
/// drawn over the northern provinces.
pub fn hovered_province(state: &Campaign, cursor: (i32, i32)) -> Result<Option<u8>> {
    for address in [0x3c42, 0x3c4a] {
        let left = word(&state.data, address)? as i32;
        let top = word(&state.data, address + 2)? as i32;
        let right = word(&state.data, address + 4)? as i32;
        let bottom = word(&state.data, address + 6)? as i32;
        if (left..=right).contains(&cursor.0) && (top..=bottom).contains(&cursor.1) {
            return Ok(None);
        }
    }
    state.province_at(cursor.0, cursor.1)
}

pub fn render_highlighted(
    assets: &Assets,
    state: &Campaign,
    background: &Bitmap,
    sprites: &[Bitmap],
    highlighted: Option<u8>,
) -> Result<Frame> {
    let mut frame = Frame::new();
    // Cached backdrop expansion at 0000:88a7 ignores the sprite transparent pen.
    frame.pixels.copy_from_slice(&background.pixels);
    paint_borders(&mut frame, state, highlighted, &assets.palette)?;
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

fn paint_borders(
    frame: &mut Frame,
    state: &Campaign,
    highlighted: Option<u8>,
    palette: &crate::water::Palette,
) -> Result<()> {
    for province in 0..27 {
        let owned = state.data[0x5285 + province * 32] != 0;
        let hover = highlighted == Some(province as u8);
        if owned || hover {
            let x = word(&state.data, 0x3c52 + province * 4)? as i32;
            let y = word(&state.data, 0x3c54 + province * 4)? as i32;
            let mut color = 9;
            if hover && (0..319).contains(&x) && (0..199).contains(&y) {
                let original = if owned {
                    9
                } else {
                    frame.pixels[y as usize * 320 + x as usize]
                };
                // CMAP defines 250 entries. Use an otherwise unused frame-local
                // pen instead of recoloring terrain that shares a border pen.
                color = 250;
                let mut highlighted_palette = *palette;
                highlighted_palette[250] = palette[original as usize]
                    .map(|channel| (u16::from(channel) * 3 / 2).min(255) as u8);
                frame.palette = Some(highlighted_palette);
            }
            // Apply hover while each province still has its original isolated
            // border path, before unit sprites and map controls cover it.
            frame.flood_replace(x, y, color);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_changes_only_one_border_and_keeps_ownership_colors_separate() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        let mut background = Frame::new();
        // Two separate province paths, plus terrain sharing the brown pen.
        for left in [10, 23] {
            background.rect(left, 10, 11, 11, 104);
            background.rect(left + 1, 11, 9, 9, 77);
        }
        background.point(15, 15, 104);
        for province in 0..27 {
            let x: u16 = if province == 0 { 10 } else { 23 };
            state.data[0x3c52 + province * 4..0x3c54 + province * 4]
                .copy_from_slice(&x.to_le_bytes());
            state.data[0x3c54 + province * 4..0x3c56 + province * 4]
                .copy_from_slice(&10u16.to_le_bytes());
        }
        state.data[0x5285 + 32] = 1;
        let mut palette = [[0; 3]; 256];
        palette[9] = [10, 20, 200];
        palette[104] = [100, 80, 60];
        let data = state.data.clone();
        for owned in [false, true] {
            state.data[0x5285] = u8::from(owned);
            let mut normal = background.clone();
            paint_borders(&mut normal, &state, None, &palette).unwrap();
            let mut hover = background.clone();
            paint_borders(&mut hover, &state, Some(0), &palette).unwrap();
            assert_eq!(
                hover.pixels[15 * 320 + 15],
                104,
                "terrain must not be recolored"
            );
            assert_eq!(hover.pixels[10 * 320 + 23], 9, "neighbor remains blue");
            assert_eq!(hover.pixels[10 * 320 + 10], 250);
            assert_eq!(
                hover.palette.unwrap()[250],
                if owned { [15, 30, 255] } else { [150, 120, 90] }
            );
            for (index, (actual, original)) in hover.pixels.iter().zip(&normal.pixels).enumerate() {
                if actual != original {
                    let (x, y) = (index % 320, index / 320);
                    assert!((10..=20).contains(&x) && (10..=20).contains(&y));
                    assert!(x == 10 || x == 20 || y == 10 || y == 20);
                }
            }
            assert!(normal.palette.is_none());
        }
        state.data[0x5285] = data[0x5285];
        assert_eq!(state.data, data);
    }

    #[test]
    fn hover_hit_testing_excludes_seas_controls_and_outside_points() {
        let mut data = vec![0; 65536];
        for y in 0..200 {
            let runs = 0x4000 + y * 4;
            data[0x3a96 + y * 2..0x3a98 + y * 2].copy_from_slice(&(runs as u16).to_le_bytes());
            data[runs..runs + 4].copy_from_slice(&[0, 160, 27, 160]);
        }
        for (index, value) in [10u16, 10, 20, 20].into_iter().enumerate() {
            data[0x3c42 + index * 2..0x3c44 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        let state = Campaign::test_data(data);
        assert_eq!(hovered_province(&state, (50, 50)).unwrap(), Some(0));
        for point in [(15, 15), (180, 50), (-1, 50), (320, 50), (50, 200)] {
            assert_eq!(hovered_province(&state, point).unwrap(), None);
        }
    }
}
