// Copyright (c) 2026 analogued. Project license has not been selected.
//! Opening wave movement, ground shadows, seeking projectiles and explosions.

use super::Random;
use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

#[derive(Clone, Copy, Debug)]
pub struct EnemySnapshot {
    pub slot: usize,
    pub depth: i16,
    pub lateral: i16,
    pub height: i16,
    pub active: bool,
    pub movement: u8,
    pub variant: u8,
    pub animation_phase: u8,
    pub requested_sprite: usize,
    pub resolved_sprite: usize,
    pub rect: Rect,
    pub phase: u8,
}

impl EnemySnapshot {
    fn new(slot: usize, sprite: usize, depth: i16, height: i16) -> Self {
        Self {
            slot,
            depth,
            lateral: 0,
            height,
            active: false,
            movement: if slot == 4 { 253 } else { 0 },
            variant: 0,
            animation_phase: 0,
            requested_sprite: sprite,
            resolved_sprite: sprite,
            rect: Rect {
                x_bytes: 128,
                y: 0,
                width_bytes: 2,
                height: 2,
            },
            phase: 0,
        }
    }

    fn project(
        &mut self,
        assets: &GraphicsAssets,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        let sprite = assets.sprites.get(self.requested_sprite).ok_or_else(|| {
            Error::InvalidData(format!(
                "missing opening wave sprite {}",
                self.requested_sprite
            ))
        })?;
        self.resolved_sprite = self.requested_sprite;
        self.rect.width_bytes = sprite.width_bytes;
        self.rect.height = sprite.height;
        if !self.active || self.depth >= 1000 {
            self.rect.x_bytes = 128;
            return Ok(());
        }
        // The original perspective table wraps its index to ten bits and
        // multiplies the resulting word as signed, including index zero.
        let scale = i32::from(((64 * 32768) / (i32::from(self.depth & 1023) + 64)) as i16);
        let horizontal = (scale * i32::from(self.lateral)) >> 16;
        self.phase = (horizontal & 3) as u8;
        self.rect.x_bytes = 40 + (horizontal >> 2) - i32::from(sprite.descriptor[5]);
        let vertical = (scale * i32::from(self.height.wrapping_sub(camera_height))) >> 16;
        self.rect.y = vertical_origin - vertical - i32::from(sprite.descriptor[6]);
        Ok(())
    }

