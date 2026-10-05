// Copyright (c) 2026 analogued. Project license has not been selected.
//! No-input craft movement and the opening's damage response.

use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

#[derive(Clone, Copy, Debug)]
pub struct PlayerSnapshot {
    pub coordinates: [i16; 3],
    pub requested_sprite: usize,
    pub resolved_sprite: usize,
    pub rect: Rect,
    pub phase: u16,
    pub damage: u16,
    pub animation_row: u16,
    pub grace: u16,
    pub horizon: i16,
    pub lives: u8,
}

#[derive(Debug)]
pub(crate) struct Player {
    state: PlayerSnapshot,
    velocity: [i16; 2],
    orientation: i16,
    shadow: Draw,
}

impl Player {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut player = Self {
            state: PlayerSnapshot {
                coordinates: [14, 0, -100],
                requested_sprite: 13,
                resolved_sprite: 13,
                rect: Rect {
                    x_bytes: 37,
                    y: 139,
                    width_bytes: 14,
                    height: 18,
                },
                phase: 0,
                damage: 0,
                animation_row: 0,
                grace: 4,
                horizon: 0,
                lives: 6,
            },
            velocity: [0, 0],
            orientation: 3,
            shadow: Draw {
                resolved_sprite: 18,
                rect: Rect {
                    x_bytes: 37,
                    y: 147,
                    width_bytes: 14,
                    height: 12,
                },
            },
        };
        player.project(assets, 106)?;
        Ok(player)
    }

    pub(crate) fn can_damage(&self) -> bool {
        self.state.phase == 0 && self.state.grace >= 4
    }

    pub(crate) fn hit(&mut self, damage: u16) {
        if !self.can_damage() {
            return;
        }
        self.state.damage = self.state.damage.wrapping_add(damage);
        self.state.phase = 1;
        self.state.animation_row = self.state.requested_sprite as u16;
    }

    pub(crate) fn finish_body(&mut self) {
        self.state.grace = (self.state.grace + 1).min(4);
    }

    /// The original calls this three times per body. A completed death requests
    /// the caller's fuel reset; life symbols follow the remaining life count.
    pub(crate) fn update_no_input(&mut self) -> Result<bool, Error> {
        let responding = self.state.phase != 0;
        let lateral = self.state.coordinates[1];
        let height = self.state.coordinates[2];
        let input = if responding {
            [
                if lateral < -30 {
                    1
                } else if lateral > 30 {
                    -1
                } else {
                    0
                },
                if height < -30 {
                    1
                } else if height > 30 {
                    -1
                } else {
                    0
                },
            ]
        } else {
            [0, 0]
        };
        self.velocity[1] = accelerate(self.velocity[1], input[1], 6);
        self.velocity[1] = self.velocity[1].max(-self.state.horizon);
        if height + self.velocity[1] * 4 > 230 {
            self.velocity[1] = (230 - height) >> 2;
        }
        self.state.coordinates[2] += self.velocity[1] * 4;
        self.state.horizon += self.velocity[1];
        self.velocity[0] = accelerate(self.velocity[0], input[0], 7);
        if lateral + self.velocity[0] * 4 > 280 {
            self.velocity[0] = (280 - lateral) >> 2;
        }
        if lateral + self.velocity[0] * 4 < -320 {
            self.velocity[0] = (-320 - lateral) >> 2;
        }
        self.state.coordinates[1] += self.velocity[0] * 4;

        let mut reset_fuel = false;
        if responding {
            self.velocity = [0, 0];
            self.state.phase = (self.state.phase & 15) + 1;
            self.state.animation_row = ((self.state.animation_row - 1) & 15) + 1;
            match response_sprite(self.state.animation_row, self.state.phase)? {
                Some(sprite) => self.state.requested_sprite = sprite,
                None => {
                    self.state.phase = 0;
                    if self.state.damage >= 2 {
                        if self.state.animation_row == 16 {
                            self.state.damage = 0;
                            self.state.requested_sprite = 0;
                            self.state.grace = 0;
                            self.state.lives = self.state.lives.saturating_sub(1);
                            reset_fuel = true;
                        } else {
                            self.state.phase = 1;
                        }
                        self.state.animation_row = 16;
                    }
                }
            }
        } else {
            self.orientation =
                (((self.state.coordinates[1] + 450) >> 7) - self.velocity[0]).clamp(1, 5);
            let height = self.state.coordinates[2];
            let vertical =
                (3 - ((height + 290 + (height >> 2)) >> 7) + self.velocity[1]).clamp(0, 2);
            self.state.requested_sprite = (self.orientation + 5 * vertical) as usize;
        }
        self.shadow.resolved_sprite = (self.orientation + 15) as usize;
        Ok(reset_fuel)
    }

    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        vertical_origin: i32,
    ) -> Result<(), Error> {
        self.state.rect = project(
            assets,
            self.state.requested_sprite,
            self.state.coordinates,
            vertical_origin,
            self.state.horizon,
        )?;
        self.state.resolved_sprite = self.state.requested_sprite;
        self.shadow.rect = project(
            assets,
            self.shadow.resolved_sprite,
            [
                self.state.coordinates[0] + 1,
                self.state.coordinates[1],
                -115,
            ],
            vertical_origin,
            self.state.horizon,
        )?;
        // Each rendering phase copies the craft's projected byte-x to its shadow.
        self.shadow.rect.x_bytes = self.state.rect.x_bytes;
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> PlayerSnapshot {
        self.state
    }

    pub(crate) fn draws(&self) -> [Draw; 2] {
        [
            self.shadow,
            Draw {
                resolved_sprite: self.state.resolved_sprite,
                rect: self.state.rect,
            },
        ]
    }
}

