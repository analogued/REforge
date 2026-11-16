// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Gladiatorial shows: overlay 3, with animation and AI tables in the original DS.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::games::{menu_input, put};
use crate::graphics::Frame;
use crate::province::{Menu, Panels};
use crate::rng::OriginalRng;
use crate::session::Input;
use crate::state::Campaign;

const ROSTER: usize = 0x7e3e;
fn record(id: u8) -> usize {
    ROSTER + usize::from(id) * 10
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Intro,
    Prompt,
    Cards,
    Training,
    Fight,
    Walk,
    Thumbs,
    Final,
    Done,
}
#[derive(Clone, Copy)]
enum After {
    First,
    Second,
    Refund,
    Training,
    Fight,
    Thumbs,
    Done,
}

pub struct Arena {
    stage: Stage,
    after: After,
    message: String,
    elapsed: u32,
    offers: [u8; 6],
    draw_offers: [u8; 6],
    offered: bool,
    first: Option<u8>,
    second: Option<u8>,
    chosen: u8,
    original_treasury: u16,
    selected: Option<usize>,
    training: Menu,
    intro: Bitmap,
    floor: Bitmap,
    arena: Vec<Bitmap>,
    show: Vec<Bitmap>,
    fight: Option<Fight>,
    versus: bool,
    enthusiastic: bool,
    dead: [bool; 2],
}

