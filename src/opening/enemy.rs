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
        let scale = (64 * 32768) / (i32::from(self.depth) + 64);
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
    Craft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Fixed,
    Seeking,
    Orbit,
}

#[derive(Debug)]
pub(crate) struct Enemy {
    bullet: EnemySnapshot,
    craft: EnemySnapshot,
    shadow: EnemySnapshot,
    target: [i16; 2],
    motion: Motion,
    orbit_center: [i16; 2],
    craft_animation: Option<u8>,
}

impl Enemy {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut state = Self {
            bullet: EnemySnapshot::new(4, 22, 14, -100),
            craft: EnemySnapshot::new(5, 0, 0, 0),
            shadow: EnemySnapshot::new(10, 0, 0, 0),
            target: [0, 0],
            motion: Motion::Fixed,
            orbit_center: [0, -100],
            craft_animation: None,
        };
        state.bullet.project(assets, 106, 0)?;
        state.craft.project(assets, 106, 0)?;
        state.shadow.project(assets, 106, 0)?;
        Ok(state)
    }

    /// Primary script commands initialize world coordinates after rendering,
    /// retaining the source and projection left by the previous wave.
    pub(crate) fn spawn_command(
        &mut self,
        command: u8,
        random: &mut Random,
        player_lateral: i16,
    ) -> Result<(), Error> {
        match command {
            0x91 => {
                self.craft.lateral = ((random.next_word() & 1023) as i16 - 512)
                    .wrapping_sub(player_lateral.wrapping_mul(2));
                self.craft.height = (random.next_word() & 255) as i16 - 50;
            }
            0x84 | 0x8e => {
                self.craft.lateral = 0;
                self.craft.height = 0;
            }
            _ => {
                return Err(Error::InvalidData(format!(
                    "unsupported enemy command {command:02x}"
                )));
            }
        }
        self.motion = match command {
            0x84 => Motion::Seeking,
            0x8e => Motion::Orbit,
            _ => Motion::Fixed,
        };
        self.craft_animation = None;
        self.craft.requested_sprite = 0;
        self.craft.depth = 1600;
        self.craft.active = true;
        self.shadow.height = -115;
        self.shadow.active = true;
        Ok(())
    }

    /// Slot order matters: a projectile can retire before its craft fires again,
    /// and the companion copies coordinates before the primary delta is applied.
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
        if let Some(animation) = self.craft_animation.filter(|_| self.craft.active) {
            let source = explosion_sprite(self.craft.depth, animation);
            if source == 0 {
                self.craft.active = false;
            } else {
                self.craft.requested_sprite = source;
                self.craft_animation = Some(animation + 1);
            }
        } else if self.craft.active {
            let steering = match self.motion {
                Motion::Orbit if self.craft.depth >= 240 => {
                    self.orbit_center[0] +=
                        player[1].wrapping_sub(self.orbit_center[0]).clamp(-9, 9);
                    self.orbit_center[1] +=
                        player[2].wrapping_sub(self.orbit_center[1]).clamp(-9, 9);
                    let phase = ((self.craft.depth & 1023) >> 4) as u8;
                    // The orbital lateral coordinate changes inside movement,
                    // before companion copying and firing. Height is a delta.
                    self.craft.lateral = self.orbit_center[0].wrapping_add(orbit_wave(phase) << 4);
                    let height = self.orbit_center[1]
                        .wrapping_add(orbit_wave(phase.wrapping_add(16)) << 4)
                        .clamp(-100, 250);
                    [0, height.wrapping_sub(self.craft.height)]
                }
                Motion::Seeking | Motion::Orbit => [
                    (player[1].wrapping_sub(self.craft.lateral) >> 1).clamp(-30, 30),
                    (player[2].wrapping_sub(self.craft.height) >> 1).clamp(-30, 30),
                ],
                Motion::Fixed => [0, 0],
            };
            self.craft.requested_sprite = craft_sprite(self.craft.depth, self.craft.phase);
            self.shadow.depth = self.craft.depth;
            self.shadow.lateral = self.craft.lateral;
            self.shadow.requested_sprite = shadow_sprite(self.craft.depth);
            if random.next_word() & 1 != 0 && !self.bullet.active {
                self.bullet.depth = self.craft.depth;
                self.bullet.lateral = self.craft.lateral;
                self.bullet.height = self.craft.height;
                self.bullet.active = true;
            }
            self.target[0] = player[1].wrapping_add((random.next_word() & 63) as i16 - 32);
            self.target[1] = player[2].wrapping_add((random.next_word() & 63) as i16 - 32);
            self.craft.depth -= 22;
            self.craft.lateral += steering[0];
            self.craft.height += steering[1];
            if self.craft.depth <= player[0] {
                self.craft.active = false;
                self.shadow.active = false;
            }
        }
        if !self.craft.active {
            self.shadow.active = false;
        }
    }

    /// Collision handler changes the primary to a stationary explosion, retaining
    /// its current source until the next object update and hiding its companion.
    pub(crate) fn explode_craft(&mut self) {
        self.craft_animation = Some(0);
        self.shadow.active = false;
    }

    /// Collision runs before source resolution, using extents from the previously
    /// resolved sprite. Only the target's height extent participates on that axis.
    pub(crate) fn player_collision(
        &self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        can_damage: bool,
    ) -> Result<Option<EnemyCollision>, Error> {
        if !can_damage {
            return Ok(None);
        }
        let player_source = assets.sprites.get(player_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing player collision sprite {player_sprite}"))
        })?;
        for (slot, collision) in [
            (self.bullet, EnemyCollision::Projectile),
            (self.craft, EnemyCollision::Craft),
        ] {
            if !slot.active || (slot.slot == 5 && self.craft_animation.is_some()) {
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
                return Ok(Some(collision));
            }
        }
        Ok(None)
    }

    /// The final projection list contains every slot with depth strictly below 70,
    /// including inactive slots. Source resolution still happens before activity
    /// sends their rectangles offscreen.
    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        for slot in [&mut self.bullet, &mut self.craft, &mut self.shadow] {
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
    pub(crate) fn shadow_draw(&self) -> Option<Draw> {
        self.shadow.active.then(|| self.shadow.draw())
    }

    pub(crate) fn snapshots(&self) -> [EnemySnapshot; 3] {
        [self.bullet, self.craft, self.shadow]
    }

    pub(crate) fn target(&self) -> [i16; 2] {
        self.target
    }

    pub(crate) fn active(&self) -> bool {
        self.craft.active
    }
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
            sprites: (0..70)
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
        enemy.craft.active = true;
        enemy.craft.depth = 800;
        enemy.craft.lateral = 174;
        enemy.craft.height = 91;
        enemy.craft.phase = 3;
        enemy.shadow.active = true;
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
        assert_eq!(enemy.craft.depth, 778);
        assert_eq!(enemy.shadow.depth, 800);
        assert_eq!(enemy.craft.requested_sprite, 35);
        assert_eq!(enemy.craft.resolved_sprite, 35);
        assert_eq!(random.next_word(), 0x4293);
    }

    #[test]
    fn second_half_firing_retains_the_previous_projected_source() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.craft.active = true;
        enemy.craft.depth = 800;
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
        enemy.craft.active = true;
        enemy.craft.depth = 36;
        enemy.shadow.active = true;
        enemy.update_half_step(&mut Random::default(), [14, 0, -100]);
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.requested_sprite, 0);
        assert!(!enemy.craft.active);
        assert!(!enemy.shadow.active);
        assert_eq!(enemy.craft.requested_sprite, 31);
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            None
        );
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 0);
        assert_eq!(enemy.craft.resolved_sprite, 31);
        assert_eq!(enemy.craft.rect.x_bytes, 128);
    }

    #[test]
    fn retired_projectile_can_be_refired_in_the_same_slot_pass() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 42;
        enemy.craft.active = true;
        enemy.craft.depth = 100;
        enemy.craft.lateral = 200;
        enemy.craft.height = 50;
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
        assert_eq!(enemy.craft.depth, 78);
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
                .player_collision(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            Some(EnemyCollision::Projectile)
        );
        assert!(enemy.bullet.active);
        enemy.bullet.depth = 30;
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            None
        );
        enemy.bullet.depth = 29;
        enemy.bullet.height = -98;
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            None
        );
        enemy.bullet.height = -101;
        enemy.bullet.lateral = -1;
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            Some(EnemyCollision::Projectile)
        );
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 13, false)
                .unwrap(),
            None
        );
    }

    #[test]
    fn primary_spawn_commands_consume_only_their_required_random_words() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 123).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(
            (enemy.craft.depth, enemy.craft.lateral, enemy.craft.height),
            (1600, 0, 0)
        );
        enemy.spawn_command(0x91, &mut random, 123).unwrap();
        assert_eq!(enemy.craft.lateral, (0x5630 & 1023) - 512 - 246);
        assert_eq!(enemy.craft.height, (0xbe90_u16 & 255) as i16 - 50);
        assert_eq!(random.next_word(), 0x6338);
    }
    #[test]
    fn seeking_craft_fires_and_copies_shadow_before_steering_deltas() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0).unwrap();
        enemy.craft.lateral = 71;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!((enemy.craft.lateral, enemy.craft.height), (41, -30));
        assert_eq!((enemy.bullet.lateral, enemy.bullet.height), (71, 0));
        assert_eq!(enemy.bullet.depth, 1600);
        assert_eq!(enemy.shadow.lateral, 71);
        for _ in 0..7 {
            enemy.update_half_step(&mut random, [14, 0, -100]);
        }
        assert_eq!(enemy.craft.height, -100);
    }

    #[test]
    fn craft_explosion_retains_source_then_animates_without_movement_or_random() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.craft.active = true;
        enemy.craft.depth = 16;
        enemy.craft.height = -100;
        enemy.craft.requested_sprite = 33;
        enemy.craft.resolved_sprite = 33;
        enemy.shadow.active = true;
        enemy.explode_craft();
        assert_eq!(enemy.craft.requested_sprite, 33);
        assert!(!enemy.shadow.active);
        assert_eq!(
            enemy
                .player_collision(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            None
        );
        let mut random = Random::default();
        for source in [69, 69, 61, 61, 69, 69, 67, 67, 65, 65, 64, 64] {
            enemy.update_half_step(&mut random, [14, 0, -100]);
            assert!(enemy.craft.active);
            assert_eq!(enemy.craft.requested_sprite, source);
            assert_eq!((enemy.craft.depth, enemy.craft.height), (16, -100));
            assert_eq!(random.state(), [0; 3]);
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(!enemy.craft.active);
        assert_eq!(enemy.craft.requested_sprite, 64);
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.craft.rect.x_bytes, 128);
    }

    #[test]
    fn orbit_changes_lateral_before_firing_but_defers_height_until_after_firing() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 50).unwrap();
        assert_eq!(random.state(), [0; 3]);
        enemy.craft.depth = 1024;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [9, -91]);
        assert_eq!(
            (enemy.craft.depth, enemy.craft.lateral, enemy.craft.height),
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
        assert_eq!(enemy.shadow.lateral, 9);
        assert!((18..=81).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
    }

    #[test]
    fn orbit_switches_to_seeking_only_below_the_old_depth_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 0).unwrap();
        enemy.craft.depth = 240;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (enemy.craft.depth, enemy.craft.lateral, enemy.craft.height),
            (218, 185, -75)
        );
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (enemy.craft.depth, enemy.craft.lateral, enemy.craft.height),
            (196, 155, -52)
        );
        assert_eq!(enemy.orbit_center, [9, -91]);
        enemy.spawn_command(0x8e, &mut random, 0).unwrap();
        assert_eq!(enemy.orbit_center, [9, -91]);
    }

    #[test]
    fn recovered_player_coordinates_drive_seeking_targets_and_retirement() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0).unwrap();
        enemy.update_half_step(&mut random, [28, 100, -28]);
        assert_eq!((enemy.craft.lateral, enemy.craft.height), (30, -14));
        assert!((68..=131).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
        enemy.craft.active = false;
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
        enemy.spawn_command(0x8e, &mut random, 0).unwrap();
        enemy.orbit_center[1] = 200;
        enemy.craft.depth = 1024;
        enemy.update_half_step(&mut random, [14, 0, 230]);
        assert_eq!(enemy.orbit_center[1], 209);
        assert_eq!(enemy.craft.height, 250);
        enemy.orbit_center[1] = -100;
        enemy.craft.depth = 512;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(enemy.craft.height, -100);
    }
}
