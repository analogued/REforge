// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original land-battle report and delayed campaign settlement (E21A).
use super::{Preparation, put};
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub struct Outcome {
    pub victory: bool,
    roman_lost: u16,
    enemy_lost: u16,
    roman_left: u16,
    enemy_left: u16,
    enemy: [u16; 3],
    text: String,
    pub replacement: Option<String>,
    acknowledged: bool,
}

fn add_counter(state: &mut Campaign, at: usize, value: u16) {
    let current = u32::from_le_bytes(state.data[at..at + 4].try_into().unwrap());
    state.data[at..at + 4].copy_from_slice(&current.wrapping_add(u32::from(value)).to_le_bytes());
}

impl Preparation {
    pub(super) fn prepare_outcome(&mut self, state: &mut Campaign) -> Result<()> {
        state.sound.stop();
        state.sound.play(self.battle_music() - 1);
        let mut totals = [[0u16; 3]; 2];
        let mut victory = false;
        for unit in self.units.iter().skip(1) {
            let count = word(unit, 0)?;
            if count == 0 {
                continue;
            }
            if unit[11] == 0 && unit[10] != 3 && !matches!(unit[13], 3 | 4) {
                victory = true;
            }
            if unit[10] < 3 {
                let total = &mut totals[usize::from(unit[11])][usize::from(unit[10])];
                *total = total.wrapping_add(count);
            }
        }
        let at = 0x1918 + self.legion * 20;
        let base = usize::from(self.province) * 32;
        let roman_left = totals[0][0].wrapping_add(totals[0][2]);
        let enemy_left = totals[1][0].wrapping_add(totals[1][2]);
        let roman_lost = word(&self.ds, at + 6)?
            .wrapping_add(word(&self.ds, at + 8)?)
            .wrapping_sub(roman_left);
        let enemy_lost = word(&self.ds, 0xc29e)?
            .wrapping_add(word(&self.ds, 0xc29c)?)
            .wrapping_add(u16::from(self.ds[0xc2a2]))
            .wrapping_sub(enemy_left)
            .wrapping_sub(totals[1][1]);
        put(&mut state.data, at + 6, totals[0][2]);
        put(&mut state.data, at + 8, totals[0][0]);
        if victory {
            state.data[at + 11] = if state.data[at + 11] > 94 {
                99
            } else {
                state.data[at + 11] + 5
            };
            if self.enemy_province == self.province {
                let recruits = enemy_left.min(word(&state.data, 0x5276 + base)?);
                put(&mut state.data, 0x5274 + base, recruits);
            }
            for &region in &self.ds
                [0x50ec + usize::from(self.province) * 8..0x50f4 + usize::from(self.province) * 8]
            {
                if region < 27 {
                    let at = 0x527d + usize::from(region) * 32;
                    state.data[at] = state.data[at].saturating_add(25).min(100);
                }
            }
            let wins = word(&state.data, 0xc0fa)?.wrapping_add(1);
            put(&mut state.data, 0xc0fa, wins);
        } else if roman_left == 0 {
            state.retire_legion(self.legion)?;
        } else {
            state.data[at + 11] = state.data[at + 11].saturating_sub(5);
        }
        add_counter(state, 0xc086, roman_lost);
        add_counter(state, 0xc082, enemy_lost);
        let roman = state.roman(state.data[at + 10])?;
        let name = state.string(at)?;
        let province = state.province_name(self.enemy_province)?;
        let leader = state
            .string(word(&self.ds, 0x5f1c + usize::from(self.enemy_province) * 2)? as usize)?;
        let enemy_tactic = state.string(word(
            &self.ds,
            0x76f6 + (self.enemy_formation * 5 + self.enemy_tactic) * 10,
        )? as usize)?;
        let tactic = state
            .string(word(&self.ds, 0x76f6 + (self.formation * 5 + self.tactic) * 10)? as usize)?;
        let commander =
            state.string(word(&self.ds, 0x1a1c + usize::from(self.ds[at + 16]) * 6)? as usize)?;
        let mut text =
            state.string(word(&self.ds, if victory { 0x7ad8 } else { 0x7ada })? as usize)?;
        for (address, value) in [
            (0x7ade, None),
            (0, Some(roman.as_str())),
            (0, Some(name.as_str())),
            (0x7ae4, None),
            (0, Some(province.as_str())),
            (0x7ae8, None),
            (0, Some(leader.as_str())),
            (0x7aec, None),
            (0, Some(enemy_tactic.as_str())),
            (0x7af0, None),
            (0, Some(tactic.as_str())),
            (0x7af4, None),
            (0, Some(commander.as_str())),
            (0x7af8, None),
            (
                0,
                Some(
                    if victory {
                        state.string(word(&self.ds, 0x7ad6)? as usize)?
                    } else {
                        state.string(word(&self.ds, 0x7ad4)? as usize)?
                    }
                    .as_str(),
                ),
            ),
        ] {
            if let Some(value) = value {
                text.push_str(value);
            } else {
                text.push_str(&state.string(word(&self.ds, address)? as usize)?);
            }
        }
        text.push_str(&roman);
        text.push_str(&name);
        text.push_str(&state.string(word(&self.ds, 0x7b00)? as usize)?);
        text.push_str(&province);
        text.push_str(&state.string(word(&self.ds, 0x7b04)? as usize)?);
        self.outcome = Some(Outcome {
            victory,
            roman_lost,
            enemy_lost,
            roman_left,
            enemy_left,
            enemy: totals[1],
            text,
            replacement: None,
            acknowledged: false,
        });
        Ok(())
    }

