// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Overlay 2 melee, simultaneous losses, morale and routing.
use super::{Preparation, put};
use crate::assets::{Result, word};
use crate::rng::OriginalRng;

impl Preparation {
    pub(super) fn melee(
        &mut self,
        index: usize,
        surviving: &mut [u16],
        rng: &mut OriginalRng,
    ) -> Result<()> {
        let unit = self.units[index];
        if word(&unit, 0)? == 0 || unit[13] == 4 || (unit[13] == 3 && unit[10] != 1) {
            return Ok(());
        }
        let neighbors = self.front(index, unit[14]);
        let width = usize::from(self.ds[0x6ae8 + usize::from(unit[10])]);
        for &other in neighbors.iter().take(width) {
            if other == 0 || other >= 254 {
                continue;
            }
            let other = usize::from(other);
            if self.units[other][11] == unit[11] && unit[10] != 1 {
                continue;
            }
            self.request(index, 3);
            if unit[10] == 1 && self.motion.elephant_fear[usize::from(unit[11])] {
                let courage = self.effective_courage(other).saturating_sub(25);
                if rng.bounded(100) > u16::from(courage) {
                    self.units[other][13] = 3;
                    self.retreat(other);
                }
            }
            let opposite = (self.units[index][14] + 2) & 3;
            if self.units[other][16] == 0
                && self.units[index][13] < 3
                && self.units[other][14] != opposite
            {
                put(&mut self.units[other], 4, 65535);
                self.units[other][6] = (opposite << 6) | 1;
            }
            if self.units[other][10] == 3 {
                // The DOS routine decrements directly, independently of its
                // buffered troop losses (including its 16-bit wrap behavior).
                let count = word(&self.units[other], 0)?.wrapping_sub(1);
                put(&mut self.units[other], 0, count);
                if count == 0 {
                    self.request(other, 4);
                    let side = self.units[other][11];
                    self.general_fallen(side, rng);
                    if side == 0 {
                        let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
                        self.ds[0x1a20 + commander * 6] = 2;
                    }
                }
            } else if self.units[index][10] != 3 {
                surviving[other] = surviving[other].saturating_sub(self.damage(index, other, rng)?);
            }
        }
        Ok(())
    }

    fn damage(&self, attacker: usize, defender: usize, rng: &mut OriginalRng) -> Result<u16> {
        let a = self.units[attacker];
        let d = self.units[defender];
        let at = (usize::from(a[10]) * 3 + usize::from(d[10])) * 4
            + usize::from(a[14].wrapping_sub(d[14]) & 3);
        let mut factor = u16::from(self.ds[0x6af0 + at]);
        match self.ds[0x48e] {
            0 if d[11] == 0 => factor = factor * 3 / 2,
            2 if d[11] == 1 => factor = factor * 5 / 4,
            3 if d[11] == 1 => factor = factor * 3 / 2,
            _ => {}
        }
        // The multiplier is stored in a byte before the division in B3A4.
        let factor = u16::from(factor as u8);
        let cohort = u16::from(self.ds[0x6b14 + usize::from(a[10])]);
        if factor == 0 || cohort == 0 {
            return Err("Invalid original battle damage divisor".into());
        }
        let rolls = word(&a, 0)? / cohort + 1;
        let bound = 64 / factor + 1;
        let mut damage = 0u16;
        for _ in 0..rolls {
            damage = damage.wrapping_add(rng.bounded(bound));
        }
        Ok(if d[10] == 1 { damage } else { damage.max(1) })
    }

    pub(super) fn apply_losses(&mut self, surviving: &[u16], rng: &mut OriginalRng) -> Result<()> {
        for (index, &count) in surviving.iter().enumerate().skip(1) {
            let unit = self.units[index];
            let before = word(&unit, 0)?;
            if before == 0 || unit[10] == 3 {
                continue;
            }
            let loss = before.wrapping_sub(count) as i16;
            let route = if loss.wrapping_shl(4) > before as i16 {
                let courage = unit[12].wrapping_sub((loss / 2) as u8);
                self.units[index][12] = if courage as i8 >= 0 { courage } else { 0 };
                self.units[index][12] < 50
                    && rng.bounded(100) > u16::from(self.effective_courage(index)) * 2
            } else {
                unit[10] == 1 && loss != 0 && rng.bounded(100) > 75
            };
            if route {
                self.units[index][13] = 3;
                self.retreat(index);
            }
            put(&mut self.units[index], 0, count);
            if count == 0 {
                self.request(index, 4);
            }
        }
        Ok(())
    }

    pub(super) fn effective_courage(&self, index: usize) -> u8 {
        let unit = self.units[index];
        let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
        if self.ds[0x1a20 + commander * 6] != 0 || unit[11] != 0 {
            return unit[12];
        }
        let general = self
            .units
            .iter()
            .skip(1)
            .find(|u| u[10] == 3 && u[11] == 0)
            .unwrap();
        let distance = unit[2].abs_diff(general[2]) + unit[3].abs_diff(general[3]);
        unit[12].wrapping_add(self.ds[0x1a1e + commander * 6].saturating_sub(distance))
    }

    fn general_fallen(&mut self, side: u8, rng: &mut OriginalRng) {
        for index in 1..self.units.len() {
            if self.units[index][11] != side || self.units[index][13] == 4 {
                continue;
            }
            self.units[index][12] = self.units[index][12].saturating_sub(15);
            if rng.bounded(100) > u16::from(self.units[index][12]) {
                self.units[index][13] = 3;
                self.retreat(index);
            }
        }
    }

    pub(super) fn retreat(&mut self, index: usize) {
        if self.units[index][13] == 4 {
            return;
        }
        let unit = self.units[index];
        let (direction, distance) = if unit[11] == 1 {
            (3, unit[3])
        } else {
            (1, 44u8.wrapping_sub(unit[3]))
        };
        self.units[index][6] = (direction << 6) | distance;
        if unit[14] != direction {
            put(&mut self.units[index], 4, 65535);
        }
    }

    pub(super) fn panic_elephant(&mut self, index: usize, rng: &mut OriginalRng) {
        if word(&self.units[index], 0).unwrap() == 0 {
            return;
        }
        self.clear(index);
        let old = self.units[index][14];
        let direction = if rng.bounded(3) == 0 {
            rng.bounded(4) as u8
        } else {
            old
        };
        if old != direction {
            put(&mut self.units[index], 4, 65535);
        }
        self.units[index][7] = (direction << 6) | 2;
        self.place_live(index);
    }

    pub(super) fn routing_blocked(&self, index: usize) -> bool {
        let unit = self.units[index];
        let (w, h) = self.footprint(&unit);
        let mut blocked = false;
        for x in usize::from(unit[2])..usize::from(unit[2]) + w {
            for y in usize::from(unit[3])..usize::from(unit[3]) + h {
                let cell = if x < 48 && y < 48 {
                    self.grid[x * 48 + y]
                } else {
                    254
                };
                if cell == 254 {
                    blocked = false;
                } else if cell != 0 && self.units[usize::from(cell)][11] != unit[11] {
                    blocked = true;
                }
            }
        }
        blocked
    }

    pub(super) fn battle_over(&self) -> bool {
        let mut active = [false; 2];
        for unit in self.units.iter().skip(1) {
            if word(unit, 0).unwrap() != 0 && unit[10] != 3 && !matches!(unit[13], 3 | 4) {
                active[usize::from(unit[11])] = true;
            }
        }
        !active[0] || !active[1]
    }
}
