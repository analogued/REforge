// Copyright (c) 2026 analogued. Project license has not been selected.
//! Procedural gate records allocated by the bounded opening's command 40.

use super::Random;
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
};

#[derive(Clone, Copy, Debug)]
pub struct SpecialSnapshot {
    pub slot: usize,
    pub coordinates: [i16; 3],
    pub depth: i16,
    pub lateral: i16,
    pub height: i16,
    pub extents: [i16; 3],
    pub active: bool,
    pub movement: u8,
    pub requested_sprite: usize,
    pub resolved_sprite: usize,
    pub phase: u8,
    pub scale: u16,
    pub rect: Rect,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SpecialCollision {
    pub points: u16,
    /// Zero still starts a player response; None means no response call.
    pub damage: Option<u16>,
}

#[derive(Debug)]
pub(crate) struct Special {
    slots: [SpecialSnapshot; 5],
}

impl Special {
    pub(crate) fn shift_lateral(&mut self, delta: i16) {
        for record in &mut self.slots {
            record.lateral = record.lateral.wrapping_add(delta);
            record.coordinates[1] = record.lateral;
        }
    }

    pub(crate) fn from_snapshots(slots: [SpecialSnapshot; 5]) -> Self {
        Self { slots }
    }

    pub(crate) fn new() -> Self {
        Self {
            slots: std::array::from_fn(|index| SpecialSnapshot {
                slot: 23 + index,
                coordinates: [0; 3],
                depth: 0,
                lateral: 0,
                height: 0,
                extents: [0; 3],
                active: false,
                movement: 1,
                requested_sprite: 0,
                resolved_sprite: 0,
                phase: 0,
                scale: 0,
                rect: Rect {
                    x_bytes: 128,
                    y: 0,
                    width_bytes: 0,
                    height: 0,
                },
            }),
        }
    }

    /// Allocation happens after drawing and consumes two random words. The
    /// selected runtime's initialized sentinel permits the first word's check.
    pub(crate) fn spawn_command_40(
        &mut self,
        random: &mut Random,
        forward_speed: i16,
        alternate_mode: bool,
    ) {
        random.next_word();
        let lateral = (random.next_word() & 511) as i16;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slot.depth = forward_speed
                .wrapping_mul(32 + index as i16 * 8)
                .wrapping_add(20);
            slot.lateral = lateral;
            slot.height = -100;
            slot.coordinates = [slot.depth, slot.lateral, slot.height];
            slot.extents = [forward_speed, 350, 130];
            slot.rect.width_bytes = 350 >> 3;
            slot.requested_sprite = if alternate_mode { 3 } else { 0 };
            slot.active = true;
        }
    }

    /// Movement one advances by the global forward speed. Retirement clears
    /// activity only, retaining source, geometry, and projection state.
    pub(crate) fn update_half_step(&mut self, player_depth: i16, forward_speed: i16) {
        for slot in &mut self.slots {
            if slot.active {
                slot.depth = slot.depth.wrapping_sub(forward_speed);
                slot.coordinates[0] = slot.depth;
                if slot.depth <= player_depth {
                    slot.active = false;
                }
            }
        }
    }

    /// These records bypass sprite resolution and are excluded from the near
    /// projection list even when their depth is below seventy.
    pub(crate) fn project(
        &mut self,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        if !full_projection {
            return Ok(());
        }
        for slot in &mut self.slots {
            if !slot.active || slot.depth >= 1000 {
                slot.rect.x_bytes = 128;
                continue;
            }
            let scale = (64 * 32768) / (i32::from(slot.depth) + 64);
            slot.scale = scale as u16;
            let horizontal = (scale * i32::from(slot.lateral)) >> 16;
            slot.phase = (horizontal & 3) as u8;
            slot.rect.x_bytes = 40 + (horizontal >> 2);
            let vertical = (scale * i32::from(slot.height.wrapping_sub(camera_height))) >> 16;
            slot.rect.y = vertical_origin - vertical;
        }
        Ok(())
    }

