// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Resident 4C24..541E: roaming enemies, spawning and defensive continuations.
use crate::assets::{Result, word};
use crate::rng::OriginalRng;
use crate::state::Campaign;

fn put(s: &mut Campaign, at: usize, value: u16) {
    s.data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn active(s: &Campaign, army: usize) -> Result<bool> {
    Ok(word(&s.data, 0xc11a + army * 14)? != 0)
}
fn distance(s: &Campaign, region: u8) -> Result<u8> {
    if region >= 37 {
        return Err("Invalid enemy region".into());
    }
    Ok(s.data[if region < 27 {
        0x528a + usize::from(region) * 32
    } else {
        0x3d55 + usize::from(region)
    }])
}
fn neighbours(s: &Campaign, region: u8) -> Result<Vec<u8>> {
    distance(s, region)?;
    Ok(
        s.data[0x50ec + usize::from(region) * 8..0x50f4 + usize::from(region) * 8]
            .iter()
            .copied()
            .filter(|p| *p != 255)
            .collect(),
    )
}
fn spawn_at(s: &mut Campaign, army: usize, location: u8, origin: u8) -> Result<()> {
    if s.ownership(location)? != 0 {
        return Ok(());
    }
    let a = army * 14;
    let p = usize::from(location) * 32;
    s.data[0xc113 + a] = location;
    put(s, 0xc114 + a, u16::from(origin));
    put(s, 0xc118 + a, word(&s.data, 0x526c + p)?);
    put(s, 0xc116 + a, word(&s.data, 0x526e + p)?);
    for (to, from) in [(0xc11c, 0x527c), (0xc11d, 0x527b), (0xc11e, 0x527a)] {
        s.data[to + a] = s.data[from + p];
    }
    put(s, 0xc11a + a, 65535);
    Ok(())
}
fn local_spawn(s: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
    // 4E4E reads BP-6 when all local slots are full. The preceding RNG
    // helper leaves its saved DX (zero on the observed normal land path) here.
    // Preserve that reference path: it overwrites border army zero. Other IRQ/
    // stale-register residues are a documented undefined-state boundary.
    let id = (7..11)
        .find(|i| !active(s, *i).unwrap_or(true))
        .unwrap_or(0);
    if !(1..27).any(|p| s.data[0x5285 + p * 32] == 0) {
        return Ok(());
    }
    let province = loop {
        let p = rng.bounded(26) as u8 + 1;
        if s.ownership(p)? == 0 {
            break p;
        }
    };
    s.data[0xc112 + id * 14] = u8::from(rng.bounded(10) <= 4);
    spawn_at(s, id, province, province)
}
fn spawn_and_reinforce(s: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
    s.data[0x3d9c] = (i16::from(s.data[0x3d9c]) + 10).min(50) as u8;
    if rng.bounded(100) < u16::from(s.data[0x3d9c]) {
        let band = usize::from(((word(&s.data, 0x634c)?.wrapping_sub(5)) / 8).min(4));
        let roll = rng.bounded(100);
        let mut sum = 0u16;
        let id = (0..7)
            .find(|i| {
                sum += u16::from(s.data[0x3d2e + band * 7 + i]);
                sum > roll
            })
            .ok_or("Invalid enemy spawn weights")?;
        if active(s, id)? {
            local_spawn(s, rng)?;
        } else {
            s.data[0xc112 + id * 14] = 0;
            spawn_at(s, id, s.data[0x3d58 + id], s.data[0x3d60 + id])?;
        }
        s.data[0x3d9c] = 10;
    } else {
        let owned = (0..27).filter(|p| s.data[0x5285 + p * 32] != 0).count();
        if rng.bounded(100) < u16::from(s.data[0x3d52 + owned / 5]) {
            local_spawn(s, rng)?;
        }
    }
    for i in 0..11 {
        if !active(s, i)? {
            continue;
        }
        let a = i * 14;
        let origin = usize::from(word(&s.data, 0xc114 + a)?);
        if origin >= 27 {
            return Err("Invalid enemy origin".into());
        }
        for (to, maximum, add) in [(0xc118, 0x526c, 100), (0xc116, 0x526e, 20)] {
            put(
                s,
                to + a,
                word(&s.data, to + a)?
                    .wrapping_add(add)
                    .min(word(&s.data, maximum + origin * 32)?),
            );
        }
    }
    for offset in 0..3 {
        let fleet = offset + 3;
        if s.data[0x1116 + fleet * 12] == 255 {
            continue;
        }
        let kind = usize::from(word(&s.data, 0x3d90 + offset * 2)?);
        if kind >= 3 {
            return Err("Invalid enemy ship kind".into());
        }
        let ships = 0x110e + fleet * 12 + kind * 2;
        let mut count = word(&s.data, ships)?;
        if count < word(&s.data, 0x3d96 + offset * 2)? {
            count = count.wrapping_add(1);
            put(s, ships, count);
        }
        if count == 0 {
            return Err("Active enemy fleet has no ships".into());
        }
        let inf = 0xc118 + (fleet + 8) * 14;
        // 4D62 reads a byte from the three-byte capacity table.
        let capacity = count.wrapping_mul(u16::from(s.data[0x8ede + kind]));
        let troops = word(&s.data, inf)?
            .wrapping_add(100)
            .min(8000)
            .min(capacity);
        put(s, inf, troops);
        s.data[0x1118 + fleet * 12] = (troops / count) as u8;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Choose,
    Land,
    Fight(usize),
    AwaitLand(usize),
    Rome,
    General,
    Garrison,
    Capture,
    Sea,
    Naval(usize),
    AwaitNaval(usize),
    DestroyFleet(usize),
    Done,
}
#[derive(Debug, PartialEq, Eq)]
pub enum EnemyAction {
    Notice(String),
    Land {
        army: usize,
        legion: usize,
        province: u8,
    },
    Naval {
        army: usize,
        fleet: usize,
    },
    Done,
}
pub struct EnemyTurn {
    army: usize,
    target: u8,
    defender: usize,
    step: Step,
}
impl EnemyTurn {
    pub fn new(s: &Campaign) -> Result<Self> {
        Ok(Self {
            army: 0,
            target: 0,
            defender: 0,
            step: if word(&s.data, 0x634c)? < 5 {
                Step::Done
            } else {
                Step::Choose
            },
        })
    }
    fn warning(&self, s: &Campaign, defender: usize, sea: bool) -> Result<String> {
        let origin = word(&s.data, 0xc114 + self.army * 14)? as u8;
        let name = if sea {
            s.string(word(&s.data, 0x1100 + defender * 2)? as usize)?
        } else {
            s.legion_name(defender)?
        };
        Ok(format!(
            "{}{}{}{}{}{}{}{}",
            s.string(word(&s.data, 0x3d7a)? as usize)?,
            s.province_name(origin)?,
            s.string(word(&s.data, 0x3d7e)? as usize)?,
            s.region_name(self.target)?,
            s.string(word(&s.data, 0x3d82)? as usize)?,
            name,
            s.string(word(&s.data, 0x3d88)? as usize)?,
            s.string(word(&s.data, if sea { 0x3d8e } else { 0x3d8c })? as usize)?
        ))
    }
    fn next_army(&mut self) {
        self.army += 1;
        self.step = Step::Choose;
    }
    pub fn next(&mut self, s: &mut Campaign, rng: &mut OriginalRng) -> Result<EnemyAction> {
        loop {
            match self.step {
                Step::Done => return Ok(EnemyAction::Done),
                Step::Choose => {
                    while self.army < 11 && !active(s, self.army)? {
                        self.army += 1;
                    }
                    if self.army == 11 {
                        spawn_and_reinforce(s, rng)?;
                        self.step = Step::Done;
                        continue;
                    }
                    let loc = s.data[0xc113 + self.army * 14];
                    let random = s.data[0xc112 + self.army * 14] == 1;
                    if !random && loc == 0 {
                        put(s, 0xc11a + self.army * 14, 0);
                        self.next_army();
                        continue;
                    }
                    let options = neighbours(s, loc)?
                        .into_iter()
                        .filter(|p| {
                            random
                                || distance(s, *p).is_ok_and(|d| d < distance(s, loc).unwrap_or(0))
                        })
                        .collect::<Vec<_>>();
                    let n = usize::from(rng.bounded(options.len() as u16));
                    self.target = *options.get(n).ok_or("Enemy has no valid destination")?;
                    self.defender = 0;
                    if self.target < 27 {
                        if self.target == 0 {
                            s.data[0x1a13] = s.data[0x527a];
                            s.data[0x1a15] = 2;
                            s.data[0x1a19] = 255;
                            for a in [0x1a12, 0x1a14, 0x1a16, 0x1a17, 0x1a1a] {
                                s.data[a] = 0;
                            }
                        }
                        self.step = Step::Land;
                    } else {
                        s.data[0xc113 + self.army * 14] = self.target;
                        self.step = Step::Sea;
                    }
                }
                Step::Land => {
                    if !active(s, self.army)? {
                        self.next_army();
                        continue;
                    }
                    if let Some(id) = (self.defender..12).find(|i| {
                        s.data[0x1925 + i * 20] != 255 && s.data[0x1924 + i * 20] == self.target
                    }) {
                        self.defender = id + 1;
                        self.step = Step::Fight(id);
                        return Ok(EnemyAction::Notice(self.warning(s, id, false)?));
                    }
                    self.step = Step::Rome;
                }
                Step::Fight(id) => {
                    self.step = Step::AwaitLand(id);
                    return Ok(EnemyAction::Land {
                        army: self.army,
                        legion: id,
                        province: self.target,
                    });
                }
                Step::AwaitLand(_) | Step::AwaitNaval(_) => {
                    return Err("Enemy turn awaits battle result".into());
                }
                Step::Rome => {
                    if self.target == 0 && active(s, self.army)? && s.ownership(0)? != 0 {
                        self.step = Step::General;
                        let origin = word(&s.data, 0xc114 + self.army * 14)? as u8;
                        return Ok(EnemyAction::Notice(format!(
                            "{}{}{}",
                            s.string(0x3d9d)?,
                            s.province_name(origin)?,
                            s.string(0x3db4)?
                        )));
                    }
                    self.step = Step::Capture;
                }
                Step::General => {
                    self.step = Step::Garrison;
                    let old = usize::from(s.data[0x1a18]);
                    if old >= 24 || s.data[0x1a20 + old * 6] != 0 {
                        let mut pool = (0..24)
                            .filter(|i| s.data[0x1a20 + i * 6] == 1)
                            .collect::<Vec<_>>();
                        if pool.is_empty() {
                            for i in 0..24 {
                                if s.data[0x1a20 + i * 6] != 0 {
                                    s.data[0x1a20 + i * 6] = 1;
                                    pool.push(i);
                                }
                            }
                        }
                        let id = *pool
                            .get(usize::from(rng.bounded(pool.len() as u16)))
                            .ok_or("No commander for Rome")?;
                        s.data[0x1a18] = id as u8;
                        s.data[0x1a20 + id * 6] = 0;
                        return Ok(EnemyAction::Notice(format!(
                            "{}{}{}{}",
                            s.string(word(&s.data, 0x1a1c + id * 6)? as usize)?,
                            s.string(0x1f89)?,
                            s.roman(s.data[0x1a12])?,
                            s.string(0x1a08)?
                        )));
                    }
                }
                Step::Garrison => {
                    put(s, 0x1a0e, word(&s.data, 0x5270)?);
                    put(s, 0x1a10, word(&s.data, 0x5272)?);
                    self.step = Step::Fight(12);
                }
                Step::Capture => {
                    if active(s, self.army)? {
                        for i in 0..12 {
                            if s.data[0x1924 + i * 20] == self.target {
                                s.retire_legion(i)?;
                            }
                        }
                        s.data[0xc113 + self.army * 14] = self.target;
                        let previous_status = s.ownership(self.target)?;
                        if previous_status != 0 {
                            let p = usize::from(self.target) * 32;
                            s.data[0x5287 + p] = 255;
                            // Use the pre-capture status, as corrected in DOS 1.00B.
                            s.data[0x634e] = ((5 - i16::from(previous_status))
                                * i16::from(s.data[0x634e])
                                / 4) as u8;
                            s.data[0x5285 + p] = 0;
                            s.reset_province_enemy_fleets(self.target)?;
                            s.change_score(-if self.target > 5 {
                                5
                            } else if self.target == 2 {
                                30
                            } else {
                                15
                            })?;
                        }
                    }
                    self.next_army();
                }
                Step::Sea => {
                    if !active(s, self.army)? {
                        self.next_army();
                        continue;
                    }
                    if let Some(id) = (self.defender..3).find(|i| {
                        s.data[0x1116 + i * 12] != 255 && s.data[0x1114 + i * 12] == self.target
                    }) {
                        self.defender = id + 1;
                        self.step = Step::Naval(id);
                        return Ok(EnemyAction::Notice(self.warning(s, id, true)?));
                    }
                    self.next_army();
                }
                Step::Naval(fleet) => {
                    s.prepare_army_fleet(self.army, rng)?;
                    self.step = Step::AwaitNaval(fleet);
                    return Ok(EnemyAction::Naval {
                        army: self.army,
                        fleet,
                    });
                }
                Step::DestroyFleet(fleet) => {
                    s.data[0x1116 + fleet * 12] = 255;
                    let cargo = s.data[0x1117 + fleet * 12];
                    if cargo != 255 {
                        s.retire_legion(usize::from(cargo))?;
                    }
                    self.step = Step::Sea;
                }
            }
        }
    }
    pub fn land_returned(&mut self, s: &mut Campaign, legion: usize, victory: bool) -> Result<()> {
        if self.step != Step::AwaitLand(legion) {
            return Err("Unexpected defensive land result".into());
        }
        if victory {
            put(s, 0xc11a + self.army * 14, 0);
            if legion == 12 {
                put(s, 0x5270, word(&s.data, 0x1a0e)?);
                put(s, 0x5272, word(&s.data, 0x1a10)?);
            }
        }
        self.step = if legion == 12 {
            Step::Capture
        } else {
            Step::Land
        };
        Ok(())
    }
    pub fn naval_returned(&mut self, s: &mut Campaign, result: u16) -> Result<Option<String>> {
        let Step::AwaitNaval(fleet) = self.step else {
            return Err("Unexpected defensive naval result".into());
        };
        self.step = Step::Sea;
        if result & 2 != 0 {
            self.next_army();
            return Ok(None);
        }
        if result & 4 != 0 {
            let loc = s.data[0x1114 + fleet * 12];
            let current = distance(s, loc)?;
            let target = neighbours(s, loc)?.into_iter().find(|p| {
                *p >= 27
                    && distance(s, *p).is_ok_and(|d| d <= current)
                    && !(3..6)
                        .any(|i| s.data[0x1114 + i * 12] == *p && s.data[0x1116 + i * 12] != 255)
            });
            if let Some(p) = target {
                s.data[0x1114 + fleet * 12] = p;
                s.data[0x1116 + fleet * 12] = 1;
                let cargo = s.data[0x1117 + fleet * 12];
                if cargo != 255 {
                    s.data[0x1924 + usize::from(cargo) * 20] = p;
                }
            } else {
                self.step = Step::DestroyFleet(fleet);
                return Ok(Some(s.string(0x3dce)?));
            }
        }
        Ok(None)
    }
}

/// Private comparison of the recovered normal full-slot stack-residue path.
pub fn spawn_observation(
    assets: &crate::assets::Assets,
    source: &std::path::Path,
    output: &std::path::Path,
) -> Result<()> {
    let bytes = crate::assets::read(source)?;
    if bytes.len() != 65536 {
        return Err("Expected a full observed DS".into());
    }
    let mut state = Campaign::new(assets)?;
    state.data = bytes;
    let mut rng = OriginalRng {
        first: word(&state.data, 0xc8)?,
        second: word(&state.data, 0xca)?,
    };
    local_spawn(&mut state, &mut rng)?;
    put(&mut state, 0xc8, rng.first);
    put(&mut state, 0xca, rng.second);
    std::fs::write(output, state.data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Campaign {
        let mut s = Campaign::test_data(vec![0; 65536]);
        for i in 0..12 {
            s.data[0x1924 + i * 20] = 255;
            s.data[0x1925 + i * 20] = 255;
        }
        for i in 0..6 {
            s.data[0x1116 + i * 12] = 255;
        }
        s.data[0x50ec..0x5214].fill(255);
        for p in 0..27 {
            s.data[0x5285 + p * 32] = 4;
        }
        put(&mut s, 0x634c, 5);
        s
    }
    fn rng() -> OriginalRng {
        OriginalRng {
            first: 123,
            second: 456,
        }
    }
    #[test]
    fn early_year_skips_spawning_reinforcement_and_randomness() {
        let mut s = state();
        put(&mut s, 0x634c, 4);
        let before = s.data.clone();
        let mut r = rng();
        assert_eq!(
            EnemyTurn::new(&s).unwrap().next(&mut s, &mut r).unwrap(),
            EnemyAction::Done
        );
        assert_eq!(s.data, before);
        assert_eq!((r.first, r.second), (123, 456));
    }
    #[test]
    fn second_defender_victory_stops_invaders_without_disbanding_first() {
        let mut s = state();
        let mut r = rng();
        put(&mut s, 0xc11a, 65535);
        s.data[0xc113] = 4;
        s.data[0x528a + 4 * 32] = 2;
        s.data[0x528a + 11 * 32] = 1;
        s.data[0x50ec + 4 * 8] = 11;
        put(&mut s, 0xc114, 4);
        for i in 0..2 {
            s.data[0x1924 + i * 20] = 11;
            s.data[0x1925 + i * 20] = 2;
            s.data[0x1928 + i * 20] = i as u8;
        }
        let mut turn = EnemyTurn::new(&s).unwrap();
        for i in 0..2 {
            assert!(matches!(
                turn.next(&mut s, &mut r).unwrap(),
                EnemyAction::Notice(_)
            ));
            assert_eq!(
                turn.next(&mut s, &mut r).unwrap(),
                EnemyAction::Land {
                    army: 0,
                    legion: i,
                    province: 11
                }
            );
            turn.land_returned(&mut s, i, i == 1).unwrap();
        }
        assert_eq!(turn.next(&mut s, &mut r).unwrap(), EnemyAction::Done);
        assert_eq!(word(&s.data, 0xc11a).unwrap(), 0);
        assert_eq!(s.data[0xc113], 4);
        assert_eq!(s.data[0x1925], 2);
        assert_eq!(s.data[0x5285 + 11 * 32], 4);
    }
    #[test]
    fn enemy_capture_applies_reputation_from_the_previous_province_status() {
        for (status, reputation, expected) in [
            (0, 80, 80),
            (1, 80, 80),
            (2, 80, 60),
            (3, 80, 40),
            (4, 80, 20),
            (4, 79, 19),
            (4, 1, 0),
            (4, 0, 0),
            (1, 255, 255),
        ] {
            let mut s = state();
            let mut r = rng();
            put(&mut s, 0xc11a + 10 * 14, 1000);
            put(&mut s, 0x594a, 100);
            s.data[0x3d2e] = 100;
            s.data[0x5285 + 11 * 32] = status;
            s.data[0x5287 + 11 * 32] = 2;
            s.data[0x634e] = reputation;
            let mut turn = EnemyTurn {
                army: 10,
                target: 11,
                defender: 0,
                step: Step::Capture,
            };
            assert_eq!(turn.next(&mut s, &mut r).unwrap(), EnemyAction::Done);
            assert_eq!(
                s.data[0x634e], expected,
                "status {status}, reputation {reputation}"
            );
            assert_eq!(s.ownership(11).unwrap(), 0);
            assert_eq!(s.data[0xc113 + 10 * 14], 11);
            assert_eq!(
                word(&s.data, 0x594a).unwrap(),
                if status == 0 { 100 } else { 95 }
            );
            assert_eq!(s.data[0x5287 + 11 * 32], if status == 0 { 2 } else { 255 });
        }
    }
    #[test]
    fn naval_retreat_spends_the_fleet_action_and_moves_cargo_without_spending_it() {
        let mut s = state();
        let mut turn = EnemyTurn {
            army: 0,
            target: 31,
            defender: 1,
            step: Step::AwaitNaval(0),
        };
        s.data[0x1114] = 31;
        s.data[0x1116] = 0;
        s.data[0x1117] = 2;
        s.data[0x1924 + 2 * 20] = 31;
        s.data[0x1926 + 2 * 20] = 2;
        s.data[0x3d55 + 31] = 2;
        s.data[0x3d55 + 30] = 2;
        s.data[0x3d55 + 32] = 1;
        s.data[0x50ec + 31 * 8] = 30;
        s.data[0x50ec + 31 * 8 + 1] = 32;
        s.data[0x1114 + 3 * 12] = 30;
        s.data[0x1116 + 3 * 12] = 0;
        assert_eq!(turn.naval_returned(&mut s, 4).unwrap(), None);
        assert_eq!(s.data[0x1114], 32);
        assert_eq!(s.data[0x1924 + 2 * 20], 32);
        assert_eq!(s.data[0x1116], 1);
        assert_eq!(s.data[0x1926 + 2 * 20], 2);
    }
    #[test]
    fn non_retreat_naval_results_preserve_fleet_location_and_action() {
        for result in [0, 2] {
            let mut s = state();
            let mut turn = EnemyTurn {
                army: 0,
                target: 31,
                defender: 1,
                step: Step::AwaitNaval(0),
            };
            s.data[0x1114] = 31;
            s.data[0x1116] = 0;
            s.data[0x1117] = 2;
            s.data[0x1924 + 2 * 20] = 31;
            let before = s.data.clone();
            assert_eq!(turn.naval_returned(&mut s, result).unwrap(), None);
            assert_eq!(s.data, before, "result {result}");
        }
    }
    #[test]
    fn garrison_victory_updates_rome_and_preserves_invaders_old_location() {
        let mut s = state();
        put(&mut s, 0xc11a, 65535);
        s.data[0xc113] = 11;
        put(&mut s, 0x1a0e, 1200);
        put(&mut s, 0x1a10, 200);
        let mut turn = EnemyTurn {
            army: 0,
            target: 0,
            defender: 12,
            step: Step::AwaitLand(12),
        };
        turn.land_returned(&mut s, 12, true).unwrap();
        assert_eq!(word(&s.data, 0x5270).unwrap(), 1200);
        assert_eq!(word(&s.data, 0x5272).unwrap(), 200);
        assert_eq!(word(&s.data, 0xc11a).unwrap(), 0);
        assert_eq!(s.data[0xc113], 11);
    }
    #[test]
    fn trapped_retreat_reports_before_destroying_fleet_and_retiring_cargo() {
        let mut s = state();
        let mut rng = rng();
        s.data[0x1114] = 31;
        s.data[0x1116] = 0;
        s.data[0x1117] = 0;
        put(&mut s, 0x110e, 20);
        s.data[0x1925] = 2;
        s.data[0x1924] = 31;
        s.data[0x1928] = 0;
        let mut turn = EnemyTurn {
            army: 0,
            target: 31,
            defender: 1,
            step: Step::AwaitNaval(0),
        };
        assert!(turn.naval_returned(&mut s, 4).unwrap().is_some());
        assert_eq!(s.data[0x1116], 0);
        assert_eq!(s.data[0x1925], 2);
        assert_eq!(turn.next(&mut s, &mut rng).unwrap(), EnemyAction::Done);
        assert_eq!(s.data[0x1116], 255);
        assert_eq!(s.data[0x1925], 255);
        assert_eq!(s.data[0x1a20], 1);
        assert_eq!(s.data[0x1117], 255);
        assert_eq!(s.data[0x1929], 255);
        assert_eq!(word(&s.data, 0x110e).unwrap(), 20);
    }
}
