// Copyright (c) 2026 analogued. Project license has not been selected.
//! The eight terrain slots used by the bounded, no-input opening.

use super::Random;
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::Rect,
    sound::{self, Effect, Foreground, Ramp},
};

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
    pub kind: u8,
    pub movement: u8,
    pub animation_phase: u8,
}

#[derive(Debug)]
pub(crate) struct Terrain {
    slots: [TerrainSnapshot; 8],
    kinds: [TerrainKind; 8],
    data_segment: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug)]
enum TerrainKind {
    Type0,
    Type1,
    Type2,
    Type3,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TerrainCollision {
    pub fuel: u16,
    pub points: u16,
    /// Ascending-record awards; zero entries are unused.
    pub awards: [u16; 8],
    pub damage: u16,
    pub contacts: [Option<TerrainContact>; 8],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainContact {
    pub fuel: u16,
    pub points: u16,
    pub damage: u16,
    pub converted: bool,
}

impl Terrain {
    pub(crate) fn shift_lateral(&mut self, delta: i16) {
        for record in &mut self.slots {
            record.lateral = record.lateral.wrapping_add(delta);
        }
    }

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
                kind: 0,
                movement: 0,
                animation_phase: 0,
            }),
            kinds: [TerrainKind::Type0; 8],
            data_segment: None,
        }
    }

    /// Advance every terrain row before collision and source resolution.
    #[cfg(test)]
    pub(crate) fn update_half_step(&mut self, player: [i16; 3]) -> u16 {
        self.update_half_step_at_speed(player, 28)
    }

    #[cfg(test)]
    pub(crate) fn update_half_step_at_speed(
        &mut self,
        player: [i16; 3],
        forward_speed: i16,
    ) -> u16 {
        self.update_half_step_at_speed_with_sound(player, forward_speed, &mut |_| {})
    }

    pub(crate) fn update_half_step_at_speed_with_sound(
        &mut self,
        player: [i16; 3],
        forward_speed: i16,
        sound: &mut Foreground<'_>,
    ) -> u16 {
        let mut fuel = 0_u16;
        for (slot, kind) in self.slots.iter_mut().zip(self.kinds) {
            if !slot.active {
                continue;
            }
            match slot.movement {
                2 => {
                    slot.requested_sprite = sprite_choice(slot.depth, slot.phase, kind);
                    if proximity_effect_at_speed(slot, kind, player, forward_speed) {
                        if matches!(kind, TerrainKind::Type1) {
                            fuel = fuel.wrapping_add(512);
                        }
                        sound::request(sound, 0x397c, Effect::Ramp(Ramp::Third));
                        start_effect_at_speed(slot, forward_speed);
                    }
                    // A newly converted effect still applies this half-step's delta.
                    slot.depth = slot.depth.wrapping_sub(forward_speed);
                    if slot.depth <= player[0] {
                        slot.active = false;
                        if slot.movement != 2 {
                            slot.requested_sprite = 0;
                        }
                    }
                }
                3 | 252 => {
                    let sprite = effect_sprite_from_data(slot, kind, self.data_segment.as_deref());
                    if sprite == 0 {
                        slot.active = false;
                    } else {
                        slot.requested_sprite = sprite;
                        slot.animation_phase = slot.animation_phase.wrapping_add(1);
                    }
                    // Both effect subtypes clear activity and the requested source.
                    if slot.depth <= player[0] {
                        slot.active = false;
                        slot.requested_sprite = 0;
                    }
                }
                _ => unreachable!("active terrain has unsupported movement subtype"),
            }
        }
        fuel
    }

    /// Collision uses the preceding resolved sources and ascending slot order.
    /// Grace gates damage only; collection/conversion still occur during grace.
    #[cfg(test)]
    pub(crate) fn player_collision(
        &mut self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        collidable: bool,
        can_damage: bool,
    ) -> Result<Option<TerrainCollision>, Error> {
        self.player_collision_at_speed(assets, player, player_sprite, collidable, can_damage, 28)
    }

    pub(crate) fn player_collision_at_speed(
        &mut self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        collidable: bool,
        can_damage: bool,
        forward_speed: i16,
    ) -> Result<Option<TerrainCollision>, Error> {
        if !collidable {
            return Ok(None);
        }
        let player_source = assets.sprites.get(player_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing player collision sprite {player_sprite}"))
        })?;
        let mut collision = None;
        for (slot, kind) in self.slots.iter_mut().zip(self.kinds) {
            if !slot.active
                || !matches!(slot.movement, 2 | 252)
                || !matches!(kind, TerrainKind::Type0 | TerrainKind::Type1)
            {
                continue;
            }
            let source = assets.sprites.get(slot.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!(
                    "missing terrain collision sprite {}",
                    slot.resolved_sprite
                ))
            })?;
            let overlaps = |position: i16, extent: i16, target: i16, target_extent: i16| {
                position.wrapping_add(extent) >= target
                    && position <= target.wrapping_add(target_extent)
            };
            if overlaps(
                player[0],
                player_source.descriptor[2],
                slot.depth,
                source.descriptor[2],
            ) && overlaps(
                player[1],
                player_source.descriptor[1],
                slot.lateral,
                source.descriptor[1],
            ) && player[2].wrapping_add(source.descriptor[0]) >= -100
                && player[2] <= (-100_i16).wrapping_add(source.descriptor[0])
            {
                let effects = collision.get_or_insert(TerrainCollision::default());
                let converted = slot.movement == 2;
                let refuel = converted && matches!(kind, TerrainKind::Type1);
                let next = effects
                    .contacts
                    .iter_mut()
                    .find(|c| c.is_none())
                    .expect("at most eight contacts");
                *next = Some(TerrainContact {
                    fuel: if refuel { 512 } else { 0 },
                    points: if refuel { 400 } else { 0 },
                    damage: if converted { 2 } else { 1 },
                    converted,
                });
                effects.damage = if slot.movement == 252 { 1 } else { 2 };
                if slot.movement == 2 {
                    if matches!(kind, TerrainKind::Type1) {
                        effects.fuel = effects.fuel.wrapping_add(512);
                        effects.points = effects.points.wrapping_add(400);
                        let next = effects
                            .awards
                            .iter_mut()
                            .find(|value| **value == 0)
                            .expect("at most one award per terrain record");
                        *next = 400;
                    }
                    start_effect_at_speed(slot, forward_speed);
                }
                if can_damage {
                    break;
                }
            }
        }
        Ok(collision)
    }

    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        for slot in &mut self.slots {
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

    #[cfg(test)]
    fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        self.update_half_step([14, 0, -100]);
        self.project(assets, full_projection, vertical_origin, camera_height)
    }

    /// Recycle after rendering and event processing, preserving resolved draw state.
    #[cfg(test)]
    pub(crate) fn recycle(&mut self, selector: u16, random: &mut Random) -> Result<(), Error> {
        self.recycle_at(selector, random, 0)
    }

    #[cfg(test)]
    pub(crate) fn recycle_at(
        &mut self,
        selector: u16,
        random: &mut Random,
        player_lateral: i16,
    ) -> Result<(), Error> {
        self.recycle_at_speed(selector, random, player_lateral, 28)
    }

    pub(crate) fn recycle_at_speed(
        &mut self,
        selector: u16,
        random: &mut Random,
        player_lateral: i16,
        forward_speed: i16,
    ) -> Result<(), Error> {
        if selector > 9 {
            return Err(Error::InvalidData(format!(
                "unsupported opening terrain selector {selector}"
            )));
        }
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.active {
                continue;
            }
            slot.lateral = ((random.next_word() & 0x7ff) as i16 - 1024)
                .wrapping_sub(player_lateral.wrapping_mul(2));
            slot.depth = forward_speed
                .wrapping_mul(32 + index as i16 * 4)
                .wrapping_add(20);
            slot.movement = 2;
            match selector {
                0 => {
                    self.kinds[index] = TerrainKind::Type0;
                    slot.active = false;
                }
                1 => {
                    self.kinds[index] = TerrainKind::Type0;
                    slot.active = true;
                }
                2 => {
                    self.kinds[index] = TerrainKind::Type1;
                    slot.active = true;
                }
                3 | 4 => {
                    let mask = if selector == 3 { 3 } else { 7 };
                    self.kinds[index] = if random.next_word() & mask == 0 {
                        TerrainKind::Type1
                    } else {
                        TerrainKind::Type0
                    };
                    slot.active = true;
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
                8 if index <= 4 => {
                    self.kinds[index] = TerrainKind::Type3;
                    slot.active = true;
                }
                8 => slot.active = false,
                9 => {
                    self.kinds[index] = if index <= 4 {
                        TerrainKind::Type2
                    } else {
                        TerrainKind::Type3
                    };
                    slot.active = true;
                }
                _ => unreachable!(),
            }
            slot.requested_sprite = sprite_choice(slot.depth, slot.phase, self.kinds[index]);
        }
        Ok(())
    }

    pub(crate) fn snapshots(&self) -> [TerrainSnapshot; 8] {
        std::array::from_fn(|index| {
            let mut slot = self.slots[index];
            slot.kind = match self.kinds[index] {
                TerrainKind::Type0 => 0,
                TerrainKind::Type1 => 1,
                TerrainKind::Type2 => 2,
                TerrainKind::Type3 => 3,
            };
            slot
        })
    }

    /// A shot converts solid terrain and awards both conversion and hit points.
    /// It neither refuels the player nor retires the projectile.
    #[cfg(test)]
    pub(crate) fn shot_collision(&mut self, slot: usize, forward_speed: i16) -> Result<u16, Error> {
        self.shot_collision_with_sound(slot, forward_speed, &mut |_| {})
    }

    pub(crate) fn shot_collision_with_sound(
        &mut self,
        slot: usize,
        forward_speed: i16,
        sound: &mut Foreground<'_>,
    ) -> Result<u16, Error> {
        let Some(index) = slot.checked_sub(15).filter(|index| *index < 8) else {
            return Err(Error::InvalidData(format!("invalid terrain slot {slot}")));
        };
        if !matches!(self.kinds[index], TerrainKind::Type0 | TerrainKind::Type1) {
            return Ok(0);
        }
        sound::request(sound, 0x397c, Effect::Ramp(Ramp::Third));
        start_effect_at_speed(&mut self.slots[index], forward_speed);
        Ok(900)
    }

    /// The new-game initializer retains resources, geometry, kinds and extents.
    pub(crate) fn reset_for_game(&mut self) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slot.active = false;
            slot.movement = 0;
            slot.depth = 1000 + index as i16 * 100;
        }
    }

    pub(crate) fn restore_snapshots(
        &mut self,
        snapshots: [TerrainSnapshot; 8],
    ) -> Result<(), Error> {
        let mut kinds = [TerrainKind::Type0; 8];
        for (kind, snapshot) in kinds.iter_mut().zip(snapshots) {
            *kind = match snapshot.kind {
                0 => TerrainKind::Type0,
                1 => TerrainKind::Type1,
                2 => TerrainKind::Type2,
                3 => TerrainKind::Type3,
                _ => return Err(Error::InvalidData("invalid retained terrain kind".into())),
            };
        }
        self.slots = snapshots;
        self.kinds = kinds;
        self.reset_for_game();
        Ok(())
    }

    /// Far shot effects can index beyond the nominal explosion sequence into
    /// subsequent program data. Preserve that original lookup from external DS.
    pub(crate) fn set_data_segment(&mut self, data: &[u8]) {
        self.data_segment = Some(data.to_vec());
    }

    /// Command 46 places eight obstacles immediately before its goal panels.
    /// Requested and resolved resources remain until the ordinary update pass.
    pub(crate) fn spawn_goal_barrier(&mut self, forward_speed: i16, difficulty: u16) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slot.depth = if difficulty == 0 {
                944_i16.wrapping_sub(forward_speed.wrapping_mul(4))
            } else {
                944
            };
            slot.active = true;
            slot.lateral = -600 + 75 * index as i16;
            slot.movement = 2;
            self.kinds[index] = TerrainKind::Type0;
        }
    }
}

