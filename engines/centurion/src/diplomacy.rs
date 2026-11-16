// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Table-driven foreign-province conversations and peaceful alliances.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scout,
    Choice,
    Response,
    Alliance,
    Withdraw,
    CleoInvitation,
    CleoPause,
    CleoDismissed,
    CleoAnimation,
    CleoAfter,
    CleoFinal,
    Complete,
}

pub struct Choice {
    id: u8,
    pub label: String,
    pub shortcut: u8,
}

pub struct Diplomacy {
    pub province: u8,
    pub legion: usize,
    pub stage: Stage,
    pub choices: Vec<Choice>,
    pub response_ticks: u32,
    state: u8,
    next_state: u8,
    selected_tribute: u8,
    tier: usize,
    weaker: usize,
    text: Vec<u8>,
    body: Frame,
    text_y: i32,
    tabs: Vec<Bitmap>,
    button: Bitmap,
    cleopatra: Option<crate::cleopatra::Cleopatra>,
}

fn block(bytes: &[u8], index: usize) -> Result<String> {
    let start = word(bytes, index * 2)? as usize;
    let end = bytes
        .get(start..)
        .ok_or("Dialogue text offset outside resource")?
        .iter()
        .position(|b| *b == 0)
        .ok_or("Unterminated dialogue text")?;
    Ok(String::from_utf8_lossy(&bytes[start..start + end]).into_owned())
}

fn width(assets: &Assets, text: &str) -> Result<i32> {
    let widths = word(&assets.data_segment, 0xa05c)? as usize;
    let tracking = i32::from(assets.data_segment[0xa064]);
    Ok(text
        .bytes()
        .filter(|b| (32..128).contains(b))
        .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + tracking)
        .sum())
}

fn variant(text: &str, rng: &mut OriginalRng) -> String {
    let choices: Vec<_> = text.split('~').collect();
    let choice = usize::from(rng.bounded(3)).min(choices.len() - 1);
    choices[choice].to_string()
}

impl Campaign {
    fn form_alliance(&mut self, province: u8, tribute: u8) -> Result<()> {
        if province == 0 || province >= 27 || !(1..=3).contains(&tribute) {
            return Err("Invalid alliance terms".into());
        }
        let base = usize::from(province) * 32;
        self.data[0x5282 + base] = tribute;
        self.data[0x5283 + base] = tribute;
        self.data[0x5285 + base] = 2;
        let count = word(&self.data, 0xc106)?.wrapping_add(1);
        self.data[0xc106..0xc108].copy_from_slice(&count.to_le_bytes());
        self.data[0x5289 + base] = 0;
        let recruits = word(&self.data, 0x5274 + base)?
            .wrapping_add(word(&self.data, 0x5272 + base)?)
            .wrapping_add(word(&self.data, 0x5270 + base)?)
            .min(word(&self.data, 0x5276 + base)?);
        self.data[0x5274 + base..0x5276 + base].copy_from_slice(&recruits.to_le_bytes());
        Ok(())
    }

    pub fn finish_alliance_march(&mut self, legion: usize, province: u8) -> Result<()> {
        self.finish_foreign_march(legion, province, 2)
    }

    pub(crate) fn finish_foreign_march(
        &mut self,
        legion: usize,
        province: u8,
        ownership: u8,
    ) -> Result<()> {
        let source = self.legion_location(legion)?;
        if province == 0 || province >= 27 || self.ownership(province)? != ownership {
            return Err("No successful encounter at this destination".into());
        }
        let at = 0x1918 + legion * 20;
        let fleet = self.data[at + 17];
        if source >= 27 && fleet >= 3 {
            return Err("Embarked legion has no valid fleet".into());
        }
        // 39C3..39E0 awards the province-entry score after negotiation returns.
        let award = if province > 5 {
            5
        } else if province == 2 {
            30
        } else {
            15
        };
        let score = word(&self.data, 0x594a)?.wrapping_add(award) as i16;
        self.data[0x594a..0x594c].copy_from_slice(&score.min(256).to_le_bytes());
        if source >= 27 {
            self.data[0x1117 + usize::from(fleet) * 12] = 255;
            self.data[at + 17] = 255;
        }
        self.data[at + 12] = province;
        for owner in (0x1139..0x115d).step_by(12) {
            if self.data[owner] == province {
                self.data[owner + 1] = 255;
            }
        }
        Ok(())
    }
}

