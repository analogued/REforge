// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Gameplay simulation using interpreted game-state updates.

mod ending;
mod enemy;
pub mod execution;
mod hud;
mod pass;
mod player;
mod presentation;
mod progression;
pub mod raster;
mod session;
mod special;
mod terrain;
#[cfg(test)]
pub(crate) mod test_support;
mod weapon;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
    scene,
    sound::{self, Effect, Foreground, Tone},
};

pub use ending::{Ending, EndingSnapshot, EndingStage, EndingTitle};
pub use enemy::EnemySnapshot as EnemyState;
pub use hud::HudSnapshot as HudState;
pub use player::{PlayerSnapshot as PlayerState, Steering};
pub use presentation::TransitionFrame;
pub use progression::{ScriptCommand, ScriptRules};
pub use session::{
    Controls, DisplayMode, F3Mode, GameStart, HeldKeys, InputPoll, KeyboardState, PauseControl,
    PauseEvent, PauseState, Preferences, RetainedWorld, SessionState,
};
pub use special::SpecialSnapshot as SpecialState;
pub use terrain::TerrainSnapshot as TerrainState;
pub use weapon::WeaponSnapshot as WeaponState;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub body: u16,
    /// Last displayed pass: zero initially, then one through three per body.
    pub render_pass: u8,
    pub strip_index: usize,
    pub strip_y: usize,
    pub vertical_origin: i32,
    pub random: [u16; 3],
    pub script_index: u16,
    pub difficulty: u16,
    pub progression: u16,
    pub speeds: [i16; 3],
    pub speed_mode: u16,
    /// Background selector retained by the most recent world compositor call.
    pub background_layer: u16,
    pub alternate_background: bool,
    pub palette_family: u8,
    pub script_cursor: u16,
    pub script_delay: u16,
    pub terrain_selector: u16,
    pub primary_count: u16,
    pub hud: HudState,
    pub player: PlayerState,
    pub weapon: WeaponState,
    pub terrain: [TerrainState; 8],
    pub enemies: [EnemyState; 11],
    pub enemy_target: [i16; 2],
    pub special: [SpecialState; 5],
    pub world_order: [usize; 19],
    pub full_world_order: [usize; 29],
    pub static_lateral: i16,
}

/// Gameplay, progression and scene state, with optional bounded diagnostic stepping.
#[derive(Debug)]
pub struct Gameplay {
    body: u16,
    render_pass: u8,
    body_limit: Option<u16>,
    static_lateral: i16,
    random: Random,
    rules: ScriptRules,
    script_index: u16,
    difficulty: u16,
    progression: u16,
    speeds: [i16; 3],
    speed_mode: u16,
    strip_family: usize,
    strip_cycle: usize,
    alternate_background: bool,
    overlay: u16,
    layer: u16,
    palette_family: u8,
    noise_divisor: i16,
    hud: hud::Hud,
    terrain: terrain::Terrain,
    enemy: enemy::Enemy,
    player: player::Player,
    weapon: weapon::Weapon,
    special: special::Special,
    strip_y: usize,
    vertical_origin: i32,
    script_cursor: u16,
    script_delay: u16,
    terrain_selector: u16,
    primary_count: u16,
    world_order: Vec<usize>,
    full_world_order: Vec<usize>,
    frame: Frame,
    pass_frame: Frame,
    pass_snapshot: Option<Box<Snapshot>>,
    capture_pass_snapshots: bool,
    updating_before_render: bool,
    selected_layer: u16,
    previous_pass_one_layer: u16,
    pass_one_wait: bool,
    last_background_layer: u16,
    remembered_draws: [Option<scene::Draw>; 29],
    remembered_requests: [usize; 29],
    remembered_hud_rects: Vec<Rect>,
    restore_regions: Vec<Rect>,
    transitions: Vec<TransitionFrame>,
    sound_returned: bool,
    initial_input_applied: bool,
}

impl Gameplay {
    fn initial(
        assets: &GraphicsAssets,
        rules: ScriptRules,
        random: [u16; 3],
        script_index: u16,
        body_limit: Option<u16>,
    ) -> Result<Self, Error> {
        let initial_frame = scene::initial_frame(assets)?;
        let random = Random { words: random };
        let mut terrain = terrain::Terrain::new();
        terrain.set_data_segment(rules.data_segment());
        let hud = hud::Hud::new(assets)?;
        let remembered_hud_rects = hud.draws().iter().map(|draw| draw.rect).collect();
        let mut gameplay = Self {
            body: 0,
            render_pass: 0,
            body_limit,
            static_lateral: 0,
            random,
            rules,
            script_index,
            difficulty: 0,
            progression: 1,
            speeds: [28, 22, 16],
            speed_mode: 0,
            strip_family: 0,
            strip_cycle: 2,
            alternate_background: false,
            overlay: 0,
            layer: 0,
            palette_family: 1,
            noise_divisor: 0,
            hud,
            terrain,
            enemy: enemy::Enemy::new(assets)?,
            player: player::Player::new(assets)?,
            weapon: weapon::Weapon::new(assets)?,
            special: special::Special::new(),
            strip_y: 92,
            vertical_origin: 106,
            script_cursor: 0,
            script_delay: 0,
            terrain_selector: 0,
            primary_count: 1,
            world_order: (5..=9)
                .chain(15..=22)
                .chain(std::iter::once(4))
                .chain(23..=27)
                .collect(),
            full_world_order: (0..29).collect(),
            frame: initial_frame.clone(),
            pass_frame: initial_frame,
            pass_snapshot: None,
            capture_pass_snapshots: false,
            updating_before_render: false,
            selected_layer: 0,
            previous_pass_one_layer: 0,
            pass_one_wait: false,
            last_background_layer: 0,
            remembered_draws: [None; 29],
            remembered_requests: [0; 29],
            remembered_hud_rects,
            restore_regions: Vec::new(),
            transitions: Vec::new(),
            sound_returned: false,
            initial_input_applied: false,
        };
        let depths = gameplay.world_depths();
        gameplay.sort_full_world(&depths);
        Ok(gameplay)
    }

