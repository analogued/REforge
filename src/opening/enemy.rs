// Copyright (c) 2026 analogued. Project license has not been selected.
//! First opening wave, its ground shadow, and its single seeking projectile.

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

    fn project(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
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
        let vertical = (scale * i32::from(self.height)) >> 16;
        self.rect.y = 106 - vertical - i32::from(sprite.descriptor[6]);
        Ok(())
    }

    fn draw(self) -> Draw {
        Draw {
            resolved_sprite: self.resolved_sprite,
            rect: self.rect,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Enemy {
    bullet: EnemySnapshot,
    craft: EnemySnapshot,
    shadow: EnemySnapshot,
    target: [i16; 2],
}

impl Enemy {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut state = Self {
            bullet: EnemySnapshot::new(4, 22, 14, -100),
            craft: EnemySnapshot::new(5, 0, 0, 0),
            shadow: EnemySnapshot::new(10, 0, 0, 0),
            target: [0, 0],
        };
        state.bullet.project(assets)?;
        state.craft.project(assets)?;
        state.shadow.project(assets)?;
        Ok(state)
    }

    /// Script command 0x91 initializes the primary slot and companion after rendering.
    pub(crate) fn spawn(&mut self, random: &mut Random) {
        self.craft.depth = 1600;
        self.craft.lateral = (random.next_word() & 1023) as i16 - 512;
        self.craft.height = (random.next_word() & 255) as i16 - 50;
        self.craft.active = true;
        self.shadow.height = -115;
        self.shadow.active = true;
    }

    /// Object slots update in numerical order. First-half projection resolves all
    /// sources; the second-half near list excludes these objects within this preview.
    pub(crate) fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        random: &mut Random,
        full_projection: bool,
    ) -> Result<(), Error> {
        if self.bullet.active {
            self.bullet.requested_sprite = match size(self.bullet.depth) {
                0..=3 => 64,
                4..=6 => 63,
                7..=9 => 65,
                _ => 66,
            };
            self.bullet.lateral += (self.target[0] - self.bullet.lateral).clamp(-80, 80);
            self.bullet.height += (self.target[1] - self.bullet.height).clamp(-80, 80);
            self.bullet.depth -= 28;
        }
        if self.craft.active {
            self.craft.requested_sprite = craft_sprite(self.craft.depth, self.craft.phase);
            self.shadow.depth = self.craft.depth;
            self.shadow.lateral = self.craft.lateral;
            self.shadow.requested_sprite = shadow_sprite(self.craft.depth);
            // The fire and target routines see coordinates before the forward delta.
            if random.next_word() & 1 != 0 && !self.bullet.active {
                self.bullet.depth = self.craft.depth;
                self.bullet.lateral = self.craft.lateral;
                self.bullet.height = self.craft.height;
                self.bullet.active = true;
            }
            self.target[0] = (random.next_word() & 63) as i16 - 32;
            self.target[1] = (random.next_word() & 63) as i16 - 132;
            self.craft.depth -= 22;
        }
        if full_projection {
            self.bullet.project(assets)?;
            self.craft.project(assets)?;
            self.shadow.project(assets)?;
        }
        Ok(())
    }

    /// Subtype-zero shadows sort ahead of all ordinary world objects.
    pub(crate) fn shadow_draw(&self) -> Option<Draw> {
        self.shadow.active.then(|| self.shadow.draw())
    }

    /// Merge ordinary objects with terrain using descending depth and slot order.
    pub(crate) fn draws(&self) -> Vec<(i16, Draw)> {
        [self.bullet, self.craft]
            .into_iter()
            .filter(|slot| slot.active)
            .map(|slot| (slot.depth, slot.draw()))
            .collect()
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
            sprites: (0..67)
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
}
