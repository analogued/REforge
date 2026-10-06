// Copyright (c) 2026 analogued. Project license has not been selected.
//! Post-game title zooms over the retained world and the best-score presentation.

use super::{EnemyState, Random, Snapshot, special::Special};
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, ROW_BYTES, Rect},
    scene::{self, Draw},
};

const VIEWPORT: Rect = Rect {
    x_bytes: 4,
    y: 44,
    width_bytes: 72,
    height: 114,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndingTitle {
    GameOver,
    HighScore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndingStage {
    Approach,
    ScoreOverlay,
    Hold,
    Departure,
    Pause,
}

#[derive(Clone, Debug)]
pub struct EndingSnapshot {
    pub title: EndingTitle,
    pub stage: EndingStage,
    /// Zero-based draw index within the approach or departure.
    pub draw_index: u16,
    /// The fifty two-unit hold waits are numbered one through fifty.
    pub hold_tick: u16,
    /// The first pair is mandatory; subsequent pairs permit returning to intro.
    pub cycle: u64,
    pub best_score: u32,
    pub world: [EnemyState; 29],
    pub full_world_order: [usize; 29],
    pub score_digits: [Draw; 6],
    pub random: [u16; 3],
}

#[derive(Debug)]
pub struct Ending {
    state: EndingSnapshot,
    special: Special,
    random: Random,
    current_score: u32,
    vertical_origin: i32,
    camera_height: i16,
    strip_index: usize,
    strip_y: usize,
    frame: Frame,
    returned_intro: Option<bool>,
}

impl Ending {
    pub fn new(assets: &GraphicsAssets, state: &Snapshot, frame: &Frame) -> Result<Self, Error> {
        Self::new_with_best(assets, state, frame, 0)
    }

    /// Preserve a best score across games; a fresh application starts at zero.
    pub fn new_with_best(
        assets: &GraphicsAssets,
        state: &Snapshot,
        frame: &Frame,
        previous_best: u32,
    ) -> Result<Self, Error> {
        let mut world = std::array::from_fn(|slot| empty_record(slot, 0, 0, 0, 0));
        let player = state.player;
        world[0] = empty_record(
            0,
            player.requested_sprite,
            player.coordinates[0],
            player.coordinates[2],
            255,
        );
        world[0].lateral = player.coordinates[1];
        world[0].active = true;
        world[0].resolved_sprite = player.resolved_sprite;
        world[0].rect = player.rect;
        world[1] = empty_record(
            1,
            player.shadow.resolved_sprite,
            player.coordinates[0] + 1,
            -115,
            0,
        );
        world[1].lateral = player.coordinates[1];
        world[1].active = true;
        world[1].rect = player.shadow.rect;
        // Shot source selection runs even when the two slots remain inactive.
        world[2] = empty_record(2, player_projectile_source(400), 400, -100, 254);
        world[3] = empty_record(3, player_projectile_source(700), 700, -100, 254);
        for record in state.enemies {
            world[record.slot] = record;
        }
        for record in state.terrain {
            world[record.slot] = EnemyState {
                slot: record.slot,
                depth: record.depth,
                lateral: record.lateral,
                height: -100,
                active: record.active,
                movement: record.movement,
                variant: record.kind,
                animation_phase: record.animation_phase,
                requested_sprite: record.requested_sprite,
                resolved_sprite: record.resolved_sprite,
                rect: record.rect,
                phase: record.phase,
            };
        }
        for record in state.special {
            world[record.slot] = special_record(record);
        }
        let current_score = state.hud.score % 1_000_000;
        let random = Random {
            words: state.random,
        };
        let mut ending = Self {
            state: EndingSnapshot {
                title: EndingTitle::GameOver,
                stage: EndingStage::Approach,
                draw_index: 0,
                hold_tick: 0,
                cycle: 0,
                best_score: previous_best % 1_000_000,
                world,
                full_world_order: state.full_world_order,
                score_digits: score_digits(current_score, 34, 33),
                random: state.random,
            },
            special: Special::from_snapshots(state.special),
            random,
            current_score,
            vertical_origin: state.vertical_origin,
            camera_height: state.player.horizon,
            strip_index: state.strip_index,
            strip_y: state.strip_y,
            frame: frame.clone(),
            returned_intro: None,
        };
        ending.begin_title(assets, EndingTitle::GameOver)?;
        Ok(ending)
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn snapshot(&self) -> EndingSnapshot {
        self.state.clone()
    }

    pub fn phase(&self) -> EndingStage {
        self.state.stage
    }

    /// Nominal timer units, excluding deferred audio and CPU/render costs.
    pub fn wait_units(&self) -> u32 {
        match self.state.stage {
            EndingStage::ScoreOverlay => 0,
            EndingStage::Pause => 6,
            _ => 2,
        }
    }

    pub fn period(&self) -> std::time::Duration {
        super::BODY_PERIOD * self.wait_units() / 3
    }

    pub fn can_return(&self) -> bool {
        self.state.cycle != 0
            && matches!(
                self.state.stage,
                EndingStage::Approach | EndingStage::Hold | EndingStage::Departure
            )
            && self.returned_intro.is_none()
    }

    /// On a permitted fresh input, select the original raw CGA intro image.
    /// True selects DATA6; false selects DATA1. Repeated calls retain the choice.
    pub fn returned_intro_choice(&mut self) -> Option<bool> {
        if let Some(choice) = self.returned_intro {
            return Some(choice);
        }
        if !self.can_return() {
            return None;
        }
        let choice = self.random.next_word() & 7 < 4;
        self.state.random = self.random.state();
        self.returned_intro = Some(choice);
        Some(choice)
    }

    /// Advance one draw or hold wait. The world remains frozen during the titles.
    pub fn advance(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        if self.returned_intro.is_some() {
            return Ok(());
        }
        match self.state.stage {
            EndingStage::Approach => {
                self.state.world[5].depth -= 28;
                if self.state.world[5].depth > 40 {
                    self.state.draw_index += 1;
                    self.render(assets, false)?;
                } else if self.state.title == EndingTitle::HighScore {
                    self.state.stage = EndingStage::ScoreOverlay;
                    let rect = self.state.world[5].rect;
                    self.state.score_digits =
                        score_digits(self.state.best_score, rect.x_bytes + 1, rect.y + 28);
                    // The overlay resolves HUD sources without projecting the world.
                    self.render_overlay(assets)?;
                } else {
                    self.state.stage = EndingStage::Hold;
                    self.state.hold_tick = 1;
                }
            }
            EndingStage::ScoreOverlay => {
                self.state.stage = EndingStage::Hold;
                self.state.hold_tick = 1;
            }
            EndingStage::Hold => {
                if self.state.hold_tick < 50 {
                    self.state.hold_tick += 1;
                } else {
                    self.state.stage = EndingStage::Departure;
                    self.state.draw_index = 0;
                    self.state.world[5].depth += 28;
                    self.render(assets, false)?;
                }
            }
            EndingStage::Departure => {
                self.state.world[5].depth += 28;
                if self.state.world[5].depth < 1000 {
                    self.state.draw_index += 1;
                    self.render(assets, false)?;
                } else if self.state.cycle == 0 {
                    if self.state.title == EndingTitle::HighScore {
                        self.state.cycle = 1;
                    }
                    self.begin_title(assets, self.other_title())?;
                } else {
                    self.state.stage = EndingStage::Pause;
                }
            }
            EndingStage::Pause => self.begin_title(assets, self.other_title())?,
        }
        Ok(())
    }

    fn other_title(&self) -> EndingTitle {
        match self.state.title {
            EndingTitle::GameOver => EndingTitle::HighScore,
            EndingTitle::HighScore => EndingTitle::GameOver,
        }
    }

    fn begin_title(&mut self, assets: &GraphicsAssets, title: EndingTitle) -> Result<(), Error> {
        self.state.title = title;
        self.state.stage = EndingStage::Approach;
        self.state.draw_index = 0;
        self.state.hold_tick = 0;
        self.state.world[0].requested_sprite = 0;
        let record = &mut self.state.world[5];
        record.active = true;
        record.movement = 2;
        record.depth = if title == EndingTitle::GameOver {
            10
        } else {
            1
        };
        self.sort_world();
        let record = &mut self.state.world[5];
        record.depth = 1000;
        record.lateral = 0;
        record.height = self.camera_height;
        if title == EndingTitle::HighScore {
            self.state.best_score = self.state.best_score.max(self.current_score);
        }
        self.render(assets, false)
    }

    fn sort_world(&mut self) {
        let mut index = 0;
        while index + 1 < self.state.full_world_order.len() {
            let a = self.state.world[self.state.full_world_order[index]];
            let b = self.state.world[self.state.full_world_order[index + 1]];
            if a.movement != 0 && (b.movement == 0 || a.depth < b.depth) {
                self.state.full_world_order.swap(index, index + 1);
                index = index.saturating_sub(1);
            } else {
                index += 1;
            }
        }
    }

    fn render(&mut self, assets: &GraphicsAssets, digits: bool) -> Result<(), Error> {
        self.state.world[5].requested_sprite =
            title_source(self.state.world[5].depth, self.state.title);
        for record in &mut self.state.world {
            if record.movement != 1 {
                record.project(assets, self.vertical_origin, self.camera_height)?;
            }
        }
        self.special
            .project(true, self.vertical_origin, self.camera_height)?;
        for record in self.special.snapshots() {
            self.state.world[record.slot] = special_record(record);
        }
        self.render_viewport(assets, digits)
    }

    fn render_overlay(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        self.render_viewport(assets, true)
    }

    fn render_viewport(&mut self, assets: &GraphicsAssets, digits: bool) -> Result<(), Error> {
        let mut viewport = scene::background_at(assets, self.strip_index, self.strip_y)?;
        for slot in self.state.full_world_order {
            let record = self.state.world[slot];
            if record.movement == 1 {
                self.special.draw_slot(&mut viewport, slot, VIEWPORT)?;
            } else if record.active {
                draw(assets, &mut viewport, record.draw())?;
            }
        }
        for command in scene::initial_draws().into_iter().skip(2).take(4) {
            draw(assets, &mut viewport, command)?;
        }
        if digits {
            for command in self.state.score_digits {
                draw(assets, &mut viewport, command)?;
            }
        }
        let mut packed = self.frame.packed().to_vec();
        for y in VIEWPORT.y as usize..(VIEWPORT.y as usize + VIEWPORT.height) {
            let start = y * ROW_BYTES + VIEWPORT.x_bytes as usize;
            let end = start + VIEWPORT.width_bytes;
            packed[start..end].copy_from_slice(&viewport.packed()[start..end]);
        }
        self.frame = Frame::from_packed(&packed)?;
        Ok(())
    }
}

fn special_record(record: super::SpecialState) -> EnemyState {
    EnemyState {
        slot: record.slot,
        depth: record.depth,
        lateral: record.lateral,
        height: record.height,
        active: record.active,
        movement: record.movement,
        variant: 0,
        animation_phase: 0,
        requested_sprite: record.requested_sprite,
        resolved_sprite: record.resolved_sprite,
        rect: record.rect,
        phase: record.phase,
    }
}

fn empty_record(slot: usize, source: usize, depth: i16, height: i16, movement: u8) -> EnemyState {
    EnemyState {
        slot,
        depth,
        lateral: 0,
        height,
        active: false,
        movement,
        variant: 0,
        animation_phase: 0,
        requested_sprite: source,
        resolved_sprite: source,
        rect: Rect {
            x_bytes: 128,
            y: 0,
            width_bytes: 2,
            height: 2,
        },
        phase: 0,
    }
}

fn title_source(depth: i16, title: EndingTitle) -> usize {
    let size = 2047 / (i32::from(depth) + 64);
    match size {
        0..=4 => 160,
        5..=9 => (156 + size) as usize,
        10..=12 => {
            if title == EndingTitle::GameOver {
                166
            } else {
                172
            }
        }
        13..=15 => {
            if title == EndingTitle::GameOver {
                167
            } else {
                173
            }
        }
        _ => {
            if title == EndingTitle::GameOver {
                168
            } else {
                174
            }
        }
    }
}

fn player_projectile_source(depth: i16) -> usize {
    if depth < 100 {
        21
    } else if depth > 160 {
        23
    } else {
        22
    }
}

fn score_digits(score: u32, x: i32, y: i32) -> [Draw; 6] {
    let mut divisor = 100_000;
    std::array::from_fn(|index| {
        let digit = (score / divisor) % 10;
        divisor /= 10;
        Draw {
            resolved_sprite: 70 + digit as usize,
            rect: Rect {
                x_bytes: x + index as i32 * 2,
                y,
                width_bytes: 2,
                height: 7,
            },
        }
    })
}

fn draw(assets: &GraphicsAssets, frame: &mut Frame, command: Draw) -> Result<(), Error> {
    let sprite = assets.sprites.get(command.resolved_sprite).ok_or_else(|| {
        Error::InvalidData(format!("missing ending sprite {}", command.resolved_sprite))
    })?;
    frame.draw_sprite_clipped(sprite, command.rect, VIEWPORT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assets::Sprite, frame::FRAME_BYTES};

    fn assets() -> GraphicsAssets {
        let mut assets = GraphicsAssets {
            primary: vec![0x55; 28_960],
            alternate: vec![0; FRAME_BYTES],
            sprites: (0..190)
                .map(|_| Sprite {
                    width_bytes: 14,
                    height: 37,
                    descriptor: [0; 7],
                    color: vec![0; 14 * 37],
                    mask: vec![255; 14 * 37],
                })
                .collect(),
        };
        for (index, width, height, x_anchor, y_anchor, color) in [
            (0, 2, 2, 0, 0, 0),
            (18, 14, 12, 3, 6, 0),
            (168, 14, 37, 6, 18, 0xe4),
            (174, 14, 37, 6, 16, 0xe4),
        ] {
            assets.sprites[index] = Sprite {
                width_bytes: width,
                height,
                descriptor: [0, 0, 0, 0, 0, x_anchor, y_anchor],
                color: vec![color; width * height],
                mask: vec![if index == 0 || index == 18 { 255 } else { 0 }; width * height],
            };
        }
        for sprite in &mut assets.sprites[70..80] {
            sprite.width_bytes = 2;
            sprite.height = 7;
            sprite.color = vec![0x1b; 14];
            sprite.mask = vec![0; 14];
        }
        assets
    }

    fn snapshot(assets: &GraphicsAssets) -> Snapshot {
        let mut state = super::super::Opening::new(assets).unwrap().snapshot();
        state.player.game_over = true;
        state.player.coordinates = [14, 0, -28];
        state.player.horizon = 18;
        state.vertical_origin = 107;
        state.hud.score = 123_456;
        state.random = [0; 3];
        state.enemies[0].depth = -12;
        for special in &mut state.special {
            special.depth = -8;
            special.coordinates[0] = -8;
        }
        state
    }

    #[test]
    fn mandatory_pair_has_original_draw_counts_and_holds_before_return() {
        let assets = assets();
        let state = snapshot(&assets);
        let frame = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let mut ending = Ending::new(&assets, &state, &frame).unwrap();
        let mut approach = [0; 2];
        let mut departure = [0; 2];
        let mut holds = [0; 2];
        let mut overlays = 0;
        let mut steps = 0;
        while !ending.can_return() {
            let state = ending.snapshot();
            let index = usize::from(state.title == EndingTitle::HighScore);
            match state.stage {
                EndingStage::Approach => approach[index] += 1,
                EndingStage::Departure => departure[index] += 1,
                EndingStage::Hold => {
                    holds[index] += 1;
                    assert_eq!(state.world[5].depth, 20);
                }
                EndingStage::ScoreOverlay => {
                    overlays += 1;
                    assert_eq!(ending.wait_units(), 0);
                }
                EndingStage::Pause => panic!("mandatory titles have no inter-title wait"),
            }
            assert_eq!(ending.returned_intro_choice(), None);
            ending.advance(&assets).unwrap();
            steps += 1;
            assert!(steps < 300);
        }
        assert_eq!(approach, [35, 35]);
        assert_eq!(departure, [34, 34]);
        assert_eq!(holds, [50, 50]);
        assert_eq!(overlays, 1);
        assert_eq!(steps, 239);
        assert_eq!(ending.snapshot().cycle, 1);
        assert_eq!(ending.snapshot().title, EndingTitle::GameOver);
        assert_eq!(ending.snapshot().random, [0; 3]);
        assert_eq!(ending.returned_intro_choice(), Some(true)); // Word5630 chooses DATA6.
        assert_eq!(ending.returned_intro_choice(), Some(true));
        assert_eq!(ending.snapshot().random, [9266, 21041, 22064]);
    }

    #[test]
    fn viewport_refresh_preserves_outer_pixels_and_removes_score_overlay_on_departure() {
        let assets = assets();
        let state = snapshot(&assets);
        let frame = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let mut ending = Ending::new_with_best(&assets, &state, &frame, 123_457).unwrap();
        assert_eq!(ending.snapshot().world[0].rect.x_bytes, 40);
        assert_eq!(ending.snapshot().world[1].rect.x_bytes, 37);
        assert_eq!(ending.snapshot().world[2].resolved_sprite, 23);
        assert_eq!(ending.snapshot().world[3].resolved_sprite, 23);
        let order = ending.snapshot().full_world_order;
        let position = |slot| order.iter().position(|value| *value == slot).unwrap();
        for slot in 23..=27 {
            assert_eq!(ending.snapshot().world[slot].movement, 1);
            assert_eq!(ending.snapshot().world[slot].depth, -8);
            assert!(position(5) < position(slot));
            assert!(position(slot) < position(4));
        }
        while ending.phase() != EndingStage::ScoreOverlay {
            ending.advance(&assets).unwrap();
        }
        let snapshot = ending.snapshot();
        assert_eq!(snapshot.best_score, 123_457);
        assert_eq!(snapshot.world[5].depth, 20);
        assert_eq!(snapshot.world[5].rect.x_bytes, 34);
        assert_eq!(snapshot.world[5].rect.y, 91);
        assert_eq!(
            snapshot.score_digits.map(|digit| digit.resolved_sprite),
            [71, 72, 73, 74, 75, 77]
        );
        let font = snapshot.score_digits[0].rect;
        assert_eq!((font.x_bytes, font.y), (35, 119));
        assert_eq!(ending.frame().packed()[119 * ROW_BYTES + 35], 0x1b);
        while ending.phase() != EndingStage::Departure {
            ending.advance(&assets).unwrap();
        }
        assert_eq!(ending.frame().packed()[119 * ROW_BYTES + 35], 0xe4);
        for y in 0..200 {
            for x in 0..80 {
                if !(44..158).contains(&y) || !(4..76).contains(&x) {
                    assert_eq!(ending.frame().packed()[y * ROW_BYTES + x], 0xaa);
                }
            }
        }
    }

    #[test]
    fn repeating_titles_pause_six_units_and_leave_world_motion_frozen() {
        let assets = assets();
        let mut state = snapshot(&assets);
        state.terrain[0].active = true;
        state.terrain[0].depth = 748;
        state.terrain[0].lateral = 867;
        state.terrain[0].requested_sprite = 107;
        let frame = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let mut ending = Ending::new(&assets, &state, &frame).unwrap();
        while ending.snapshot().cycle == 0 || ending.phase() != EndingStage::Pause {
            ending.advance(&assets).unwrap();
        }
        assert_eq!(ending.wait_units(), 6);
        assert!(!ending.can_return());
        let terrain = ending.snapshot().world[15];
        assert_eq!(
            (terrain.depth, terrain.lateral, terrain.requested_sprite),
            (748, 867, 107)
        );
        ending.advance(&assets).unwrap();
        assert_eq!(ending.snapshot().title, EndingTitle::HighScore);
        assert_eq!(ending.phase(), EndingStage::Approach);
        assert!(ending.can_return());
        assert_eq!(ending.snapshot().world[15].depth, 748);
    }
}
