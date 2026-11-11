// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Hearts rival walking-pose placement, independent of temporal interpolation.
use crate::{
    assets::{sprite_bytes, word},
    framebuffer::Dimensions,
};

const SHAPE: Dimensions = Dimensions(0x0c03);

fn front(pixels: &[u8], left: bool) -> Option<usize> {
    let mut edge = None;
    for y in 0..12 {
        for x in 0..24 {
            // OR artwork is drawn over colour one; only its upper colour bit
            // changes the visible silhouette.
            if (pixels[y * 6 + x / 4] >> (6 - 2 * (x & 3))) & 2 != 0 {
                edge =
                    Some(edge.map_or(x, |old: usize| if left { old.min(x) } else { old.max(x) }));
            }
        }
    }
    edge
}

/// Register the retained normal walking picture around a mirrored front anchor.
/// Native positions, animation selection and velocity estimates remain untouched.
pub(crate) fn offset(data: &[u8], pixels: &[u8]) -> Result<f64, String> {
    if pixels.len() != SHAPE.len() {
        return Ok(0.0);
    }
    let mut selected = None;
    for index in 0..12 {
        let phase = (index % 6) * 2 + if index >= 6 { 16 } else { 0 };
        if sprite_bytes(data, word(data, 0x4a60 + phase), SHAPE)? == pixels {
            selected = Some(index);
            break;
        }
    }
    let Some(index) = selected else {
        return Ok(0.0);
    };
    let mut sum = 0;
    for phase in (0..12).step_by(2) {
        let image = sprite_bytes(data, word(data, 0x4a60 + phase), SHAPE)?;
        let Some(edge) = front(image, false) else {
            return Ok(0.0);
        };
        sum += edge;
    }
    let right_anchor = (sum + 3) / 6;
    let left = index >= 6;
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
        for (index, right) in [19, 21, 21, 21, 20, 18]
            .into_iter()
            .cycle()
            .take(12)
            .enumerate()
        {
            let source = 0x5000 + index * SHAPE.len();
            let left = index >= 6;
            let phase = (index % 6) * 2 + if left { 16 } else { 0 };
            set_word(&mut data, 0x4a60 + phase, source as u16);
            // Colour one must not count as silhouette in OR artwork.
            data[source..source + SHAPE.len()].fill(0x55);
            let x = if left { 23 - right } else { right };
            data[source + 3 * 6 + x / 4] |= 2 << (6 - 2 * (x & 3));
            let leg = if left { 11 } else { 12 };
            data[source + 9 * 6 + leg / 4] |= 2 << (6 - 2 * (leg & 3));
        }
        data
    }

    #[test]
    fn six_normal_poses_share_a_mirrored_front_without_mutating_data() {
        let data = images();
        let before = data.clone();
        for left in [false, true] {
            for phase in (0..12).step_by(2) {
                let table = 0x4a60 + phase + if left { 16 } else { 0 };
                let pixels = sprite_bytes(&data, word(&data, table), SHAPE).unwrap();
                let edge = front(pixels, left).unwrap() as f64;
                let placement = offset(&data, pixels).unwrap();
                assert_eq!(edge + placement, if left { 3.0 } else { 20.0 });
            }
        }
        assert_eq!(data, before);
    }

    #[test]
    fn unmatched_idle_recoil_and_other_shapes_keep_their_placement() {
        let data = images();
        assert_eq!(offset(&data, &[0xff; 72]).unwrap(), 0.0);
        assert_eq!(offset(&data, &[0xaa; 72]).unwrap(), 0.0);
        assert_eq!(offset(&data, &[0x55; 72]).unwrap(), 0.0);
        assert_eq!(offset(&data, &[0xff; 66]).unwrap(), 0.0);
    }
}