impl Diplomacy {
    pub fn cursor_visible(&self) -> bool {
        !matches!(
            self.stage,
            Stage::CleoAnimation | Stage::CleoAfter | Stage::CleoFinal
        )
    }
    pub fn diagnostic(&self) -> String {
        format!("diplomacy={:?} state={}", self.stage, self.state)
    }
    pub fn available(state: &Campaign, province: u8) -> bool {
        province > 0 && province < 27 && state.data[0x5287 + usize::from(province) * 32] == 0
    }

    pub fn new(
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        legion: usize,
        province: u8,
    ) -> Result<Self> {
        if !Self::available(state, province) || state.legion_location(legion)? >= 37 {
            return Err("This province requires a direct battle".into());
        }
        let base = usize::from(province) * 32;
        let mut body = Frame::new();
        body.rect(0, 0, 320, 200, 116);
        // The selected release uses drawing mode 2 (VGA), whose sky pen is 23.
        body.rect(0, 0, 320, 32, 23);
        let sky = rng.bounded(100);
        if u16::from(state.data[0x58a6 + usize::from(province)]) <= sky {
            let number = rng.bounded(4);
            body.blit(&assets.bitmap("MNEGO", &format!("SKY{number}.CBM"))?, 0, 0);
        }
        let backdrop = char::from(state.data[0x588a + usize::from(province)] + b'A');
        body.blit(
            &assets.bitmap("MNEGO", &format!("NEGBG{backdrop}.CBM"))?,
            0,
            0,
        );
        let letter = char::from(b'@' + province);
        body.blit(
            &assets.bitmap("MNEGO", &format!("NEGFG{letter}.CBM"))?,
            0,
            31,
        );
        body.rect(0, 63, 320, 1, 0);
        body.rect(0, 64, 320, 1, 8);
        body.point(0, 63, 116);
        body.point(0, 64, 116);
        body.point(1, 64, 116);
        body.center_text(assets, &state.province_name(province)?, 160, 65, 0)?;
        body.rect(117, 82, 191, 111, 26);
        let tabs = assets.sprites("MNEGO", "TAB")?;
        for tab in tabs.iter().take(4) {
            body.blit(tab, 113, 79);
        }
        let panorama = body.pixels.clone();
        for y in 0..66 {
            for x in 0..96 {
                body.pixels[(80 + y) * 320 + 9 + x] = panorama[y * 320 + 159 + x];
            }
        }
        body.blit(
            &assets.bitmap("MNEGO", &format!("LEADER{letter}.CBM"))?,
            8,
            79,
        );
        for (x, y, w, h, color) in [
            (9, 145, 96, 1, 0),
            (104, 80, 1, 66, 0),
            (10, 146, 96, 1, 8),
            (105, 81, 1, 66, 8),
        ] {
            body.rect(x, y, w, h, color);
        }
        let tier = usize::from(state.data[0x634e] / 34);
        let tier = if state.data[0x527e + base] as i8 > 75 && tier > 0 {
            tier - 1
        } else {
            tier
        };
        let at = 0x1918 + legion * 20;
        let strength = word(&state.data, at + 6)?.wrapping_add(word(&state.data, at + 8)?);
        let enemy =
            word(&state.data, 0x5270 + base)?.wrapping_add(word(&state.data, 0x5272 + base)?);
        state.data[at + 14] = 1;
        let mut scene = Self {
            cleopatra: if province == 2 {
                Some(crate::cleopatra::Cleopatra::new(assets)?)
            } else {
                None
            },
            province,
            legion,
            stage: Stage::Scout,
            choices: Vec::new(),
            response_ticks: 0,
            state: u8::from(
                state.data[0x5971] >= state.data[0x4092 + usize::from(province)]
                    && state.data[0x634e] >= state.data[0x40ae + usize::from(province)],
            ),
            next_state: 0,
            selected_tribute: 0,
            tier,
            weaker: if strength < enemy {
                usize::from(state.data[0x4322 + usize::from(province)])
            } else {
                0
            },
            text: assets.resource("MNEGO", &format!("TEXT{letter}.LZW"))?,
            body,
            text_y: 86,
            tabs,
            button: assets
                .sprites("MINIT", "INIT")?
                .into_iter()
                .nth(1)
                .ok_or("Missing menu button")?,
        };
        let scout = assets.resource("MNEGO", "SCOUT.LZW")?;
        scene.say(assets, &block(&scout, usize::from(province))?, 0, false)?;
        scene.blank(assets, 1)?;
        let wealth = usize::from(state.data[0x5284 + base] / 40);
        let army = usize::from(
            word(&state.data, 0x5272 + base)?
                .wrapping_mul(3)
                .wrapping_add(word(&state.data, 0x5270 + base)?)
                / 2500,
        );
        let courage = usize::from(state.data[0x527a + base] / 25);
        let description = format!(
            "{}{}{}{}{}{}{}{}",
            state.string(0x5eab)?,
            state.province_name(province)?,
            state.string(0x5eba)?,
            state.string(word(&state.data, 0x5f52 + wealth * 2)? as usize)?,
            state.string(0x5ebf)?,
            state.string(word(&state.data, 0x5f5a + army * 2)? as usize)?,
            state.string(0x5ece)?,
            state.string(word(&state.data, 0x5f62 + courage * 2)? as usize)?
        );
        scene.say(assets, &description, 0, false)?;
        Ok(scene)
    }

