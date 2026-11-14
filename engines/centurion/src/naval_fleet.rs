// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fleet losses outside the flagship duel. The original deliberately applies
//! each side's sampled losses to its opponent, with fractional Roman carry.
use crate::assets::{Result, word};
use crate::naval_motion::put;
use crate::rng::OriginalRng;
pub fn counter(ds: &mut [u8], at: usize, delta: u16) {
    let old = u32::from_le_bytes(ds[at..at + 4].try_into().unwrap());
    ds[at..at + 4].copy_from_slice(&old.wrapping_add(u32::from(delta)).to_le_bytes());
}
fn total(ds: &[u8], id: usize) -> Result<u16> {
    Ok((0..3)
        .map(|k| word(ds, 0x110e + id * 12 + k * 2).unwrap_or(0))
        .fold(0u16, u16::wrapping_add))
}
pub fn attrition(
    ds: &mut [u8],
    player: usize,
    enemy: usize,
    rng: &mut OriginalRng,
) -> Result<bool> {
    if player >= 3 || !(3..7).contains(&enemy) {
        return Err("Invalid naval attrition fleets".into());
    }
    ds[0xc400..0xc40c].fill(0);
    let pcargo = usize::from(ds[0x1117 + player * 12]);
    let ecargo = usize::from(ds[0x1117 + enemy * 12]);
    if pcargo != 255 {
        if pcargo >= 12 {
            return Err("Invalid naval legion cargo".into());
        }
        let troops = word(ds, 0x191e + pcargo * 20)?.wrapping_add(word(ds, 0x1920 + pcargo * 20)?);
        put(ds, 0xc408, troops);
        for _ in (0..usize::from(troops)).step_by(500) {
            if rng.bounded(100) <= 5 {
                put(ds, 0xc400, word(ds, 0xc400)?.wrapping_add(1));
            }
        }
    }
    let difficulty = usize::from(ds[0x491]);
    let player_total = total(ds, player)?;
    put(ds, 0xc404, player_total);
    for kind in 0..3 {
        let count = word(ds, 0x110e + player * 12 + kind * 2)?;
        let chance = ds[if ds[0xc38f] == 0 { 0x99c8 } else { 0x99bc } + difficulty * 3 + kind];
        for _ in 0..count {
            if rng.bounded(100) <= u16::from(chance) {
                put(ds, 0xc400, word(ds, 0xc400)?.wrapping_add(1));
            }
        }
    }
    if player_total != 0 {
        for _ in 0..ds[0x99d4 + difficulty] {
            if (rng.bounded(100) as i16) <= i16::from(ds[0xc38f] as i8) / 4 {
                put(ds, 0xc400, word(ds, 0xc400)?.wrapping_add(1));
            }
        }
    }
    if ecargo != 255 {
        if ecargo >= 14 {
            return Err("Invalid enemy naval cargo".into());
        }
        let troops = word(ds, 0xc116 + ecargo * 14)?.wrapping_add(word(ds, 0xc118 + ecargo * 14)?);
        put(ds, 0xc40a, troops);
        for _ in (0..usize::from(troops)).step_by(500) {
            if rng.bounded(100) <= 5 {
                put(ds, 0xc402, word(ds, 0xc402)?.wrapping_add(1));
            }
        }
    }
    let kind = usize::from(ds[0xc37a]);
    let count = word(ds, 0x110e + enemy * 12 + kind * 2)?;
    put(ds, 0xc406, count);
    let chance = ds[if ds[0xc38e] == 0 { 0x99ce } else { 0x99c2 } + kind];
    for _ in 0..count {
        if rng.bounded(100) <= u16::from(chance) {
            put(ds, 0xc402, word(ds, 0xc402)?.wrapping_add(1));
        }
    }
    if count != 0 {
        for _ in 0..4 {
            if (rng.bounded(100) as i16) <= i16::from(ds[0xc38e] as i8) / 4 {
                put(ds, 0xc402, word(ds, 0xc402)?.wrapping_add(1));
            }
        }
    }
    if player_total != 0 {
        roman_losses(ds, player)?;
    }
    if count != 0 {
        enemy_losses(ds, enemy)?;
    }
    let p = total(ds, player)?;
    let e = total(ds, enemy)?;
    put(ds, 0xc404, p);
    put(ds, 0xc406, e);
    Ok(p == 0 || e == 0)
}
fn roman_losses(ds: &mut [u8], id: usize) -> Result<()> {
    let mut lost = 0u16;
    let denominator = word(ds, 0xc404)?;
    let loss = word(ds, 0xc402)?;
    for kind in 0..3 {
        let at = 0x110e + id * 12 + kind * 2;
        let n = word(ds, at)?;
        let carry = 0xc3be + kind * 2;
        let numerator = n.wrapping_mul(loss).wrapping_add(word(ds, carry)?);
        let take = (numerator / denominator).min(n);
        put(ds, carry, numerator % denominator);
        put(ds, at, n - take);
        lost = lost.wrapping_add(take);
    }
    counter(ds, 0xc08e, lost);
    let troops = word(ds, 0xc408)?;
    if lost == 0 || troops == 0 {
        return Ok(());
    }
    let cargo = usize::from(ds[0x1117 + id * 12]);
    let at = 0x191e + cargo * 20;
    let infantry = word(ds, at)?;
    let cavalry = word(ds, at + 2)?;
    let capacity = lost.wrapping_mul(u16::from(ds[0x1118 + id * 12]));
    let cavalry_loss = (capacity.wrapping_mul(cavalry) / troops / 4).min(cavalry);
    put(ds, at + 2, cavalry - cavalry_loss);
    let infantry_loss = capacity.wrapping_sub(cavalry_loss).min(infantry);
    put(ds, at, infantry - infantry_loss);
    counter(ds, 0xc086, infantry_loss.wrapping_add(cavalry_loss));
    Ok(())
}
fn enemy_losses(ds: &mut [u8], id: usize) -> Result<()> {
    let at = 0x110e + id * 12 + usize::from(ds[0xc37a]) * 2;
    let count = word(ds, at)?;
    let lost = word(ds, 0xc400)?.min(count);
    put(ds, at, count - lost);
    counter(ds, 0xc08a, lost);
    let troops = word(ds, 0xc40a)?;
    if lost == 0 || troops == 0 {
        return Ok(());
    }
    let cargo = usize::from(ds[0x1117 + id * 12]);
    let base = 0xc114 + cargo * 14;
    let infantry = word(ds, base + 2)?;
    let cavalry = word(ds, base + 4)?;
    let elephants = u16::from(ds[base + 8]);
    let mut capacity = u16::from(ds[0x1118 + id * 12]).wrapping_mul(lost);
    let elephant_loss = (elephants.wrapping_mul(capacity) / troops / 8).min(elephants);
    ds[base + 8] = (elephants - elephant_loss) as u8;
    capacity = capacity.wrapping_sub(elephant_loss);
    let infantry_loss = (capacity.wrapping_mul(infantry) / troops / 4).min(infantry);
    put(ds, base + 2, infantry - infantry_loss);
    capacity = capacity.wrapping_sub(infantry_loss);
    let cavalry_loss = capacity.min(cavalry);
    put(ds, base + 4, cavalry - cavalry_loss);
    counter(
        ds,
        0xc082,
        elephant_loss
            .wrapping_add(infantry_loss)
            .wrapping_add(cavalry_loss),
    );
    Ok(())
}