impl Arena {
    pub fn new(assets: &Assets, state: &Campaign) -> Result<Self> {
        state.sound.play(4);
        Ok(Self {
            stage: Stage::Intro,
            after: After::Done,
            message: String::new(),
            elapsed: 0,
            offers: [0; 6],
            draw_offers: [0; 6],
            offered: false,
            first: None,
            second: None,
            chosen: 0,
            original_treasury: word(&state.data, 0x634a)?,
            selected: None,
            training: Menu::read(state, 0x7cb0)?,
            intro: assets.bitmap("MGLAD", "COLOSEUM.CBM")?,
            floor: assets.cached_bitmap("MGLAD", "ARENABTM.CBM")?,
            arena: assets.sprites("MGLAD", "ARENA")?,
            show: assets.sprites("MGLAD", "SHOW")?,
            fight: None,
            versus: false,
            enthusiastic: false,
            dead: [false; 2],
        })
    }
    pub fn diagnostic(&self) -> String {
        let combat = self
            .fight
            .as_ref()
            .map(|f| {
                format!(
                    " hp={},{} animation={},{} x={},{} crowd={} hits={} blocks={}",
                    f.units[0].health,
                    f.units[1].health,
                    f.units[0].animation,
                    f.units[1].animation,
                    f.units[0].x,
                    f.units[1].x,
                    f.crowd,
                    f.hits,
                    f.blocks
                )
            })
            .unwrap_or_default();
        format!("arena={:?}{combat}", self.stage)
    }
    pub fn moves_cursor(&self) -> bool {
        self.stage == Stage::Cards
    }
    pub fn cursor_visible(&self) -> bool {
        matches!(self.stage, Stage::Cards | Stage::Training | Stage::Thumbs)
    }
    fn prompt(&mut self, state: &Campaign, address: usize, after: After) -> Result<()> {
        self.message = state.string(address)?;
        self.after = after;
        self.stage = Stage::Prompt;
        Ok(())
    }
    fn clear_pool(&self, state: &mut Campaign) {
        for id in self.offers {
            if state.data[record(id) + 5] == 3 {
                state.data[record(id) + 5] = 0;
            }
        }
    }
    fn cancel(&mut self, state: &mut Campaign) {
        if let Some(id) = self.first {
            state.data[record(id) + 5] = 0;
        }
        self.clear_pool(state);
        self.stage = Stage::Done;
    }
    fn offer(&mut self, state: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        state.sound.play(7);
        if word(&state.data, 0x634a)? < 20 {
            return self.prompt(state, 0x7cc2, After::Done);
        }
        self.offered = true;
        for slot in 0..6 {
            let start = rng.bounded(10) as u8;
            let id = (0..10)
                .map(|n| (start + n) % 10)
                .find(|&id| state.data[record(id) + 5] == 0)
                .ok_or("Gladiator roster has fewer than six available fighters")?;
            self.offers[slot] = id;
            state.data[record(id) + 5] = 3;
        }
        self.draw_offers = self.offers;
        self.prompt(state, 0x7ce4, After::First)
    }
    fn choose_card(
        &mut self,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        x: i32,
        y: i32,
    ) -> Result<()> {
        let slot = (0..6).find(|&n| {
            let left = 7 + (n % 3) * 106;
            let top = 2 + (n / 3) * 100;
            (left..=left + 91).contains(&x) && (top..=top + 91).contains(&y)
        });
        if let Some(slot) = slot {
            let slot = slot as usize;
            self.chosen = self.offers[slot];
            let at = record(self.chosen);
            state.data[at + 5] = 1;
            self.offers[slot] = 10 + rng.bounded(3) as u8;
            let labels = if state.data[at + 6] == 4 {
                0x7c76
            } else {
                0x7c6e
            };
            for (n, item) in self.training.items.iter_mut().enumerate() {
                item.label = format!(
                    "{} ({} Talents)",
                    state.string(word(&state.data, labels + n * 2)? as usize)?,
                    (n + 1) * 10
                );
                item.disabled = false;
            }
            self.selected = None;
            self.stage = Stage::Training;
        }
        Ok(())
    }
    fn buy(&mut self, state: &mut Campaign, row: usize) -> Result<()> {
        self.selected = Some(row);
        let price = (row as u16 + 1) * 10;
        let treasury = word(&state.data, 0x634a)?;
        if treasury < price {
            self.training.items[row].disabled = true;
            return self.prompt(state, 0x7d7d, After::Training);
        }
        put(state, 0x634a, treasury - price);
        state.data[record(self.chosen) + 8] = row as u8;
        if self.first.is_none() {
            self.first = Some(self.chosen);
            self.draw_offers = self.offers;
            if treasury - price < 10 {
                self.prompt(state, 0x7d0a, After::Refund)?;
            } else {
                state.sound.play(5);
                self.prompt(state, 0x7d59, After::Second)?;
            }
        } else {
            self.second = Some(self.chosen);
            self.clear_pool(state);
            state.sound.play(4);
            self.versus = true;
            self.prompt(state, 0x7bc6, After::Fight)?;
        }
        Ok(())
    }
    fn start_fight(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        state.sound.play(5);
        let player = self.first.ok_or("Missing controlled gladiator")?;
        let enemy = self.second.ok_or("Missing opponent")?;
        self.fight = Some(Fight::new(assets, state, rng, player, enemy)?);
        put(state, 0xc100, word(&state.data, 0xc100)?.wrapping_add(1));
        state.data[0xed4] = (state.data[0xed4] as i8).min(10) as u8;
        put(state, 0xed8, 0);
        self.elapsed = 0;
        self.stage = Stage::Fight;
        Ok(())
    }
    fn outcome(&mut self, state: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        state.sound.stop();
        put(state, 0x154c, word(&state.data, 0x634c)?);
        let fight = self.fight.as_mut().unwrap();
        self.enthusiastic =
            fight.crowd > i16::from(fight.units[0].skill + fight.units[1].skill + 2) * 2;
        let mut bonus = false;
        for side in 0..2 {
            let u = &fight.units[side];
            let opponent = &fight.units[1 - side];
            let at = record(u.id);
            if u.health > 0 {
                state.data[at + 3] = state.data[at + 3].wrapping_add(1);
                if opponent.skill > u.skill {
                    state.change_score(i16::from(opponent.skill - u.skill))?;
                    bonus = true;
                }
                if side == 0 {
                    state.change_score(3)?;
                }
            } else {
                state.data[at + 4] = state.data[at + 4].wrapping_add(1);
            }
        }
        if bonus {
            state.change_score(3)?;
        }
        let player_alive = fight.units[0].health > 0;
        let enemy_alive = fight.units[1].health > 0;
        let animal = state.data[record(fight.units[1].id) + 6] == 4;
        self.dead = [animal && !enemy_alive, animal && enemy_alive];
        if !player_alive && !enemy_alive {
            state.change_score(3)?;
            self.dead = [true; 2];
        }
        if self.dead.iter().any(|&v| v) {
            self.finish_roster(state, rng)?;
        } else {
            self.stage = Stage::Walk;
        }
        Ok(())
    }
    fn thumbs(&mut self, state: &mut Campaign, rng: &mut OriginalRng, kill: bool) -> Result<()> {
        let fight = self.fight.as_mut().unwrap();
        let player_alive = fight.units[0].health > 0;
        let enemy_alive = fight.units[1].health > 0;
        if kill {
            if player_alive {
                self.dead[1] = true;
                fight.units[0].animation = 8;
                fight.units[1].animation = fight.units[1].default;
            } else if enemy_alive {
                self.dead[0] = true;
                fight.units[1].animation = 8;
                fight.units[0].animation = fight.units[0].default;
            }
        }
        let approved = kill == self.enthusiastic;
        // C0AB..C0D0: an excited crowd wants mercy; a bored crowd wants death.
        let delta = if approved { -5 } else { 5 };
        fight.change_crowd(delta, &state.sound);
        state.change_score(delta)?;
        if kill {
            for u in &mut fight.units {
                u.draw_animation = u.animation;
            }
        }
        self.finish_roster(state, rng)
    }
    fn finish_roster(&mut self, state: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        let fight = self.fight.as_ref().unwrap();
        // Original replaces the opponent first, then the controlled fighter.
        for side in [1, 0] {
            if !self.dead[side] {
                continue;
            }
            let at = record(fight.units[side].id);
            state.data[at + 3] = 0;
            state.data[at + 4] = 0;
            state.data[at + 2] = state.data[at + 2].wrapping_add(1);
            if state.data[at + 6] != 4 {
                state.data[at + 7] = rng.bounded(2) as u8;
                state.data[at + 6] = rng.bounded(4) as u8;
            }
            state.data[at + 5] = 0;
        }
        self.message = state.string(0x7bc7)?;
        self.stage = Stage::Final;
        Ok(())
    }
    pub fn input(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        input: Input,
    ) -> Result<bool> {
        if let Input::Tick(ticks) = input {
            match self.stage {
                Stage::Intro => {
                    self.elapsed = self.elapsed.saturating_add(ticks);
                    if self.elapsed >= 432 {
                        self.offer(state, rng)?;
                    }
                }
                Stage::Fight | Stage::Walk => {
                    self.elapsed = self.elapsed.saturating_add(ticks);
                    while self.elapsed >= 4 && matches!(self.stage, Stage::Fight | Stage::Walk) {
                        self.elapsed -= 4;
                        if self.stage == Stage::Fight {
                            if self.fight.as_mut().unwrap().step(state, rng)? {
                                self.outcome(state, rng)?;
                            }
                        } else if self.fight.as_mut().unwrap().walk(&state.data)? {
                            self.prompt(state, 0x800e, After::Thumbs)?;
                        }
                    }
                }
                _ => {}
            }
        } else if self.stage == Stage::Fight {
            let fight = self.fight.as_mut().unwrap();
            match input {
                Input::Up => fight.held |= 8,
                Input::Down => fight.held |= 4,
                Input::Left => fight.held |= 2,
                Input::Right => fight.held |= 1,
                Input::ReleaseKey(bits) => fight.held &= !bits,
                _ => {}
            }
        } else if !matches!(
            input,
            Input::Mouse(..) | Input::Release(..) | Input::ReleaseKey(_) | Input::Random(_)
        ) {
            match self.stage {
                Stage::Intro => self.offer(state, rng)?,
                Stage::Prompt => match self.after {
                    After::First | After::Second => self.stage = Stage::Cards,
                    After::Refund => {
                        put(state, 0x634a, self.original_treasury);
                        self.cancel(state);
                    }
                    After::Training => self.stage = Stage::Training,
                    After::Fight => self.start_fight(assets, state, rng)?,
                    After::Thumbs => self.stage = Stage::Thumbs,
                    After::Done => self.stage = Stage::Done,
                },
                Stage::Cards => {
                    // 359B returns AL=0 for Escape; B505's intended escape branch
                    // is consequently unreachable on the card screen. Training
                    // menus still accept Escape through their own input routine.
                    if let Input::Click(x, y) = input {
                        self.choose_card(state, rng, x, y)?;
                    }
                }
                Stage::Training => {
                    if matches!(input, Input::Escape) {
                        self.cancel(state);
                    } else if let Some(row) = menu_input(&self.training, &mut self.selected, &input)
                    {
                        self.buy(state, row)?;
                    }
                }
                Stage::Thumbs => {
                    let choice =
                        match input {
                            Input::Up => Some(false),
                            Input::Down => Some(true),
                            Input::Click(x, y) => [0x7ec0, 0x7ec8]
                                .into_iter()
                                .enumerate()
                                .find_map(|(n, at)| {
                                    let d = &state.data;
                                    let (l, t, r, b) = (
                                        word(d, at).ok()? as i32,
                                        word(d, at + 2).ok()? as i32,
                                        word(d, at + 4).ok()? as i32,
                                        word(d, at + 6).ok()? as i32,
                                    );
                                    ((l..=r).contains(&x) && (t..=b).contains(&y)).then_some(n == 1)
                                }),
                            _ => None,
                        };
                    if let Some(kill) = choice {
                        self.thumbs(state, rng, kill)?;
                    }
                }
                Stage::Final => {
                    for id in [self.first, self.second].into_iter().flatten() {
                        state.data[record(id) + 5] = 0;
                    }
                    self.stage = Stage::Done;
                }
                _ => {}
            }
        }
        if self.stage == Stage::Done {
            state.sound.stop();
        }
        Ok(self.stage == Stage::Done)
    }

