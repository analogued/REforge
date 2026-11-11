// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared walking-pose placement, independent of temporal interpolation.
use crate::{
    assets::{sprite_bytes, word},
    framebuffer::Dimensions,
};

const SHAPE: Dimensions = Dimensions(0x0b03);

fn front(pixels: &[u8], left: bool) -> Option<usize> {
    let mut edge = None;
    for y in 0..11 {
        for x in 0..24 {
            if (pixels[y * 6 + x / 4] >> (6 - 2 * (x & 3))) & 3 != 3 {
                edge =
                    Some(edge.map_or(x, |old: usize| if left { old.min(x) } else { old.max(x) }));
            }
        }
    }
    edge
}

/// Register the selected full walking mask around one mirrored front anchor.
/// The original pixels, logical position and animation phase are untouched.
pub(crate) fn offset(data: &[u8], pixels: &[u8]) -> Result<f64, String> {
    if pixels.len() != SHAPE.len() {
        return Ok(0.0);
    }
    // Use the image actually retained by the compositor. Input direction can
    // already have changed while the previous walking pose is still visible.
    let mut selected = None;
    for index in 0..12 {
        if sprite_bytes(data, word(data, 0xf7a + index * 2), SHAPE)? == pixels {
            selected = Some(index);
            break;
        }
    }
    let Some(index) = selected else {
        return Ok(0.0);
    };
    let left = index >= 6;
    let mut sum = 0;
    for i in 0..6 {
        let image = sprite_bytes(data, word(data, 0xf7a + i * 2), SHAPE)?;
        let Some(edge) = front(image, false) else {
            return Ok(0.0);
        };
        sum += edge;
    }
    let right_anchor = (sum + 3) / 6;
    let reference = if left {
        23 - right_anchor
    } else {
        right_anchor
    };
    Ok(front(pixels, left).map_or(0.0, |edge| reference as f64 - edge as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{DATA_LENGTH, set_word};
    fn images() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        for (i, right) in [19, 21, 21, 21, 20, 18]
            .into_iter()
            .cycle()
            .take(12)
            .enumerate()
        {
            let source = 0x5000 + i * SHAPE.len();
            set_word(&mut data, 0xf7a + i * 2, source as u16);
            data[source..source + SHAPE.len()].fill(0xff);
            let x = if i < 6 { right } else { 23 - right };
            data[source + 3 * 6 + x / 4] &= !(3 << (6 - 2 * (x & 3)));
            data[source + 8 * 6 + 3] &= !0xc0; // A separate leg pixel at column 12.
        }
        data
    }
    #[test]
    fn six_poses_share_a_mirrored_anchor_without_changing_their_pixels() {
        let data = images();
        let before = data.clone();
        for direction in [1, 255] {
            for frame in 0u8..6 {
                let index = usize::from(frame) + if direction == 255 { 6 } else { 0 };
                let pixels = sprite_bytes(&data, word(&data, 0xf7a + index * 2), SHAPE).unwrap();
                let edge = front(pixels, direction == 255).unwrap() as f64;
                let placement = offset(&data, pixels).unwrap();
                assert_eq!(edge + placement, if direction == 255 { 3.0 } else { 20.0 });
            }
        }
        assert_eq!(data, before);
    }
    #[test]
    fn retained_idle_or_unmatched_images_keep_their_existing_placement() {
        let data = images();
        assert_eq!(offset(&data, &[0xff; 66]).unwrap(), 0.0);
        assert_eq!(offset(&data, &[0xff; 64]).unwrap(), 0.0);
    }
}
