// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Overlay 2's battle loop (E07D), movement, turning and target selection.
use super::{Preparation, Stage, put};
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;

#[derive(Default)]
pub struct Motion {
    pub paused: bool,
    pub show_orders: bool,
    pub selected: Option<usize>,
    pub message: Option<String>,
    pub(super) pending: Option<(bool, Option<usize>)>,
    pub(super) order_changed: bool,
    pub(super) dragging: bool,
    pub iteration: u16,
    pub finished: bool,
    pub(super) drawn: Vec<[u8; 18]>,
    elapsed: u32,
    initialized: bool,
    activity: [bool; 2],
    idle: [u8; 2],
    ending: u8,
    pub(super) elephant_fear: [bool; 2],
}

impl Preparation {
    pub fn advance(
        &mut self,
        ticks: u32,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        if self.motion.finished {
            if self.outcome.is_none() {
                self.motion.elapsed = self.motion.elapsed.saturating_add(ticks);
                if self.motion.elapsed >= 12 {
                    self.prepare_outcome(state)?;
                    self.motion.elapsed = 0;
                }
            }
            return Ok(());
        }
        if self.stage != Stage::Deployment
            || self.motion.paused
            || self.motion.finished
            || self.motion.message.is_some()
        {
            return Ok(());
        }
        if !self.motion.initialized {
            // E19C checks before the first update too. 4B3F may continue to a
            // second army after the first has destroyed the legion.
            if self.battle_over() {
                self.motion.ending = 1;
            }
            self.motion.elephant_fear[0] = state.data[0x1927 + self.legion * 20] < 1;
            // E012: an elephant encounter invalidates the commander's previous
            // elephant experience. Delay until live entry, after the E000 snapshot.
            if self.ds[0xc2a2] != 0 {
                state.data[0x1927 + self.legion * 20] = 255;
            }
            self.motion.initialized = true;
        }
        self.motion.elapsed = self.motion.elapsed.saturating_add(ticks);
        while self.motion.elapsed >= 12 {
            self.motion.elapsed -= 12;
            self.step(rng)?;
            let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
            state.data[0x1a20 + commander * 6] = self.ds[0x1a20 + commander * 6];
            if self.battle_over() {
                self.motion.ending = self.motion.ending.wrapping_add(1);
                if self.motion.ending > 30 {
                    self.motion.finished = true;
                    if self.motion.elapsed >= 12 {
                        self.prepare_outcome(state)?;
                        self.motion.elapsed = 0;
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    fn step(&mut self, rng: &mut OriginalRng) -> Result<()> {
        if self.motion.iteration != 0 {
            for side in 0..2 {
                self.motion.idle[side] = if self.motion.activity[side] {
                    0
                } else {
                    self.motion.idle[side].wrapping_add(1)
                };
                self.motion.activity[side] = false;
            }
            let defense = self.ds[0x76fe + (self.enemy_formation * 5 + self.enemy_tactic) * 10];
            if defense == 1 && self.motion.idle[1] >= 10 {
                for unit in self.units.iter_mut().skip(1) {
                    if unit[11] == 1 && unit[13] == 1 {
                        unit[13] = 2;
                    }
                }
            }
        }
        let mut drawn = vec![[0; 18]; self.units.len()];
        let mut surviving: Vec<_> = self
            .units
            .iter()
            .map(|u| u16::from_le_bytes([u[0], u[1]]))
            .collect();
        // E0BF walks in allocation order, so earlier units affect later collisions.
        for (index, image) in drawn.iter_mut().enumerate().skip(1) {
            if self.units[index][13] == 4 {
                image[13] = 4;
                continue;
            }
            if self.units[index][15] == 0 {
                if word(&self.units[index], 4)? == 0 {
                    if !self.march(index) {
                        self.melee(index, &mut surviving, rng)?;
                    }
                    if self.units[index][13] == 3 && self.units[index][10] == 1 {
                        self.panic_elephant(index, rng);
                    }
                } else {
                    self.turn(index);
                }
                if self.units[index][11] == 1 {
                    self.enemy_decision(index, rng);
                }
                self.action(index);
            }
            // B94D queues the current animation *before* incrementing its counter.
            *image = self.units[index];
            if self.units[index][13] == 4 {
                continue;
            }
            let kind = usize::from(self.units[index][9]);
            self.units[index][15] += 1;
            if self.units[index][15] >= self.ds[0x6bd0 + kind] {
                self.units[index][15] = 0;
            }
        }
        self.apply_losses(&surviving, rng)?;
        self.motion.drawn = drawn;
        self.motion.iteration = self.motion.iteration.wrapping_add(1);
        Ok(())
    }

    pub(super) fn clear(&mut self, index: usize) {
        // CB3C only clears cells owned by this unit, preserving borders/overlaps.
        for cell in &mut self.grid {
            if usize::from(*cell) == index {
                *cell = 0;
            }
        }
    }

    fn collision(&self, index: usize) -> u8 {
        let unit = self.units[index];
        let (w, h) = self.footprint(&unit);
        let mut result = 0;
        for x in usize::from(unit[2])..usize::from(unit[2]) + w {
            for y in usize::from(unit[3])..usize::from(unit[3]) + h {
                let cell = if x < 48 && y < 48 {
                    self.grid[x * 48 + y]
                } else {
                    254
                };
                if cell == 254 {
                    result = 254;
                } else if cell != 0 && usize::from(cell) != index {
                    result = 255;
                }
            }
        }
        result
    }

    pub(super) fn place_live(&mut self, index: usize) {
        if self.units[index][13] == 4 {
            return;
        }
        self.occupy(index);
        let unit = self.units[index];
        let (w, h) = self.footprint(&unit);
        for x in usize::from(unit[2])..usize::from(unit[2]) + w {
            for y in usize::from(unit[3])..usize::from(unit[3]) + h {
                if x >= 48 || y >= 48 || self.grid[x * 48 + y] == 254 {
                    self.units[index][13] = 4;
                    self.clear(index);
                    return;
                }
            }
        }
    }

    pub(super) fn request(&mut self, index: usize, action: u8) {
        if self.units[index][17] != 4 {
            self.units[index][17] = action;
        }
    }

    fn action(&mut self, index: usize) {
        if self.units[index][16] == 4 {
            self.units[index][13] = 4;
            self.clear(index);
            return;
        }
        let unit = self.units[index];
        let action = usize::from(unit[17] & 15);
        let facing = usize::from(if action == 2 {
            (unit[17] & 0x30) >> 4
        } else {
            unit[14]
        });
        let mut kind = self.ds[0x6c24 + (action * 4 + usize::from(unit[10])) * 4 + facing];
        if action == 2 && (unit[14] == 1 || unit[14] == 2) {
            kind += 1;
        }
        self.units[index][9] = kind;
        self.units[index][16] = unit[17];
        self.units[index][17] = self.ds[0x724c + action];
    }

    fn turn(&mut self, index: usize) {
        let unit = self.units[index];
        let desired = unit[6] >> 6;
        let old = unit[14];
        if unit[6] & 63 == 0 {
            put(&mut self.units[index], 4, 0);
            return;
        }
        self.clear(index);
        if desired == old {
            put(&mut self.units[index], 4, 0);
        } else if unit[10] == 2 {
            put(&mut self.units[index], 4, 0);
            self.units[index][14] = desired;
            self.request(index, 0);
        } else {
            let facing = if desired.abs_diff(old) == 2 {
                (desired + old) / 2
            } else {
                desired
            };
            self.units[index][14] = facing;
            if unit[10] == 1 || unit[10] == 3 {
                let at = usize::from(old) * 4 + usize::from(facing);
                self.units[index][2] = unit[2].wrapping_add(self.ds[0x740a + at]);
                self.units[index][3] = unit[3].wrapping_add(self.ds[0x741a + at]);
            }
            if self.collision(index) == 255 {
                self.units[index][2..4].copy_from_slice(&unit[2..4]);
                self.units[index][14] = old;
                self.request(index, 0);
            } else {
                if facing == desired {
                    put(&mut self.units[index], 4, 0);
                }
                self.request(index, (old << 4) | 2);
            }
        }
        self.place_live(index);
    }

    pub(super) fn front(&self, index: usize, direction: u8) -> [u8; 2] {
        // C948 uses the type's unrotated frontage for all four directions.
        let unit = self.units[index];
        let width = usize::from(self.ds[0x6ae8 + usize::from(unit[10])]);
        let depth = i32::from(self.ds[0x6aec + usize::from(unit[10])]);
        let mut result = [0; 2];
        for (n, value) in result.iter_mut().enumerate().take(width) {
            let (x, y) = (i32::from(unit[2]), i32::from(unit[3]));
            let (x, y) = match direction {
                0 => (x + depth, y + n as i32),
                1 => (x + n as i32, y + depth),
                2 => (x - 1, y + n as i32),
                _ => (x + n as i32, y - 1),
            };
            *value = if (0..48).contains(&x) && (0..48).contains(&y) {
                self.grid[x as usize * 48 + y as usize]
            } else {
                254
            };
        }
        result
    }

    fn march(&mut self, index: usize) -> bool {
        let unit = self.units[index];
        if word(&unit, 0).unwrap() == 0 || unit[13] == 4 || unit[16] == 4 || unit[16] & 15 == 2 {
            return false;
        }
        let direction = unit[6] >> 6;
        let mut remaining = unit[6] & 63;
        if unit[6] == 128 || remaining == 0 {
            if unit[13] == 2 {
                self.seek(index);
                return true;
            }
            return false;
        }
        self.clear(index);
        match direction {
            0 => self.units[index][2] = unit[2].wrapping_add(1),
            1 => self.units[index][3] = unit[3].wrapping_add(1),
            2 => self.units[index][2] = unit[2].wrapping_sub(1),
            _ => self.units[index][3] = unit[3].wrapping_sub(1),
        }
        remaining -= 1;
        self.request(index, 1);
        let moved = if unit[13] == 3 && unit[10] != 1 {
            !self.routing_blocked(index)
        } else {
            self.collision(index) != 255
        };
        if !moved {
            remaining += 1;
            self.units[index][2..4].copy_from_slice(&unit[2..4]);
            self.request(index, 0);
            let enemy = self.front(index, unit[14]).into_iter().any(|other| {
                other != 0 && other < 254 && self.units[usize::from(other)][11] != unit[11]
            });
            if !enemy && unit[13] == 2 {
                self.units[index][6] = 128;
                self.seek(index);
                remaining = self.units[index][6] & 63;
            }
        }
        if remaining == 0 {
            put(&mut self.units[index], 4, 65535);
            if unit[13] == 2 {
                self.seek(index);
            } else {
                self.units[index][6] = unit[7];
                self.units[index][7] = unit[8];
                self.units[index][8] = 128;
            }
        } else {
            self.units[index][6] = (self.units[index][6] & 0xc0) | remaining;
        }
        self.place_live(index);
        if self.units[index][3] == 1 || self.units[index][3] == 44 {
            self.units[index][13] = 4;
            self.clear(index);
        }
        self.motion.activity[usize::from(unit[11])] = moved;
        moved
    }

    pub(super) fn seek(&mut self, index: usize) {
        let unit = self.units[index];
        let mut nearest = None;
        let mut distance = i32::MAX;
        for (other, candidate) in self.units.iter().enumerate().skip(1) {
            if candidate[11] == unit[11] || matches!(candidate[13], 3 | 4) {
                continue;
            }
            let x = i32::from(unit[2] as i8) - i32::from(candidate[2] as i8);
            let y = i32::from(unit[3] as i8) - i32::from(candidate[3] as i8);
            let squared = x * x + y * y;
            if squared < distance || (squared == distance && candidate[10] == 3) {
                nearest = Some(other);
                distance = squared;
            }
        }
        let Some(other) = nearest else {
            self.units[index][6] = 128;
            return;
        };
        let target = self.units[other];
        let x = i32::from(unit[2] as i8) - i32::from(target[2] as i8);
        let y = i32::from(unit[3] as i8) - i32::from(target[3] as i8);
        let mut first = (if x > 0 { 128 } else { 0 }) | ((x.abs() + 1) / 2) as u8;
        let mut second = (if y > 0 { 192 } else { 64 }) | ((y.abs() + 1) / 2) as u8;
        // B71B compares signed deltas, not their absolute lengths.
        if y > x {
            std::mem::swap(&mut first, &mut second);
        }
        self.units[index][6] = first;
        if first & 63 == 0
            || self.front(index, first >> 6).into_iter().any(|other| {
                other != 0 && other < 254 && self.units[usize::from(other)][11] == unit[11]
            })
        {
            self.units[index][6] = second;
        }
        put(&mut self.units[index], 4, 65535);
    }

    fn enemy_decision(&mut self, index: usize, rng: &mut OriginalRng) {
        if self.motion.iteration <= 15 || self.units[index][13] != 1 {
            return;
        }
        let count = self
            .units
            .iter()
            .skip(1)
            .filter(|u| u[13] != 4)
            .count()
            .min(13);
        let at = 0x6b18 + ((count - 1) & !3) + usize::from(self.units[index][12] / 25);
        let aggression = self.ds[0xc2a3];
        if rng.bounded(100) > u16::from(aggression).wrapping_sub(u16::from(self.ds[at])) {
            self.units[index][13] = 2;
        }
    }

    pub(super) fn draw_controls(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        state: &Campaign,
    ) -> Result<()> {
        if let Some(outcome) = &self.outcome {
            outcome.render(frame, assets)?;
            if let Some(message) = &outcome.replacement {
                crate::legion::message(frame, assets, state, message)?;
            }
        } else if self.motion.paused {
            for row in 0..4 {
                let at = 0x748e + row * 8;
                let left = i32::from(word(&self.ds, at)?);
                let top = i32::from(word(&self.ds, at + 2)?);
                let right = i32::from(word(&self.ds, at + 4)?);
                let bottom = i32::from(word(&self.ds, at + 6)?);
                frame.bevel(left, top, right, bottom, 26);
                frame.text(
                    assets,
                    &state.string(word(&self.ds, 0x74ae + row * 2)? as usize)?,
                    left + 5,
                    top + 5,
                    15,
                )?;
            }
            self.draw_selection(frame, assets, state)?;
        }
        if let Some(message) = &self.motion.message {
            crate::legion::message(frame, assets, state, message)?;
        }
        Ok(())
    }
}