#[cfg(test)]
fn proximity_effect(slot: &TerrainSnapshot, kind: TerrainKind, player: [i16; 3]) -> bool {
    proximity_effect_at_speed(slot, kind, player, 28)
}

fn proximity_effect_at_speed(
    slot: &TerrainSnapshot,
    kind: TerrainKind,
    player: [i16; 3],
    forward_speed: i16,
) -> bool {
    if slot.depth >= player[0].wrapping_add(forward_speed.wrapping_mul(2)) || player[2] >= 60 {
        return false;
    }
    match kind {
        TerrainKind::Type0 => {
            slot.lateral > player[1].wrapping_sub(100) && slot.lateral < player[1].wrapping_add(200)
        }
        TerrainKind::Type1 => {
            player[2] > -80
                && slot.lateral > player[1].wrapping_sub(60)
                && slot.lateral < player[1].wrapping_add(100)
        }
        _ => false,
    }
}

fn start_effect_at_speed(slot: &mut TerrainSnapshot, forward_speed: i16) {
    slot.movement = if slot.depth < forward_speed.wrapping_mul(2) {
        252
    } else {
        3
    };
    slot.animation_phase = 0;
}

fn effect_sprite_from_data(
    slot: &TerrainSnapshot,
    kind: TerrainKind,
    data_segment: Option<&[u8]>,
) -> usize {
    let frame = usize::from(slot.animation_phase / 2);
    if slot.movement == 252 {
        let first = if matches!(kind, TerrainKind::Type1) {
            118
        } else {
            117
        };
        if frame < 124 - first {
            first + frame
        } else {
            0
        }
    } else {
        if let Some(data) = data_segment {
            let offset = 0x0562_u16
                .wrapping_add(((slot.depth >> 5) as u16).wrapping_mul(10))
                .wrapping_add(frame as u16);
            return usize::from(data.get(usize::from(offset)).copied().unwrap_or(0));
        }
        let row = slot.depth >> 5;
        let sequence: &[usize] = match row {
            0..=2 => &[69, 61, 69, 67, 65, 64],
            3..=4 => &[69, 68, 67, 66, 65, 64],
            5..=7 => &[68, 67, 66, 65, 64],
            8..=11 => &[67, 66, 65, 64],
            12..=15 => &[66, 65, 63, 64],
            _ => unreachable!("far terrain effect depth outside supported range"),
        };
        sequence.get(frame).copied().unwrap_or(0)
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
        TerrainKind::Type1 => match size {
            0..=1 => {
                if shifted {
                    113
                } else {
                    105
                }
            }
            2..=3 => {
                if shifted {
                    114
                } else {
                    106
                }
            }
            4..=5 => {
                if shifted {
                    115
                } else {
                    107
                }
            }
            6..=8 => {
                if shifted {
                    116
                } else {
                    108
                }
            }
            9..=14 => 109,
            15..=19 => 110,
            20..=24 => 111,
            _ => 112,
        },
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
        TerrainKind::Type0 => match size {
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

    #[test]
    fn additional_selectors_keep_their_random_consumption_and_excluded_kind() {
        let mut terrain = Terrain::new();
        let mut random = Random::default();
        terrain.recycle_at_speed(1, &mut random, 17, 40).unwrap();
        let state = terrain.snapshots();
        assert!(state.iter().all(|slot| slot.active && slot.kind == 0));
        assert_eq!(
            state.map(|slot| slot.depth),
            [1300, 1460, 1620, 1780, 1940, 2100, 2260, 2420]
        );
        terrain.reset_for_game();
        terrain.recycle_at_speed(8, &mut random, -17, 28).unwrap();
        let state = terrain.snapshots();
        assert!(state[..5].iter().all(|slot| slot.active && slot.kind == 3));
        assert!(state[5..].iter().all(|slot| !slot.active && slot.kind == 0));
        let mut expected_random = Random::default();
        for _ in 0..16 {
            expected_random.next_word();
        }
        assert_eq!(random.state(), expected_random.state());
        terrain.reset_for_game();
        terrain.recycle_at_speed(4, &mut random, 0, 28).unwrap();
        for _ in 0..16 {
            expected_random.next_word();
        }
        assert_eq!(random.state(), expected_random.state());
        assert!(
            terrain
                .snapshots()
                .iter()
                .all(|slot| slot.active && slot.kind <= 1)
        );
    }

    #[test]
    fn far_shot_effect_reads_following_external_data_and_retains_it_on_reset() {
        let mut terrain = Terrain::new();
        let mut data = vec![0; 0x10000];
        data[0x562 + 20 * 10] = 86;
        data[0x562 + 20 * 10 + 1] = 87;
        terrain.set_data_segment(&data);
        terrain.spawn_goal_barrier(28, 1);
        terrain.slots[0].depth = 640;
        terrain.shot_collision(15, 28).unwrap();
        terrain.update_half_step_at_speed([14, 1000, 1000], 28);
        assert_eq!(terrain.slots[0].requested_sprite, 86);
        assert_eq!(terrain.slots[0].depth, 640);
        terrain.update_half_step_at_speed([14, 1000, 1000], 28);
        terrain.update_half_step_at_speed([14, 1000, 1000], 28);
        assert_eq!(terrain.slots[0].requested_sprite, 87);
        terrain.restore_snapshots(terrain.snapshots()).unwrap();
        assert_eq!(terrain.data_segment.unwrap()[0x562 + 20 * 10], 86);
    }

    #[test]
    fn speed_changes_conversion_and_goal_obstacle_spacing_without_resolving_sources() {
        let mut terrain = Terrain::new();
        terrain.spawn_goal_barrier(40, 0);
        assert!(
            terrain
                .slots
                .iter()
                .all(|slot| slot.depth == 784 && slot.active)
        );
        assert_eq!(
            terrain.slots.map(|slot| slot.lateral),
            [-600, -525, -450, -375, -300, -225, -150, -75]
        );
        terrain.slots[0].depth = 79;
        terrain.slots[0].requested_sprite = 54;
        assert_eq!(terrain.shot_collision(15, 40).unwrap(), 900);
        assert_eq!(terrain.slots[0].movement, 252);
        assert_eq!(terrain.slots[0].requested_sprite, 54);
        terrain.slots[1].depth = 80;
        assert_eq!(terrain.shot_collision(16, 40).unwrap(), 900);
        assert_eq!(terrain.slots[1].movement, 3);
        terrain.spawn_goal_barrier(56, 1);
        assert!(terrain.slots.iter().all(|slot| slot.depth == 944));
        terrain.reset_for_game();
        assert_eq!(terrain.slots[0].requested_sprite, 54);
        assert_eq!(terrain.slots[0].lateral, -600);
        assert!(!terrain.slots[0].active);
        assert_eq!(terrain.slots[0].movement, 0);
    }
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
        slot.movement = 2;
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
        slot.movement = 2;
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
            slot.movement = 2;
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
        assert!(terrain.recycle(10, &mut random).is_err());
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

    #[test]
    fn selector_three_interleaves_lateral_and_kind_random_words() {
        let mut terrain = Terrain::new();
        let mut random = Random::default();
        let mut reference = Random::default();
        let expected: Vec<_> = (0..8)
            .map(|_| {
                let lateral = (reference.next_word() & 2047) as i16 - 1024;
                let kind = u8::from(reference.next_word() & 3 == 0);
                (lateral, kind)
            })
            .collect();
        terrain.recycle(3, &mut random).unwrap();
        for (slot, (lateral, kind)) in terrain.snapshots().into_iter().zip(expected) {
            assert_eq!(slot.lateral, lateral);
            assert_eq!(slot.kind, kind);
            assert_eq!(slot.movement, 2);
            assert!(slot.active);
        }
        assert_eq!(random.state(), reference.state());
    }

    #[test]
    fn selector_two_allocates_fuel_rows_without_kind_randomness_and_preserves_live_rows() {
        let mut terrain = Terrain::new();
        let mut random = Random::default();
        let mut reference = Random::default();
        terrain.recycle(2, &mut random).unwrap();
        for slot in terrain.snapshots() {
            let expected_lateral = (reference.next_word() & 2047) as i16 - 1024;
            assert_eq!(slot.lateral, expected_lateral);
            assert_eq!(slot.kind, 1);
            assert_eq!(slot.movement, 2);
            assert!(slot.active);
            assert_eq!(
                slot.requested_sprite,
                if slot.slot == 15 { 106 } else { 105 }
            );
            assert_eq!(slot.resolved_sprite, 0);
        }
        assert_eq!(random.state(), reference.state());

        terrain.kinds[0] = TerrainKind::Type2;
        let live_lateral = terrain.slots[0].lateral;
        terrain.slots[3].active = false;
        terrain.slots[3].phase = 3;
        terrain.slots[3].resolved_sprite = 156;
        terrain.slots[3].animation_phase = 9;
        terrain.slots[3].rect.y = 140;
        let expected_lateral = (reference.next_word() & 2047) as i16 - 1024;
        terrain.recycle(2, &mut random).unwrap();
        assert_eq!(random.state(), reference.state());
        let snapshots = terrain.snapshots();
        assert_eq!(snapshots[0].kind, 2);
        assert_eq!(snapshots[0].lateral, live_lateral);
        assert_eq!(snapshots[3].kind, 1);
        assert_eq!(snapshots[3].depth, 1252);
        assert_eq!(snapshots[3].lateral, expected_lateral);
        assert_eq!(snapshots[3].requested_sprite, 113);
        assert_eq!(snapshots[3].resolved_sprite, 156);
        assert_eq!(snapshots[3].animation_phase, 9);
        assert_eq!(snapshots[3].rect.y, 140);
    }

    #[test]
    fn selector_two_fuel_collection_uses_existing_near_effect_without_repeat_collection() {
        let mut terrain = Terrain::new();
        terrain.recycle(2, &mut Random::default()).unwrap();
        terrain.slots[0].depth = 48;
        terrain.slots[0].lateral = 0;
        assert_eq!(terrain.update_half_step([14, 0, -28]), 512);
        assert_eq!(terrain.slots[0].movement, 252);
        assert_eq!(terrain.slots[0].depth, 20);
        assert_eq!(terrain.slots[0].requested_sprite, 110);
        assert_eq!(terrain.update_half_step([14, 0, -28]), 0);
        assert_eq!(terrain.slots[0].requested_sprite, 118);
        assert_eq!(terrain.slots[0].depth, 20);
        assert_eq!(terrain.slots[0].animation_phase, 1);
    }

    #[test]
    fn proximity_conversion_applies_motion_once_then_freezes_and_ends_animation() {
        let mut terrain = Terrain::new();
        terrain.kinds[0] = TerrainKind::Type1;
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.movement = 2;
        slot.depth = 48;
        assert_eq!(terrain.update_half_step([14, 0, -28]), 512);
        assert_eq!(terrain.slots[0].depth, 20);
        assert_eq!(terrain.slots[0].movement, 252);
        for expected in [118, 118, 119, 119, 120, 120, 121, 121, 122, 122, 123, 123] {
            assert_eq!(terrain.update_half_step([14, 0, -28]), 0);
            assert_eq!(terrain.slots[0].requested_sprite, expected);
            assert_eq!(terrain.slots[0].depth, 20);
            assert!(terrain.slots[0].active);
        }
        terrain.update_half_step([14, 0, -28]);
        assert!(!terrain.slots[0].active);
        assert_eq!(terrain.slots[0].requested_sprite, 123);
        terrain.slots[0].active = true;
        terrain.slots[0].movement = 2;
        terrain.slots[0].depth = 42;
        terrain.update_half_step([14, 0, -28]);
        assert!(!terrain.slots[0].active);
        assert_eq!(terrain.slots[0].requested_sprite, 0);
    }

    #[test]
    fn proximity_bounds_are_strict_and_far_effect_uses_post_motion_depth_row() {
        let mut terrain = Terrain::new();
        terrain.kinds[0] = TerrainKind::Type1;
        let slot = &mut terrain.slots[0];
        slot.active = true;
        slot.movement = 2;
        slot.depth = 70;
        for (depth, lateral, height, expected) in [
            (70, 0, -28, false),
            (69, -60, -28, false),
            (69, 100, -28, false),
            (69, 0, -80, false),
            (69, 0, 60, false),
            (69, -59, -79, true),
        ] {
            slot.depth = depth;
            slot.lateral = lateral;
            assert_eq!(
                proximity_effect(slot, TerrainKind::Type1, [14, 0, height]),
                expected
            );
        }
        slot.depth = 64;
        slot.lateral = 0;
        assert_eq!(terrain.update_half_step([14, 0, -28]), 512);
        assert_eq!(terrain.slots[0].movement, 3);
        assert_eq!(terrain.slots[0].depth, 36);
        terrain.update_half_step([14, 0, -28]);
        assert_eq!(terrain.slots[0].requested_sprite, 69);
        terrain.update_half_step([14, 0, -28]);
        terrain.update_half_step([14, 0, -28]);
        assert_eq!(terrain.slots[0].requested_sprite, 61);
        assert_eq!(terrain.slots[0].depth, 36);
    }

    #[test]
    fn collision_grace_retains_collection_and_conversion_but_damage_stops_slot_iteration() {
        let mut assets = assets();
        assets.sprites[0].descriptor[2] = 30;
        let setup = || {
            let mut terrain = Terrain::new();
            for index in 0..2 {
                terrain.kinds[index] = TerrainKind::Type1;
                terrain.slots[index].active = true;
                terrain.slots[index].movement = 2;
                terrain.slots[index].depth = 40;
            }
            terrain
        };
        let mut immune = setup();
        let effect = immune
            .player_collision(&assets, [14, 0, -100], 0, true, false)
            .unwrap()
            .unwrap();
        assert_eq!((effect.fuel, effect.points), (1024, 800));
        assert_eq!(effect.awards, [400, 400, 0, 0, 0, 0, 0, 0]);
        assert!(immune.slots[..2].iter().all(|slot| slot.movement == 252));
        let mut vulnerable = setup();
        let effect = vulnerable
            .player_collision(&assets, [14, 0, -100], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!((effect.fuel, effect.points), (512, 400));
        assert_eq!(vulnerable.slots[0].movement, 252);
        assert_eq!(vulnerable.slots[1].movement, 2);
        let mut phased = setup();
        assert!(
            phased
                .player_collision(&assets, [14, 0, -100], 0, false, true)
                .unwrap()
                .is_none()
        );
        assert!(phased.slots[..2].iter().all(|slot| slot.movement == 2));
    }

    #[test]
    fn near_effect_collides_for_one_damage_without_restart_or_collection() {
        let mut assets = assets();
        assets.sprites[0].descriptor[2] = 30;
        assets.sprites[117].descriptor[0] = 80;
        let mut terrain = Terrain::new();
        terrain.slots[0].active = true;
        terrain.slots[0].movement = 252;
        terrain.slots[0].depth = 20;
        terrain.slots[0].animation_phase = 3;
        terrain.slots[0].resolved_sprite = 117;
        let effect = terrain
            .player_collision(&assets, [14, 0, -28], 0, true, true)
            .unwrap()
            .unwrap();
        assert_eq!((effect.damage, effect.fuel, effect.points), (1, 0, 0));
        assert_eq!(terrain.slots[0].movement, 252);
        assert_eq!(terrain.slots[0].animation_phase, 3);
        terrain.slots[0].movement = 3;
        assert!(
            terrain
                .player_collision(&assets, [14, 0, -28], 0, true, true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn selector_nine_splits_recycled_rows_without_excluding_or_retyping_live_rows() {
        let mut terrain = Terrain::new();
        let mut random = Random::default();
        let mut reference = Random::default();
        terrain.recycle(9, &mut random).unwrap();
        for slot in terrain.snapshots() {
            reference.next_word();
            assert!(slot.active);
            assert_eq!(slot.kind, if slot.slot <= 19 { 2 } else { 3 });
        }
        assert_eq!(random.state(), reference.state());
        terrain.kinds[0] = TerrainKind::Type3;
        let live_lateral = terrain.slots[0].lateral;
        for index in [4, 5] {
            terrain.slots[index].active = false;
            terrain.slots[index].phase = 3;
            terrain.slots[index].resolved_sprite = 157;
            terrain.slots[index].animation_phase = 9;
        }
        terrain.recycle(9, &mut random).unwrap();
        let expected_laterals =
            [reference.next_word(), reference.next_word()].map(|word| (word & 2047) as i16 - 1024);
        assert_eq!(random.state(), reference.state());
        let snapshots = terrain.snapshots();
        assert_eq!(snapshots[0].kind, 3);
        assert_eq!(snapshots[0].lateral, live_lateral);
        for (index, kind, sprite, lateral) in [
            (4, 2, 149, expected_laterals[0]),
            (5, 3, 158, expected_laterals[1]),
        ] {
            let slot = snapshots[index];
            assert!(slot.active);
            assert_eq!(slot.kind, kind);
            assert_eq!(slot.movement, 2);
            assert_eq!(slot.lateral, lateral);
            assert_eq!(slot.requested_sprite, sprite);
            assert_eq!(slot.resolved_sprite, 157);
            assert_eq!(slot.phase, 3);
            assert_eq!(slot.animation_phase, 9);
        }
        terrain.project(&assets(), true, 106, 0).unwrap();
        assert_eq!(terrain.slots[4].resolved_sprite, 149);
        assert_eq!(terrain.slots[5].resolved_sprite, 158);
        assert_eq!(terrain.slots[4].phase, 3);
        assert_eq!(terrain.slots[5].phase, 3);
        terrain.recycle(9, &mut random).unwrap();
        assert_eq!(random.state(), reference.state());
    }
}
