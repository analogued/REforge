// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only native drawing transactions for the visual presenter.

use super::*;
use crate::frame::{FRAME_BYTES, ROW_BYTES};

/// A masked, byte-aligned drawing operation in retained world order.
#[derive(Clone, Debug)]
pub struct Layer {
    pub slot: usize,
    pub source: usize,
    pub rect: Rect,
    pub color: Vec<u8>,
    pub mask: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub base: Frame,
    pub layers: Vec<Layer>,
}

/// Actual background restores followed by clipped draws for one ordinary pass.
/// Consumers must apply these to the preceding retained scene. A full viewport
/// restore starts each body; partial passes can expose newer far-object draws.
#[derive(Clone, Debug)]
pub struct RasterPass {
    pub background: Frame,
    pub restores: Vec<Rect>,
    pub layers: Vec<Layer>,
}

impl Layer {
    /// Neutralize bytes outside an original clipping region, without changing
    /// the sprite origin/stride (needed when visual anchors move separately).
    fn restrict(&mut self, clip: Rect) {
        for row in 0..self.rect.height {
            for column in 0..self.rect.width_bytes {
                let x = self.rect.x_bytes + column as i32;
                let y = self.rect.y + row as i32;
                if x < clip.x_bytes
                    || x >= clip.x_bytes + clip.width_bytes as i32
                    || y < clip.y
                    || y >= clip.y + clip.height as i32
                {
                    let index = row * self.rect.width_bytes + column;
                    self.color[index] = 0;
                    self.mask[index] = 255;
                }
            }
        }
    }

    fn nonempty(&self) -> bool {
        self.color
            .iter()
            .zip(&self.mask)
            .any(|(&c, &m)| c != 0 || m != 255)
    }
}

impl Gameplay {
    /// Export the real drawing transaction, rather than assuming a fresh full
    /// composition from remembered per-object placements. No gameplay mutation.
    /// Call at every ordinary publication; timed transition transactions are not
    /// composed by the gameplay transition path.
    pub fn raster_pass(&self, assets: &GraphicsAssets) -> Result<RasterPass, Error> {
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
        let draws = self.world_draws();
        let mut layers = Vec::new();
        for &clip in &self.restore_regions {
            for &slot in &self.full_world_order {
                let mut layer = if (23..=27).contains(&slot) {
                    let mut zero = Frame::from_packed(&vec![0; FRAME_BYTES])?;
                    let mut ones = Frame::from_packed(&vec![255; FRAME_BYTES])?;
                    self.special.draw_slot(&mut zero, slot, clip)?;
                    self.special.draw_slot(&mut ones, slot, clip)?;
                    let color = zero.packed().to_vec();
                    let mask = ones
                        .packed()
                        .iter()
                        .zip(&color)
                        .map(|(a, b)| a ^ b)
                        .collect();
                    Layer {
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
                    }
                } else if let Some(draw) = draws[slot] {
                    let sprite = &assets.sprites[draw.resolved_sprite];
                    let size = draw.rect.width_bytes * draw.rect.height;
                    Layer {
                        slot,
                        source: draw.resolved_sprite,
                        rect: draw.rect,
                        color: sprite.color[..size].to_vec(),
                        mask: sprite.mask[..size].to_vec(),
                    }
                } else {
                    continue;
                };
                layer.restrict(clip);
                if layer.nonempty() {
                    layers.push(layer);
                }
            }
            for (index, draw) in scene::initial_draws()
                .into_iter()
                .skip(2)
                .take(4)
                .enumerate()
            {
                let sprite = &assets.sprites[draw.resolved_sprite];
                let size = draw.rect.width_bytes * draw.rect.height;
                let mut layer = Layer {
                    slot: 29 + index,
                    source: draw.resolved_sprite,
                    rect: draw.rect,
                    color: sprite.color[..size].to_vec(),
                    mask: sprite.mask[..size].to_vec(),
                };
                layer.restrict(clip);
                if layer.nonempty() {
                    layers.push(layer);
                }
            }
        }
        Ok(RasterPass {
            background,
            restores: self.restore_regions.clone(),
            layers,
        })
    }

    /// Does not resolve sources, sort records or mutate any cached drawing state.
    #[cfg(feature = "dev-tools")]
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

    fn apply(pass: RasterPass, frame: &mut Frame) {
        for clip in pass.restores {
            presentation::restore_background(frame, &pass.background, clip).unwrap();
        }
        for layer in pass.layers {
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
    }

    #[test]
    fn raster_transactions_recompose_partial_updates_without_mutation() {
        let assets = super::super::tests::synthetic_assets();
        let mut game = Gameplay::test_game(&assets).unwrap();
        let mut retained = game.frame().clone();
        let mut visual_world = crate::motion::World::default();
        for step in 0..60 {
            game.advance_subframe(
                &assets,
                Steering {
                    left: step % 12 < 4,
                    right: step % 12 >= 6,
                    up: step % 16 < 5,
                    down: step % 16 >= 9,
                    fire: true,
                },
            )
            .unwrap();
            let before = format!("{game:?}");
            let raster = game.raster_pass(&assets).unwrap();
            assert_eq!(format!("{game:?}"), before);
            apply(raster, &mut retained);
            assert_eq!(retained, *game.pass_frame(), "step {step}");
            if step == 16 {
                visual_world.reset();
            }
            if let Some(o) = visual_world
                .capture(&game, &assets, f64::from(step) / 30.0, step as u64)
                .unwrap()
            {
                if o.pose.normal() {
                    assert_eq!(
                        crate::motion::compose(&o, o.native_anchors).unwrap(),
                        *game.pass_frame()
                    );
                }
            } else {
                assert_ne!(game.snapshot().render_pass, 1);
            }
        }
    }

    #[test]
    fn partial_clip_exports_current_far_draw_instead_of_retained_placement() {
        let mut assets = super::super::tests::synthetic_assets();
        assets.sprites[23].color.fill(0xff);
        assets.sprites[23].mask.fill(0);
        let mut game = Gameplay::test_game(&assets).unwrap();
        game.render_pass = 3;
        let mut shots = game.weapon.snapshot().shots;
        shots[1].active = true;
        shots[1].depth = 100;
        shots[1].resolved_sprite = 23;
        shots[1].rect = Rect {
            x_bytes: 50,
            y: 90,
            width_bytes: 2,
            height: 2,
        };
        game.weapon.restore(shots);
        game.remembered_draws = game.world_draws();
        let background = game
            .presentation()
            .background(&assets, game.snapshot().strip_index, game.strip_y, 3)
            .unwrap();
        let mut native = background.clone();
        game.draw_world(&assets, &mut native, presentation::VIEWPORT)
            .unwrap();
        let mut replay = native.clone();
        shots[1].rect.x_bytes = 49;
        game.weapon.restore(shots);
        let clip = Rect {
            x_bytes: 49,
            y: 90,
            width_bytes: 1,
            height: 1,
        };
        game.restore_regions = vec![clip];
        presentation::restore_background(&mut native, &background, clip).unwrap();
        game.draw_world(&assets, &mut native, clip).unwrap();
        let raster = game.raster_pass(&assets).unwrap();
        let shot = raster.layers.iter().find(|layer| layer.slot == 3).unwrap();
        assert_eq!(shot.rect.x_bytes, 49);
        assert_eq!(game.remembered_draws[3].unwrap().rect.x_bytes, 50);
        assert_eq!(&shot.color, &[255, 0, 0, 0]);
        assert_eq!(&shot.mask, &[0, 255, 255, 255]);
        apply(raster, &mut replay);
        assert_eq!(native, replay);
    }

    #[cfg(feature = "dev-tools")]
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