impl crate::state::Campaign {
    /// 531F creates the unsaved seventh fleet for a roaming army at sea.
    pub fn prepare_army_fleet(&mut self, army: usize, rng: &mut OriginalRng) -> Result<()> {
        if army >= 11 || word(&self.data, 0xc11a + army * 14)? == 0 {
            return Err("No active naval army".into());
        }
        let kind = usize::from(rng.bounded(3));
        let at = 0xc114 + army * 14;
        let weighted = u16::from(self.data[at + 8])
            .wrapping_mul(2)
            .wrapping_add(word(&self.data, at + 2)?)
            .wrapping_mul(4)
            .wrapping_add(word(&self.data, at + 4)?);
        let capacity = u16::from(self.data[0x8ede + kind]);
        if capacity == 0 {
            return Err("Invalid army ship capacity".into());
        }
        let transports = weighted / capacity;
        let ships = transports.wrapping_add(transports / 8);
        if ships == 0 {
            return Err("Naval army cannot fill a transport".into());
        }
        for k in 0..3 {
            put(
                &mut self.data,
                0x1156 + k * 2,
                if k == kind { ships } else { 0 },
            );
        }
        self.data[0x115c] = self.data[at - 1];
        self.data[0x115e] = 0;
        self.data[0x115f] = army as u8;
        self.data[0x1160] = (weighted / ships) as u8;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fleets_apply_opponents_losses_and_roman_fractional_distribution() {
        let mut ds = vec![0; 65536];
        for id in 0..7 {
            ds[0x1117 + id * 12] = 255;
        }
        for kind in 0..3 {
            put(&mut ds, 0x110e + kind * 2, 2);
            ds[0x99bc + kind] = 100;
        }
        put(&mut ds, 0x110e + 3 * 12, 3);
        ds[0x99c2] = 100;
        ds[0xc38e] = 128;
        ds[0xc38f] = 128;
        let mut rng = OriginalRng {
            first: 0,
            second: 0,
        };
        assert!(attrition(&mut ds, 0, 3, &mut rng).unwrap());
        assert_eq!(
            (word(&ds, 0xc400).unwrap(), word(&ds, 0xc402).unwrap()),
            (6, 3)
        );
        for kind in 0..3 {
            assert_eq!(word(&ds, 0x110e + kind * 2).unwrap(), 1);
        }
        assert_eq!(word(&ds, 0x110e + 3 * 12).unwrap(), 0);
        assert_eq!(
            (word(&ds, 0xc08e).unwrap(), word(&ds, 0xc08a).unwrap()),
            (3, 3)
        );
    }
}
