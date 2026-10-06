// Copyright (c) 2026 analogued. Project license has not been selected.
//! Bounded, no-input opening simulation using interpreted game-state updates.

mod ending;
mod enemy;
mod hud;
mod player;
mod special;
mod terrain;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
    scene,
};

pub use ending::{Ending, EndingSnapshot, EndingStage, EndingTitle};
pub use enemy::EnemySnapshot as EnemyState;
pub use hud::HudSnapshot as HudState;
pub use player::PlayerSnapshot as PlayerState;
pub use special::SpecialSnapshot as SpecialState;
pub use terrain::TerrainSnapshot as TerrainState;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub body: u16,
    pub strip_index: usize,
    pub strip_y: usize,
    pub vertical_origin: i32,
    pub random: [u16; 3],
    pub script_cursor: u16,
    pub script_delay: u16,
    pub terrain_selector: u16,
    pub primary_count: u16,
    pub hud: HudState,
    pub player: PlayerState,
    pub terrain: [TerrainState; 8],
    pub enemies: [EnemyState; 11],
    pub enemy_target: [i16; 2],
    pub special: [SpecialState; 5],
    pub world_order: [usize; 19],
    pub full_world_order: [usize; 29],
}

/// No directional input, with bounded encounter and collision/damage updates.
#[derive(Debug)]
pub struct Opening {
    body: u16,
    random: Random,
    hud: hud::Hud,
    terrain: terrain::Terrain,
    enemy: enemy::Enemy,
    player: player::Player,
    special: special::Special,
    strip_y: usize,
    vertical_origin: i32,
    script_cursor: u16,
    script_delay: u16,
    terrain_selector: u16,
    primary_count: u16,
    world_order: Vec<usize>,
    full_world_order: Vec<usize>,
    frame: Frame,
}

