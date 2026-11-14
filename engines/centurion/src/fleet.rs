// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Fleet construction and legion transport, recovered from resident 2458/3AE4.
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

pub const MENU: usize = 0x11dc;
const BASE: usize = 0x110e;
fn put(ds: &mut [u8], at: usize, value: u16) {
    ds[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

pub enum BuildSite {
    Available(usize),
    Unavailable(usize),
}
impl Campaign {
    pub fn shipbuilding_site(&self, province: u8) -> Result<BuildSite> {
        if province >= 27 || self.ownership(province)? == 0 {
            return Err("No owned shipbuilding province".into());
        }
        let sea = self.data[0x50ec + usize::from(province) * 8];
        if !(27..37).contains(&sea) {
            return Ok(BuildSite::Unavailable(0x1274));
        }
        if self.data[0x5971] >= 9 {
            return Err("Invalid fleet rank".into());
        }
        let limit = *self
            .data
            .get(0x5968 + usize::from(self.data[0x5971]))
            .ok_or("Invalid fleet rank")?;
        if limit == 0 {
            return Ok(BuildSite::Unavailable(0x1218));
        }
        if (3..6)
            .any(|i| self.data[BASE + i * 12 + 6] == sea && self.data[BASE + i * 12 + 8] != 255)
        {
            return Ok(BuildSite::Unavailable(0x128c));
        }
        let active: Vec<_> = (0..3)
            .filter(|i| self.data[BASE + i * 12 + 8] != 255)
            .collect();
        if let Some(&i) = active
            .iter()
            .find(|&&i| self.data[BASE + i * 12 + 6] == sea)
        {
            return Ok(BuildSite::Available(i));
        }
        if active.len() == 3 {
            return Ok(BuildSite::Unavailable(0x12b6));
        }
        if active.len() >= usize::from(limit) {
            return Ok(BuildSite::Unavailable(0x1244));
        }
        Ok(BuildSite::Available(
            (0..3)
                .find(|i| self.data[BASE + i * 12 + 8] == 255)
                .unwrap(),
        ))
    }
}

pub struct Purchase {
    pub kind: usize,
    pub count: u16,
    pub talents: u16,
}
pub struct Construction {
    pub province: u8,
    pub slot: usize,
    initial: [u16; 3],
    pub purchase: Option<Purchase>,
    background: Frame,
}
impl Construction {
    pub fn new(state: &Campaign, province: u8, slot: usize, background: Frame) -> Result<Self> {
        let initial = [
            word(&state.data, BASE + slot * 12)?,
            word(&state.data, BASE + slot * 12 + 2)?,
            word(&state.data, BASE + slot * 12 + 4)?,
        ];
        Ok(Self {
            province,
            slot,
            initial,
            purchase: None,
            background,
        })
    }
    pub fn select(&mut self, state: &Campaign, kind: usize) -> Result<()> {
        if kind >= 3 {
            return Err("Invalid ship type".into());
        }
        self.purchase = Some(Purchase {
            kind,
            count: word(&state.data, BASE + self.slot * 12 + kind * 2)?,
            talents: word(&state.data, 0x634a)?,
        });
        Ok(())
    }
    pub fn adjust(&mut self, state: &Campaign, increase: bool) {
        let Some(p) = self.purchase.as_mut() else {
            return;
        };
        let cost = u16::from(state.data[0x11fb + p.kind * 14]);
        if increase && p.count < 999 && p.talents >= cost {
            p.count += 1;
            p.talents -= cost;
        } else if !increase && p.count > self.initial[p.kind] {
            p.count -= 1;
            p.talents += cost;
        }
    }
    pub fn finish_purchase(&mut self, state: &mut Campaign, accept: bool) -> Result<()> {
        let p = self.purchase.take().ok_or("No ship purchase")?;
        if accept {
            // Each editor starts from the current count, and cannot go below the
            // construction session's initial count. Previously bought ships refund.
            put(&mut state.data, BASE + self.slot * 12 + p.kind * 2, p.count);
            put(&mut state.data, 0x634a, p.talents);
        }
        Ok(())
    }
    pub fn finish(&self, state: &mut Campaign) -> Result<()> {
        let count = (0..3)
            .map(|kind| word(&state.data, BASE + self.slot * 12 + kind * 2))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(u32::from)
            .sum::<u32>();
        let at = BASE + self.slot * 12;
        if state.data[at + 8] == 255 && count != 0 {
            state.data[at + 8] = 2;
            state.data[at + 9] = 255;
            state.data[at + 7] = self.province;
            state.data[at + 6] = state.data[0x50ec + usize::from(self.province) * 8];
        } else if count > self.initial.iter().map(|&n| u32::from(n)).sum() {
            state.data[at + 8] = 2;
        }
        Ok(())
    }
    pub fn render(
        &self,
        assets: &Assets,
        state: &Campaign,
        panels: &crate::province::Panels,
        selected: Option<usize>,
    ) -> Result<Frame> {
        let mut frame = self.background.clone();
        panels.menu(assets, state, &mut frame, MENU, selected)?;
        // 2A32 formats the live count strings, replacing their original space padding.
        for kind in 0..3 {
            let count = word(&state.data, BASE + self.slot * 12 + kind * 2)?;
            frame.rect(23, 99 + kind as i32 * 28, 125, 22, 26);
            let label = if kind == 1 {
                "Quinqueremes".to_owned()
            } else {
                state.string(0x11ee + kind * 14)?
            };
            let color = if selected == Some(kind) { 15 } else { 0 };
            frame.text(
                assets,
                &format!("{label}:\n {count} ships"),
                23,
                99 + kind as i32 * 28,
                color,
            )?;
        }
        if self.purchase.is_some() {
            panels.deactivate_menu(assets, state, &mut frame, MENU)?;
        }
        let sprites = assets.sprites("MINIT", "INIT")?;
        for (sprite, x, y) in [(9, 90, 96), (10, 82, 121), (11, 65, 150)] {
            frame.blit(&sprites[sprite], x, y);
        }
        frame.center_text(
            assets,
            &state.string(word(&state.data, 0x1100 + self.slot * 2)? as usize)?,
            109,
            183,
            0,
        )?;
        // The purchase routine updates the treasury immediately after closing.
        frame.rect(211, 167, 55, 8, 26);
        frame.text(assets, &word(&state.data, 0x634a)?.to_string(), 211, 167, 0)?;
        if let Some(p) = &self.purchase {
            frame.bevel(84, 75, 234, 131, 26);
            frame.blit(&sprites[12], 90, 81);
            frame.blit(&sprites[13], 90, 91);
            frame.rect(101, 84, 19, 12, 15);
            let number = (p.count - self.initial[p.kind]).to_string();
            let widths = word(&assets.data_segment, 0xa05c)? as usize;
            let width: i32 = number
                .bytes()
                .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                .sum();
            frame.text(assets, &number, 119 - width, 85, 0)?;
            frame.text(assets, &state.string(0x11ee + p.kind * 14)?, 124, 85, 0)?;
            frame.text(assets, &state.string(0x12da)?, 190, 85, 0)?;
            frame.text(
                assets,
                &state.data[0x11fb + p.kind * 14].to_string(),
                214,
                85,
                0,
            )?;
            frame.text(assets, &state.string(0x12e0)?, 114, 116, 0)?;
            frame.text(assets, &p.talents.to_string(), 194, 116, 0)?;
            frame.text(assets, &state.string(0x12f3)?, 90, 105, 0)?;
        }
        Ok(frame)
    }
}

pub struct Boarding {
    pub legion: usize,
    pub fleet: usize,
    pub source: u8,
    pub destination: u8,
    pub infantry: u16,
    pub cavalry: u16,
    pub left: u16,
}
impl Boarding {
    pub fn prepare(state: &Campaign, legion: usize, destination: u8) -> Result<Self> {
        let source = state.legion_location(legion)?;
        if source >= 27
            || destination < 27
            || !state
                .legion_destinations(legion)?
                .iter()
                .any(|&(p, sea)| sea && p == destination)
        {
            return Err("No embarkation destination".into());
        }
        let fleet = (0..3)
            .find(|i| {
                let at = BASE + i * 12;
                state.data[at + 6] == destination
                    && state.data[at + 8] != 255
                    && state.data[at + 9] == 255
            })
            .ok_or("No empty transport fleet")?;
        let capacity = (0..3)
            .map(|kind| {
                Ok(u32::from(word(&state.data, BASE + fleet * 12 + kind * 2)?)
                    * u32::from(state.data[0x8ede + kind]))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .sum::<u32>()
            .min(10000) as u16;
        let at = 0x1918 + legion * 20;
        let old_infantry = word(&state.data, at + 6)?;
        let old_cavalry = word(&state.data, at + 8)?;
        let cavalry = old_cavalry.min(capacity / 4);
        let infantry = old_infantry.min(capacity - cavalry * 4);
        Ok(Self {
            legion,
            fleet,
            source,
            destination,
            infantry,
            cavalry,
            left: old_infantry - infantry + old_cavalry - cavalry,
        })
    }
    pub fn apply(&self, state: &mut Campaign) -> Result<()> {
        let at = 0x1918 + self.legion * 20;
        let fleet = BASE + self.fleet * 12;
        if state.legion_location(self.legion)? != self.source
            || state.data[at + 14] != 0
            || state.data[fleet + 9] != 255
        {
            return Err("Boarding state changed".into());
        }
        let ships = (0..3)
            .map(|kind| word(&state.data, fleet + kind * 2))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(u32::from)
            .sum::<u32>();
        if ships == 0 {
            return Err("Transport fleet has no ships".into());
        }
        put(&mut state.data, at + 6, self.infantry);
        put(&mut state.data, at + 8, self.cavalry);
        state.data[fleet + 10] =
            ((u32::from(self.cavalry) * 4 + u32::from(self.infantry)) / ships) as u8;
        let province = usize::from(self.source) * 32;
        let recruits = word(&state.data, 0x5274 + province)?.wrapping_add(self.left);
        let maximum = word(&state.data, 0x5276 + province)?;
        put(
            &mut state.data,
            0x5274 + province,
            (recruits as i16).min(maximum as i16) as u16,
        );
        state.data[fleet + 9] = self.legion as u8;
        state.data[at + 17] = self.fleet as u8;
        state.data[at + 14] = 1;
        state.data[at + 12] = self.destination;
        Ok(())
    }
    pub fn question(&self, state: &Campaign) -> Result<String> {
        Ok(format!("{}{}", self.left, state.string(0x1f28)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Campaign {
        let mut data = vec![0; 65536];
        for i in 0..6 {
            data[BASE + i * 12 + 8] = 255;
            data[BASE + i * 12 + 9] = 255;
        }
        data[0x5285] = 4;
        data[0x50ec] = 32;
        data[0x5971] = 2;
        data[0x596a] = 1;
        data[0x11fb] = 5;
        data[0x1209] = 10;
        data[0x1217] = 20;
        data[0x8ede..0x8ee1].copy_from_slice(&[50, 100, 200]);
        data[0x1929] = 255;
        put(&mut data, 0x634a, 200);
        put(&mut data, 0x5274, 5000);
        put(&mut data, 0x5276, 7000);
        put(&mut data, 0x191e, 4200);
        put(&mut data, 0x1920, 300);
        Campaign::test_data(data)
    }
    #[test]
    fn shipbuilding_limits_allow_existing_fleet_but_enemy_blockade_wins() {
        let mut state = state();
        assert!(matches!(
            state.shipbuilding_site(0).unwrap(),
            BuildSite::Available(0)
        ));
        state.data[BASE + 8] = 0;
        state.data[BASE + 6] = 31;
        assert!(matches!(
            state.shipbuilding_site(0).unwrap(),
            BuildSite::Unavailable(0x1244)
        ));
        state.data[BASE + 6] = 32;
        assert!(matches!(
            state.shipbuilding_site(0).unwrap(),
            BuildSite::Available(0)
        ));
        state.data[BASE + 3 * 12 + 8] = 0;
        state.data[BASE + 3 * 12 + 6] = 32;
        assert!(matches!(
            state.shipbuilding_site(0).unwrap(),
            BuildSite::Unavailable(0x128c)
        ));
    }
    #[test]
    fn purchases_defer_payment_allow_refunds_and_activate_only_on_exit() {
        let mut state = state();
        let mut scene = Construction::new(&state, 0, 0, Frame::new()).unwrap();
        let before = state.data.clone();
        scene.select(&state, 0).unwrap();
        for _ in 0..3 {
            scene.adjust(&state, true);
        }
        assert_eq!(state.data, before);
        scene.finish_purchase(&mut state, false).unwrap();
        assert_eq!(state.data, before);
        scene.select(&state, 0).unwrap();
        for _ in 0..50 {
            scene.adjust(&state, true);
        }
        assert_eq!(scene.purchase.as_ref().unwrap().count, 40);
        scene.finish_purchase(&mut state, true).unwrap();
        assert_eq!(word(&state.data, 0x634a).unwrap(), 0);
        assert_eq!(state.data[BASE + 8], 255);
        scene.select(&state, 0).unwrap();
        scene.adjust(&state, false);
        scene.finish_purchase(&mut state, true).unwrap();
        assert_eq!(word(&state.data, 0x634a).unwrap(), 5);
        scene.finish(&mut state).unwrap();
        assert_eq!(&state.data[BASE + 6..BASE + 10], &[32, 0, 2, 255]);
    }
    #[test]
    fn boarding_prioritizes_cavalry_caps_capacity_and_returns_people_to_recruits() {
        let mut state = state();
        state.data[BASE + 6] = 32;
        state.data[BASE + 8] = 0;
        put(&mut state.data, BASE, 10);
        let plan = Boarding::prepare(&state, 0, 32).unwrap();
        assert_eq!((plan.infantry, plan.cavalry, plan.left), (0, 125, 4375));
        plan.apply(&mut state).unwrap();
        assert_eq!(word(&state.data, 0x5274).unwrap(), 7000);
        assert_eq!(&state.data[BASE + 8..BASE + 11], &[0, 0, 50]);
        assert_eq!(state.data[0x1924], 32);
        assert_eq!(state.data[0x1926], 1);
        assert_eq!(state.data[0x1929], 0);
        state.data[0x1924] = 0;
        state.data[0x1926] = 0;
        state.data[BASE + 9] = 255;
        put(&mut state.data, BASE, 999);
        put(&mut state.data, 0x191e, 10000);
        put(&mut state.data, 0x1920, 3000);
        let plan = Boarding::prepare(&state, 0, 32).unwrap();
        assert_eq!((plan.infantry, plan.cavalry, plan.left), (0, 2500, 10500));
    }
}
