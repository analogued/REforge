// Copyright (c) 2026 analogued. Project license has not been selected.
//! Bounded, no-input opening simulation using interpreted game-state updates.

mod enemy;
mod hud;
mod player;
mod terrain;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
    scene,
};

pub use enemy::EnemySnapshot as EnemyState;
pub use hud::HudSnapshot as HudState;
pub use player::PlayerSnapshot as PlayerState;
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
    pub hud: HudState,
    pub player: PlayerState,
    pub terrain: [TerrainState; 8],
    pub enemies: [EnemyState; 3],
    pub enemy_target: [i16; 2],
}

/// No directional input, two enemy waves, and their collision/damage responses.
#[derive(Debug)]
pub struct Opening {
    body: u16,
    random: Random,
    hud: hud::Hud,
    terrain: terrain::Terrain,
    enemy: enemy::Enemy,
    player: player::Player,
    strip_y: usize,
    vertical_origin: i32,
    script_cursor: u16,
    script_delay: u16,
    terrain_selector: u16,
    frame: Frame,
}

impl Opening {
    pub fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut random = Random::default();
        random.next_word(); // Intro screen selection.
        random.next_word(); // Fresh-process script selection: script zero.
        Ok(Self {
            body: 0,
            random,
            hud: hud::Hud::new(assets)?,
            terrain: terrain::Terrain::new(),
            enemy: enemy::Enemy::new(assets)?,
            player: player::Player::new(assets)?,
            strip_y: 92,
            vertical_origin: 106,
            script_cursor: 0,
            script_delay: 0,
            terrain_selector: 0,
            frame: scene::initial_frame(assets)?,
        })
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn complete(&self) -> bool {
        self.body == LAST_BODY
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
            hud: self.hud.snapshot(),
            player: self.player.snapshot(),
            terrain: self.terrain.snapshots(),
            enemies: self.enemy.snapshots(),
            enemy_target: self.enemy.target(),
        }
    }

    /// Advance one completed loop body. Return false at the supported boundary.
    pub fn advance(&mut self, assets: &GraphicsAssets) -> Result<bool, Error> {
        if self.complete() {
            return Ok(false);
        }
        self.body += 1;
        self.advance_player()?;
        self.advance_enemy(assets, true)?;
        self.terrain.advance_half_step(
            assets,
            true,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.player.project(assets, self.vertical_origin)?;
        // Far-object ordering remains from the first render pass even when the
        // second movement update changes relative depths without reprojecting.
        let first_depths: Vec<_> = self
            .enemy
            .snapshots()
            .into_iter()
            .take(2)
            .map(|slot| (slot.slot, slot.depth))
            .chain(
                self.terrain
                    .snapshots()
                    .into_iter()
                    .map(|slot| (slot.slot, slot.depth)),
            )
            .collect();
        self.hud.update(self.body, &mut self.random);
        self.advance_player()?;
        self.player.project(assets, self.vertical_origin)?;
        self.advance_player()?;
        self.advance_enemy(assets, false)?;
        self.terrain.advance_half_step(
            assets,
            false,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.player.project(assets, self.vertical_origin)?;
        let mut frame = scene::background_at(assets, self.snapshot().strip_index, self.strip_y)?;
        let viewport = Rect {
            x_bytes: 4,
            y: 44,
            width_bytes: 72,
            height: 114,
        };
        let mut world: Vec<_> = self
            .enemy
            .snapshots()
            .into_iter()
            .take(2)
            .filter(|slot| slot.active)
            .map(|slot| {
                (
                    slot.slot,
                    scene::Draw {
                        resolved_sprite: slot.resolved_sprite,
                        rect: slot.rect,
                    },
                )
            })
            .chain(
                self.terrain
                    .snapshots()
                    .into_iter()
                    .filter(|slot| slot.active)
                    .map(|slot| {
                        (
                            slot.slot,
                            scene::Draw {
                                resolved_sprite: slot.resolved_sprite,
                                rect: slot.rect,
                            },
                        )
                    }),
            )
            .map(|(slot, draw)| {
                (
                    first_depths
                        .iter()
                        .find(|(index, _)| *index == slot)
                        .unwrap()
                        .1,
                    draw,
                )
            })
            .collect();
        world.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        let [player_shadow, player_craft] = self.player.draws();
        for draw in std::iter::once(player_shadow)
            .chain(
                self.enemy
                    .shadow_draw()
                    .into_iter()
                    .chain(world.into_iter().map(|(_, draw)| draw)),
            )
            .chain(std::iter::once(player_craft))
        {
            let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite))
            })?;
            frame.draw_sprite_clipped(sprite, draw.rect, viewport)?;
        }
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
                        self.enemy.spawn_command(0x91, &mut self.random, 0)?;
                        self.script_delay = 0;
                    }
                    3 => self.terrain_selector = 7,
                    4 => {
                        self.enemy.spawn_command(0x84, &mut self.random, 0)?;
                        self.script_delay = 0;
                    }
                    5 => self.terrain_selector = 6,
                    6 => {
                        self.enemy.spawn_command(
                            0x8e,
                            &mut self.random,
                            self.player.snapshot().coordinates[1],
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

    fn advance_enemy(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
    ) -> Result<(), Error> {
        let player = self.player.snapshot();
        self.enemy
            .update_half_step(&mut self.random, player.coordinates);
        // Collision sees the preceding resolved descriptors, before projection.
        if let Some(collision) = self.enemy.player_collision(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.can_damage(),
        )? {
            match collision {
                enemy::EnemyCollision::Projectile => self.player.hit(1),
                enemy::EnemyCollision::Craft => {
                    self.hud.add_points(1000);
                    self.enemy.explode_craft();
                    self.player.hit(2);
                }
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

/// Final completed body of the currently verified, no-input opening batch.
pub const LAST_BODY: u16 = 270;

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
        for body in 1..=LAST_BODY {
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
                } else {
                    6
                }
            );
            assert_eq!(
                state.enemies[1].active,
                (61..98).contains(&body)
                    || (138..181).contains(&body)
                    || (221..264).contains(&body),
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
                } else {
                    7
                }
            );
        }
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
}
