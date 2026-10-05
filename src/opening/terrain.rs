// Copyright (c) 2026 analogued. Project license has not been selected.
//! The eight terrain slots used by the bounded, no-input opening.

use super::Random;
use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

#[derive(Clone, Copy, Debug)]
pub struct TerrainSnapshot {
    pub slot: usize,
    pub depth: i16,
    pub lateral: i16,
    pub active: bool,
    pub requested_sprite: usize,
    pub resolved_sprite: usize,
    pub rect: Rect,
    pub phase: u8,
}

#[derive(Debug)]
pub(crate) struct Terrain {
    slots: [TerrainSnapshot; 8],
}

impl Terrain {
    pub(crate) fn new() -> Self {
        Self {
            slots: std::array::from_fn(|index| TerrainSnapshot {
                slot: index + 15,
                depth: 1000 + index as i16 * 100,
                lateral: 0,
                active: false,
                requested_sprite: 0,
                resolved_sprite: 0,
                rect: Rect {
                    x_bytes: 128,
                    y: 0,
                    width_bytes: 2,
                    height: 2,
                },
                phase: 0,
            }),
        }
    }

    /// Advance the world, then resolve sources and project the render list.
    /// The first pass projects all slots; the second includes only depth < 70.
    pub(crate) fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
    ) -> Result<(), Error> {
        for slot in &mut self.slots {
            if slot.active {
                // Selector 5's subtype 2 has no player collision effect. Its
                // retirement handler clears activity only after passing depth 14.
                // Sprite choice observes the old depth and the preceding projection's phase.
                slot.requested_sprite = sprite_choice(slot.depth, slot.phase, true);
                slot.depth -= 28;
                if slot.depth <= 14 {
                    slot.active = false;
                }
            }
            if full_projection || slot.depth < 70 {
                let sprite = assets.sprites.get(slot.requested_sprite).ok_or_else(|| {
                    Error::InvalidData(format!("missing terrain sprite {}", slot.requested_sprite))
                })?;
                slot.resolved_sprite = slot.requested_sprite;
                slot.rect.width_bytes = sprite.width_bytes;
                slot.rect.height = sprite.height;
                if !slot.active || slot.depth >= 1000 {
                    slot.rect.x_bytes = 128;
                    continue;
                }
                let scale = perspective_scale(slot.depth);
                let horizontal = (scale * i32::from(slot.lateral)) >> 16;
                slot.phase = (horizontal & 3) as u8;
                slot.rect.x_bytes = 40 + (horizontal >> 2) - i32::from(sprite.descriptor[5]);
                let vertical = (scale * -100) >> 16;
                slot.rect.y = 106 - vertical - i32::from(sprite.descriptor[6]);
            }
        }
        Ok(())
    }

    /// Recycle after rendering and event processing, preserving resolved draw state.
    pub(crate) fn recycle(&mut self, selector: u16, random: &mut Random) -> Result<(), Error> {
        if selector != 0 && selector != 5 {
            return Err(Error::InvalidData(format!(
                "unsupported opening terrain selector {selector}"
            )));
        }
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.active {
                continue;
            }
            slot.lateral = (random.next_word() & 0x7ff) as i16 - 1024;
            slot.depth = 916 + index as i16 * 112;
            slot.active = selector == 5;
            slot.requested_sprite = sprite_choice(slot.depth, slot.phase, slot.active);
        }
        Ok(())
    }

    pub(crate) fn draws(&self) -> Vec<(i16, Draw)> {
        let mut slots: Vec<_> = self.slots.iter().filter(|slot| slot.active).collect();
        slots.sort_by_key(|slot| std::cmp::Reverse(slot.depth));
        slots
            .into_iter()
            .map(|slot| {
                (
                    slot.depth,
                    Draw {
                        resolved_sprite: slot.resolved_sprite,
                        rect: slot.rect,
                    },
                )
            })
            .collect()
    }

    pub(crate) fn snapshots(&self) -> [TerrainSnapshot; 8] {
        self.slots
    }
}

