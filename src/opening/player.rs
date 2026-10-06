// Copyright (c) 2026 analogued. Project license has not been selected.
//! Craft steering, banking and damage response.

use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Steering {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Steering {
    pub const fn bits(self) -> u16 {
        self.up as u16
            | ((self.down as u16) << 1)
            | ((self.left as u16) << 2)
            | ((self.right as u16) << 3)
    }
}

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
    pub game_over: bool,
    pub shadow: Draw,
    /// Lateral and vertical velocities after the response branch.
    pub velocity: [i16; 2],
    pub orientation: i16,
    pub vertical_band: i16,
    pub effective_input: u16,
    /// Clamped lateral velocity applied to the world before response zeroing.
    pub update_lateral_velocity: i16,
}

#[derive(Debug)]
pub(crate) struct Player {
    state: PlayerSnapshot,
    velocity: [i16; 2],
    orientation: i16,
    vertical_band: i16,
    shadow: Draw,
}

impl Player {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let shadow = Draw {
            resolved_sprite: 18,
            rect: Rect {
                x_bytes: 37,
                y: 147,
                width_bytes: 14,
                height: 12,
            },
        };
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
                game_over: false,
                shadow,
                velocity: [0, 0],
                orientation: 3,
                vertical_band: 2,
                effective_input: 0,
                update_lateral_velocity: 0,
            },
            velocity: [0, 0],
            orientation: 3,
            vertical_band: 2,
            shadow,
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
    /// the caller's fuel reset. Exhausting the last symbol allows one more
    /// recovery; a death with no symbol sets the exit flag for the next body.
    pub(crate) fn update_no_input(&mut self) -> Result<bool, Error> {
        self.update(Steering::default())
    }

    pub(crate) fn update(&mut self, steering: Steering) -> Result<bool, Error> {
        let responding = self.state.phase != 0;
        let lateral = self.state.coordinates[1];
        let height = self.state.coordinates[2];
        let steering = if responding {
            Steering {
                right: lateral < -30,
                left: lateral > 30,
                up: height < -30,
                down: height > 30,
            }
        } else {
            steering
        };
        self.state.effective_input = steering.bits();
        self.velocity[1] = accelerate(self.velocity[1], steering.up, steering.down, 6);
        self.velocity[1] = self.velocity[1].max(-self.state.horizon);
        if height + self.velocity[1] * 4 > 230 {
            self.velocity[1] = (230 - height) >> 2;
        }
        self.state.coordinates[2] += self.velocity[1] * 4;
        self.state.horizon += self.velocity[1];
        self.velocity[0] = accelerate(self.velocity[0], steering.right, steering.left, 7);
        if lateral + self.velocity[0] * 4 > 280 {
            self.velocity[0] = (280 - lateral) >> 2;
        }
        if lateral + self.velocity[0] * 4 < -320 {
            self.velocity[0] = (-320 - lateral) >> 2;
        }
        self.state.coordinates[1] += self.velocity[0] * 4;
        self.state.update_lateral_velocity = self.velocity[0];

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
                            if self.state.lives == 0 {
                                self.state.game_over = true;
                            } else {
                                self.state.lives -= 1;
                            }
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
            self.vertical_band =
                (3 - ((height + 290 + (height >> 2)) >> 7) + self.velocity[1]).clamp(0, 2);
            self.state.requested_sprite = (self.orientation + 5 * self.vertical_band) as usize;
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
        PlayerSnapshot {
            shadow: self.shadow,
            velocity: self.velocity,
            orientation: self.orientation,
            vertical_band: self.vertical_band,
            ..self.state
        }
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

fn accelerate(mut velocity: i16, positive: bool, negative: bool, limit: i16) -> i16 {
    if positive && velocity < limit {
        velocity += 1;
    }
    if negative && velocity > -limit {
        velocity -= 1;
    }
    if !positive && !negative {
        velocity -= velocity.signum();
    }
    velocity
}

/// Roll programs describe durations on the same five-bank, three-height pose
/// grid used by normal flight. The final bank-five, band-two pose precedes recovery.
fn response_sprite(row: u16, phase: u16) -> Result<Option<usize>, Error> {
    if !(1..=16).contains(&row) || !(1..=16).contains(&phase) {
        return Err(Error::InvalidData(format!(
            "invalid craft response row {row}, phase {phase}"
        )));
    }
    if row == 16 {
        return Ok(match phase {
            1 => Some(15),
            2..=4 => Some(57),
            5..=14 => Some(58 + usize::from((phase - 5) / 2)),
            15 => Some(0),
            _ => None,
        });
    }
    // Row eleven adds two phases at bank one, band one before row one.
    if row == 11 {
        return if phase <= 2 {
            Ok(Some(6))
        } else {
            response_sprite(1, phase - 2)
        };
    }
    // Each run is (last phase, bank, height band), rather than an embedded
    // frame-index lookup. Different starting poses reach recovery at different
    // phases; padding after an original terminator is never executed.
    let (runs, recovery): (&[(u16, usize, usize)], u16) = match row {
        1 => (
            &[
                (2, 1, 0),
                (4, 2, 0),
                (6, 3, 0),
                (8, 4, 0),
                (10, 5, 0),
                (12, 5, 1),
            ],
            14,
        ),
        2 => (
            &[(2, 2, 0), (4, 3, 0), (6, 3, 1), (8, 4, 1), (10, 4, 2)],
            12,
        ),
        3 => (
            &[
                (2, 3, 0),
                (4, 2, 0),
                (5, 2, 1),
                (6, 3, 1),
                (8, 4, 1),
                (10, 5, 1),
            ],
            12,
        ),
        4 => (
            &[
                (1, 4, 0),
                (3, 3, 0),
                (5, 2, 1),
                (7, 1, 1),
                (9, 2, 1),
                (11, 4, 1),
                (13, 5, 1),
            ],
            15,
        ),
        5 => (
            &[
                (2, 4, 0),
                (4, 3, 0),
                (6, 2, 0),
                (8, 1, 1),
                (10, 2, 1),
                (12, 4, 1),
                (13, 5, 1),
            ],
            15,
        ),
        6 => (
            &[
                (2, 1, 0),
                (4, 2, 0),
                (5, 3, 0),
                (6, 3, 1),
                (8, 4, 1),
                (10, 5, 1),
                (12, 5, 0),
                (13, 5, 1),
            ],
            15,
        ),
        7 => (
            &[
                (2, 2, 0),
                (4, 1, 0),
                (6, 1, 1),
                (7, 2, 1),
                (8, 3, 1),
                (10, 3, 2),
                (11, 4, 2),
                (12, 4, 1),
                (13, 5, 1),
            ],
            15,
        ),
        8 => (
            &[
                (2, 2, 1),
                (4, 1, 0),
                (6, 1, 1),
                (8, 1, 2),
                (9, 2, 2),
                (10, 3, 2),
                (11, 4, 2),
                (13, 4, 1),
                (14, 5, 1),
            ],
            16,
        ),
        9 => (
            &[
                (1, 3, 1),
                (3, 2, 1),
                (5, 2, 0),
                (7, 3, 0),
                (9, 4, 0),
                (11, 5, 0),
                (13, 5, 1),
            ],
            15,
        ),
        10 => (
            &[
                (2, 4, 1),
                (3, 3, 1),
                (5, 2, 0),
                (7, 3, 0),
                (9, 4, 0),
                (11, 4, 1),
                (13, 4, 2),
            ],
            15,
        ),
        12 => (
            &[
                (2, 2, 1),
                (4, 1, 1),
                (6, 2, 0),
                (8, 3, 0),
                (10, 4, 0),
                (12, 4, 1),
                (14, 4, 2),
            ],
            16,
        ),
        13 => (
            &[
                (2, 2, 1),
                (4, 1, 0),
                (6, 1, 1),
                (8, 2, 2),
                (10, 3, 2),
                (12, 4, 1),
                (14, 4, 2),
            ],
            16,
        ),
        14 => (
            &[
                (2, 3, 2),
                (4, 2, 1),
                (6, 2, 0),
                (7, 3, 0),
                (9, 4, 0),
                (11, 5, 0),
                (13, 5, 1),
            ],
            15,
        ),
        15 => (
            &[
                (2, 4, 2),
                (4, 3, 2),
                (6, 2, 2),
                (8, 2, 1),
                (10, 3, 0),
                (12, 4, 0),
                (13, 4, 1),
                (14, 4, 2),
            ],
            16,
        ),
        _ => unreachable!("death and row-eleven rolls were handled above"),
    };
    if phase >= recovery {
        Ok(None)
    } else {
        Ok(Some(
            runs.iter()
                .find(|run| phase <= run.0)
                .map_or(15, |run| run.1 + 5 * run.2),
        ))
    }
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
        assert!(!player.snapshot().game_over);
        assert_eq!(player.snapshot().grace, 0);
        assert!(!player.can_damage());
        assert!(!player.update_no_input().unwrap());
        assert_eq!(player.snapshot().requested_sprite, 13);
        assert_eq!(player.snapshot().coordinates, [14, 0, -28]);
        assert_eq!(player.snapshot().horizon, 18);
        for _ in 0..4 {
            assert!(!player.can_damage());
            player.finish_body();
        }
        assert!(player.can_damage());
    }

    #[test]
    fn clearing_the_last_life_symbol_recovers_before_the_following_death_exits() {
        let mut player = Player::new(&assets()).unwrap();
        player.state.lives = 1;
        player.state.phase = 15;
        player.state.animation_row = 16;
        player.state.damage = 2;
        player.state.requested_sprite = 0;
        assert!(player.update_no_input().unwrap());
        let state = player.snapshot();
        assert_eq!(state.lives, 0);
        assert!(!state.game_over);
        assert_eq!((state.damage, state.phase, state.grace), (0, 0, 0));
        assert_eq!(state.requested_sprite, 0);

        // The rest of the current body still runs after a completed death.
        assert!(!player.update_no_input().unwrap());
        assert_eq!(player.snapshot().requested_sprite, 13);
        for _ in 0..4 {
            player.finish_body();
        }
        assert!(player.can_damage());
        player.hit(2);
        for _ in 0..29 {
            assert!(!player.update_no_input().unwrap());
            assert!(!player.snapshot().game_over);
        }
        assert!(player.update_no_input().unwrap());
        let state = player.snapshot();
        assert_eq!(state.lives, 0);
        assert!(state.game_over);
        assert_eq!((state.damage, state.phase, state.grace), (0, 0, 0));
        assert_eq!(state.requested_sprite, 0);
        assert!(!player.update_no_input().unwrap());
        assert!(player.snapshot().game_over);
        assert_eq!(player.snapshot().requested_sprite, 13);
    }
    #[test]
    fn steering_accelerates_then_release_slows_and_updates_banking() {
        let mut player = Player::new(&assets()).unwrap();
        let right_up = Steering {
            right: true,
            up: true,
            ..Steering::default()
        };
        for _ in 0..6 {
            player.update(right_up).unwrap();
        }
        let state = player.snapshot();
        assert_eq!(state.velocity, [6, 6]);
        assert_eq!(state.coordinates, [14, 84, -16]);
        assert_eq!(state.horizon, 21);
        assert_eq!(
            (
                state.orientation,
                state.vertical_band,
                state.requested_sprite
            ),
            (1, 2, 11)
        );
        assert_eq!(state.effective_input, 9);
        player.update(Steering::default()).unwrap();
        let state = player.snapshot();
        assert_eq!(state.velocity, [5, 5]);
        assert_eq!(state.coordinates, [14, 104, 4]);
        assert_eq!(state.horizon, 26);
        assert_eq!(state.update_lateral_velocity, 5);
    }

    #[test]
    fn opposing_keys_keep_momentum_except_at_positive_speed_limit() {
        for limit in [6, 7] {
            for velocity in -limit..=limit {
                let expected = if velocity == limit {
                    limit - 1
                } else {
                    velocity
                };
                assert_eq!(accelerate(velocity, true, true, limit), expected);
                assert_eq!(
                    accelerate(velocity, false, false, limit),
                    velocity - velocity.signum()
                );
            }
        }
        assert_eq!(
            Steering {
                up: true,
                down: true,
                left: true,
                right: true
            }
            .bits(),
            15
        );
    }

    #[test]
    fn limits_clamp_velocity_before_world_translation_and_keep_floor_height() {
        let mut player = Player::new(&assets()).unwrap();
        let right_up = Steering {
            right: true,
            up: true,
            ..Steering::default()
        };
        for _ in 0..100 {
            player.update(right_up).unwrap();
        }
        let state = player.snapshot();
        assert_eq!(state.coordinates, [14, 280, 228]);
        assert_eq!(state.velocity, [0, 0]);
        assert_eq!(state.update_lateral_velocity, 0);
        assert_eq!(state.horizon, 82);
        assert_eq!((state.orientation, state.vertical_band), (5, 0));
        let left_down = Steering {
            left: true,
            down: true,
            ..Steering::default()
        };
        for _ in 0..100 {
            player.update(left_down).unwrap();
        }
        let state = player.snapshot();
        assert_eq!(state.coordinates, [14, -320, -100]);
        assert_eq!(state.velocity, [0, 0]);
        assert_eq!(state.horizon, 0);
        assert_eq!((state.orientation, state.vertical_band), (1, 2));
    }

    #[test]
    fn every_flight_pose_can_respond_and_hit_input_is_replaced_by_recentering() {
        for row in 1..=15 {
            let mut player = Player::new(&assets()).unwrap();
            player.state.requested_sprite = row;
            player.state.coordinates[1] = -100;
            player.hit(0);
            player
                .update(Steering {
                    left: true,
                    down: true,
                    ..Steering::default()
                })
                .unwrap();
            let state = player.snapshot();
            assert_eq!(state.effective_input, 9);
            assert_eq!(state.coordinates, [14, -96, -96]);
            assert_eq!(state.velocity, [0, 0]);
            assert_eq!(state.update_lateral_velocity, 1);
            assert_eq!(state.orientation, 3);
            assert_eq!(state.shadow.resolved_sprite, 18);
            for _ in 0..15 {
                if player.snapshot().phase == 0 {
                    break;
                }
                assert!(!player.update(Steering::default()).unwrap());
            }
            let state = player.snapshot();
            assert_eq!(state.phase, 0, "row {row} did not recover");
            assert_eq!(state.requested_sprite, 15);
            assert_eq!(state.lives, 6);
            assert_eq!(state.damage, 0);
            assert!(!state.game_over);
        }
        assert!(response_sprite(0, 1).is_err());
        assert!(response_sprite(17, 1).is_err());
        assert!(response_sprite(1, 0).is_err());
    }
}
