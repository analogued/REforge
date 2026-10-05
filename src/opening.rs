// Copyright (c) 2026 analogued. Project license has not been selected.
//! Bounded, no-input opening simulation using interpreted game-state updates.

mod enemy;
mod hud;
mod terrain;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
    scene,
};

pub use enemy::EnemySnapshot as EnemyState;
pub use hud::HudSnapshot as HudState;
pub use terrain::TerrainSnapshot as TerrainState;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub body: u16,
    pub strip_index: usize,
    pub random: [u16; 3],
    pub script_cursor: u16,
    pub script_delay: u16,
    pub terrain_selector: u16,
    pub hud: HudState,
    pub terrain: [TerrainState; 8],
    pub enemies: [EnemyState; 3],
    pub enemy_target: [i16; 2],
}

/// Stationary player, terrain, HUD, and the opening's first enemy wave.
#[derive(Debug)]
pub struct Opening {
    body: u16,
    random: Random,
    hud: hud::Hud,
    terrain: terrain::Terrain,
    enemy: enemy::Enemy,
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
            random: self.random.state(),
            script_cursor: self.script_cursor,
            script_delay: self.script_delay,
            terrain_selector: self.terrain_selector,
            hud: self.hud.snapshot(),
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
        self.enemy
            .advance_half_step(assets, &mut self.random, true)?;
        self.terrain.advance_half_step(assets, true)?;
        self.hud.update(self.body, &mut self.random);
        self.enemy
            .advance_half_step(assets, &mut self.random, false)?;
        self.terrain.advance_half_step(assets, false)?;
        let mut frame = scene::background(assets, self.snapshot().strip_index)?;
        let viewport = Rect {
            x_bytes: 4,
            y: 44,
            width_bytes: 72,
            height: 114,
        };
        let mut world = self.enemy.draws();
        world.extend(self.terrain.draws());
        world.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        for draw in self
            .enemy
            .shadow_draw()
            .into_iter()
            .chain(world.into_iter().map(|(_, draw)| draw))
        {
            let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite))
            })?;
            frame.draw_sprite_clipped(sprite, draw.rect, viewport)?;
        }
        for draw in scene::initial_draws()
            .into_iter()
            .take(6)
            .chain(self.hud.draws())
        {
            let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite))
            })?;
            frame.draw_sprite(sprite, draw.rect)?;
        }
        self.frame = frame;

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
                        self.enemy.spawn(&mut self.random);
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
}

/// Final reference boundary for the currently implemented first-wave preview.
pub const LAST_BODY: u16 = 89;

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
                .map(|_| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: [0; 7],
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
            assert_eq!(state.terrain_selector, if body < 21 { 0 } else { 5 });
            assert_eq!(state.enemies[1].active, body >= 61);
            assert_eq!(
                state.script_cursor,
                if body < 21 {
                    0
                } else if body < 41 {
                    1
                } else if body < 61 {
                    2
                } else {
                    3
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