fn accelerate(velocity: i16, input: i16, limit: i16) -> i16 {
    if input == 0 {
        velocity - velocity.signum()
    } else {
        (velocity + input).clamp(-limit, limit)
    }
}

fn response_sprite(row: u16, phase: u16) -> Result<Option<usize>, Error> {
    Ok(match (row, phase) {
        (13, 1..=2) => Some(7),
        (13, 3..=4) => Some(1),
        (13, 5..=6) => Some(6),
        (13, 7..=8) => Some(12),
        (13, 9..=10) => Some(13),
        (13, 11..=12) => Some(9),
        (13, 13..=14) => Some(14),
        (13, 15) | (16, 1) => Some(15),
        (16, 2..=4) => Some(57),
        (16, 5..=14) => Some(58 + usize::from((phase - 5) / 2)),
        (16, 15) => Some(0),
        (13 | 16, 16) => None,
        _ => {
            return Err(Error::InvalidData(format!(
                "unsupported craft response row {row}, phase {phase}"
            )));
        }
    })
}

fn project(
    assets: &GraphicsAssets,
    requested: usize,
    coordinates: [i16; 3],
    vertical_origin: i32,
    camera_height: i16,
) -> Result<Rect, Error> {
    let sprite = assets
        .sprites
        .get(requested)
        .ok_or_else(|| Error::InvalidData(format!("missing player sprite {requested}")))?;
    let scale = (64 * 32768) / (i32::from(coordinates[0]) + 64);
    let horizontal = (scale * i32::from(coordinates[1])) >> 16;
    let vertical = (scale * i32::from(coordinates[2].wrapping_sub(camera_height))) >> 16;
    Ok(Rect {
        x_bytes: 40 + (horizontal >> 2) - i32::from(sprite.descriptor[5]),
        y: vertical_origin - vertical - i32::from(sprite.descriptor[6]),
        width_bytes: sprite.width_bytes,
        height: sprite.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![],
            alternate: vec![],
            sprites: (0..63)
                .map(|_| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: [0; 7],
                    color: vec![],
                    mask: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn damage_requires_four_completed_bodies_and_blocks_further_hits() {
        let mut player = Player::new(&assets()).unwrap();
        player.state.grace = 0; // A completed death resets the immunity gate.
        for _ in 0..4 {
            assert!(!player.can_damage());
            player.hit(2);
            assert_eq!(player.snapshot().damage, 0);
            player.finish_body();
        }
        assert!(player.can_damage());
        player.hit(2);
        player.hit(1);
        assert_eq!(player.snapshot().damage, 2);
        assert_eq!(player.snapshot().phase, 1);
        assert_eq!(player.snapshot().animation_row, 13);
        assert!(!player.can_damage());
    }

    #[test]
    fn response_recenters_before_animation_then_defers_life_loss_until_death_finishes() {
        let mut player = Player::new(&assets()).unwrap();
        for _ in 0..4 {
            player.finish_body();
        }
        player.hit(2);
        for _ in 0..15 {
            assert!(!player.update_no_input().unwrap());
        }
        let state = player.snapshot();
        assert_eq!(state.coordinates, [14, 0, -40]);
        assert_eq!(state.horizon, 15);
        assert_eq!(state.animation_row, 16);
        assert_eq!(state.phase, 1);
        assert_eq!(state.requested_sprite, 15);
        assert_eq!(state.lives, 6);
        for _ in 0..3 {
            assert!(!player.update_no_input().unwrap());
        }
        assert_eq!(player.snapshot().coordinates[2], -28);
        assert_eq!(player.snapshot().requested_sprite, 57);
        assert_eq!(player.snapshot().horizon, 18);
        for _ in 0..11 {
            assert!(!player.update_no_input().unwrap());
        }
        assert_eq!(player.snapshot().requested_sprite, 0);
        assert!(player.update_no_input().unwrap());
        assert_eq!(player.snapshot().damage, 0);
        assert_eq!(player.snapshot().lives, 5);
        assert_eq!(player.snapshot().grace, 0);
    }
}
