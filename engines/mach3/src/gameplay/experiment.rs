// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only visual export for an isolated interpolation experiment.

use super::*;
use crate::frame::{FRAME_BYTES, ROW_BYTES};

/// A masked, byte-aligned drawing operation in retained world order.
#[derive(Debug)]
pub struct Layer {
    pub slot: usize,
    pub source: usize,
    pub rect: Rect,
    pub color: Vec<u8>,
    pub mask: Vec<u8>,
}

#[derive(Debug)]
pub struct Scene {
    pub base: Frame,
    pub layers: Vec<Layer>,
}

impl Gameplay {
    /// Does not resolve sources, sort records or mutate any cached drawing state.
    pub fn experimental_scene(&self, assets: &GraphicsAssets) -> Result<Scene, Error> {
        let background = presentation::Presentation {
            layer: self.selected_layer,
            overlay_counter: 0,
            ..self.presentation()
        }
        .background(
            assets,
            self.snapshot().strip_index,
            self.strip_y,
            self.render_pass,
        )?;
        let mut base = self.pass_frame.clone();
        presentation::restore_background(&mut base, &background, presentation::VIEWPORT)?;
        let mut layers = Vec::new();
        for &slot in &self.full_world_order {
            if (23..=27).contains(&slot) {
                // Two constant inputs recover this procedural operation's exact
                // byte mask and colour without interpreting its drawing algorithm.
                let mut zero = Frame::from_packed(&vec![0; FRAME_BYTES])?;
                let mut ones = Frame::from_packed(&vec![255; FRAME_BYTES])?;
                self.special
                    .draw_slot(&mut zero, slot, presentation::VIEWPORT)?;
                self.special
                    .draw_slot(&mut ones, slot, presentation::VIEWPORT)?;
                let color = zero.packed().to_vec();
                let mask = ones
                    .packed()
                    .iter()
                    .zip(&color)
                    .map(|(a, b)| a ^ b)
                    .collect();
                layers.push(Layer {
                    slot,
                    source: usize::MAX,
                    rect: Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: ROW_BYTES,
                        height: 200,
                    },
                    color,
                    mask,
                });
            } else if let Some(draw) = self.remembered_draws[slot] {
                let sprite = &assets.sprites[draw.resolved_sprite];
                let size = draw.rect.width_bytes * draw.rect.height;
                layers.push(Layer {
                    slot,
                    source: draw.resolved_sprite,
                    rect: draw.rect,
                    color: sprite.color[..size].to_vec(),
                    mask: sprite.mask[..size].to_vec(),
                });
            }
        }
        for (i, draw) in scene::initial_draws()
            .into_iter()
            .skip(2)
            .take(4)
            .enumerate()
        {
            let sprite = &assets.sprites[draw.resolved_sprite];
            let size = draw.rect.width_bytes * draw.rect.height;
            layers.push(Layer {
                slot: 29 + i,
                source: draw.resolved_sprite,
                rect: draw.rect,
                color: sprite.color[..size].to_vec(),
                mask: sprite.mask[..size].to_vec(),
            });
        }
        Ok(Scene { base, layers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Sprite;

    #[test]
    fn export_is_observational_and_recomposes_the_native_scene() {
        let assets = super::super::tests::synthetic_assets();
        let mut game = Gameplay::test_game(&assets).unwrap();
        for _ in 0..9 {
            game.advance_subframe(&assets, Steering::default()).unwrap();
            let before = format!("{game:?}");
            let scene = game.experimental_scene(&assets).unwrap();
            assert_eq!(format!("{game:?}"), before);
            let mut frame = scene.base;
            for layer in scene.layers {
                let sprite = Sprite {
                    width_bytes: layer.rect.width_bytes,
                    height: layer.rect.height,
                    descriptor: [0; 7],
                    color: layer.color,
                    mask: layer.mask,
                };
                frame
                    .draw_sprite_clipped(&sprite, layer.rect, presentation::VIEWPORT)
                    .unwrap();
            }
            assert_eq!(frame, *game.pass_frame());
        }
    }
}
