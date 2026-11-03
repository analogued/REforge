// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete asset sprites and procedural gates in native painter order.

use super::{Tracker, project, scale};
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, ROW_BYTES},
    gameplay::SpecialState,
};

pub(super) fn render(
    tracker: &Tracker,
    at: f64,
    craft_anchors: Option<[f64; 3]>,
    assets: &GraphicsAssets,
) -> Result<Frame, Error> {
    let f = tracker.frame.as_ref().unwrap();
    // The fresh native base includes the original, unchanged mountain strip.
    let mut packed = f.scene.base.packed().to_vec();
    let (positions, camera) = tracker.positions(at);
    let craft = f.scene.layers.iter().find(|l| l.slot == 0);
    let craft_anchors = craft_anchors.and_then(|anchors| tracker.craft_anchors(at, anchors));
    for layer in &f.scene.layers {
        let slot = layer.slot;
        let mut x = f64::from(layer.rect.x_bytes) * 4.0;
        let mut y = f64::from(layer.rect.y);
        let mut color = layer.color.as_slice();
        let mut mask = layer.mask.as_slice();
        let mut width = layer.rect.width_bytes;
        let mut height = layer.rect.height;
        let object = f.objects.get(&slot);
        if (23..=27).contains(&slot) && object.is_some_and(|o| !o.active) {
            continue;
        }
        let mut source = None;
        if slot <= 1 {
            if let Some(anchors) = craft_anchors {
                let sprite = sprite(assets, layer.source)?;
                let craft = sprite_or_error(assets, craft.map(|c| c.source))?;
                x = anchors[0] - f64::from(craft.descriptor[5]) * 4.0;
                y = anchors[if slot == 0 { 1 } else { 2 }] - f64::from(sprite.descriptor[6]);
                source = Some(sprite);
            }
        } else if object.is_some_and(|o| o.birth != o.projected_birth) {
            // A recycled record can still cache an earlier lifetime's image.
        } else if let Some(&position) = positions.get(&slot)
            && (1.0..1000.0).contains(&position[0])
            && x != 512.0
        {
            if let Some(gate) = object.and_then(|o| o.gate) {
                paint_gate(&mut packed, gate, position, camera[1], camera[2]);
                continue;
            }
            let sprite = sprite(assets, layer.source)?;
            let anchor = project(position, camera[1], camera[2]);
            x = anchor[0] - f64::from(sprite.descriptor[5]) * 4.0;
            y = anchor[1] - f64::from(sprite.descriptor[6]);
            source = Some(sprite);
        }
        if let Some(sprite) = source {
            color = &sprite.color;
            mask = &sprite.mask;
            width = sprite.width_bytes;
            height = sprite.height;
        }
        draw(&mut packed, color, mask, [width, height], x, y);
    }
    Frame::from_packed(&packed)
}

fn sprite(assets: &GraphicsAssets, source: usize) -> Result<&crate::assets::Sprite, Error> {
    sprite_or_error(assets, Some(source))
}

fn sprite_or_error(
    assets: &GraphicsAssets,
    source: Option<usize>,
) -> Result<&crate::assets::Sprite, Error> {
    source
        .and_then(|source| assets.sprites.get(source))
        .ok_or_else(|| Error::InvalidData("missing scene sprite".into()))
}

fn draw(packed: &mut [u8], color: &[u8], mask: &[u8], size: [usize; 2], x: f64, y: f64) {
    let x = (x + 0.5).floor() as i32;
    let y = (y + 0.5).floor() as i32;
    for dy in y.max(44)..(y + size[1] as i32).min(158) {
        for dx in x.max(16)..(x + size[0] as i32 * 4).min(304) {
            let sx = (dx - x) as usize;
            let src = (dy - y) as usize * size[0] + sx / 4;
            let shift = 6 - sx % 4 * 2;
            let dst = dy as usize * ROW_BYTES + dx as usize / 4;
            let ds = 6 - dx as usize % 4 * 2;
            let old = (packed[dst] >> ds) & 3;
            let value = (old & ((mask[src] >> shift) & 3)) | ((color[src] >> shift) & 3);
            packed[dst] = (packed[dst] & !(3 << ds)) | (value << ds);
        }
    }
}

pub(super) fn gate_rectangles(
    gate: SpecialState,
    position: [f64; 3],
    horizon: f64,
    origin: f64,
) -> Vec<[f64; 4]> {
    let s = scale(position[0]);
    let [left, bottom] = project(position, horizon, origin);
    let height = f64::from(gate.extents[2]) * s;
    let half = f64::from(gate.extents[1] >> 1) * s;
    let stroke = (gate.rect.width_bytes as f64 * s).clamp(1.0, 10.0);
    let right = left + 2.0 * half;
    let top = bottom - height;
    if matches!(gate.requested_sprite, 1 | 2) {
        vec![[left, top, right - left + 1.0, height]]
    } else {
        vec![
            [left, top, right - left + stroke, stroke],
            [left, top, stroke, height + 1.0],
            [right, top, stroke, height + 1.0],
        ]
    }
}

fn paint_gate(
    packed: &mut [u8],
    gate: SpecialState,
    position: [f64; 3],
    horizon: f64,
    origin: f64,
) {
    for [x, y, w, h] in gate_rectangles(gate, position, horizon, origin) {
        let [left, top, right, bottom] = [x, y, x + w, y + h].map(|v| (v + 0.5).floor() as i32);
        for dy in top.max(44)..bottom.min(158) {
            for dx in left.max(16)..right.min(304) {
                let color = match gate.requested_sprite {
                    0 | 2 => 0,
                    1 | 3 => 3,
                    _ => {
                        if dx % 2 == 0 {
                            2
                        } else {
                            1
                        }
                    }
                };
                let dst = dy as usize * ROW_BYTES + dx as usize / 4;
                let shift = 6 - dx as usize % 4 * 2;
                packed[dst] = (packed[dst] & !(3 << shift)) | (color << shift);
            }
        }
    }
}
