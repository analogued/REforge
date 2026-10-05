// Copyright (c) 2026 analogued. Project license has not been selected.
//! The eight terrain slots used by the bounded, no-input opening.

use super::Random;
use crate::{Error, assets::GraphicsAssets, frame::Rect};

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
    kinds: [TerrainKind; 8],
}

#[derive(Clone, Copy, Debug)]
enum TerrainKind {
    Empty,
    Type2,
    Type3,
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
            kinds: [TerrainKind::Empty; 8],
        }
    }

    /// Advance the world, then resolve sources and project the render list.
    /// The first pass projects all slots; the second includes only depth < 70.
    pub(crate) fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        for (slot, kind) in self.slots.iter_mut().zip(self.kinds) {
            if slot.active {
                // Terrain kinds 2 and 3 have no player collision effect. Their
                // retirement handler clears activity only after passing depth 14.
                // Sprite choice observes the old depth and the preceding projection's phase.
                slot.requested_sprite = sprite_choice(slot.depth, slot.phase, kind);
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
                let vertical = (scale * (-100 - i32::from(camera_height))) >> 16;
                slot.rect.y = vertical_origin - vertical - i32::from(sprite.descriptor[6]);
            }
        }
        Ok(())
    }

    /// Recycle after rendering and event processing, preserving resolved draw state.
    pub(crate) fn recycle(&mut self, selector: u16, random: &mut Random) -> Result<(), Error> {
        if !matches!(selector, 0 | 5 | 6 | 7) {
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
            match selector {
                0 => {
                    self.kinds[index] = TerrainKind::Empty;
                    slot.active = false;
                }
                5 => {
                    self.kinds[index] = TerrainKind::Type2;
                    slot.active = true;
                }
                6 if index <= 4 => {
                    self.kinds[index] = TerrainKind::Type2;
                    slot.active = true;
                }
                6 => {
                    // The excluded slots retain their previous kind and phase.
                    // Their repeated recycling still consumes a random word.
                    slot.active = false;
                }
                7 => {
                    self.kinds[index] = TerrainKind::Type3;
                    slot.active = true;
                }
                _ => unreachable!(),
            }
            slot.requested_sprite = sprite_choice(slot.depth, slot.phase, self.kinds[index]);
        }
        Ok(())
    }

    pub(crate) fn snapshots(&self) -> [TerrainSnapshot; 8] {
        self.slots
    }
}

fn perspective_scale(depth: i16) -> i32 {
    // The startup table divides the fixed-point focal length by depth + 64.
    (64 * 32768) / (i32::from(depth) + 64)
}

