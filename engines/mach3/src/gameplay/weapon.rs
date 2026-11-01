// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Two forward-moving player projectiles and the original firing counter.

use super::enemy::EnemySnapshot;
use crate::{
    Error,
    assets::GraphicsAssets,
    sound::{self, Effect, Foreground, Tone},
};

#[derive(Clone, Copy, Debug)]
pub struct WeaponSnapshot {
    pub counter: i16,
    pub shots: [EnemySnapshot; 2],
}

#[derive(Debug)]
pub(crate) struct Weapon {
    state: WeaponSnapshot,
    near: [bool; 2],
}

impl Weapon {
    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut weapon = Self {
            near: [false; 2],
            state: WeaponSnapshot {
                counter: 4,
                shots: std::array::from_fn(|index| EnemySnapshot {
                    slot: index + 2,
                    depth: 400 + index as i16 * 300,
                    lateral: 0,
                    height: -100,
                    active: false,
                    movement: 254,
                    variant: 0,
                    animation_phase: 0,
                    requested_sprite: 22,
                    resolved_sprite: 22,
                    rect: crate::frame::Rect {
                        x_bytes: 128,
                        y: 0,
                        width_bytes: 2,
                        height: 3,
                    },
                    phase: 0,
                }),
            },
        };
        weapon.project(assets, true, 106, 0)?;
        Ok(weapon)
    }

    pub(crate) fn near(&self, index: usize) -> bool {
        self.near[index]
    }

    pub(crate) fn set_counter(&mut self, counter: i16) {
        self.state.counter = counter;
    }

    pub(crate) fn restore(&mut self, shots: [EnemySnapshot; 2]) {
        self.state.shots = shots;
    }

    pub(crate) fn snapshot(&self) -> WeaponSnapshot {
        self.state
    }

    pub(crate) fn shift_lateral(&mut self, delta: i16) {
        for shot in &mut self.state.shots {
            shot.lateral = shot.lateral.wrapping_add(delta);
        }
    }

    /// Called after world collision on the first and third player polls. The
    /// second player's input is not used for firing until a later weapon call.
    #[cfg(test)]
    pub(crate) fn advance(&mut self, fire: bool, player: [i16; 3]) {
        self.advance_with_sound(fire, player, &mut |_| {});
    }

    pub(crate) fn advance_with_sound(
        &mut self,
        fire: bool,
        player: [i16; 3],
        sound: &mut Foreground<'_>,
    ) {
        self.near = self.state.shots.map(|shot| shot.depth < 70);
        if fire {
            self.state.counter = self.state.counter.wrapping_add(7);
        }
        self.state.counter = self.state.counter.wrapping_sub(1);
        if self.state.counter < 207 {
            self.state.counter = self.state.counter.wrapping_sub(6);
            if fire {
                self.state.counter = self.state.counter.wrapping_add(2).max(4);
                let available = if !self.state.shots[0].active && self.state.shots[1].depth > 130 {
                    Some(0)
                } else if !self.state.shots[1].active && self.state.shots[0].depth > 130 {
                    Some(1)
                } else {
                    None
                };
                if let Some(index) = available {
                    self.state.counter = (self.state.counter & 0xf0) + 15;
                    sound::request(
                        sound,
                        if index == 0 { 0x20d3 } else { 0x2128 },
                        Effect::Tone(Tone::Fourth),
                    );
                    let shot = &mut self.state.shots[index];
                    shot.depth = player[0].wrapping_add(15);
                    shot.lateral = player[1].wrapping_add(30);
                    shot.height = player[2];
                    shot.active = true;
                }
            }
        }
        for shot in &mut self.state.shots {
            if shot.active {
                shot.depth = shot.depth.wrapping_add((shot.depth >> 4).wrapping_add(22));
                if shot.depth > 300 {
                    shot.active = false;
                }
            }
            shot.requested_sprite = match shot.depth {
                ..100 => 21,
                100..=160 => 22,
                _ => 23,
            };
        }
    }

    pub(crate) fn deactivate(&mut self, index: usize) {
        self.state.shots[index].active = false;
    }

    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        horizon: i16,
    ) -> Result<(), Error> {
        for (index, shot) in self.state.shots.iter_mut().enumerate() {
            if full_projection || self.near[index] {
                shot.project(assets, vertical_origin, horizon)?;
            }
        }
        Ok(())
    }

    /// Collision precedes the weapon's movement and uses the preceding resolved
    /// source. The height extent belongs to the target on both sides.
    pub(crate) fn overlaps(
        &self,
        assets: &GraphicsAssets,
        index: usize,
        target: EnemySnapshot,
    ) -> Result<bool, Error> {
        let source = assets
            .sprites
            .get(self.state.shots[index].resolved_sprite)
            .ok_or_else(|| {
                Error::InvalidData("missing player projectile collision source".into())
            })?;
        let target_source = assets.sprites.get(target.resolved_sprite).ok_or_else(|| {
            Error::InvalidData("missing projectile target collision source".into())
        })?;
        let shot = self.state.shots[index];
        let overlaps = |position: i16, extent: i16, other: i16, other_extent: i16| {
            position.wrapping_add(extent) >= other && position <= other.wrapping_add(other_extent)
        };
        Ok(overlaps(
            shot.depth,
            source.descriptor[2],
            target.depth,
            target_source.descriptor[2],
        ) && overlaps(
            shot.lateral,
            source.descriptor[1],
            target.lateral,
            target_source.descriptor[1],
        ) && overlaps(
            shot.height,
            target_source.descriptor[0],
            target.height,
            target_source.descriptor[0],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![],
            alternate: vec![],
            sprites: (0..24)
                .map(|_| crate::assets::Sprite {
                    width_bytes: 2,
                    height: 3,
                    descriptor: [9, 9, 70, 0, 0, 0, 0],
                    color: vec![],
                    mask: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn held_fire_alternates_after_the_other_shot_passes_the_spacing_gate() {
        let mut weapon = Weapon::new(&assets()).unwrap();
        let player = [14, -40, 60];
        weapon.advance(true, player);
        let state = weapon.snapshot();
        assert_eq!(state.counter, 15);
        assert_eq!(state.shots[0].depth, 52);
        assert_eq!((state.shots[0].lateral, state.shots[0].height), (-10, 60));
        assert!(state.shots[0].active);
        assert!(!state.shots[1].active);
        for _ in 0..3 {
            weapon.advance(true, player);
        }
        assert_eq!(weapon.snapshot().shots[0].depth, 131);
        assert!(!weapon.snapshot().shots[1].active);
        weapon.advance(true, player);
        assert_eq!(weapon.snapshot().shots[1].depth, 52);
        assert!(weapon.snapshot().shots[1].active);
        assert_eq!(weapon.snapshot().counter, 31);
    }

    #[test]
    fn counter_overheat_and_signed_recovery_do_not_stop_existing_shots() {
        let mut weapon = Weapon::new(&assets()).unwrap();
        weapon.state.counter = 201;
        weapon.advance(true, [14, 0, -100]);
        assert_eq!(weapon.state.counter, 207);
        assert!(weapon.state.shots.iter().all(|s| !s.active));
        weapon.advance(false, [14, 0, -100]);
        assert_eq!(weapon.state.counter, 200);
        weapon.advance(true, [14, 0, -100]);
        assert!(weapon.state.shots[0].active);
        for _ in 0..12 {
            weapon.advance(false, [14, 0, -100]);
        }
        assert!(!weapon.state.shots[0].active);
        assert!(weapon.state.shots[0].depth > 300);
        assert_eq!(weapon.state.shots[0].requested_sprite, 23);
        weapon.state.counter = i16::MIN;
        weapon.advance(false, [14, 0, -100]);
        assert_eq!(weapon.state.counter, i16::MAX);
    }
    #[test]
    fn sound_is_requested_only_when_a_projectile_slot_is_selected() {
        let mut weapon = Weapon::new(&assets()).unwrap();
        let mut actions = Vec::new();
        weapon.state.counter = 201;
        weapon.advance_with_sound(true, [14, 0, -100], &mut |a| actions.push(a));
        assert!(actions.is_empty());
        weapon.state.counter = 4;
        for _ in 0..5 {
            weapon.advance_with_sound(true, [14, 0, -100], &mut |a| actions.push(a));
        }
        assert_eq!(
            actions,
            [
                sound::GameplayAction::Request {
                    site: 0x20d3,
                    effect: Effect::Tone(Tone::Fourth)
                },
                sound::GameplayAction::Request {
                    site: 0x2128,
                    effect: Effect::Tone(Tone::Fourth)
                },
            ]
        );
    }
}
