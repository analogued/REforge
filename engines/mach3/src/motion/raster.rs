// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Retain native world fragments; draw complete moving actors with original occlusion.

use super::Pose;
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, ROW_BYTES, Rect},
    gameplay::{
        Gameplay,
        experiment::{Layer, Scene},
    },
};

const VIEW: Rect = Rect {
    x_bytes: 4,
    y: 44,
    width_bytes: 72,
    height: 114,
};

#[derive(Clone, Debug)]
pub struct Observation {
    pub pose: Pose,
    pub scene: Scene,
    pub order: [usize; 29],
    pub actors: [Layer; 2],
    pub offsets: [f64; 3],
    pub native_anchors: [f64; 3],
}

#[derive(Default, Debug)]
pub struct World {
    scene: Option<Scene>,
}

fn restore(target: &mut [u8], source: &[u8], clip: Rect) {
    for y in clip.y as usize..clip.y as usize + clip.height {
        let start = y * ROW_BYTES + clip.x_bytes as usize;
        target[start..start + clip.width_bytes]
            .copy_from_slice(&source[start..start + clip.width_bytes]);
    }
}

fn erase(layer: &mut Layer, clip: Rect) {
    let r = layer.rect;
    let left = r.x_bytes.max(clip.x_bytes);
    let right = (r.x_bytes + r.width_bytes as i32).min(clip.x_bytes + clip.width_bytes as i32);
    let top = r.y.max(clip.y);
    let bottom = (r.y + r.height as i32).min(clip.y + clip.height as i32);
    for y in top..bottom {
        for x in left..right {
            let i = (y - r.y) as usize * r.width_bytes + (x - r.x_bytes) as usize;
            layer.color[i] = 0;
            layer.mask[i] = 255;
        }
    }
}

impl World {
    pub fn reset(&mut self) {
        self.scene = None;
    }

    /// A complete first pass is required after a transition/reset. Partial
    /// transactions alone cannot recover the previous world's hidden pixels.
    pub fn capture(
        &mut self,
        game: &Gameplay,
        assets: &GraphicsAssets,
        at: f64,
        id: u64,
    ) -> Result<Option<Observation>, Error> {
        let state = game.snapshot();
        if state.render_pass == 1 {
            self.scene = Some(Scene {
                base: game.pass_frame().clone(),
                layers: Vec::new(),
            });
        }
        let Some(scene) = &mut self.scene else {
            return Ok(None);
        };
        let pass = game.experimental_raster_pass(assets)?;
        let mut base = game.pass_frame().packed().to_vec();
        restore(&mut base, scene.base.packed(), VIEW);
        for clip in pass.restores {
            restore(&mut base, pass.background.packed(), clip);
            if (clip.x_bytes, clip.y, clip.width_bytes, clip.height)
                == (VIEW.x_bytes, VIEW.y, VIEW.width_bytes, VIEW.height)
            {
                scene.layers.clear();
            } else {
                for layer in &mut scene.layers {
                    erase(layer, clip);
                }
            }
        }
        scene.layers.retain(|l| {
            l.color
                .iter()
                .zip(&l.mask)
                .any(|(&c, &m)| c != 0 || m != 255)
        });
        scene
            .layers
            .extend(pass.layers.into_iter().filter(|l| l.slot > 1));
        scene.base = Frame::from_packed(&base)?;
        let player = state.player;
        let sources = [player.resolved_sprite, player.shadow.resolved_sprite];
        let actors = std::array::from_fn(|slot| {
            let source = sources[slot];
            let s = &assets.sprites[source];
            Layer {
                slot,
                source,
                rect: Rect {
                    width_bytes: s.width_bytes,
                    height: s.height,
                    ..if slot == 0 {
                        player.rect
                    } else {
                        player.shadow.rect
                    }
                },
                color: s.color.clone(),
                mask: s.mask.clone(),
            }
        });
        let craft = &assets.sprites[sources[0]];
        let shadow = &assets.sprites[sources[1]];
        let offsets = [
            f64::from(craft.descriptor[5]) * 4.0,
            f64::from(craft.descriptor[6]),
            f64::from(shadow.descriptor[6]),
        ];
        let native_anchors = [
            f64::from(player.rect.x_bytes) * 4.0 + offsets[0],
            f64::from(player.rect.y) + offsets[1],
            f64::from(player.shadow.rect.y) + offsets[2],
        ];
        let pose = Pose {
            at,
            id,
            body: state.body,
            pass: state.render_pass,
            coordinates: player.coordinates,
            phase: player.phase,
            controls: (player.effective_input & 31) as u8,
            horizon: player.horizon,
            origin: state.vertical_origin,
            velocity: player.velocity,
            source: [
                sources[0],
                craft.width_bytes * 4,
                craft.height,
                sources[1],
                shadow.width_bytes * 4,
                shadow.height,
            ],
        };
        Ok(Some(Observation {
            pose,
            scene: scene.clone(),
            order: state.full_world_order,
            actors,
            offsets,
            native_anchors,
        }))
    }
}