    /// The broad overlap precedes a stricter opening-containment check. Phase
    /// gates all effects; grace gates the returned response call in Player.
    pub(crate) fn player_collision(
        &self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        collidable: bool,
        can_damage: bool,
    ) -> Result<Option<SpecialCollision>, Error> {
        if !collidable {
            return Ok(None);
        }
        let source = assets.sprites.get(player_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing player collision sprite {player_sprite}"))
        })?;
        let mut collision = None;
        for slot in &self.slots {
            if !slot.active {
                continue;
            }
            let overlaps = |position: i16, extent: i16, target: i16, target_extent: i16| {
                position.wrapping_add(extent) >= target
                    && position <= target.wrapping_add(target_extent)
            };
            if !overlaps(player[0], source.descriptor[2], slot.depth, slot.extents[0])
                || !overlaps(
                    player[1],
                    source.descriptor[1],
                    slot.lateral,
                    slot.extents[1],
                )
                || !overlaps(player[2], slot.extents[2], slot.height, slot.extents[2])
            {
                continue;
            }
            let effect = collision.get_or_insert(SpecialCollision::default());
            let contained = player[2].wrapping_add(source.descriptor[0])
                <= slot.height.wrapping_add(slot.extents[2])
                && player[1] >= slot.lateral
                && player[1].wrapping_add(source.descriptor[1])
                    <= slot.lateral.wrapping_add(slot.extents[1]);
            match slot.requested_sprite {
                0 | 3 => {
                    if contained {
                        effect.points = effect.points.wrapping_add(1000);
                    } else {
                        effect.damage = Some(0);
                    }
                }
                _ => {
                    return Err(Error::InvalidData(
                        "unsupported special collision mode".into(),
                    ));
                }
            }
            if effect.damage.is_some() && can_damage {
                break;
            }
        }
        Ok(collision)
    }

    pub(crate) fn snapshots(&self) -> [SpecialSnapshot; 5] {
        self.slots
    }

    /// Insert each gate at its retained position among the ordinary world draws.
    /// The mask geometry rounds its horizontal start to an even pixel.
    pub(crate) fn draw_slot(
        &self,
        frame: &mut Frame,
        slot: usize,
        clip: Rect,
    ) -> Result<(), Error> {
        let record = slot
            .checked_sub(23)
            .and_then(|index| self.slots.get(index))
            .ok_or_else(|| Error::InvalidData(format!("invalid special slot {slot}")))?;
        if !record.active || record.rect.x_bytes >= 128 {
            return Ok(());
        }
        if record.requested_sprite != 0 {
            return Err(Error::InvalidData(
                "unsupported special rendering mode".into(),
            ));
        }
        let scale = i32::from(record.scale);
        let height = (i32::from(record.extents[2]) * scale) >> 16;
        let half_width = (i32::from(record.extents[1] >> 1) * scale) >> 16;
        let thickness = ((record.rect.width_bytes as i32 * scale) >> 16).clamp(1, 10);
        let left = record.rect.x_bytes * 4 + i32::from(record.phase >> 1) * 2;
        let right = left + half_width * 2;
        let top = record.rect.y - height;
        clear_rectangle(frame, clip, left, top, right - left + thickness, thickness);
        clear_rectangle(frame, clip, left, top, thickness, height + 1);
        clear_rectangle(frame, clip, right, top, thickness, height + 1);
        Ok(())
    }
}