    fn background(&self, assets: &Assets, state: &Campaign) -> Result<Frame> {
        if !self.offered {
            let mut frame = Frame::new();
            frame.blit(&self.intro, 0, 0);
            return Ok(frame);
        }
        let d = &state.data;
        let mut frame = Frame::new();
        frame.rect(0, 0, 320, 200, 1);
        for at in (0x7bda..0x7c1a).step_by(4) {
            frame.rect(0, i32::from(d[at + 2]), 320, 1, word(d, at)? as u8);
        }
        for at in (0x7c1a..0x7c6e).step_by(6) {
            frame.blit(
                &self.arena[usize::from(d[at + 4])],
                i32::from(word(d, at)? as i16),
                i32::from(word(d, at + 2)? as i16),
            );
        }
        if self.versus {
            frame.center_text(assets, &state.string(0x7bbf)?, 160, 125, 15)?;
            self.card(&mut frame, assets, state, 5, self.first.unwrap(), true)?;
            self.card(&mut frame, assets, state, 3, self.second.unwrap(), false)?;
        } else {
            for (slot, id) in self.draw_offers.iter().enumerate() {
                self.card(&mut frame, assets, state, slot, *id, self.first.is_none())?;
            }
        }
        Ok(frame)
    }
    fn card(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        state: &Campaign,
        slot: usize,
        id: u8,
        left: bool,
    ) -> Result<()> {
        let at = record(id);
        let d = &state.data;
        let x = 56 + (slot % 3) as i32 * 106;
        let y = 87 + (slot / 3) as i32 * 100;
        let mut name = state.string(word(d, at)? as usize)?;
        if d[at + 6] != 4 && d[at + 2] != 1 {
            name.push_str(&state.roman(d[at + 2])?);
        }
        frame.center_text(assets, &name, x, y - 80, 15)?;
        if d[at + 3] != 0 {
            frame.center_text(assets, &state.string(0x7dad)?, x, y - 70, 15)?;
            frame.center_text(assets, &state.roman(d[at + 3])?, x, y - 60, 15)?;
        }
        let sprite = usize::from(d[at + 7]) + if left { 0 } else { 2 };
        frame.blit(
            &self.show[sprite],
            x + if left {
                -10
            } else {
                10 + if d[at + 6] == 4 { 15 } else { 0 }
            },
            y,
        );
        Ok(())
    }
    pub fn render(&self, assets: &Assets, state: &Campaign, panels: &Panels) -> Result<Frame> {
        if self.stage == Stage::Intro {
            let mut f = Frame::new();
            f.blit(&self.intro, 0, 0);
            return Ok(f);
        }
        let mut frame = if let Some(fight) = &self.fight {
            let mut f = Frame::new();
            f.blit(&self.arena[0], 0, 0);
            f.blit(&self.floor, 0, 126);
            fight.render(&mut f, &state.data)?;
            if matches!(self.stage, Stage::Thumbs | Stage::Final)
                || (self.stage == Stage::Prompt && matches!(self.after, After::Thumbs))
            {
                f.blit(&self.arena[3], 79, 20);
                fight.bar(&mut f, &state.data)?;
            }
            f
        } else {
            self.background(assets, state)?
        };
        if self.stage == Stage::Training
            || (self.stage == Stage::Prompt && matches!(self.after, After::Training))
        {
            panels.draw_menu(assets, state, &mut frame, &self.training, self.selected)?;
        }
        if matches!(self.stage, Stage::Prompt | Stage::Final) {
            crate::legion::message(&mut frame, assets, state, &self.message)?;
        }
        Ok(frame)
    }
}

