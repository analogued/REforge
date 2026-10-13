// Copyright (c) 2026 analogued. Project license has not been selected.
//! Opening wave movement, ground shadows, seeking projectiles and explosions.

use super::Random;
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
    pub difficulty: i16,
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

#[derive(Debug)]
pub(crate) struct Enemy {
    bullet: EnemySnapshot,
    crafts: [EnemySnapshot; 5],
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
                    if i > 2 || i as i16 > context.difficulty {
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

    pub(crate) fn update_with_sound(
        &mut self,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        sound: &mut Foreground<'_>,
    ) -> Result<EnemyEffects, Error> {
        let mut effects = EnemyEffects::default();
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
        for i in 0..5 {
            effects.merge(self.update_craft(i, random, player, context, sound)?);
        }
        for i in 0..5 {
            if !self.crafts[i].active {
                self.shadows[i].active = false;
            }
        }
        Ok(effects)
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
    ) -> Result<EnemyEffects, Error> {
        let mut effects = EnemyEffects::default();
        if !self.crafts[i].active {
            return Ok(effects);
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
            return Ok(effects);
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
            return Ok(effects);
        }
        let delta = self.motion(i, random, player, context, &mut effects, sound)?;
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
        Ok(effects)
    }

    fn motion(
        &mut self,
        i: usize,
        random: &mut Random,
        player: [i16; 3],
        context: EnemyContext,
        effects: &mut EnemyEffects,
        sound: &mut Foreground<'_>,
    ) -> Result<[i16; 3], Error> {
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
                        sound(GameplayAction::IntroSample {
                            site: 0x42bf,
                            request: SampleRequest {
                                resource: Resource::Data8,
                                offset: 0,
                                count: 9723,
                                reverse: false,
                            },
                        });
                        craft.animation_phase = 1;
                        craft.depth = 50;
                    }
                }
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
        Ok(delta)
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
            .or_else(|| patrol_sources(context.difficulty))
            .ok_or_else(|| {
                Error::InvalidData(format!(
                    "patrol difficulty {} requires the original adjacent source resource",
                    context.difficulty
                ))
            })?;
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
                        || slot.variant >= 3))
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
        for slot in std::iter::once(&mut self.bullet)
            .chain(self.crafts.iter_mut())
            .chain(self.shadows.iter_mut())
        {
            if full_projection || slot.depth < 70 {
                slot.project(assets, vertical_origin, camera_height)?;
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
fn patrol_sources(difficulty: i16) -> Option<[usize; 3]> {
    let start = match difficulty {
        0 => 187,
        1..=9 => 175 + 3 * usize::try_from((difficulty - 1).min(3)).unwrap(),
        // The next two original triplets lie in zero padding. An equal resolved
        // source makes the original retry loop stall rather than select artwork.
        10..=11 => return Some([0; 3]),
        _ => return None,
    };
    Some([start, start + 1, start + 2])
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
mod tests {
    use super::*;
    use crate::assets::Sprite;

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![],
            alternate: vec![],
            sprites: (0..190)
                .map(|_| Sprite {
                    width_bytes: 4,
                    height: 7,
                    descriptor: [1, 1, 1, 4, 7, 2, 5],
                    color: vec![0; 28],
                    mask: vec![255; 28],
                })
                .collect(),
        }
    }

    #[test]
    fn seeking_precedes_target_update_and_shadow_observes_old_depth() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 800;
        enemy.crafts[0].lateral = 174;
        enemy.crafts[0].height = 91;
        enemy.crafts[0].phase = 3;
        enemy.shadows[0].active = true;
        enemy.bullet.active = true;
        enemy.bullet.depth = 900;
        enemy.bullet.lateral = 174;
        enemy.bullet.height = 91;
        enemy.target = [10, -100];
        let mut random = Random::default();
        enemy
            .advance_half_step(&assets(), &mut random, true)
            .unwrap();
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (872, 94, 11)
        );
        assert_eq!(enemy.crafts[0].depth, 778);
        assert_eq!(enemy.shadows[0].depth, 800);
        assert_eq!(enemy.crafts[0].requested_sprite, 35);
        assert_eq!(enemy.crafts[0].resolved_sprite, 35);
        assert_eq!(random.next_word(), 0x4293);
    }

    #[test]
    fn second_half_firing_retains_the_previous_projected_source() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 800;
        let mut random = Random::default();
        enemy
            .advance_half_step(&assets(), &mut random, true)
            .unwrap();
        assert!(!enemy.bullet.active);
        enemy
            .advance_half_step(&assets(), &mut random, false)
            .unwrap();
        assert!(enemy.bullet.active);
        assert_eq!(enemy.bullet.depth, 778);
        assert_eq!(enemy.bullet.resolved_sprite, 22);
        assert_eq!(enemy.bullet.rect.x_bytes, 128);
    }
    #[test]
    fn retirement_precedes_collision_and_preserves_craft_source() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 42;
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 36;
        enemy.shadows[0].active = true;
        enemy.update_half_step(&mut Random::default(), [14, 0, -100]);
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.requested_sprite, 0);
        assert!(!enemy.crafts[0].active);
        assert!(!enemy.shadows[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, 31);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            [None; 6]
        );
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 0);
        assert_eq!(enemy.crafts[0].resolved_sprite, 31);
        assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
    }

    #[test]
    fn retired_projectile_can_be_refired_in_the_same_slot_pass() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 42;
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 100;
        enemy.crafts[0].lateral = 200;
        enemy.crafts[0].height = 50;
        let mut random = Random::default();
        random.next_word();
        random.next_word();
        random.next_word(); // Next word is odd, so the craft fires.
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(enemy.bullet.active);
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (100, 200, 50)
        );
        assert_eq!(enemy.bullet.requested_sprite, 0);
        assert_eq!(enemy.crafts[0].depth, 78);
    }

    #[test]
    fn near_projection_has_a_strict_depth_gate_and_resolves_inactive_sources() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.requested_sprite = 65;
        enemy.bullet.depth = 70;
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 22);
        enemy.bullet.depth = 69;
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.bullet.resolved_sprite, 65);
        assert_eq!(enemy.bullet.rect.x_bytes, 128);
    }

    #[test]
    fn collision_uses_previous_source_and_inclusive_target_height_extent() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        assets.sprites[64].descriptor[..3].copy_from_slice(&[1, 1, 1]);
        assets.sprites[65].descriptor[..3].copy_from_slice(&[100, 100, 100]);
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 29;
        enemy.bullet.lateral = 128;
        enemy.bullet.height = -99;
        enemy.bullet.resolved_sprite = 64;
        enemy.bullet.requested_sprite = 65;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                None,
                None,
                None,
                None,
                None
            ]
        );
        assert!(enemy.bullet.active);
        enemy.bullet.depth = 30;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
        enemy.bullet.depth = 29;
        enemy.bullet.height = -98;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
        enemy.bullet.height = -101;
        enemy.bullet.lateral = -1;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                None,
                None,
                None,
                None,
                None
            ]
        );
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, false)
                .unwrap(),
            [None; 6]
        );
    }

    #[test]
    fn primary_spawn_commands_consume_only_their_required_random_words() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 123, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (1600, 0, 0)
        );
        enemy.spawn_command(0x91, &mut random, 123, 1).unwrap();
        assert_eq!(enemy.crafts[0].lateral, (0x5630 & 1023) - 512 - 246);
        assert_eq!(enemy.crafts[0].height, (0xbe90_u16 & 255) as i16 - 50);
        assert_eq!(random.next_word(), 0x6338);
    }
    #[test]
    fn seeking_craft_fires_and_copies_shadow_before_steering_deltas() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
        enemy.crafts[0].lateral = 71;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (41, -30));
        assert_eq!((enemy.bullet.lateral, enemy.bullet.height), (71, 0));
        assert_eq!(enemy.bullet.depth, 1600);
        assert_eq!(enemy.shadows[0].lateral, 71);
        for _ in 0..7 {
            enemy.update_half_step(&mut random, [14, 0, -100]);
        }
        assert_eq!(enemy.crafts[0].height, -100);
    }

    #[test]
    fn craft_explosion_retains_source_then_animates_without_movement_or_random() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].depth = 16;
        enemy.crafts[0].height = -100;
        enemy.crafts[0].requested_sprite = 33;
        enemy.crafts[0].resolved_sprite = 33;
        enemy.shadows[0].active = true;
        enemy.explode_craft(5).unwrap();
        assert_eq!(enemy.crafts[0].requested_sprite, 33);
        assert!(!enemy.shadows[0].active);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 0, true)
                .unwrap(),
            [None; 6]
        );
        let mut random = Random::default();
        for source in [69, 69, 61, 61, 69, 69, 67, 67, 65, 65, 64, 64] {
            enemy.update_half_step(&mut random, [14, 0, -100]);
            assert!(enemy.crafts[0].active);
            assert_eq!(enemy.crafts[0].requested_sprite, source);
            assert_eq!((enemy.crafts[0].depth, enemy.crafts[0].height), (16, -100));
            assert_eq!(random.state(), [0; 3]);
        }
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(!enemy.crafts[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, 64);
        enemy.project(&assets, false, 106, 0).unwrap();
        assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
    }

    #[test]
    fn orbit_changes_lateral_before_firing_but_defers_height_until_after_firing() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 50, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        enemy.crafts[0].depth = 1024;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [9, -91]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (1002, 9, 85)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (1024, 9, 0)
        );
        assert_eq!(enemy.shadows[0].lateral, 9);
        assert!((18..=81).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
    }

    #[test]
    fn orbit_switches_to_seeking_only_below_the_old_depth_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 240;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (218, 185, -75)
        );
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (196, 155, -52)
        );
        assert_eq!(enemy.orbit_center, [9, -91]);
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        assert_eq!(enemy.orbit_center, [9, -91]);
    }

    #[test]
    fn recovered_player_coordinates_drive_seeking_targets_and_retirement() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
        enemy.update_half_step(&mut random, [28, 100, -28]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (30, -14));
        assert!((68..=131).contains(&enemy.target[0]));
        assert!((-60..=3).contains(&enemy.target[1]));
        enemy.crafts[0].active = false;
        enemy.bullet.active = true;
        enemy.bullet.depth = 56;
        enemy.update_half_step(&mut random, [28, 100, -28]);
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.depth, 28);
    }
    #[test]
    fn orbit_height_clamps_before_the_world_delta_is_applied() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
        enemy.orbit_center[1] = 200;
        enemy.crafts[0].depth = 1024;
        enemy.update_half_step(&mut random, [14, 0, 230]);
        assert_eq!(enemy.orbit_center[1], 209);
        assert_eq!(enemy.crafts[0].height, 250);
        enemy.orbit_center[1] = -100;
        enemy.crafts[0].depth = 512;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(enemy.crafts[0].height, -100);
    }
    #[test]
    fn lateral_orbit_preserves_height_and_does_not_seek_at_near_depth() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
        assert_eq!(random.state(), [0; 3]);
        enemy.crafts[0].depth = 128;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [9, -91]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (106, 137, 0)
        );
        enemy.update_half_step(&mut random, [14, 50, -28]);
        assert_eq!(enemy.orbit_center, [18, -82]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (84, 114, 0)
        );
    }

    #[test]
    fn lateral_orbit_firing_observes_the_new_lateral_and_preserved_height() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.orbit_center = [0, -28];
        enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 38;
        for _ in 0..3 {
            random.next_word();
        }
        enemy.update_half_step(&mut random, [14, 0, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (16, 32, 0)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (38, 32, 0)
        );
        assert_eq!((enemy.shadows[0].depth, enemy.shadows[0].lateral), (38, 32));
        assert!(enemy.crafts[0].active);
    }
    #[test]
    fn collision_candidates_preserve_slot_order_for_grace_gated_damage() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        for slot in [&mut enemy.bullet, &mut enemy.crafts[0]] {
            slot.active = true;
            slot.depth = 29;
            slot.lateral = 128;
            slot.height = -99;
        }
        enemy.bullet.resolved_sprite = 64;
        enemy.crafts[0].resolved_sprite = 33;
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                Some(EnemyCollision::Craft(5)),
                None,
                None,
                None,
                None
            ]
        );
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, false)
                .unwrap(),
            [None; 6]
        );
    }
    #[test]
    fn formation_spawn_staggers_depth_and_retains_cached_projection() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.crafts[1].resolved_sprite = 32;
        enemy.crafts[1].rect.y = 71;
        enemy.spawn_command(0x88, &mut random, 100, 2).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1600, 1500, 0, 0, 0]);
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, true, false, false, false]
        );
        assert_eq!(
            enemy.shadows.map(|c| c.active),
            [true, true, false, false, false]
        );
        assert_eq!(enemy.crafts[1].requested_sprite, 0);
        assert_eq!(enemy.crafts[1].movement, 8);
        assert_eq!(enemy.crafts[1].resolved_sprite, 32);
        assert_eq!(enemy.crafts[1].rect.y, 71);
        assert_eq!(
            enemy.snapshots().map(|c| c.slot),
            [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
        );
        enemy.spawn_command(0x88, &mut random, 100, 20).unwrap();
        assert_eq!(
            enemy.crafts.map(|c| c.depth),
            [1600, 1500, 1400, 1300, 1200]
        );
    }

    #[test]
    fn full_orbit_keeps_vertical_motion_below_the_seeking_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.orbit_center = [0, -28];
        enemy.spawn_command(0x88, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 128;
        enemy.update_half_step(&mut random, [14, 0, -28]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (106, 128, 100)
        );
    }

    #[test]
    fn formation_updates_shared_center_and_random_target_in_slot_order() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x88, &mut random, 0, 2).unwrap();
        enemy.crafts[0].depth = 1024;
        enemy.crafts[1].depth = 512;
        enemy.bullet.active = true;
        enemy.bullet.depth = 600;
        enemy.update_half_step(&mut random, [14, 50, -28]);
        for _ in 0..4 {
            expected.next_word();
        }
        let lateral = 50 + (expected.next_word() & 63) as i16 - 32;
        let height = -28 + (expected.next_word() & 63) as i16 - 32;
        assert_eq!(random.state(), expected.state());
        assert_eq!(enemy.target(), [lateral, height]);
        assert_eq!(enemy.orbit_center, [18, -82]);
        assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (9, 85));
        assert_eq!(
            (enemy.crafts[1].lateral, enemy.crafts[1].height),
            (18, -100)
        );
        assert_eq!(
            (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
            (9, 18)
        );
    }

    #[test]
    fn secondary_explosion_preserves_other_craft_and_parser_activity() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
        enemy.crafts[1].depth = 16;
        enemy.explode_craft(6).unwrap();
        assert_eq!(enemy.crafts[1].movement, 3);
        assert_eq!(enemy.crafts[0].movement, 4);
        assert!(enemy.shadows[0].active);
        assert!(!enemy.shadows[1].active);
        for _ in 0..13 {
            enemy.update_half_step(&mut random, [14, 0, -100]);
        }
        assert!(!enemy.crafts[1].active);
        assert!(enemy.crafts[0].active);
        assert!(enemy.active());
        assert_eq!(enemy.shadow_draws().count(), 1);
        enemy.crafts[0].active = false;
        assert!(!enemy.active());
        assert!(enemy.explode_craft(4).is_err());
        assert!(enemy.explode_craft(10).is_err());
    }
    #[test]
    fn later_craft_can_fire_into_shared_slot_before_its_steering_delta() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
        enemy.crafts[0].depth = 800;
        enemy.crafts[0].lateral = 200;
        enemy.crafts[0].height = 50;
        enemy.crafts[1].depth = 700;
        enemy.crafts[1].lateral = -40;
        enemy.crafts[1].height = -10;
        assert_eq!(expected.next_word() & 1, 0);
        expected.next_word();
        expected.next_word();
        assert_eq!(expected.next_word() & 1, 1);
        let lateral = (expected.next_word() & 63) as i16 - 32;
        let height = -100 + (expected.next_word() & 63) as i16 - 32;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(enemy.bullet.active);
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (700, -40, -10)
        );
        assert_eq!(
            (
                enemy.crafts[1].depth,
                enemy.crafts[1].lateral,
                enemy.crafts[1].height
            ),
            (678, -20, -40)
        );
        assert_eq!(enemy.bullet.requested_sprite, 22);
        assert_eq!(enemy.target(), [lateral, height]);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn all_primary_collision_candidates_identify_slots_and_skip_explosions() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        for slot in std::iter::once(&mut enemy.bullet).chain(enemy.crafts.iter_mut()) {
            slot.active = true;
            slot.depth = 29;
            slot.lateral = 128;
            slot.height = -99;
            slot.resolved_sprite = 33;
        }
        enemy.explode_craft(7).unwrap();
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [
                Some(EnemyCollision::Projectile),
                Some(EnemyCollision::Craft(5)),
                Some(EnemyCollision::Craft(6)),
                None,
                Some(EnemyCollision::Craft(8)),
                Some(EnemyCollision::Craft(9)),
            ]
        );
    }
    #[test]
    fn sweep_initializes_variant_one_but_only_the_first_primary_is_active() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.crafts[0].resolved_sprite = 33;
        enemy.crafts[0].rect.y = 51;
        enemy.spawn_command(0x97, &mut random, 100, 3).unwrap();
        assert_eq!(random.state(), [0; 3]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1000, 1000, 1000, 0, 0]);
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, false, false, false, false]
        );
        assert_eq!(
            enemy.shadows.map(|c| c.active),
            [true, false, false, false, false]
        );
        assert_eq!(enemy.crafts.map(|c| c.variant), [1, 1, 1, 0, 0]);
        assert_eq!(enemy.crafts[0].requested_sprite, 0);
        assert_eq!(enemy.crafts[0].resolved_sprite, 33);
        assert_eq!(enemy.crafts[0].rect.y, 51);
    }

    #[test]
    fn sweep_copies_absolute_coordinates_before_firing_and_updates_shared_target() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
        enemy.crafts[0].animation_phase = 33;
        enemy.crafts[0].depth = 800;
        for _ in 0..3 {
            random.next_word();
        }
        let mut expected = random;
        assert_eq!(expected.next_word() & 1, 1);
        let target = [
            50 + (expected.next_word() & 63) as i16 - 32,
            -28 + (expected.next_word() & 63) as i16 - 32,
        ];
        enemy.update_half_step(&mut random, [14, 50, -28]);
        let craft = enemy.crafts[0];
        assert_eq!(
            (craft.depth, craft.lateral, craft.height),
            (460, -1040, 194)
        );
        assert_eq!((craft.requested_sprite, craft.animation_phase), (91, 34));
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (460, -1040, 194)
        );
        assert_eq!(
            (enemy.shadows[0].depth, enemy.shadows[0].lateral),
            (460, -1040)
        );
        assert_eq!(enemy.shadows[0].requested_sprite, 37);
        assert_eq!(enemy.target(), target);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn sweep_terminal_coordinates_retire_before_collision_and_keep_the_requested_source() {
        let assets = assets();
        let mut enemy = Enemy::new(&assets).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
        enemy.crafts[0].animation_phase = 77;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(!enemy.active());
        assert!(!enemy.shadows[0].active);
        assert_eq!(enemy.crafts[0].depth, -50);
        assert_eq!(enemy.crafts[0].animation_phase, 0);
        assert_eq!(enemy.crafts[0].requested_sprite, 100);
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap(),
            [None; 6]
        );
    }

    #[test]
    fn sweep_collision_keeps_the_preceding_descriptor_until_projection() {
        let mut assets = assets();
        assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
        assets.sprites[90].descriptor[..3].copy_from_slice(&[40, 158, 15]);
        assets.sprites[91].descriptor[..3].copy_from_slice(&[100, 158, 15]);
        let mut enemy = Enemy::new(&assets).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x97, &mut random, 0, 1).unwrap();
        enemy.crafts[0].animation_phase = 29;
        enemy.crafts[0].resolved_sprite = 90;
        let player = [453, -1182, 120];
        enemy.update_half_step(&mut random, player);
        assert_eq!(enemy.crafts[0].requested_sprite, 91);
        assert_eq!(enemy.crafts[0].resolved_sprite, 90);
        assert_eq!(
            enemy.player_collisions(&assets, player, 13, true).unwrap(),
            [None; 6]
        );
        enemy.project(&assets, true, 106, 0).unwrap();
        assert_eq!(
            enemy.player_collisions(&assets, player, 13, true).unwrap(),
            [None, Some(EnemyCollision::Craft(5)), None, None, None, None]
        );
    }
    #[test]
    fn relative_sweep_initializes_all_primaries_with_staggered_row_phases() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x96, &mut random, 0, 5).unwrap();
        assert_eq!(enemy.crafts.map(|c| c.animation_phase), [0, 4, 8, 12, 16]);
        assert_eq!(enemy.crafts.map(|c| c.variant), [1; 5]);
        assert_eq!(enemy.crafts.map(|c| c.depth), [1000; 5]);
        assert!(enemy.crafts.iter().all(|c| c.active && c.movement == 22));
        assert!(enemy.shadows.iter().all(|c| c.active));
        assert_eq!(random.state(), [0; 3]);
    }

    #[test]
    fn relative_sweep_follows_center_per_craft_and_spaces_absolute_firing_coordinates() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x96, &mut random, 0, 2).unwrap();
        enemy.update_half_step(&mut random, [14, 100, -80]);
        assert_eq!(enemy.orbit_center, [8, -92]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (500, -2756, -80)
        );
        assert_eq!(
            (
                enemy.crafts[1].depth,
                enemy.crafts[1].lateral,
                enemy.crafts[1].height
            ),
            (500, -2488, -76)
        );
        assert_eq!(
            (
                enemy.crafts[0].animation_phase,
                enemy.crafts[1].animation_phase
            ),
            (1, 5)
        );
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height
            ),
            (500, -2488, -76)
        );
        assert_eq!(
            (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
            (-2756, -2488)
        );
        for _ in 0..4 {
            expected.next_word();
        }
        let target = [
            100 + (expected.next_word() & 63) as i16 - 32,
            -80 + (expected.next_word() & 63) as i16 - 32,
        ];
        assert_eq!(enemy.target(), target);
        assert_eq!(random.state(), expected.state());
    }

    #[test]
    fn negative_depth_projection_wraps_the_table_index_and_resolves_the_source() {
        let mut assets = assets();
        assets.sprites[1].descriptor[5..].fill(0);
        for (depth, lateral, height, origin, camera, expected) in [
            (-50, -160, -28, 107, 18, (38, 109, 3)),
            (-64, 128, 64, 100, 0, (41, 98, 0)),
            (-1024, 128, 64, 100, 0, (24, 132, 0)),
        ] {
            let mut slot = EnemySnapshot::new(4, 0, depth, height);
            slot.lateral = lateral;
            slot.active = true;
            slot.requested_sprite = 1;
            slot.project(&assets, origin, camera).unwrap();
            assert_eq!((slot.rect.x_bytes, slot.rect.y, slot.phase), expected);
            assert_eq!(slot.resolved_sprite, 1);
            assert_eq!((slot.rect.width_bytes, slot.rect.height), (4, 7));

            slot.active = false;
            slot.depth -= 28;
            slot.requested_sprite = 0;
            slot.project(&assets, 200, 100).unwrap();
            assert_eq!(slot.rect.x_bytes, 128);
            assert_eq!((slot.rect.y, slot.phase), (expected.1, expected.2));
            assert_eq!(slot.resolved_sprite, 0);
        }
    }

    #[test]
    fn randomized_forward_spawn_staggers_three_craft_and_preserves_far_coordinates() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        let mut coordinates = [[0; 2]; 3];
        for pair in &mut coordinates {
            *pair = [
                (expected.next_word() & 1023) as i16 - 512 - 2 * 90,
                (expected.next_word() & 255) as i16 - 50,
            ];
        }
        enemy.spawn_command(0x92, &mut random, 90, 3).unwrap();
        for (i, pair) in coordinates.into_iter().enumerate() {
            let craft = enemy.crafts[i];
            assert_eq!((craft.lateral, craft.height), (pair[0], pair[1]));
            assert_eq!(craft.depth, 1600 - 100 * i as i16);
            assert_eq!(
                (craft.variant, craft.movement, craft.animation_phase),
                (0, 18, 0)
            );
            assert!(craft.active && enemy.shadows[i].active);
        }
        assert_eq!(random.state(), expected.state());
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(
            enemy.crafts[..3]
                .iter()
                .map(|c| c.depth)
                .collect::<Vec<_>>(),
            [1578, 1478, 1378]
        );
        for (craft, pair) in enemy.crafts[..3].iter().zip(coordinates) {
            assert_eq!((craft.lateral, craft.height), (pair[0], pair[1]));
        }
        assert_eq!(
            enemy.shadows[..3]
                .iter()
                .map(|s| s.depth)
                .collect::<Vec<_>>(),
            [1600, 1500, 1400]
        );
        for _ in 0..9 {
            expected.next_word();
        }
        assert_eq!(random.state(), expected.state());
    }
    #[test]
    fn forward_motion_switches_to_seeking_below_the_old_depth_threshold() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x92, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 240;
        enemy.crafts[0].lateral = 100;
        enemy.crafts[0].height = 60;
        let center = enemy.orbit_center;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (218, 100, 60)
        );
        assert_eq!(
            (enemy.shadows[0].depth, enemy.shadows[0].lateral),
            (240, 100)
        );
        assert_eq!(enemy.orbit_center, center);
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (196, 70, 30)
        );
        assert_eq!(
            (enemy.shadows[0].depth, enemy.shadows[0].lateral),
            (218, 100)
        );
        assert_eq!(enemy.orbit_center, center);
    }
    #[test]
    fn all_enemy_commands_initialize_their_variant_and_formation() {
        for command in 0x84..=0xa2 {
            let mut enemy = Enemy::new(&assets()).unwrap();
            let mut random = Random::default();
            let context = EnemyContext {
                difficulty: 2,
                ..EnemyContext::default()
            };
            let effects = enemy
                .spawn_command_with_context(command, &mut random, [14, 80, -100], 5, context)
                .unwrap();
            assert!(
                enemy
                    .crafts
                    .iter()
                    .all(|c| c.movement == command & 63 && c.requested_sprite == 0)
            );
            let variant = match command {
                0x95..=0x98 | 0x9d..=0x9e => 1,
                0x99..=0x9a => 2,
                0x9b..=0x9c => 3,
                0x9f..=0xa1 => 5,
                0xa2 => 6,
                _ => 0,
            };
            assert!(enemy.crafts.iter().all(|c| c.variant == variant));
            let expected_active = match command {
                0x95 | 0x97 | 0x99..=0x9d => 1,
                0x9f | 0xa2 => 3,
                0xa0..=0xa1 => 4,
                _ => 5,
            };
            assert_eq!(
                enemy.crafts.iter().filter(|c| c.active).count(),
                expected_active,
                "command {command:02x}"
            );
            assert_eq!(
                (effects.difficulty, effects.lives),
                if matches!(command, 0x9b | 0x9c) {
                    (1, 1)
                } else {
                    (0, 0)
                }
            );
        }
    }

    #[test]
    fn world_speeds_distinguish_primary_slow_and_projectile_handlers() {
        let context = EnemyContext {
            speeds: [40, 28, 10],
            ..EnemyContext::default()
        };
        for (command, depth, height) in [
            (0x84, 1572, -30),
            (0x85, 1590, -30),
            (0x86, 1572, 0),
            (0x87, 1590, 0),
        ] {
            let mut enemy = Enemy::new(&assets()).unwrap();
            let mut random = Random::default();
            enemy.spawn_command(command, &mut random, 0, 1).unwrap();
            enemy.bullet.active = true;
            enemy.bullet.depth = 200;
            enemy
                .update_half_step_with_context(&mut random, [14, 0, -100], context)
                .unwrap();
            assert_eq!(
                (enemy.crafts[0].depth, enemy.crafts[0].height),
                (depth, height)
            );
            assert_eq!(enemy.bullet.depth, 160);
            assert_eq!(enemy.shadows[0].depth, 1600);
        }
    }

    #[test]
    fn quarter_speed_orbits_seek_only_below_the_old_depth_threshold() {
        let context = EnemyContext {
            speeds: [40, 28, 10],
            ..EnemyContext::default()
        };
        for command in [0x89, 0x8b, 0x8d] {
            let mut enemy = Enemy::new(&assets()).unwrap();
            let mut random = Random::default();
            enemy.spawn_command(command, &mut random, 0, 1).unwrap();
            enemy.crafts[0].depth = 240;
            enemy.crafts[0].lateral = 100;
            enemy.crafts[0].height = 60;
            enemy
                .update_half_step_with_context(&mut random, [14, 0, -100], context)
                .unwrap();
            assert_eq!(enemy.crafts[0].depth, 233);
            enemy.crafts[0].lateral = 100;
            enemy.crafts[0].height = 60;
            enemy
                .update_half_step_with_context(&mut random, [14, 0, -100], context)
                .unwrap();
            assert_eq!(
                (
                    enemy.crafts[0].depth,
                    enemy.crafts[0].lateral,
                    enemy.crafts[0].height
                ),
                (205, 70, 30)
            );
        }
    }

    #[test]
    fn vertical_orbit_preserves_lateral_and_alternate_mode_hides_shadows() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x8c, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 800;
        enemy.crafts[0].lateral = 123;
        enemy
            .update_half_step_with_context(
                &mut random,
                [14, 0, -100],
                EnemyContext {
                    alternate_shadows: true,
                    ..EnemyContext::default()
                },
            )
            .unwrap();
        assert_eq!(enemy.crafts[0].lateral, 123);
        assert_eq!(enemy.shadows[0].lateral, 123);
        assert!(!enemy.shadows[0].active);
        assert_ne!(enemy.crafts[0].height, 0);
    }

    #[test]
    fn new_sweep_aliases_follow_their_center_steps_and_absolute_spacing() {
        for (command, step, stagger) in [
            (0x95, 8, false),
            (0x98, 0, true),
            (0x9d, 17, false),
            (0x9e, 6, true),
        ] {
            let mut enemy = Enemy::new(&assets()).unwrap();
            let mut random = Random::default();
            enemy.spawn_command(command, &mut random, 0, 2).unwrap();
            let mut effects = EnemyEffects::default();
            let delta = enemy
                .motion(
                    0,
                    &mut random,
                    [14, 100, 100],
                    EnemyContext::default(),
                    &mut effects,
                    &mut |_| {},
                )
                .unwrap();
            assert_eq!(delta, [0; 3]);
            assert_eq!(enemy.orbit_center, [step, -100 + step]);
            assert_eq!(
                (
                    enemy.crafts[0].depth,
                    enemy.crafts[0].lateral,
                    enemy.crafts[0].height,
                    enemy.crafts[0].requested_sprite
                ),
                (
                    500,
                    -2600 + step + if stagger { -160 } else { 0 },
                    16 + if step == 0 { 0 } else { -100 + step },
                    90
                )
            );
            assert_eq!(enemy.crafts[1].animation_phase, if stagger { 4 } else { 0 });
        }
    }

    #[test]
    fn falling_spawn_consumes_four_words_even_for_disabled_primaries() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x99, &mut random, 0, 3).unwrap();
        for _ in 0..12 {
            expected.next_word();
        }
        assert_eq!(random.state(), expected.state());
        assert_eq!(enemy.falling_count, 1);
        assert_eq!(enemy.crafts.map(|c| c.depth), [20, 14, 14, 0, 0]);
        assert_eq!(
            enemy.crafts[..3]
                .iter()
                .map(|c| (c.lateral, c.height))
                .collect::<Vec<_>>(),
            [(2000, 0); 3]
        );
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, false, false, false, false]
        );
        assert!(enemy.shadows.iter().all(|c| !c.active));
        enemy.spawn_command(0x9a, &mut random, 0, 3).unwrap();
        assert_eq!(enemy.falling_count, 2);
        assert!(enemy.crafts[1].active);
    }

    #[test]
    fn falling_ground_recycle_reuses_depth_and_does_not_consume_a_firing_word() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.falling_movement = 26;
        enemy.crafts[0].active = true;
        enemy.crafts[0].variant = 2;
        enemy.crafts[0].movement = 251;
        enemy.crafts[0].depth = 20;
        enemy.crafts[0].height = -120;
        enemy.crafts[0].animation_phase = 12;
        let height = (expected.next_word() & 255) as i16 + 280;
        let side = (expected.next_word() & 256) as i16;
        let lateral = (expected.next_word() & 255) as i16 + 3 * side - 512;
        let phase = (expected.next_word() & 1) as u8;
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(random.state(), expected.state());
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].height,
                enemy.crafts[0].lateral,
                enemy.crafts[0].movement,
                enemy.crafts[0].animation_phase
            ),
            (19, height, lateral, 26, phase)
        );
        assert_eq!(enemy.target, [0, 0]);
        assert!(!enemy.bullet.active);
    }

    #[test]
    fn falling_recycle_at_player_depth_retires_with_the_new_movement() {
        let mut assets = assets();
        assets.sprites[123].width_bytes = 10;
        assets.sprites[123].height = 31;
        for movement in [25, 26] {
            let mut enemy = Enemy::new(&assets).unwrap();
            let mut random = Random::default();
            let mut expected = Random::default();
            enemy.falling_movement = movement;
            let craft = &mut enemy.crafts[0];
            craft.active = true;
            craft.variant = 2;
            craft.movement = 251;
            craft.depth = 15;
            craft.height = -120;
            craft.animation_phase = 12;
            craft.requested_sprite = 123;
            enemy.project(&assets, false, 106, 0).unwrap();
            let old_rect = enemy.crafts[0].rect;
            let height = (expected.next_word() & 255) as i16 + 280;
            let side = (expected.next_word() & 256) as i16;
            let lateral = (expected.next_word() & 255) as i16 + 3 * side - 512;
            expected.next_word();

            enemy.update_half_step(&mut random, [14, 0, -100]);
            let craft = enemy.crafts[0];
            assert!(!craft.active);
            assert_eq!(
                (craft.depth, craft.lateral, craft.height),
                (14, lateral, height)
            );
            assert_eq!((craft.movement, craft.animation_phase), (movement, 0));
            assert_eq!(craft.requested_sprite, 123);
            assert_eq!(random.state(), expected.state());

            enemy.project(&assets, false, 106, 0).unwrap();
            let craft = enemy.crafts[0];
            assert_eq!(craft.resolved_sprite, 123);
            assert_eq!(craft.rect.x_bytes, 128);
            assert_eq!(craft.rect.y, old_rect.y);
            assert_eq!((craft.rect.width_bytes, craft.rect.height), (10, 31));
            enemy.update_half_step(&mut random, [14, 0, -100]);
            assert_eq!(random.state(), expected.state());
            assert_eq!(enemy.crafts[0].requested_sprite, 123);
        }
    }

    #[test]
    fn falling_variants_split_contact_recycling_from_weapon_explosions() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x99, &mut random, 0, 1).unwrap();
        enemy.explode_craft(5).unwrap();
        assert_eq!(enemy.crafts[0].movement, 251);
        assert_eq!(enemy.craft_animations[0], None);
        enemy.hit_by_weapon(5).unwrap();
        assert_eq!(enemy.crafts[0].movement, 3);
        assert_eq!(enemy.craft_animations[0], Some(0));
        enemy.bullet.active = true;
        enemy.bullet.requested_sprite = 67;
        enemy.deactivate_projectile();
        assert!(!enemy.bullet.active);
        assert_eq!(enemy.bullet.requested_sprite, 67);
    }

    #[test]
    fn transition_grants_life_once_then_scores_and_departs_from_phase_thirty() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let context = EnemyContext {
            horizon: 18,
            speeds: [40, 28, 10],
            ..EnemyContext::default()
        };
        let spawn = enemy
            .spawn_command_with_context(0x9b, &mut random, [14, 70, -28], 5, context)
            .unwrap();
        assert_eq!((spawn.lives, spawn.difficulty), (1, 1));
        enemy.crafts[0].depth = 50;
        let enter = enemy
            .update_half_step_with_context(&mut random, [14, 70, -28], context)
            .unwrap();
        assert_eq!(enter.points, 0);
        assert_eq!(enter.points, 0);
        assert_eq!(
            (enemy.crafts[0].depth, enemy.crafts[0].animation_phase),
            (50, 2)
        );
        enemy.crafts[0].animation_phase = 30;
        let depart = enemy
            .update_half_step_with_context(&mut random, [14, 70, -28], context)
            .unwrap();
        assert_eq!(depart.points, 3000);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height,
                enemy.crafts[0].animation_phase
            ),
            (90, 0, 18, 31)
        );
        enemy.crafts[0].depth = 1000;
        enemy
            .update_half_step_with_context(&mut random, [14, 70, -28], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].depth, -50);
        assert!(!enemy.crafts[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, 160);
    }

    #[test]
    fn boss_variants_use_narrow_targets_and_phase_driven_departure() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.spawn_command(0x9f, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 200;
        let context = EnemyContext::default();
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        expected.next_word();
        let target = [
            (expected.next_word() & 7) as i16 - 3,
            -100 + (expected.next_word() & 7) as i16 - 3,
        ];
        assert_eq!(enemy.target, target);
        assert_eq!(random.state(), expected.state());
        assert_eq!(enemy.crafts[0].animation_phase, 1);
        enemy.crafts[0].animation_phase = 120;
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].animation_phase, 255);
        let old = enemy.crafts[0].depth;
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].depth, old + 22);
    }

    #[test]
    fn crossing_boss_uses_two_parameterized_segments_before_departure() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0xa1, &mut random, 0, 4).unwrap();
        enemy.orbit_center = [0, -100];
        enemy.crafts[0].animation_phase = 40;
        enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
        assert_eq!(
            (
                enemy.crafts[0].lateral,
                enemy.crafts[0].height,
                enemy.crafts[0].animation_phase
            ),
            (400, 120, 41)
        );
        enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
        assert_eq!(
            (enemy.crafts[0].lateral, enemy.crafts[0].height),
            (400, 115)
        );
        enemy.crafts[0].animation_phase = 81;
        enemy.crafts[0].depth = 200;
        let depth = enemy.crafts[0].depth;
        enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
        assert_eq!(enemy.crafts[0].depth, depth + 22);
        assert_eq!(enemy.crafts[0].animation_phase, 81);
    }

    #[test]
    fn patrol_changes_source_after_resolution_and_crosses_all_four_turns() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let context = EnemyContext {
            difficulty: 2,
            patrol_sources: Some([178, 179, 180]),
            ..EnemyContext::default()
        };
        enemy
            .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 5, context)
            .unwrap();
        assert_eq!(
            enemy.crafts.map(|c| c.active),
            [true, true, true, false, false]
        );
        enemy.crafts[0].lateral = 400;
        enemy
            .patrol(0, &mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].animation_phase, 1);
        assert!((178..=180).contains(&enemy.crafts[0].requested_sprite));
        let state = random.state();
        enemy.crafts[0].height = 0;
        enemy
            .patrol(0, &mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].animation_phase, 2);
        assert_eq!(random.state(), state);
        enemy.crafts[0].lateral = -400;
        enemy
            .patrol(0, &mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].animation_phase, 3);
        enemy.crafts[0].height = -100;
        enemy
            .patrol(0, &mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].animation_phase, 0);
    }

    #[test]
    fn game_reset_retains_primary_cache_and_restores_global_target_and_center() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x92, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 90;
        enemy.crafts[0].height = 120;
        enemy.crafts[0].phase = 3;
        enemy.crafts[0].variant = 5;
        enemy.crafts[0].animation_phase = 87;
        enemy.crafts[0].requested_sprite = 139;
        enemy.crafts[0].resolved_sprite = 138;
        enemy.target = [99, 77];
        enemy.orbit_center = [20, 40];
        let old = enemy.crafts[0];
        enemy.reset_for_game(&assets()).unwrap();
        let after = enemy.crafts[0];
        assert_eq!(
            (
                after.depth,
                after.height,
                after.phase,
                after.variant,
                after.animation_phase,
                after.requested_sprite,
                after.resolved_sprite
            ),
            (
                old.depth,
                old.height,
                old.phase,
                old.variant,
                old.animation_phase,
                old.requested_sprite,
                old.resolved_sprite
            )
        );
        assert!(!after.active);
        assert_eq!(after.movement, 0);
        assert_eq!(enemy.target, [0, 0]);
        assert_eq!(enemy.orbit_center, [0, -100]);
        assert_eq!(
            (
                enemy.bullet.depth,
                enemy.bullet.lateral,
                enemy.bullet.height,
                enemy.bullet.requested_sprite,
                enemy.bullet.resolved_sprite
            ),
            (14, 0, -100, 22, 22)
        );
    }
    #[test]
    fn patrol_reports_original_retry_stall_without_fabricating_a_source() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let context = EnemyContext {
            difficulty: 10,
            ..EnemyContext::default()
        };
        enemy
            .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 1, context)
            .unwrap();
        let before = random.state();
        let error = enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap_err();
        assert!(matches!(error,Error::InvalidData(message) if message.contains("stalls")));
        assert_eq!(enemy.crafts[0].requested_sprite, 0);
        assert_eq!(random.state(), before);
        let missing = EnemyContext {
            difficulty: 12,
            ..context
        };
        assert!(
            enemy
                .update_half_step_with_context(&mut random, [14, 0, -100], missing)
                .is_err()
        );
    }

    #[test]
    fn restored_records_keep_cache_and_goal_clear_defers_shadow_retirement() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(0x88, &mut random, 0, 2).unwrap();
        enemy.crafts[1].requested_sprite = 33;
        enemy.crafts[1].resolved_sprite = 32;
        enemy.crafts[1].phase = 2;
        let snapshots = enemy.snapshots();
        let mut restored = Enemy::restore_snapshots(snapshots, &assets()).unwrap();
        assert_eq!(
            (
                restored.crafts[1].requested_sprite,
                restored.crafts[1].resolved_sprite,
                restored.crafts[1].phase
            ),
            (33, 32, 2)
        );
        restored.clear_primary();
        assert!(!restored.active());
        assert!(restored.shadows[0].active);
        restored.update_half_step(&mut random, [14, 0, -100]);
        assert!(restored.shadows.iter().all(|shadow| !shadow.active));
        let mut malformed = snapshots;
        malformed[0].slot = 5;
        assert!(Enemy::restore_snapshots(malformed, &assets()).is_err());
    }
    #[test]
    fn falling_spawn_requests_one_tone_for_every_initialized_slot() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut actions = Vec::new();
        enemy
            .spawn_with_sound(
                0x99,
                &mut Random::default(),
                [14, 0, -100],
                5,
                EnemyContext::default(),
                &mut |a| actions.push(a),
            )
            .unwrap();
        assert_eq!(
            actions,
            vec![
                GameplayAction::Request {
                    site: 0x1a7e,
                    effect: Effect::Tone(Tone::Second)
                };
                5
            ]
        );
        assert!(!enemy.crafts[4].active);
    }

    #[test]
    fn transition_requests_tone_then_the_guarded_sample_before_departure() {
        let mut enemy = Enemy::new(&assets()).unwrap();
        enemy
            .spawn_command(0x9b, &mut Random::default(), 0, 1)
            .unwrap();
        enemy.crafts[0].depth = 50;
        let mut actions = Vec::new();
        enemy
            .update_with_sound(
                &mut Random::default(),
                [14, 0, -100],
                EnemyContext::default(),
                &mut |a| actions.push(a),
            )
            .unwrap();
        assert_eq!(
            actions,
            [
                GameplayAction::Request {
                    site: 0x42bc,
                    effect: Effect::Tone(Tone::First)
                },
                GameplayAction::IntroSample {
                    site: 0x42bf,
                    request: SampleRequest {
                        resource: Resource::Data8,
                        offset: 0,
                        count: 9723,
                        reverse: false
                    }
                },
            ]
        );
    }
}
