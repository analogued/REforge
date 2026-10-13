// Copyright (c) 2026 analogued. Project license has not been selected.
//! Procedural gates and panels created by gameplay commands.

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
    /// Ascending-record awards; zero entries are unused.
    pub awards: [u16; 5],
    /// Zero still starts a player response; None means no response call.
    pub damage: Option<u16>,
    pub transition: Option<SpecialTransition>,
    pub contacts: [Option<SpecialContact>; 5],
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SpecialContact {
    Award {
        site: u16,
        points: u16,
        transition: Option<SpecialTransition>,
    },
    Damage(u16),
}

impl SpecialCollision {
    fn contact(&mut self, contact: SpecialContact) {
        *self
            .contacts
            .iter_mut()
            .find(|c| c.is_none())
            .expect("at most five contacts") = Some(contact);
    }

    fn award(&mut self, points: u16) {
        self.points = self.points.wrapping_add(points);
        let next = self
            .awards
            .iter_mut()
            .find(|value| **value == 0)
            .expect("at most one award per special record");
        *next = points;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpecialTransition {
    Normal,
    Alternate,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SpecialSpawn {
    pub terrain_selector: Option<u16>,
    pub goal_barrier: bool,
}

#[derive(Debug)]
pub(crate) struct Special {
    slots: [SpecialSnapshot; 5],
    continuous: bool,
}

impl Special {
    pub(crate) fn shift_lateral(&mut self, delta: i16) {
        for record in &mut self.slots {
            record.lateral = record.lateral.wrapping_add(delta);
            record.coordinates[1] = record.lateral;
        }
    }

    pub(crate) fn from_snapshots(slots: [SpecialSnapshot; 5]) -> Self {
        Self {
            slots,
            continuous: false,
        }
    }

    pub(crate) fn new() -> Self {
        Self {
            continuous: false,
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
        self.spawn_command(0x40, random, forward_speed, alternate_mode)
            .expect("command 40 is supported");
    }

    /// Allocate procedural records after drawing. All prior projected fields
    /// and remembered sources remain until the next full projection.
    pub(crate) fn spawn_command(
        &mut self,
        command: u8,
        random: &mut Random,
        forward_speed: i16,
        alternate_mode: bool,
    ) -> Result<SpecialSpawn, Error> {
        if !(0x40..=0x46).contains(&command) {
            return Err(Error::InvalidData(format!(
                "invalid special command {command:02x}"
            )));
        }
        self.continuous = command == 0x44;
        if command == 0x45 {
            return Ok(SpecialSpawn::default());
        }
        let lateral = match command {
            0x40 => {
                random.next_word();
                (random.next_word() & 511) as i16
            }
            0x41 => (random.next_word() & 0x32b) as i16,
            0x42 => (random.next_word() & 511) as i16 - 700,
            0x43 => (random.next_word() & 255) as i16,
            _ => 0,
        };
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let i = index as i16;
            slot.depth = if command == 0x43 {
                1400 - 32 * i
            } else if command == 0x46 {
                1084_i16.wrapping_add(forward_speed.wrapping_mul(i))
            } else {
                916_i16.wrapping_add(forward_speed.wrapping_mul(8 * i))
            };
            slot.lateral = match command {
                0x41 => lateral.wrapping_sub(i << 8),
                0x42 => lateral.wrapping_add(i << 8),
                0x44 => (random.next_word() & 1023) as i16 - 600,
                0x46 => -500,
                _ => lateral,
            };
            slot.height = -100;
            slot.coordinates = [slot.depth, slot.lateral, slot.height];
            let width = match command {
                0x43 => 300 + (i << 4),
                0x46 => 400,
                _ => 350,
            };
            slot.extents = [forward_speed, width, ((width >> 1) - (width >> 3)).min(130)];
            slot.rect.width_bytes = (width >> 3) as usize;
            slot.requested_sprite = match command {
                0x43 => {
                    if alternate_mode {
                        1
                    } else {
                        2
                    }
                }
                0x46 if index == 4 => 2,
                0x46 => 4,
                _ => {
                    if alternate_mode {
                        3
                    } else {
                        0
                    }
                }
            };
            slot.active = true;
        }
        Ok(SpecialSpawn {
            terrain_selector: (command == 0x46).then_some(1),
            goal_barrier: command == 0x46,
        })
    }

    /// Command 44 enables recycling after terrain recycling. Each inactive
    /// record consumes its own lateral word and refreshes its extents.
    pub(crate) fn recycle_continuous(
        &mut self,
        random: &mut Random,
        forward_speed: i16,
        alternate_mode: bool,
    ) {
        if !self.continuous {
            return;
        }
        for slot in &mut self.slots {
            if slot.active {
                continue;
            }
            slot.extents = [forward_speed, 350, 130];
            slot.rect.width_bytes = 43;
            slot.requested_sprite = if alternate_mode { 3 } else { 0 };
            slot.height = -100;
            slot.active = true;
            slot.lateral = (random.next_word() & 1023) as i16 - 600;
            slot.depth = 832;
            slot.coordinates = [slot.depth, slot.lateral, slot.height];
        }
    }

    pub(crate) fn clear_active(&mut self) {
        for slot in &mut self.slots {
            slot.active = false;
        }
    }

    pub(crate) fn reset_for_game(&mut self) {
        self.continuous = false;
        for slot in &mut self.slots {
            slot.active = false;
            slot.movement = 1;
        }
    }

    pub(crate) fn restore_snapshots(&mut self, snapshots: [SpecialSnapshot; 5]) {
        self.slots = snapshots;
        self.reset_for_game();
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
            let scale = ((64 * 32768) / (i32::from(slot.depth & 1023) + 64)) as i16;
            slot.scale = scale as u16;
            let scale = i32::from(scale);
            let horizontal = (scale * i32::from(slot.lateral)) >> 16;
            slot.phase = (horizontal & 3) as u8;
            slot.rect.x_bytes = 40 + (horizontal >> 2);
            let vertical = (scale * i32::from(slot.height.wrapping_sub(camera_height))) >> 16;
            slot.rect.y = vertical_origin - vertical;
        }
        Ok(())
    }

    /// The broad overlap precedes a stricter gameplay-containment check. Phase
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
                        effect.award(1000);
                        effect.contact(SpecialContact::Award {
                            site: 0x37a3,
                            points: 1000,
                            transition: None,
                        });
                    } else {
                        effect.damage = Some(0);
                        effect.contact(SpecialContact::Damage(0));
                    }
                }
                1 | 2 => {
                    effect.award(1000);
                    effect.transition = Some(if slot.requested_sprite == 1 {
                        SpecialTransition::Normal
                    } else {
                        SpecialTransition::Alternate
                    });
                    effect.contact(SpecialContact::Award {
                        site: if slot.requested_sprite == 1 {
                            0x372b
                        } else {
                            0x3746
                        },
                        points: 1000,
                        transition: effect.transition,
                    });
                    break;
                }
                4 => {
                    if contained {
                        effect.award(550);
                        effect.contact(SpecialContact::Award {
                            site: 0x37fd,
                            points: 550,
                            transition: None,
                        });
                    } else {
                        effect.damage = Some(2);
                        effect.contact(SpecialContact::Damage(2));
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
        if record.rect.x_bytes >= 128 {
            return Ok(());
        }
        if record.requested_sprite > 4 {
            return Err(Error::InvalidData(
                "unsupported special rendering mode".into(),
            ));
        }
        let scale = i32::from(record.scale as i16);
        let height = (i32::from(record.extents[2]) * scale) >> 16;
        let half_width = (i32::from(record.extents[1] >> 1) * scale) >> 16;
        let thickness = ((record.rect.width_bytes as i32 * scale) >> 16).clamp(1, 10);
        let left = record.rect.x_bytes * 4 + i32::from(record.phase >> 1) * 2;
        let right = left + half_width * 2;
        let top = record.rect.y - height;
        let mut packed = frame.packed().to_vec();
        let mode = record.requested_sprite;
        if matches!(mode, 1 | 2) {
            paint_rectangle(
                &mut packed,
                clip,
                [left, top, right - left + 1, height],
                mode,
            );
        } else {
            paint_rectangle(
                &mut packed,
                clip,
                [left, top, right - left + thickness, thickness],
                mode,
            );
            paint_rectangle(&mut packed, clip, [left, top, thickness, height + 1], mode);
            paint_rectangle(&mut packed, clip, [right, top, thickness, height + 1], mode);
        }
        if matches!(mode, 0 | 2) {
            for (index, byte) in packed.into_iter().enumerate() {
                frame.mask_byte(index % 80, index / 80, byte);
            }
        } else {
            *frame = Frame::from_packed(&packed)?;
        }
        Ok(())
    }
}

fn paint_rectangle(packed: &mut [u8], clip: Rect, bounds: [i32; 4], mode: usize) {
    let [x, y, width, height] = bounds;
    let left = x.max(clip.x_bytes * 4).max(0);
    let right = (x + width)
        .min((clip.x_bytes + clip.width_bytes as i32) * 4)
        .min(320);
    let top = y.max(clip.y).max(0);
    let bottom = (y + height).min(clip.y + clip.height as i32).min(200);
    for scanline in top..bottom {
        for byte_x in (left >> 2)..((right + 3) >> 2) {
            for pixel in left.max(byte_x * 4)..right.min(byte_x * 4 + 4) {
                let shift = 6 - (pixel & 3) * 2;
                let color = match mode {
                    0 | 2 => 0,
                    1 | 3 => 3,
                    4 => {
                        if pixel & 1 == 0 {
                            2
                        } else {
                            1
                        }
                    }
                    _ => unreachable!("validated procedural drawing mode"),
                };
                let byte = &mut packed[scanline as usize * 80 + byte_x as usize];
                *byte = (*byte & !(3 << shift)) | (color << shift);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assets::Sprite, frame::FRAME_BYTES};

    #[test]
    fn allocation_uses_constant_base_and_noop_disables_continuous_recycling() {
        let mut special = Special::new();
        let mut random = Random::default();
        special.spawn_command(0x41, &mut random, 40, true).unwrap();
        assert_eq!(
            special.slots.map(|slot| slot.depth),
            [916, 1236, 1556, 1876, 2196]
        );
        assert_eq!(special.slots[1].lateral, special.slots[0].lateral - 256);
        assert!(special.slots.iter().all(|slot| slot.requested_sprite == 3));
        special.spawn_command(0x44, &mut random, 40, false).unwrap();
        special.clear_active();
        special.recycle_continuous(&mut random, 56, true);
        assert!(special.slots.iter().all(|slot| slot.depth == 832
            && slot.extents == [56, 350, 130]
            && slot.requested_sprite == 3
            && slot.active));
        let retained = special.slots;
        let retained_random = random.state();
        special.spawn_command(0x45, &mut random, 56, true).unwrap();
        assert_eq!(
            special.slots.map(|slot| slot.coordinates),
            retained.map(|slot| slot.coordinates)
        );
        special.clear_active();
        special.recycle_continuous(&mut random, 56, false);
        assert!(special.slots.iter().all(|slot| !slot.active));
        assert_eq!(random.state(), retained_random);
    }

    #[test]
    fn goal_and_panel_modes_report_transitions_and_retain_fields_on_reset() {
        let mut special = Special::new();
        let mut random = Random::default();
        let effect = special.spawn_command(0x46, &mut random, 40, false).unwrap();
        assert_eq!(effect.terrain_selector, Some(1));
        assert!(effect.goal_barrier);
        assert_eq!(
            special.slots.map(|slot| slot.depth),
            [1084, 1124, 1164, 1204, 1244]
        );
        assert_eq!(
            special.slots.map(|slot| slot.requested_sprite),
            [4, 4, 4, 4, 2]
        );
        special.clear_active();
        let slot = &mut special.slots[4];
        slot.active = true;
        slot.depth = 15;
        slot.lateral = -500;
        let collision = special
            .player_collision(&assets(), [14, -501, -100], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(collision.transition, Some(SpecialTransition::Alternate));
        assert_eq!(collision.points, 1000);
        assert_eq!(collision.damage, None);
        special.slots[4].requested_sprite = 1;
        let collision = special
            .player_collision(&assets(), [14, -501, -100], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(collision.transition, Some(SpecialTransition::Normal));
        special.restore_snapshots(special.snapshots());
        assert_eq!(special.slots[4].requested_sprite, 1);
        assert_eq!(special.slots[4].depth, 15);
        assert_eq!(special.slots[4].movement, 1);
        assert!(!special.slots[4].active);
    }

    #[test]
    fn procedural_modes_fill_panels_and_paint_alternating_gate_colors() {
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
        let clip = Rect {
            x_bytes: 4,
            y: 44,
            width_bytes: 72,
            height: 114,
        };
        special.slots[0].requested_sprite = 1;
        let mut frame = Frame::from_packed(&vec![0; FRAME_BYTES]).unwrap();
        special.draw_slot(&mut frame, 23, clip).unwrap();
        assert_eq!(frame.packed()[47 * 80 + 5], 255);
        assert_eq!(frame.packed()[49 * 80 + 6], 252);
        assert_eq!(frame.packed()[50 * 80 + 5], 0);
        special.slots[0].requested_sprite = 4;
        let mut frame = Frame::from_packed(&vec![255; FRAME_BYTES]).unwrap();
        special.draw_slot(&mut frame, 23, clip).unwrap();
        assert_eq!(frame.packed()[45 * 80 + 5], 0x99);
        assert_eq!(frame.packed()[47 * 80 + 5], 255);
        assert_eq!(frame.packed()[47 * 80 + 4], 0x9f);
        special.slots[0].requested_sprite = 3;
        let mut frame = Frame::from_packed(&vec![0; FRAME_BYTES]).unwrap();
        special.draw_slot(&mut frame, 23, clip).unwrap();
        assert_eq!(frame.packed()[45 * 80 + 5], 255);
        assert_eq!(frame.packed()[47 * 80 + 5], 0);
    }

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
    fn overlapping_gates_preserve_individual_awards_in_record_order() {
        let mut special = Special::new();
        for (slot, mode) in special.slots.iter_mut().take(3).zip([0, 4, 3]) {
            slot.active = true;
            slot.depth = 15;
            slot.lateral = 0;
            slot.height = -100;
            slot.extents = [28, 350, 130];
            slot.requested_sprite = mode;
        }
        let effect = special
            .player_collision(&assets(), [14, 0, -100], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!(effect.points, 2550);
        assert_eq!(effect.awards, [1000, 550, 1000, 0, 0]);
        assert_eq!(effect.damage, None);
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
        // Procedural records bypass the activity check during drawing. Their
        // last projection remains visible until a full projection hides them.
        special.slots[0].active = false;
        let mut retained = Frame::from_packed(&vec![255; FRAME_BYTES]).unwrap();
        let clip = Rect {
            x_bytes: 4,
            y: 44,
            width_bytes: 72,
            height: 114,
        };
        special.draw_slot(&mut retained, 23, clip).unwrap();
        assert_eq!(retained, frame);
        special.project(true, 106, 0).unwrap();
        let mut hidden = Frame::from_packed(&vec![255; FRAME_BYTES]).unwrap();
        special.draw_slot(&mut hidden, 23, clip).unwrap();
        assert!(hidden.packed().iter().all(|&byte| byte == 255));
    }
}