    #[cfg(test)]
    pub(crate) fn test_game(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut random = Random::default();
        random.next_word();
        random.next_word();
        let mut game = Self::initial(
            assets,
            test_support::diagnostic_rules(),
            random.state(),
            0,
            Some(DEFAULT_EXPORT_BODY_LIMIT),
        )?;
        game.capture_pass_snapshots(true);
        Ok(game)
    }

    #[cfg(test)]
    pub(crate) fn test_controlled_game(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut game = Self::test_game(assets)?;
        game.body_limit = Some(DEFAULT_CONTROLLED_EXPORT_BODY_LIMIT);
        Ok(game)
    }

    /// Construct a bounded game using external encounter rules and retained session state.
    pub fn from_game_start(
        assets: &GraphicsAssets,
        rules: ScriptRules,
        start: GameStart,
        limit: u16,
    ) -> Result<Self, Error> {
        let mut gameplay =
            Self::initial(assets, rules, start.random, start.script_index, Some(limit))?;
        gameplay.capture_pass_snapshots(true);
        gameplay.enemy.reset_for_game(assets)?;
        if let Some(world) = start.retained_world {
            gameplay.enemy = enemy::Enemy::restore_snapshots(
                std::array::from_fn(|i| world.records[i + 4]),
                assets,
            )?;
            gameplay
                .terrain
                .restore_snapshots(std::array::from_fn(|i| {
                    let row = world.records[i + 15];
                    TerrainState {
                        slot: row.slot,
                        depth: row.depth,
                        lateral: row.lateral,
                        active: row.active,
                        movement: row.movement,
                        kind: row.variant,
                        animation_phase: row.animation_phase,
                        requested_sprite: row.requested_sprite,
                        resolved_sprite: row.resolved_sprite,
                        rect: row.rect,
                        phase: row.phase,
                    }
                }))?;
            gameplay.special.restore_snapshots(world.special);
            gameplay
                .weapon
                .restore([world.records[2], world.records[3]]);
            gameplay.static_lateral = world.records[28].lateral;
        }
        // Initialization resolves and projects the reset records before its first
        // displayed frame. Retained unused coordinates and animation fields survive.
        gameplay.weapon.project(assets, true, 106, 0)?;
        gameplay.enemy.project(assets, true, 106, 0)?;
        gameplay.terrain.project(assets, true, 106, 0)?;
        gameplay.special.project(true, 106, 0)?;
        gameplay.full_world_order = (0..29).collect();
        let depths = gameplay.world_depths();
        gameplay.sort_full_world(&depths);
        gameplay.sort_world(&depths);
        Ok(gameplay)
    }

    /// Start live gameplay without an export limit. Game over still completes
    /// the current body's three passes before returning to the ending sequence.
    pub fn for_play(
        assets: &GraphicsAssets,
        rules: ScriptRules,
        start: GameStart,
    ) -> Result<Self, Error> {
        let mut gameplay = Self::from_game_start(assets, rules, start, 0)?;
        gameplay.body_limit = None;
        gameplay.capture_pass_snapshots(false);
        Ok(gameplay)
    }

    pub fn request_game_over(&mut self) {
        self.player.end_game();
    }

