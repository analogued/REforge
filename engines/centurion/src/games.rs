// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Resident games/festivals entry points (3342/3457), separate from each sport.
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::province::{Menu, Panels};
use crate::rng::OriginalRng;
use crate::session::Input;
use crate::state::Campaign;

pub(crate) fn put(state: &mut Campaign, at: usize, value: u16) {
    state.data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

/// Payment precedes the report; the unrest/event updates follow acknowledgment.
fn festival(state: &mut Campaign, province: u8, choice: usize) {
    let at = 0x527e + usize::from(province) * 32;
    let anger = i16::from(state.data[at] as i8);
    let reduced = match choice {
        0 => anger - 10,
        1 => anger * 3 / 4,
        2 => anger / 2,
        _ => anger / 4,
    };
    state.data[at] = reduced.max(1) as u8;
    let count = u16::from_le_bytes(state.data[0xc100..0xc102].try_into().unwrap());
    put(state, 0xc100, count.wrapping_add(1));
    state.data[0xed5] = (state.data[0xed5] as i8).min(10) as u8;
    if state.data[0xed6] == province {
        put(state, 0xeda, 0);
        state.data[0xed6] = 0;
    }
}

pub struct Games {
    background: Frame,
    province: u8,
    menu: Menu,
    selected: Option<usize>,
    stage: Stage,
    arena: Option<crate::arena::Arena>,
    race: Option<crate::race::Race>,
}

enum Stage {
    Menu,
    Amphitheatre,
    Message(String, Option<usize>),
    Arena,
    Race,
    Done,
}

impl Games {
    pub fn new(state: &Campaign, province: u8, background: Frame) -> Result<Self> {
        let stage = if province != 0 && word(&state.data, 0x634a)? < u16::from(state.data[0x15d4]) {
            Stage::Message(state.string(word(&state.data, 0x15d8)? as usize)?, None)
        } else {
            Stage::Menu
        };
        Ok(Self {
            background,
            province,
            menu: Menu::read(state, if province == 0 { 0x15c2 } else { 0x15b0 })?,
            selected: None,
            stage,
            arena: None,
            race: None,
        })
    }

    pub fn diagnostic(&self) -> String {
        self.race
            .as_ref()
            .map(|s| s.diagnostic())
            .or_else(|| self.arena.as_ref().map(|s| s.diagnostic()))
            .unwrap_or_default()
    }
    pub fn race_field_frame(&self) -> Option<&Frame> {
        self.race.as_ref().and_then(|r| r.field_frame())
    }
    #[cfg(feature = "sdl")]
    pub fn relative_steering(&self) -> bool {
        self.race.as_ref().is_some_and(|r| r.relative_steering())
    }
    pub fn has_race(&self) -> bool {
        self.race.is_some()
    }
    pub fn moves_cursor(&self) -> bool {
        self.arena.as_ref().is_some_and(|a| a.moves_cursor())
    }
    pub fn cursor_visible(&self) -> bool {
        match self.stage {
            Stage::Menu | Stage::Amphitheatre => true,
            Stage::Arena => self.arena.as_ref().is_some_and(|s| s.cursor_visible()),
            Stage::Race => self.race.as_ref().is_some_and(|s| s.cursor_visible()),
            _ => false,
        }
    }

    pub fn input(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        panels: &Panels,
        input: Input,
    ) -> Result<bool> {
        if matches!(self.stage, Stage::Race) {
            if self
                .race
                .as_mut()
                .unwrap()
                .input(assets, state, rng, input)?
            {
                self.stage = Stage::Done;
            }
            return Ok(matches!(self.stage, Stage::Done));
        }
        if matches!(self.stage, Stage::Arena) {
            if self
                .arena
                .as_mut()
                .unwrap()
                .input(assets, state, rng, input)?
            {
                self.stage = Stage::Done;
            }
            return Ok(matches!(self.stage, Stage::Done));
        }
        if matches!(
            input,
            Input::Tick(_)
                | Input::Mouse(..)
                | Input::Release(..)
                | Input::ReleaseKey(_)
                | Input::Random(_)
        ) {
            return Ok(false);
        }
        match &self.stage {
            Stage::Message(_, effect) => {
                if let Some(choice) = effect {
                    festival(state, self.province, *choice);
                }
                self.stage = Stage::Done;
            }
            Stage::Amphitheatre => {
                let answer = match input {
                    Input::Shortcut(b'y') => Some(true),
                    Input::Shortcut(b'n') | Input::Escape => Some(false),
                    Input::Enter | Input::Space => Some(self.selected == Some(0)),
                    Input::Left | Input::Right | Input::Up | Input::Down => {
                        self.selected = Some(1 - self.selected.unwrap_or(1));
                        None
                    }
                    Input::Click(x, y) if (129..=188).contains(&x) && (105..=116).contains(&y) => {
                        Some(x < 160)
                    }
                    _ => None,
                };
                if let Some(yes) = answer {
                    if yes {
                        put(state, 0x634a, word(&state.data, 0x634a)? - 50);
                        put(state, 0x15d2, 0xffff);
                        put(state, 0x154a, 0xffff);
                        self.start_arena(assets, state)?;
                    } else {
                        self.stage = Stage::Done;
                    }
                }
            }
            Stage::Menu => {
                if matches!(input, Input::Escape) {
                    self.stage = Stage::Done;
                } else if let Some(row) = menu_input(&self.menu, &mut self.selected, &input) {
                    self.activate(assets, state, rng, panels, row)?;
                }
            }
            _ => {}
        }
        Ok(matches!(self.stage, Stage::Done))
    }

    fn activate(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        panels: &Panels,
        row: usize,
    ) -> Result<()> {
        self.selected = Some(row);
        let last = self.menu.items.len() - 1;
        if row == last {
            self.stage = Stage::Done;
        } else if self.province != 0 {
            let cost = u16::from(state.data[0x15d4 + row]);
            let treasury = word(&state.data, 0x634a)?;
            self.stage = if treasury < cost {
                Stage::Message(state.string(word(&state.data, 0x15d8)? as usize)?, None)
            } else {
                put(state, 0x634a, treasury - cost);
                Stage::Message(
                    state.string(word(&state.data, 0x15da)? as usize)?,
                    Some(row),
                )
            };
        } else if row == 1 {
            let built = word(&state.data, 0x154a)? != 0;
            if !built {
                if word(&state.data, 0x634a)? < 50 {
                    self.stage = Stage::Message(state.string(0x161e)?, None);
                } else {
                    // Retain the selected label beneath the purchase question.
                    panels.draw_menu(assets, state, &mut self.background, &self.menu, Some(row))?;
                    panels.deactivate_menu(assets, state, &mut self.background, 0x15c2)?;
                    self.stage = Stage::Amphitheatre;
                    self.selected = None;
                }
            } else if word(&state.data, 0x154c)? == word(&state.data, 0x634c)? {
                self.stage = Stage::Message(state.string(0x167a)?, None);
            } else {
                self.start_arena(assets, state)?;
            }
        } else {
            // The race implementation supplies its own entry flow.
            let text = if word(&state.data, 0x154e)? == word(&state.data, 0x634c)? {
                state.string(0x15dc)?
            } else if word(&state.data, 0x634a)? < 5 {
                state.string(0x15f3)?
            } else {
                self.race = Some(crate::race::Race::new(assets, state, rng)?);
                self.stage = Stage::Race;
                return Ok(());
            };
            self.stage = Stage::Message(text, None);
        }
        // Resident 585B deactivates the choice under a following report.
        if matches!(self.stage, Stage::Message(..)) {
            panels.draw_menu(assets, state, &mut self.background, &self.menu, None)?;
        }
        Ok(())
    }

    fn start_arena(&mut self, assets: &Assets, state: &Campaign) -> Result<()> {
        self.arena = Some(crate::arena::Arena::new(assets, state)?);
        self.stage = Stage::Arena;
        Ok(())
    }

    pub fn render(&self, assets: &Assets, state: &Campaign, panels: &Panels) -> Result<Frame> {
        let mut frame = self.background.clone();
        match &self.stage {
            Stage::Menu => {
                panels.draw_menu(assets, state, &mut frame, &self.menu, self.selected)?
            }
            Stage::Amphitheatre => panels.confirm(
                assets,
                state,
                &mut frame,
                &state.string(0x164e)?,
                self.selected,
            )?,
            Stage::Message(text, _) => crate::legion::message(&mut frame, assets, state, text)?,
            Stage::Arena => return self.arena.as_ref().unwrap().render(assets, state, panels),
            Stage::Race => return self.race.as_ref().unwrap().render(assets, state, panels),
            Stage::Done => {}
        }
        Ok(frame)
    }
}

pub(crate) fn menu_input(
    menu: &Menu,
    selected: &mut Option<usize>,
    input: &Input,
) -> Option<usize> {
    match input {
        Input::Up | Input::Down => {
            let length = menu.items.len();
            let start = selected.unwrap_or(0);
            let backwards = matches!(input, Input::Up);
            for distance in 1..=length {
                let row = if backwards {
                    (start + length - distance) % length
                } else {
                    (start + distance) % length
                };
                if !menu.items[row].disabled {
                    *selected = Some(row);
                    break;
                }
            }
            None
        }
        Input::Enter | Input::Space => {
            Some(selected.unwrap_or(0)).filter(|&r| !menu.items[r].disabled)
        }
        Input::Click(x, y) => menu.hit(*x, *y),
        Input::Shortcut(key) => menu
            .items
            .iter()
            .position(|i| !i.disabled && i.shortcut == key.to_ascii_lowercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn festivals_keep_signed_unrest_and_only_clear_the_selected_event() {
        for (anger, choice, result) in [
            (100u8, 0, 90),
            (101, 1, 75),
            (101, 2, 50),
            (101, 3, 25),
            (0xff, 0, 1),
        ] {
            let mut c = Campaign::test_data(vec![0; 65536]);
            c.data[0x527e + 32] = anger;
            c.data[0xed5] = 30;
            c.data[0xed6] = 1;
            put(&mut c, 0xeda, 100);
            put(&mut c, 0xc100, 65535);
            festival(&mut c, 1, choice);
            assert_eq!(c.data[0x527e + 32], result);
            assert_eq!(c.data[0xed5], 10);
            assert_eq!(c.data[0xed6], 0);
            assert_eq!(word(&c.data, 0xeda).unwrap(), 0);
            assert_eq!(word(&c.data, 0xc100).unwrap(), 0);
        }
    }
}
