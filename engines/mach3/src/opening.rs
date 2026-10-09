// Copyright (c) 2026 analogued. Project license has not been selected.
//! Gameplay simulation using interpreted game-state updates.

mod ending;
mod enemy;
mod hud;
mod player;
mod presentation;
mod progression;
mod session;
mod special;
mod terrain;
mod weapon;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{Frame, Rect},
    scene,
};

pub use ending::{Ending, EndingSnapshot, EndingStage, EndingTitle};
pub use enemy::EnemySnapshot as EnemyState;
pub use hud::HudSnapshot as HudState;
pub use player::{PlayerSnapshot as PlayerState, Steering};
pub use presentation::TransitionFrame;
pub use progression::{ScriptCommand, ScriptRules};
pub use session::{
    Controls, DisplayMode, GameStart, HeldKeys, InputPoll, KeyboardState, PauseControl, PauseEvent,
    PauseState, Preferences, RetainedWorld, SessionState,
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

/// Bounded encounter and collision/damage updates, with optional steering.
#[derive(Debug)]
pub struct Opening {
    body: u16,
    render_pass: u8,
    body_limit: Option<u16>,
    static_lateral: i16,
    random: Random,
    rules: Option<ScriptRules>,
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
    distance_hint: i16,
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
    updating_before_render: bool,
    selected_layer: u16,
    last_background_layer: u16,
    remembered_draws: [Option<scene::Draw>; 29],
    remembered_requests: [usize; 29],
    transitions: Vec<TransitionFrame>,
}

impl Opening {
    pub fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let initial_frame = scene::initial_frame(assets)?;
        let mut random = Random::default();
        random.next_word(); // Intro screen selection.
        random.next_word(); // Fresh-process script selection: script zero.
        let mut opening = Self {
            body: 0,
            render_pass: 0,
            body_limit: Some(LAST_BODY),
            static_lateral: 0,
            random,
            rules: None,
            script_index: 0,
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
            distance_hint: 2400,
            hud: hud::Hud::new(assets)?,
            terrain: terrain::Terrain::new(),
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
            updating_before_render: false,
            selected_layer: 0,
            last_background_layer: 0,
            remembered_draws: [None; 29],
            remembered_requests: [0; 29],
            transitions: Vec::new(),
        };
        let depths = opening.world_depths();
        opening.sort_full_world(&depths);
        Ok(opening)
    }

    /// A short controlled prefix, independently verified with held steering.
    pub fn with_steering(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut opening = Self::new(assets)?;
        opening.body_limit = Some(STEERING_LAST_BODY);
        Ok(opening)
    }

    /// Construct a bounded game using external encounter rules and retained session state.
    pub fn from_game_start(
        assets: &GraphicsAssets,
        rules: ScriptRules,
        start: GameStart,
        limit: u16,
    ) -> Result<Self, Error> {
        let mut opening = Self::new(assets)?;
        opening.random = Random {
            words: start.random,
        };
        opening.script_index = start.script_index;
        opening.body_limit = Some(limit);
        opening.terrain.set_data_segment(rules.data_segment());
        opening.rules = Some(rules);
        opening.enemy.reset_for_game(assets)?;
        if let Some(world) = start.retained_world {
            opening.enemy = enemy::Enemy::restore_snapshots(
                std::array::from_fn(|i| world.records[i + 4]),
                assets,
            )?;
            opening.terrain.restore_snapshots(std::array::from_fn(|i| {
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
            opening.special.restore_snapshots(world.special);
            opening.weapon.restore([world.records[2], world.records[3]]);
            opening.static_lateral = world.records[28].lateral;
        }
        // Initialization resolves and projects the reset records before its first
        // displayed frame. Retained unused coordinates and animation fields survive.
        opening.weapon.project(assets, true, 106, 0)?;
        opening.enemy.project(assets, true, 106, 0)?;
        opening.terrain.project(assets, true, 106, 0)?;
        opening.special.project(true, 106, 0)?;
        opening.full_world_order = (0..29).collect();
        let depths = opening.world_depths();
        opening.sort_full_world(&depths);
        opening.sort_world(&depths);
        Ok(opening)
    }

    /// Start live gameplay without an export limit. Game over still completes
    /// the current body's three passes before returning to the ending sequence.
    pub fn for_play(
        assets: &GraphicsAssets,
        rules: ScriptRules,
        start: GameStart,
    ) -> Result<Self, Error> {
        let mut opening = Self::from_game_start(assets, rules, start, 0)?;
        opening.body_limit = None;
        Ok(opening)
    }

    pub fn request_game_over(&mut self) {
        self.player.end_game();
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

    /// State at the last ordinary framebuffer checkpoint, before end-of-body events.
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
        if self.complete() {
            return Ok(false);
        }
        self.updating_before_render = true;
        if self.at_body_boundary() {
            self.body = self.body.wrapping_add(1);
            self.advance_player(input)?;
            self.advance_world(assets, true)?;
            self.player.project(assets, self.vertical_origin)?;
            // Far-object ordering remains from the first render pass even when
            // later movement changes relative depths without reprojecting.
            let first_depths = self.world_depths();
            self.sort_world(&first_depths);
            self.sort_full_world(&first_depths);
            self.hud.set_orientation(self.player.snapshot().orientation);
            self.hud.set_progression(self.progression);
            self.hud.set_score_multiplier(1);
            if let Some(rules) = &self.rules {
                self.hud
                    .set_marker_styles(assets, rules.marker_styles(self.difficulty)?)?;
            }
            let effects = self.hud.update_with_weapon(
                self.body,
                &mut self.random,
                self.weapon.snapshot().counter,
            )?;
            if let Some(counter) = effects.weapon_counter {
                self.weapon.set_counter(counter);
            }
            if let Some(damage) = effects.damage {
                self.player.hit(damage);
            }
            self.advance_strip();
            self.advance_speed();
            self.render_pass = 1;
        } else {
            self.advance_player(input)?;
            if self.render_pass == 2 {
                self.advance_world(assets, false)?;
            }
            self.player.project(assets, self.vertical_origin)?;
            self.render_pass += 1;
        }
        self.render(assets)?;
        self.pass_frame = self.frame.clone();
        self.pass_snapshot = Some(Box::new(self.snapshot()));
        self.updating_before_render = false;
        self.hud
            .set_score_multiplier(if self.selected_layer == 1 { 4 } else { 1 });
        if self.render_pass != 3 {
            return Ok(true);
        }
        self.player.finish_body();

        // Event creation and recycling happen after the last framebuffer write.
        if self.overlay != 0 {
            self.overlay = self.overlay.wrapping_add(1);
            if self.overlay > 80 {
                self.overlay = 0;
                self.queue_transition(assets)?;
            }
        }
        if self.rules.is_some() {
            self.parse_commands(assets)?;
        } else {
            if !self.enemy.active() {
                let old_delay = self.script_delay;
                self.script_delay += 1;
                if old_delay >= 20 {
                    self.script_delay = 1; // Non-wave handler repeats the delay gate once.
                    match self.script_cursor {
                        0 => self.terrain_selector = 5,
                        1 => {} // Palette-family 1 is already active.
                        2 => {
                            self.enemy.spawn_command(
                                0x91,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        3 => self.terrain_selector = 7,
                        4 => {
                            self.enemy.spawn_command(
                                0x84,
                                &mut self.random,
                                0,
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        5 => self.terrain_selector = 6,
                        6 => {
                            self.enemy.spawn_command(
                                0x8e,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        7 => self.terrain_selector = 3,
                        8 => {
                            self.enemy.spawn_command(
                                0x8a,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        9 => self.terrain_selector = 7,
                        10 => {
                            self.random.next_word();
                            self.primary_count += 1;
                        }
                        11 => {
                            self.enemy.spawn_command(
                                0x88,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        12 => self.terrain_selector = 9,
                        13 => self.special.spawn_command_40(&mut self.random, 28, false),
                        14 => {
                            self.enemy.spawn_command(
                                0x97,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        15 => self.terrain_selector = 9,
                        16 => self.special.spawn_command_40(&mut self.random, 28, false),
                        17 => {
                            self.enemy.spawn_command(
                                0x96,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        18 => self.terrain_selector = 2,
                        19 => {
                            self.random.next_word();
                            self.primary_count += 1;
                        }
                        20 => {
                            self.enemy.spawn_command(
                                0x84,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        21 => self.terrain_selector = 2,
                        22 => {
                            self.enemy.spawn_command(
                                0x92,
                                &mut self.random,
                                self.player.snapshot().coordinates[1],
                                usize::from(self.primary_count),
                            )?;
                            self.script_delay = 0;
                        }
                        _ => return Err(Error::InvalidData("unsupported opening command".into())),
                    }
                    self.script_cursor += 1;
                }
            }
        }
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
        Ok(true)
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

    pub fn take_transitions(&mut self) -> Vec<TransitionFrame> {
        std::mem::take(&mut self.transitions)
    }

    fn render(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        let state = self.presentation();
        let strip = self.snapshot().strip_index;
        let viewport = presentation::VIEWPORT;
        if self.render_pass == 1 {
            self.selected_layer = state.world_layer();
        }
        self.last_background_layer = self.selected_layer;
        let background = presentation::Presentation {
            layer: self.selected_layer,
            overlay_counter: 0,
            ..state
        }
        .background(assets, strip, self.strip_y, self.render_pass)?;
        let mut frame = self.frame.clone();
        if self.render_pass == 1 {
            // HUD writes have their own normal background layer. Retain indicator
            // pixels because their masks deliberately update the preceding frame.
            let hud_background = state.background_with_layer(assets, strip, self.strip_y, 0)?;
            let mut packed = frame.packed().to_vec();
            for y in 0..200 {
                for x in 0..80 {
                    if (44..158).contains(&y) && (4..76).contains(&x) {
                        continue;
                    }
                    if (13..20).contains(&y) && ((28..34).contains(&x) || (47..53).contains(&x)) {
                        continue;
                    }
                    packed[y * 80 + x] = hud_background.packed()[y * 80 + x];
                }
            }
            frame = Frame::from_packed(&packed)?;
        }
        let current = self.world_draws();
        let requests = self.world_requests();
        if self.render_pass == 1 {
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
            for &clip in &regions {
                presentation::restore_background(&mut frame, &background, clip)?;
            }
            for clip in regions {
                self.draw_world(assets, &mut frame, clip)?;
            }
        }
        if self.render_pass == 1 {
            for draw in self.hud.draws() {
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

    fn advance_world(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
    ) -> Result<(), Error> {
        let player = self.player.snapshot();
        let context = self.enemy_context()?;
        let effects = self.enemy.update_half_step_with_context(
            &mut self.random,
            player.coordinates,
            context,
        )?;
        self.apply_enemy_effects(effects);
        let collected_fuel = self
            .terrain
            .update_half_step_at_speed(player.coordinates, self.speeds[0]);
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
                enemy::EnemyCollision::Projectile => self.player.hit(1),
                enemy::EnemyCollision::Craft(slot) => {
                    if self.enemy.snapshots()[slot - 4].variant != 2 {
                        self.hud.add_points(1000);
                    }
                    self.enemy.explode_craft(slot)?;
                    self.player.hit(2);
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
            self.hud.add_fuel(collision.fuel);
            for points in collision.awards {
                self.hud.add_points(points);
            }
            self.player.hit(collision.damage);
        }
        if let Some(collision) = self.special.player_collision(
            assets,
            player.coordinates,
            player.resolved_sprite,
            self.player.snapshot().phase == 0,
            self.player.can_damage(),
        )? {
            for points in collision.awards {
                self.hud.add_points(points);
            }
            if let Some(transition) = collision.transition {
                match transition {
                    special::SpecialTransition::Normal => self.change_stage(false),
                    special::SpecialTransition::Alternate => self.change_stage(true),
                }
                self.queue_transition(assets)?;
                self.follow_edge(true)?;
            }
            if let Some(damage) = collision.damage {
                self.player.hit(damage);
            }
        }
        self.collide_weapon(assets)?;
        let player = self.player.snapshot();
        self.weapon
            .advance(player.effective_input & 16 != 0, player.coordinates);
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
        )
    }

    fn collide_weapon(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
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
                    self.enemy.hit_by_weapon(slot)?;
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
                if self.weapon.overlaps(assets, index, enemy)?
                    && self.terrain.shot_collision(target.slot, self.speeds[0])? != 0
                {
                    self.hud.add_points(400);
                    self.hud.add_points(500);
                }
            }
        }
        Ok(())
    }

    fn enemy_context(&self) -> Result<enemy::EnemyContext, Error> {
        Ok(enemy::EnemyContext {
            speeds: self.speeds,
            difficulty: self.difficulty as i16,
            alternate_shadows: self.overlay.wrapping_add(self.layer) as i16 > 0,
            horizon: self.player.snapshot().horizon,
            patrol_sources: self
                .rules
                .as_ref()
                .map(|rules| rules.patrol_sources(self.difficulty))
                .transpose()?,
        })
    }

    fn apply_enemy_effects(&mut self, effects: enemy::EnemyEffects) {
        self.hud.add_points(effects.points);
        self.player.add_lives(effects.lives);
        self.hud.set_lives(self.player.snapshot().lives);
        self.difficulty = self.difficulty.wrapping_add(u16::from(effects.difficulty));
        if let Some(distance) = effects.distance_hint {
            self.distance_hint = self.distance_hint.min(distance);
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
        self.script_index = self
            .rules
            .as_ref()
            .ok_or_else(|| Error::InvalidData("progression requires external rules".into()))?
            .progression(self.script_index, goal)?;
        self.progression = self.progression.wrapping_add(1);
        self.script_cursor = 0;
        self.enemy.clear_primary();
        self.special.clear_active();
        Ok(())
    }

    fn parse_commands(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        // Negative delays permit several commands in one invocation. Detect a
        // resource cycle which would stall the original, instead of hanging UI.
        let mut visited = std::collections::HashSet::new();
        while !self.enemy.active() {
            let old = self.script_delay as i16;
            self.script_delay = self.script_delay.wrapping_add(1);
            if old < progression::delay_threshold(self.difficulty) {
                break;
            }
            if !visited.insert((self.script_index, self.script_cursor, self.difficulty)) {
                return Err(Error::InvalidData(
                    "encounter command cycle stalls without an active wave".into(),
                ));
            }
            self.script_delay = 0;
            let command = self
                .rules
                .as_ref()
                .expect("external parser rules")
                .next(self.script_index, self.script_cursor)?;
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
                    let effects = self.enemy.spawn_command_with_context(
                        command,
                        &mut self.random,
                        self.player.snapshot().coordinates,
                        usize::from(self.primary_count),
                        context,
                    )?;
                    self.apply_enemy_effects(effects);
                }
                ScriptCommand::Control(command) => match command {
                    0xc0..=0xc2 | 0xc4 | 0xc8 | 0xc9 => {}
                    0xc3 => {
                        self.random.next_word();
                        self.hud.add_points(3000);
                    }
                    0xc5 => self.speed_mode = 1,
                    0xc6 => self.speed_mode = 2,
                    0xc7 => {
                        self.overlay = 0;
                        self.queue_transition(assets)?;
                    }
                    0xca => {
                        self.script_cursor = 0;
                        self.script_index = self.random.next_word() & 3;
                    }
                    0xcb => {
                        self.change_stage(true);
                        self.queue_transition(assets)?;
                    }
                    0xcc => {
                        self.change_stage(false);
                        self.queue_transition(assets)?;
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
                    }
                    _ => {
                        return Err(Error::InvalidData(format!(
                            "unknown control command {command:#x}"
                        )));
                    }
                },
            }
        }
        Ok(())
    }

    fn advance_player(&mut self, input: Steering) -> Result<(), Error> {
        let reset_fuel = if input.bits() == 0 {
            self.player.update_no_input()?
        } else {
            self.player.update(input)?
        };
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
        Ok(())
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

/// Final completed body of the currently verified, no-input opening batch.
pub const LAST_BODY: u16 = 836;

/// Verified prefix for steering-controlled playback.
pub const STEERING_LAST_BODY: u16 = 90;

/// Three nominal 36.413 Hz timer units per completed loop body.
/// Original CPU/render costs and startup timer transitions are not emulated.
pub const BODY_PERIOD: std::time::Duration = std::time::Duration::from_nanos(82_388_102);

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
mod tests {
    use super::*;

    #[test]
    fn random_preserves_sixteen_bit_arithmetic_and_add_carry() {
        let mut random = Random::default();
        let words: Vec<_> = (0..4).map(|_| random.next_word()).collect();
        assert_eq!(words, [0x5630, 0xbe90, 0x6338, 0x4293]);
        assert_eq!(random.state(), [34449, 35689, 17043]);
    }

    fn synthetic_assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![0; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|index| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: if index == 13 {
                        [0, 0, 15, 0, 0, 0, 0]
                    } else {
                        [0; 7]
                    },
                    color: vec![0; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        }
    }

    #[test]
    fn authored_events_and_supported_boundary_preserve_the_final_state() {
        let assets = synthetic_assets();
        let mut opening = Opening::new(&assets).unwrap();
        assert_eq!(opening.frame(), &scene::initial_frame(&assets).unwrap());
        for body in 1..=360 {
            assert!(opening.advance(&assets).unwrap());
            let state = opening.snapshot();
            assert_eq!(state.body, body);
            assert_eq!(
                state.terrain_selector,
                if body < 21 {
                    0
                } else if body < 118 {
                    5
                } else if body < 201 {
                    7
                } else if body < 284 {
                    6
                } else {
                    3
                }
            );
            assert_eq!(
                state.enemies[1].active,
                (61..98).contains(&body)
                    || (138..181).contains(&body)
                    || (221..264).contains(&body)
                    || (304..341).contains(&body),
                "enemy activation at body {body}"
            );
            assert_eq!(
                state.script_cursor,
                if body < 21 {
                    0
                } else if body < 41 {
                    1
                } else if body < 61 {
                    2
                } else if body < 118 {
                    3
                } else if body < 138 {
                    4
                } else if body < 201 {
                    5
                } else if body < 221 {
                    6
                } else if body < 284 {
                    7
                } else if body < 304 {
                    8
                } else {
                    9
                }
            );
        }
        // Synthetic descriptor geometry does not reproduce the original
        // collision route. Exercise the bounded guard independently of it.
        opening.body = LAST_BODY;
        let final_frame = opening.frame().clone();
        let final_random = opening.snapshot().random;
        for _ in 0..3 {
            assert!(!opening.advance(&assets).unwrap());
        }
        assert!(opening.complete());
        assert_eq!(opening.snapshot().body, LAST_BODY);
        assert_eq!(opening.snapshot().random, final_random);
        assert_eq!(opening.frame(), &final_frame);
    }

    #[test]
    fn displayed_passes_sample_input_and_finish_the_boundary_only_after_pass_three() {
        let assets = synthetic_assets();
        let mut opening = Opening::with_steering(&assets).unwrap();
        opening.body_limit = Some(1);
        let initial = opening.snapshot();
        assert!(opening.at_body_boundary());
        for (pass, lateral, velocity) in [(1, -4, -1), (2, -12, -2), (3, -24, -3)] {
            assert!(
                opening
                    .advance_subframe(
                        &assets,
                        Steering {
                            left: true,
                            ..Steering::default()
                        }
                    )
                    .unwrap()
            );
            let state = opening.snapshot();
            assert_eq!(state.body, 1);
            assert_eq!(state.render_pass, pass);
            assert_eq!(state.player.coordinates[1], lateral);
            assert_eq!(state.player.velocity[0], velocity);
            assert_eq!(opening.at_body_boundary(), pass == 3);
            assert_eq!(opening.complete(), pass == 3);
            assert_eq!(state.script_delay, u16::from(pass == 3));
            assert_eq!(
                state.player.grace,
                initial.player.grace + u16::from(pass == 3 && initial.player.grace < 4)
            );
        }
        let last_frame = opening.frame().clone();
        assert!(
            !opening
                .advance_subframe(&assets, Steering::default())
                .unwrap()
        );
        assert_eq!(opening.frame(), &last_frame);
    }

    #[test]
    fn input_changes_between_passes_do_not_repeat_world_motion() {
        let assets = synthetic_assets();
        let mut opening = Opening::new(&assets).unwrap();
        let left = Steering {
            left: true,
            ..Steering::default()
        };
        opening.advance_subframe(&assets, left).unwrap();
        let first = opening.snapshot();
        assert!(opening.advance(&assets).is_err());
        opening
            .advance_subframe(&assets, Steering::default())
            .unwrap();
        let second = opening.snapshot();
        assert_eq!(second.player.velocity[0], 0);
        assert_eq!(second.player.coordinates[1], first.player.coordinates[1]);
        assert_eq!(second.terrain[0].depth, first.terrain[0].depth);
        assert_eq!(second.hud.score, first.hud.score);
        assert_eq!(second.random, first.random);
        assert_eq!(second.player.grace, first.player.grace);
        opening
            .advance_subframe(
                &assets,
                Steering {
                    right: true,
                    ..Steering::default()
                },
            )
            .unwrap();
        assert_eq!(opening.snapshot().player.velocity[0], 1);
        assert_eq!(opening.snapshot().player.coordinates[1], 0);
        assert!(opening.at_body_boundary());
        assert!(opening.advance(&assets).unwrap());
        assert_eq!(opening.snapshot().body, 2);
    }

    #[test]
    fn formation_count_command_consumes_one_word_and_waits_to_spawn() {
        let assets = synthetic_assets();
        let mut baseline = Opening::new(&assets).unwrap();
        baseline.script_cursor = 9;
        baseline.script_delay = 20;
        baseline.terrain_selector = 7;
        let mut formation = Opening::new(&assets).unwrap();
        formation.script_cursor = 10;
        formation.script_delay = 20;
        formation.terrain_selector = 7;
        baseline.advance(&assets).unwrap();
        formation.advance(&assets).unwrap();
        let mut expected_random = baseline.random;
        expected_random.next_word();
        assert_eq!(formation.random, expected_random);
        assert_eq!(formation.primary_count, 2);
        assert_eq!(formation.script_cursor, 11);
        assert_eq!(formation.script_delay, 1);
        assert!(!formation.enemy.active());
        for _ in 0..20 {
            formation.advance(&assets).unwrap();
        }
        let state = formation.snapshot();
        assert_eq!(state.script_cursor, 12);
        assert_eq!(state.script_delay, 0);
        assert_eq!(state.enemies[1].depth, 1600);
        assert_eq!(state.enemies[2].depth, 1500);
        assert!(state.enemies[1].active && state.enemies[2].active);
        assert!(state.enemies[6].active && state.enemies[7].active);
    }

    #[test]
    fn gate_allocation_preserves_the_delay_before_the_scripted_enemy() {
        let assets = synthetic_assets();
        let mut opening = Opening::new(&assets).unwrap();
        opening.script_cursor = 13;
        opening.script_delay = 20;
        opening.terrain_selector = 7;
        opening.primary_count = 2;
        opening.advance(&assets).unwrap();
        let state = opening.snapshot();
        assert_eq!(state.script_cursor, 14);
        assert_eq!(state.script_delay, 1);
        assert_eq!(
            state.special.map(|slot| slot.depth),
            [916, 1140, 1364, 1588, 1812]
        );
        assert!(state.special.iter().all(|slot| slot.active
            && slot.rect.x_bytes == 128
            && slot.rect.width_bytes == 43
            && slot.rect.height == 0));
        assert!(!opening.enemy.active());
        for _ in 0..20 {
            opening.advance(&assets).unwrap();
        }
        let state = opening.snapshot();
        assert_eq!(state.script_cursor, 15);
        assert_eq!(state.script_delay, 0);
        assert!(state.enemies[1].active);
        assert!(!state.enemies[2].active);
        assert_eq!(state.enemies[1].variant, 1);
        assert_eq!(state.enemies[1].animation_phase, 0);
        assert_eq!(state.enemies[1].depth, 1000);
        assert_eq!(state.enemies[2].depth, 1000);
        assert!(state.enemies[6].active);
        assert!(!state.enemies[7].active);
    }

    #[test]
    fn zero_movement_entries_keep_order_until_their_movement_changes() {
        let mut opening = Opening::new(&synthetic_assets()).unwrap();
        opening.world_order = vec![4, 5, 6, 15];
        opening.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 900, 0), (15, 200, 2)]);
        assert_eq!(opening.world_order, [5, 6, 15, 4]);
        opening.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 200, 8), (15, 200, 2)]);
        assert_eq!(opening.world_order, [5, 6, 15, 4]);
    }

    #[test]
    fn equal_depth_objects_retain_their_order_from_the_preceding_pass() {
        let mut opening = Opening::new(&synthetic_assets()).unwrap();
        let mut depths: Vec<_> = opening
            .world_order
            .iter()
            .map(|slot| (*slot, 100, 2))
            .collect();
        depths
            .iter_mut()
            .find(|(slot, _, _)| *slot == 22)
            .unwrap()
            .1 = 200;
        opening.sort_world(&depths);
        assert_eq!(opening.world_order[0], 22);
        depths
            .iter_mut()
            .find(|(slot, _, _)| *slot == 22)
            .unwrap()
            .1 = 100;
        opening.sort_world(&depths);
        assert_eq!(opening.world_order[0], 22);
    }
    fn synthetic_rules(commands: &[u8]) -> ScriptRules {
        let mut data = vec![0; 65536];
        data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
        for index in 0..21 {
            let at = 0x0a3e + 50 * index;
            data[at..at + commands.len()].copy_from_slice(commands);
            data[at + commands.len()] = 0xcf;
            data[0x0e58 + 4 * index..0x0e5a + 4 * index]
                .copy_from_slice(&(((index + 1) % 21) as u16).to_le_bytes());
            data[0x0e5a + 4 * index..0x0e5c + 4 * index]
                .copy_from_slice(&(index as u16).to_le_bytes());
        }
        ScriptRules::from_data_segment(&data).unwrap()
    }

    #[test]
    fn mode_reset_persists_until_an_actual_palette_command() {
        let assets = synthetic_assets();
        let start = SessionState::default().begin_game();
        let mut game = Opening::for_play(&assets, synthetic_rules(&[0xce, 0xce]), start).unwrap();
        for _ in 0..21 {
            game.advance(&assets).unwrap();
        }
        assert_eq!(game.palette_family(), 0);
        let mut before = game.snapshot();
        let pixels = game.frame().clone();
        game.reset_video_palette();
        assert_eq!(game.palette_family(), 1);
        assert_eq!(game.frame(), &pixels);
        before.palette_family = 1;
        assert_eq!(format!("{:?}", game.snapshot()), format!("{before:?}"));
        for _ in 0..19 {
            game.advance(&assets).unwrap();
            assert_eq!(game.palette_family(), 1);
        }
        // Even a repeated CE is a new register write, not a changed-value test.
        game.advance(&assets).unwrap();
        assert_eq!(game.snapshot().body, 41);
        assert_eq!(game.palette_family(), 0);
    }

    #[test]
    fn transition_compositor_does_not_change_the_pinned_body_layer() {
        let mut assets = synthetic_assets();
        assets.primary.fill(0x55);
        let mut game = Opening::new(&assets).unwrap();
        game.advance_subframe(&assets, Steering::default()).unwrap();
        assert_eq!(game.selected_layer, 0);
        game.layer = 2;
        game.queue_transition(&assets).unwrap();
        assert_eq!(game.selected_layer, 0);
        assert_eq!(game.snapshot().background_layer, 2);
        assert_eq!(game.frame().packed()[140 * 80 + 37], 0);
        for _ in 0..2 {
            game.advance_subframe(
                &assets,
                Steering {
                    right: true,
                    ..Steering::default()
                },
            )
            .unwrap();
            assert_eq!(game.selected_layer, 0);
            assert_eq!(game.snapshot().background_layer, 0);
            assert!(
                game.frame()
                    .packed()
                    .chunks(80)
                    .enumerate()
                    .any(|(y, row)| {
                        (44..158).contains(&y)
                            && !(92..107).contains(&y)
                            && row[4..76].contains(&0x55)
                    })
            );
        }
        game.advance_subframe(&assets, Steering::default()).unwrap();
        assert_eq!(game.selected_layer, 2);
        assert_eq!(game.snapshot().background_layer, 2);
    }

    #[test]
    fn ordinary_pass_capture_precedes_post_body_scene_transitions() {
        let mut assets = synthetic_assets();
        assets.alternate.fill(0x55);
        let start = SessionState::default().begin_game();
        let mut game = Opening::for_play(&assets, synthetic_rules(&[0xd6, 0x85]), start).unwrap();
        game.script_delay = 20;
        game.advance(&assets).unwrap();
        let pass_state = game.pass_snapshot().unwrap();
        assert_eq!(pass_state.script_delay, 20);
        assert_ne!(pass_state.script_cursor, game.snapshot().script_cursor);
        let transitions = game.take_transitions();
        assert_eq!(transitions.len(), 7);
        assert!(transitions.iter().all(|frame| !frame.before_pass));
        assert!(transitions.iter().all(|frame| frame.state.is_some()));
        assert_ne!(game.pass_frame(), game.frame());
        assert_eq!(game.frame(), &transitions.last().unwrap().frame);
        assert_eq!(game.snapshot().render_pass, 3);
    }

    #[test]
    fn live_play_passes_export_limits_wraps_the_counter_and_obeys_game_over() {
        let assets = synthetic_assets();
        let start = SessionState::default().begin_game();
        let mut game = Opening::for_play(&assets, synthetic_rules(&[0x85]), start).unwrap();
        for boundary in [STEERING_LAST_BODY, LAST_BODY] {
            game.body = boundary - 1;
            for expected in [boundary, boundary + 1] {
                assert!(game.advance(&assets).unwrap());
                assert_eq!(game.snapshot().body, expected);
                assert!(!game.complete());
            }
        }
        game.body = u16::MAX;
        assert!(game.advance_subframe(&assets, Steering::default()).unwrap());
        assert_eq!(game.snapshot().body, 0);
        game.request_game_over();
        assert!(!game.complete());
        for _ in 0..2 {
            assert!(game.advance_subframe(&assets, Steering::default()).unwrap());
        }
        assert!(game.complete());
        assert!(!game.advance(&assets).unwrap());
    }

    #[test]
    fn new_game_reprojects_reset_shots_and_preserves_unused_enemy_coordinates() {
        let assets = synthetic_assets();
        let previous = Opening::new(&assets).unwrap();
        let ending = Ending::new(&assets, &previous.snapshot(), previous.frame()).unwrap();
        let mut session = SessionState::default();
        session.remember_ending(&ending.snapshot(), ending.special_snapshots());
        let world = session.retained_world.as_mut().unwrap();
        world.records[5].depth = 692;
        for shot in &mut world.records[2..4] {
            shot.requested_sprite = 23;
            shot.resolved_sprite = 23;
            shot.rect.x_bytes = 17;
        }
        let start = session.begin_game();
        let expected_random = start.random;
        let game = Opening::from_game_start(&assets, synthetic_rules(&[0x85]), start, 90).unwrap();
        let state = game.snapshot();
        assert_eq!(state.random, expected_random);
        assert_eq!(state.enemies[1].depth, 692);
        assert!(!state.enemies[1].active);
        for shot in state.weapon.shots {
            assert_eq!(shot.requested_sprite, 22);
            assert_eq!(shot.resolved_sprite, 22);
            assert_eq!(shot.rect.x_bytes, 128);
            assert!(!shot.active);
        }
    }

    #[test]
    fn external_parser_preserves_delay_and_continues_after_non_wave_commands() {
        let assets = synthetic_assets();
        let mut game = Opening::from_game_start(
            &assets,
            synthetic_rules(&[1, 0xc5, 0x85]),
            GameStart {
                script_index: 2,
                random: [0; 3],
                retained_world: None,
            },
            90,
        )
        .unwrap();
        for _ in 0..20 {
            game.advance(&assets).unwrap();
        }
        assert_eq!((game.script_cursor, game.terrain_selector), (0, 0));
        game.advance(&assets).unwrap();
        assert_eq!(
            (game.script_cursor, game.terrain_selector, game.script_delay),
            (1, 1, 1)
        );
        for _ in 0..20 {
            game.advance(&assets).unwrap();
        }
        assert_eq!(game.speed_mode, 1);
        assert_eq!(game.speeds, [28, 22, 16]);
        game.advance(&assets).unwrap();
        assert_eq!(game.speeds, [29, 23, 17]);
        assert_eq!(game.script_index, 2);
    }

    #[test]
    fn negative_delay_can_execute_several_commands_and_follow_both_edges() {
        let assets = synthetic_assets();
        let mut game = Opening::from_game_start(
            &assets,
            synthetic_rules(&[1, 0xc5, 0x85]),
            GameStart {
                script_index: 2,
                random: [0; 3],
                retained_world: None,
            },
            90,
        )
        .unwrap();
        game.difficulty = 3;
        game.parse_commands(&assets).unwrap();
        assert_eq!(game.script_cursor, 3);
        assert!(game.enemy.active());
        assert_eq!(game.speed_mode, 1);
        game.follow_edge(true).unwrap();
        assert_eq!(
            (game.script_index, game.script_cursor, game.progression),
            (2, 0, 2)
        );
        assert!(!game.enemy.active());
        game.follow_edge(false).unwrap();
        assert_eq!(
            (game.script_index, game.script_cursor, game.progression),
            (3, 0, 3)
        );
    }

    #[test]
    fn speed_limits_and_deceleration_keep_the_three_speeds_in_step() {
        let assets = synthetic_assets();
        let mut game = Opening::new(&assets).unwrap();
        game.speed_mode = 1;
        for _ in 0..40 {
            game.advance_speed();
        }
        assert_eq!(game.speeds, [56, 50, 44]);
        game.speed_mode = 2;
        for _ in 0..14 {
            game.advance_speed();
        }
        assert_eq!(game.speeds, [28, 22, 16]);
        game.advance_speed();
        assert_eq!(game.speed_mode, 0);
        game.request_game_over();
        assert!(game.complete());
    }
    #[test]
    fn weapon_hits_destroy_craft_pay_score_and_leave_other_slots_available() {
        let assets = synthetic_assets();
        let mut game = Opening::new(&assets).unwrap();
        game.enemy
            .spawn_command(0x84, &mut game.random, 0, 1)
            .unwrap();
        let target = game.enemy.snapshots()[1];
        let mut shots = game.weapon.snapshot().shots;
        shots[0].active = true;
        shots[0].depth = target.depth;
        shots[0].lateral = target.lateral;
        shots[0].height = target.height;
        game.weapon.restore(shots);
        game.collide_weapon(&assets).unwrap();
        assert!(!game.weapon.snapshot().shots[0].active);
        assert_eq!(game.enemy.snapshots()[1].movement, 3);
        assert_eq!(game.hud.snapshot().pending_score, 1500);
        game.collide_weapon(&assets).unwrap();
        assert_eq!(game.hud.snapshot().pending_score, 1500);
    }

    #[test]
    fn weapon_terrain_conversion_does_not_consume_the_projectile_or_collect_fuel() {
        let assets = synthetic_assets();
        let mut game = Opening::new(&assets).unwrap();
        game.terrain.spawn_goal_barrier(28, 0);
        let target = game.terrain.snapshots()[0];
        let mut shots = game.weapon.snapshot().shots;
        shots[0].active = true;
        shots[0].depth = target.depth;
        shots[0].lateral = target.lateral;
        shots[0].height = -100;
        game.weapon.restore(shots);
        let fuel = game.hud.snapshot().fuel;
        game.collide_weapon(&assets).unwrap();
        assert!(game.weapon.snapshot().shots[0].active);
        assert_eq!(game.terrain.snapshots()[0].movement, 3);
        assert_eq!(game.hud.snapshot().pending_score, 900);
        assert_eq!(game.hud.snapshot().fuel, fuel);
    }
    #[test]
    fn extra_life_awards_fill_only_missing_symbols_and_terrain_awards_cap_separately() {
        let assets = synthetic_assets();
        let mut game = Opening::new(&assets).unwrap();
        game.apply_enemy_effects(enemy::EnemyEffects {
            lives: 1,
            difficulty: 1,
            ..Default::default()
        });
        assert_eq!((game.player.snapshot().lives, game.difficulty), (6, 1));
        game.terrain.spawn_goal_barrier(28, 0);
        let target = game.terrain.snapshots()[0];
        let mut shots = game.weapon.snapshot().shots;
        shots[0].active = true;
        shots[0].depth = target.depth;
        shots[0].lateral = target.lateral;
        shots[0].height = -100;
        game.weapon.restore(shots);
        game.hud.add_points(31_900);
        game.collide_weapon(&assets).unwrap();
        assert_eq!(game.hud.snapshot().pending_score, 32_000);
    }
}
