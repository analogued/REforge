// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Naval encounter and flagship scene (overlay 5). Simulation is recovered here.
#[path = "naval_animation.rs"]
mod animation;

use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::naval_combat::{self as combat, Geometry};
use crate::naval_motion::{self as motion, Clock, signed};
use crate::rng::OriginalRng;
use crate::state::Campaign;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Ship,
    Briefing,
    Battle,
    Report,
    Reengage,
    Boarding,
    BoardingReport,
    Animation,
}
pub struct Naval {
    pub player: usize,
    pub enemy: usize,
    pub stage: Stage,
    pub choices: Vec<usize>,
    pub view: Campaign,
    previous: Frame,
    briefing: crate::naval_intro::Briefing,
    pub ticks: u32,
    clock: Clock,
    execution: crate::naval_clock::ExecutionClock,
    music_cost: [u64; 17],
    geometry: Geometry,
    pub held: u8,
    pub result: u16,
    cause: u16,
    boarding_tail: u8,
    melee_frame: usize,
    smoke_visible: [bool; 2],
    boarding_visible: [bool; 2],
    animation: Option<animation::Animation>,
    resolution_frame: Option<Frame>,
    round_lost: u16,
    footer_normal: bool,
    speed_visible: bool,
    pub text: String,
}
fn put(ds: &mut [u8], at: usize, value: u16) {
    ds[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
impl Naval {
    pub fn new(
        assets: &Assets,
        state: &Campaign,
        player: usize,
        enemy: usize,
        previous: Frame,
    ) -> Result<Self> {
        if player >= 3 || !(3..7).contains(&enemy) {
            return Err("Invalid naval fleets".into());
        }
        let choices = (0..3)
            .filter(|kind| word(&state.data, 0x110e + player * 12 + kind * 2).unwrap_or(0) != 0)
            .collect::<Vec<_>>();
        if choices.is_empty() {
            return Err("Roman fleet has no ships".into());
        }
        state.sound.stop();
        let mut view = state.clone();
        put(
            &mut view.data,
            0x9756,
            if choices.len() == 2 { 118 } else { 130 },
        );
        view.data[0x975e] = choices.len() as u8;
        let entries: [u8; 36] = view.data[0x972c..0x9750].try_into().unwrap();
        for (row, &kind) in choices.iter().enumerate() {
            let label = word(&view.data, 0x9722 + kind * 2)?;
            view.data[0x972c + row * 12..0x972c + (row + 1) * 12]
                .copy_from_slice(&entries[row * 12..(row + 1) * 12]);
            put(&mut view.data, 0x972c + row * 12, label);
            view.data[0x972e + row * 12] = view.data[0x9728 + kind];
        }
        let briefing = crate::naval_intro::Briefing::new(assets)?;
        let mut music_cost = [26_000; 17];
        for (id, cost) in music_cost.iter_mut().enumerate().skip(13) {
            let score = crate::music::song_data(assets, id as u8)?;
            *cost += score[0x22..0x2b].iter().filter(|c| **c == 9).count() as u64 * 2_340;
        }
        let mut scene = Self {
            player,
            enemy,
            stage: Stage::Ship,
            choices,
            view,
            previous,
            briefing,
            ticks: 0,
            clock: Clock::default(),
            execution: crate::naval_clock::ExecutionClock::default(),
            music_cost,
            geometry: Geometry::new(assets)?,
            held: 0,
            result: 0,
            cause: 0,
            boarding_tail: 0,
            melee_frame: 0,
            smoke_visible: [false; 2],
            boarding_visible: [false; 2],
            animation: None,
            resolution_frame: None,
            round_lost: 0,
            footer_normal: false,
            speed_visible: false,
            text: String::new(),
        };
        if scene.choices.len() == 1 {
            scene.select(scene.choices[0])?;
        }
        Ok(scene)
    }
    pub fn diagnostic(&self) -> String {
        format!(
            "naval={:?} clock={} polls={}",
            self.stage,
            self.clock.now,
            self.execution.polls()
        )
    }
    pub fn menu(&self) -> Result<crate::province::Menu> {
        crate::province::Menu::read(&self.view, 0x9750)
    }
    pub fn select(&mut self, kind: usize) -> Result<()> {
        if !self.choices.contains(&kind) {
            return Err("Unavailable flagship".into());
        }
        let enemy = (0..3)
            .rev()
            .find(|k| word(&self.view.data, 0x110e + self.enemy * 12 + k * 2).unwrap_or(0) != 0)
            .ok_or("Enemy fleet has no ships")?;
        self.view.data[0xc37a] = enemy as u8;
        self.view.data[0xc37b] = kind as u8;
        self.view.data[0xc38e] = self.view.data[0x8ee2 + enemy];
        self.view.data[0xc38f] = self.view.data[0x8ee2 + kind];
        self.view.data[0xc3be..0xc3c4].fill(0);
        self.view.sound.play(13);
        self.stage = Stage::Briefing;
        self.ticks = 0;
        self.briefing.reset();
        Ok(())
    }
    pub fn begin_round(&mut self, state: &mut Campaign) -> Result<()> {
        state.sound.stop();
        let enemy_kind = usize::from(self.view.data[0xc37a]);
        let player_kind = usize::from(self.view.data[0xc37b]);
        for (id, kind) in [(self.enemy, enemy_kind), (self.player, player_kind)] {
            let at = 0x110e + id * 12 + kind * 2;
            let count = word(&state.data, at)?
                .checked_sub(1)
                .ok_or("No selected ship remains")?;
            put(&mut state.data, at, count);
            put(&mut self.view.data, at, count);
        }
        let count = word(&state.data, 0xc0fc)?.wrapping_add(1);
        put(&mut state.data, 0xc0fc, count);
        put(&mut self.view.data, 0xc0fc, count);
        // The original enemy cargo field is a roaming-army index; Roman cargo
        // points at a legion. Their courage tables deliberately differ.
        for (side, id, kind) in [(0, self.enemy, enemy_kind), (1, self.player, player_kind)] {
            let cargo = state.data[0x1117 + id * 12];
            let mut marines = u16::from(state.data[0x8ede + kind]);
            let morale = if cargo == 255 {
                if side == 1 {
                    marines /= 2;
                }
                state.data[0x527a + usize::from(state.data[0x1115 + id * 12]) * 32]
            } else if side == 0 {
                state.data[0xc11e + usize::from(cargo) * 14]
            } else {
                state.data[0x1923 + usize::from(cargo) * 20]
            };
            put(&mut self.view.data, 0xc37e + side * 2, marines);
            self.view.data[0xc36e + side] = morale.wrapping_add(
                state.data
                    [if side == 0 { 0x8ee6 } else { 0x8eea } + usize::from(state.data[0x491])],
            );
            self.view.data[0xc388 + side] = if side == 0 { 4 } else { 12 };
            self.view.data[0xc390 + side] = 6;
            self.view.data[0xc38c + side] = 1;
            self.view.data[0xc38a + side] = state.data[0x9819 + kind * 5];
            let at = 0x971e - side * 4;
            let x = word(&self.view.data, at)?;
            let y = word(&self.view.data, at + 2)?;
            put(&mut self.view.data, 0xc372 + side * 4, x);
            put(&mut self.view.data, 0xc374 + side * 4, y);
        }
        self.reset_motion();
        self.stage = Stage::Battle;
        self.ticks = 0;
        Ok(())
    }
    fn ship(&self, frame: &mut Frame, assets: &Assets, side: usize) -> Result<()> {
        self.ship_parts(frame, assets, side, 2)
    }
    fn ship_parts(&self, frame: &mut Frame, assets: &Assets, side: usize, part: u8) -> Result<()> {
        let direction = usize::from(self.view.data[0xc388 + side]);
        let oar = usize::from(self.view.data[0xc390 + side]);
        let oar_base = usize::from(self.view.data[0x93e6 + direction]);
        let (stem, frame_id) = if direction < 8 {
            ("OARS1", oar_base + oar)
        } else {
            ("OARS2", oar_base - 56 + oar)
        };
        let oars = assets.sprites("NAVL", stem)?;
        let kind = usize::from(self.view.data[0xc37a + side]);
        let hulls = assets.sprites("MNAVAL", ["SML", "MED", "LRG"][kind])?;
        let x = i32::from(word(&self.view.data, 0xc372 + side * 4)? as i16);
        let y = i32::from(word(&self.view.data, 0xc374 + side * 4)? as i16);
        if part != 1 {
            frame.blit(&oars[frame_id], x, y);
        }
        if part != 0 {
            frame.blit(&hulls[direction], x, y);
        }
        if part != 0 && self.smoke_visible[side] {
            let effects = assets.sprites("MNAVAL", "BOMBS")?;
            frame.blit(
                &effects[48 + usize::from(self.view.data[0x98e0 + side])],
                x,
                y,
            );
        }
        Ok(())
    }
    pub fn render(&self, assets: &Assets, selected: Option<usize>) -> Result<Frame> {
        if self.stage == Stage::Animation {
            return Ok(self
                .animation
                .as_ref()
                .ok_or("Missing naval animation")?
                .frame
                .clone());
        }
        if self.stage == Stage::Report
            && let Some(frame) = &self.resolution_frame
        {
            let mut frame = frame.clone();
            crate::legion::message(&mut frame, assets, &self.view, &self.text)?;
            return Ok(frame);
        }

        if self.stage == Stage::Briefing {
            return self.briefing.render(assets, &self.view);
        }
        if self.stage == Stage::Ship {
            let mut f = self.previous.clone();
            let menu = self.menu()?;
            f.bevel(menu.rect.0, menu.rect.1, menu.rect.2, menu.rect.3, 26);
            f.center_text(
                assets,
                &menu.title,
                (menu.rect.0 + menu.rect.2) / 2,
                menu.rect.1 + 6,
                15,
            )?;
            let button = assets.sprites("MINIT", "INIT")?.remove(1);
            for (row, item) in menu.items.iter().enumerate() {
                f.blit(&button, item.rect.0, item.rect.1 + 1);
                f.text(
                    assets,
                    &item.label,
                    item.rect.0 + 11,
                    item.rect.1,
                    if selected == Some(row) { 15 } else { 0 },
                )?;
            }
            return Ok(f);
        }
        let mut f = Frame::new();
        f.rect(0, 0, 320, 186, 9);
        f.rect(0, 186, 320, 1, 0);
        f.rect(0, 187, 320, 13, 26);
        let text = self.view.string(0x97ba)?;
        let font = 0xa3b4;
        let widths = word(&assets.data_segment, font)? as usize;
        let w: i32 = text
            .bytes()
            .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
            .sum();
        f.text_font(assets, &text, 159 - w / 2, 187, 0, font)?;
        let left = i32::from(self.view.data[0xc38e]) * 115 / 60;
        let right = 319 - i32::from(self.view.data[0xc38f]) * 115 / 60;
        f.rect(0, 190, left + 1, 7, 4);
        // DOS 7E74 sorts rectangle endpoints before clipping. Hull values can
        // exceed the nominal 60; the reversed background span must still draw.
        f.rect(left.min(115), 190, (left - 115).abs() + 1, 7, 26);
        f.rect(right, 190, 320 - right, 7, 4);
        f.rect(right.min(204), 190, (right - 204).abs() + 1, 7, 26);
        f.rect(119, 193, 81, 7, 26);
        let footer_font = if self.footer_normal { 0xa05c } else { font };
        let footer_widths = word(&assets.data_segment, footer_font)? as usize;
        let show_speed = self.speed_visible;
        let text = self.view.string(if show_speed {
            word(
                &self.view.data,
                0x9870 + usize::from(self.view.data[0xc38d]) * 2,
            )? as usize
        } else {
            0x98ba
        })?;
        let w: i32 = text
            .bytes()
            .map(|b| i32::from(assets.data_segment[footer_widths + usize::from(b - 32)]) + 1)
            .sum();
        f.text_font(assets, &text, 159 - w / 2, 193, 0, footer_font)?;
        for (side, x, right_align) in [(0, 144, true), (1, 174, false)] {
            let id = if side == 0 { self.enemy } else { self.player };
            let count = (0..3)
                .map(|kind| word(&self.view.data, 0x110e + id * 12 + kind * 2))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .fold(0u16, u16::wrapping_add)
                .to_string();
            let count_width: i32 = count
                .bytes()
                .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                .sum();
            f.text_font(
                assets,
                &count,
                x - if right_align { count_width } else { 0 },
                187,
                0,
                font,
            )?;
            if show_speed {
                continue;
            }
            let text = word(&self.view.data, 0xc37e + side * 2)?.to_string();
            let w: i32 = text
                .bytes()
                .map(|b| i32::from(assets.data_segment[footer_widths + usize::from(b - 32)]) + 1)
                .sum();
            f.text_font(
                assets,
                &text,
                x - if right_align { w } else { 0 },
                193,
                0,
                footer_font,
            )?;
        }
        let footer = f.pixels[186 * 320..].to_vec();
        let order = usize::from(self.view.data[0xc384]);
        for part in [0, 1] {
            self.ship_parts(&mut f, assets, order ^ 1, part)?;
            self.ship_parts(&mut f, assets, order, part)?;
        }
        let effects = assets.sprites("MNAVAL", "BOMBS")?;
        if !matches!(self.stage, Stage::Boarding | Stage::BoardingReport) {
            for side in [1, 0] {
                if !self.boarding_visible[side] {
                    continue;
                }
                let ds = &self.view.data;
                let dir = usize::from(ds[0xc388 + side]);
                let delta = ds[0xc388 + side].wrapping_sub(ds[0xc370 + side]) & 15;
                let (table, xs, ys) = if delta > 0 && delta < 8 {
                    (0x95d4, 0x95f4, 0x9624)
                } else if delta > 8 {
                    (0x95e4, 0x9654, 0x9684)
                } else {
                    continue;
                };
                let i = usize::from(ds[0xc37a + side]) * 16 + dir;
                let frame = 58 + usize::from(ds[table + dir]);
                let x = signed(ds, 0xc372 + side * 4).wrapping_add(i16::from(ds[xs + i] as i8));
                let y = signed(ds, 0xc374 + side * 4).wrapping_add(i16::from(ds[ys + i] as i8));
                f.blit(&effects[frame], i32::from(x), i32::from(y));
            }
        }
        for side in [1, 0] {
            if self.view.data[0xc392 + side] != 0 {
                let frame = usize::from(self.view.data[0x98be + side])
                    + if self.view.data[0xc392 + side] == 1 {
                        0
                    } else {
                        usize::from(self.view.data[0xc3cc + side])
                    };
                f.blit(
                    effects.get(frame).ok_or("Invalid arrow frame")?,
                    i32::from(signed(&self.view.data, 0xc3d0 + side * 4)),
                    i32::from(signed(&self.view.data, 0xc3d2 + side * 4)),
                );
            }
            if self.view.data[0xc3ba + side] != 0 {
                let frame = 32 + usize::from(self.view.data[0xc3ce + side]);
                f.blit(
                    &effects[frame],
                    i32::from(signed(&self.view.data, 0xc3d8 + side * 4)),
                    i32::from(signed(&self.view.data, 0xc3da + side * 4)),
                );
            }
        }
        if matches!(self.stage, Stage::Boarding | Stage::BoardingReport) {
            let x = (i32::from(signed(&self.view.data, 0xc372))
                + i32::from(signed(&self.view.data, 0xc376)))
                / 2;
            let y = (i32::from(signed(&self.view.data, 0xc374))
                + i32::from(signed(&self.view.data, 0xc378)))
                / 2;
            f.blit(&assets.sprites("MNAVAL", "SMOK")?[self.melee_frame], x, y);
        }
        f.pixels[186 * 320..].copy_from_slice(&footer);
        if self.stage == Stage::Reengage {
            f.rect(0, 0, 320, 186, 9);
        }
        if matches!(self.stage, Stage::Report | Stage::BoardingReport) {
            crate::legion::message(&mut f, assets, &self.view, &self.text)?;
        }
        Ok(f)
    }
}

impl Naval {
    fn reset_motion(&mut self) {
        let ds = self.view.data.as_mut_slice();
        for side in 0..2 {
            ds[0xc388 + side] = if side == 0 { 4 } else { 12 };
            ds[0xc390 + side] = 6;
            ds[0xc38c + side] = 1;
            ds[0xc38a + side] = ds[0x9819 + usize::from(ds[0xc37a + side]) * 5];
            for axis in [0, 2] {
                let coordinate = signed(ds, 0x971e - side * 4 + axis);
                put(ds, 0xc372 + side * 4 + axis, coordinate as u16);
                put(ds, 0xc398 + side * 4 + axis, 0);
            }
        }
        for at in [
            0xc3a0, 0xc3a1, 0xc3aa, 0xc3ab, 0xc37c, 0xc394, 0xc395, 0xc396, 0xc397, 0xc392, 0xc393,
            0xc3ba, 0xc3bb,
        ] {
            ds[at] = 0;
        }
        put(ds, 0xc384, 1);
        put(ds, 0xc386, 216);
        self.footer_normal = false;
        self.speed_visible = false;
        self.clock = Clock::default();
        // Timer zero still contains the briefing's elapsed time at B248.
        self.clock.stamps[0] = u32::MAX - 2;
        self.execution = crate::naval_clock::ExecutionClock::default();
        self.held = 0;
        self.cause = 0;
    }
    pub fn fire_arrows(&mut self) {
        combat::shoot(&mut self.view.data, &mut self.clock, 1);
    }
    pub fn board_enemy(&mut self) {
        self.view.data[0xc3ab] = 20;
    }
    pub fn retreat(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        self.cause = 4;
        self.finish_round(assets, state, rng)
    }
    pub fn reengage(
        &mut self,
        assets: &Assets,
        yes: bool,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        if yes {
            self.reset_motion();
            self.stage = Stage::Battle;
            Ok(())
        } else {
            self.footer_normal = true;
            self.finish_round(assets, state, rng)
        }
    }
    pub fn acknowledge_boarding(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        self.finish_round(assets, state, rng)
    }
    pub fn advance(
        &mut self,
        assets: &Assets,
        count: u32,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        for _ in 0..count {
            self.ticks = self.ticks.wrapping_add(1);
            if self.stage == Stage::Briefing {
                self.briefing.advance(&mut self.view);
                state.data[0x9816..0x9818].copy_from_slice(&self.view.data[0x9816..0x9818]);
                continue;
            }
            if self.stage == Stage::Animation {
                let mut animation = self.animation.take().ok_or("Missing naval animation")?;
                if animation.advance(self, assets, rng)? {
                    self.resolution_frame = Some(animation.frame);
                    self.finish_settlement(assets, state, rng)?;
                } else {
                    self.animation = Some(animation);
                }
                continue;
            }
            if !matches!(self.stage, Stage::Battle | Stage::Boarding) {
                continue;
            }
            if self.stage == Stage::Boarding {
                self.clock.now = self.clock.now.wrapping_add(1);
                self.boarding_tick(rng)?;
                continue;
            }
            self.execution.advance_tick();
            while let Some(now) = self.execution.next_poll() {
                self.clock.now = now;
                let repaint = self.clock.elapsed(0) > 2;
                let checkpoint = state.sound.checkpoint();
                self.poll(assets, state, rng)?;
                self.execution.charge(
                    repaint,
                    state.sound.driver_cost_since(checkpoint, &self.music_cost),
                );
                if self.stage != Stage::Battle {
                    break;
                }
            }
            self.clock.now = self.execution.now();
        }
        Ok(())
    }

    fn poll(&mut self, assets: &Assets, state: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        motion::bearings(&mut self.view.data)?;
        let ds = self.view.data.as_mut_slice();
        if self.held & 8 != 0 {
            ds[0xc395] = 1;
        }
        if self.held & 4 != 0 {
            ds[0xc395] = 255;
        }
        if self.held & 2 != 0 {
            ds[0xc397] = 255;
        }
        if self.held & 1 != 0 {
            ds[0xc397] = 1;
        }
        if motion::ai(ds, rng) {
            combat::shoot(ds, &mut self.clock, 0);
        }
        for side in [1, 0] {
            motion::turn(ds, &mut self.clock, side);
        }
        let sound_speed = self.clock.elapsed(12) > u32::from(ds[0x982c + usize::from(ds[0xc37b])]);
        for side in [1, 0] {
            motion::speed(ds, &mut self.clock, side);
        }
        if sound_speed {
            if ds[0xc38b] == 0 {
                state.sound.stop();
            } else {
                state.sound.play(ds[0x9704 + usize::from(ds[0xc38d])]);
            }
        }
        for side in [1, 0] {
            motion::movement(ds, &mut self.clock, side);
        }
        if self.cause == 0 {
            self.cause = motion::bounds(ds);
        }
        self.geometry.update(ds)?;
        let mut boarding = self.geometry.ram(ds, rng);
        for side in [1, 0] {
            self.geometry.arrow(ds, &mut self.clock, side, rng);
        }
        for side in [1, 0] {
            combat::start_fire(ds, side);
        }
        for side in [1, 0] {
            self.geometry.fire(ds, &mut self.clock, side, rng);
        }
        if self.clock.elapsed(8) > u32::from(word(ds, 0xc386)?) {
            self.clock.reset(8);
            crate::naval_fleet::attrition(ds, self.player, self.enemy, rng)?;
            put(ds, 0xc386, (rng.bounded(9) + 2) * 72);
        }
        if self.clock.elapsed(0) > 2 {
            self.clock.reset(0);
            for side in [1, 0] {
                self.smoke_visible[side] = ds[0xc3a0 + side] != 0;
                if ds[0xc3a0 + side] != 0 {
                    ds[0xc3a0 + side] -= 1;
                    if ds[0xc3a0 + side] & 15 == 0 {
                        ds[0xc38e + side] = ds[0xc38e + side].wrapping_sub(1);
                    }
                    ds[0x98e0 + side] = (ds[0x98e0 + side] + 1) % 3;
                }
                self.boarding_visible[side] = ds[0xc3aa + side] != 0;
                boarding |= self.geometry.board(ds, side);
            }
            self.speed_visible = ds[0xc37c] != 0;
            if ds[0xc37c] != 0 {
                ds[0xc37c] -= 1;
            }
            for side in 0..2 {
                if ds[0xc38e + side] as i8 <= 0 {
                    ds[0xc38e + side] = 0;
                    put(ds, 0xc37e + side * 2, 0);
                }
            }
        }
        if ds[0xc38e] == 0 || ds[0xc38f] == 0 {
            self.finish_round(assets, state, rng)?;
            return Ok(());
        }
        if boarding != 0 {
            for (side, flag) in [(0, 2), (1, 1)] {
                if boarding & flag != 0 {
                    ds[0xc36e + side] = ds[0xc36e + side].wrapping_add(10);
                }
                ds[0xc390 + side] = 6;
            }
            ds[0xc37c] = 0;
            put(ds, 0xc386, 72);
            self.boarding_tail = 20;
            state.sound.play(16);
            self.stage = Stage::Boarding;
        } else if self.cause & 4 != 0 {
            state.sound.play(13);
            self.stage = Stage::Reengage;
            self.held = 0;
        } else if self.cause & 8 != 0 {
            self.finish_round(assets, state, rng)?;
        }
        Ok(())
    }
    fn boarding_tick(&mut self, rng: &mut OriginalRng) -> Result<()> {
        if self.clock.elapsed(2) <= 8 {
            return Ok(());
        }
        self.clock.reset(2);
        self.boarding_tail = self.boarding_tail.saturating_sub(1);
        let ds = self.view.data.as_mut_slice();
        if self.clock.elapsed(8) > u32::from(word(ds, 0xc386)?) {
            self.clock.reset(8);
            put(ds, 0xc386, rng.bounded(108));
            let mut hits = [0u16; 2];
            for (side, hit) in hits.iter_mut().enumerate() {
                *hit = combat::marine_hits(ds, side, ds[0xc36e + side], rng)?;
            }
            for side in 0..2 {
                put(
                    ds,
                    0xc37e + side * 2,
                    combat::subtract(word(ds, 0xc37e + side * 2)?, hits[side ^ 1]),
                );
            }
        }
        // The melee sprite is also selected randomly once per displayed frame.
        self.melee_frame = usize::from(rng.bounded(3));
        if (word(ds, 0xc37e)? == 0 || word(ds, 0xc380)? == 0) && self.boarding_tail == 0 {
            if word(ds, 0xc37e)? == 0 {
                ds[0xc38e] = 0;
            }
            if word(ds, 0xc380)? == 0 {
                ds[0xc38f] = 0;
            }
            let (label, outcome) = if ds[0xc38f] == 0 {
                (0x96c4, if ds[0xc38e] == 0 { 0x96cc } else { 0x96c8 })
            } else {
                (0x96c6, 0x96ca)
            };
            let labels = [
                word(ds, label)? as usize,
                word(ds, 0x96d0)? as usize,
                word(ds, outcome)? as usize,
            ];
            self.text = String::new();
            for address in labels {
                self.text.push_str(&self.view.string(address)?);
            }
            self.stage = Stage::BoardingReport;
        }
        Ok(())
    }
    fn finish_round(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        // C71B updates the footer before returning the selected ships. Preserve
        // that displayed field (the re-engagement question clears its sea).
        self.view.data[0xc37c] = 0;
        self.speed_visible = false;
        let prior_stage = self.stage;
        if self.stage == Stage::BoardingReport {
            self.stage = Stage::Boarding;
        }
        self.resolution_frame = Some(self.render(assets, None)?);
        self.stage = prior_stage;
        let ds = self.view.data.as_mut_slice();
        let mut lost = 0u16;
        for (side, id, counter, flag) in [(0, self.enemy, 0xc08a, 2), (1, self.player, 0xc08e, 1)] {
            if ds[0xc38e + side] == 0 {
                lost |= flag;
                crate::naval_fleet::counter(ds, counter, 1);
            } else {
                let at = 0x110e + id * 12 + usize::from(ds[0xc37a + side]) * 2;
                put(ds, at, word(ds, at)?.wrapping_add(1));
            }
        }
        ds[0xc37c] = 0;
        self.round_lost = lost;
        if lost != 0 {
            let animation = animation::Animation::new(self, assets, lost)?;
            self.animation = Some(animation);
            self.stage = Stage::Animation;
            self.held = 0;
            return Ok(());
        }
        self.finish_settlement(assets, state, rng)
    }
    fn finish_settlement(
        &mut self,
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        let ds = self.view.data.as_mut_slice();
        let lost = self.round_lost;
        if self.cause == 8 {
            ds[0xc38e] = 0;
        }
        if ds[0xc38e] == 0 || ds[0xc38f] == 0 {
            while !crate::naval_fleet::attrition(ds, self.player, self.enemy, rng)? {}
        }
        if (ds[0xc38e] == 0 || ds[0xc38f] == 0)
            && let Some(frame) = &mut self.resolution_frame
        {
            counts_footer(frame, assets, ds, self.enemy, self.player)?;
        }

        let mut result = 0;
        for (id, flag) in [(self.player, 1), (self.enemy, 2)] {
            let remaining = (0..3)
                .map(|k| word(ds, 0x110e + id * 12 + k * 2).unwrap_or(0))
                .fold(0u16, u16::wrapping_add);
            if remaining == 0 {
                result |= flag;
                ds[0x1116 + id * 12] = 255;
                let cargo = usize::from(ds[0x1117 + id * 12]);
                if cargo != 255 {
                    if flag == 1 {
                        ds[0x1925 + cargo * 20] = 255;
                        ds[0x1924 + cargo * 20] = 255;
                        let leader = usize::from(ds[0x1928 + cargo * 20]);
                        if ds[0x1a20 + leader * 6] != 2 {
                            ds[0x1a20 + leader * 6] = 1;
                        }
                    } else {
                        put(ds, 0xc11a + cargo * 14, 0);
                    }
                }
            }
        }
        if result == 0 {
            result = 4;
        }
        self.result = result;
        let first = if self.cause == 8 {
            0x8ff2
        } else {
            match lost {
                0 => 0x3e02,
                1 => 0x8f52,
                2 => 0x8eee,
                _ => 0x8f96,
            }
        };
        let second = match result {
            1 => {
                if self.cause == 8 {
                    0x90b0
                } else if lost & 1 != 0 {
                    0x913c
                } else {
                    0x9220
                }
            }
            2 => {
                put(ds, 0xc0fe, word(ds, 0xc0fe)?.wrapping_add(1));
                if self.cause == 8 {
                    0x904c
                } else if lost & 1 != 0 {
                    0x9104
                } else {
                    0x9198
                }
            }
            3 => 0x92b2,
            _ => 0x9318,
        };
        put(ds, 0x934a, word(ds, 0x1100 + self.player * 2)?);
        put(ds, 0x934e, word(ds, 0x1100 + self.enemy * 2)?);
        put(
            ds,
            0x9352,
            word(ds, 0x5222 + usize::from(ds[0x1114 + self.enemy * 12]) * 2)?,
        );
        put(ds, 0x9356, first);
        put(ds, 0x9358, second);
        let mut text = String::new();
        for at in (0x934a..0x935a).step_by(2) {
            text.push_str(&self.view.string(word(&self.view.data, at)? as usize)?);
        }
        self.text = text;
        state.sound.play(13);
        self.stage = Stage::Report;
        self.held = 0;
        state.load(&self.view.save())?;
        Ok(())
    }
}

/// Render an observed battle state without interpreting any original code.
pub fn render_observation(assets: &Assets, ds: &[u8]) -> Result<Frame> {
    let mut state = Campaign::new(assets)?;
    state.data.copy_from_slice(ds);
    let mut scene = Naval::new(assets, &state, 0, 3, Frame::new())?;
    scene.view.data.copy_from_slice(ds);
    scene.stage = Stage::Battle;
    scene.speed_visible = ds[0xc37c] != 0;
    for side in 0..2 {
        scene.boarding_visible[side] = ds[0xc3aa + side] != 0;
        scene.smoke_visible[side] = ds[0xc3a0 + side] != 0;
        if scene.smoke_visible[side] {
            scene.view.data[0xc3a0 + side] -= 1;
            if scene.view.data[0xc3a0 + side] & 15 == 0 {
                scene.view.data[0xc38e + side] = scene.view.data[0xc38e + side].wrapping_sub(1);
            }
            scene.view.data[0x98e0 + side] = (scene.view.data[0x98e0 + side] + 1) % 3;
        }
    }
    for side in 0..2 {
        if scene.view.data[0xc38e + side] as i8 <= 0 {
            scene.view.data[0xc38e + side] = 0;
            put(&mut scene.view.data, 0xc37e + side * 2, 0);
        }
    }
    scene.render(assets, None)
}

/// Check the complete resolution from an observed round-end state. No DOS
/// instructions are run; this is the same native settlement and animation path.
pub fn resolve_observation(
    assets: &Assets,
    ds: &[u8],
    cause: u16,
    rng: &mut OriginalRng,
) -> Result<(Frame, Vec<u8>)> {
    let mut state = Campaign::new(assets)?;
    state.data.copy_from_slice(ds);
    let mut initial = state.clone();
    for (side, id) in [(0, 3), (1, 0)] {
        let at = 0x110e + id * 12 + usize::from(ds[0xc37a + side]) * 2;
        put(&mut initial.data, at, word(ds, at)?.wrapping_add(1));
    }
    let mut scene = Naval::new(assets, &initial, 0, 3, Frame::new())?;
    scene.view.data.copy_from_slice(ds);
    let normal_font = ds[0x9b56..0x9b60] == ds[0xa05c..0xa066];
    scene.stage = if (normal_font && cause & 4 != 0) || cause & 256 != 0 {
        Stage::Reengage
    } else {
        Stage::Battle
    };
    scene.cause = cause & 255;
    scene.footer_normal = normal_font;
    scene.finish_round(assets, &mut state, rng)?;
    for _ in 0..10000 {
        if scene.stage != Stage::Animation {
            break;
        }
        scene.advance(assets, 1, &mut state, rng)?;
    }
    if scene.stage != Stage::Report {
        return Err("Naval resolution did not terminate".into());
    }
    Ok((scene.render(assets, None)?, scene.view.data))
}

fn counts_footer(
    frame: &mut Frame,
    assets: &Assets,
    ds: &[u8],
    enemy: usize,
    player: usize,
) -> Result<()> {
    let font = 0xa3b4;
    let widths = word(&assets.data_segment, font)? as usize;
    let width = |text: &str| {
        text.bytes()
            .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
            .sum::<i32>()
    };
    frame.rect(119, 187, 81, 6, 26);
    let label = std::str::from_utf8(&ds[0x97ba..0x97bf]).map_err(|e| e.to_string())?;
    frame.text_font(assets, label, 159 - width(label) / 2, 187, 0, font)?;
    for (id, x, right) in [(enemy, 144, true), (player, 174, false)] {
        let text = (0..3)
            .map(|kind| word(ds, 0x110e + id * 12 + kind * 2).unwrap_or(0))
            .fold(0u16, u16::wrapping_add)
            .to_string();
        frame.text_font(
            assets,
            &text,
            x - if right { width(&text) } else { 0 },
            187,
            0,
            font,
        )?;
    }
    Ok(())
}
