// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Chariot race selection, preparation and campaign settlement (overlay 4).
use crate::assets::{Assets, Bitmap, Result, VideoMode, word};
use crate::games::{menu_focus, menu_input, put};
use crate::graphics::Frame;
use crate::province::{Menu, Panels};
use crate::rng::OriginalRng;
use crate::session::Input;
use crate::state::{Campaign, ContinueInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Type,
    Selected,
    Prepare,
    Dirty,
    Bet,
    ConfirmExit,
    ConfirmNoBet,
    Prompt,
    Gallery,
    Racing,
    Crash,
    Victory,
    Result,
    Done,
}
#[derive(Clone, Copy)]
enum Effect {
    None,
    Physician,
    Gods,
}

pub struct Race {
    stage: Stage,
    resume: Stage,
    message: String,
    effect: Effect,
    selected: Option<usize>,
    chosen: bool,
    menu: Menu,
    menu_focus: [Option<usize>; 4],
    circus: Bitmap,
    sprites: Vec<Bitmap>,
    dirty_budget: Option<u16>,
    dirty_spent: u16,
    prompt_money: Option<u16>,
    gallery: Option<crate::race_gallery::Gallery>,
    motion: crate::race_motion::Motion,
    view: Option<crate::race_render::View>,
    elapsed: u32,
    held: u8,
    mouse: (i32, i32),
    mouse_previous: (i32, i32),
    buttons: u8,
    standing: [usize; 4],
    position: usize,
    crash: Option<crate::race_crash::Crash>,
    parade: Option<crate::race_gallery::Parade>,
}
impl Race {
    pub fn new(assets: &Assets, state: &mut Campaign, rng: &mut OriginalRng) -> Result<Self> {
        state.data[0xc350] = 255;
        crate::race_motion::Motion::initialize_models(state)?;
        for at in (0xaa78..0xaab4).step_by(6) {
            put(state, at, rng.bounded(77) * 4 + 5);
            put(state, at + 2, rng.bounded(109) + 45);
            put(state, at + 4, rng.bounded(65535));
        }
        put(state, 0xc322, 0);
        let year = word(&state.data, 0x634c)?;
        for id in 0..4 {
            if word(&state.data, 0x892e)? != year {
                let at = 0xc2c2 + id * 18;
                state.data[at + 3] = rng.bounded(27) as u8;
                state.data[at + 4] = rng.bounded(3) as u8;
                let style = rng.bounded(4) as u8;
                state.data[0xc368 + id] = if style == 3 { 1 } else { style };
                state.data[0xaac2 + id] = state.data[0x8ba6]
                    .wrapping_add(rng.bounded(11) as u8)
                    .wrapping_sub(5);
            }
            put(state, 0xc2c2 + id * 18, 65535);
            put(state, 0xc324 + id * 2, 0);
        }
        put(state, 0x892e, year);
        state.data[0xc2fb] = 0;
        let menu = Menu::read(state, 0x88e6)?;
        let selected = menu_focus(&menu, state, None);
        Ok(Self {
            stage: Stage::Type,
            resume: Stage::Type,
            message: String::new(),
            effect: Effect::None,
            selected,
            chosen: false,
            menu,
            menu_focus: [None; 4],
            circus: assets.bitmap("MRACE", "CIRCUS.CBM")?,
            sprites: assets.sprites("MRACE", "RACE")?,
            dirty_budget: None,
            dirty_spent: 0,
            prompt_money: None,
            gallery: None,
            motion: crate::race_motion::Motion::default(),
            view: None,
            elapsed: 0,
            held: 0,
            mouse: (160, 100),
            mouse_previous: (160, 100),
            buttons: 0,
            standing: [0, 1, 2, 3],
            position: 0,
            crash: None,
            parade: None,
        })
    }
    #[cfg(feature = "sdl")]
    pub fn relative_steering(&self) -> bool {
        self.stage == Stage::Racing
    }
    pub fn field_frame(&self) -> Option<&Frame> {
        self.view.as_ref().map(|v| &v.frame)
    }
    pub fn diagnostic(&self) -> String {
        format!(
            "race={:?}{}",
            self.stage,
            self.crash
                .as_ref()
                .map(|c| format!(" crash_variant={}", c.variant()))
                .unwrap_or_default()
        )
    }
    pub fn cursor_visible(&self) -> bool {
        matches!(
            self.stage,
            Stage::Type
                | Stage::Prepare
                | Stage::Dirty
                | Stage::Bet
                | Stage::ConfirmExit
                | Stage::ConfirmNoBet
        )
    }
    fn menu_index(stage: Stage) -> Option<usize> {
        match stage {
            Stage::Type => Some(0),
            Stage::Prepare => Some(1),
            Stage::Dirty => Some(2),
            Stage::Bet => Some(3),
            _ => None,
        }
    }
    fn remember_menu_focus(&mut self) {
        if let Some(index) = Self::menu_index(self.stage) {
            self.menu_focus[index] = self.selected;
        }
    }
    fn confirm(&mut self, state: &Campaign, stage: Stage) {
        self.remember_menu_focus();
        self.stage = stage;
        // Enter previously chose No with no focus. Make that default visible.
        self.selected = (state.continue_input == ContinueInput::Keyboard).then_some(1);
    }
    fn menu(&mut self, state: &Campaign, stage: Stage) -> Result<()> {
        let returning = matches!(
            self.stage,
            Stage::Prompt | Stage::ConfirmExit | Stage::ConfirmNoBet
        ) || matches!(self.stage, Stage::Dirty | Stage::Bet)
            && stage == Stage::Prepare;
        self.remember_menu_focus();
        self.stage = stage;
        self.prompt_money = None;
        self.menu = Menu::read(
            state,
            match stage {
                Stage::Type => 0x88e6,
                Stage::Bet => 0x890a,
                Stage::Dirty => 0x891c,
                _ => 0x88f8,
            },
        )?;
        let previous = if returning {
            self.menu_focus[Self::menu_index(stage).unwrap()]
        } else {
            None
        };
        self.selected = menu_focus(&self.menu, state, previous);
        Ok(())
    }
    fn prompt(&mut self, text: String, resume: Stage, effect: Effect) {
        self.remember_menu_focus();
        self.message = text;
        self.resume = resume;
        self.effect = effect;
        self.stage = Stage::Prompt;
    }
    fn setup(&mut self, state: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        let lane = rng.bounded(4);
        for id in 0..4 {
            let at = 0xc2c2 + id * 18;
            let kind = usize::from(state.data[at + 4]);
            crate::race_motion::Motion::chariot(state, id, kind);
            state.data[at + 5] = if id == 3 { 100 } else { 110 };
            state.data[at + 6] = state.data[at + 5] / 3;
            state.data[at + 7] = state.data[at + 6];
            state.data[at + 8] = 0;
            state.data[at + 9] = 0;
            state.data[at + 10] = 0;
            state.data[at + 11] = state.data[0x87fa + kind];
            state.data[at + 12] = state.data[0x87e6 + kind];
            state.data[at + 2] = state.data[0xc350];
            state.data[at + 13] = 0;
            put(state, at - 4, 0x4074);
            put(state, 0xc31a + id * 2, 65535);
            put(state, 0xc334 + id * 4, ((lane + id as u16) & 3) * 70);
            put(state, 0xc336 + id * 4, 100);
        }
        put(state, 0xaabc, word(&state.data, 0x634a)?);
        state.data[0xaaba] = 0;
        put(state, 0xaab6, 0);
        put(state, 0xaab8, 0);
        put(state, 0xc366, 0);
        self.menu(state, Stage::Prepare)
    }
    fn available(&self, state: &Campaign) -> Result<u16> {
        let total = word(&state.data, 0x634a)?;
        Ok(if self.dirty_budget.is_some() {
            total
        } else {
            total.wrapping_sub(word(&state.data, 0xc366)?)
        })
    }
    fn leave_dirty(&mut self, state: &mut Campaign) -> Result<()> {
        put(state, 0xaabc, word(&state.data, 0x634a)?);
        put(
            state,
            0x634a,
            self.dirty_budget
                .take()
                .ok_or("Missing race preparation budget")?
                .wrapping_sub(self.dirty_spent),
        );
        self.menu(state, Stage::Prepare)
    }
    fn dirty(&mut self, state: &mut Campaign, rng: &mut OriginalRng, row: usize) -> Result<()> {
        if row == 3 {
            return self.leave_dirty(state);
        }
        let display_money = self.available(state)?;
        let cost = [10, 30, 50][row];
        if word(&state.data, 0x634a)? < cost {
            self.prompt(
                state.string(word(&state.data, 0x880a)? as usize)?,
                Stage::Dirty,
                Effect::None,
            );
        } else if (row == 0 && state.data[0xaaba] >= 3)
            || (row != 0 && word(&state.data, if row == 1 { 0xaab6 } else { 0xaab8 })? != 0)
        {
            self.prompt(
                state.string(if row == 0 {
                    0x8930
                } else {
                    word(&state.data, 0x8824)? as usize
                })?,
                Stage::Dirty,
                Effect::None,
            );
        } else {
            put(state, 0x634a, word(&state.data, 0x634a)? - cost);
            self.dirty_spent += cost;
            let (text, effect) = match row {
                0 => {
                    let id = usize::from(state.data[0xaaba]);
                    let text = format!(
                        "{}{}",
                        state.string(word(&state.data, 0x8818 + id * 2)? as usize)?,
                        state.string(word(&state.data, 0x881e)? as usize)?
                    );
                    state.data[0xc2c7 + id * 18] =
                        state.data[0xc2c7 + id * 18].wrapping_sub(rng.bounded(10) as u8);
                    state.data[0xaaba] += 1;
                    (text, Effect::None)
                }
                1 => {
                    put(state, 0xaab6, 65535);
                    (
                        state.string(word(&state.data, 0x8820)? as usize)?,
                        Effect::Physician,
                    )
                }
                _ => {
                    put(state, 0xaab8, 65535);
                    (
                        state.string(word(&state.data, 0x8822)? as usize)?,
                        Effect::Gods,
                    )
                }
            };
            self.prompt(text, Stage::Dirty, effect);
            self.prompt_money = Some(display_money);
        }
        Ok(())
    }
    pub fn input(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        input: Input,
    ) -> Result<bool> {
        if let Input::RaceStack(value) = input {
            self.motion.observed_leader = Some(value);
            return Ok(false);
        }
        if let Input::Tick(count) = input {
            if self.stage == Stage::Gallery {
                self.gallery.as_mut().unwrap().tick(state, rng, count)?;
            } else if self.stage == Stage::Victory {
                if self.parade.as_mut().unwrap().tick(state, rng, count)? {
                    self.settle_finish(state)?;
                }
            } else if self.stage == Stage::Crash {
                let done = self
                    .crash
                    .as_mut()
                    .unwrap()
                    .tick(state, &mut self.motion, count)?;
                self.view.as_mut().unwrap().update(state, &self.motion)?;
                if done {
                    put(state, 0xc32c, 65535);
                    self.finish(state, rng)?;
                }
            } else if self.stage == Stage::Racing {
                self.elapsed = self.elapsed.saturating_add(count);
                while self.elapsed >= 7 && self.stage == Stage::Racing {
                    self.elapsed -= 7;
                    let command = self.mouse_command();
                    let done = self.motion.step(state, rng, self.held, command)?;
                    if self.motion.crash {
                        self.motion.crash_presentation = true;
                    }
                    self.view.as_mut().unwrap().update(state, &self.motion)?;
                    if self.motion.crash {
                        self.crash = Some(crate::race_crash::Crash::new(state, rng));
                        put(state, 0xc32c, 0);
                        state.sound.stop();
                        self.stage = Stage::Crash;
                    } else if done {
                        self.finish(state, rng)?;
                    }
                }
                if self.stage == Stage::Crash && self.elapsed > 0 {
                    let remaining = std::mem::take(&mut self.elapsed);
                    let done =
                        self.crash
                            .as_mut()
                            .unwrap()
                            .tick(state, &mut self.motion, remaining)?;
                    self.view.as_mut().unwrap().update(state, &self.motion)?;
                    if done {
                        put(state, 0xc32c, 65535);
                        self.finish(state, rng)?;
                    }
                }
            }
            return Ok(self.stage == Stage::Done);
        }
        if self.stage == Stage::Crash {
            if matches!(input, Input::Escape) {
                self.crash.as_mut().unwrap().clear(state, &mut self.motion);
                self.view.as_mut().unwrap().update(state, &self.motion)?;
                put(state, 0xc32c, 65535);
                self.finish(state, rng)?;
            }
            return Ok(false);
        }
        if self.stage == Stage::Racing {
            match input {
                Input::Up => self.held |= 8,
                Input::Down => self.held |= 4,
                Input::Left => self.held |= 2,
                Input::Right => self.held |= 1,
                Input::ReleaseKey(bits) => {
                    self.held &= !bits;
                    if bits == 15 {
                        self.buttons = 0;
                        self.mouse_previous = self.mouse;
                    }
                }
                Input::MouseDelta(x, y) => {
                    self.mouse.0 = self.mouse.0.saturating_add(x);
                    self.mouse.1 = self.mouse.1.saturating_add(y);
                }
                Input::ReleaseButton(mask) => self.buttons &= !mask,
                Input::Mouse(x, y) => self.mouse = (x, y),
                Input::Click(x, y) => {
                    self.mouse = (x, y);
                    self.mouse_previous = self.mouse;
                    self.buttons |= 1;
                }
                Input::RightClick(x, y) => {
                    self.mouse = (x, y);
                    self.mouse_previous = self.mouse;
                    self.buttons |= 2;
                }
                Input::Release(..) => self.buttons = 0,
                _ => {}
            }
            return Ok(false);
        }
        if let Input::Mouse(x, y) = input {
            match self.stage {
                Stage::Type | Stage::Prepare | Stage::Dirty | Stage::Bet => {
                    menu_input(&self.menu, &mut self.selected, &input);
                }
                Stage::ConfirmExit | Stage::ConfirmNoBet => {
                    self.selected = crate::province::confirmation_hit(
                        &state.string(if self.stage == Stage::ConfirmExit {
                            0x84ce
                        } else {
                            0x87ba
                        })?,
                        x,
                        y,
                    );
                }
                _ => {}
            }
            return Ok(false);
        }
        if matches!(
            input,
            Input::Tick(_)
                | Input::MouseDelta(..)
                | Input::ReleaseButton(_)
                | Input::Release(..)
                | Input::ReleaseKey(_)
                | Input::Random(_)
        ) {
            return Ok(false);
        }
        match self.stage {
            Stage::Selected => self.setup(state, rng)?,
            Stage::Prompt => {
                match self.effect {
                    Effect::Physician => {
                        state.data[0xc2fd] = state.data[0xc2fd].wrapping_add(rng.bounded(10) as u8)
                    }
                    Effect::Gods => {
                        state.data[0xc304] =
                            state.data[0xc304].wrapping_add(10 + rng.bounded(10) as u8)
                    }
                    Effect::None => {}
                }
                self.menu(state, self.resume)?;
            }
            Stage::ConfirmExit | Stage::ConfirmNoBet => {
                let answer = match input {
                    Input::Shortcut(b'y') => Some(true),
                    Input::Shortcut(b'n') | Input::Escape => Some(false),
                    Input::Enter | Input::Space => Some(self.selected == Some(0)),
                    Input::Up | Input::Down | Input::Left | Input::Right => {
                        self.selected = Some(match self.selected {
                            Some(0) => 1,
                            _ => 0,
                        });
                        None
                    }
                    Input::Click(x, y) => crate::province::confirmation_hit(
                        &state.string(if self.stage == Stage::ConfirmExit {
                            0x84ce
                        } else {
                            0x87ba
                        })?,
                        x,
                        y,
                    )
                    .map(|row| row == 0),
                    _ => None,
                };
                if let Some(yes) = answer {
                    if self.stage == Stage::ConfirmExit {
                        if yes {
                            self.stage = Stage::Done;
                        } else {
                            self.menu(
                                state,
                                if self.chosen {
                                    Stage::Prepare
                                } else {
                                    Stage::Type
                                },
                            )?;
                        }
                    } else if yes {
                        self.start(assets, state, rng)?;
                    } else {
                        self.menu(state, Stage::Prepare)?;
                    }
                }
            }
            Stage::Type | Stage::Prepare | Stage::Dirty | Stage::Bet => {
                if matches!(input, Input::Escape) {
                    match self.stage {
                        Stage::Dirty => self.leave_dirty(state)?,
                        Stage::Bet => self.menu(state, Stage::Prepare)?,
                        _ => {
                            self.confirm(state, Stage::ConfirmExit);
                        }
                    }
                } else if let Some(row) = menu_input(&self.menu, &mut self.selected, &input) {
                    self.selected = Some(row);
                    self.remember_menu_focus();
                    match self.stage {
                        Stage::Type => {
                            if row == 3 {
                                self.confirm(state, Stage::ConfirmExit);
                            } else if word(&state.data, 0x634a)?
                                < u16::from(state.data[0x87f2 + row])
                            {
                                self.prompt(
                                    state.string(word(&state.data, 0x880a)? as usize)?,
                                    Stage::Type,
                                    Effect::None,
                                );
                            } else {
                                state.data[0xc2fc] = row as u8;
                                put(
                                    state,
                                    0x634a,
                                    word(&state.data, 0x634a)?
                                        - u16::from(state.data[0x87f2 + row]),
                                );
                                self.chosen = true;
                                self.stage = Stage::Selected;
                            }
                        }
                        Stage::Prepare => match row {
                            0 => {
                                if word(&state.data, 0xc366)? == 0 && self.available(state)? != 0 {
                                    self.confirm(state, Stage::ConfirmNoBet);
                                } else {
                                    self.start(assets, state, rng)?;
                                }
                            }
                            1 => {
                                let total = word(&state.data, 0x634a)?;
                                put(state, 0x634a, self.available(state)?);
                                self.dirty_budget = Some(total);
                                self.dirty_spent = 0;
                                self.menu(state, Stage::Dirty)?;
                            }
                            2 => self.menu(state, Stage::Bet)?,
                            _ => {
                                self.confirm(state, Stage::ConfirmExit);
                            }
                        },
                        Stage::Dirty => self.dirty(state, rng, row)?,
                        Stage::Bet => {
                            let total = word(&state.data, 0x634a)?.min(100);
                            let bet = word(&state.data, 0xc366)?;
                            match row {
                                0 => put(state, 0xc366, bet.saturating_add(10).min(total)),
                                1 => put(state, 0xc366, bet.saturating_sub(10)),
                                2 => put(state, 0xc366, total),
                                _ => self.menu(state, Stage::Prepare)?,
                            }
                        }
                        _ => {}
                    }
                }
            }
            Stage::Gallery => self.begin_race(assets, state)?,
            Stage::Result => self.settle(assets, state, rng)?,
            Stage::Victory => self.settle_finish(state)?,
            _ => {}
        }
        Ok(self.stage == Stage::Done)
    }
    fn start(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        _rng: &mut OriginalRng,
    ) -> Result<()> {
        put(
            state,
            0x634a,
            word(&state.data, 0x634a)?.wrapping_sub(word(&state.data, 0xc366)?),
        );
        state.sound.play(2);
        self.gallery = Some(crate::race_gallery::Gallery::new(assets)?);
        self.stage = Stage::Gallery;
        Ok(())
    }
    fn mouse_command(&mut self) -> u8 {
        let (dx, dy) = (
            self.mouse.0 - self.mouse_previous.0,
            self.mouse.1 - self.mouse_previous.1,
        );
        self.mouse_previous = self.mouse;
        if self.buttons == 3 {
            1
        } else if self.buttons & 1 != 0 {
            2
        } else if self.buttons & 2 != 0 {
            3
        } else if dx.abs() > dy.abs() && dx.abs() > 2 {
            if dx < 0 { 9 } else { 10 }
        } else if dy.abs() > 2 {
            if dy < 0 { 0 } else { 8 }
        } else {
            0
        }
    }
    fn begin_race(&mut self, assets: &Assets, state: &mut Campaign) -> Result<()> {
        state.sound.stop();
        state.sound.play(3);
        self.gallery = None;
        self.view = Some(crate::race_render::View::new(assets, state)?);
        put(state, 0xc100, word(&state.data, 0xc100)?.wrapping_add(1));
        put(state, 0xc102, word(&state.data, 0xc102)?.wrapping_add(1));
        put(state, 0x154e, word(&state.data, 0x634c)?);
        for at in [0xc32c, 0xc32e, 0xc36c, 0xc330, 0xc332] {
            put(state, at, 0);
        }
        self.elapsed = 0;
        self.held = 0;
        self.buttons = 0;
        self.stage = Stage::Racing;
        Ok(())
    }
    fn finish(&mut self, state: &Campaign, rng: &mut OriginalRng) -> Result<()> {
        state.sound.stop();
        self.standing = [0, 1, 2, 3];
        for i in 0..3 {
            for j in (i + 1..=3).rev() {
                let a = self.standing[i];
                let b = self.standing[j];
                let ta = word(&state.data, 0xc31a + a * 2)?;
                let tb = word(&state.data, 0xc31a + b * 2)?;
                let pa = word(&state.data, 0xc2be + a * 18)?;
                let pb = word(&state.data, 0xc2be + b * 18)?;
                if ta > tb || (ta == tb && (pb > pa || (pa == pb && rng.bounded(1) == 0))) {
                    self.standing.swap(i, j);
                }
            }
        }
        self.position = self.standing.iter().position(|&id| id == 3).unwrap();
        self.message = state.string(word(
            &state.data,
            if word(&state.data, 0xc320)? == 65535 {
                0x8816
            } else {
                0x880c + self.position * 2
            },
        )? as usize)?;
        self.stage = Stage::Result;
        Ok(())
    }
    fn settle(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        let finished = self.rewards(state)?;
        if finished && self.position == 0 {
            self.parade = Some(crate::race_gallery::Parade::new(assets, state, rng, false)?);
            self.stage = Stage::Victory;
            return Ok(());
        }
        self.settle_finish(state)
    }
    fn rewards(&self, state: &mut Campaign) -> Result<bool> {
        let kind = usize::from(state.data[0xc2fc]);
        let finished = word(&state.data, 0xc320)? != 65535;
        if finished {
            match self.position {
                0 => {
                    state.change_score(10)?;
                    state.data[0x634e] = state.data[0x634e].saturating_add(1).min(100);
                    let prize = (u16::from(state.data[0x87ee + kind]) + 1)
                        .wrapping_mul(word(&state.data, 0xc366)?);
                    put(
                        state,
                        0x634a,
                        word(&state.data, 0x634a)?.wrapping_add(prize),
                    );
                    let old = u32::from(word(&state.data, 0xc096)?)
                        | (u32::from(word(&state.data, 0xc098)?) << 16);
                    state.data[0xc096..0xc09a]
                        .copy_from_slice(&old.wrapping_add(u32::from(prize)).to_le_bytes());
                    if state.data[0x87ee + kind] > 1 {
                        state.data[0x87ee + kind] -= 1;
                    }
                    put(state, 0xc104, word(&state.data, 0xc104)?.wrapping_add(1));
                }
                1 => state.change_score(5)?,
                2 => state.change_score(1)?,
                _ => {}
            }
            if self.position >= 2 && state.data[0x87ee + kind] < 5 {
                state.data[0x87ee + kind] += 1;
            }
        } else if state.data[0x87ee + kind] < 5 {
            state.data[0x87ee + kind] += 1;
        }
        Ok(finished)
    }
    fn settle_finish(&mut self, state: &mut Campaign) -> Result<()> {
        state.sound.stop();
        if word(&state.data, 0xc320)? != 65535 {
            let base = (i16::from(state.data[0x8ba6])
                .wrapping_add(i16::from(state.data[0x8966 + self.position] as i8))
                + match state.data[0x48f] {
                    0 => -3,
                    1 => 3,
                    2 => 6,
                    _ => 0,
                })
            .clamp(75, 95);
            state.data[0x8ba6] = base as u8;
        }
        let winner = self.standing[0];
        if winner != 3 {
            let province = usize::from(state.data[0xc2c5 + winner * 18]);
            let at = 0x527e + province * 32;
            state.data[at] = (i16::from(state.data[at] as i8) - 10).max(1) as u8;
        }
        self.stage = Stage::Done;
        Ok(())
    }
    fn background(&self, video: VideoMode) -> Frame {
        let mut frame = Frame::new();
        frame.blit(&self.circus, 0, 0);
        frame.rect(0, 61, 320, 139, video.color(17, 7));
        frame
    }
    fn card(&self, assets: &Assets, state: &Campaign, frame: &mut Frame, id: usize) -> Result<()> {
        let d = &state.data;
        let at = 0xc2c2 + id * 18;
        let x = i32::from(word(d, 0x8a8a + id * 4)?);
        let y = i32::from(word(d, 0x8a8c + id * 4)?);
        let color = word(d, 0x87de + id * 2)? as u8;
        frame.bevel(x, y, x + 100, y + 80, 26);
        frame.center_text(
            assets,
            &state.province_name(d[at + 3])?,
            x + 50,
            y + 6,
            color,
        )?;
        if id != 3 {
            frame.center_text(
                assets,
                &state.string(word(d, 0x8804 + usize::from(d[0xc368 + id]) * 2)? as usize)?,
                x + 50,
                y + 17,
                color,
            )?;
        }
        let kind = usize::from(d[at + 4]);
        frame.center_text(
            assets,
            &state.string(word(d, 0x87fe + kind * 2)? as usize)?,
            x + 50,
            y + 63,
            color,
        )?;
        frame.blit(&self.sprites[usize::from(d[0x87f6 + kind])], x + 33, y + 36);
        Ok(())
    }
    pub fn render(&self, assets: &Assets, state: &Campaign, panels: &Panels) -> Result<Frame> {
        if self.stage == Stage::Gallery {
            return Ok(self.gallery.as_ref().unwrap().render());
        }
        if self.stage == Stage::Victory {
            return Ok(self.parade.as_ref().unwrap().render());
        }
        if matches!(self.stage, Stage::Racing | Stage::Crash | Stage::Result) {
            let mut frame = self.view.as_ref().unwrap().frame.clone();
            if self.stage == Stage::Result {
                crate::legion::message(&mut frame, assets, state, &self.message)?;
            }
            return Ok(frame);
        }
        let mut background = self.background(assets.video);
        let stage = if self.stage == Stage::Prompt {
            self.resume
        } else if matches!(self.stage, Stage::ConfirmExit | Stage::ConfirmNoBet) {
            if self.chosen {
                Stage::Prepare
            } else {
                Stage::Type
            }
        } else {
            self.stage
        };
        if matches!(stage, Stage::Type | Stage::Selected) {
            for id in 0..3 {
                self.card(assets, state, &mut background, id)?;
            }
            if self.chosen {
                self.card(assets, state, &mut background, 3)?;
            }
            if stage == Stage::Type {
                panels.plain_menu(
                    assets,
                    state,
                    &mut background,
                    &Menu::read(state, 0x88e6)?,
                    self.selected,
                )?;
            }
            if stage == Stage::Selected {
                crate::legion::message(&mut background, assets, state, "")?;
            }
        } else {
            let mut view = state.clone();
            let available = if self.stage == Stage::Prompt {
                self.prompt_money.unwrap_or(self.available(state)?)
            } else {
                self.available(state)?
            };
            put(&mut view, 0x634a, available);
            background = panels.screen(
                assets,
                &view,
                Some((0, &background)),
                match stage {
                    Stage::Bet => 0x890a,
                    Stage::Dirty => 0x891c,
                    _ => 0x88f8,
                },
                self.selected,
            )?;
            background.pixels[61 * 320..79 * 320].fill(assets.video.color(17, 7));
            if stage == Stage::Bet {
                background.rect(10, 150, 136, 31, 26);
                let kind = usize::from(state.data[0xc2fc]);
                background.center_lines(
                    assets,
                    &format!(
                        "{}{}{}{}{}",
                        state.string(0x8945)?,
                        word(&state.data, 0xc366)?,
                        state.string(0x8950)?,
                        state.data[0x87ee + kind],
                        state.string(0x8960)?
                    ),
                    78,
                    150,
                    0,
                )?;
            }
        }
        if self.stage == Stage::Prompt {
            crate::legion::message(&mut background, assets, state, &self.message)?;
        }
        if matches!(self.stage, Stage::ConfirmExit | Stage::ConfirmNoBet) {
            panels.confirm(
                assets,
                state,
                &mut background,
                &state.string(if self.stage == Stage::ConfirmExit {
                    0x84ce
                } else {
                    0x87ba
                })?,
                self.selected,
            )?;
        }
        Ok(background)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn race() -> Race {
        Race {
            stage: Stage::Prepare,
            resume: Stage::Prepare,
            message: String::new(),
            effect: Effect::None,
            selected: None,
            chosen: true,
            menu: Menu {
                joined_decoration: true,
                rect: (0, 0, 0, 0),
                title: String::new(),
                items: Vec::new(),
            },
            menu_focus: [None; 4],
            circus: Bitmap {
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
                pixels: Vec::new(),
                visible: Vec::new(),
            },
            sprites: Vec::new(),
            dirty_budget: None,
            dirty_spent: 0,
            prompt_money: None,
            gallery: None,
            motion: crate::race_motion::Motion::default(),
            view: None,
            elapsed: 0,
            held: 0,
            mouse: (160, 100),
            mouse_previous: (160, 100),
            buttons: 0,
            standing: [3, 0, 1, 2],
            position: 0,
            crash: None,
            parade: None,
        }
    }
    fn state() -> Campaign {
        let mut s = Campaign::test_data(vec![0; 65536]);
        put(&mut s, 0x634a, 200);
        put(&mut s, 0x594a, 100);
        put(&mut s, 0xc366, 25);
        s.data[0x87ee] = 3;
        s.data[0x8ba6] = 85;
        s.data[0x8966..0x896a].copy_from_slice(&[8, 4, 0, 254]);
        // Minimal authored menu: one empty item/string, no original data.
        s.data[0x88f8 + 14] = 1;
        s
    }
    fn focus_menus(state: &mut Campaign) {
        for (index, address) in [0x88e6, 0x88f8, 0x891c, 0x890a].into_iter().enumerate() {
            let entries = 0x9000 + index * 48;
            put(state, address + 10, entries as u16);
            state.data[address + 14] = 3;
            state.data[entries + 3] = 1;
        }
    }
    #[test]
    fn keyboard_race_children_start_first_and_returns_restore_the_parent_row() {
        let assets = Assets::test_data(vec![0; 65536]);
        let mut state = state();
        focus_menus(&mut state);
        state.continue_input = ContinueInput::Keyboard;
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        let mut r = race();
        r.stage = Stage::Selected;
        r.menu(&state, Stage::Prepare).unwrap();
        assert_eq!(r.selected, Some(1));
        r.selected = Some(2);
        r.menu(&state, Stage::Bet).unwrap();
        assert_eq!(r.selected, Some(1));
        r.selected = Some(2);
        r.input(&assets, &mut state, &mut rng, Input::Escape)
            .unwrap();
        assert_eq!(r.stage, Stage::Prepare);
        assert_eq!(r.selected, Some(2));
        r.menu(&state, Stage::Bet).unwrap();
        assert_eq!(
            r.selected,
            Some(1),
            "a fresh child entry must not revive old child focus"
        );
        r.menu(&state, Stage::Prepare).unwrap();
        assert_eq!(r.selected, Some(2));
        r.prompt("Report".into(), Stage::Prepare, Effect::None);
        r.input(&assets, &mut state, &mut rng, Input::Enter)
            .unwrap();
        assert_eq!(r.stage, Stage::Prepare);
        assert_eq!(r.selected, Some(2));
        for confirmation in [Stage::ConfirmExit, Stage::ConfirmNoBet] {
            r.confirm(&state, confirmation);
            assert_eq!(
                r.selected,
                Some(1),
                "keyboard entry must visibly preserve No as the Enter default"
            );
            r.input(&assets, &mut state, &mut rng, Input::Enter)
                .unwrap();
            assert_eq!(r.stage, Stage::Prepare);
            assert_eq!(r.selected, Some(2));
        }
        r.chosen = false;
        r.menu(&state, Stage::Type).unwrap();
        r.selected = Some(2);
        r.confirm(&state, Stage::ConfirmExit);
        r.input(&assets, &mut state, &mut rng, Input::Enter)
            .unwrap();
        assert_eq!(r.stage, Stage::Type);
        assert_eq!(r.selected, Some(2));
        state.continue_input = ContinueInput::Mouse;
        r.menu(&state, Stage::Bet).unwrap();
        assert_eq!(r.selected, None);
        r.menu(&state, Stage::Prepare).unwrap();
        assert_eq!(r.selected, None);
    }
    #[test]
    fn first_confirmation_arrow_focuses_yes_instead_of_toggling_an_unseen_default() {
        let assets = Assets::test_data(vec![0; 65536]);
        let mut state = state();
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        let before = state.data.clone();
        for confirmation in [Stage::ConfirmExit, Stage::ConfirmNoBet] {
            for arrow in [Input::Up, Input::Down, Input::Left, Input::Right] {
                state.continue_input = ContinueInput::Mouse;
                let mut r = race();
                r.confirm(&state, confirmation);
                assert_eq!(r.selected, None);
                for expected in [Some(0), Some(1), Some(0)] {
                    r.input(&assets, &mut state, &mut rng, arrow.clone())
                        .unwrap();
                    assert_eq!(r.selected, expected);
                    assert_eq!(r.stage, confirmation);
                }
                state.continue_input = ContinueInput::Keyboard;
                r.confirm(&state, confirmation);
                assert_eq!(r.selected, Some(1), "keyboard entry retains the No default");
                r.input(&assets, &mut state, &mut rng, Input::Enter)
                    .unwrap();
                assert_eq!(r.stage, Stage::Prepare);
                assert_eq!(state.data, before);
                assert_eq!((rng.first, rng.second), (15, 40));
            }
        }
    }
    #[test]
    fn menu_hover_selects_without_purchases_and_motion_does_not_acknowledge_race_effects() {
        let mut r = race();
        let assets = Assets::test_data(vec![0; 65536]);
        let mut state = state();
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        r.menu.items.push(crate::province::Item {
            label: "Available".into(),
            shortcut: b'a',
            disabled: false,
            rect: (10, 20, 50, 28),
        });
        let before = state.data.clone();
        for stage in [Stage::Type, Stage::Prepare, Stage::Dirty, Stage::Bet] {
            r.stage = stage;
            assert!(
                !r.input(&assets, &mut state, &mut rng, Input::Mouse(30, 24))
                    .unwrap()
            );
            assert_eq!(r.selected, Some(0));
            assert_eq!(r.stage, stage);
            assert!(
                !r.input(&assets, &mut state, &mut rng, Input::Mouse(0, 0))
                    .unwrap()
            );
            assert_eq!(r.selected, None);
        }
        r.effect = Effect::Physician;
        for stage in [
            Stage::Selected,
            Stage::Prompt,
            Stage::Gallery,
            Stage::Victory,
            Stage::Result,
        ] {
            r.stage = stage;
            for input in [
                Input::Mouse(30, 24),
                Input::MouseDelta(5, 5),
                Input::ReleaseButton(1),
            ] {
                assert!(!r.input(&assets, &mut state, &mut rng, input).unwrap());
                assert_eq!(r.stage, stage);
                assert_eq!(state.data, before);
                assert_eq!((rng.first, rng.second), (15, 40));
            }
        }
    }
    #[test]
    fn confirmations_hover_and_click_only_their_painted_title_dependent_rows() {
        let assets = Assets::test_data(vec![0; 65536]);
        let mut state = state();
        for (at, title) in [(0x84ce, "Leave?"), (0x87ba, "Begin\nwithout\na wager?")] {
            state.data[at..at + title.len()].copy_from_slice(title.as_bytes());
            state.data[at + title.len()] = 0;
        }
        let before = state.data.clone();
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        for (stage, y) in [(Stage::ConfirmExit, 101), (Stage::ConfirmNoBet, 112)] {
            let mut r = race();
            r.stage = stage;
            assert!(
                !r.input(&assets, &mut state, &mut rng, Input::Mouse(140, y))
                    .unwrap()
            );
            assert_eq!(r.selected, Some(0));
            assert_eq!(r.stage, stage);
            for input in [
                Input::Mouse(163, y),
                Input::Click(163, y),
                Input::Click(140, y - 1),
                Input::Click(197, y),
            ] {
                assert!(!r.input(&assets, &mut state, &mut rng, input).unwrap());
                assert_eq!(r.stage, stage);
                assert_eq!(r.selected, None);
            }
            assert!(
                !r.input(&assets, &mut state, &mut rng, Input::Mouse(195, y))
                    .unwrap()
            );
            assert_eq!(r.selected, Some(1));
            // The right-hand portion of No is also clickable, beyond old x=188.
            assert!(
                !r.input(&assets, &mut state, &mut rng, Input::Click(195, y))
                    .unwrap()
            );
            assert_eq!(r.stage, Stage::Prepare);
            assert_eq!(state.data, before);
            assert_eq!((rng.first, rng.second), (15, 40));
        }
    }
    #[test]
    fn finishing_rank_controls_score_odds_skill_and_wager_payout() {
        for (rank, score, odds, skill) in [
            (0, 110, 2, 90),
            (1, 105, 3, 86),
            (2, 101, 4, 82),
            (3, 100, 4, 80),
        ] {
            let mut r = race();
            let mut s = state();
            r.position = rank;
            assert!(r.rewards(&mut s).unwrap());
            r.settle_finish(&mut s).unwrap();
            assert_eq!(word(&s.data, 0x594a).unwrap(), score);
            assert_eq!(s.data[0x87ee], odds);
            assert_eq!(s.data[0x8ba6], skill);
            assert_eq!(
                word(&s.data, 0x634a).unwrap(),
                if rank == 0 { 300 } else { 200 }
            );
            assert_eq!(
                word(&s.data, 0xc096).unwrap(),
                if rank == 0 { 100 } else { 0 }
            );
            assert_eq!(word(&s.data, 0xc104).unwrap(), u16::from(rank == 0));
            assert_eq!(r.stage, Stage::Done);
        }
    }
    #[test]
    fn failing_to_finish_never_awards_a_win_even_when_ranked_first() {
        let mut r = race();
        let mut s = state();
        put(&mut s, 0xc320, 65535);
        assert!(!r.rewards(&mut s).unwrap());
        r.settle_finish(&mut s).unwrap();
        assert_eq!(word(&s.data, 0x634a).unwrap(), 200);
        assert_eq!(word(&s.data, 0x594a).unwrap(), 100);
        assert_eq!(word(&s.data, 0xc104).unwrap(), 0);
        assert_eq!((s.data[0x87ee], s.data[0x8ba6]), (4, 85));
    }
    #[test]
    fn odds_and_skill_retain_original_limits_and_difficulty_modifiers() {
        for (rank, difficulty, old, expected) in [(0, 0, 1, 1), (3, 0, 5, 5), (0, 2, 1, 1)] {
            let mut r = race();
            let mut s = state();
            r.position = rank;
            s.data[0x48f] = difficulty;
            s.data[0x87ee] = old;
            s.data[0x8ba6] = if rank == 0 { 95 } else { 75 };
            r.rewards(&mut s).unwrap();
            r.settle_finish(&mut s).unwrap();
            assert_eq!(s.data[0x87ee], expected);
            assert_eq!(s.data[0x8ba6], if rank == 0 { 95 } else { 75 });
        }
    }
    #[test]
    fn dirty_money_preserves_pending_wager_and_does_not_refund_purchases() {
        let mut r = race();
        let mut s = state();
        assert_eq!(r.available(&s).unwrap(), 175);
        r.dirty_budget = Some(200);
        r.dirty_spent = 90;
        put(&mut s, 0x634a, 85);
        assert_eq!(r.available(&s).unwrap(), 85);
        r.leave_dirty(&mut s).unwrap();
        assert_eq!(word(&s.data, 0x634a).unwrap(), 110);
        assert_eq!(word(&s.data, 0xc366).unwrap(), 25);
        assert_eq!(r.available(&s).unwrap(), 85);
        assert_eq!(word(&s.data, 0xaabc).unwrap(), 85);
    }
    #[test]
    fn opposing_winner_reduces_only_its_own_unrest_including_after_player_failure() {
        let mut r = race();
        let mut s = state();
        r.standing = [1, 3, 0, 2];
        s.data[0xc2c5 + 18] = 4;
        s.data[0x527e + 4 * 32] = 6;
        s.data[0x527e] = 60;
        put(&mut s, 0xc320, 65535);
        r.settle_finish(&mut s).unwrap();
        assert_eq!(s.data[0x527e + 4 * 32], 1);
        assert_eq!(s.data[0x527e], 60);
    }
    #[test]
    fn standing_prefers_finish_time_then_raw_progress_and_keeps_tie_rng_calls() {
        let mut r = race();
        let mut s = state();
        for id in 0..4 {
            put(&mut s, 0xc31a + id * 2, [100, 80, 65535, 90][id]);
            put(&mut s, 0xc2be + id * 18, 100);
        }
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        let before = rng;
        r.finish(&s, &mut rng).unwrap();
        assert_eq!(r.standing, [1, 3, 0, 2]);
        assert_eq!(r.position, 1);
        assert_eq!((rng.first, rng.second), (before.first, before.second));
        for id in 0..4 {
            put(&mut s, 0xc31a + id * 2, 65535);
            put(&mut s, 0xc2be + id * 18, [100, 200, 50, 300][id]);
        }
        r.finish(&s, &mut rng).unwrap();
        assert_eq!(r.standing, [3, 1, 0, 2]);
        assert_eq!((rng.first, rng.second), (before.first, before.second));
        for id in 0..4 {
            put(&mut s, 0xc2be + id * 18, 100);
        }
        let mut expected = rng;
        for _ in 0..6 {
            expected.bounded(1);
        }
        r.finish(&s, &mut rng).unwrap();
        assert_eq!((rng.first, rng.second), (expected.first, expected.second));
    }
}
