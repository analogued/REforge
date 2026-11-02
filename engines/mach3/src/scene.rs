// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The verified no-directional-input initial scene, before the gameplay loop.
//! These are resolved draw records, not a model of subsequent gameplay updates.

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{FRAME_BYTES, Frame, ROW_BYTES, Rect},
};

#[derive(Clone, Copy, Debug)]
pub struct Draw {
    /// The source resolved before drawing, rather than a later requested index.
    pub resolved_sprite: usize,
    pub rect: Rect,
}

const fn draw(sprite: usize, x: i32, y: i32, width: usize, height: usize) -> Draw {
    Draw {
        resolved_sprite: sprite,
        rect: Rect {
            x_bytes: x,
            y,
            width_bytes: width,
            height,
        },
    }
}

/// Ordered scene records followed by HUD records. All x coordinates are bytes.
pub fn initial_draws() -> Vec<Draw> {
    let mut draws = vec![
        draw(18, 37, 147, 14, 12), // Shadow precedes the craft.
        draw(13, 37, 139, 14, 18),
        draw(130, 4, 44, 4, 15),
        draw(131, 72, 44, 4, 15),
        draw(132, 4, 144, 4, 15),
        draw(133, 72, 144, 4, 15),
        draw(24, 8, 38, 13, 3), // Object width also overrides the source stride.
        draw(24, 58, 38, 14, 3),
    ];
    for x in (34..=44).step_by(2) {
        draws.push(draw(70, x, 33, 2, 7));
    }
    // Requested indices have changed to 87, but sources still resolve 85 and 86.
    draws.push(draw(85, 28, 13, 6, 7));
    draws.push(draw(86, 47, 13, 6, 7));
    for x in [63, 2, 68, 7, 73, 12] {
        draws.push(draw(28, x, 180, 6, 7));
    }
    draws.push(draw(0, 38, 4, 2, 2));
    draws
}

pub fn initial_frame(assets: &GraphicsAssets) -> Result<Frame, Error> {
    compose(assets, 2, &initial_draws())
}

pub(crate) fn compose(
    assets: &GraphicsAssets,
    strip_index: usize,
    draws: &[Draw],
) -> Result<Frame, Error> {
    let mut frame = background(assets, strip_index)?;
    for command in draws {
        let sprite = assets.sprites.get(command.resolved_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing sprite {}", command.resolved_sprite))
        })?;
        frame.draw_sprite(sprite, command.rect)?;
    }
    Ok(frame)
}

pub(crate) fn background(assets: &GraphicsAssets, strip_index: usize) -> Result<Frame, Error> {
    background_at(assets, strip_index, 92)
}

pub(crate) fn background_at(
    assets: &GraphicsAssets,
    strip_index: usize,
    strip_y: usize,
) -> Result<Frame, Error> {
    let base = assets
        .primary
        .get(..FRAME_BYTES)
        .ok_or_else(|| Error::InvalidData("truncated primary background".into()))?;
    let mut packed = base.to_vec();
    const STRIP_BYTES: usize = 72 * 15;
    if strip_index >= 12 {
        return Err(Error::InvalidData("invalid terrain strip index".into()));
    }
    if strip_y > crate::frame::HEIGHT - 15 {
        return Err(Error::InvalidData("invalid terrain strip position".into()));
    }
    let start = FRAME_BYTES + strip_index * STRIP_BYTES;
    let strip = assets
        .primary
        .get(start..start + STRIP_BYTES)
        .ok_or_else(|| Error::InvalidData("missing terrain strip".into()))?;
    for y in 0..15 {
        let destination = (strip_y + y) * ROW_BYTES + 4;
        packed[destination..destination + 72].copy_from_slice(&strip[y * 72..(y + 1) * 72]);
    }
    Frame::from_packed(&packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Sprite;

    #[test]
    fn terrain_and_resolved_sources_compose_without_capture_state() {
        let mut assets = GraphicsAssets {
            primary: vec![0xaa; FRAME_BYTES + 12 * 72 * 15],
            alternate: vec![0; FRAME_BYTES],
            sprites: (0..190)
                .map(|_| Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: [0; 7],
                    color: vec![0; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        };
        assets.primary[FRAME_BYTES + 2 * 72 * 15..FRAME_BYTES + 3 * 72 * 15].fill(0x55);
        // The craft covers the upper part of the shadow; the latter extends below.
        assets.sprites[18].color.fill(0);
        assets.sprites[18].mask.fill(0);
        assets.sprites[13].color.fill(0xff);
        assets.sprites[13].mask.fill(0);
        assets.sprites[85].color.fill(0x1b);
        assets.sprites[85].mask.fill(0);
        assets.sprites[87].color.fill(0xe4);
        assets.sprites[87].mask.fill(0);
        let frame = initial_frame(&assets).unwrap();
        assert_eq!(frame.packed()[92 * 80 + 4], 0x55);
        assert_eq!(frame.packed()[106 * 80 + 75], 0x55);
        assert_eq!(frame.packed()[107 * 80 + 4], 0xaa);
        assert_eq!(frame.packed()[150 * 80 + 37], 0xff);
        assert_eq!(frame.packed()[157 * 80 + 37], 0);
        assert_eq!(frame.packed()[13 * 80 + 28], 0x1b);
    }

    #[test]
    fn missing_resource_returns_error() {
        let assets = GraphicsAssets {
            primary: vec![0; FRAME_BYTES],
            alternate: vec![],
            sprites: vec![],
        };
        assert!(initial_frame(&assets).is_err());
    }
}
