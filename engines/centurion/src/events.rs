// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Resident 1E98..22CA: yearly unrest and the nineteen resource-defined events.
use crate::assets::{Result, word};
use crate::rng::OriginalRng;
use crate::state::Campaign;
fn put(s: &mut Campaign, at: usize, value: u16) {
    s.data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn text(s: &Campaign, at: usize) -> Result<String> {
    s.string(usize::from(word(&s.data, at)?))
}
/// Unrest is evaluated before random events, and suppresses them while pending.
pub fn unrest(s: &mut Campaign, rng: &mut OriginalRng) -> Result<Option<String>> {
    if word(&s.data, 0xeda)? == 0 && (rng.bounded(100) as i8) < s.data[0xed4] as i8 {
        put(s, 0xed8, 65535);
    }
    if word(&s.data, 0xed8)? == 0 && (rng.bounded(100) as i8) < s.data[0xed5] as i8 {
        put(s, 0xeda, 65535);
    }
    for at in [0xed4, 0xed5] {
        s.data[at] = (i16::from(s.data[at] as i8) + 5).min(100) as u8;
    }
    if word(&s.data, 0xed8)? != 0 {
        return if s.ownership(0)? == 4 {
            Ok(Some(s.string(0xf6e)?))
        } else {
            Ok(None)
        };
    }
    if word(&s.data, 0xeda)? == 0 {
        return Ok(None);
    }
    if s.data[0xed6] == 0 {
        let owned = (1u8..27)
            .filter(|p| s.data[0x5285 + usize::from(*p) * 32] != 0)
            .collect::<Vec<_>>();
        if !owned.is_empty() {
            s.data[0xed6] = owned[usize::from(rng.bounded(owned.len() as u16))];
        }
    }
    if s.data[0xed6] == 0 {
        put(s, 0xeda, 0);
        Ok(None)
    } else {
        Ok(Some(format!(
            "{}{}{}",
            s.string(0xfb2)?,
            s.province_name(s.data[0xed6])?,
            s.string(0xfc3)?
        )))
    }
}
fn storm(s: &mut Campaign, rng: &mut OriginalRng, event: usize) -> Result<Option<String>> {
    let percent = rng.bounded(10) + if event == 0 { 10 } else { 0 };
    let mut losses = 0u16;
    for fleet in 0..3 {
        if s.data[0x1116 + fleet * 12] == 255 || s.data[0x1114 + fleet * 12] >= 34 {
            continue;
        }
        let mut count = 0u16;
        let mut capacity = 0u32;
        for kind in 0..3 {
            let at = 0x110e + fleet * 12 + kind * 2;
            let old = word(&s.data, at)?;
            if old == 0 {
                continue;
            }
            let mut lost = old.wrapping_mul(percent) / 100;
            if lost == 1 {
                lost = 2;
            }
            losses = losses.wrapping_add(lost);
            let remaining = old.wrapping_sub(lost);
            put(s, at, remaining);
            count = count.wrapping_add(remaining);
            capacity += u32::from(remaining) * u32::from(s.data[0x8ede + kind]);
        }
        let cargo = s.data[0x1117 + fleet * 12];
        if cargo != 255 {
            if cargo >= 12 || count == 0 {
                return Err("Invalid storm transport capacity".into());
            }
            let at = 0x1918 + usize::from(cargo) * 20;
            let capacity = capacity.min(10000) as u16;
            let cav = word(&s.data, at + 8)?.min(capacity / 4);
            let inf = word(&s.data, at + 6)?.min(capacity - cav * 4);
            put(s, at + 8, cav);
            put(s, at + 6, inf);
            s.data[0x1118 + fleet * 12] = (inf.wrapping_add(cav.wrapping_mul(4)) / count) as u8;
        }
    }
    if losses == 0 {
        return Ok(None);
    }
    Ok(Some(format!(
        "{}{}{}{}",
        text(s, 0xf3c)?,
        text(s, if event == 0 { 0xf40 } else { 0xf42 })?,
        losses,
        text(s, 0xf3e)?
    )))
}
pub fn random_event(
    s: &mut Campaign,
    rng: &mut OriginalRng,
    income: u16,
) -> Result<Option<String>> {
    if word(&s.data, 0xed8)? != 0 || word(&s.data, 0xeda)? != 0 || rng.bounded(100) >= 20 {
        return Ok(None);
    }
    let event = loop {
        let id = usize::from(rng.bounded(19));
        let flag = s.data[0xf02 + id * 3];
        if flag == 1 || (flag == 0 && word(&s.data, 0xedc + id * 2)? == 0) {
            break id;
        }
    };
    put(s, 0xedc + event * 2, 65535);
    let message = match event {
        0 | 1 => storm(s, rng, event)?,
        2 => Some(format!(
            "{}{}{}",
            text(s, 0xf48 + event * 2)?,
            text(s, 0x5974 + usize::from(s.data[0x5971]) * 2)?,
            s.string(0xed1)?
        )),
        3 => {
            if s.legions().is_empty() {
                None
            } else {
                // 1FDE pushes zero, so this draw always selects slot zero,
                // including when another slot is the only active legion.
                let id = usize::from(rng.bounded(0));
                let loc = s.data[0x1924 + id * 20];
                for i in 0..12 {
                    if s.data[0x1924 + i * 20] == loc {
                        let at = 0x191e + i * 20;
                        let inf = word(&s.data, at)?;
                        put(s, at, inf - inf / 10);
                    }
                }
                Some(format!(
                    "{}{}{}",
                    text(s, 0xf44)?,
                    text(s, 0x5222 + usize::from(loc) * 2)?,
                    text(s, 0xf46)?
                ))
            }
        }
        _ => Some(text(s, 0xf48 + event * 2)?),
    };
    if message.is_none() {
        return Ok(None);
    }
    s.change_score(i16::from(s.data[0xf03 + event * 3] as i8))?;
    let amount = s.data[0xf04 + event * 3] as i8;
    let gold = word(&s.data, 0x634a)?;
    let gold = match amount {
        -128 => gold.wrapping_sub(income >> 1),
        127 => gold.wrapping_sub(income),
        n if n < 0 => gold.saturating_sub(u16::from(n.unsigned_abs())),
        n => gold.wrapping_add(n as u16),
    };
    put(s, 0x634a, gold);
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_foreign_unrest_uses_no_new_draw_and_survives_lost_province() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        put(&mut s, 0xeda, 65535);
        s.data[0xed6] = 2;
        s.data[0xed4] = 200;
        s.data[0xed5] = 100;
        s.data[0x5226..0x5228].copy_from_slice(&0x500u16.to_le_bytes());
        s.data[0x500] = b'X';
        let mut rng = OriginalRng {
            first: 1,
            second: 2,
        };
        // Rome roll skipped; provincial roll still occurs because Rome is quiet.
        unrest(&mut s, &mut rng).unwrap();
        let mut expected = OriginalRng {
            first: 1,
            second: 2,
        };
        expected.bounded(100);
        assert_eq!((rng.first, rng.second), (expected.first, expected.second));
        assert_eq!(s.data[0xed4], 205);
        assert_eq!(s.data[0xed6], 2);
    }
    #[test]
    fn plague_uses_slot_zero_location_even_when_only_another_legion_is_active() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        for i in 0..12 {
            s.data[0x1925 + i * 20] = 255;
            s.data[0x1924 + i * 20] = 255;
        }
        s.data[0x1925 + 20] = 2;
        s.data[0x1924 + 20] = 5;
        put(&mut s, 0x191e, 1000);
        put(&mut s, 0x191e + 20, 2000);
        let seed = (0..=u16::MAX)
            .find(|n| {
                let mut r = OriginalRng {
                    first: *n,
                    second: 123,
                };
                r.bounded(100) < 20 && r.bounded(19) == 3
            })
            .unwrap();
        let mut rng = OriginalRng {
            first: seed,
            second: 123,
        };
        assert!(random_event(&mut s, &mut rng, 0).unwrap().is_some());
        assert_eq!(word(&s.data, 0x191e).unwrap(), 900);
        assert_eq!(word(&s.data, 0x191e + 20).unwrap(), 2000);
        let mut expected = OriginalRng {
            first: seed,
            second: 123,
        };
        expected.bounded(100);
        expected.bounded(19);
        expected.bounded(0);
        assert_eq!((rng.first, rng.second), (expected.first, expected.second));
    }
}