struct Fighter {
    id: u8,
    sequence: usize,
    animation: u8,
    draw_animation: u8,
    default: u8,
    x: i16,
    skill: u8,
    health: i8,
    sprites: Vec<Bitmap>,
    moving: bool,
}
impl Fighter {
    fn entry(&self, data: &[u8]) -> [u8; 4] {
        data[self.sequence + usize::from(self.animation) * 4..][..4]
            .try_into()
            .unwrap()
    }
}
struct Fight {
    units: [Fighter; 2],
    held: u8,
    crowd: i16,
    decay: u32,
    input_wait: [i16; 2],
    frame_wait: [i16; 2],
    hits: u16,
    blocks: u16,
    cheers: u16,
    jeers: u16,
    level: i16,
}
impl Fight {
    fn new(
        assets: &Assets,
        state: &Campaign,
        rng: &mut OriginalRng,
        player: u8,
        enemy: u8,
    ) -> Result<Self> {
        let data = &state.data;
        let mut units = Vec::new();
        for (side, id) in [player, enemy].into_iter().enumerate() {
            let at = record(id);
            let kind = usize::from(data[at + 7]);
            let sequence = word(
                data,
                if side == 0 {
                    0x7fe8 + kind * 2
                } else {
                    0x7fec + kind * 2
                },
            )? as usize;
            let stem = state.string(word(
                data,
                if side == 0 {
                    0x7bcc + kind * 2
                } else {
                    0x7bd0 + kind * 2
                },
            )? as usize)?;
            let skill = data[at + 8];
            let mut health = 10i8;
            for _ in 0..skill {
                health = health.wrapping_add(rng.bounded(10) as i8);
            }
            units.push(Fighter {
                id,
                sequence,
                animation: 0,
                draw_animation: 0,
                default: data[if side == 0 {
                    0x7ed0 + kind
                } else {
                    0x7ed2 + kind
                }],
                x: if side == 0 { 192 } else { 127 },
                skill,
                health,
                sprites: assets.sprites("MGLAD", &stem.to_ascii_uppercase())?,
                moving: false,
            });
        }
        let units: [Fighter; 2] = units.try_into().map_err(|_| "Invalid fighter pair")?;
        Ok(Self {
            crowd: i16::from(units[0].skill + units[1].skill + 2),
            units,
            held: 0,
            decay: 0,
            input_wait: [0; 2],
            frame_wait: [6; 2],
            hits: 0,
            blocks: 0,
            cheers: 0,
            jeers: 0,
            level: 1,
        })
    }
    fn change_crowd(&mut self, change: i16, sound: &crate::sound::Bus) {
        self.crowd = (self.crowd + change).clamp(0, 30);
        if change > 0 {
            self.jeers = 0;
            self.cheers += 1;
            if self.cheers > 3 {
                self.cheers = 0;
                self.level = (self.level + 1).min(2);
                sound.play([7, 5, 6][self.level as usize]);
            }
        } else {
            self.cheers = 0;
            self.jeers += 1;
            if self.jeers > 3 {
                self.jeers = 0;
                self.level = (self.level - 1).max(0);
                sound.play([7, 5, 6][self.level as usize]);
            }
        }
    }
    fn ai(&mut self, state: &mut Campaign, rng: &mut OriginalRng) {
        let kind = state.data[record(self.units[1].id) + 6];
        if self.units[1].health <= 0 {
            self.units[1].animation = if kind == 4 { 4 } else { 9 };
            return;
        }
        state.data[0x800d] = state.data[0x800d]
            .wrapping_add(self.units[1].skill)
            .wrapping_add(state.data[0x490])
            .wrapping_add(5);
        if state.data[0x800d] < 30 {
            return;
        }
        state.data[0x800d] = 0;
        let x = self.units[1].x;
        let near = x + 20 >= self.units[0].x;
        let medium = !near && x + 36 >= self.units[0].x;
        let mut action = match kind {
            0 => {
                if near {
                    if rng.bounded(2) == 0 { 4 } else { 3 }
                } else {
                    1
                }
            }
            1 => {
                if medium {
                    1
                } else if near {
                    state.data[0x7ff6 + rng.bounded(10) as usize]
                } else {
                    2
                }
            }
            2 | 3 => {
                if near {
                    state.data[0x8000 + rng.bounded(10) as usize]
                } else if medium {
                    1
                } else {
                    match rng.bounded(4) {
                        0 | 1 => 5,
                        2 => 6,
                        _ => 7,
                    }
                }
            }
            _ => {
                if self.units[1].health > 5 {
                    if medium || !near { 1 } else { 3 }
                } else if x < 30 || near {
                    3
                } else {
                    2
                }
            }
        };
        if kind != 4 {
            if action == 1 && x > 290 {
                action = 4;
            }
            if action == 2 && x < 30 {
                action = if rng.bounded(2) == 0 { 5 } else { 6 };
            }
        } else if (action == 1 && x > 290) || (action == 2 && x < 30) {
            action = 3;
        }
        self.units[1].animation = action;
    }
    fn hit(&mut self, data: &[u8], side: usize, sound: &crate::sound::Bus) {
        let attack = self.units[side].entry(data)[3] & 6;
        if attack == 0 || (self.units[0].x - self.units[1].x).abs() >= 20 {
            return;
        }
        if (self.units[1 - side].entry(data)[3] & 0x18) >> 2 == attack {
            self.blocks = self.blocks.wrapping_add(1);
        } else {
            self.hits = self.hits.wrapping_add(1);
            self.units[1 - side].health = self.units[1 - side]
                .health
                .wrapping_sub((self.units[side].skill + 1) as i8);
        }
        self.change_crowd(1, sound);
        self.decay = 0;
    }
    fn step(&mut self, state: &mut Campaign, rng: &mut OriginalRng) -> Result<bool> {
        let ending = self.units.iter().any(|u| u.animation == 255);
        if self.input_wait[0] <= 0 {
            self.input_wait[0] = (4 - i16::from(self.units[0].skill)) * 4;
            if self.units[0].entry(&state.data)[3] & 1 != 0 {
                let u = &mut self.units[0];
                u.animation = if u.health <= 0 {
                    9
                } else {
                    match self.held {
                        1 if u.x < 290 => 2,
                        2 if u.x > 30 => 1,
                        4 => 5,
                        6 => 4,
                        8 if u.animation == 0 => {
                            if rng.bounded(2) == 0 {
                                7
                            } else {
                                6
                            }
                        }
                        8 => u.animation,
                        10 => 3,
                        _ => 0,
                    }
                };
                u.moving = true;
            }
        }
        self.input_wait[0] -= 1;
        self.frame_wait[0] -= 1;
        if self.input_wait[1] <= 0 {
            self.input_wait[1] = (4 - i16::from(self.units[1].skill)) * 2;
            if self.units[1].entry(&state.data)[3] & 1 != 0 {
                self.ai(state, rng);
                self.units[1].moving = true;
            }
        }
        self.input_wait[1] -= 1;
        self.frame_wait[1] -= 1;
        // B9DA checks the end flag after polling both fighters, before motion.
        if ending {
            return Ok(true);
        }
        let mut next = [0i16; 2];
        for (n, u) in self.units.iter_mut().enumerate() {
            next[n] = u.x
                + if u.moving {
                    i16::from(u.entry(&state.data)[0] as i8)
                } else {
                    0
                };
            u.moving = false;
        }
        if next[1] < next[0] {
            self.units[0].x = next[0];
            self.units[1].x = next[1];
        } else if rng.bounded(2) == 0 {
            if next[0] > self.units[1].x {
                self.units[0].x = next[0];
            }
        } else if next[1] < self.units[0].x {
            self.units[1].x = next[1];
        }
        for u in &mut self.units {
            u.draw_animation = u.animation;
        }
        for side in 0..2 {
            if self.frame_wait[side] == 0 {
                self.hit(&state.data, side, &state.sound);
                self.units[side].animation = self.units[side].entry(&state.data)[1];
                self.units[side].moving = true;
                self.frame_wait[side] = 6;
            }
        }
        self.decay += 4;
        if self.decay > 216 {
            self.change_crowd(-1, &state.sound);
            self.decay = 0;
        }
        Ok(self.units.iter().any(|u| u.animation == 255)
            && self
                .units
                .iter()
                .all(|u| u.animation == 0 || u.animation == 255))
    }
    fn walk(&mut self, data: &[u8]) -> Result<bool> {
        if self.units[0].health > 0 {
            self.units[1].animation = self.units[1].default;
            if self.units[0].entry(data)[3] & 1 != 0 {
                self.units[0].animation = 1;
            }
            self.units[0].x += i16::from(self.units[0].entry(data)[0] as i8);
        } else {
            self.units[0].animation = self.units[0].default;
            if self.units[1].entry(data)[3] & 1 != 0 {
                self.units[1].animation = 1;
            }
            self.units[1].x += i16::from(self.units[1].entry(data)[0] as i8);
        }
        for u in &mut self.units {
            u.draw_animation = u.animation;
            u.animation = u.entry(data)[1];
        }
        Ok(self.units[1].x > self.units[0].x)
    }
    fn bar(&self, frame: &mut Frame, data: &[u8]) -> Result<()> {
        let length = (145 * self.crowd / 30).clamp(0, 145) as i32;
        frame.rect(82, 13, length + 1, 5, 15);
        frame.rect(
            82 + length,
            13,
            222 - 82 - length,
            5,
            word(data, 0x6346)? as u8,
        );
        Ok(())
    }
    fn render(&self, frame: &mut Frame, data: &[u8]) -> Result<()> {
        let mut draw = |side: usize| -> Result<()> {
            let u = &self.units[side];
            let sprite = usize::from(data[u.sequence + usize::from(u.draw_animation) * 4 + 2]);
            frame.blit(
                u.sprites
                    .get(sprite)
                    .ok_or("Invalid arena animation sprite")?,
                i32::from(u.x),
                175,
            );
            Ok(())
        };
        if self.units[1].animation == self.units[1].default {
            draw(0)?;
            draw(1)?;
        } else {
            draw(1)?;
            draw(0)?;
        }
        // Resident 3233 is the narrow two-pixel bevel, unlike message boxes.
        frame.rect(79, 10, 156, 11, 26);
        frame.rect(79, 10, 2, 11, 8);
        frame.rect(233, 10, 2, 11, 8);
        frame.rect(79, 10, 156, 1, 15);
        frame.rect(80, 11, 154, 1, 15);
        frame.rect(79, 20, 156, 1, 0);
        frame.rect(80, 19, 154, 1, 0);
        self.bar(frame, data)?;
        for side in 0..2 {
            let length = i32::from(self.units[side].health.max(0)) * 128 / 50;
            let (left, right) = if side == 0 {
                (319 - length, 319)
            } else {
                (0, length)
            };
            frame.rect(if side == 0 { 191 } else { 0 }, 188, 129, 6, 54);
            frame.rect(left + i32::from(side == 0), 188, right - left, 6, 4);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fighter(id: u8, sequence: usize, x: i16) -> Fighter {
        Fighter {
            id,
            sequence,
            animation: 0,
            draw_animation: 0,
            default: 0,
            x,
            skill: 2,
            health: 10,
            sprites: Vec::new(),
            moving: false,
        }
    }
    fn fight() -> Fight {
        Fight {
            units: [fighter(0, 100, 150), fighter(1, 200, 131)],
            held: 0,
            crowd: 2,
            decay: 0,
            input_wait: [0; 2],
            frame_wait: [6; 2],
            hits: 0,
            blocks: 0,
            cheers: 0,
            jeers: 0,
            level: 1,
        }
    }
    fn arena(fight: Fight) -> Arena {
        Arena {
            stage: Stage::Thumbs,
            after: After::Done,
            message: String::new(),
            elapsed: 0,
            offers: [0; 6],
            draw_offers: [0; 6],
            offered: true,
            first: Some(0),
            second: Some(1),
            chosen: 0,
            original_treasury: 100,
            selected: None,
            training: Menu {
                joined_decoration: true,
                rect: (0, 0, 0, 0),
                title: String::new(),
                items: Vec::new(),
            },
            intro: Bitmap {
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
                pixels: Vec::new(),
                visible: Vec::new(),
            },
            floor: Bitmap {
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
                pixels: Vec::new(),
                visible: Vec::new(),
            },
            arena: Vec::new(),
            show: Vec::new(),
            fight: Some(fight),
            versus: false,
            enthusiastic: false,
            dead: [false; 2],
        }
    }
    #[test]
    fn attack_reach_and_guard_height_affect_damage_but_both_excite_the_crowd() {
        let mut data = vec![0; 65536];
        data[103] = 4; // high strike
        data[203] = 16; // high guard
        let mut f = fight();
        f.hit(&data, 0, &crate::sound::Bus::default());
        assert_eq!(
            (f.units[1].health, f.blocks, f.hits, f.crowd),
            (10, 1, 0, 3)
        );
        data[203] = 8; // low guard does not block the high strike
        f.hit(&data, 0, &crate::sound::Bus::default());
        assert_eq!((f.units[1].health, f.blocks, f.hits, f.crowd), (7, 1, 1, 4));
        f.units[1].x = 130; // exactly 20 is outside reach
        f.hit(&data, 0, &crate::sound::Bus::default());
        assert_eq!((f.units[1].health, f.blocks, f.hits, f.crowd), (7, 1, 1, 4));
    }
    #[test]
    fn mercy_preserves_the_losers_identity_and_killing_replaces_the_correct_fighter() {
        for kill in [false, true] {
            let mut state = Campaign::test_data(vec![0; 65536]);
            put(&mut state, 0x594a, 100);
            state.data[record(1) + 2] = 7;
            let mut f = fight();
            f.units[1].health = 0;
            f.units[0].animation = 3;
            f.units[0].draw_animation = 2;
            f.units[1].animation = 255;
            f.units[1].draw_animation = 9;
            let mut a = arena(f);
            let mut rng = OriginalRng {
                first: 15,
                second: 40,
            };
            let mut expected = rng;
            if kill {
                expected.bounded(2);
                expected.bounded(4);
            }
            a.thumbs(&mut state, &mut rng, kill).unwrap();
            assert_eq!(
                word(&state.data, 0x594a).unwrap(),
                if kill { 105 } else { 95 }
            );
            assert_eq!(state.data[record(1) + 2], if kill { 8 } else { 7 });
            assert_eq!((rng.first, rng.second), (expected.first, expected.second));
            let f = a.fight.as_ref().unwrap();
            assert_eq!(f.units[0].draw_animation, if kill { 8 } else { 2 });
            assert_eq!(f.units[1].draw_animation, if kill { 0 } else { 9 });
            assert_eq!(a.stage, Stage::Final);
        }
    }
}
