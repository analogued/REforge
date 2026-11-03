// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Enemy movement, ground shadows, seeking projectiles and explosions.

use super::{Random, progression};
use crate::{
    Error,
    assets::GraphicsAssets,
    frame::Rect,
    scene::Draw,
    sound::{self, Effect, Foreground, GameplayAction, Ramp, Resource, SampleRequest, Tone},
};

#[derive(Clone, Copy, Debug)]
pub struct EnemySnapshot {
    pub slot: usize,
    pub depth: i16,
    pub lateral: i16,
    pub height: i16,
    pub active: bool,
    pub movement: u8,
    pub variant: u8,
    pub animation_phase: u8,
    pub requested_sprite: usize,
    pub resolved_sprite: usize,
    pub rect: Rect,
    pub phase: u8,
}

impl EnemySnapshot {
    fn new(slot: usize, sprite: usize, depth: i16, height: i16) -> Self {
        Self {
            slot,
            depth,
            lateral: 0,
            height,
            active: false,
            movement: if slot == 4 { 253 } else { 0 },
            variant: 0,
            animation_phase: 0,
            requested_sprite: sprite,
            resolved_sprite: sprite,
            rect: Rect {
                x_bytes: 128,
                y: 0,
                width_bytes: 2,
                height: 2,
            },
            phase: 0,
        }
    }

    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        let sprite = assets.sprites.get(self.requested_sprite).ok_or_else(|| {
            Error::InvalidData(format!(
                "missing opening wave sprite {}",
                self.requested_sprite
            ))
        })?;
        self.resolved_sprite = self.requested_sprite;
        self.rect.width_bytes = sprite.width_bytes;
        self.rect.height = sprite.height;
        if !self.active || self.depth >= 1000 {
            self.rect.x_bytes = 128;
            return Ok(());
        }
        // The original perspective table wraps its index to ten bits and
        // multiplies the resulting word as signed, including index zero.
        let scale = i32::from(((64 * 32768) / (i32::from(self.depth & 1023) + 64)) as i16);
        let horizontal = (scale * i32::from(self.lateral)) >> 16;
        self.phase = (horizontal & 3) as u8;
        self.rect.x_bytes = 40 + (horizontal >> 2) - i32::from(sprite.descriptor[5]);
        let vertical = (scale * i32::from(self.height.wrapping_sub(camera_height))) >> 16;
        self.rect.y = vertical_origin - vertical - i32::from(sprite.descriptor[6]);
        Ok(())
    }

    pub(crate) fn draw(self) -> Draw {
        Draw {
            resolved_sprite: self.resolved_sprite,
            rect: self.rect,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnemyCollision {
    Projectile,
    Craft(usize),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EnemyContext {
    pub speeds: [i16; 3],
    pub difficulty: u16,
    pub alternate_shadows: bool,
    pub horizon: i16,
    pub patrol_sources: Option<[usize; 3]>,
}

impl Default for EnemyContext {
    fn default() -> Self {
        Self {
            speeds: [28, 22, 16],
            difficulty: 0,
            alternate_shadows: false,
            horizon: 0,
            patrol_sources: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EnemyEffects {
    pub points: u16,
    pub lives: u8,
    pub difficulty: u8,
    pub distance_hint: Option<i16>,
}

impl EnemyEffects {
    fn merge(&mut self, other: Self) {
        self.points = self.points.wrapping_add(other.points);
        self.lives = self.lives.wrapping_add(other.lives);
        self.difficulty = self.difficulty.wrapping_add(other.difficulty);
        self.distance_hint = self
            .distance_hint
            .into_iter()
            .chain(other.distance_hint)
            .max();
    }
}

/// Retained foreground state while a craft's synchronous sample is running.
#[derive(Debug)]
pub(crate) struct EnemyUpdate {
    next_slot: usize,
    player: [i16; 3],
    context: EnemyContext,
    effects: EnemyEffects,
    pending_sample: bool,
}

impl EnemyUpdate {
    /// Effects become available after `resume_update` returns `None`.
    pub(crate) fn effects(&self) -> EnemyEffects {
        assert!(self.next_slot == 5 && !self.pending_sample);
        self.effects
    }
}

#[derive(Debug)]
pub(crate) struct Enemy {
    bullet: EnemySnapshot,
    crafts: [EnemySnapshot; 5],
    pub(super) births: [u64; 5],
    pub(super) bullet_birth: u64,
    pub(super) projected_births: [u64; 11],
    shadows: [EnemySnapshot; 5],
    target: [i16; 2],
    orbit_center: [i16; 2],
    craft_animations: [Option<u8>; 5],
    falling_count: i16,
    falling_movement: u8,
}

impl Enemy {
    pub(crate) fn shift_lateral(&mut self, delta: i16) {
        for record in std::iter::once(&mut self.bullet)
            .chain(self.crafts.iter_mut())
            .chain(self.shadows.iter_mut())
        {
            record.lateral = record.lateral.wrapping_add(delta);
        }
    }

    pub(crate) fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut state = Self {
            bullet: EnemySnapshot::new(4, 22, 14, -100),
            crafts: std::array::from_fn(|i| EnemySnapshot::new(5 + i, 0, 0, 0)),
            births: [0; 5],
            bullet_birth: 0,
            projected_births: [0; 11],
            shadows: std::array::from_fn(|i| EnemySnapshot::new(10 + i, 0, 0, 0)),
            target: [0, 0],
            orbit_center: [0, -100],
            craft_animations: [None; 5],
            falling_count: 0,
            falling_movement: 0,
        };
        state.project(assets, true, 106, 0)?;
        Ok(state)
    }

    /// Initialize primaries while retaining their resolved source and projection.
    #[cfg(test)]
    pub(crate) fn spawn_command(
        &mut self,
        command: u8,
        random: &mut Random,
        player_lateral: i16,
        count: usize,
    ) -> Result<(), Error> {
        self.spawn_command_with_sound(command, random, player_lateral, count, &mut |_| {})
    }

    #[cfg(test)]
    pub(crate) fn spawn_command_with_sound(
        &mut self,
        command: u8,
        random: &mut Random,
        player_lateral: i16,
        count: usize,
        sound: &mut Foreground<'_>,
    ) -> Result<(), Error> {
        self.spawn_with_sound(
            command,
            random,
            [14, player_lateral, -100],
            count,
            EnemyContext::default(),
            sound,
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn spawn_command_with_context(
        &mut self,
        command: u8,
        random: &mut Random,
        player: [i16; 3],
        count: usize,
        context: EnemyContext,
    ) -> Result<EnemyEffects, Error> {
        self.spawn_with_sound(command, random, player, count, context, &mut |_| {})
    }

    pub(crate) fn spawn_with_sound(
        &mut self,
        command: u8,
        random: &mut Random,
        player: [i16; 3],
        count: usize,
        context: EnemyContext,
        sound: &mut Foreground<'_>,
    ) -> Result<EnemyEffects, Error> {
        if !(0x84..=0xa2).contains(&command) {
            return Err(Error::InvalidData(format!(
                "unsupported enemy command {command:02x}"
            )));
        }
        let mut effects = EnemyEffects::default();
        for i in 0..count.min(5) {
            {
                self.births[i] = self.births[i].wrapping_add(1);
            }
            let craft = &mut self.crafts[i];
            self.craft_animations[i] = None;
            craft.requested_sprite = 0;
            craft.movement = command & 63;
            craft.depth = 1000;
            craft.lateral = 0;
            craft.height = 0;
            craft.active = true;
            self.shadows[i].height = -115;
            self.shadows[i].active = true;
            craft.animation_phase = 0;
            craft.variant = match command {
                0x95..=0x98 | 0x9d..=0x9e => 1,
                0x99..=0x9a => 2,
                0x9b..=0x9c => 3,
                0x9f..=0xa1 => 5,
                0xa2 => 6,
                _ => 0,
            };
            match command {
                0x91..=0x92 => {
                    craft.depth = 1600 - 100 * i as i16;
                    craft.lateral = ((random.next_word() & 1023) as i16 - 512)
                        .wrapping_sub(player[1].wrapping_mul(2));
                    craft.height = (random.next_word() & 255) as i16 - 50;
                }
                0x95 | 0x97 | 0x9d => {
                    if i != 0 {
                        craft.active = false;
                        self.shadows[i].active = false;
                    }
                }
                0x96 | 0x98 | 0x9e => craft.animation_phase = 4 * i as u8,
                0x99..=0x9a => {
                    if i == 0 {
                        self.falling_count = self.falling_count.wrapping_add(1);
                    }
                    sound::request(sound, 0x1a7e, Effect::Tone(Tone::Second));
                    self.falling_movement = craft.movement;
                    craft.depth = player[0].wrapping_add(6);
                    self.shadows[i].active = false;
                    self.initialize_falling(i, random, player[0]);
                    if 2 * i as i16 > self.falling_count {
                        self.crafts[i].active = false;
                        self.crafts[i].depth = player[0];
                    }
                }
                0x9b..=0x9c => {
                    if i == 0 {
                        effects.difficulty = 1;
                        effects.lives = 1;
                    }
                    self.shadows[i].active = false;
                    if i != 0 {
                        craft.active = false;
                    }
                }
                0x9f => {
                    if i > 2 {
                        craft.active = false;
                        self.shadows[i].active = false;
                    }
                }
                0xa0..=0xa1 => {
                    if i > 3 {
                        craft.active = false;
                        self.shadows[i].active = false;
                    }
                }
                0xa2 => {
                    craft.depth = 188;
                    craft.lateral = -1000 - 280 * i as i16;
                    if i > 2 || i as u16 > context.difficulty {
                        craft.active = false;
                        self.shadows[i].active = false;
                    }
                }
                _ => craft.depth = 1600 - 100 * i as i16,
            }
        }
        Ok(effects)
    }

    /// World movement precedes collisions and source resolution. Each active
    /// primary consumes a firing word even when its variant cannot fire.
    #[cfg(test)]
    pub(crate) fn update_half_step(&mut self, random: &mut Random, player: [i16; 3]) {
        self.update_half_step_with_context(random, player, EnemyContext::default())
            .expect("default patrol resources contain distinct sources");
    }

    #[cfg(test)]
    pub(crate) fn update_half_step_with_context(
        &mut self,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
    ) -> Result<EnemyEffects, Error> {
        self.update_with_sound(random, player, context, &mut |_| {})
    }

    #[cfg(test)]
    pub(crate) fn update_with_sound(
        &mut self,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        sound: &mut Foreground<'_>,
    ) -> Result<EnemyEffects, Error> {
        let mut task = self.begin_update(player, context);
        while let Some(action) = self.resume_update(&mut task, random, sound)? {
            sound(action);
        }
        Ok(task.effects())
    }

    pub(crate) fn begin_update(&mut self, player: [i16; 3], context: EnemyContext) -> EnemyUpdate {
        if self.bullet.active {
            self.bullet.requested_sprite = projectile_sprite(self.bullet.depth);
            self.bullet.lateral = self.bullet.lateral.wrapping_add(
                self.target[0]
                    .wrapping_sub(self.bullet.lateral)
                    .clamp(-80, 80),
            );
            self.bullet.height = self.bullet.height.wrapping_add(
                self.target[1]
                    .wrapping_sub(self.bullet.height)
                    .clamp(-80, 80),
            );
            self.bullet.depth = self.bullet.depth.wrapping_sub(context.speeds[0]);
            if self.bullet.depth <= player[0] {
                self.bullet.requested_sprite = 0;
                self.bullet.active = false;
            }
        }
        EnemyUpdate {
            next_slot: 0,
            player,
            context,
            effects: EnemyEffects::default(),
            pending_sample: false,
        }
    }

    /// A returned sample must finish before this task is resumed again.
    pub(crate) fn resume_update(
        &mut self,
        task: &mut EnemyUpdate,
        random: &mut Random,
        sound: &mut Foreground<'_>,
    ) -> Result<Option<GameplayAction>, Error> {
        if task.pending_sample {
            let i = task.next_slot;
            self.crafts[i].animation_phase = 1;
            self.crafts[i].depth = 50;
            let mut effects = EnemyEffects::default();
            self.transition_motion_tail(i, task.context, &mut effects);
            task.effects.merge(self.finish_craft(
                i,
                random,
                task.player,
                task.context,
                ([0; 3], effects),
                sound,
            ));
            task.pending_sample = false;
            task.next_slot += 1;
        }
        while task.next_slot < 5 {
            let Some(effects) =
                self.update_craft(task.next_slot, random, task.player, task.context, sound)?
            else {
                task.pending_sample = true;
                return Ok(Some(GameplayAction::PlaySample {
                    site: 0x42bf,
                    request: SampleRequest {
                        resource: Resource::Data8,
                        offset: 0,
                        count: 9723,
                        reverse: false,
                    },
                }));
            };
            task.effects.merge(effects);
            task.next_slot += 1;
        }
        for i in 0..5 {
            if !self.crafts[i].active {
                self.shadows[i].active = false;
            }
        }
        Ok(None)
    }

    fn initialize_falling(&mut self, i: usize, random: &mut Random, player_depth: i16) {
        let craft = &mut self.crafts[i];
        craft.active = true;
        craft.height = (random.next_word() & 255) as i16 + 280;
        let side = (random.next_word() & 256) as i16;
        craft.lateral = (random.next_word() & 255) as i16 + 3 * side - 512;
        if craft.depth == player_depth.wrapping_add(6) {
            craft.lateral = 2000;
            craft.height = 0;
        }
        let mut phase = (random.next_word() & 1) as u8;
        if craft.lateral > 100 {
            phase |= 2;
        } else if craft.lateral > -100 {
            phase |= (random.next_word() & 2) as u8;
        }
        craft.animation_phase = phase;
    }

    fn falling_explosion(&mut self, i: usize, random: &mut Random, player_depth: i16) {
        let craft = &mut self.crafts[i];
        let phase = craft.animation_phase;
        let source = if craft.height < -90 {
            ground_explosion_sprite(craft.variant, phase)
        } else {
            explosion_sprite(craft.depth, phase)
        };
        if source == 0 {
            craft.active = true;
            craft.depth = craft.depth.wrapping_sub(1);
            craft.movement = self.falling_movement;
            self.initialize_falling(i, random, player_depth);
        } else {
            craft.requested_sprite = source;
            craft.animation_phase = phase.wrapping_add(1);
        }
    }

    fn update_craft(
        &mut self,
        i: usize,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        sound: &mut Foreground<'_>,
    ) -> Result<Option<EnemyEffects>, Error> {
        let mut effects = EnemyEffects::default();
        if !self.crafts[i].active {
            return Ok(Some(effects));
        }
        if self.crafts[i].movement == 251 {
            self.falling_explosion(i, random, player[0]);
            if self.crafts[i].depth <= player[0] {
                self.crafts[i].active = false;
                // Recycling changes the movement before terminal dispatch.
                // Falling retirement retains the source and resets its phase.
                if self.crafts[i].movement == 251 {
                    self.crafts[i].requested_sprite = 0;
                } else {
                    self.crafts[i].animation_phase = 0;
                }
            }
            return Ok(Some(effects));
        }
        if let Some(animation) = self.craft_animations[i] {
            let source = explosion_sprite(self.crafts[i].depth, animation);
            if source == 0 {
                self.crafts[i].active = false;
            } else {
                self.crafts[i].requested_sprite = source;
                self.craft_animations[i] = Some(animation.wrapping_add(1));
                self.crafts[i].animation_phase = animation.wrapping_add(1);
            }
            if self.crafts[i].depth <= player[0] {
                self.crafts[i].active = false;
                self.crafts[i].requested_sprite = 0;
            }
            return Ok(Some(effects));
        }
        let Some(delta) = self.motion(i, random, player, context, &mut effects, sound)? else {
            return Ok(None);
        };
        Ok(Some(self.finish_craft(
            i,
            random,
            player,
            context,
            (delta, effects),
            sound,
        )))
    }

    fn finish_craft(
        &mut self,
        i: usize,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        motion: ([i16; 3], EnemyEffects),
        sound: &mut Foreground<'_>,
    ) -> EnemyEffects {
        let (delta, mut effects) = motion;
        match self.crafts[i].variant {
            0 => {
                self.crafts[i].requested_sprite =
                    craft_sprite(self.crafts[i].depth, self.crafts[i].phase)
            }
            2 => {
                if self.crafts[i].movement == 251 {
                    self.falling_explosion(i, random, player[0]);
                } else {
                    self.crafts[i].requested_sprite =
                        falling_sprite(self.crafts[i].animation_phase);
                }
            }
            3 | 4 => self.crafts[i].requested_sprite = transition_sprite(self.crafts[i].depth),
            _ => {}
        }
        let craft = self.crafts[i];
        if matches!(craft.variant, 0 | 1 | 5 | 6) {
            let shadow = &mut self.shadows[i];
            shadow.depth = craft.depth;
            shadow.lateral = craft.lateral;
            shadow.requested_sprite = if craft.depth < 0 {
                36
            } else {
                shadow_sprite(craft.depth)
            };
            shadow.active = !context.alternate_shadows;
        }
        let firing = random.next_word() & 1 != 0;
        if firing && !self.bullet.active && !matches!(craft.variant, 2..=4) {
            {
                self.bullet_birth = self.bullet_birth.wrapping_add(1);
            }
            sound::request(sound, 0x2c9b, Effect::Tone(Tone::Third));
            self.bullet.depth = craft.depth;
            self.bullet.lateral = craft.lateral;
            self.bullet.height = craft.height;
            self.bullet.movement = 253;
            self.bullet.active = true;
        }
        if matches!(craft.variant, 0 | 1 | 5 | 6) {
            let (mask, bias) = if craft.variant < 2 { (63, 32) } else { (7, 3) };
            self.target[0] = player[1].wrapping_add((random.next_word() & mask) as i16 - bias);
            self.target[1] = player[2].wrapping_add((random.next_word() & mask) as i16 - bias);
        }
        if craft.depth < 85 {
            effects.distance_hint = Some((craft.depth >> 1).wrapping_add(2400));
        }
        let craft = &mut self.crafts[i];
        craft.depth = craft.depth.wrapping_add(delta[0]);
        craft.lateral = craft.lateral.wrapping_add(delta[1]);
        craft.height = craft.height.wrapping_add(delta[2]);
        if craft.depth <= player[0] {
            craft.active = false;
            craft.animation_phase = 0;
            if craft.variant < 2 {
                self.shadows[i].active = false;
            }
        }
        effects
    }

    fn motion(
        &mut self,
        i: usize,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        effects: &mut EnemyEffects,
        sound: &mut Foreground<'_>,
    ) -> Result<Option<[i16; 3]>, Error> {
        let movement = self.crafts[i].movement;
        let old_depth = self.crafts[i].depth;
        let primary = context.speeds[1];
        let mut delta = [-primary, 0, 0];
        let seek = |craft: EnemySnapshot| {
            [
                player[1]
                    .wrapping_sub(craft.lateral)
                    .wrapping_shr(1)
                    .clamp(-30, 30),
                player[2]
                    .wrapping_sub(craft.height)
                    .wrapping_shr(1)
                    .clamp(-30, 30),
            ]
        };
        match movement {
            4 | 5 | 19 | 20 => {
                let steering = seek(self.crafts[i]);
                delta[1..].copy_from_slice(&steering);
                if movement == 5 {
                    delta[0] = -context.speeds[2];
                }
            }
            6 | 7 | 17 => {
                if movement == 7 {
                    delta[0] = -context.speeds[2];
                }
            }
            8..=16 => {
                let near_seeker = matches!(movement, 9 | 11 | 13..=16);
                if near_seeker && old_depth < 240 {
                    delta[1..].copy_from_slice(&seek(self.crafts[i]));
                } else {
                    let lateral = matches!(movement, 8..=11 | 14 | 15);
                    let vertical = matches!(movement, 8 | 9 | 12..=14 | 16);
                    let steering = self.orbit_delta(i, player, lateral, vertical);
                    delta[1..].copy_from_slice(&steering);
                    if matches!(movement, 9 | 11 | 13) {
                        delta[0] = (-primary) >> 2;
                    }
                }
            }
            18 => {
                if old_depth < 240 {
                    delta[1..].copy_from_slice(&seek(self.crafts[i]));
                }
            }
            21..=24 | 29..=30 => {
                delta = [0; 3];
                let step = match movement {
                    21 => 8,
                    22 => 4,
                    29 => 17,
                    30 => 6,
                    _ => 0,
                };
                if step != 0 {
                    self.follow_center(player, step);
                }
                let craft = &mut self.crafts[i];
                let (coords, source) = scripted_sweep(craft.animation_phase);
                [craft.depth, craft.lateral, craft.height] = coords;
                if step != 0 {
                    craft.lateral = craft.lateral.wrapping_add(self.orbit_center[0]);
                    craft.height = craft.height.wrapping_add(self.orbit_center[1]);
                }
                if matches!(movement, 22 | 24 | 30) {
                    craft.lateral = craft.lateral.wrapping_add((2 * i as i16 - 5) << 5);
                }
                craft.requested_sprite = source;
                craft.animation_phase = craft.animation_phase.wrapping_add(1);
            }
            25..=26 => {
                delta = [0; 3];
                let craft = &mut self.crafts[i];
                craft.animation_phase = craft.animation_phase.wrapping_add(4) & 31;
                let mut drop = if movement == 25 { -20 } else { -13 };
                let mut slide = if movement == 25 { 15 } else { 10 };
                if craft.animation_phase & 2 != 0 {
                    slide = -slide;
                }
                if craft.animation_phase & 1 != 0 {
                    drop -= drop >> 2;
                    slide -= slide >> 2;
                }
                craft.height = craft.height.wrapping_add(drop);
                craft.lateral = craft.lateral.wrapping_add(slide);
                if craft.height < -100 {
                    sound::request(sound, 0x39c9, Effect::Ramp(Ramp::Third));
                    craft.movement = 251;
                    craft.animation_phase = 0;
                }
            }
            27..=28 => {
                delta = [0; 3];
                let craft = &mut self.crafts[i];
                if craft.animation_phase == 0 {
                    craft.lateral = 0;
                    craft.height = context.horizon;
                    craft.depth = craft.depth.wrapping_sub(context.speeds[0]);
                    if craft.depth < 50 {
                        sound::request(sound, 0x42bc, Effect::Tone(Tone::First));
                        return Ok(None);
                    }
                }
                self.transition_motion_tail(i, context, effects);
            }
            31..=33 => {
                delta = [0; 3];
                self.boss_formation(i, player, context);
            }
            34 => {
                delta = [0; 3];
                self.patrol(i, random, player, context)?;
            }
            _ => {}
        }
        Ok(Some(delta))
    }

    fn transition_motion_tail(
        &mut self,
        i: usize,
        context: EnemyContext,
        effects: &mut EnemyEffects,
    ) {
        let craft = &mut self.crafts[i];
        if craft.animation_phase == 30 {
            effects.points = 3000;
            craft.lateral = 0;
            craft.height = context.horizon;
            craft.animation_phase = 31;
        }
        if (1..30).contains(&craft.animation_phase) {
            craft.lateral = 0;
            craft.height = context.horizon;
            craft.animation_phase += 1;
        }
        if craft.animation_phase == 31 {
            craft.lateral = 0;
            craft.height = context.horizon;
            craft.depth = craft.depth.wrapping_add(context.speeds[0]);
            if craft.depth > 1000 {
                craft.depth = -50;
            }
        }
        if craft.animation_phase == 1 {
            effects.points = effects.points.wrapping_add(3000);
        }
    }

    fn boss_formation(&mut self, i: usize, player: [i16; 3], context: EnemyContext) {
        let movement = self.crafts[i].movement;
        self.follow_center(player, if movement == 33 { 6 } else { 15 });
        let craft = &mut self.crafts[i];
        let phase = craft.animation_phase;
        if phase == 0 || (phase == 255 && movement != 33) || (phase == 81 && movement == 33) {
            craft.requested_sprite = boss_sprite(craft.depth, craft.phase);
            if phase != 0 {
                craft.depth = craft.depth.wrapping_add(context.speeds[1]);
                if craft.depth > 1000 {
                    craft.depth = -50;
                }
                return;
            }
        } else {
            craft.requested_sprite = boss_animation_sprite(phase);
        }
        let offsets = if movement == 31 {
            match i {
                0 => [400, 20],
                1 => [-400, 20],
                2 => [0, 200],
                _ => [0, 0],
            }
        } else {
            match i {
                0 => [400, 20],
                1 => [-400, 20],
                2 => [400, 200],
                3 => [-400, 200],
                _ => [0, 0],
            }
        };
        if i < (if movement == 31 { 3 } else { 4 }) {
            let mut position = offsets;
            if movement == 33 && (1..41).contains(&phase) {
                let t = i16::from(phase);
                match i {
                    0 => position[1] += 5 * t,
                    1 => position[0] += 20 * t,
                    2 => position[0] -= 20 * t,
                    3 => position[1] -= 5 * t,
                    _ => {}
                }
            } else if movement == 33 && (41..81).contains(&phase) {
                let t = i16::from(phase);
                match i {
                    0 => position[1] = 420 - 5 * t,
                    1 => position[0] += 1600 - 20 * t,
                    2 => position[0] += -1600 + 20 * t,
                    3 => position[1] += -400 + 5 * t,
                    _ => {}
                }
            }
            craft.lateral = self.orbit_center[0].wrapping_add(position[0]);
            craft.height = self.orbit_center[1].wrapping_add(position[1]);
        }
        if phase == 0 {
            craft.depth = craft.depth.wrapping_sub(context.speeds[1]);
            if craft.depth < 200 {
                craft.animation_phase = 1;
            }
        } else {
            craft.animation_phase = phase.wrapping_add(1);
            if movement != 33 && usize::from(phase) == 120 + 4 * i {
                craft.animation_phase = 255;
            }
        }
    }

    fn patrol(
        &mut self,
        i: usize,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
    ) -> Result<(), Error> {
        self.follow_center(player, 6);
        let craft = &mut self.crafts[i];
        let sources = context
            .patrol_sources
            .unwrap_or_else(|| patrol_sources(context.difficulty));
        if craft.requested_sprite == craft.resolved_sprite
            && sources
                .iter()
                .all(|source| *source == craft.resolved_sprite)
        {
            return Err(Error::InvalidData(format!(
                "original patrol source selection stalls: all choices equal resolved source {}",
                craft.resolved_sprite
            )));
        }
        while craft.requested_sprite == craft.resolved_sprite {
            let choice = random.next_word() & 3;
            if choice != 3 {
                craft.requested_sprite = sources[choice as usize];
            }
        }
        let slot = i as i16;
        match craft.animation_phase {
            0 => {
                craft.lateral = craft.lateral.wrapping_add(70);
                if craft.lateral > self.orbit_center[0].wrapping_add(400) {
                    craft.animation_phase = 1;
                }
            }
            1 => {
                craft.height = craft.height.wrapping_add(10);
                if craft.height > self.orbit_center[1].wrapping_add((slot + 1) * 100) {
                    craft.animation_phase = 2;
                }
            }
            2 => {
                craft.lateral = craft.lateral.wrapping_sub(50);
                if craft.lateral < self.orbit_center[0].wrapping_add(slot * 10 - 400) {
                    craft.animation_phase = 3;
                }
            }
            3 => {
                craft.height = craft.height.wrapping_sub(10);
                if craft.height < self.orbit_center[1].wrapping_add(slot * 80) {
                    craft.animation_phase = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn follow_center(&mut self, player: [i16; 3], step: i16) {
        for (center, coordinate) in self.orbit_center.iter_mut().zip(player[1..].iter()) {
            *center = center.wrapping_add(coordinate.wrapping_sub(*center).clamp(-step, step));
        }
    }

    fn orbit_delta(
        &mut self,
        i: usize,
        player: [i16; 3],
        lateral_motion: bool,
        vertical_motion: bool,
    ) -> [i16; 2] {
        self.follow_center(player, 9);
        let craft = &mut self.crafts[i];
        let phase = ((craft.depth & 1023) >> 4) as u8;
        // Lateral changes before companion copying and firing. Height is a delta.
        if lateral_motion {
            craft.lateral = self.orbit_center[0].wrapping_add(orbit_wave(phase) << 4);
        }
        let height_delta = if vertical_motion {
            self.orbit_center[1]
                .wrapping_add(orbit_wave(phase.wrapping_add(16)) << 4)
                .clamp(-100, 250)
                .wrapping_sub(craft.height)
        } else {
            0
        };
        [0, height_delta]
    }

    pub(crate) fn deactivate_projectile(&mut self) {
        self.bullet.active = false;
    }

    /// Player contact recycles a falling enemy; weapon hits use an ordinary explosion.
    #[cfg(test)]
    pub(crate) fn explode_craft(&mut self, slot: usize) -> Result<(), Error> {
        self.explode_craft_with_sound(slot, &mut |_| {})
    }

    pub(crate) fn explode_craft_with_sound(
        &mut self,
        slot: usize,
        sound: &mut Foreground<'_>,
    ) -> Result<(), Error> {
        let i = self.craft_index(slot)?;
        if self.crafts[i].variant == 2 {
            sound::request(sound, 0x39c9, Effect::Ramp(Ramp::Third));
            self.crafts[i].movement = 251;
            self.crafts[i].animation_phase = 0;
            self.craft_animations[i] = None;
            Ok(())
        } else {
            self.hit_by_weapon_with_sound(slot, sound)
        }
    }

    #[cfg(test)]
    pub(crate) fn hit_by_weapon(&mut self, slot: usize) -> Result<(), Error> {
        self.hit_by_weapon_with_sound(slot, &mut |_| {})
    }

    pub(crate) fn hit_by_weapon_with_sound(
        &mut self,
        slot: usize,
        sound: &mut Foreground<'_>,
    ) -> Result<(), Error> {
        let i = self.craft_index(slot)?;
        sound::request(sound, 0x3955, Effect::Ramp(Ramp::Third));
        self.craft_animations[i] = Some(0);
        self.crafts[i].movement = 3;
        self.crafts[i].animation_phase = 0;
        self.shadows[i].active = false;
        Ok(())
    }

    fn craft_index(&self, slot: usize) -> Result<usize, Error> {
        slot.checked_sub(5)
            .filter(|i| *i < 5)
            .ok_or_else(|| Error::InvalidData(format!("invalid primary explosion slot {slot}")))
    }

    pub(crate) fn clear_primary(&mut self) {
        for craft in &mut self.crafts {
            craft.active = false;
        }
    }

    pub(crate) fn restore_snapshots(
        snapshots: [EnemySnapshot; 11],
        assets: &GraphicsAssets,
    ) -> Result<Self, Error> {
        let mut enemy = Self::new(assets)?;
        enemy.reset_for_game(assets)?;
        for (i, record) in snapshots.iter().enumerate() {
            if record.slot != 4 + i {
                return Err(Error::InvalidData(
                    "retained enemy slots must be ordered 4 through 14".into(),
                ));
            }
        }
        enemy.bullet = snapshots[0];
        enemy.crafts.copy_from_slice(&snapshots[1..6]);
        enemy.shadows.copy_from_slice(&snapshots[6..]);
        Ok(enemy)
    }

    pub(crate) fn reset_for_game(&mut self, assets: &GraphicsAssets) -> Result<(), Error> {
        self.bullet.depth = 14;
        self.bullet.lateral = 0;
        self.bullet.height = -100;
        self.bullet.movement = 253;
        self.bullet.active = false;
        self.bullet.requested_sprite = 22;
        self.bullet.project(assets, 106, 0)?;
        for slot in self.crafts.iter_mut().chain(self.shadows.iter_mut()) {
            slot.active = false;
            slot.movement = 0;
        }
        self.craft_animations = [None; 5];
        self.orbit_center = [0, -100];
        self.target = [0, 0];
        self.falling_count = 0;
        self.falling_movement = 0;
        Ok(())
    }

    /// Collision uses preceding resolved descriptors. Phase gates candidates;
    /// damage grace is checked by the caller without suppressing target effects.
    pub(crate) fn player_collisions(
        &self,
        assets: &GraphicsAssets,
        player: [i16; 3],
        player_sprite: usize,
        collidable: bool,
    ) -> Result<[Option<EnemyCollision>; 6], Error> {
        let mut collisions = [None; 6];
        if !collidable {
            return Ok(collisions);
        }
        let player_source = assets.sprites.get(player_sprite).ok_or_else(|| {
            Error::InvalidData(format!("missing player collision sprite {player_sprite}"))
        })?;
        for (index, slot) in std::iter::once(&self.bullet)
            .chain(self.crafts.iter())
            .enumerate()
        {
            if !slot.active
                || (index > 0
                    && (self.craft_animations[index - 1].is_some()
                        || slot.movement == 251
                        || !matches!(slot.variant, 0..=2 | 5 | 6)))
            {
                continue;
            }
            let source = assets.sprites.get(slot.resolved_sprite).ok_or_else(|| {
                Error::InvalidData(format!(
                    "missing enemy collision sprite {}",
                    slot.resolved_sprite
                ))
            })?;
            let overlaps = |position: i16, extent: i16, target: i16, target_extent: i16| {
                position.wrapping_add(extent) >= target
                    && position <= target.wrapping_add(target_extent)
            };
            if overlaps(
                player[0],
                player_source.descriptor[2],
                slot.depth,
                source.descriptor[2],
            ) && overlaps(
                player[1],
                player_source.descriptor[1],
                slot.lateral,
                source.descriptor[1],
            ) && player[2].wrapping_add(source.descriptor[0]) >= slot.height
                && player[2] <= slot.height.wrapping_add(source.descriptor[0])
            {
                collisions[index] = Some(if index == 0 {
                    EnemyCollision::Projectile
                } else {
                    EnemyCollision::Craft(slot.slot)
                });
            }
        }
        Ok(collisions)
    }

    /// Near projection includes inactive slots with depth strictly below 70.
    pub(crate) fn project(
        &mut self,
        assets: &GraphicsAssets,
        full_projection: bool,
        vertical_origin: i32,
        camera_height: i16,
    ) -> Result<(), Error> {
        let births: [u64; 11] = std::array::from_fn(|i| {
            if i == 0 {
                self.bullet_birth
            } else {
                self.births[(i - 1) % 5]
            }
        });
        for slot in std::iter::once(&mut self.bullet)
            .chain(self.crafts.iter_mut())
            .chain(self.shadows.iter_mut())
        {
            if full_projection || slot.depth < 70 {
                slot.project(assets, vertical_origin, camera_height)?;
                {
                    self.projected_births[slot.slot - 4] = births[slot.slot - 4];
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn advance_half_step(
        &mut self,
        assets: &GraphicsAssets,
        random: &mut Random,
        full_projection: bool,
    ) -> Result<(), Error> {
        self.update_half_step(random, [14, 0, -100]);
        self.project(assets, full_projection, 106, 0)
    }

    /// Subtype-zero shadows sort ahead of all ordinary world objects.
    #[cfg(test)]
    pub(crate) fn shadow_draws(&self) -> impl Iterator<Item = Draw> + '_ {
        self.shadows
            .iter()
            .filter(|shadow| shadow.active)
            .map(|shadow| shadow.draw())
    }

    pub(crate) fn snapshots(&self) -> [EnemySnapshot; 11] {
        std::array::from_fn(|i| match i {
            0 => self.bullet,
            1..=5 => self.crafts[i - 1],
            _ => self.shadows[i - 6],
        })
    }

    pub(crate) fn target(&self) -> [i16; 2] {
        self.target
    }

    pub(crate) fn active(&self) -> bool {
        self.crafts.iter().any(|craft| craft.active)
    }
}

fn projectile_sprite(depth: i16) -> usize {
    match size(depth) {
        0..=3 => 64,
        4..=6 => 63,
        7..=9 => 65,
        10..=16 => 66,
        17..=20 => 67,
        21..=25 => 68,
        _ => 69,
    }
}
fn falling_sprite(phase: u8) -> usize {
    let quarter = (phase >> 2) & 3;
    124 + usize::from((phase >> 1) & 1) * 3
        + usize::from(if quarter <= 2 { quarter } else { 4 - quarter })
}
fn ground_explosion_sprite(variant: u8, phase: u8) -> usize {
    let source = if variant == 0 { 117 } else { 118 } + usize::from(phase >> 1);
    if source <= 123 { source } else { 0 }
}
fn transition_sprite(depth: i16) -> usize {
    match size(depth) {
        0..=3 => 160,
        4..=8 => 157 + size(depth) as usize,
        9..=11 => 169,
        12..=14 => 170,
        _ => 171,
    }
}
fn boss_sprite(depth: i16, phase: u8) -> usize {
    let q = size(depth);
    match q {
        0..=2 => {
            if phase >= 2 {
                141
            } else {
                134
            }
        }
        3..=5 => {
            if phase >= 2 {
                142
            } else {
                135
            }
        }
        6..=14 => 134 + (q / 3) as usize,
        15..=20 => 139,
        _ => 140,
    }
}
fn boss_animation_sprite(phase: u8) -> usize {
    let p = phase & 7;
    137 + if p == 0 || p == 7 {
        0
    } else if p <= 2 || p >= 5 {
        1
    } else {
        2
    }
}
fn patrol_sources(difficulty: u16) -> [usize; 3] {
    let start = match progression::visual_difficulty(difficulty) {
        0 => 187,
        family => 175 + 3 * usize::from((family - 1).min(3)),
    };
    [start, start + 1, start + 2]
}

/// Resource-defined sweep reconstructed as affine segments. Division truncates
/// toward zero, preserving the original quantized coordinates without floats.
fn scripted_sweep(phase: u8) -> ([i16; 3], usize) {
    if phase >= 77 {
        return ([-50, 0, 0], 100);
    }
    let (start, axes): (u8, [[i32; 3]; 3]) = match phase {
        0..=11 => (0, [[500, 0, 1], [-2600, 50, 1], [16, 0, 1]]),
        12..=25 => (12, [[500, -2, 1], [-2000, 48, 1], [16, 2, 1]]),
        26..=27 => (26, [[472, -2, 1], [-1328, 55, 1], [44, 2, 1]]),
        28..=32 => (28, [[470, -2, 1], [-2436, 71, 2], [48, 29, 1]]),
        33..=40 => (33, [[460, -1, 1], [-2080, 53, 2], [388, 73, 2]]),
        41..=49 => (41, [[1812, -81, 4], [-5796, 333, 7], [972, -17, 2]]),
        50..=54 => (50, [[1084, -45, 4], [-400, 40, 1], [1230, -38, 3]]),
        55..=58 => (55, [[215, -11, 1], [-200, 10, 1], [1040, -38, 3]]),
        59..=63 => (59, [[513, -23, 3], [-160, 10, 1], [888, -44, 3]]),
        64..=65 => (64, [[133, -8, 1], [-106, 10, 1], [223, -15, 1]]),
        66..=68 => (66, [[236, -11, 2], [-84, 12, 1], [194, -20, 1]]),
        69..=70 => (69, [[101, -10, 1], [-48, 24, 1], [135, -39, 1]]),
        71..=72 => (71, [[82, -17, 1], [-18, 12, 1], [72, -48, 1]]),
        _ => (73, [[47, -10, 1], [0, 0, 1], [0, 0, 1]]),
    };
    let step = i32::from(phase - start);
    let coordinates = axes.map(|[base, delta, divisor]| ((base + delta * step) / divisor) as i16);
    let source = match phase {
        0..=28 => 90,
        29..=33 => 91,
        34..=41 => 92,
        42..=55 => 93,
        56..=61 => 94,
        62..=66 => 95,
        67..=69 => 96,
        70 => 97,
        71 => 98,
        72..=73 => 99,
        _ => usize::from(phase) + 26,
    };
    (coordinates, source)
}

/// Signed integer wave with 64 phases; mirror the quarter-wave's small stepped
/// amplitudes rather than introducing floating-point rounding into movement.
fn orbit_wave(phase: u8) -> i16 {
    let phase = phase & 63;
    let half = phase & 31;
    let quarter = if half <= 16 { half } else { 32 - half };
    let amplitude = match quarter {
        0..=8 => i16::from(quarter),
        9..=10 => 9,
        11..=12 => 10,
        _ => 11,
    };
    if phase < 32 { amplitude } else { -amplitude }
}

fn explosion_sprite(depth: i16, animation: u8) -> usize {
    let sources: &[usize] = match depth >> 5 {
        0..=2 => &[69, 61, 69, 67, 65, 64],
        3..=4 => &[69, 68, 67, 66, 65, 64],
        5..=7 => &[68, 67, 66, 65, 64],
        8..=11 => &[67, 66, 65, 64],
        _ => &[66, 65, 63, 64],
    };
    sources
        .get(usize::from(animation >> 1))
        .copied()
        .unwrap_or(0)
}

fn size(depth: i16) -> i32 {
    // Configured departure routines write exactly -50, whose original lookup
    // falls in the startup-zeroed gap before the size table.
    if depth == -50 {
        0
    } else {
        2047 / (i32::from(depth) + 64)
    }
}

fn craft_sprite(depth: i16, phase: u8) -> usize {
    let shifted = phase >= 2;
    match size(depth) {
        0..=1 => {
            if shifted {
                34
            } else {
                25
            }
        }
        2..=3 => {
            if shifted {
                35
            } else {
                26
            }
        }
        4..=5 => 27,
        6..=8 => 28,
        9..=11 => 29,
        12..=15 => 30,
        16..=20 => 31,
        21..=25 => 32,
        26..=31 => 33,
        _ => 33,
    }
}

fn shadow_sprite(depth: i16) -> usize {
    match size(depth) {
        0..=1 => 36,
        2..=3 => 37,
        4..=5 => 38,
        6..=8 => 39,
        9..=11 => 40,
        12..=15 => 41,
        16..=20 => 42,
        21..=25 => 43,
        _ => 44,
    }
}

#[cfg(test)]
mod tests;