    /// Apply the original 03d7 player poll/update before initial composition
    /// and pre-game playback. The caller supplies a reconstructed Controls poll.
    /// This does not advance a body/pass, enemies, weapons, fuel or random state.
    pub fn initialize_input(&mut self, assets: &GraphicsAssets, mask: u8) -> Result<(), Error> {
        if self.initial_input_applied || self.body != 0 || self.render_pass != 0 {
            return Err(Error::InvalidData(
                "initial input must precede gameplay and run once".into(),
            ));
        }
        if mask & !0x9f != 0 {
            return Err(Error::InvalidData("invalid initializer input mask".into()));
        }
        self.player.initialize_input(mask)?;
        self.finish_player(false);
        let player = self.player.snapshot();
        self.strip_y = (92 + (player.horizon >> 3)) as usize;
        self.vertical_origin = 106 + i32::from(player.horizon >> 4);
        self.weapon
            .project(assets, true, self.vertical_origin, player.horizon)?;
        self.enemy
            .project(assets, true, self.vertical_origin, player.horizon)?;
        self.terrain
            .project(assets, true, self.vertical_origin, player.horizon)?;
        self.special
            .project(true, self.vertical_origin, player.horizon)?;
        self.player.project(assets, self.vertical_origin)?;
        let depths = self.world_depths();
        self.sort_full_world(&depths);
        self.sort_world(&depths);
        self.hud.set_orientation(self.player.snapshot().orientation);

        // Initial HUD writes resolve the preceding indicator artwork (85/86),
        // even when the player update selects a different orientation source.
        let mut draws = scene::initial_draws();
        draws[..2].copy_from_slice(&self.player.draws());
        self.frame = scene::background_at(assets, self.snapshot().strip_index, self.strip_y)?;
        for draw in draws {
            let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!(
                    "missing initializer sprite {}",
                    draw.resolved_sprite
                ))
            })?;
            self.frame.draw_sprite(sprite, draw.rect)?;
        }
        self.pass_frame = self.frame.clone();
        self.initial_input_applied = true;
        Ok(())
    }

    pub fn palette_family(&self) -> u8 {
        self.palette_family
    }

    /// Both CGA BIOS mode sets used by F3 select palette family 1. Keep this
    /// effective register state until a later CD/CE command selects a palette.
    /// The saved framebuffer bytes and gameplay state are unaffected.
    pub fn reset_video_palette(&mut self) {
        self.palette_family = 1;
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The most recent ordinary render pass, before any end-of-body transitions.
    pub fn pass_frame(&self) -> &Frame {
        &self.pass_frame
    }

    /// Enable diagnostic state capture at future ordinary framebuffer checkpoints.
    /// Live playback leaves this disabled; disabling it clears the saved state.
    pub fn capture_pass_snapshots(&mut self, enabled: bool) {
        self.capture_pass_snapshots = enabled;
        if !enabled {
            self.pass_snapshot = None;
        }
    }

    /// State at the last captured framebuffer checkpoint, before end-of-body events.
    pub fn pass_snapshot(&self) -> Option<&Snapshot> {
        self.pass_snapshot.as_deref()
    }

    pub fn complete(&self) -> bool {
        self.at_body_boundary()
            && (self.body_limit.is_some_and(|limit| self.body >= limit)
                || self.player.snapshot().game_over)
    }

    /// True initially and after all three render passes and end-of-body events.
    pub fn at_body_boundary(&self) -> bool {
        self.render_pass == 0 || self.render_pass == 3
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            body: self.body,
            render_pass: self.render_pass,
            static_lateral: self.static_lateral,
            strip_index: self.strip_family * 3 + self.strip_cycle,
            strip_y: self.strip_y,
            vertical_origin: self.vertical_origin,
            random: self.random.state(),
            script_index: self.script_index,
            difficulty: self.difficulty,
            progression: self.progression,
            speeds: self.speeds,
            speed_mode: self.speed_mode,
            background_layer: self.last_background_layer,
            alternate_background: self.alternate_background,
            palette_family: self.palette_family,
            script_cursor: self.script_cursor,
            script_delay: self.script_delay,
            terrain_selector: self.terrain_selector,
            primary_count: self.primary_count,
            hud: self.hud.snapshot(),
            player: self.player.snapshot(),
            weapon: self.weapon.snapshot(),
            terrain: self.terrain.snapshots(),
            enemies: self.enemy.snapshots(),
            enemy_target: self.enemy.target(),
            special: self.special.snapshots(),
            world_order: std::array::from_fn(|i| self.world_order[i]),
            full_world_order: std::array::from_fn(|i| self.full_world_order[i]),
        }
    }

    /// Advance one completed loop body. Return false at the supported boundary.
    pub fn advance(&mut self, assets: &GraphicsAssets) -> Result<bool, Error> {
        self.advance_with_input(assets, Steering::default())
    }

    /// Sample the same held directions at all three player updates in this body.
    pub fn advance_with_input(
        &mut self,
        assets: &GraphicsAssets,
        input: Steering,
    ) -> Result<bool, Error> {
        self.advance_with_inputs(assets, [input; 3])
    }

    /// Replay held-state samples at each of the three player polls.
    pub fn advance_with_inputs(
        &mut self,
        assets: &GraphicsAssets,
        inputs: [Steering; 3],
    ) -> Result<bool, Error> {
        if !self.at_body_boundary() {
            return Err(Error::InvalidData(
                "complete the pending render passes before advancing a whole body".into(),
            ));
        }
        if self.complete() {
            return Ok(false);
        }
        for input in inputs {
            self.advance_subframe(assets, input)?;
        }
        Ok(true)
    }

    /// Advance to the next of the original's three displayed passes.
    /// Each pass samples steering once; world motion runs on passes one and three.
    pub fn advance_subframe(
        &mut self,
        assets: &GraphicsAssets,
        input: Steering,
    ) -> Result<bool, Error> {
        self.advance_subframe_with_sound(assets, input, &mut |_| {})
    }

    /// Select foreground sound and wait operations in original call order.
    /// The sink may execute them against timer state; it must not re-enter this
    /// game. Instruction delays and frontend input during waits belong to the
    /// executor. The default stepping API uses a silent sink.
    pub fn advance_subframe_with_sound(
        &mut self,
        assets: &GraphicsAssets,
        input: Steering,
        sound: &mut Foreground<'_>,
    ) -> Result<bool, Error> {
        if self.complete() {
            self.return_sound(sound);
            return Ok(false);
        }
        let mut task = pass::PassTask::begin(self);
        loop {
            match task.resume(self, assets, input, sound)? {
                pass::PassProgress::Transition => {}
                pass::PassProgress::Action(action) => sound(action),
                pass::PassProgress::Publish { .. } => task.publish(self),
                pass::PassProgress::Returned => return Ok(true),
            }
        }
    }

    fn begin_finish_pass(&mut self, assets: &GraphicsAssets) -> Result<bool, Error> {
        self.hud
            .set_score_multiplier(if self.selected_layer == 1 { 4 } else { 1 });
        if self.render_pass != 3 {
            return Ok(false);
        }
        self.player.finish_body();

        // Event creation and recycling happen after the last framebuffer write.
        if self.overlay != 0 {
            self.overlay = self.overlay.wrapping_add(1);
            if self.overlay > 80 {
                self.overlay = 0;
                self.queue_transition(assets)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn finish_pass(&mut self, sound: &mut Foreground<'_>) -> Result<(), Error> {
        let lateral = self.player.snapshot().coordinates[1];
        self.terrain.recycle_at_speed(
            self.terrain_selector,
            &mut self.random,
            lateral,
            self.speeds[0],
        )?;
        self.special.recycle_continuous(
            &mut self.random,
            self.speeds[0],
            self.overlay.wrapping_add(self.layer) as i16 > 0,
        );
        self.return_sound(sound);
        Ok(())
    }

    fn return_sound(&mut self, sound: &mut Foreground<'_>) {
        if self.at_body_boundary() && self.player.snapshot().game_over && !self.sound_returned {
            sound::request(sound, 0x0452, Effect::Reset);
            self.sound_returned = true;
        }
    }

    fn presentation(&self) -> presentation::Presentation {
        presentation::Presentation {
            alternate_background: self.alternate_background,
            layer: self.layer,
            overlay_counter: self.overlay,
        }
    }

    pub(crate) fn queue_transition(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        self.last_background_layer = self.layer;
        self.hud
            .set_score_multiplier(if self.layer == 1 { 4 } else { 1 });
        let mut frames = self.presentation().transition_frames(
            assets,
            &self.frame,
            self.snapshot().strip_index,
            self.strip_y,
            |frame, clip| self.draw_world(assets, frame, clip),
        )?;
        let state = self.snapshot();
        for frame in &mut frames {
            frame.before_pass = self.updating_before_render;
            frame.palette_family = self.palette_family;
            frame.state = Some(Box::new(state.clone()));
        }
        if let Some(last) = frames.last() {
            self.frame = last.frame.clone();
        }
        self.transitions.extend(frames);
        self.remembered_draws = self.world_draws();
        self.remembered_requests = self.world_requests();
        Ok(())
    }

    /// The extra one-unit wait before the first pass's CGA copy is skipped
    /// when its selected background layer changes from the preceding body.
    pub fn pass_one_wait_required(&self) -> bool {
        self.render_pass == 1 && self.pass_one_wait
    }

    pub fn take_transitions(&mut self) -> Vec<TransitionFrame> {
        std::mem::take(&mut self.transitions)
    }

    fn render(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        let state = self.presentation();
        let strip = self.snapshot().strip_index;
        let viewport = presentation::VIEWPORT;
        if self.render_pass == 1 {
            self.selected_layer = state.world_layer();
            self.pass_one_wait = self.selected_layer == self.previous_pass_one_layer;
            self.previous_pass_one_layer = self.selected_layer;
        }
        self.last_background_layer = self.selected_layer;
        let background = presentation::Presentation {
            layer: self.selected_layer,
            overlay_counter: 0,
            ..state
        }
        .background(assets, strip, self.strip_y, self.render_pass)?;
        let mut frame = self.frame.clone();
        let hud_draws = if self.render_pass == 1 {
            self.hud.draws()
        } else {
            Vec::new()
        };
        if self.render_pass == 1 {
            // Restore HUD placements, including their preceding rectangles, rather
            // than repainting static borders when the world background changes.
            let hud_background = state.background_with_layer(assets, strip, self.strip_y, 0)?;
            let screen = Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: 80,
                height: 200,
            };
            for rect in self
                .remembered_hud_rects
                .iter()
                .copied()
                .chain(hud_draws.iter().map(|draw| draw.rect))
            {
                // Indicator masks deliberately update the preceding artwork.
                if rect.y == 13 && matches!(rect.x_bytes, 28 | 47) {
                    continue;
                }
                if let Some(clip) = presentation::dirty_region(rect, screen) {
                    presentation::restore_background(&mut frame, &hud_background, clip)?;
                }
            }
            self.remembered_hud_rects = hud_draws.iter().map(|draw| draw.rect).collect();
        }
        let current = self.world_draws();
        let requests = self.world_requests();
        self.restore_regions.clear();
        if self.render_pass == 1 {
            self.restore_regions.push(viewport);
            presentation::restore_background(&mut frame, &background, viewport)?;
            self.draw_world(assets, &mut frame, viewport)?;
            self.remembered_draws = current;
            self.remembered_requests = requests;
        } else {
            let mut near = [false; 29];
            near[0] = true;
            near[1] = true;
            if self.render_pass == 3 {
                near[2] = self.weapon.near(0);
                near[3] = self.weapon.near(1);
                for record in self.enemy.snapshots() {
                    near[record.slot] = record.depth < 70 && record.movement != 1;
                }
                for record in self.terrain.snapshots() {
                    near[record.slot] = record.depth < 70 && record.movement != 1;
                }
            }
            let mut regions = Vec::new();
            for (slot, &included) in near.iter().enumerate() {
                if !included {
                    continue;
                }
                let unchanged = match (current[slot], self.remembered_draws[slot]) {
                    (None, None) => true,
                    (Some(a), Some(b)) => {
                        a.rect.x_bytes == b.rect.x_bytes
                            && a.rect.y == b.rect.y
                            && requests[slot] == self.remembered_requests[slot]
                    }
                    _ => false,
                };
                if !unchanged {
                    regions.extend(presentation::dirty_regions(
                        current[slot].map(|draw| draw.rect),
                        self.remembered_draws[slot].map(|draw| draw.rect),
                        viewport,
                    ));
                }
                self.remembered_draws[slot] = current[slot];
                self.remembered_requests[slot] = requests[slot];
            }
            self.restore_regions.clone_from(&regions);
            for &clip in &regions {
                presentation::restore_background(&mut frame, &background, clip)?;
            }
            for clip in regions {
                self.draw_world(assets, &mut frame, clip)?;
            }
        }
        if self.render_pass == 1 {
            for draw in hud_draws {
                let sprite = assets.sprites.get(draw.resolved_sprite).ok_or_else(|| {
                    Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite))
                })?;
                frame.draw_sprite(sprite, draw.rect)?;
            }
        }
        self.frame = frame;
        Ok(())
    }

    fn world_requests(&self) -> [usize; 29] {
        let mut requests = [0; 29];
        let player = self.player.snapshot();
        requests[0] = player.requested_sprite;
        requests[1] = player.shadow_requested_sprite;
        for shot in self.weapon.snapshot().shots {
            requests[shot.slot] = shot.requested_sprite;
        }
        for record in self.enemy.snapshots() {
            requests[record.slot] = record.requested_sprite;
        }
        for record in self.terrain.snapshots() {
            requests[record.slot] = record.requested_sprite;
        }
        requests
    }

    fn world_draws(&self) -> [Option<scene::Draw>; 29] {
        let mut draws = [None; 29];
        let [shadow, craft] = self.player.draws();
        draws[0] = Some(craft);
        draws[1] = Some(shadow);
        for shot in self.weapon.snapshot().shots {
            if shot.active {
                draws[shot.slot] = Some(shot.draw());
            }
        }
        for state in self.enemy.snapshots() {
            if state.active {
                draws[state.slot] = Some(scene::Draw {
                    resolved_sprite: state.resolved_sprite,
                    rect: state.rect,
                });
            }
        }
        for state in self.terrain.snapshots() {
            if state.active {
                draws[state.slot] = Some(scene::Draw {
                    resolved_sprite: state.resolved_sprite,
                    rect: state.rect,
                });
            }
        }
        draws
    }

    fn draw_world(
        &self,
        assets: &GraphicsAssets,
        frame: &mut Frame,
        viewport: Rect,
    ) -> Result<(), Error> {
        let draws = self.world_draws();
        for &slot in &self.full_world_order {
            if (23..=27).contains(&slot) {
                self.special.draw_slot(frame, slot, viewport)?;
            } else if let Some(draw) = draws[slot] {
                draw_clipped(assets, frame, draw, viewport)?;
            }
        }
        for draw in scene::initial_draws().into_iter().skip(2).take(4) {
            draw_clipped(assets, frame, draw, viewport)?;
        }
        Ok(())
    }

    fn world_depths(&self) -> Vec<(usize, i16, u8)> {
        self.enemy
            .snapshots()
            .into_iter()
            .map(|slot| (slot.slot, slot.depth, slot.movement))
            .chain(
                self.terrain
                    .snapshots()
                    .into_iter()
                    .map(|slot| (slot.slot, slot.depth, slot.movement)),
            )
            .chain(
                self.special
                    .snapshots()
                    .into_iter()
                    .map(|slot| (slot.slot, slot.depth, slot.movement)),
            )
            .chain([
                (0, self.player.snapshot().coordinates[0], 255),
                (1, self.player.snapshot().coordinates[0] + 1, 0),
                (2, self.weapon.snapshot().shots[0].depth, 254),
                (3, self.weapon.snapshot().shots[1].depth, 254),
                (28, 0, 0),
            ])
            .collect()
    }

    fn sort_full_world(&mut self, depths: &[(usize, i16, u8)]) {
        self.full_world_order.sort_by_key(|slot| {
            let (_, depth, movement) = depths.iter().find(|(index, _, _)| index == slot).unwrap();
            (
                *movement != 0,
                std::cmp::Reverse(if *movement == 0 { 0 } else { *depth }),
            )
        });
    }

    fn sort_world(&mut self, depths: &[(usize, i16, u8)]) {
        self.world_order.sort_by_key(|slot| {
            let (_, depth, movement) = depths.iter().find(|(index, _, _)| index == slot).unwrap();
            (
                *movement != 0,
                std::cmp::Reverse(if *movement == 0 { 0 } else { *depth }),
            )
        });
    }

    fn begin_finish_world(
        &mut self,
        assets: &GraphicsAssets,
        effects: enemy::EnemyEffects,
        sound: &mut Foreground<'_>,
    ) -> Result<pass::WorldCompletion, Error> {
        let player = self.player.snapshot();
        self.apply_enemy_effects(effects);
        let collected_fuel = self.terrain.update_half_step_at_speed_with_sound(
            player.coordinates,
            self.speeds[0],
            sound,
        );
        self.hud.add_fuel(collected_fuel);
        self.special
            .update_half_step(player.coordinates[0], self.speeds[0]);
        // All motion precedes collision, which sees the previous resolved sources.
        let collisions = self.enemy.player_collisions(
            assets,
            player.coordinates,
            player.resolved_sprite,
            player.phase == 0,
        )?;
        for collision in collisions.into_iter().flatten() {
            if self.player.snapshot().phase != 0 {
                break;
            }
            match collision {
                enemy::EnemyCollision::Projectile => self.player.hit_with_sound(1, sound),
                enemy::EnemyCollision::Craft(slot) => {
                    if self.enemy.snapshots()[slot - 4].variant != 2 {
                        self.hud.add_points(1000);
                    }
                    self.enemy.explode_craft_with_sound(slot, sound)?;
                    self.player.hit_with_sound(2, sound);
                }
            }
        }
        if let Some(collision) = self.terrain.player_collision_at_speed(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.snapshot().phase == 0,
            self.player.can_damage(),
            self.speeds[0],
        )? {
            for contact in collision.contacts.into_iter().flatten() {
                self.hud.add_fuel(contact.fuel);
                self.hud.add_points(contact.points);
                self.player.hit_with_sound(contact.damage, sound);
                if contact.converted {
                    sound::request(sound, 0x397c, Effect::Ramp(sound::Ramp::Third));
                }
            }
        }
        let collision = self.special.player_collision(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.snapshot().phase == 0,
            self.player.can_damage(),
        )?;
        Ok(pass::WorldCompletion::new(
            collision.map_or([None; 5], |collision| collision.contacts),
        ))
    }

    /// Resume at the exact source point after each synchronous transition call.
    fn resume_finish_world(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        task: &mut pass::WorldCompletion,
        sound: &mut Foreground<'_>,
    ) -> Result<bool, Error> {
        if task.follow_goal {
            self.follow_edge(true)?;
            task.follow_goal = false;
        }
        while let Some(contact) = task.contacts.get(task.next_contact).copied() {
            task.next_contact += 1;
            let Some(contact) = contact else { continue };
            match contact {
                special::SpecialContact::Award {
                    site,
                    points,
                    transition,
                } => {
                    sound::request(sound, site, Effect::Tone(Tone::First));
                    self.hud.add_points(points);
                    if let Some(transition) = transition {
                        match transition {
                            special::SpecialTransition::Normal => self.change_stage(false),
                            special::SpecialTransition::Alternate => self.change_stage(true),
                        }
                        self.queue_transition(assets)?;
                        task.follow_goal = true;
                        return Ok(true);
                    }
                }
                special::SpecialContact::Damage(damage) => {
                    self.player.hit_with_sound(damage, sound)
                }
            }
        }
        self.collide_weapon_with_sound(assets, sound)?;
        let player = self.player.snapshot();
        self.weapon
            .advance_with_sound(player.effective_input & 16 != 0, player.coordinates, sound);
        if full_projection {
            self.strip_y = (92 + (player.horizon >> 3)) as usize;
            self.vertical_origin = 106 + i32::from(player.horizon >> 4);
        }
        self.weapon.project(
            assets,
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.enemy.project(
            assets,
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.special.project(
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        self.terrain.project(
            assets,
            full_projection,
            self.vertical_origin,
            self.player.snapshot().horizon,
        )?;
        Ok(false)
    }

    #[cfg(test)]
    fn collide_weapon(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        self.collide_weapon_with_sound(assets, &mut |_| {})
    }

    fn collide_weapon_with_sound(
        &mut self,
        assets: &GraphicsAssets,
        sound: &mut Foreground<'_>,
    ) -> Result<(), Error> {
        for index in 0..2 {
            if !self.weapon.snapshot().shots[index].active {
                continue;
            }
            // Activity is tested once for the source. Consuming a shot does not
            // stop the original scan of overlapping target records.
            for slot in 4..=9 {
                let target = self.enemy.snapshots()[slot - 4];
                if !target.active || !self.weapon.overlaps(assets, index, target)? {
                    continue;
                }
                if slot == 4 {
                    self.weapon.deactivate(index);
                    self.enemy.deactivate_projectile();
                } else if (4..251).contains(&target.movement)
                    && (target.variant < 3 || matches!(target.variant, 5 | 6))
                {
                    self.weapon.deactivate(index);
                    self.hud
                        .add_points(if target.variant == 6 { 2600 } else { 1500 });
                    self.enemy.hit_by_weapon_with_sound(slot, sound)?;
                }
            }
            for target in self.terrain.snapshots() {
                if !target.active || target.movement != 2 || target.kind >= 2 {
                    continue;
                }
                let enemy = EnemyState {
                    slot: target.slot,
                    depth: target.depth,
                    lateral: target.lateral,
                    height: -100,
                    active: target.active,
                    movement: target.movement,
                    variant: target.kind,
                    animation_phase: target.animation_phase,
                    requested_sprite: target.requested_sprite,
                    resolved_sprite: target.resolved_sprite,
                    rect: target.rect,
                    phase: target.phase,
                };
                if self.weapon.overlaps(assets, index, enemy)? {
                    let hud = &mut self.hud;
                    if self.terrain.shot_collision_with_sound(
                        target.slot,
                        self.speeds[0],
                        &mut |action| {
                            hud.add_points(400);
                            sound(action);
                        },
                    )? != 0
                    {
                        self.hud.add_points(500);
                    }
                }
            }
        }
        Ok(())
    }

    fn enemy_context(&self) -> Result<enemy::EnemyContext, Error> {
        Ok(enemy::EnemyContext {
            speeds: self.speeds,
            difficulty: self.difficulty,
            alternate_shadows: self.overlay.wrapping_add(self.layer) as i16 > 0,
            horizon: self.player.snapshot().horizon,
            patrol_sources: Some(
                self.rules
                    .patrol_sources(progression::visual_difficulty(self.difficulty))?,
            ),
        })
    }

    fn apply_enemy_effects(&mut self, effects: enemy::EnemyEffects) {
        self.hud.add_points(effects.points);
        self.player.add_lives(effects.lives);
        self.hud.set_lives(self.player.snapshot().lives);
        self.difficulty = self.difficulty.wrapping_add(u16::from(effects.difficulty));
        if let Some(distance) = effects.distance_hint {
            self.noise_divisor = self.noise_divisor.max(distance);
        }
    }

    fn advance_strip(&mut self) {
        self.strip_cycle = if self.strip_cycle == 0 {
            2
        } else {
            self.strip_cycle - 1
        };
        let horizon = self.player.snapshot().horizon;
        self.strip_y = (92 + (horizon >> 3)) as usize;
        self.vertical_origin = 106 + i32::from(horizon >> 4);
    }

    fn advance_speed(&mut self) {
        self.noise_divisor = self
            .noise_divisor
            .wrapping_sub(self.speeds[0].wrapping_sub(28).wrapping_shl(3));
        match self.speed_mode {
            1 if self.speeds[0] < 56 => self
                .speeds
                .iter_mut()
                .for_each(|speed| *speed = speed.wrapping_add(1)),
            2 if self.speeds[0] > 28 => self
                .speeds
                .iter_mut()
                .for_each(|speed| *speed = speed.wrapping_sub(2)),
            2 => {
                self.speeds = [28, 22, 16];
                self.speed_mode = 0;
            }
            _ => {}
        }
    }

    fn change_stage(&mut self, alternate: bool) {
        self.strip_family = usize::from(alternate);
        if !alternate {
            self.alternate_background = false;
        }
        self.advance_strip();
        self.layer = if alternate { 2 } else { 0 };
    }

    fn follow_edge(&mut self, goal: bool) -> Result<(), Error> {
        self.script_index = self.rules.progression(self.script_index, goal)?;
        self.progression = self.progression.wrapping_add(1);
        self.script_cursor = 0;
        self.enemy.clear_primary();
        self.special.clear_active();
        Ok(())
    }

    #[cfg(test)]
    fn parse_commands(
        &mut self,
        assets: &GraphicsAssets,
        sound: &mut Foreground<'_>,
    ) -> Result<(), Error> {
        let mut task = pass::CommandTask::default();
        while self.resume_commands(assets, &mut task, sound)? {}
        Ok(())
    }

    fn resume_commands(
        &mut self,
        assets: &GraphicsAssets,
        task: &mut pass::CommandTask,
        sound: &mut Foreground<'_>,
    ) -> Result<bool, Error> {
        // Retain the cycle guard across transitions in a single parser invocation.
        while !self.enemy.active() {
            let old = self.script_delay as i16;
            self.script_delay = self.script_delay.wrapping_add(1);
            if old < progression::delay_threshold(self.difficulty) {
                break;
            }
            if !task
                .visited
                .insert((self.script_index, self.script_cursor, self.difficulty))
            {
                return Err(Error::InvalidData(
                    "encounter command cycle stalls without an active wave".into(),
                ));
            }
            self.script_delay = 0;
            let command = self.rules.next(self.script_index, self.script_cursor)?;
            self.script_cursor = self.script_cursor.wrapping_add(1);
            match command {
                ScriptCommand::Terrain(selector) => self.terrain_selector = selector,
                ScriptCommand::Special(command) => {
                    let spawned = self.special.spawn_command(
                        command,
                        &mut self.random,
                        self.speeds[0],
                        self.overlay.wrapping_add(self.layer) as i16 > 0,
                    )?;
                    if let Some(selector) = spawned.terrain_selector {
                        self.terrain_selector = selector;
                    }
                    if spawned.goal_barrier {
                        self.terrain
                            .spawn_goal_barrier(self.speeds[0], self.difficulty);
                    }
                }
                ScriptCommand::Enemy(command) => {
                    let context = self.enemy_context()?;
                    let effects = self.enemy.spawn_with_sound(
                        command,
                        &mut self.random,
                        self.player.snapshot().coordinates,
                        usize::from(self.primary_count),
                        context,
                        sound,
                    )?;
                    self.apply_enemy_effects(effects);
                }
                ScriptCommand::Control(command) => match command {
                    0xc0..=0xc2 | 0xc8 | 0xc9 => {}
                    0xc4 => sound::request(sound, 0x1cf1, Effect::Tone(Tone::First)),
                    0xc3 => {
                        self.random.next_word();
                        sound::request(sound, 0x1ce2, Effect::Tone(Tone::First));
                        self.hud.add_points(3000);
                    }
                    0xc5 => self.speed_mode = 1,
                    0xc6 => self.speed_mode = 2,
                    0xc7 => {
                        self.overlay = 0;
                        self.queue_transition(assets)?;
                        return Ok(true);
                    }
                    0xca => {
                        self.script_cursor = 0;
                        self.script_index = self.random.next_word() & 3;
                    }
                    0xcb => {
                        self.change_stage(true);
                        self.queue_transition(assets)?;
                        return Ok(true);
                    }
                    0xcc => {
                        self.change_stage(false);
                        self.queue_transition(assets)?;
                        return Ok(true);
                    }
                    0xcd => self.palette_family = 1,
                    0xce => self.palette_family = 0,
                    0xcf => self.follow_edge(false)?,
                    0xd0 | 0xd1 => {
                        self.strip_family = usize::from(command - 0xd0);
                        self.advance_strip();
                    }
                    0xd2 | 0xd3 => self.strip_family = usize::from(command - 0xd0),
                    0xd4 => {
                        self.random.next_word();
                        self.primary_count = self.primary_count.wrapping_add(1);
                    }
                    0xd5 | 0xd6 => {
                        self.alternate_background = command == 0xd6;
                        self.strip_family = if self.alternate_background { 3 } else { 0 };
                        self.overlay = 0;
                        self.layer = 0;
                        self.advance_strip();
                        self.queue_transition(assets)?;
                        return Ok(true);
                    }
                    _ => {
                        return Err(Error::InvalidData(format!(
                            "unknown control command {command:#x}"
                        )));
                    }
                },
            }
        }
        Ok(false)
    }

    fn finish_player(&mut self, reset_fuel: bool) {
        self.noise_divisor = self.player.noise_divisor();
        if reset_fuel {
            self.hud.reset_fuel();
            self.hud.set_lives(self.player.snapshot().lives);
        }
        let delta = self
            .player
            .snapshot()
            .update_lateral_velocity
            .wrapping_mul(-8);
        self.weapon.shift_lateral(delta);
        self.enemy.shift_lateral(delta);
        self.terrain.shift_lateral(delta);
        self.special.shift_lateral(delta);
        self.static_lateral = self.static_lateral.wrapping_add(delta);
    }
}

fn draw_clipped(
    assets: &GraphicsAssets,
    frame: &mut Frame,
    draw: scene::Draw,
    viewport: Rect,
) -> Result<(), Error> {
    let sprite = assets
        .sprites
        .get(draw.resolved_sprite)
        .ok_or_else(|| Error::InvalidData(format!("missing sprite {}", draw.resolved_sprite)))?;
    frame.draw_sprite_clipped(sprite, draw.rect, viewport)
}

/// Final completed body of the currently verified, no-input gameplay batch.
pub const DEFAULT_EXPORT_BODY_LIMIT: u16 = 836;

/// Verified prefix for steering-controlled playback.
pub const DEFAULT_CONTROLLED_EXPORT_BODY_LIMIT: u16 = 90;

/// Four nominal 36.413 Hz timer units per ordinary completed loop body.
/// Original CPU/render costs and startup timer transitions are not emulated.
pub const NOMINAL_PREVIEW_BODY_PERIOD: std::time::Duration =
    std::time::Duration::from_nanos(109_850_804);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Random {
    words: [u16; 3],
}

impl Random {
    pub(crate) fn next_word(&mut self) -> u16 {
        let [mut a, mut b, mut c] = self.words;
        a = a.wrapping_add(0x2432) ^ b ^ c;
        b = (a.rotate_right(1).wrapping_sub(c) ^ b).wrapping_add(0x1238);
        let sum = u32::from((b ^ a).rotate_right(1)) + u32::from(a);
        // RCR includes the carry produced by ADD, unlike an ordinary rotation.
        b = ((sum as u16) >> 1) | if sum > 0xffff { 0x8000 } else { 0 };
        c = c.wrapping_add(a) ^ 0x2033 ^ b;
        self.words = [a, b, c];
        c
    }

    pub(crate) fn state(&self) -> [u16; 3] {
        self.words
    }
}

#[cfg(test)]
mod tests;