    pub fn acknowledge_outcome(
        &mut self,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<bool> {
        let outcome = self.outcome.as_mut().ok_or("Missing battle result")?;
        if !outcome.acknowledged {
            outcome.acknowledged = true;
            let at = 0x1918 + self.legion * 20;
            let commander = usize::from(state.data[at + 16]);
            if state.data[0x1a20 + commander * 6] == 2 && state.data[at + 13] != 255 {
                let mut available: Vec<_> = (0..24)
                    .filter(|i| state.data[0x1a20 + i * 6] == 1)
                    .collect();
                if available.is_empty() {
                    available = (0..24)
                        .filter(|i| state.data[0x1a20 + i * 6] != 0)
                        .collect();
                    for &i in &available {
                        state.data[0x1a20 + i * 6] = 1;
                    }
                }
                if available.is_empty() {
                    return Err("No replacement commander".into());
                }
                let next = available[usize::from(rng.bounded(available.len() as u16))];
                state.data[at + 16] = next as u8;
                state.data[0x1a20 + next * 6] = 0;
                outcome.replacement = Some(format!(
                    "{}{}{}{}",
                    state.string(word(&state.data, 0x1a1c + next * 6)? as usize)?,
                    state.string(0x1f89)?,
                    state.roman(state.data[at + 10])?,
                    state.string(at)?
                ));
                return Ok(false);
            }
        }
        // The replacement notification restores the casualty report beneath it.
        outcome.replacement = None;
        if self.enemy_province != self.province {
            let army = self.army.ok_or("Missing roaming-army result destination")?;
            let at = 0xc114 + army * 14;
            state.data[at..at + 12].copy_from_slice(&self.ds[0xc29a..0xc2a6]);
            put(&mut state.data, at + 2, outcome.enemy[0]);
            put(&mut state.data, at + 4, outcome.enemy[2]);
            state.data[at + 8] = outcome.enemy[1] as u8;
            if outcome.victory {
                put(&mut state.data, at + 6, 0);
            }
        } else if !outcome.victory {
            let base = usize::from(self.province) * 32;
            put(&mut state.data, 0x5270 + base, outcome.enemy[2]);
            put(&mut state.data, 0x5272 + base, outcome.enemy[0]);
            state.data[0x527c + base] = outcome.enemy[1] as u8;
        }
        Ok(true)
    }
}

impl Outcome {
    pub(super) fn render(&self, frame: &mut Frame, assets: &Assets) -> Result<()> {
        let widths = word(&assets.data_segment, 0xa05c)? as usize;
        let width = self
            .text
            .lines()
            .map(|line| {
                line.bytes()
                    .filter(|b| (32..128).contains(b))
                    .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                    .sum::<i32>()
            })
            .max()
            .unwrap_or(0);
        let half_height = self.text.split('\n').count() as i32 * 11 / 2;
        frame.bevel(
            159 - width / 2 - 7,
            99 - half_height - 7,
            159 + width / 2 + 7,
            99 + half_height + 7,
            26,
        );
        frame.center_lines(assets, &self.text, 159, 99 - half_height, 0)?;
        for (value, x, y, right) in [
            (self.roman_lost, 120, 138, true),
            (self.enemy_lost, 196, 138, false),
            (self.roman_left, 120, 149, true),
            (self.enemy_left, 196, 149, false),
        ] {
            let text = value.to_string();
            let width = text
                .bytes()
                .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                .sum::<i32>();
            frame.text(assets, &text, if right { x - width } else { x }, y, 0)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(roman_status: u8) -> (Preparation, Campaign) {
        let mut scene = super::super::tests::synthetic();
        scene.ds[0x50ec + 11 * 8..0x50f4 + 11 * 8].fill(255);
        scene.ds[0x50ec + 11 * 8] = 12;
        put(&mut scene.ds, 0x191e, 100);
        put(&mut scene.ds, 0x5270 + 11 * 32, 100);
        put(&mut scene.ds, 0xc29e, 100);
        put(&mut scene.ds, 0x5276 + 11 * 32, 20);
        scene.ds[0x1923] = 70;
        scene.ds[0x527d + 12 * 32] = 90;
        scene.add_units(90, 90, 2, 0, 75);
        scene.units[1][13] = roman_status;
        scene.add_units(30, 30, 2, 1, 55);
        scene.units[2][13] = 3;
        let state = Campaign::test_data(scene.ds.clone());
        (scene, state)
    }

    #[test]
    fn victory_counts_routed_survivors_and_stages_conquest_after_the_report() {
        let (mut scene, mut state) = result(1);
        state.data[0xc082..0xc086].copy_from_slice(&u32::MAX.to_le_bytes());
        scene.prepare_outcome(&mut state).unwrap();
        let outcome = scene.outcome.as_ref().unwrap();
        assert!(outcome.victory);
        assert_eq!(
            (
                outcome.roman_lost,
                outcome.enemy_lost,
                outcome.roman_left,
                outcome.enemy_left
            ),
            (10, 70, 90, 30)
        );
        assert_eq!(word(&state.data, 0x191e).unwrap(), 90);
        assert_eq!(word(&state.data, 0x5274 + 11 * 32).unwrap(), 20);
        assert_eq!(state.data[0x1923], 75);
        assert_eq!(state.data[0x527d + 12 * 32], 100);
        assert_eq!(word(&state.data, 0xc0fa).unwrap(), 1);
        assert_eq!(
            u32::from_le_bytes(state.data[0xc082..0xc086].try_into().unwrap()),
            69
        );
        // The caller owns conquest, tribute, score and the march after acknowledgment.
        assert_eq!(state.data[0x5285 + 11 * 32], 0);
        assert_eq!(state.data[0x1924], 0);
    }

    #[test]
    fn defeat_keeps_routed_survivors_but_destroyed_legions_preserve_dead_commanders() {
        let (mut scene, mut state) = result(3);
        scene.prepare_outcome(&mut state).unwrap();
        assert!(!scene.outcome.as_ref().unwrap().victory);
        assert_eq!(state.data[0x1923], 65);
        let mut rng = OriginalRng {
            first: 0,
            second: 0,
        };
        assert!(scene.acknowledge_outcome(&mut state, &mut rng).unwrap());
        assert_eq!(word(&state.data, 0x5270 + 11 * 32).unwrap(), 30);
        let (mut scene, mut state) = result(3);
        put(&mut scene.units[1], 0, 0);
        state.data[0x1a20] = 2;
        scene.prepare_outcome(&mut state).unwrap();
        assert_eq!(state.data[0x1924], 255);
        assert_eq!(state.data[0x1925], 255);
        assert_eq!(state.data[0x1a20], 2);
    }

    #[test]
    fn annihilated_landing_force_frees_transport_and_slot_reuse_cannot_move_new_legion() {
        let (mut scene, mut state) = result(3);
        put(&mut scene.units[1], 0, 0);
        state.data[0x1924] = 31;
        state.data[0x1929] = 1;
        state.data[0x1114 + 12] = 31;
        state.data[0x1116 + 12] = 0;
        state.data[0x1117 + 12] = 0;
        put(&mut state.data, 0x110e + 12, 10);
        scene.prepare_outcome(&mut state).unwrap();
        assert!(!scene.outcome.as_ref().unwrap().victory);
        assert_eq!(state.data[0x1929], 255);
        assert_eq!(state.data[0x1117 + 12], 255);
        assert_eq!(state.data[0x1116 + 12], 0);
        assert_eq!(word(&state.data, 0x110e + 12).unwrap(), 10);

        // Recruit into the freed record while the surviving fleet stays at sea.
        state.data[0x595e] = 12;
        put(&mut state.data, 0x5274 + 5 * 32, 100);
        put(&mut state.data, 0x1aac, 100);
        let mut rng = OriginalRng {
            first: 1,
            second: 2,
        };
        assert!(state.begin_recruitment(5, 0, &mut rng).unwrap().success);
        assert_eq!(state.data[0x1924], 5);
        assert_eq!(state.data[0x1929], 255);

        // Another local legion can use the now empty transport.
        state.data[0x1924 + 20] = 11;
        state.data[0x1929 + 20] = 255;
        put(&mut state.data, 0x191e + 20, 100);
        state.data[0x8ede] = 50;
        state.data[0x50ec + 11 * 8..0x50f4 + 11 * 8].fill(255);
        state.data[0x50ec + 11 * 8] = 31;
        assert_eq!(
            crate::fleet::Boarding::prepare(&state, 1, 31)
                .unwrap()
                .fleet,
            1
        );

        state.data[0x50ec + 31 * 8..0x50f4 + 31 * 8].fill(255);
        state.data[0x50ec + 31 * 8] = 32;
        state.finish_fleet_sailing(1, 32).unwrap();
        assert_eq!(state.data[0x1114 + 12], 32);
        assert_eq!(state.data[0x1924], 5);
        assert_eq!(state.data[0x1924 + 20], 11);
    }

    #[test]
    fn replacing_a_fallen_commander_waits_for_the_report_and_consumes_one_draw() {
        let (mut scene, mut state) = result(1);
        state.data[0x1a20] = 2;
        state.data[0x1a20 + 6] = 1;
        state.data[0x1a20 + 12] = 1;
        scene.prepare_outcome(&mut state).unwrap();
        assert_eq!(state.data[0x1928], 0);
        let mut rng = OriginalRng {
            first: 13,
            second: 17,
        };
        let mut expected = rng;
        let next = [1, 2][usize::from(expected.bounded(2))];
        assert!(!scene.acknowledge_outcome(&mut state, &mut rng).unwrap());
        assert_eq!(state.data[0x1928], next);
        assert_eq!(state.data[0x1a20], 2);
        assert_eq!(state.data[0x1a20 + usize::from(next) * 6], 0);
        assert!(scene.outcome.as_ref().unwrap().replacement.is_some());
        assert!(scene.acknowledge_outcome(&mut state, &mut rng).unwrap());
        assert!(scene.outcome.as_ref().unwrap().replacement.is_none());
        assert_eq!((rng.first, rng.second), (expected.first, expected.second));
    }

    #[test]
    fn roaming_losses_update_the_army_or_its_home_garrison_under_the_original_rule() {
        for origin in [3, 11] {
            let (mut scene, mut state) = result(3);
            scene.army = Some(2);
            scene.enemy_province = origin;
            put(&mut scene.ds, 0xc29a, u16::from(origin));
            put(&mut scene.ds, 0xc2a0, 77);
            let at = 0xc114 + 2 * 14;
            put(&mut state.data, at, u16::from(origin));
            put(&mut state.data, at + 4, 100);
            put(&mut state.data, at + 6, 77);
            scene.prepare_outcome(&mut state).unwrap();
            let mut rng = OriginalRng {
                first: 0,
                second: 0,
            };
            assert!(scene.acknowledge_outcome(&mut state, &mut rng).unwrap());
            assert_eq!(word(&state.data, at + 6).unwrap(), 77);
            // E1B9 chooses by origin == battlefield, even for a roaming army.
            assert_eq!(
                word(&state.data, at + 4).unwrap(),
                if origin == 11 { 100 } else { 30 }
            );
            assert_eq!(
                word(&state.data, 0x5270 + 11 * 32).unwrap(),
                if origin == 11 { 30 } else { 100 }
            );
        }
    }
}
