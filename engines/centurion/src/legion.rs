// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Legion records and original recruitment/reinforcement/upgrade arithmetic.
use crate::assets::{Result, word};
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub const MENU: usize = 0x1c0a;
pub const RAISE_MENU: usize = 0x1c1c;
const BASE: usize = 0x1918;
const STRIDE: usize = 20;

pub struct Recruitment {
    pub message: String,
    pub payment: u16,
    pub success: bool,
}
fn put(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn record(index: usize) -> Result<usize> {
    if index >= 12 {
        return Err("Legion index out of range".into());
    }
    Ok(BASE + index * STRIDE)
}

impl Campaign {
    pub fn legions(&self) -> Vec<usize> {
        (0..12)
            .filter(|i| self.data[BASE + i * STRIDE + 13] != 255)
            .collect()
    }
    pub fn local_legions(&self, province: u8) -> Vec<usize> {
        self.legions()
            .into_iter()
            .filter(|i| self.data[BASE + i * STRIDE + 12] == province)
            .collect()
    }
    pub fn roman(&self, value: u8) -> Result<String> {
        let value = usize::from(value);
        Ok(format!(
            "{}{}{}",
            self.string(word(&self.data, 0x4f82 + value / 100 * 2)? as usize)?,
            self.string(word(&self.data, 0x4f6e + value % 100 / 10 * 2)? as usize)?,
            self.string(word(&self.data, 0x4f5a + value % 10 * 2)? as usize)?
        ))
    }
    pub fn legion_name(&self, index: usize) -> Result<String> {
        let at = record(index)?;
        Ok(format!(
            "{}{}",
            self.roman(self.data[at + 10])?,
            self.string(at)?
        ))
    }
    pub fn recruitment_gate(&self) -> Result<Option<usize>> {
        let count = self.legions().len();
        if count == 12 {
            return Ok(Some(0x1c9c));
        }
        let rank = self.data[0x5971] as usize;
        if rank >= 9 {
            return Err("Invalid commander rank".into());
        }
        Ok((count >= self.data[0x595e + rank] as usize).then_some(0x1cba))
    }
    pub fn begin_recruitment(
        &mut self,
        province: u8,
        kind: u8,
        rng: &mut OriginalRng,
    ) -> Result<Recruitment> {
        let error = if let Some(gate) = self.recruitment_gate()? {
            Some(gate)
        } else if kind == 1 && self.data[0x5971] < 2 {
            Some(0x1d9e)
        } else if kind == 2 && self.data[0x5971] < 6 {
            Some(0x1dd0)
        } else {
            None
        };
        if let Some(at) = error {
            return Ok(Recruitment {
                message: self.string(at)?,
                payment: 0,
                success: false,
            });
        }
        if province >= 27 || kind > 2 {
            return Err("Invalid recruitment selection".into());
        }
        let province_offset = province as usize * 32;
        let recruits = word(&self.data, 0x5274 + province_offset)?;
        let cost = u16::from(self.data[0x1ec8 + kind as usize]);
        let error = if (recruits as i16) < 100 {
            Some(0x1e98)
        } else if cost > word(&self.data, 0x634a)? {
            Some(0x1d6a)
        } else {
            None
        };
        if let Some(at) = error {
            return Ok(Recruitment {
                message: self.string(at)?,
                payment: 0,
                success: false,
            });
        }
        let mut available: Vec<usize> =
            (0..24).filter(|i| self.data[0x1a20 + i * 6] == 1).collect();
        let recycle = available.is_empty();
        if recycle {
            available = (0..24).filter(|i| self.data[0x1a20 + i * 6] != 0).collect();
        }
        if available.is_empty() {
            return Err("Original commander pool is empty".into());
        }
        let prefix = self.string(word(&self.data, 0x1c2e + province as usize * 2)? as usize)?;
        if prefix.len() >= 6 {
            return Err("Legion prefix exceeds its field".into());
        }
        let index = (0..12)
            .find(|i| self.data[BASE + i * STRIDE + 13] == 255)
            .ok_or("No free legion record")?;
        let at = record(index)?;
        let mut infantry = word(&self.data, 0x1aac + kind as usize * 2)?;
        let mut cavalry = word(&self.data, 0x1ab2 + kind as usize * 2)?;
        if i32::from(infantry) + i32::from(cavalry) > i32::from(recruits) {
            cavalry = cavalry.min(recruits);
            infantry = recruits - cavalry;
        }
        self.data[at..at + prefix.len()].copy_from_slice(prefix.as_bytes());
        self.data[at + prefix.len()] = 0;
        put(&mut self.data, at + 6, infantry);
        put(&mut self.data, at + 8, cavalry);
        self.data[at + 11] = self.data[0x527a + province_offset];
        self.data[at + 12] = province;
        self.data[at + 13] = kind;
        self.data[at + 14] = 2;
        self.data[at + 15] = 0;
        self.data[at + 17] = 255;
        self.data[at + 18] = province;
        let serial = self.data[0x5288 + province_offset].wrapping_add(1);
        self.data[0x5288 + province_offset] = if serial > 50 { 1 } else { serial };
        self.data[at + 10] = self.data[0x5288 + province_offset];
        put(
            &mut self.data,
            0x5274 + province_offset,
            recruits - infantry - cavalry,
        );
        if recycle {
            for &commander in &available {
                self.data[0x1a20 + commander * 6] = 1;
            }
        }
        let commander = available[rng.bounded(available.len() as u16) as usize];
        self.data[at + 16] = commander as u8;
        self.data[0x1a20 + commander * 6] = 0;
        let name = self.string(word(&self.data, 0x1a1c + commander * 6)? as usize)?;
        Ok(Recruitment {
            message: format!("{name}{}{}", self.string(0x1f89)?, self.legion_name(index)?),
            payment: cost,
            success: true,
        })
    }
    pub fn finish_recruitment(&mut self, cost: u16) -> Result<()> {
        let money = word(&self.data, 0x634a)?;
        if money < cost {
            return Err("Recruitment payment exceeds treasury".into());
        }
        put(&mut self.data, 0x634a, money - cost);
        Ok(())
    }
    pub fn strengthen_legion(&mut self, province: u8, index: usize) -> Result<Option<String>> {
        let at = record(index)?;
        if province >= 27 || self.data[at + 13] > 2 || self.data[at + 12] != province {
            return Err("Legion is not local".into());
        }
        if self.data[at + 14] != 0 {
            return Ok(Some(self.string(0x1d0e)?));
        }
        let kind = self.data[at + 13] as usize;
        let max_inf = word(&self.data, 0x1aac + kind * 2)?;
        let max_cav = word(&self.data, 0x1ab2 + kind * 2)?;
        let infantry = word(&self.data, at + 6)?;
        let cavalry = word(&self.data, at + 8)?;
        if infantry == max_inf && cavalry == max_cav {
            return Ok(Some(self.string(0x1e74)?));
        }
        let base = province as usize * 32;
        let recruits = word(&self.data, 0x5274 + base)?;
        let money = word(&self.data, 0x634a)?;
        let cost = u32::from(self.data[0x1ec8 + kind]);
        let capacity = u32::from(max_inf) + u32::from(max_cav);
        let budget = if u32::from(money) < cost {
            (u32::from(money) * capacity / cost) as u16
        } else {
            recruits
        };
        let mut remaining = recruits.min(budget);
        let extra_cav = max_cav
            .saturating_sub(cavalry)
            .min(recruits / 4)
            .min(remaining);
        remaining -= extra_cav;
        let extra_inf = max_inf.saturating_sub(infantry).min(remaining);
        let extra = u32::from(extra_inf) + u32::from(extra_cav);
        let old = u32::from(infantry) + u32::from(cavalry);
        if old + extra == 0 {
            return Err("Cannot average an empty legion".into());
        }
        put(&mut self.data, at + 6, infantry + extra_inf);
        put(&mut self.data, at + 8, cavalry + extra_cav);
        put(&mut self.data, 0x5274 + base, recruits - extra as u16);
        self.data[at + 11] = ((u32::from(self.data[at + 11]) * old
            + u32::from(self.data[0x527a + base]) * extra)
            / (old + extra)) as u8;
        put(
            &mut self.data,
            0x634a,
            money - (cost * extra / capacity) as u16,
        );
        if extra != 0 {
            self.data[at + 14] = 2;
        }
        Ok(None)
    }
    pub fn upgrade_legion(&mut self, index: usize) -> Result<String> {
        let at = record(index)?;
        let kind = self.data[at + 13];
        if kind > 2 {
            return Err("Invalid legion type".into());
        }
        let error = if kind == 2 {
            Some(0x1e02)
        } else if kind == 0 && self.data[0x5971] < 2 {
            Some(0x1d9e)
        } else if kind == 1 && self.data[0x5971] < 6 {
            Some(0x1dd0)
        } else {
            None
        };
        if let Some(at) = error {
            return self.string(at);
        }
        let cost = u16::from(self.data[0x1ec9 + kind as usize])
            - u16::from(self.data[0x1ec8 + kind as usize]);
        let money = word(&self.data, 0x634a)?;
        if money < cost {
            return self.string(0x1d6a);
        }
        put(&mut self.data, 0x634a, money - cost);
        self.data[at + 13] += 1;
        Ok(format!(
            "{}{}",
            self.string(at)?,
            self.string(if kind == 0 { 0x1e4c } else { 0x1e26 })?
        ))
    }
}

use crate::assets::Assets;
use crate::graphics::Frame;

fn right(frame: &mut Frame, assets: &Assets, text: &str, x: i32, y: i32) -> Result<()> {
    let widths = word(&assets.data_segment, 0xa05c)? as usize;
    let tracking = i32::from(assets.data_segment[0xa064]);
    let width: i32 = text
        .bytes()
        .filter(|b| (32..128).contains(b))
        .map(|b| i32::from(assets.data_segment[widths + (b - 32) as usize]) + tracking)
        .sum();
    frame.text(assets, text, x - width, y, 0)
}
pub(crate) fn row(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    index: usize,
    y: i32,
    local: bool,
) -> Result<()> {
    let at = record(index)?;
    let x = if local { 35 } else { 25 };
    right(frame, assets, &state.roman(state.data[at + 10])?, x, y)?;
    frame.text(assets, &state.string(at)?, x + 2, y, 0)?;
    frame.text(
        assets,
        &state.string(word(&state.data, 0x1c64 + state.data[at + 13] as usize * 2)? as usize)?,
        77,
        y,
        0,
    )?;
    right(
        frame,
        assets,
        &word(&state.data, at + 6)?.to_string(),
        144,
        y,
    )?;
    right(
        frame,
        assets,
        &word(&state.data, at + 8)?.to_string(),
        188,
        y,
    )?;
    let label = if local {
        state.string(word(&state.data, 0x7486 + state.data[at + 11] as usize / 25 * 2)? as usize)?
    } else if state.data[at + 17] == 255 {
        state.province_name(state.data[at + 12])?
    } else {
        state.string(word(&state.data, 0x1100 + state.data[at + 17] as usize * 2)? as usize)?
    };
    frame.center_text(assets, &label, 236, y, 0)?;
    frame.center_text(
        assets,
        &state.string(word(&state.data, 0x1b54 + state.data[at + 14] as usize * 2)? as usize)?,
        292,
        y,
        0,
    )
}
pub fn local_list(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    selected: Option<usize>,
    province: u8,
) -> Result<()> {
    let ids = state.local_legions(province);
    let count = ids.len().min(3);
    if count == 0 {
        return Ok(());
    }
    frame.bevel(2, 0, 317, 15 + (count as i32 + 1) * 10 + 7, 26);
    let title =
        if selected.is_some() && ids.iter().any(|&i| state.data[BASE + i * STRIDE + 14] == 0) {
            state.string(0x1f5e)?
        } else {
            state.string(0x1d58)?
        };
    frame.center_text(assets, &title, 160, 5, 15)?;
    frame.text(assets, &state.string(0x1efa)?, 115, 15, 0)?;
    for (number, &index) in ids.iter().take(3).enumerate() {
        row(frame, assets, state, index, 25 + number as i32 * 10, true)?;
        if let Some(selected) = selected {
            let button = assets
                .sprites("MINIT", "INIT")?
                .into_iter()
                .nth(1)
                .ok_or("Missing selection button")?;
            let y = 25 + number as i32 * 10;
            frame.blit(&button, 8, y);
            if selected == number {
                frame.rect(10, y + 2, 5, 3, 8);
            }
        }
    }
    Ok(())
}
pub fn complete_list(frame: &mut Frame, assets: &Assets, state: &Campaign) -> Result<()> {
    let ids = state.legions();
    if ids.is_empty() {
        return Ok(());
    }
    frame.acknowledgment_popup = true;
    let height = (ids.len() as i32 + 3) * 8 + 15;
    let top = (200 - height) / 2;
    frame.bevel(2, top, 317, top + height, 26);
    frame.center_text(
        assets,
        &format!("{}{}", state.string(0x1f70)?, state.continue_footer()?),
        160,
        top + 5,
        15,
    )?;
    frame.text(assets, &state.string(0x1ecc)?, 115, top + 22, 0)?;
    for (number, &index) in ids.iter().enumerate() {
        row(
            frame,
            assets,
            state,
            index,
            top + 32 + number as i32 * 8,
            false,
        )?;
    }
    Ok(())
}
pub fn message(frame: &mut Frame, assets: &Assets, state: &Campaign, text: &str) -> Result<()> {
    message_with_footer_offset(frame, assets, state, text, 0)
}
pub fn message_with_footer_offset(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    text: &str,
    footer_offset: i32,
) -> Result<()> {
    let footer = state.continue_footer()?;
    message_with_footer(frame, assets, text, &footer, footer_offset)
}
pub fn message_with_footer(
    frame: &mut Frame,
    assets: &Assets,
    text: &str,
    footer: &str,
    footer_offset: i32,
) -> Result<()> {
    frame.acknowledgment_popup = true;
    let widths = word(&assets.data_segment, 0xa05c)? as usize;
    let width = text
        .split('\n')
        .chain(std::iter::once(footer))
        .map(|line| {
            line.bytes()
                .filter(|b| (32..128).contains(b))
                .map(|b| i32::from(assets.data_segment[widths + (b - 32) as usize]) + 1)
                .sum::<i32>()
        })
        .max()
        .unwrap_or(0);
    let lines = if text.is_empty() {
        0
    } else {
        text.split('\n').count()
    };
    let half_height = (lines as i32 + 1) * 11 / 2;
    frame.bevel(
        159 - width / 2 - 7,
        99 - half_height - 7,
        159 + width / 2 + 7,
        99 + half_height + 7,
        26,
    );
    frame.center_lines(assets, text, 159, 99 - half_height, 0)?;
    frame.center_text(
        assets,
        footer,
        159,
        99 + half_height - 8 + footer_offset,
        15,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Campaign {
        let mut data = vec![0; 65536];
        for index in 0..12 {
            data[BASE + index * STRIDE + 13] = 255;
        }
        data[BASE + 13] = 0;
        data[0x5971] = 1;
        data[0x595e..0x5967].copy_from_slice(&[1, 2, 3, 4, 6, 8, 10, 12, 12]);
        for (index, (infantry, cavalry)) in [(4200, 0), (4200, 300), (6000, 600)]
            .into_iter()
            .enumerate()
        {
            put(&mut data, 0x1aac + index * 2, infantry);
            put(&mut data, 0x1ab2 + index * 2, cavalry);
        }
        data[0x1ec8..0x1ecb].copy_from_slice(&[20, 30, 60]);
        put(&mut data, 0x634a, 20);
        put(&mut data, 0x5274, 1000);
        data[0x5285] = 4;
        data[0x527a] = 75;
        data[0x5288] = 50;
        put(&mut data, 0x1c2e, 0x7000);
        data[0x7000..0x7006].copy_from_slice(b" Test\0");
        for index in 0..2 {
            put(&mut data, 0x1a1c + index * 6, 0x7010 + index as u16 * 8);
            data[0x7010 + index * 8] = b'A' + index as u8;
            data[0x1a20 + index * 6] = 1;
        }
        put(&mut data, 0x4f5c, 0x7030);
        data[0x7030..0x7032].copy_from_slice(b"I\0");
        Campaign::test_data(data)
    }

    #[test]
    fn recruitment_uses_available_recruits_wraps_serial_and_defers_payment() {
        let mut state = fixture();
        let mut rng = OriginalRng {
            first: 0,
            second: 0,
        };
        let result = state.begin_recruitment(0, 0, &mut rng).unwrap();
        assert!(result.success);
        assert_eq!(result.payment, 20);
        assert_eq!(state.legions(), vec![0, 1]);
        let at = BASE + STRIDE;
        assert_eq!(word(&state.data, at + 6).unwrap(), 1000);
        assert_eq!(word(&state.data, at + 8).unwrap(), 0);
        assert_eq!(state.data[at + 10], 1);
        assert_eq!(state.data[at + 14], 2);
        assert_eq!(word(&state.data, 0x5274).unwrap(), 0);
        assert_eq!(word(&state.data, 0x634a).unwrap(), 20);
        assert_eq!((rng.first, rng.second), (0x1b0f, 0x7abd));
        state.finish_recruitment(result.payment).unwrap();
        assert_eq!(word(&state.data, 0x634a).unwrap(), 0);
    }

    #[test]
    fn rejected_recruitment_does_not_mutate_campaign_or_rng() {
        for (address, value, size, kind) in [
            (0x5971, 0, 1, 0),
            (0x634a, 0, 2, 0),
            (0x5274, 99, 2, 0),
            (0x5971, 1, 1, 1),
            (0x5971, 2, 1, 2),
        ] {
            let mut state = fixture();
            state.data[address..address + size].copy_from_slice(&u16::to_le_bytes(value)[..size]);
            let before = state.data.clone();
            let mut rng = OriginalRng {
                first: 0,
                second: 0,
            };
            assert!(!state.begin_recruitment(0, kind, &mut rng).unwrap().success);
            assert_eq!(state.data, before);
            assert_eq!((rng.first, rng.second), (0, 0));
        }
    }

    #[test]
    fn reinforcement_prioritizes_cavalry_limits_spending_and_averages_courage() {
        let mut state = fixture();
        state.data[BASE + 13] = 1;
        state.data[BASE + 11] = 50;
        put(&mut state.data, BASE + 6, 2000);
        put(&mut state.data, 0x634a, 5);
        assert!(state.strengthen_legion(0, 0).unwrap().is_none());
        assert_eq!(word(&state.data, BASE + 8).unwrap(), 250); // quarter of recruits
        assert_eq!(word(&state.data, BASE + 6).unwrap(), 2500);
        assert_eq!(word(&state.data, 0x5274).unwrap(), 250);
        assert_eq!(word(&state.data, 0x634a).unwrap(), 0);
        assert_eq!(state.data[BASE + 11], 56);
        assert_eq!(state.data[BASE + 14], 2);
        let unchanged = state.data.clone();
        assert!(state.strengthen_legion(0, 0).unwrap().is_some());
        assert_eq!(state.data, unchanged);
    }

    #[test]
    fn upgrade_charges_only_type_cost_difference_without_adding_soldiers() {
        let mut state = fixture();
        state.data[0x5971] = 2;
        put(&mut state.data, BASE + 6, 2000);
        state.upgrade_legion(0).unwrap();
        assert_eq!(state.data[BASE + 13], 1);
        assert_eq!(word(&state.data, 0x634a).unwrap(), 10);
        assert_eq!(word(&state.data, BASE + 6).unwrap(), 2000);
        assert_eq!(word(&state.data, BASE + 8).unwrap(), 0);
    }
}