impl Opening {
    pub fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut random = Random::default();
        random.next_word(); // Intro screen selection.
        random.next_word(); // Fresh-process script selection: script zero.
        let mut opening = Self {
            body: 0,
            random,
            hud: hud::Hud::new(assets)?,
            terrain: terrain::Terrain::new(),
            enemy: enemy::Enemy::new(assets)?,
            player: player::Player::new(assets)?,
            special: special::Special::new(),
            strip_y: 92,
            vertical_origin: 106,
            script_cursor: 0,
            script_delay: 0,
            terrain_selector: 0,
            primary_count: 1,
            world_order: (5..=9)
                .chain(15..=22)
                .chain(std::iter::once(4))
                .chain(23..=27)
                .collect(),
            full_world_order: (0..29).collect(),
            frame: scene::initial_frame(assets)?,
        };
        let depths = opening.world_depths();
        opening.sort_full_world(&depths);
        Ok(opening)
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn complete(&self) -> bool {
        self.body == LAST_BODY || self.player.snapshot().game_over
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            body: self.body,
            strip_index: (2 + 3 - usize::from(self.body % 3)) % 3,
            strip_y: self.strip_y,
            vertical_origin: self.vertical_origin,
            random: self.random.state(),
            script_cursor: self.script_cursor,
            script_delay: self.script_delay,
            terrain_selector: self.terrain_selector,
            primary_count: self.primary_count,
            hud: self.hud.snapshot(),
            player: self.player.snapshot(),
            terrain: self.terrain.snapshots(),
            enemies: self.enemy.snapshots(),
            enemy_target: self.enemy.target(),
            special: self.special.snapshots(),
            world_order: std::array::from_fn(|i| self.world_order[i]),
            full_world_order: std::array::from_fn(|i| self.full_world_order[i]),
        }
    }

    /// Advance one completed loop body. Return false at the supported boundary.
    pub fn advance(&mut self, assets: &GraphicsAssets) -> Result<bool, Error> {
        if self.complete() {
            return Ok(false);
        }
        self.body += 1;
        self.advance_player()?;
        self.advance_world(assets, true)?;
        self.player.project(assets, self.vertical_origin)?;
        // Far-object ordering remains from the first render pass even when the
        // second movement update changes relative depths without reprojecting.
        let first_depths = self.world_depths();
        // Subtype-zero entries form a stable prefix. Other entries keep their
        // preceding order at equal depth, including after another row recycles.
        self.sort_world(&first_depths);
        self.sort_full_world(&first_depths);
        self.hud.update(self.body, &mut self.random);
        self.advance_player()?;
        self.player.project(assets, self.vertical_origin)?;
        self.advance_player()?;
        self.advance_world(assets, false)?;
        self.player.project(assets, self.vertical_origin)?;
        let mut frame = scene::background_at(assets, self.snapshot().strip_index, self.strip_y)?;
        let viewport = Rect {
            x_bytes: 4,
            y: 44,
            width_bytes: 72,
            height: 114,
        };
        self.draw_world(assets, &mut frame, viewport)?;
        for draw in scene::initial_draws()
            .into_iter()
            .skip(2)
            .take(4)
            .chain(self.hud.draws())
        {
            let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite))
            })?;
            frame.draw_sprite(sprite, draw.rect)?;
        }
        self.frame = frame;
        self.player.finish_body();

        // Event creation and recycling happen after the last framebuffer write.
        if !self.enemy.active() {
            let old_delay = self.script_delay;
            self.script_delay += 1;
            if old_delay >= 20 {
                self.script_delay = 1; // Non-wave handler repeats the delay gate once.
                match self.script_cursor {
                    0 => self.terrain_selector = 5,
                    1 => {} // Palette-family 1 is already active.
                    2 => {
                        self.enemy.spawn_command(
                            0x91,
                            &mut self.random,
                            0,
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    3 => self.terrain_selector = 7,
                    4 => {
                        self.enemy.spawn_command(
                            0x84,
                            &mut self.random,
                            0,
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    5 => self.terrain_selector = 6,
                    6 => {
                        self.enemy.spawn_command(
                            0x8e,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    7 => self.terrain_selector = 3,
                    8 => {
                        self.enemy.spawn_command(
                            0x8a,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    9 => self.terrain_selector = 7,
                    10 => {
                        self.random.next_word();
                        self.primary_count += 1;
                    }
                    11 => {
                        self.enemy.spawn_command(
                            0x88,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    12 => self.terrain_selector = 9,
                    13 => self.special.spawn_command_40(&mut self.random, 28, false),
                    14 => {
                        self.enemy.spawn_command(
                            0x97,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    15 => self.terrain_selector = 9,
                    16 => self.special.spawn_command_40(&mut self.random, 28, false),
                    17 => {
                        self.enemy.spawn_command(
                            0x96,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    18 => self.terrain_selector = 2,
                    19 => {
                        self.random.next_word();
                        self.primary_count += 1;
                    }
                    20 => {
                        self.enemy.spawn_command(
                            0x84,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    21 => self.terrain_selector = 2,
                    22 => {
                        self.enemy.spawn_command(
                            0x92,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
                            usize::from(self.primary_count),
                        )?;
                        self.script_delay = 0;
                    }
                    _ => return Err(Error::InvalidData("unsupported opening command".into())),
                }
                self.script_cursor += 1;
            }
        }
        self.terrain
            .recycle(self.terrain_selector, &mut self.random)?;
        Ok(true)
    }

    fn draw_world(
        &self,
        assets: &GraphicsAssets,
        frame: &mut Frame,
        viewport: Rect,
    ) -> Result<(), Error> {
        let [player_shadow, player_craft] = self.player.draws();
        for draw in std::iter::once(player_shadow).chain(self.enemy.shadow_draws()) {
            draw_clipped(assets, frame, draw, viewport)?;
        }
        let enemies = self.enemy.snapshots();
        let terrain = self.terrain.snapshots();
        for &slot in &self.world_order {
            let (active, draw) = match slot {
                4..=9 => {
                    let state = enemies[slot - 4];
                    (
                        state.active,
                        scene::Draw {
                            resolved_sprite: state.resolved_sprite,
                            rect: state.rect,
                        },
                    )
                }
                15..=22 => {
                    let state = terrain[slot - 15];
                    (
                        state.active,
                        scene::Draw {
                            resolved_sprite: state.resolved_sprite,
                            rect: state.rect,
                        },
                    )
                }
                23..=27 => {
                    self.special.draw_slot(frame, slot, viewport)?;
                    continue;
                }
                _ => unreachable!("unsupported world slot"),
            };
            if active {
                draw_clipped(assets, frame, draw, viewport)?;
            }
        }
        draw_clipped(assets, frame, player_craft, viewport)
    }

    fn world_depths(&self) -> Vec<(usize, i16, u8)> {
        self.enemy
            .snapshots()
            .into_iter()
            .map(|slot| (slot.slot, slot.depth, slot.movement))
            .chain(
                self.terrain
                    .snapshots()
                    .into_iter()
                    .map(|slot| (slot.slot, slot.depth, slot.movement)),
            )
            .chain(
                self.special
                    .snapshots()
                    .into_iter()
                    .map(|slot| (slot.slot, slot.depth, slot.movement)),
            )
            .chain([
                (0, self.player.snapshot().coordinates[0], 255),
                (1, self.player.snapshot().coordinates[0] + 1, 0),
                (2, 400, 254),
                (3, 700, 254),
                (28, 0, 0),
            ])
            .collect()
    }

    fn sort_full_world(&mut self, depths: &[(usize, i16, u8)]) {
        self.full_world_order.sort_by_key(|slot| {
            let (_, depth, movement) = depths.iter().find(|(index, _, _)| index == slot).unwrap();
            (
                *movement != 0,
                std::cmp::Reverse(if *movement == 0 { 0 } else { *depth }),
            )
        });
    }

    fn sort_world(&mut self, depths: &[(usize, i16, u8)]) {
        self.world_order.sort_by_key(|slot| {
            let (_, depth, movement) = depths.iter().find(|(index, _, _)| index == slot).unwrap();
            (
                *movement != 0,
                std::cmp::Reverse(if *movement == 0 { 0 } else { *depth }),
            )
        });
    }

    fn advance_world(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
    ) -> Result<(), Error> {
        let player = self.player.snapshot();
        self.enemy
            .update_half_step(&mut self.random, player.coordinates);
        let collected_fuel = self.terrain.update_half_step(player.coordinates);
        self.hud.add_fuel(collected_fuel);
        self.special.update_half_step(player.coordinates[0], 28);
        // All motion precedes collision, which sees the previous resolved sources.
        let collisions = self.enemy.player_collisions(
            assets,
            player.coordinates,
            player.resolved_sprite,
            player.phase == 0,
        )?;
        for collision in collisions.into_iter().flatten() {
            if self.player.snapshot().phase != 0 {
                break;
            }
            match collision {
                enemy::EnemyCollision::Projectile => self.player.hit(1),
                enemy::EnemyCollision::Craft(slot) => {
                    self.hud.add_points(1000);
                    self.enemy.explode_craft(slot)?;
                    self.player.hit(2);
                }
            }
        }
        if let Some(collision) = self.terrain.player_collision(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.snapshot().phase == 0,
            self.player.can_damage(),
        )? {
            self.hud.add_fuel(collision.fuel);
            self.hud.add_points(collision.points);
            self.player.hit(collision.damage);
        }
        if let Some(collision) = self.special.player_collision(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.snapshot().phase == 0,
            self.player.can_damage(),
        )? {
            self.hud.add_points(collision.points);
            if let Some(damage) = collision.damage {
                self.player.hit(damage);
            }
        }
        if full_projection {
            self.strip_y = (92 + (player.horizon >> 3)) as usize;
            self.vertical_origin = 106 + i32::from(player.horizon >> 4);
        }
        self.enemy.project(
            assets,
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.special.project(
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.terrain.project(
            assets,
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )
    }

    fn advance_player(&mut self) -> Result<(), Error> {
        if self.player.update_no_input()? {
            self.hud.reset_fuel();
            self.hud.set_lives(self.player.snapshot().lives);
        }
        Ok(())
    }
}

fn draw_clipped(
    assets: &GraphicsAssets,
    frame: &mut Frame,
    draw: scene::Draw,
    viewport: Rect,
) -> Result<(), Error> {
    let sprite = assets
        .sprites
        .get(draw.resolved_sprite)
        .ok_or_else(|| Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite)))?;
    frame.draw_sprite_clipped(sprite, draw.rect, viewport)
}

/// Final completed body of the currently verified, no-input opening batch.
pub const LAST_BODY: u16 = 836;

/// Three nominal 36.413 Hz timer units per completed loop body.
/// Original CPU/render costs and startup timer transitions are not emulated.
pub const BODY_PERIOD: std::time::Duration = std::time::Duration::from_nanos(82_388_102);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Random {
    words: [u16; 3],
}

impl Random {
    pub(crate) fn next_word(&mut self) -> u16 {
        let [mut a, mut b, mut c] = self.words;
        a = a.wrapping_add(0x2432) ^ b ^ c;
        b = (a.rotate_right(1).wrapping_sub(c) ^ b).wrapping_add(0x1238);
        let sum = u32::from((b ^ a).rotate_right(1)) + u32::from(a);
        // RCR includes the carry produced by ADD, unlike an ordinary rotation.
        b = ((sum as u16) >> 1) | if sum > 0xffff { 0x8000 } else { 0 };
        c = c.wrapping_add(a) ^ 0x2033 ^ b;
        self.words = [a, b, c];
        c
    }

    pub(crate) fn state(&self) -> [u16; 3] {
        self.words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_preserves_sixteen_bit_arithmetic_and_add_carry() {
        let mut random = Random::default();
        let words: Vec<_> = (0..4).map(|_| random.next_word()).collect();
        assert_eq!(words, [0x5630, 0xbe90, 0x6338, 0x4293]);
        assert_eq!(random.state(), [34449, 35689, 17043]);
    }

    fn synthetic_assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![0; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|index| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: if index == 13 {
                        [0, 0, 15, 0, 0, 0, 0]
                    } else {
                        [0; 7]
                    },
                    color: vec![0; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        }
    }

    #[test]
    fn authored_events_and_supported_boundary_preserve_the_final_state() {
        let assets = synthetic_assets();
        let mut opening = Opening::new(&assets).unwrap();
        assert_eq!(opening.frame(), &scene::initial_frame(&assets).unwrap());
        for body in 1..=360 {
            assert!(opening.advance(&assets).unwrap());
            let state = opening.snapshot();
            assert_eq!(state.body, body);
            assert_eq!(
                state.terrain_selector,
                if body < 21 {
                    0
                } else if body < 118 {
                    5
                } else if body < 201 {
                    7
                } else if body < 284 {
                    6
                } else {
                    3
                }
            );
            assert_eq!(
                state.enemies[1].active,
                (61..98).contains(&body)
                    || (138..181).contains(&body)
                    || (221..264).contains(&body)
                    || (304..341).contains(&body),
                "enemy activation at body {body}"
            );
            assert_eq!(
                state.script_cursor,
                if body < 21 {
                    0
                } else if body < 41 {
                    1
                } else if body < 61 {
                    2
                } else if body < 118 {
                    3
                } else if body < 138 {
                    4
                } else if body < 201 {
                    5
                } else if body < 221 {
                    6
                } else if body < 284 {
                    7
                } else if body < 304 {
                    8
                } else {
                    9
                }
            );
        }
        // Synthetic descriptor geometry does not reproduce the original
        // collision route. Exercise the bounded guard independently of it.
        opening.body = LAST_BODY;
        let final_frame = opening.frame().clone();
        let final_random = opening.snapshot().random;
        for _ in 0..3 {
            assert!(!opening.advance(&assets).unwrap());
        }
        assert!(opening.complete());
        assert_eq!(opening.snapshot().body, LAST_BODY);
        assert_eq!(opening.snapshot().random, final_random);
        assert_eq!(opening.frame(), &final_frame);
    }

    #[test]
    fn formation_count_command_consumes_one_word_and_waits_to_spawn() {
        let assets = synthetic_assets();
        let mut baseline = Opening::new(&assets).unwrap();
        baseline.script_cursor = 9;
        baseline.script_delay = 20;
        baseline.terrain_selector = 7;
        let mut formation = Opening::new(&assets).unwrap();
        formation.script_cursor = 10;
        formation.script_delay = 20;
        formation.terrain_selector = 7;
        baseline.advance(&assets).unwrap();
        formation.advance(&assets).unwrap();
        let mut expected_random = baseline.random;
        expected_random.next_word();
        assert_eq!(formation.random, expected_random);
        assert_eq!(formation.primary_count, 2);
        assert_eq!(formation.script_cursor, 11);
        assert_eq!(formation.script_delay, 1);
        assert!(!formation.enemy.active());
        for _ in 0..20 {
            formation.advance(&assets).unwrap();
        }
        let state = formation.snapshot();
        assert_eq!(state.script_cursor, 12);
        assert_eq!(state.script_delay, 0);
        assert_eq!(state.enemies[1].depth, 1600);
        assert_eq!(state.enemies[2].depth, 1500);
        assert!(state.enemies[1].active && state.enemies[2].active);
        assert!(state.enemies[6].active && state.enemies[7].active);
    }

    #[test]
    fn gate_allocation_preserves_the_delay_before_the_scripted_enemy() {
        let assets = synthetic_assets();
        let mut opening = Opening::new(&assets).unwrap();
        opening.script_cursor = 13;
        opening.script_delay = 20;
        opening.terrain_selector = 7;
        opening.primary_count = 2;
        opening.advance(&assets).unwrap();
        let state = opening.snapshot();
        assert_eq!(state.script_cursor, 14);
        assert_eq!(state.script_delay, 1);
        assert_eq!(
            state.special.map(|slot| slot.depth),
            [916, 1140, 1364, 1588, 1812]
        );
        assert!(state.special.iter().all(|slot| slot.active
            && slot.rect.x_bytes == 128
            && slot.rect.width_bytes == 43
            && slot.rect.height == 0));
        assert!(!opening.enemy.active());
        for _ in 0..20 {
            opening.advance(&assets).unwrap();
        }
        let state = opening.snapshot();
        assert_eq!(state.script_cursor, 15);
        assert_eq!(state.script_delay, 0);
        assert!(state.enemies[1].active);
        assert!(!state.enemies[2].active);
        assert_eq!(state.enemies[1].variant, 1);
        assert_eq!(state.enemies[1].animation_phase, 0);
        assert_eq!(state.enemies[1].depth, 1000);
        assert_eq!(state.enemies[2].depth, 1000);
        assert!(state.enemies[6].active);
        assert!(!state.enemies[7].active);
    }

    #[test]
    fn zero_movement_entries_keep_order_until_their_movement_changes() {
        let mut opening = Opening::new(&synthetic_assets()).unwrap();
        opening.world_order = vec![4, 5, 6, 15];
        opening.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 900, 0), (15, 200, 2)]);
        assert_eq!(opening.world_order, [5, 6, 15, 4]);
        opening.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 200, 8), (15, 200, 2)]);
        assert_eq!(opening.world_order, [5, 6, 15, 4]);
    }

    #[test]
    fn equal_depth_objects_retain_their_order_from_the_preceding_pass() {
        let mut opening = Opening::new(&synthetic_assets()).unwrap();
        let mut depths: Vec<_> = opening
            .world_order
            .iter()
            .map(|slot| (*slot, 100, 2))
            .collect();
        depths
            .iter_mut()
            .find(|(slot, _, _)| *slot == 22)
            .unwrap()
            .1 = 200;
        opening.sort_world(&depths);
        assert_eq!(opening.world_order[0], 22);
        depths
            .iter_mut()
            .find(|(slot, _, _)| *slot == 22)
            .unwrap()
            .1 = 100;
        opening.sort_world(&depths);
        assert_eq!(opening.world_order[0], 22);
    }
}