fn perspective_scale(depth: i16) -> i32 {
    // The startup table divides the fixed-point focal length by depth + 64.
    (64 * 32768) / (i32::from(depth) + 64)
}

fn sprite_choice(depth: i16, phase: u8, visible_type: bool) -> usize {
    let size = 2047 / (i32::from(depth) + 64);
    let shifted = phase >= 2;
    if visible_type {
        match size {
            0..=1 => {
                if shifted {
                    149
                } else {
                    143
                }
            }
            2 => {
                if shifted {
                    150
                } else {
                    143
                }
            }
            3 => {
                if shifted {
                    150
                } else {
                    144
                }
            }
            4..=7 => 145,
            8..=11 => 146,
            12..=15 => 147,
            _ => 148,
        }
    } else {
        match size {
            0..=1 => {
                if shifted {
                    54
                } else {
                    46
                }
            }
            2..=3 => {
                if shifted {
                    55
                } else {
                    47
                }
            }
            4..=5 => {
                if shifted {
                    56
                } else {
                    48
                }
            }
            6..=8 => 49,
            9..=14 => 50,
            15..=19 => 51,
            20..=24 => 52,
            _ => 53,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Sprite;

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![],
            alternate: vec![],
            sprites: (0..151)
                .map(|_| Sprite {
                    width_bytes: 4,
                    height: 7,
                    descriptor: [1, 1, 1, 4, 7, 2, 5],
                    color: vec![0; 28],
                    mask: vec![255; 28],
                })
                .collect(),
        }
    }

    #[test]
    fn projection_uses_signed_products_and_previous_phase_for_sprite_choice() {
        let mut terrain = Terrain::new();
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.depth = 664;
        slot.lateral = -200;
        terrain.advance_half_step(&assets(), true).unwrap();
        let slot = terrain.snapshots()[0];
        assert_eq!(slot.depth, 636);
        assert_eq!(slot.requested_sprite, 143);
        assert_eq!(slot.phase, 2);
        assert_eq!(slot.rect.x_bytes, 35);
        assert_eq!(slot.rect.y, 106);
    }

    #[test]
    fn second_render_retains_sources_until_the_near_list_boundary() {
        let mut terrain = Terrain::new();
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.depth = 580;
        slot.phase = 3;
        slot.resolved_sprite = 143;
        terrain.advance_half_step(&assets(), false).unwrap();
        let slot = terrain.snapshots()[0];
        assert_eq!(slot.requested_sprite, 150);
        assert_eq!(slot.resolved_sprite, 143);
        terrain.slots[0].depth = 76;
        terrain.advance_half_step(&assets(), false).unwrap();
        assert_eq!(terrain.slots[0].depth, 48);
        assert_eq!(terrain.slots[0].resolved_sprite, 147);
        terrain.advance_half_step(&assets(), false).unwrap();
        terrain.advance_half_step(&assets(), false).unwrap();
        assert!(!terrain.slots[0].active);
        assert_eq!(terrain.slots[0].rect.x_bytes, 128);
    }

    #[test]
    fn recycling_consumes_randomness_only_for_inactive_slots_and_retains_draw_state() {
        let mut terrain = Terrain::new();
        for slot in &mut terrain.slots[1..] {
            slot.active = true;
        }
        terrain.slots[0].resolved_sprite = 148;
        terrain.slots[0].rect.y = 140;
        let mut random = Random::default();
        terrain.recycle(5, &mut random).unwrap();
        let slot = terrain.snapshots()[0];
        assert_eq!(slot.lateral, (0x5630 & 0x7ff) - 1024);
        assert_eq!(slot.depth, 916);
        assert_eq!(slot.requested_sprite, 143);
        assert_eq!(slot.resolved_sprite, 148);
        assert_eq!(slot.rect.y, 140);
        assert!(slot.active);
        assert_eq!(random.next_word(), 0xbe90);
        assert!(terrain.recycle(7, &mut random).is_err());
    }
}