fn pixel(layer: &Layer, x: i32, y: i32) -> Option<(u8, u8)> {
    let r = layer.rect;
    if x < r.x_bytes * 4
        || x >= (r.x_bytes + r.width_bytes as i32) * 4
        || y < r.y
        || y >= r.y + r.height as i32
    {
        return None;
    }
    let dx = (x - r.x_bytes * 4) as usize;
    let i = (y - r.y) as usize * r.width_bytes + dx / 4;
    let shift = 6 - (dx % 4) * 2;
    Some(((layer.mask[i] >> shift) & 3, (layer.color[i] >> shift) & 3))
}

/// Full actors are supplied from loaded assets, never from clipped paint
/// fragments. World operations retain their native history and geometry.
pub fn compose(o: &Observation, anchors: [f64; 3]) -> Result<Frame, Error> {
    let mut packed = o.scene.base.packed().to_vec();
    for layer in &o.scene.layers {
        let r = layer.rect;
        for y in r.y.max(44)..(r.y + r.height as i32).min(158) {
            for x in r.x_bytes.max(4)..(r.x_bytes + r.width_bytes as i32).min(76) {
                let src = (y - r.y) as usize * r.width_bytes + (x - r.x_bytes) as usize;
                let dst = y as usize * ROW_BYTES + x as usize;
                packed[dst] = (packed[dst] & layer.mask[src]) | layer.color[src];
            }
        }
    }
    let mut rank = [0; 33];
    for (i, &slot) in o.order.iter().enumerate() {
        rank[slot] = i;
    }
    for (slot, r) in rank.iter_mut().enumerate().skip(29) {
        *r = 29 + slot;
    }
    let mut slots = [0, 1];
    slots.sort_by_key(|&slot| rank[slot]);
    for slot in slots {
        let actor = &o.actors[slot];
        let x = (anchors[0] - o.offsets[0] + 0.5).floor() as i32;
        let y = (anchors[if slot == 0 { 1 } else { 2 }] - o.offsets[if slot == 0 { 1 } else { 2 }]
            + 0.5)
            .floor() as i32;
        let r = actor.rect;
        for dy in y.max(44)..(y + r.height as i32).min(158) {
            for dx in x.max(16)..(x + r.width_bytes as i32 * 4).min(304) {
                let sx = (dx - x) as usize;
                let src = (dy - y) as usize * r.width_bytes + sx / 4;
                let shift = 6 - (sx % 4) * 2;
                let mut visible = 3;
                for layer in &o.scene.layers {
                    if rank[layer.slot] > rank[slot]
                        && let Some((m, c)) = pixel(layer, dx, dy)
                    {
                        visible &= m & (3 ^ c);
                    }
                }
                let mask = ((actor.mask[src] >> shift) & 3) | (3 ^ visible);
                let color = ((actor.color[src] >> shift) & 3) & visible;
                let dst = dy as usize * ROW_BYTES + dx as usize / 4;
                let ds = 6 - (dx as usize % 4) * 2;
                let old = (packed[dst] >> ds) & 3;
                packed[dst] = (packed[dst] & !(3 << ds)) | (((old & mask) | color) << ds);
            }
        }
    }
    Frame::from_packed(&packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FRAME_BYTES;
    fn layer(slot: usize, x_bytes: i32, width_bytes: usize, color: u8, mask: u8) -> Layer {
        Layer {
            slot,
            source: 0,
            rect: Rect {
                x_bytes,
                y: 50,
                width_bytes,
                height: 4,
            },
            color: vec![color; width_bytes * 4],
            mask: vec![mask; width_bytes * 4],
        }
    }
    fn observation(world: Vec<Layer>, craft: Layer) -> Observation {
        let pose = Pose {
            at: 0.0,
            id: 0,
            body: 1,
            pass: 1,
            coordinates: [14, 0, 0],
            phase: 0,
            controls: 0,
            horizon: 0,
            origin: 106,
            velocity: [0, 0],
            source: [13, 16, 4, 18, 4, 4],
        };
        let order: Vec<_> = [1, 5, 0]
            .into_iter()
            .chain((0..29).filter(|i| ![1, 5, 0].contains(i)))
            .collect();
        Observation {
            pose,
            scene: Scene {
                base: Frame::from_packed(&vec![0; FRAME_BYTES]).unwrap(),
                layers: world,
            },
            order: order.try_into().unwrap(),
            actors: [craft, layer(1, 0, 1, 0, 255)],
            offsets: [0.0; 3],
            native_anchors: [24.0, 50.0, 50.0],
        }
    }
    fn at(frame: &Frame, x: usize, y: usize) -> u8 {
        (frame.packed()[y * ROW_BYTES + x / 4] >> (6 - (x % 4) * 2)) & 3
    }
    #[test]
    fn full_craft_covers_far_enemy_but_respects_nearer_effect_and_border() {
        let o = observation(
            vec![
                layer(5, 8, 4, 0x55, 0),
                layer(23, 7, 1, 0xaa, 0),
                layer(29, 9, 1, 0, 0),
            ],
            layer(0, 0, 4, 255, 0),
        );
        let f = compose(&o, [28.0, 50.0, 50.0]).unwrap();
        for x in 28..32 {
            assert_eq!(at(&f, x, 50), 2);
        }
        for x in 32..36 {
            assert_eq!(at(&f, x, 50), 3);
        }
        for x in 36..40 {
            assert_eq!(at(&f, x, 50), 0);
        }
        for x in 40..44 {
            assert_eq!(at(&f, x, 50), 3);
        }
        for x in 44..48 {
            assert_eq!(at(&f, x, 50), 1);
        }
    }
    #[test]
    fn two_bit_visibility_matches_independent_painter_composition() {
        for background in 0..4 {
            for fm in 0..4 {
                for fc in 0..4 {
                    for am in 0..4 {
                        for ac in 0..4 {
                            let mut o = observation(
                                vec![layer(23, 6, 1, fc * 85, fm * 85)],
                                layer(0, 0, 1, ac * 85, am * 85),
                            );
                            o.scene.base =
                                Frame::from_packed(&vec![background * 85; FRAME_BYTES]).unwrap();
                            let f = compose(&o, [24.0, 50.0, 50.0]).unwrap();
                            assert_eq!(at(&f, 24, 50), (((background & am) | ac) & fm) | fc);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn original_clipping_and_reference_bytes_survive_fractional_placement() {
        let o = observation(Vec::new(), layer(0, 0, 4, 255, 0));
        let before = format!("{o:?}");
        let f = compose(&o, [12.375, 42.25, 50.0]).unwrap();
        assert_eq!(before, format!("{o:?}"));
        assert_eq!(at(&f, 15, 44), 0);
        assert_eq!(at(&f, 16, 43), 0);
        assert_eq!(at(&f, 16, 44), 3);
    }
    #[test]
    fn erasing_a_native_dirty_clip_preserves_other_sprite_bytes() {
        let mut l = layer(5, 6, 2, 255, 0);
        erase(
            &mut l,
            Rect {
                x_bytes: 7,
                y: 51,
                width_bytes: 1,
                height: 1,
            },
        );
        assert_eq!(l.color, [255, 255, 255, 0, 255, 255, 255, 255]);
        assert_eq!(l.mask, [0, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn displaced_actors_match_an_independent_clipped_painter() {
        // Canonical painter order, with retained far-object fragments and an
        // overlapping later effect. The oracle paints all operations afresh;
        // it does not calculate visibility or use compose's pixel helper.
        let mut far = layer(5, 5, 7, 0x69, 0x36);
        let mut effect = layer(23, 7, 3, 0x82, 0x59);
        for row in 0..4 {
            // A previous background restore erased this retained clip.
            far.color[row * 7 + 3] = 0;
            far.mask[row * 7 + 3] = 255;
            // Only a clipped portion of the effect was repainted.
            effect.color[row * 3] = 0;
            effect.mask[row * 3] = 255;
        }
        let mut o = observation(
            vec![far, effect, layer(29, 9, 1, 0x05, 0xf0)],
            layer(0, 0, 5, 0xc6, 0x39),
        );
        o.actors[1] = layer(1, 0, 6, 0x91, 0x6c);
        o.scene.base = Frame::from_packed(&vec![0x1b; FRAME_BYTES]).unwrap();
        o.offsets = [2.0, 1.0, 0.0];
        for x in [12.25, 24.0, 29.5, 302.0] {
            for y in [42.0, 50.25, 156.0] {
                let anchors = [x, y, y + 1.0];
                let mut pixels = vec![0; 320 * 200];
                for row in 0..200 {
                    for column in 0..320 {
                        pixels[row * 320 + column] = at(&o.scene.base, column, row);
                    }
                }
                // Native order here is shadow, far craft, player, effect, HUD.
                for (l, actor) in [
                    (&o.actors[1], Some(1)),
                    (&o.scene.layers[0], None),
                    (&o.actors[0], Some(0)),
                    (&o.scene.layers[1], None),
                    (&o.scene.layers[2], None),
                ] {
                    let (left, top) = actor.map_or((l.rect.x_bytes * 4, l.rect.y), |slot| {
                        (
                            (anchors[0] - o.offsets[0] + 0.5).floor() as i32,
                            (anchors[slot + 1] - o.offsets[slot + 1] + 0.5).floor() as i32,
                        )
                    });
                    for row in 0..l.rect.height {
                        for column in 0..l.rect.width_bytes * 4 {
                            let (dx, dy) = (left + column as i32, top + row as i32);
                            if !(16..304).contains(&dx) || !(44..158).contains(&dy) {
                                continue;
                            }
                            let byte = row * l.rect.width_bytes + column / 4;
                            let shift = 6 - column % 4 * 2;
                            let pixel = &mut pixels[dy as usize * 320 + dx as usize];
                            *pixel = (*pixel & ((l.mask[byte] >> shift) & 3))
                                | ((l.color[byte] >> shift) & 3);
                        }
                    }
                }
                let actual = compose(&o, anchors).unwrap();
                for y in 0..200 {
                    for x in 0..320 {
                        assert_eq!(
                            at(&actual, x, y),
                            pixels[y * 320 + x],
                            "{anchors:?} {x},{y}"
                        );
                    }
                }
            }
        }
    }
}