fn clear_rectangle(frame: &mut Frame, clip: Rect, x: i32, y: i32, width: i32, height: i32) {
    let left = x.max(clip.x_bytes * 4).max(0);
    let right = (x + width)
        .min((clip.x_bytes + clip.width_bytes as i32) * 4)
        .min(320);
    let top = y.max(clip.y).max(0);
    let bottom = (y + height).min(clip.y + clip.height as i32).min(200);
    for scanline in top..bottom {
        for byte_x in (left >> 2)..((right + 3) >> 2) {
            let mut mask = 255_u8;
            for pixel in left.max(byte_x * 4)..right.min(byte_x * 4 + 4) {
                mask &= !(3 << (6 - (pixel & 3) * 2));
            }
            frame.mask_byte(byte_x as usize, scanline as usize, mask);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assets::Sprite, frame::FRAME_BYTES};

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: Vec::new(),
            alternate: Vec::new(),
            sprites: vec![Sprite {
                width_bytes: 2,
                height: 2,
                descriptor: [7, 9, 15, 0, 0, 0, 0],
                color: vec![0; 4],
                mask: vec![255; 4],
            }],
        }
    }

    #[test]
    fn creation_and_retirement_preserve_resolved_geometry() {
        let mut special = Special::new();
        let mut random = Random::default();
        let initial = special.snapshots();
        assert!(initial.iter().all(|slot| !slot.active
            && slot.movement == 1
            && slot.rect.width_bytes == 0
            && slot.rect.height == 0));
        special.spawn_command_40(&mut random, 28, false);
        assert_eq!(random.state(), [19557, 15414, 48784]);
        assert_eq!(
            special.slots.map(|slot| slot.depth),
            [916, 1140, 1364, 1588, 1812]
        );
        assert!(special.slots.iter().all(|slot| slot.lateral == 144
            && slot.extents == [28, 350, 130]
            && slot.rect.width_bytes == 43
            && slot.rect.x_bytes == 128));
        special.project(true, 106, 0).unwrap();
        let first = special.slots[0];
        special.update_half_step(14, 900);
        special.project(false, 120, 18).unwrap();
        assert_eq!(special.slots[0].rect.x_bytes, first.rect.x_bytes);
        special.update_half_step(14, 28);
        assert!(!special.slots[0].active);
        assert_eq!(special.slots[0].requested_sprite, 0);
        assert_eq!(special.slots[0].scale, first.scale);
        special.project(true, 120, 18).unwrap();
        assert_eq!(special.slots[0].rect.x_bytes, 128);
        assert_eq!(special.slots[0].rect.y, first.rect.y);
    }

    #[test]
    fn crossing_checks_containment_and_preserves_zero_damage_response() {
        let mut special = Special::new();
        let slot = &mut special.slots[0];
        slot.active = true;
        slot.depth = 15;
        slot.lateral = 0;
        slot.height = -100;
        slot.extents = [28, 350, 130];
        let assets = assets();
        let effect = special
            .player_collision(&assets, [14, 0, -28], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(effect.points, 1000);
        assert_eq!(effect.damage, None);
        let effect = special
            .player_collision(&assets, [14, -1, -28], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(effect.points, 0);
        assert_eq!(effect.damage, Some(0));
        let effect = special
            .player_collision(&assets, [14, 0, 24], 0, true, false)
            .unwrap()
            .unwrap();
        assert_eq!(effect.damage, Some(0));
        assert!(
            special
                .player_collision(&assets, [14, 0, -28], 0, false, true)
                .unwrap()
                .is_none()
        );
        special.slots[0].depth = 30;
        assert!(
            special
                .player_collision(&assets, [14, 0, -28], 0, true, true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn procedural_gate_masks_pixels_and_clips_without_filling_the_opening() {
        let mut special = Special::new();
        let slot = &mut special.slots[0];
        slot.active = true;
        slot.scale = 16384;
        slot.extents = [28, 40, 20];
        slot.rect = Rect {
            x_bytes: 4,
            y: 50,
            width_bytes: 8,
            height: 0,
        };
        slot.phase = 3;
        let mut frame = Frame::from_packed(&vec![255; FRAME_BYTES]).unwrap();
        special
            .draw_slot(
                &mut frame,
                23,
                Rect {
                    x_bytes: 4,
                    y: 44,
                    width_bytes: 72,
                    height: 114,
                },
            )
            .unwrap();
        assert_eq!(frame.packed()[45 * 80 + 4], 0xf0);
        assert_eq!(frame.packed()[45 * 80 + 5], 0);
        assert_eq!(frame.packed()[45 * 80 + 6], 0);
        assert_eq!(frame.packed()[47 * 80 + 4], 0xf0);
        assert_eq!(frame.packed()[47 * 80 + 5], 255);
        assert_eq!(frame.packed()[47 * 80 + 7], 0x0f);
        assert_eq!(frame.packed()[50 * 80 + 4], 0xf0);
        assert_eq!(frame.packed()[51 * 80 + 4], 255);
        assert!(frame.packed()[..44 * 80].iter().all(|byte| *byte == 255));
    }
}