fn sprite_choice(depth: i16, phase: u8, kind: TerrainKind) -> usize {
    let size = 2047 / (i32::from(depth) + 64);
    let shifted = phase >= 2;
    match kind {
        TerrainKind::Type2 => match size {
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
        },
        TerrainKind::Type3 => match size {
            0..=1 => {
                if shifted {
                    158
                } else {
                    151
                }
            }
            2..=3 => {
                if shifted {
                    159
                } else {
                    152
                }
            }
            4..=5 => 153,
            6..=8 => 154,
            9..=11 => 155,
            12..=15 => 156,
            _ => 157,
        },
        TerrainKind::Empty => match size {
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
        },
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
            sprites: (0..160)
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
        terrain.kinds[0] = TerrainKind::Type2;
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.depth = 664;
        slot.lateral = -200;
        terrain.advance_half_step(&assets(), true, 106, 0).unwrap();
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
        terrain.kinds[0] = TerrainKind::Type2;
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.depth = 580;
        slot.phase = 3;
        slot.resolved_sprite = 143;
        terrain.advance_half_step(&assets(), false, 106, 0).unwrap();
        let slot = terrain.snapshots()[0];
        assert_eq!(slot.requested_sprite, 150);
        assert_eq!(slot.resolved_sprite, 143);
        terrain.slots[0].depth = 76;
        terrain.advance_half_step(&assets(), false, 106, 0).unwrap();
        assert_eq!(terrain.slots[0].depth, 48);
        assert_eq!(terrain.slots[0].resolved_sprite, 147);
        terrain.advance_half_step(&assets(), false, 106, 0).unwrap();
        terrain.advance_half_step(&assets(), false, 106, 0).unwrap();
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
        assert!(terrain.recycle(8, &mut random).is_err());
    }

    #[test]
    fn selector_change_preserves_live_rows_and_replaces_only_retired_rows() {
        let mut terrain = Terrain::new();
        let mut random = Random::default();
        terrain.recycle(5, &mut random).unwrap();
        let first_lateral = terrain.slots[0].lateral;
        terrain.slots[1].depth = 42;
        terrain.slots[1].phase = 3;
        terrain.advance_half_step(&assets(), false, 106, 0).unwrap();
        assert!(!terrain.slots[1].active);
        assert_eq!(terrain.slots[1].resolved_sprite, 148);
        let previous_rect = terrain.slots[1].rect;
        let before_recycle = random.state();
        terrain.recycle(7, &mut random).unwrap();
        assert_eq!(terrain.slots[0].depth, 888);
        assert_eq!(terrain.slots[0].lateral, first_lateral);
        assert!(matches!(terrain.kinds[0], TerrainKind::Type2));
        assert_eq!(terrain.slots[1].depth, 1028);
        assert!(matches!(terrain.kinds[1], TerrainKind::Type3));
        assert_eq!(terrain.slots[1].requested_sprite, 158);
        assert_eq!(terrain.slots[1].resolved_sprite, 148);
        assert_eq!(terrain.slots[1].rect.x_bytes, previous_rect.x_bytes);
        assert_eq!(terrain.slots[1].rect.y, previous_rect.y);
        assert_eq!(terrain.slots[1].rect.width_bytes, previous_rect.width_bytes);
        assert_eq!(terrain.slots[1].rect.height, previous_rect.height);
        assert_ne!(random.state(), before_recycle);
        terrain.advance_half_step(&assets(), true, 106, 0).unwrap();
        assert_eq!(terrain.slots[1].resolved_sprite, 158);
        assert_eq!(terrain.slots[1].rect.x_bytes, 128);
    }

    #[test]
    fn selector_six_recycles_excluded_rows_without_replacing_their_kind() {
        let mut terrain = Terrain::new();
        terrain.kinds[7] = TerrainKind::Type3;
        terrain.slots[7].phase = 3;
        let mut random = Random::default();
        let mut reference = Random::default();
        terrain.recycle(6, &mut random).unwrap();
        for _ in 0..8 {
            reference.next_word();
        }
        assert_eq!(random.state(), reference.state());
        assert!(terrain.slots[..5].iter().all(|slot| slot.active));
        assert!(terrain.slots[5..].iter().all(|slot| !slot.active));
        assert_eq!(terrain.slots[5].requested_sprite, 46);
        assert_eq!(terrain.slots[7].requested_sprite, 158);
        for slot in &mut terrain.slots[5..] {
            let next_word = reference.next_word();
            slot.lateral = (next_word & 0x7ff) as i16 - 1024;
        }
        let expected_laterals: Vec<_> =
            terrain.slots[5..].iter().map(|slot| slot.lateral).collect();
        terrain.recycle(6, &mut random).unwrap();
        assert_eq!(random.state(), reference.state());
        assert_eq!(
            terrain.slots[5..]
                .iter()
                .map(|slot| slot.lateral)
                .collect::<Vec<_>>(),
            expected_laterals
        );
        assert!(matches!(terrain.kinds[7], TerrainKind::Type3));
    }

    #[test]
    fn second_sprite_family_changes_at_size_and_phase_boundaries() {
        for (depth, aligned, shifted) in [
            (1000, 151, 158),
            (959, 152, 159),
            (448, 152, 159),
            (447, 153, 153),
            (278, 153, 153),
            (277, 154, 154),
            (164, 154, 154),
            (163, 155, 155),
            (107, 155, 155),
            (106, 156, 156),
            (64, 156, 156),
            (63, 157, 157),
        ] {
            assert_eq!(sprite_choice(depth, 1, TerrainKind::Type3), aligned);
            assert_eq!(sprite_choice(depth, 2, TerrainKind::Type3), shifted);
        }
    }
}
