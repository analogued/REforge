// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Private bounded gameplay controller. Uses the same inputs as the window;
//! exports observations, and does not replace the campaign's random generator.
use super::{Input, Phase, Session};
use crate::assets::{Assets, Result, word};
use std::io::Write;
use std::path::Path;

impl Session {
    pub fn soak(&mut self, assets: &Assets, years: u16, output: &Path) -> Result<()> {
        if self.phase != Phase::Map || years == 0 || years > 200 {
            return Err("Campaign soak requires the map and 1..200 years".into());
        }
        std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
        let saves = output.join("slots");
        std::fs::create_dir(&saves).map_err(|e| e.to_string())?;
        self.save_dir = Some(saves);
        let mut log =
            std::fs::File::create(output.join("gameplay.tsv")).map_err(|e| e.to_string())?;
        writeln!(log, "step\tyear\tphase\trng0\trng1\taudio").map_err(|e| e.to_string())?;
        let first = word(&self.campaign.data, 0x634c)?;
        let mut checked = None;
        let mut battles = 0;
        let mut previous = self.phase;
        let mut retreated = false;
        for step in 0..100_000 {
            let year = word(&self.campaign.data, 0x634c)?;
            if self.phase == Phase::Map && checked != Some(year) {
                // Exercise the real save/load dialogs, including description entry.
                let mut before = self.campaign.save();
                let description = format!("Campaign soak year {year}");
                before[4..52].fill(0);
                before[4..4 + description.len()].copy_from_slice(description.as_bytes());
                let rng = (self.rng.first, self.rng.second);
                for input in [Input::Escape, Input::Shortcut(b's'), Input::Enter] {
                    self.input(assets, input)?;
                }
                for _ in 0..48 {
                    self.input(assets, Input::Backspace)?;
                }
                self.input(assets, Input::Text(description))?;
                self.input(assets, Input::Enter)?;
                if self.phase != Phase::Map {
                    return Err("Soak save dialog did not return to map".into());
                }
                for input in [Input::Escape, Input::Shortcut(b'l'), Input::Enter] {
                    self.input(assets, input)?;
                }
                if self.phase != Phase::Map
                    || self.campaign.save() != before
                    || (self.rng.first, self.rng.second) != rng
                {
                    return Err("Save/load changed campaign or RNG during soak".into());
                }
                std::fs::write(output.join(format!("year-{year:03}.CSV")), before)
                    .map_err(|e| e.to_string())?;
                self.render(assets, false)?
                    .export(assets, output, &format!("year-{year:03}"))?;
                checked = Some(year);
                if year.wrapping_sub(first) >= years {
                    writeln!(
                        log,
                        "complete\t{year}\tMap\t{}\t{}\tbattles={battles}",
                        self.rng.first, self.rng.second
                    )
                    .map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
            if self.phase.is_startup() || self.quit {
                writeln!(
                    log,
                    "ended\t{year}\t{:?}\t{}\t{}\tbattles={battles}",
                    self.phase, self.rng.first, self.rng.second
                )
                .map_err(|e| e.to_string())?;
                return Ok(());
            }
            let cues = self.campaign.sound.drain();
            writeln!(
                log,
                "{step}\t{year}\t{:?}\t{}\t{}\t{cues:?}",
                self.phase, self.rng.first, self.rng.second
            )
            .map_err(|e| e.to_string())?;
            if previous != self.phase && self.phase == Phase::BattlePreparation {
                battles += 1;
                retreated = false;
            }
            previous = self.phase;
            let input = match self.phase {
                Phase::Map => Input::Shortcut(b'e'),
                Phase::BattlePreparation => {
                    let scene = self.battle.as_ref().ok_or("Soak battle missing")?;
                    match scene.stage {
                        crate::battle::Stage::Formation => Input::Shortcut(b'b'),
                        crate::battle::Stage::Tactics => Input::Shortcut(b'1'),
                        crate::battle::Stage::Deployment if scene.outcome.is_some() => Input::Enter,
                        crate::battle::Stage::Deployment if scene.motion.paused => {
                            retreated = true;
                            Input::Shortcut(b'r')
                        }
                        crate::battle::Stage::Deployment
                            if scene.motion.iteration > 5000 && !retreated =>
                        {
                            Input::Space
                        }
                        crate::battle::Stage::Deployment => Input::Tick(120),
                    }
                }
                Phase::Naval => match self.naval.as_ref().ok_or("Soak naval missing")?.stage {
                    crate::naval::Stage::Battle => Input::Shortcut(b'r'),
                    crate::naval::Stage::Reengage => Input::Shortcut(b'n'),
                    crate::naval::Stage::Animation | crate::naval::Stage::Boarding => {
                        Input::Tick(72)
                    }
                    _ => Input::Enter,
                },
                Phase::TurnContinue => {
                    self.advance_turn(assets)?;
                    continue;
                }
                Phase::TurnRevolt => {
                    self.advance_revolt(assets)?;
                    continue;
                }
                Phase::TurnEnemy => {
                    self.advance_enemies(assets)?;
                    continue;
                }
                _ => Input::Enter,
            };
            self.input(assets, input)?;
        }
        Err("Campaign soak exceeded its bounded input budget".into())
    }
}
