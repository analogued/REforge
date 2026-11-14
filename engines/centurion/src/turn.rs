// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Turn economy primitives (09BA..0D13); event coordination is built separately.
use crate::assets::{Result, word};
use crate::state::Campaign;
fn add_cap(value: u16, amount: u16, maximum: u16) -> u16 {
    (value.wrapping_add(amount) as i16).min(maximum as i16) as u16
}
fn sub_floor(value: u16, amount: u16, minimum: u16) -> u16 {
    (value.wrapping_sub(amount) as i16).max(minimum as i16) as u16
}
impl Campaign {
    fn turn_put(&mut self, at: usize, value: u16) {
        self.data[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    pub fn change_score(&mut self, change: i16) -> Result<()> {
        let score = word(&self.data, 0x594a)?;
        self.turn_put(
            0x594a,
            if change >= 0 {
                add_cap(score, change as u16, 256)
            } else {
                sub_floor(score, change.unsigned_abs(), 0)
            },
        );
        Ok(())
    }
    pub fn turn_recovery(&mut self) -> Result<()> {
        self.data[0x634e] = add_cap(u16::from(self.data[0x634e]), 5, 100) as u8;
        let games = if word(&self.data, 0xed8)? != 0 {
            Some(0)
        } else if word(&self.data, 0xeda)? != 0 {
            Some(self.data[0xed6])
        } else {
            None
        };
        if let Some(province) = games {
            if province >= 27 {
                return Err("Invalid game event province".into());
            }
            let at = 0x527e + usize::from(province) * 32;
            self.data[at] = add_cap(self.data[at] as i8 as i16 as u16, 15, 100) as u8;
        }
        for i in self.legions() {
            let at = 0x1918 + i * 20;
            self.data[at + 14] = 0;
            self.data[at + 11] = add_cap(u16::from(self.data[at + 11]), 2, 99) as u8;
            if self.data[at + 17] == 255 {
                let kind = usize::from(self.data[at + 13]);
                self.turn_put(
                    at + 6,
                    add_cap(
                        word(&self.data, at + 6)?,
                        100,
                        word(&self.data, 0x1aac + kind * 2)?,
                    ),
                );
                if kind != 0 {
                    self.turn_put(
                        at + 8,
                        add_cap(
                            word(&self.data, at + 8)?,
                            10,
                            word(&self.data, 0x1ab2 + kind * 2)?,
                        ),
                    );
                }
            }
        }
        for i in self.fleets() {
            self.data[0x1116 + i * 12] = 0;
        }
        Ok(())
    }
    /// State at the original call to 0D13, before any revolt battle or message.
    pub fn turn_province_economy(&mut self, province: u8) -> Result<bool> {
        let status = self.ownership(province)?;
        let at = usize::from(province) * 32;
        self.data[0x5289 + at] = self.data[0x5289 + at].wrapping_add(1);
        if status == 0 || province == 0 {
            self.turn_put(
                0x5270 + at,
                add_cap(
                    word(&self.data, 0x5270 + at)?,
                    100,
                    word(&self.data, 0x526c + at)?,
                ),
            );
            self.turn_put(
                0x5272 + at,
                add_cap(
                    word(&self.data, 0x5272 + at)?,
                    25,
                    word(&self.data, 0x526e + at)?,
                ),
            );
        }
        if status == 0 {
            return Ok(false);
        }
        let maximum = word(&self.data, 0x5276 + at)?;
        self.turn_put(
            0x5274 + at,
            add_cap(
                word(&self.data, 0x5274 + at)?,
                (maximum / 40).max(50),
                maximum,
            ),
        );
        let wealth = (u16::from(self.data[0x527f + at]) * u16::from(status)) / 4;
        let old = self.data[0x527e + at] as i8 as i16;
        let level = self.data[0x5282 + at];
        let (tax, anger) = match level {
            0 => (0, (old - 10).max(0)),
            1 => (wealth / 4, (old - 5).max(0)),
            2 => (wealth / 2, (old + 5).min(100)),
            3 => (wealth * 3 / 4, (old + 10).min(100)),
            4 => (wealth, (old + 20).min(100)),
            _ => return Err("Invalid province tribute level".into()),
        };
        self.data[0x5281 + at] = tax as u8;
        if self.data[0x5283 + at] < level {
            self.data[0x634e] = self.data[0x634e].saturating_sub(15);
        }
        self.data[0x527e + at] = anger.max(1) as u8;
        self.data[0x5278 + at] = wealth as u8;
        if maximum == 0 {
            return Err("Province has zero recruit capacity".into());
        }
        let ratio = 50u16.wrapping_mul(word(&self.data, 0x5274 + at)?) / maximum;
        let satisfaction =
            (i32::from(ratio) + 50 - (i32::from(tax) * 50) / i32::from(wealth + 1)) as u16;
        let reputation = word(&self.data, 0x594a)?;
        let value = if province == 0 {
            u32::from(self.data[0x634e]) * u32::from(satisfaction) * u32::from(reputation) / 25500
        } else {
            u32::from((u16::from(self.data[0x634e]) * u16::from(status)).wrapping_mul(satisfaction))
                / 400
        };
        self.data[0x5280 + at] = (value as u16 as i16).clamp(1, 99) as u8;
        Ok(
            (self.data[0x527e + at].wrapping_sub(self.data[0x527d + at]) as i8)
                > (self.data[0x5280 + at] as i8),
        )
    }
    pub fn turn_province_finish(&mut self, province: u8) -> Result<u16> {
        let status = self.ownership(province)?;
        let at = usize::from(province) * 32;
        if status == 0 || self.ownership(0)? == 0 {
            return Ok(0);
        }
        let tax = u16::from(self.data[0x5281 + at]);
        if 25u8.wrapping_mul(status) < self.data[0x634e]
            && self.data[0x5289 + at] > 5
            && self.morale(province)? >= 2
        {
            self.data[0x5289 + at] = 0;
            self.data[0x5285 + at] = (status + 1).min(4);
            if status < 4 {
                self.change_score(1)?;
            }
        }
        Ok(tax)
    }
    pub fn turn_finish_year(&mut self, income: u16) -> Result<()> {
        if self.data[0x634e] > 75 {
            self.change_score(1)?;
        }
        let score = word(&self.data, 0x594a)? as i16;
        let rank = (0..8)
            .find(|i| word(&self.data, 0x594e + i * 2).is_ok_and(|n| (n as i16) >= score))
            .unwrap_or(8);
        self.data[0x5971] = self.data[0x5971].max(rank as u8);
        if self.data[0x5971] == 8 && (0..27).any(|p| self.data[0x5285 + p * 32] == 0) {
            self.data[0x5971] = 7;
        }
        self.turn_put(0x634c, word(&self.data, 0x634c)?.wrapping_add(1));
        self.turn_put(0x634a, word(&self.data, 0x634a)?.wrapping_add(income));
        // Resident 0CEB indexes one record beyond the 27-province loop. Preserve
        // its actual accounting byte, rather than silently replacing it by income.
        let total = u32::from_le_bytes(self.data[0xc092..0xc096].try_into().unwrap())
            .wrapping_add(u32::from(self.data[0x5281 + 27 * 32]));
        self.data[0xc092..0xc096].copy_from_slice(&total.to_le_bytes());
        Ok(())
    }
}

/// A revolt stays suspended across each defending legion's real land battle.
/// Losing/canceling disbands that legion; the next defender still gets its turn.
pub struct Revolt {
    pub province: u8,
    next_legion: usize,
    pending: Option<usize>,
    fighting: Option<usize>,
    reported: bool,
    defended: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum RevoltAction {
    Warn(usize),
    Fight(usize),
    Report,
    Done,
}
fn province_score(province: u8) -> i16 {
    if province > 5 {
        5
    } else if province == 2 {
        30
    } else {
        15
    }
}
impl Campaign {
    pub fn retire_legion(&mut self, index: usize) -> Result<()> {
        if index >= 13 {
            return Err("Invalid legion to disband".into());
        }
        let at = 0x1918 + index * 20;
        let commander = usize::from(self.data[at + 16]);
        if commander >= 24 {
            return Err("Invalid legion commander".into());
        }
        self.data[at + 13] = 255;
        self.data[at + 12] = 255;
        if self.data[0x1a20 + commander * 6] != 2 {
            self.data[0x1a20 + commander * 6] = 1;
        }
        Ok(())
    }
    pub fn reset_province_enemy_fleets(&mut self, province: u8) -> Result<()> {
        self.ownership(province)?;
        for (offset, id) in (3..6).enumerate() {
            if self.data[0x1115 + id * 12] != province {
                continue;
            }
            let total = (0..3)
                .map(|k| word(&self.data, 0x110e + id * 12 + k * 2).unwrap_or(0))
                .fold(0u16, u16::wrapping_add);
            let kind = usize::from(word(&self.data, 0x3d90 + offset * 2)?);
            if kind >= 3 {
                return Err("Invalid home fleet ship kind".into());
            }
            let count = word(&self.data, 0x3d96 + offset * 2)?;
            for k in 0..3 {
                self.turn_put(0x110e + id * 12 + k * 2, 0);
            }
            self.turn_put(
                0x110e + id * 12 + kind * 2,
                if total == 0 { count / 5 } else { count },
            );
            self.data[0x1116 + id * 12] = 0;
        }
        Ok(())
    }
}
impl Revolt {
    pub fn begin(state: &mut Campaign, province: u8) -> Result<Self> {
        let status = state.ownership(province)?;
        if status == 0 {
            return Err("No Roman province can revolt here".into());
        }
        let at = usize::from(province) * 32;
        let reputation = (5i16 - i16::from(status))
            .wrapping_mul(i16::from(state.data[0x634e]))
            .wrapping_mul(25)
            / 100;
        state.data[0x634e] = reputation as u8;
        state.change_score(-province_score(province))?;
        state.data[0x5285 + at] = 0;
        state.data[0x5287 + at] = 255;
        let infantry = i16::from(state.data[0x527e + at] as i8)
            .wrapping_mul(20)
            .wrapping_add(word(&state.data, 0x5274 + at)? as i16);
        state.turn_put(0x5270 + at, infantry as u16);
        state.turn_put(0x5272 + at, 0);
        Ok(Self {
            province,
            next_legion: 0,
            pending: None,
            fighting: None,
            reported: false,
            defended: false,
        })
    }
    pub fn next(&mut self, state: &mut Campaign) -> Result<RevoltAction> {
        if let Some(legion) = self.pending.take() {
            self.fighting = Some(legion);
            return Ok(RevoltAction::Fight(legion));
        }
        if self.fighting.is_some() {
            return Err("Revolt still awaits battle result".into());
        }
        if self.reported {
            return Ok(RevoltAction::Done);
        }
        if !self.defended
            && let Some(legion) =
                (self.next_legion..12).find(|i| state.data[0x1924 + i * 20] == self.province)
        {
            self.next_legion = legion + 1;
            self.pending = Some(legion);
            return Ok(RevoltAction::Warn(legion));
        }
        if !self.defended {
            state.reset_province_enemy_fleets(self.province)?;
        }
        self.reported = true;
        Ok(RevoltAction::Report)
    }
    pub fn battle_returned(
        &mut self,
        state: &mut Campaign,
        legion: usize,
        victory: bool,
    ) -> Result<()> {
        if self.fighting != Some(legion) {
            return Err("Unexpected revolt battle result".into());
        }
        self.fighting = None;
        if victory {
            let at = usize::from(self.province) * 32;
            state.data[0x5285 + at] = 1;
            state.data[0x5287 + at] = 0;
            state.change_score(province_score(self.province))?;
            self.defended = true;
        } else {
            state.retire_legion(legion)?;
        }
        Ok(())
    }
    pub fn warning(&self, state: &Campaign, legion: usize) -> Result<String> {
        Ok(format!(
            "{}{}{}{}{}",
            state.string(word(&state.data, 0x1b4)? as usize)?,
            state.province_name(self.province)?,
            state.string(word(&state.data, 0x1b8)? as usize)?,
            state.legion_name(legion)?,
            state.string(word(&state.data, 0x1be)? as usize)?
        ))
    }
    pub fn report(&self, state: &Campaign) -> Result<String> {
        let at = if self.defended { 0x1c6 } else { 0x1c0 };
        Ok(format!(
            "{}{}{}",
            state.string(word(&state.data, at)? as usize)?,
            state.province_name(self.province)?,
            state.string(word(&state.data, at + 4)? as usize)?
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Campaign {
        let mut ds = vec![0; 65536];
        for i in 0..12 {
            ds[0x1924 + i * 20] = 255;
            ds[0x1925 + i * 20] = 255;
        }
        ds[0x5285 + 32] = 3;
        ds[0x527e + 32] = 80;
        ds[0x634e] = 80;
        ds[0x594a..0x594c].copy_from_slice(&100u16.to_le_bytes());
        ds[0x5274 + 32..0x5276 + 32].copy_from_slice(&300u16.to_le_bytes());
        Campaign::test_data(ds)
    }
    #[test]
    fn second_defender_can_save_a_revolt_without_refunding_reputation() {
        let mut state = state();
        for i in [0, 2] {
            state.data[0x1924 + i * 20] = 1;
            state.data[0x1925 + i * 20] = 0;
            state.data[0x1928 + i * 20] = i as u8;
        }
        let mut revolt = Revolt::begin(&mut state, 1).unwrap();
        assert_eq!(
            (
                state.data[0x634e],
                word(&state.data, 0x594a).unwrap(),
                word(&state.data, 0x5270 + 32).unwrap()
            ),
            (40, 85, 1900)
        );
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Warn(0));
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Fight(0));
        revolt.battle_returned(&mut state, 0, false).unwrap();
        assert_eq!(state.data[0x1925], 255);
        assert_eq!(state.data[0x1a20], 1);
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Warn(2));
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Fight(2));
        revolt.battle_returned(&mut state, 2, true).unwrap();
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Report);
        assert_eq!(
            (
                state.data[0x5285 + 32],
                state.data[0x5287 + 32],
                state.data[0x634e],
                word(&state.data, 0x594a).unwrap()
            ),
            (1, 0, 40, 100)
        );
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Done);
    }
    #[test]
    fn undefended_loss_rebuilds_only_the_provinces_home_fleet() {
        let mut state = state();
        state.data[0x1115 + 3 * 12] = 1;
        state.data[0x1115 + 4 * 12] = 2;
        state.data[0x1115 + 5 * 12] = 3;
        state.turn_put(0x3d90, 2);
        state.turn_put(0x3d96, 50);
        let mut revolt = Revolt::begin(&mut state, 1).unwrap();
        assert_eq!(revolt.next(&mut state).unwrap(), RevoltAction::Report);
        assert_eq!(word(&state.data, 0x110e + 3 * 12 + 4).unwrap(), 10);
        assert_eq!(state.data[0x5285 + 32], 0);
    }
}