    fn blank(&mut self, assets: &Assets, count: usize) -> Result<()> {
        for _ in 0..count {
            self.say(assets, "", 0, false)?;
        }
        Ok(())
    }

    fn say(&mut self, assets: &Assets, text: &str, color: u8, quote: bool) -> Result<()> {
        let text = if quote {
            format!("\"{text}\"")
        } else {
            text.to_string()
        };
        let mut line = String::new();
        let mut words = text
            .split(' ')
            .filter(|word| !word.is_empty())
            .map(Some)
            .chain(std::iter::once(None));
        for word in &mut words {
            if word.is_none() || width(assets, &line)? + width(assets, word.unwrap())? > 186 {
                if self.text_y + 10 > 190 {
                    let before = self.body.pixels.clone();
                    for y in 86..181 {
                        for x in 120..306 {
                            self.body.pixels[y * 320 + x] = before[(y + 10) * 320 + x];
                        }
                    }
                    self.text_y -= 10;
                    self.body.rect(120, self.text_y, 186, 11, 26);
                }
                self.body.text(assets, &line, 120, self.text_y, color)?;
                self.text_y += 10;
                line.clear();
            }
            if let Some(word) = word {
                line.push_str(word);
                line.push(' ');
            }
        }
        Ok(())
    }

    fn greet(&mut self, assets: &Assets, state: &Campaign, rng: &mut OriginalRng) -> Result<()> {
        let text = variant(&block(&self.text, usize::from(self.state))?, rng);
        self.say(assets, &text, 15, true)?;
        self.refresh_choices(state)?;
        self.stage = Stage::Choice;
        Ok(())
    }