    fn draw(self) -> Draw {
        Draw {
            resolved_sprite: self.resolved_sprite,
            rect: self.rect,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnemyCollision {
    Projectile,
    Craft(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Fixed,
    Seeking,
    Orbit,
    LateralOrbit,
    FullOrbit,
    ScriptedSweep,
    RelativeSweep,
}

#[derive(Debug)]
pub(crate) struct Enemy {
    bullet: EnemySnapshot,
    crafts: [EnemySnapshot; 5],
    shadows: [EnemySnapshot; 5],
    target: [i16; 2],
    motions: [Motion; 5],
    orbit_center: [i16; 2],
    craft_animations: [Option<u8>; 5],
}

impl Enemy {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut state = Self {
            bullet: EnemySnapshot::new(4, 22, 14, -100),
            crafts: std::array::from_fn(|i| EnemySnapshot::new(5 + i, 0, 0, 0)),
            shadows: std::array::from_fn(|i| EnemySnapshot::new(10 + i, 0, 0, 0)),
            target: [0, 0],
            motions: [Motion::Fixed; 5],
            orbit_center: [0, -100],
            craft_animations: [None; 5],
        };
        state.project(assets, true, 106, 0)?;
        Ok(state)
    }

    /// Initialize the first count primary slots after rendering, retaining each
    /// resolved source and projection. Original allocation stops at slot nine.
    pub(crate) fn spawn_command(
        &mut self,
        command: u8,
        random: &mut Random,
        player_lateral: i16,
        count: usize,
    ) -> Result<(), Error> {
        let motion = match command {
            0x91 => Motion::Fixed,
            0x84 => Motion::Seeking,
            0x8e => Motion::Orbit,
            0x8a => Motion::LateralOrbit,
            0x88 => Motion::FullOrbit,
            0x97 => Motion::ScriptedSweep,
            0x96 => Motion::RelativeSweep,
            _ => {
                return Err(Error::InvalidData(format!(
                    "unsupported enemy command {command:02x}"
                )));
            }
        };
        for i in 0..count.min(5) {
            let craft = &mut self.crafts[i];
            if command == 0x91 {
                craft.lateral = ((random.next_word() & 1023) as i16 - 512)
                    .wrapping_sub(player_lateral.wrapping_mul(2));
                craft.height = (random.next_word() & 255) as i16 - 50;
            } else {
                craft.lateral = 0;
                craft.height = 0;
            }
            self.motions[i] = motion;
            self.craft_animations[i] = None;
            craft.requested_sprite = 0;
            craft.movement = command & 63;
            craft.variant = u8::from(matches!(command, 0x96 | 0x97));
            craft.animation_phase = if command == 0x96 { 4 * i as u8 } else { 0 };
            craft.depth = if matches!(command, 0x96 | 0x97) {
                1000
            } else {
                1600 - 100 * i as i16
            };
            craft.active = true;
            self.shadows[i].height = -115;
            self.shadows[i].active = true;
            if command == 0x97 && i != 0 {
                craft.active = false;
                self.shadows[i].active = false;
            }
        }
        Ok(())
    }

    /// Slot order matters: the projectile precedes all craft. Each active craft
    /// can fire into that shared slot and replaces the shared target afterwards.
    pub(crate) fn update_half_step(&mut self, random: &mut Random, player: [i16; 3]) {
        if self.bullet.active {
            self.bullet.requested_sprite = match size(self.bullet.depth) {
                0..=3 => 64,
                4..=6 => 63,
                7..=9 => 65,
                10..=16 => 66,
                17..=20 => 67,
                21..=25 => 68,
                _ => 69,
            };
            self.bullet.lateral += (self.target[0] - self.bullet.lateral).clamp(-80, 80);
            self.bullet.height += (self.target[1] - self.bullet.height).clamp(-80, 80);
            self.bullet.depth -= 28;
            if self.bullet.depth <= player[0] {
                self.bullet.requested_sprite = 0;
                self.bullet.active = false;
            }
        }
        for i in 0..5 {
            self.update_craft(i, random, player);
        }
        // All shadow handlers run after all craft handlers.
        for i in 0..5 {
            if !self.crafts[i].active {
                self.shadows[i].active = false;
            }
        }
    }

    fn update_craft(&mut self, i: usize, random: &mut Random, player: [i16; 3]) {
        if !self.crafts[i].active {
            return;
        }
        if let Some(animation) = self.craft_animations[i] {
            let source = explosion_sprite(self.crafts[i].depth, animation);
            if source == 0 {
                self.crafts[i].active = false;
            } else {
                self.crafts[i].requested_sprite = source;
                self.craft_animations[i] = Some(animation + 1);
                self.crafts[i].animation_phase = animation + 1;
            }
            return;
        }
        let steering = match self.motions[i] {
            Motion::Orbit if self.crafts[i].depth >= 240 => self.orbit_delta(i, player, true),
            Motion::FullOrbit => self.orbit_delta(i, player, true),
            Motion::LateralOrbit => self.orbit_delta(i, player, false),
            Motion::Seeking | Motion::Orbit => [
                (player[1].wrapping_sub(self.crafts[i].lateral) >> 1).clamp(-30, 30),
                (player[2].wrapping_sub(self.crafts[i].height) >> 1).clamp(-30, 30),
            ],
            Motion::ScriptedSweep | Motion::RelativeSweep => {
                let relative = self.motions[i] == Motion::RelativeSweep;
                if relative {
                    self.follow_center(player, 4);
                }
                let craft = &mut self.crafts[i];
                let (coordinates, source) = scripted_sweep(craft.animation_phase);
                [craft.depth, craft.lateral, craft.height] = coordinates;
                if relative {
                    let spacing = (2 * i as i16 - 5) << 5;
                    craft.lateral = craft
                        .lateral
                        .wrapping_add(self.orbit_center[0])
                        .wrapping_add(spacing);
                    craft.height = craft.height.wrapping_add(self.orbit_center[1]);
                }
                craft.requested_sprite = source;
                craft.animation_phase = craft.animation_phase.wrapping_add(1);
                [0, 0]
            }
            Motion::Fixed => [0, 0],
        };
        let craft = &mut self.crafts[i];
        let shadow = &mut self.shadows[i];
        if craft.variant == 0 {
            craft.requested_sprite = craft_sprite(craft.depth, craft.phase);
        }
        shadow.depth = craft.depth;
        shadow.lateral = craft.lateral;
        shadow.requested_sprite = if craft.depth < 0 {
            // The supported sweep's terminal -50 row reads a startup-zeroed
            // size-table gap, selecting source 36 before forward retirement.
            36
        } else {
            shadow_sprite(craft.depth)
        };
        if craft.variant == 1 {
            // The opening's alternate-shadow mode remains disabled.
            shadow.active = true;
        }
        if random.next_word() & 1 != 0 && !self.bullet.active {
            self.bullet.depth = craft.depth;
            self.bullet.lateral = craft.lateral;
            self.bullet.height = craft.height;
            self.bullet.active = true;
        }
        self.target[0] = player[1].wrapping_add((random.next_word() & 63) as i16 - 32);
        self.target[1] = player[2].wrapping_add((random.next_word() & 63) as i16 - 32);
        if craft.variant == 0 {
            craft.depth -= 22;
        }
        craft.lateral += steering[0];
        craft.height += steering[1];
        if craft.depth <= player[0] {
            craft.active = false;
            craft.animation_phase = 0;
            shadow.active = false;
        }
    }

    fn follow_center(&mut self, player: [i16; 3], step: i16) {
        for (center, coordinate) in self.orbit_center.iter_mut().zip(player[1..].iter()) {
            *center = center.wrapping_add(coordinate.wrapping_sub(*center).clamp(-step, step));
        }
    }

    fn orbit_delta(&mut self, i: usize, player: [i16; 3], vertical_motion: bool) -> [i16; 2] {
        self.follow_center(player, 9);
        let craft = &mut self.crafts[i];
        let phase = ((craft.depth & 1023) >> 4) as u8;
        // Lateral changes before companion copying and firing. Height is a delta.
        craft.lateral = self.orbit_center[0].wrapping_add(orbit_wave(phase) << 4);
        let height_delta = if vertical_motion {
            self.orbit_center[1]
                .wrapping_add(orbit_wave(phase.wrapping_add(16)) << 4)
                .clamp(-100, 250)
                .wrapping_sub(craft.height)
        } else {
            0
        };
        [0, height_delta]
    }

    /// Convert the identified primary to an explosion and hide its companion.
    pub(crate) fn explode_craft(&mut self, slot: usize) -> Result<(), Error> {
        let i = slot
            .checked_sub(5)
            .filter(|i| *i < 5)
            .ok_or_else(|| Error::InvalidData(format!("invalid primary explosion slot {slot}")))?;
        self.craft_animations[i] = Some(0);
        self.crafts[i].movement = 3;
        self.crafts[i].animation_phase = 0;
        self.shadows[i].active = false;
        Ok(())
    }

    /// Collision uses preceding resolved descriptors. Phase gates candidates;
    /// damage grace is checked by the caller without suppressing target effects.
    pub(crate) fn player_collisions(
        &self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        collidable: bool,
    ) -> Result<[Option<EnemyCollision>; 6], Error> {
        let mut collisions = [None; 6];
        if !collidable {
            return Ok(collisions);
        }
        let player_source = assets.sprites.get(player_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing player collision sprite {player_sprite}"))
        })?;
        for (index, slot) in std::iter::once(&self.bullet)
            .chain(self.crafts.iter())
            .enumerate()
        {
            if !slot.active || (index > 0 && self.craft_animations[index - 1].is_some()) {
                continue;
            }
            let source = assets.sprites.get(slot.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!(
                    "missing enemy collision sprite {}",
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
            ) && player[2].wrapping_add(source.descriptor[0]) >= slot.height
                && player[2] <= slot.height.wrapping_add(source.descriptor[0])
            {
                collisions[index] = Some(if index == 0 {
                    EnemyCollision::Projectile
                } else {
                    EnemyCollision::Craft(slot.slot)
                });
            }
        }
        Ok(collisions)
    }

    /// Near projection includes inactive slots with depth strictly below 70.
    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        for slot in std::iter::once(&mut self.bullet)
            .chain(self.crafts.iter_mut())
            .chain(self.shadows.iter_mut())
        {
            if full_projection || slot.depth < 70 {
                slot.project(assets, vertical_origin, camera_height)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        random: &mut Random,
        full_projection: bool,
    ) -> Result<(), Error> {
        self.update_half_step(random, [14, 0, -100]);
        self.project(assets, full_projection, 106, 0)
    }

    /// Subtype-zero shadows sort ahead of all ordinary world objects.
    pub(crate) fn shadow_draws(&self) -> impl Iterator<Item = Draw> + '_ {
        self.shadows
            .iter()
            .filter(|shadow| shadow.active)
            .map(|shadow| shadow.draw())
    }

    pub(crate) fn snapshots(&self) -> [EnemySnapshot; 11] {
        std::array::from_fn(|i| match i {
            0 => self.bullet,
            1..=5 => self.crafts[i - 1],
            _ => self.shadows[i - 6],
        })
    }

    pub(crate) fn target(&self) -> [i16; 2] {
        self.target
    }

    pub(crate) fn active(&self) -> bool {
        self.crafts.iter().any(|craft| craft.active)
    }
}

/// Resource-defined sweep reconstructed as affine segments. Division truncates
/// toward zero, preserving the original quantized coordinates without floats.
fn scripted_sweep(phase: u8) -> ([i16; 3], usize) {
    if phase >= 77 {
        return ([-50, 0, 0], 100);
    }
    let (start, axes): (u8, [[i32; 3]; 3]) = match phase {
        0..=11 => (0, [[500, 0, 1], [-2600, 50, 1], [16, 0, 1]]),
        12..=25 => (12, [[500, -2, 1], [-2000, 48, 1], [16, 2, 1]]),
        26..=27 => (26, [[472, -2, 1], [-1328, 55, 1], [44, 2, 1]]),
        28..=32 => (28, [[470, -2, 1], [-2436, 71, 2], [48, 29, 1]]),
        33..=40 => (33, [[460, -1, 1], [-2080, 53, 2], [388, 73, 2]]),
        41..=49 => (41, [[1812, -81, 4], [-5796, 333, 7], [972, -17, 2]]),
        50..=54 => (50, [[1084, -45, 4], [-400, 40, 1], [1230, -38, 3]]),
        55..=58 => (55, [[215, -11, 1], [-200, 10, 1], [1040, -38, 3]]),
        59..=63 => (59, [[513, -23, 3], [-160, 10, 1], [888, -44, 3]]),
        64..=65 => (64, [[133, -8, 1], [-106, 10, 1], [223, -15, 1]]),
        66..=68 => (66, [[236, -11, 2], [-84, 12, 1], [194, -20, 1]]),
        69..=70 => (69, [[101, -10, 1], [-48, 24, 1], [135, -39, 1]]),
        71..=72 => (71, [[82, -17, 1], [-18, 12, 1], [72, -48, 1]]),
        _ => (73, [[47, -10, 1], [0, 0, 1], [0, 0, 1]]),
    };
    let step = i32::from(phase - start);
    let coordinates = axes.map(|[base, delta, divisor]| ((base + delta * step) / divisor) as i16);
    let source = match phase {
        0..=28 => 90,
        29..=33 => 91,
        34..=41 => 92,
        42..=55 => 93,
        56..=61 => 94,
        62..=66 => 95,
        67..=69 => 96,
        70 => 97,
        71 => 98,
        72..=73 => 99,
        _ => usize::from(phase) + 26,
    };
    (coordinates, source)
}

/// Signed integer wave with 64 phases; mirror the quarter-wave's small stepped
/// amplitudes rather than introducing floating-point rounding into movement.
fn orbit_wave(phase: u8) -> i16 {
    let phase = phase & 63;
    let half = phase & 31;
    let quarter = if half <= 16 { half } else { 32 - half };
    let amplitude = match quarter {
        0..=8 => i16::from(quarter),
        9..=10 => 9,
        11..=12 => 10,
        _ => 11,
    };
    if phase < 32 { amplitude } else { -amplitude }
}

fn explosion_sprite(depth: i16, animation: u8) -> usize {
    let sources: &[usize] = match depth >> 5 {
        0..=2 => &[69, 61, 69, 67, 65, 64],
        3..=4 => &[69, 68, 67, 66, 65, 64],
        5..=7 => &[68, 67, 66, 65, 64],
        8..=11 => &[67, 66, 65, 64],
        _ => &[66, 65, 63, 64],
    };
    sources
        .get(usize::from(animation >> 1))
        .copied()
        .unwrap_or(0)
}

fn size(depth: i16) -> i32 {
    2047 / (i32::from(depth) + 64)
}

fn craft_sprite(depth: i16, phase: u8) -> usize {
    let shifted = phase >= 2;
    match size(depth) {
        0..=1 => {
            if shifted {
                34
            } else {
                25
            }
        }
        2..=3 => {
            if shifted {
                35
            } else {
                26
            }
        }
        4..=5 => 27,
        6..=8 => 28,
        9..=11 => 29,
        12..=15 => 30,
        16..=20 => 31,
        21..=25 => 32,
        26..=31 => 33,
        _ => 33,
    }
}

fn shadow_sprite(depth: i16) -> usize {
    match size(depth) {
        0..=1 => 36,
        2..=3 => 37,
        4..=5 => 38,
        6..=8 => 39,
        9..=11 => 40,
        12..=15 => 41,
        16..=20 => 42,
        21..=25 => 43,
        _ => 44,
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
            sprites: (0..190)
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
    fn seeking_precedes_target_update_and_shadow_observes_old_depth() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 800;
        enemy.crafts[0].lateral = 174;
        enemy.crafts[0].height = 91;
        enemy.crafts[0].phase = 3;
        enemy.shadows[0].active = true;
        enemy.bullet.active = true;
        enemy.bullet.depth = 900;
        enemy.bullet.lateral = 174;
        enemy.bullet.height = 91;
        enemy.target = [10, -100];
        let mut random = Random::default();
        enemy
            .advance_half_step(&assets(), &mut random, true)
            .unwrap();
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (872, 94, 11)
        );
        assert_eq!(enemy.crafts[0].depth, 778);
        assert_eq!(enemy.shadows[0].depth, 800);
        assert_eq!(enemy.crafts[0].requested_sprite, 35);
        assert_eq!(enemy.crafts[0].resolved_sprite, 35);
        assert_eq!(random.next_word(), 0x4293);
    }

    #[test]
    fn second_half_firing_retains_the_previous_projected_source() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 800;
        let mut random = Random::default();
        enemy
            .advance_half_step(&assets(), &mut random, true)
            .unwrap();
        assert!(!enemy.bullet.active);
        enemy
            .advance_half_step(&assets(), &mut random, false)
            .unwrap();
        assert!(enemy.bullet.active);
        assert_eq!(enemy.bullet.depth, 778);
        assert_eq!(enemy.bullet.resolved_sprite, 22);
        assert_eq!(enemy.bullet.rect.x_bytes, 128);
    }
    #[test]
    fn retirement_precedes_collision_and_preserves_craft_source() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 42;
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 36;
        enemy.shadows[0].active = true;
        enemy.update_half_step(&mut Random::default(), [14, 0, -100]);
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.requested_sprite, 0);
        assert!(!enemy.crafts[0].active);
        assert!(!enemy.shadows[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, 31);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            [None; 6]
        );
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 0);
        assert_eq!(enemy.crafts[0].resolved_sprite, 31);
        assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
    }

    #[test]
    fn retired_projectile_can_be_refired_in_the_same_slot_pass() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 42;
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 100;
        enemy.crafts[0].lateral = 200;
        enemy.crafts[0].height = 50;
        let mut random = Random::default();
        random.next_word();
        random.next_word();
        random.next_word(); // Next word is odd, so the craft fires.
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(enemy.bullet.active);
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (100, 200, 50)
        );
        assert_eq!(enemy.bullet.requested_sprite, 0);
        assert_eq!(enemy.crafts[0].depth, 78);
    }

    #[test]
    fn near_projection_has_a_strict_depth_gate_and_resolves_inactive_sources() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.requested_sprite = 65;
        enemy.bullet.depth = 70;
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 22);
        enemy.bullet.depth = 69;
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 65);
        assert_eq!(enemy.bullet.rect.x_bytes, 128);
    }

    #[test]
    fn collision_uses_previous_source_and_inclusive_target_height_extent() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        assets.sprites[64].descriptor[..3].copy_from_slice(&[1, 1, 1]);
        assets.sprites[65].descriptor[..3].copy_from_slice(&[100, 100, 100]);
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 29;
        enemy.bullet.lateral = 128;
        enemy.bullet.height = -99;
        enemy.bullet.resolved_sprite = 64;
        enemy.bullet.requested_sprite = 65;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                None,
                None,
                None,
                None,
                None
            ]
        );
        assert!(enemy.bullet.active);
        enemy.bullet.depth = 30;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
        enemy.bullet.depth = 29;
        enemy.bullet.height = -98;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
        enemy.bullet.height = -101;
        enemy.bullet.lateral = -1;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                None,
                None,
                None,
                None,
                None
            ]
        );
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, false)
                .unwrap(),
            [None; 6]
        );
    }

    #[test]
    fn primary_spawn_commands_consume_only_their_required_random_words() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 123, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (1600, 0, 0)
        );
        enemy.spawn_command(0x91, &mut random, 123, 1).unwrap();
        assert_eq!(enemy.crafts[0].lateral, (0x5630 & 1023) - 512 - 246);
        assert_eq!(enemy.crafts[0].height, (0xbe90_u16 & 255) as i16 - 50);
        assert_eq!(random.next_word(), 0x6338);
    }
    #[test]
    fn seeking_craft_fires_and_copies_shadow_before_steering_deltas() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
        enemy.crafts[0].lateral = 71;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (41, -30));
        assert_eq!((enemy.bullet.lateral, enemy.bullet.height), (71, 0));
        assert_eq!(enemy.bullet.depth, 1600);
        assert_eq!(enemy.shadows[0].lateral, 71);
        for _ in 0..7 {
            enemy.update_half_step(&mut random, [14, 0, -100]);
        }
        assert_eq!(enemy.crafts[0].height, -100);
    }

    #[test]
    fn craft_explosion_retains_source_then_animates_without_movement_or_random() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 16;
        enemy.crafts[0].height = -100;
        enemy.crafts[0].requested_sprite = 33;
        enemy.crafts[0].resolved_sprite = 33;
        enemy.shadows[0].active = true;
        enemy.explode_craft(5).unwrap();
        assert_eq!(enemy.crafts[0].requested_sprite, 33);
        assert!(!enemy.shadows[0].active);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            [None; 6]
        );
        let mut random = Random::default();
        for source in [69, 69, 61, 61, 69, 69, 67, 67, 65, 65, 64, 64] {
            enemy.update_half_step(&mut random, [14, 0, -100]);
            assert!(enemy.crafts[0].active);
            assert_eq!(enemy.crafts[0].requested_sprite, source);
            assert_eq!((enemy.crafts[0].depth, enemy.crafts[0].height), (16, -100));
            assert_eq!(random.state(), [0; 3]);
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(!enemy.crafts[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, 64);
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
    }

    #[test]
    fn orbit_changes_lateral_before_firing_but_defers_height_until_after_firing() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 50, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        enemy.crafts[0].depth = 1024;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [9, -91]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (1002, 9, 85)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (1024, 9, 0)
        );
        assert_eq!(enemy.shadows[0].lateral, 9);
        assert!((18..=81).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
    }

    #[test]
    fn orbit_switches_to_seeking_only_below_the_old_depth_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 240;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (218, 185, -75)
        );
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (196, 155, -52)
        );
        assert_eq!(enemy.orbit_center, [9, -91]);
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        assert_eq!(enemy.orbit_center, [9, -91]);
    }

    #[test]
    fn recovered_player_coordinates_drive_seeking_targets_and_retirement() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
        enemy.update_half_step(&mut random, [28, 100, -28]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (30, -14));
        assert!((68..=131).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
        enemy.crafts[0].active = false;
        enemy.bullet.active = true;
        enemy.bullet.depth = 56;
        enemy.update_half_step(&mut random, [28, 100, -28]);
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.depth, 28);
    }
    #[test]
    fn orbit_height_clamps_before_the_world_delta_is_applied() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        enemy.orbit_center[1] = 200;
        enemy.crafts[0].depth = 1024;
        enemy.update_half_step(&mut random, [14, 0, 230]);
        assert_eq!(enemy.orbit_center[1], 209);
        assert_eq!(enemy.crafts[0].height, 250);
        enemy.orbit_center[1] = -100;
        enemy.crafts[0].depth = 512;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(enemy.crafts[0].height, -100);
    }
    #[test]
    fn lateral_orbit_preserves_height_and_does_not_seek_at_near_depth() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        enemy.crafts[0].depth = 128;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [9, -91]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (106, 137, 0)
        );
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [18, -82]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (84, 114, 0)
        );
    }

    #[test]
    fn lateral_orbit_firing_observes_the_new_lateral_and_preserved_height() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.orbit_center = [0, -28];
        enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 38;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 0, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (16, 32, 0)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (38, 32, 0)
        );
        assert_eq!((enemy.shadows[0].depth, enemy.shadows[0].lateral), (38, 32));
        assert!(enemy.crafts[0].active);
    }
    #[test]
    fn collision_candidates_preserve_slot_order_for_grace_gated_damage() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        for slot in [&mut enemy.bullet, &mut enemy.crafts[0]] {
            slot.active = true;
            slot.depth = 29;
            slot.lateral = 128;
            slot.height = -99;
        }
        enemy.bullet.resolved_sprite = 64;
        enemy.crafts[0].resolved_sprite = 33;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                Some(EnemyCollision::Craft(5)),
                None,
                None,
                None,
                None
            ]
        );
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, false)
                .unwrap(),
            [None; 6]
        );
    }
    #[test]
    fn formation_spawn_staggers_depth_and_retains_cached_projection() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.crafts[1].resolved_sprite = 32;
        enemy.crafts[1].rect.y = 71;
        enemy.spawn_command(0x88, &mut random, 100, 2).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1600, 1500, 0, 0, 0]);
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, true, false, false, false]
        );
        assert_eq!(
            enemy.shadows.map(|c| c.active),
            [true, true, false, false, false]
        );
        assert_eq!(enemy.crafts[1].requested_sprite, 0);
        assert_eq!(enemy.crafts[1].movement, 8);
        assert_eq!(enemy.crafts[1].resolved_sprite, 32);
        assert_eq!(enemy.crafts[1].rect.y, 71);
        assert_eq!(
            enemy.snapshots().map(|c| c.slot),
            [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
        );
        enemy.spawn_command(0x88, &mut random, 100, 20).unwrap();
        assert_eq!(
            enemy.crafts.map(|c| c.depth),
            [1600, 1500, 1400, 1300, 1200]
        );
    }

    #[test]
    fn full_orbit_keeps_vertical_motion_below_the_seeking_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.orbit_center = [0, -28];
        enemy.spawn_command(0x88, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 128;
        enemy.update_half_step(&mut random, [14, 0, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (106, 128, 100)
        );
    }

    #[test]
    fn formation_updates_shared_center_and_random_target_in_slot_order() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x88, &mut random, 0, 2).unwrap();
        enemy.crafts[0].depth = 1024;
        enemy.crafts[1].depth = 512;
        enemy.bullet.active = true;
        enemy.bullet.depth = 600;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        for _ in 0..4 {
            expected.next_word();
        }
        let lateral = 50 + (expected.next_word() & 63) as i16 - 32;
        let height = -28 + (expected.next_word() & 63) as i16 - 32;
        assert_eq!(random.state(), expected.state());
        assert_eq!(enemy.target(), [lateral, height]);
        assert_eq!(enemy.orbit_center, [18, -82]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (9, 85));
        assert_eq!(
            (enemy.crafts[1].lateral, enemy.crafts[1].height),
            (18, -100)
        );
        assert_eq!(
            (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
            (9, 18)
        );
    }

    #[test]
    fn secondary_explosion_preserves_other_craft_and_parser_activity() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
        enemy.crafts[1].depth = 16;
        enemy.explode_craft(6).unwrap();
        assert_eq!(enemy.crafts[1].movement, 3);
        assert_eq!(enemy.crafts[0].movement, 4);
        assert!(enemy.shadows[0].active);
        assert!(!enemy.shadows[1].active);
        for _ in 0..13 {
            enemy.update_half_step(&mut random, [14, 0, -100]);
        }
        assert!(!enemy.crafts[1].active);
        assert!(enemy.crafts[0].active);
        assert!(enemy.active());
        assert_eq!(enemy.shadow_draws().count(), 1);
        enemy.crafts[0].active = false;
        assert!(!enemy.active());
        assert!(enemy.explode_craft(4).is_err());
        assert!(enemy.explode_craft(10).is_err());
    }
    #[test]
    fn later_craft_can_fire_into_shared_slot_before_its_steering_delta() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
        enemy.crafts[0].depth = 800;
        enemy.crafts[0].lateral = 200;
        enemy.crafts[0].height = 50;
        enemy.crafts[1].depth = 700;
        enemy.crafts[1].lateral = -40;
        enemy.crafts[1].height = -10;
        assert_eq!(expected.next_word() & 1, 0);
        expected.next_word();
        expected.next_word();
        assert_eq!(expected.next_word() & 1, 1);
        let lateral = (expected.next_word() & 63) as i16 - 32;
        let height = -100 + (expected.next_word() & 63) as i16 - 32;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(enemy.bullet.active);
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (700, -40, -10)
        );
        assert_eq!(
            (
                enemy.crafts[1].depth,
                enemy.crafts[1].lateral,
                enemy.crafts[1].height
            ),
            (678, -20, -40)
        );
        assert_eq!(enemy.bullet.requested_sprite, 22);
        assert_eq!(enemy.target(), [lateral, height]);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn all_primary_collision_candidates_identify_slots_and_skip_explosions() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        for slot in std::iter::once(&mut enemy.bullet).chain(enemy.crafts.iter_mut()) {
            slot.active = true;
            slot.depth = 29;
            slot.lateral = 128;
            slot.height = -99;
            slot.resolved_sprite = 33;
        }
        enemy.explode_craft(7).unwrap();
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                Some(EnemyCollision::Craft(5)),
                Some(EnemyCollision::Craft(6)),
                None,
                Some(EnemyCollision::Craft(8)),
                Some(EnemyCollision::Craft(9)),
            ]
        );
    }
    #[test]
    fn sweep_initializes_variant_one_but_only_the_first_primary_is_active() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.crafts[0].resolved_sprite = 33;
        enemy.crafts[0].rect.y = 51;
        enemy.spawn_command(0x97, &mut random, 100, 3).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1000, 1000, 1000, 0, 0]);
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, false, false, false, false]
        );
        assert_eq!(
            enemy.shadows.map(|c| c.active),
            [true, false, false, false, false]
        );
        assert_eq!(enemy.crafts.map(|c| c.variant), [1, 1, 1, 0, 0]);
        assert_eq!(enemy.crafts[0].requested_sprite, 0);
        assert_eq!(enemy.crafts[0].resolved_sprite, 33);
        assert_eq!(enemy.crafts[0].rect.y, 51);
    }

    #[test]
    fn sweep_copies_absolute_coordinates_before_firing_and_updates_shared_target() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
        enemy.crafts[0].animation_phase = 33;
        enemy.crafts[0].depth = 800;
        for _ in 0..3 {
            random.next_word();
        }
        let mut expected = random;
        assert_eq!(expected.next_word() & 1, 1);
        let target = [
            50 + (expected.next_word() & 63) as i16 - 32,
            -28 + (expected.next_word() & 63) as i16 - 32,
        ];
        enemy.update_half_step(&mut random, [14, 50, -28]);
        let craft = enemy.crafts[0];
        assert_eq!(
            (craft.depth, craft.lateral, craft.height),
            (460, -1040, 194)
        );
        assert_eq!((craft.requested_sprite, craft.animation_phase), (91, 34));
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (460, -1040, 194)
        );
        assert_eq!(
            (enemy.shadows[0].depth, enemy.shadows[0].lateral),
            (460, -1040)
        );
        assert_eq!(enemy.shadows[0].requested_sprite, 37);
        assert_eq!(enemy.target(), target);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn sweep_terminal_coordinates_retire_before_collision_and_keep_the_requested_source() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
        enemy.crafts[0].animation_phase = 77;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(!enemy.active());
        assert!(!enemy.shadows[0].active);
        assert_eq!(enemy.crafts[0].depth, -50);
        assert_eq!(enemy.crafts[0].animation_phase, 0);
        assert_eq!(enemy.crafts[0].requested_sprite, 100);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
    }

    #[test]
    fn sweep_collision_keeps_the_preceding_descriptor_until_projection() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        assets.sprites[90].descriptor[..3].copy_from_slice(&[40, 158, 15]);
        assets.sprites[91].descriptor[..3].copy_from_slice(&[100, 158, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 1).unwrap();
        enemy.crafts[0].animation_phase = 29;
        enemy.crafts[0].resolved_sprite = 90;
        let player = [453, -1182, 120];
        enemy.update_half_step(&mut random, player);
        assert_eq!(enemy.crafts[0].requested_sprite, 91);
        assert_eq!(enemy.crafts[0].resolved_sprite, 90);
        assert_eq!(
            enemy.player_collisions(&assets, player, 13, true).unwrap(),
            [None; 6]
        );
        enemy.project(&assets, true, 106, 0).unwrap();
        assert_eq!(
            enemy.player_collisions(&assets, player, 13, true).unwrap(),
            [None, Some(EnemyCollision::Craft(5)), None, None, None, None]
        );
    }
    #[test]
    fn relative_sweep_initializes_all_primaries_with_staggered_row_phases() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x96, &mut random, 0, 5).unwrap();
        assert_eq!(enemy.crafts.map(|c| c.animation_phase), [0, 4, 8, 12, 16]);
        assert_eq!(enemy.crafts.map(|c| c.variant), [1; 5]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1000; 5]);
        assert!(enemy.crafts.iter().all(|c| c.active && c.movement == 22));
        assert!(enemy.shadows.iter().all(|c| c.active));
        assert_eq!(random.state(), [0; 3]);
    }

    #[test]
    fn relative_sweep_follows_center_per_craft_and_spaces_absolute_firing_coordinates() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x96, &mut random, 0, 2).unwrap();
        enemy.update_half_step(&mut random, [14, 100, -80]);
        assert_eq!(enemy.orbit_center, [8, -92]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (500, -2756, -80)
        );
        assert_eq!(
            (
                enemy.crafts[1].depth,
                enemy.crafts[1].lateral,
                enemy.crafts[1].height
            ),
            (500, -2488, -76)
        );
        assert_eq!(
            (
                enemy.crafts[0].animation_phase,
                enemy.crafts[1].animation_phase
            ),
            (1, 5)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (500, -2488, -76)
        );
        assert_eq!(
            (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
            (-2756, -2488)
        );
        for _ in 0..4 {
            expected.next_word();
        }
        let target = [
            100 + (expected.next_word() & 63) as i16 - 32,
            -80 + (expected.next_word() & 63) as i16 - 32,
        ];
        assert_eq!(enemy.target(), target);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn negative_depth_projection_wraps_the_table_index_and_resolves_the_source() {
        let mut assets = assets();
        assets.sprites[1].descriptor[5..].fill(0);
        for (depth, lateral, height, origin, camera, expected) in [
            (-50, -160, -28, 107, 18, (38, 109, 3)),
            (-64, 128, 64, 100, 0, (41, 98, 0)),
            (-1024, 128, 64, 100, 0, (24, 132, 0)),
        ] {
            let mut slot = EnemySnapshot::new(4, 0, depth, height);
            slot.lateral = lateral;
            slot.active = true;
            slot.requested_sprite = 1;
            slot.project(&assets, origin, camera).unwrap();
            assert_eq!((slot.rect.x_bytes, slot.rect.y, slot.phase), expected);
            assert_eq!(slot.resolved_sprite, 1);
            assert_eq!((slot.rect.width_bytes, slot.rect.height), (4, 7));

            slot.active = false;
            slot.depth -= 28;
            slot.requested_sprite = 0;
            slot.project(&assets, 200, 100).unwrap();
            assert_eq!(slot.rect.x_bytes, 128);
            assert_eq!((slot.rect.y, slot.phase), (expected.1, expected.2));
            assert_eq!(slot.resolved_sprite, 0);
        }
    }
}