    fn refresh_choices(&mut self, state: &Campaign) -> Result<()> {
        self.choices.clear();
        let at = word(&state.data, 0x42ec + usize::from(self.province) * 2)? as usize
            + usize::from(self.state) * 3;
        for &id in &state.data[at..at + 3] {
            if id == 61 {
                break;
            }
            let label = usize::from(state.data[0x40e6 + usize::from(id)]);
            if label >= 17 {
                return Err("Invalid dialogue option".into());
            }
            self.choices.push(Choice {
                id,
                label: state.string(word(&state.data, 0x4028 + label * 2)? as usize)?,
                shortcut: state.data[0x404a + label].to_ascii_lowercase(),
            });
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        match self.stage {
            Stage::Scout => {
                self.blank(assets, 2)?;
                let leader = state
                    .string(word(&state.data, 0x5f1c + usize::from(self.province) * 2)? as usize)?;
                self.say(
                    assets,
                    &format!(
                        "{leader}{}{}{}",
                        state.string(0x5ef5)?,
                        state.province_name(self.province)?,
                        state.string(0x5f06)?
                    ),
                    0,
                    false,
                )?;
                self.greet(assets, state, rng)?;
            }
            Stage::Response => {
                if self.next_state == 2 {
                    self.accept_alliance(assets, state, rng, self.selected_tribute)?;
                } else if self.province == 2 && (17..=19).contains(&self.next_state) {
                    let text = variant(&block(&self.text, usize::from(self.next_state))?, rng);
                    self.say(assets, &text, 15, true)?;
                    self.blank(assets, 1)?;
                    self.state = self.next_state;
                    self.response_ticks = 0;
                    self.stage = Stage::CleoPause;
                } else if self.province == 2 && self.next_state == 21 {
                    self.blank(assets, 2)?;
                    self.choices.clear();
                    self.body.rect(8, 151, 100, 48, state.data[0x6344]);
                    self.cleopatra
                        .as_mut()
                        .unwrap()
                        .start(&mut self.body, state);
                    self.state = self.next_state;
                    self.stage = Stage::CleoAnimation;
                } else if self.province == 2 && self.next_state == 22 {
                    state.sound.stop();
                    self.stage = Stage::Complete;
                } else {
                    self.state = self.next_state;
                    self.greet(assets, state, rng)?;
                }
            }
            Stage::Alliance if self.province == 2 => {
                state.sound.play(8);
                self.choices.clear();
                self.cleopatra
                    .as_ref()
                    .unwrap()
                    .barge(&mut self.body, state);
                self.say(assets, &state.string(0x4dd4)?, 0, false)?;
                self.blank(assets, 1)?;
                self.say(assets, &state.string(0x4e28)?, 0, false)?;
                self.blank(assets, 1)?;
                self.stage = Stage::CleoInvitation;
            }
            Stage::CleoInvitation => {
                self.cleopatra.as_ref().unwrap().bedroom(&mut self.body);
                self.state = 15;
                self.greet(assets, state, rng)?;
            }
            Stage::CleoPause => {
                self.say(assets, &state.string(0x4d0c)?, 0, false)?;
                self.stage = Stage::CleoDismissed;
            }
            Stage::CleoAnimation => self.cleopatra.as_mut().unwrap().skip_lead_in(),
            Stage::CleoAfter => self.final_scene(assets, state)?,
            Stage::CleoFinal => self.cleopatra.as_mut().unwrap().queue_key(),
            _ => {}
        }
        Ok(())
    }
    fn final_scene(&mut self, _assets: &Assets, state: &Campaign) -> Result<()> {
        self.cleopatra
            .as_mut()
            .unwrap()
            .finish(&mut self.body, state);
        self.stage = Stage::CleoFinal;
        Ok(())
    }
    pub fn tick(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        mut count: u32,
    ) -> Result<()> {
        if self.stage == Stage::CleoAnimation {
            if let Some(left) = self
                .cleopatra
                .as_mut()
                .unwrap()
                .tick(&mut self.body, state, count)
            {
                self.stage = Stage::CleoAfter;
                self.response_ticks = 0;
                count = left;
                // The original BIOS queue is not read during the timed poses.
                // Its first pending key interrupts the following144-tick wait.
                if self.cleopatra.as_mut().unwrap().take_key() {
                    self.final_scene(assets, state)?;
                }
            } else {
                return Ok(());
            }
        }
        if self.stage == Stage::CleoFinal {
            if self.cleopatra.as_mut().unwrap().idle(rng, count) {
                self.stage = Stage::Complete;
            }
            return Ok(());
        }
        let wait = match self.stage {
            Stage::Response | Stage::Withdraw => 72,
            Stage::CleoPause | Stage::CleoAfter => 144,
            _ => return Ok(()),
        };
        let before = self.response_ticks;
        self.response_ticks = self.response_ticks.saturating_add(count);
        if self.response_ticks >= wait {
            self.advance(assets, state, rng)?;
            let remaining = count.saturating_sub(wait.saturating_sub(before));
            if self.stage == Stage::CleoAnimation && remaining > 0 {
                return self.tick(assets, state, rng, remaining);
            }
            if self.stage == Stage::CleoFinal
                && self.cleopatra.as_mut().unwrap().idle(rng, remaining)
            {
                self.stage = Stage::Complete;
            } else if self.stage == Stage::CleoPause && wait == 72 {
                self.response_ticks = remaining;
                if remaining >= 144 {
                    self.advance(assets, state, rng)?;
                }
            }
        }
        Ok(())
    }
    pub fn completed(&self) -> bool {
        matches!(
            self.stage,
            Stage::Withdraw | Stage::CleoDismissed | Stage::Complete
        ) || (self.stage == Stage::CleoFinal && self.cleopatra.as_ref().unwrap().pollable())
            || (self.stage == Stage::Alliance && self.province != 2)
    }
    pub fn allied(&self) -> bool {
        self.stage != Stage::Withdraw
    }

    // False leaves a combat choice at the pending boundary without consuming RNG.
    pub fn choose(&mut self, assets: &Assets, state: &Campaign, row: usize) -> Result<bool> {
        let choice = self.choices.get(row).ok_or("Invalid diplomacy selection")?;
        let id = usize::from(choice.id);
        let label = state.data[0x40e6 + id];
        if label == 1 {
            return Ok(false);
        }
        self.say(
            assets,
            &state.string(word(&state.data, 0x4c92 + id * 2)? as usize)?,
            9,
            true,
        )?;
        self.response_ticks = 0;
        if label == 0 {
            self.stage = Stage::Withdraw;
            return Ok(true);
        }
        self.next_state = if label >= 14 {
            if state.data[0x40ca + usize::from(self.province)] <= label - 14 {
                3
            } else {
                2
            }
        } else {
            let index =
                self.tier * usize::from(state.data[0x4322 + usize::from(self.province)]) * 2
                    + self.weaker
                    + usize::from(state.data[0x4124 + id]);
            let responses = word(&state.data, 0x455a + usize::from(self.province) * 2)? as usize;
            state.data[responses + index]
        };
        self.selected_tribute = (row + 1) as u8;
        self.stage = Stage::Response;
        Ok(true)
    }

    fn accept_alliance(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        tribute: u8,
    ) -> Result<()> {
        state.form_alliance(self.province, tribute)?;
        let text = variant(&block(&self.text, 2)?, rng);
        self.say(assets, &text, 15, true)?;
        self.blank(assets, 1)?;
        self.stage = Stage::Alliance;
        Ok(())
    }

    pub fn render(
        &self,
        assets: &Assets,
        state: &Campaign,
        selected: Option<usize>,
    ) -> Result<Frame> {
        let mut frame = self.body.clone();
        frame.acknowledgment_popup = false;
        if self.stage == Stage::Scout {
            // The portrait's saved region is restored only after the scouting
            // acknowledgment, when 6570 finishes assembling the leader panel.
            frame.rect(8, 79, 98, 68, 116);
        }
        if matches!(
            self.stage,
            Stage::Choice
                | Stage::Response
                | Stage::Alliance
                | Stage::Withdraw
                | Stage::CleoPause
                | Stage::CleoDismissed
        ) {
            frame.rect(10, 151, 94, 47, 26);
            for tab in self.tabs.iter().skip(4).take(4) {
                frame.blit(tab, 8, 151);
            }
            crate::province::decorate_menu(
                &mut frame,
                &self.button,
                state,
                &(0..self.choices.len())
                    .map(|row| {
                        let y = 155 + row as i32 * 13;
                        (15, y, 101, y + 10)
                    })
                    .collect::<Vec<_>>(),
                selected,
                true,
            );
            for (row, choice) in self.choices.iter().enumerate() {
                let y = 155 + row as i32 * 13;
                frame.text(
                    assets,
                    &choice.label,
                    26,
                    y,
                    if selected == Some(row) { 15 } else { 0 },
                )?;
            }
        }
        if self.stage == Stage::CleoFinal {
            self.cleopatra.as_ref().unwrap().overlay(&mut frame);
            frame.center_text(assets, &state.continue_footer()?, 212, 184, 4)?;
            frame.acknowledgment_popup = true;
        }
        if matches!(
            self.stage,
            Stage::Scout | Stage::Alliance | Stage::CleoInvitation | Stage::CleoDismissed
        ) {
            frame.center_text(assets, &state.continue_footer()?, 212, 183, 4)?;
            frame.acknowledgment_popup = true;
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(state: &mut Campaign, at: usize, value: u16) {
        state.data[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn alliance_caps_recruits_wraps_counter_and_defers_march() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        let base = 11 * 32;
        put(&mut state, 0xc106, 65535);
        put(&mut state, 0x5270 + base, 2000);
        put(&mut state, 0x5272 + base, 300);
        put(&mut state, 0x5274 + base, 700);
        put(&mut state, 0x5276 + base, 2500);
        state.data[0x5289 + base] = 8;
        let before = state.data.clone();
        state.form_alliance(11, 3).unwrap();
        assert_eq!(word(&state.data, 0x5274 + base).unwrap(), 2500);
        assert_eq!(word(&state.data, 0xc106).unwrap(), 0);
        assert_eq!(&state.data[0x5282 + base..0x5284 + base], &[3, 3]);
        assert_eq!(state.data[0x5285 + base], 2);
        assert_eq!(state.data[0x5289 + base], 0);
        assert_eq!(&state.data[0x1918..0x192c], &before[0x1918..0x192c]);
        assert_eq!(&state.data[0x594a..0x594c], &before[0x594a..0x594c]);
        assert!(state.form_alliance(11, 4).is_err());
    }

    #[test]
    fn acknowledged_alliance_caps_score_lands_and_removes_only_its_enemy_fleets() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        state.data[0x1924] = 31;
        state.data[0x1926] = 1;
        state.data[0x1929] = 0;
        state.data[0x1114] = 31;
        state.data[0x1117] = 0;
        state.data[0x1139] = 11;
        state.data[0x1145] = 13;
        state.data[0x5285 + 11 * 32] = 2;
        put(&mut state, 0x594a, 254);
        state.finish_alliance_march(0, 11).unwrap();
        assert_eq!(word(&state.data, 0x594a).unwrap(), 256);
        assert_eq!(state.data[0x1924], 11);
        assert_eq!(state.data[0x1926], 1);
        assert_eq!(state.data[0x1929], 255);
        assert_eq!(state.data[0x1114], 31);
        assert_eq!(state.data[0x1117], 255);
        assert_eq!(state.data[0x113a], 255);
        assert_eq!(state.data[0x1146], 0);
    }

    #[test]
    fn quotes_consume_one_rng_call_even_with_fewer_variants() {
        for seed in 0..32 {
            let mut rng = OriginalRng {
                first: seed,
                second: 99,
            };
            let mut expected = rng;
            let choice = usize::from(expected.bounded(3));
            assert_eq!(
                variant("one~two~three", &mut rng),
                ["one", "two", "three"][choice]
            );
            assert_eq!((rng.first, rng.second), (expected.first, expected.second));
            let mut rng = OriginalRng {
                first: seed,
                second: 99,
            };
            assert_eq!(variant("one", &mut rng), "one");
            assert_eq!((rng.first, rng.second), (expected.first, expected.second));
        }
    }
}
