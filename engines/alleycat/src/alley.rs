// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! External game assets, scenery, actors, player updates, and scene transitions.

use crate::assets::{
    Assets, idle_sources, initialize_alley, next_random, set_word, sprite_bytes, word,
};
use crate::cga::{Dimensions, Framebuffer, PlayerCompositor, pixel_offset};
use crate::decoration::{award_points, draw_ground_objects, draw_hud, initialize_score_state};
#[cfg(feature = "dev-tools")]
use crate::dev_tools::{Actor, Damage, DevOptions};
use crate::mice::{Blend, Contact, Image, Mice};
use crate::player::{Controls, Draw, Player, UpdateOutcome};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Machine {
    #[default]
    Cga,
    Pcjr,
}

impl Machine {
    /// Conventional digital RGB values; composite/monitor conversion is separate.
    pub(crate) fn colors(self) -> [[u8; 4]; 4] {
        match self {
            Self::Cga => [
                [0, 0, 0, 255],
                [0, 170, 170, 255],
                [170, 0, 170, 255],
                [170, 170, 170, 255],
            ],
            Self::Pcjr => [
                [0, 0, 0, 255],
                [85, 255, 85, 255],
                [85, 85, 255, 255],
                [255, 255, 255, 255],
            ],
        }
    }
}

#[derive(Clone, Copy)]
enum Harm {
    AlleyDog,
    RoomDog,
    WindowProjectiles,
    MilkDogs,
    Spider,
    Eels,
    Air,
}

#[derive(Clone, Copy)]
enum EffectKind {
    Projectile(crate::projectiles::VisibleSprite),
    Spider,
    Eel {
        phase: EelPhase,
        palette_tick: u16,
        background: u8,
    },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EelPhase {
    Poll,
    Extra,
    Palette,
    Restart,
}

/// Native blocking-loop path, used by Runtime to charge its execution cost.
pub(crate) enum DamagePoll {
    Ordinary,
    Spider,
    Eel,
    EelPalette,
    EelRestart,
}

struct DamageEffect {
    since: u16,
    duration: u16,
    kind: EffectKind,
}

/// Post-pass selection state for development traces; observing it consumes no RNG.
#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionTraceState {
    pub last_room: u16,
    pub previous_room: u16,
    pub random_state: u16,
    pub difficulty: u16,
    pub pending_hearts: bool,
}

pub struct Alley {
    data: Vec<u8>,
    pub(crate) sound: crate::sound::Sound,
    screen: Framebuffer,
    presented: Framebuffer,
    compositor: PlayerCompositor,
    bin_image: Option<crate::bin_cat::VisibleSprite>,
    mice: Mice,
    mouse_effect: Option<Image>,
    pub player: Player,
    machine: Machine,
    completion: Option<crate::completion::Completion>,
    hearts_completion: Option<crate::hearts_completion::HeartsCompletion>,
    selection_work: Option<crate::windows::SelectionWork>,
    /// Normal cat image held aside while the milk drinking helper owns drawing.
    milk_player_image: Option<(Dimensions, Vec<u8>)>,
    air_warning: u8,
    dog_image: Option<crate::dog::VisibleSprite>,
    projectile_image: Option<crate::projectiles::VisibleSprite>,
    damage_effect: Option<DamageEffect>,
    failure: Option<crate::failure::Failure>,
    game_over: bool,
    initial_seed: u16,
    initial_difficulty: u16,
    #[cfg(feature = "dev-tools")]
    dev_options: DevOptions,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MouseAdvance {
    pub redraw: bool,
    pub caught: bool,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BinAdvance {
    pub draw_rows: u8,
    pub restore_rows: u8,
    pub started: bool,
}

impl Alley {
    fn harm_enabled(&self, harm: Harm) -> bool {
        #[cfg(feature = "dev-tools")]
        {
            !self.dev_options.damage_disabled(match harm {
                Harm::AlleyDog => Damage::AlleyDog,
                Harm::RoomDog => Damage::RoomDog,
                Harm::WindowProjectiles => Damage::WindowProjectiles,
                Harm::MilkDogs => Damage::MilkDogs,
                Harm::Spider => Damage::Spider,
                Harm::Eels => Damage::Eels,
                Harm::Air => Damage::Air,
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            let _ = harm;
            true
        }
    }

    fn infinite_lives(&self) -> bool {
        #[cfg(feature = "dev-tools")]
        {
            self.dev_options.infinite_lives
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            false
        }
    }

    #[cfg(feature = "sdl")]
    pub(crate) fn toggle_sound(&mut self) {
        self.data[0] = !self.data[0];
        if self.data[0] == 0 {
            self.sound.stop();
        }
    }
    #[cfg(feature = "sdl")]
    pub(crate) fn title_sound_preference(&mut self, splash: &crate::splash::Splash) {
        self.data[0] = splash.data()[0];
    }
    pub fn game_over(&self) -> bool {
        self.game_over
    }

    /// Native last-life return: promote the score before drawing the title.
    pub fn game_over_splash(&mut self) -> Result<crate::splash::Splash, String> {
        if !self.game_over {
            return Err("The current run has not ended".into());
        }
        self.promote_high_score();
        crate::splash::Splash::after_game_over(&self.data, self.machine, 0)
    }

    /// Resume after the later title without reseeding or clearing room history.
    pub fn start_after_game_over(&mut self, splash: &crate::splash::Splash) -> Result<(), String> {
        if !self.game_over
            || !splash.finished()
            || splash.machine() != self.machine
            || splash.data()[0x41a] == 0
        {
            return Err("A completed game-over title is required".into());
        }
        self.data[0] = splash.data()[0];
        set_word(&mut self.data, 0x2ae5, word(splash.data(), 0x2ae5));
        self.reset_run(0, false)
    }

    /// Build the keyboard menu from the external executable's text tables.
    pub fn menu(&self, difficulty: Option<u16>) -> Result<crate::menu::Menu, String> {
        match difficulty {
            Some(level) => crate::menu::Menu::with_difficulty(&self.data, self.machine, level),
            None => crate::menu::Menu::new(&self.data, self.machine),
        }
    }

    /// Apply the menu selection to this run and subsequent restarts.
    pub fn start_new_game(&mut self, difficulty: u16, tick: u16) -> Result<(), String> {
        if difficulty > 7 {
            return Err("Difficulty must be 0..7".into());
        }
        self.initial_difficulty = difficulty;
        set_word(&mut self.data, 0x6df8, difficulty & 3);
        self.restart(tick)
    }

    /// Begin a new run, preserving the high score and configured development controls.
    pub fn restart(&mut self, tick: u16) -> Result<(), String> {
        self.reset_run(tick, true)
    }

    fn promote_high_score(&mut self) {
        if self.data[0x1f82..0x1f89] > self.data[0x1f89..0x1f90] {
            let score = self.data[0x1f82..0x1f89].to_vec();
            self.data[0x1f89..0x1f90].copy_from_slice(&score);
        }
    }

    fn reset_run(&mut self, tick: u16, fresh_random: bool) -> Result<(), String> {
        self.sound.reset();
        self.promote_high_score();
        self.data[0x1f82..0x1f89].fill(0);
        self.data[0x1f80] = 3;
        self.data[0x418] = 0;
        self.data[0x419] = 0;
        self.data[0x41b] = 0;
        self.data[0x41c] = 0;
        self.data[0x552] = 0;
        self.data[0x553] = 0;
        for (offset, value) in [
            (8, self.initial_difficulty),
            (0x414, 0),
            (0x1c30, 0),
            (0x412, tick),
        ] {
            set_word(&mut self.data, offset, value);
        }
        if fresh_random {
            set_word(&mut self.data, 0x2ae5, self.initial_seed);
            set_word(&mut self.data, 0x41d, 0xffff);
            set_word(&mut self.data, 0x41f, 0xffff);
        }
        self.game_over = false;
        self.failure = None;
        self.completion = None;
        self.hearts_completion = None;
        self.selection_work = None;
        self.damage_effect = None;
        // Host shortcuts can interrupt the timed mouse-contact presentation.
        // Its runtime wait is discarded along with this run, so discard its image.
        self.mouse_effect = None;
        self.return_to_alley()
    }

    fn initialize_ground_entry(&mut self) {
        set_word(&mut self.data, 0x579, 0);
        self.data[0x57b] = 180;
        self.data[0x57c] = 230;
        set_word(&mut self.data, 0x55f, pixel_offset(0, 180));
        set_word(&mut self.data, 0x561, 0x0b03);
        self.data[0x56e] = 1;
        self.data[0x558] = 3;
        self.data[0x559] = 12;
        self.data[0x571] = 0;
        set_word(&mut self.data, 0x572, 2);
        self.data[0x576] = 1;
        for offset in [
            0x55b, 0x550, 0x55c, 0x55a, 0x583, 0x698, 0x699, 0x551, 0x584, 0x552, 0x553, 0x127c,
        ] {
            self.data[offset] = 0;
        }
        for offset in [0x554, 0x57d, 0x684] {
            set_word(&mut self.data, offset, 0);
        }
    }

    fn finish_run(&mut self) {
        self.sound.stop();
        self.game_over = true;
        self.damage_effect = None;
        self.dog_image = None;
        self.projectile_image = None;
        // The host returns to the title immediately; retain the last native
        // presentation for headless callers instead of drawing an authored screen.
        self.presented = self.screen.clone();
    }

    pub(crate) fn dying(&self) -> bool {
        self.data[0x1cb8] != 0
    }
    pub(crate) fn damage_waiting(&self) -> bool {
        self.damage_effect.is_some()
    }

    pub(crate) fn damage_poll(&self) -> DamagePoll {
        match self.damage_effect.as_ref().map(|effect| effect.kind) {
            Some(EffectKind::Spider) => DamagePoll::Spider,
            Some(EffectKind::Eel {
                phase: EelPhase::Palette,
                ..
            }) => DamagePoll::EelPalette,
            Some(EffectKind::Eel {
                phase: EelPhase::Restart,
                ..
            }) => DamagePoll::EelRestart,
            Some(EffectKind::Eel { .. }) => DamagePoll::Eel,
            _ => DamagePoll::Ordinary,
        }
    }

    pub(crate) fn advance_damage_effect(
        &mut self,
        tick: u16,
        counter: u16,
        retrace: bool,
    ) -> Result<bool, String> {
        let Some(effect) = &mut self.damage_effect else {
            return Ok(true);
        };
        if let EffectKind::Eel {
            phase,
            palette_tick,
            background,
        } = &mut effect.kind
        {
            // CS:3543..356e checks the thirteen-tick exit only after its
            // retrace loop, extra pulse and BIOS palette request. Crossing the
            // deadline while retrace is clear must not preempt those steps.
            if *phase != EelPhase::Restart || tick.wrapping_sub(effect.since) < effect.duration {
                match *phase {
                    EelPhase::Poll | EelPhase::Restart => {
                        if *phase == EelPhase::Restart {
                            *palette_tick = tick;
                        }
                        self.sound.eel_step(&mut self.data);
                        *phase = if retrace {
                            EelPhase::Extra
                        } else {
                            EelPhase::Poll
                        };
                    }
                    EelPhase::Extra => {
                        self.sound.eel_step(&mut self.data);
                        *phase = EelPhase::Palette;
                    }
                    EelPhase::Palette => {
                        *background = if *palette_tick & 1 != 0 { 1 } else { 15 };
                        self.sound.eel_step(&mut self.data);
                        *phase = EelPhase::Restart;
                    }
                }
                return Ok(false);
            }
        } else if tick.wrapping_sub(effect.since) < effect.duration {
            match effect.kind {
                EffectKind::Projectile(_) => self.sound.projectile_step(&mut self.data, counter),
                EffectKind::Spider => self.sound.spider_step(&mut self.data),
                EffectKind::Eel { .. } => unreachable!(),
            }
            return Ok(false);
        }
        self.sound.stop();
        let effect = self.damage_effect.take().expect("effect exists");
        match effect.kind {
            EffectKind::Projectile(_) => {
                let lives = self.data[0x1f80];
                crate::projectiles::finish_contact(&mut self.data)?;
                if self.infinite_lives() {
                    self.data[0x1f80] = lives;
                }
                self.player = Player::from_data(&self.data)?;
                draw_hud(&mut self.data, &mut self.screen)?;
                if self.data[0x1f80] == 0 {
                    self.finish_run();
                }
            }
            EffectKind::Spider => crate::bookcase::finish_spider_failure(&mut self.data)?,
            EffectKind::Eel { .. } => {}
        }
        if !self.game_over {
            self.compose_actors()?;
        }
        Ok(true)
    }

    fn draw_projectile(&mut self, image: crate::projectiles::VisibleSprite) -> Result<(), String> {
        self.presented.zero_transparent_rect(
            image.offset,
            image.dimensions,
            sprite_bytes(&self.data, image.source, image.dimensions)?,
        )?;
        Ok(())
    }

    fn apply_projectile_contact(
        &mut self,
        contact: crate::projectiles::Contact,
        tick: u16,
    ) -> Result<bool, String> {
        if matches!(contact, crate::projectiles::Contact::None) {
            return Ok(false);
        }
        self.compositor.restore(&mut self.screen)?;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        if let crate::projectiles::Contact::FirstHit { effect } = contact {
            self.damage_effect = Some(DamageEffect {
                since: tick,
                duration: 10,
                kind: EffectKind::Projectile(effect),
            });
        }
        self.compose_actors()?;
        Ok(true)
    }

    pub(crate) fn check_player_projectile(&mut self, tick: u16) -> Result<bool, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::WindowProjectiles) {
            return Ok(false);
        }
        self.player.write_data(&mut self.data)?;
        let enabled = self.harm_enabled(Harm::WindowProjectiles);
        let contact = crate::projectiles::contact(&mut self.data, &self.player, tick, enabled);
        self.apply_projectile_contact(contact, tick)
    }

    pub(crate) fn advance_projectiles(&mut self, tick: u16) -> Result<bool, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::WindowProjectiles) {
            return Ok(false);
        }
        self.player.write_data(&mut self.data)?;
        let enabled = self.harm_enabled(Harm::WindowProjectiles);
        let Some(step) = crate::projectiles::advance(&mut self.data, &self.player, tick, enabled)?
        else {
            return Ok(false);
        };
        if step.restore_previous {
            self.projectile_image = None;
        }
        if let Some(image) = step.sprite {
            set_word(&mut self.data, 0x17e5, image.offset);
            set_word(&mut self.data, 0x17e3, image.dimensions.0);
            self.projectile_image = Some(image);
        }
        let contact = self.apply_projectile_contact(step.contact, tick)?;
        if !contact {
            self.compose_actors()?;
        }
        Ok(step.sprite.is_some())
    }

    pub(crate) fn advance_ground_dog(&mut self, tick: u16, retrace: bool) -> Result<bool, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(if self.scene() == 0 {
            Actor::AlleyDog
        } else if self.scene() == 6 {
            Actor::MilkDogs
        } else {
            Actor::RoomDog
        }) {
            return Ok(false);
        }
        self.player.write_data(&mut self.data)?;
        let enabled = self.harm_enabled(if self.scene() == 0 {
            Harm::AlleyDog
        } else if self.scene() == 6 {
            Harm::MilkDogs
        } else {
            Harm::RoomDog
        });
        let lives = self.data[0x1f80];
        let Some(step) = crate::dog::advance(&mut self.data, tick, retrace, !enabled)? else {
            return Ok(false);
        };
        let drawn = step.sprite.is_some();
        self.dog_image = step.sprite;
        if step.started_death || step.failure {
            self.compositor.restore(&mut self.screen)?;
            self.player = Player::from_data(&self.data)?;
        }
        if step.life_lost {
            if self.infinite_lives() {
                self.data[0x1f80] = lives;
            }
            draw_hud(&mut self.data, &mut self.screen)?;
            if self.data[0x1f80] == 0 {
                self.finish_run();
                return Ok(drawn);
            }
        }
        self.compose_actors()?;
        Ok(drawn)
    }

    pub(crate) fn check_player_dog(&mut self) -> Result<bool, String> {
        let scene = self.scene();
        if !matches!(scene, 0 | 1 | 3..=5) {
            return Ok(false);
        }
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(if scene == 0 {
            Actor::AlleyDog
        } else if scene == 6 {
            Actor::MilkDogs
        } else {
            Actor::RoomDog
        }) {
            return Ok(false);
        }
        self.player.write_data(&mut self.data)?;
        if !self.harm_enabled(if scene == 0 {
            Harm::AlleyDog
        } else {
            Harm::RoomDog
        }) || !crate::dog::contact(&self.data)
        {
            return Ok(false);
        }
        self.compositor.restore(&mut self.screen)?;
        self.dog_image = None;
        crate::dog::begin_death(&mut self.data)?;
        self.player = Player::from_data(&self.data)?;
        self.compose_actors()?;
        Ok(true)
    }
    /// Configure development overrides before play or diagnostic scene entry.
    /// Suppression also retires images already drawn in the current scene.
    #[cfg(feature = "dev-tools")]
    pub fn set_dev_options(&mut self, options: DevOptions) -> Result<(), String> {
        self.dev_options = options;
        self.apply_actor_overrides()
    }

    #[cfg(feature = "dev-tools")]
    fn apply_actor_overrides(&mut self) -> Result<(), String> {
        let options = self.dev_options;
        let scene = self.scene();
        if options.actor_disabled(if scene == 0 {
            Actor::AlleyDog
        } else if scene == 6 {
            Actor::MilkDogs
        } else {
            Actor::RoomDog
        }) {
            self.dog_image = None;
            crate::dog::initialize(&mut self.data)?;
        }
        if options.actor_disabled(Actor::WindowProjectiles) {
            self.projectile_image = None;
            self.data[0x1673] = 0;
            self.data[0x1678] = 0;
        }
        if scene == 0 {
            if options.actor_disabled(Actor::BinCat) {
                self.data[0x1d59] = 0;
                self.bin_image = None;
            }
            if options.actor_disabled(Actor::AlleyMice) {
                self.mouse_effect = None;
            }
            return self.compose_actors();
        }
        self.room_actor(|data, screen, _sound| {
            if matches!(scene, 1 | 3..=6) && options.actor_disabled(Actor::Broom) {
                crate::room::restore_broom(data, screen)?;
                data[0x3286] = 1;
            }
            if scene == 3 && options.actor_disabled(Actor::Spider) {
                crate::bookcase::remove_spider(data, screen)?;
            }
            if scene == 2 && options.actor_disabled(Actor::Eels) {
                crate::fishbowl::remove_eels(data, screen)?;
            }
            if scene == 6 && options.actor_disabled(Actor::MilkDogs) {
                crate::milk::remove_dogs(data, screen)?;
            }
            if scene == 7 {
                if options.actor_disabled(Actor::HeartsRivals) {
                    crate::hearts::remove_rivals(data, screen)?;
                }
                if options.actor_disabled(Actor::HeartsArrows) {
                    crate::hearts::remove_arrow(data, screen)?;
                }
            }
            Ok(())
        })
    }

    pub fn machine(&self) -> Machine {
        self.machine
    }

    /// Keyboard pause captions from the supported executable's data segment.
    pub fn pause_text(&self) -> Result<[&str; 2], String> {
        let read = |offset| {
            let bytes = self
                .data
                .get(offset..offset + 33)
                .ok_or("missing pause text")?;
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or("unterminated pause text")?;
            std::str::from_utf8(&bytes[..end]).map_err(|_| "invalid pause text")
        };
        Ok([
            read(0x6d91).map_err(str::to_owned)?,
            read(0x6db2).map_err(str::to_owned)?,
        ])
    }

    pub(crate) fn advance_random(&mut self) {
        let bit = (next_random(&mut self.data) as u8) & 2 & self.data[0];
        self.sound.gate(self.sound.control() ^ bit);
    }
    pub(crate) fn bin_sound(&mut self) {
        for divisor in [0x7d0, 0xa6e, 0xdec] {
            self.sound.beep(&self.data, divisor, 0x1800);
        }
    }
    pub(crate) fn support_sound_latched(&self) -> bool {
        self.data[0x127c] != 0
    }
    pub(crate) fn clear_support_sound(&mut self) {
        self.data[0x127c] = 0;
    }
    pub(crate) fn support_sound(&mut self, divisor: u16, attached: bool) {
        if self.data[0x1cbf] == 0 {
            self.sound.beep(&self.data, divisor, 0x1800);
        }
        self.data[0x127c] = u8::from(attached);
    }
    pub(crate) fn upper_landing_sound(&mut self, tick: u16) {
        self.sound.upper_landing(&mut self.data, tick);
    }

    pub(crate) fn bin_contact_sound_ready(&self) -> bool {
        self.data[0] != 0 && !self.bin_contact_latched()
    }

    pub(crate) fn bin_contact_latched(&self) -> bool {
        self.data[0x56c] != 0
    }

    #[cfg(test)]
    pub(crate) fn synthetic() -> Self {
        let mut data = vec![0; 0x7130];
        set_word(&mut data, 0x2ae5, 0xfa59);
        data[0x1028] = 0xff;
        data[0x100e] = 7;
        Self {
            sound: crate::sound::Sound::default(),
            data,
            screen: Framebuffer::default(),
            presented: Framebuffer::default(),
            compositor: PlayerCompositor::default(),
            bin_image: None,
            mice: Mice::default(),
            mouse_effect: None,
            player: Player::at_alley_floor(80),
            machine: Machine::Cga,
            completion: None,
            hearts_completion: None,
            selection_work: None,
            milk_player_image: None,
            air_warning: 0,
            dog_image: None,
            projectile_image: None,
            damage_effect: None,
            failure: None,
            game_over: false,
            initial_seed: 0xfa59,
            initial_difficulty: 0,
            #[cfg(feature = "dev-tools")]
            dev_options: DevOptions::default(),
        }
    }

    pub fn load(
        original: impl AsRef<Path>,
        seed: u16,
        difficulty: u16,
        machine: Machine,
    ) -> Result<Self, String> {
        let assets = Assets::load(original)?;
        let mut data = assets.runtime_data();
        // Original startup enables sound. Host mute/volume must not change this
        // native flag: the speaker routines also affect timing and shared RNG.
        data[0] = 0xff;
        data[0x69a] = 16;
        // CS:0073..0079 marks both selector-history entries as empty.
        set_word(&mut data, 0x41d, 0xffff);
        set_word(&mut data, 0x41f, 0xffff);
        set_word(&mut data, 8, difficulty);
        set_word(&mut data, 0x2ae5, seed);
        set_word(&mut data, 0x6df8, 0);
        data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
        let mut screen = Framebuffer::default();
        initialize_score_state(&mut data)?;
        initialize_alley(&mut data, &mut screen)?;
        draw_ground_objects(&mut data, &mut screen)?;
        draw_hud(&mut data, &mut screen)?;
        crate::dog::initialize(&mut data)?;
        crate::bin_cat::initialize(&mut data)?;
        crate::windows::initialize(&mut data)?;
        let mice = Mice::initialize(&mut data, 8)?;
        let mut alley = Self {
            sound: crate::sound::Sound::default(),
            data,
            presented: screen.clone(),
            screen,
            compositor: PlayerCompositor::default(),
            bin_image: None,
            mice,
            mouse_effect: None,
            player: Player::at_alley_floor(8),
            machine,
            completion: None,
            hearts_completion: None,
            selection_work: None,
            milk_player_image: None,
            air_warning: 0,
            dog_image: None,
            projectile_image: None,
            damage_effect: None,
            failure: None,
            game_over: false,
            initial_seed: 0xfa59,
            initial_difficulty: 0,
            #[cfg(feature = "dev-tools")]
            dev_options: DevOptions::default(),
        };
        alley.initial_seed = seed;
        alley.initial_difficulty = difficulty;
        // The first accepted neutral update establishes the original idle image.
        alley.update(Controls::default())?;
        Ok(alley)
    }

    pub fn update(&mut self, controls: Controls) -> Result<UpdateOutcome, String> {
        self.update_at_tick(controls, word(&self.data, 0x57d))
    }

    pub(crate) fn update_at_tick(
        &mut self,
        controls: Controls,
        tick: u16,
    ) -> Result<UpdateOutcome, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::BinCat) {
            self.data[0x1d59] = 0;
        }
        let mut floor_mark = false;
        let image = if matches!(self.scene(), 1 | 3..=6 | 7) {
            let image = self.compositor.suspend(&mut self.screen)?;
            self.player.write_data(&mut self.data)?;
            if matches!(self.scene(), 1 | 3..=6)
                && !self.dying()
                && self.player.floor_track_due(controls)
            {
                self.data[0x56e] = controls.x as u8;
                floor_mark = crate::room::record_floor_track(&mut self.data, &mut self.screen)?;
            }
            image
        } else {
            None
        };
        let projectile_enabled = self.scene() == 0 && self.harm_enabled(Harm::WindowProjectiles);
        #[cfg(feature = "dev-tools")]
        let projectile_enabled =
            projectile_enabled && !self.dev_options.actor_disabled(Actor::WindowProjectiles);
        // The final entrance step moves the logical position inward before
        // handling controls. CS:1069's idle draw still uses the last image's
        // offset; walking recalculates its destination at CS:0f6d instead.
        let entry_idle_offset = (self.player.entry_state == 1 && self.player.entry_delay == 0)
            .then(|| pixel_offset(self.player.x, self.player.y));
        let mut outcome =
            self.player
                .update_with_projectiles(controls, &self.data, projectile_enabled)?;
        outcome.floor_mark = floor_mark;
        if outcome.projectile_contact {
            self.check_player_projectile(tick)?;
            return Ok(outcome);
        }
        if self.player.take_room_exit() {
            self.data[0x551] = 1;
        }
        if outcome.bin_contact_checked {
            if let Some(expected) = outcome.forced_controls {
                self.data[0x1d58] = 1;
                // The downward-transition tail clears the support-tone latch,
                // independently of the first-contact speaker effect.
                self.data[0x127c] = 0;
                let actual = crate::bin_cat::response_controls(&mut self.data);
                debug_assert_eq!(actual, expected);
            } else {
                crate::bin_cat::clear_response_latch(&mut self.data);
            }
        }
        if outcome.edge_reset {
            // Non-player state cleared by the original alley edge initializer.
            // Its blocking speaker helper's PRNG schedule is still separate.
            for offset in [0x583, 0x698, 0x699, 0x551, 0x584, 0x552, 0x553, 0x127c] {
                self.data[offset] = 0;
            }
            for offset in [0x554, 0x57d, 0x684] {
                set_word(&mut self.data, offset, 0);
            }
        }
        let offset = pixel_offset(self.player.x, self.player.y);
        match outcome.draw {
            Draw::Keep => {
                if let Some((dimensions, source)) = image {
                    self.compositor
                        .draw(&mut self.screen, offset, dimensions, &source)?;
                }
            }
            Draw::Erase => self.compositor.restore(&mut self.screen)?,
            Draw::Idle => {
                let (top, bottom) = idle_sources(&mut self.data);
                let top = sprite_bytes(&self.data, top, Dimensions(0x0602))?;
                let bottom = sprite_bytes(&self.data, bottom, Dimensions(0x0602))?;
                self.compositor.draw_idle(
                    &mut self.screen,
                    entry_idle_offset.unwrap_or(offset),
                    top,
                    bottom,
                )?;
            }
            Draw::Sprite(sprite) => {
                let dimensions = Dimensions(sprite.dimensions);
                let source = sprite_bytes(&self.data, sprite.source, dimensions)?;
                self.compositor
                    .draw(&mut self.screen, offset, dimensions, source)?;
            }
            Draw::StridedSprite {
                sprite,
                stride_bytes,
            } => {
                let dimensions = Dimensions(sprite.dimensions);
                let width = dimensions.width_bytes();
                if stride_bytes < width {
                    return Err("Sprite source stride is smaller than its visible width".into());
                }
                let mut packed = Vec::with_capacity(dimensions.len());
                for row in 0..dimensions.rows() {
                    let start = usize::from(sprite.source) + row * stride_bytes;
                    let source = self
                        .data
                        .get(start..start + width)
                        .ok_or("Strided sprite exceeds executable data segment")?;
                    packed.extend_from_slice(source);
                }
                self.compositor
                    .draw(&mut self.screen, offset, dimensions, &packed)?;
            }
        }
        self.compose_actors()?;
        Ok(outcome)
    }

    /// Scene two uses inertia and buoyancy instead of ordinary jumping.
    pub(crate) fn update_swimming(
        &mut self,
        controls: Controls,
        tick: u16,
        accepted_ax: u16,
    ) -> Result<(UpdateOutcome, crate::swimming::SwimmingWork), String> {
        let previous_dimensions = word(&self.data, 0x561);
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 0x57f, accepted_ax);
        let suppress_air = !self.harm_enabled(Harm::Air);
        let step = crate::swimming::advance_with_damage(
            &mut self.data,
            controls,
            tick,
            suppress_air,
            &mut self.sound,
        )?;
        if let Some((offset, sprite)) = step.bubble {
            let dimensions = Dimensions(sprite.dimensions);
            let background = self.screen.and_rect(
                offset,
                dimensions,
                sprite_bytes(&self.data, sprite.source, dimensions)?,
            )?;
            self.data[0x0e..0x0e + background.len()].copy_from_slice(&background);
        }
        self.compositor.suspend(&mut self.screen)?;
        self.player = Player::from_data(&self.data)?;
        self.air_warning = step.air_warning;
        let Draw::Sprite(sprite) = step.draw else {
            return Err("Swimming update requires its external sprite".into());
        };
        let dimensions = Dimensions(sprite.dimensions);
        self.compositor.draw(
            &mut self.screen,
            pixel_offset(self.player.x, self.player.y),
            dimensions,
            sprite_bytes(&self.data, sprite.source, dimensions)?,
        )?;
        let suppress_eels = !self.harm_enabled(Harm::Eels);
        #[cfg(feature = "dev-tools")]
        let contact = {
            let disabled = self.dev_options.actor_disabled(Actor::Eels);
            self.room_actor(|data, screen, _sound| {
                crate::fishbowl::contacts_dev_with_damage(
                    data,
                    screen,
                    tick,
                    disabled,
                    suppress_eels,
                    _sound,
                )
            })?
        };
        #[cfg(not(feature = "dev-tools"))]
        let contact = self.room_actor(|data, screen, _sound| {
            crate::fishbowl::contacts_with_damage(data, screen, tick, suppress_eels, _sound)
        })?;
        if contact.wait_ticks != 0 {
            crate::fishbowl::draw_eel_failure(&mut self.data, &mut self.screen)?;
            self.sound.eel_start(&mut self.data);
            self.damage_effect = Some(DamageEffect {
                since: tick,
                duration: contact.wait_ticks,
                kind: EffectKind::Eel {
                    phase: EelPhase::Restart,
                    palette_tick: tick,
                    background: self.air_warning,
                },
            });
            self.compose_actors()?;
        }
        let work = crate::swimming::SwimmingWork {
            previous_dimensions,
            dimensions: sprite.dimensions,
            bubble: step.bubble.is_some(),
            // CS:0b72..0b95: FD near-surface wait, after movement and before
            // restoring/drawing the player. Physical timing uses the XT clock.
            surface_wait: self.data[0x697] == 0xfd && self.player.y <= 48,
            contacts: contact,
        };
        Ok((
            UpdateOutcome {
                draw: step.draw,
                landed: false,
                support: crate::player::Support::None,
                lower_support: false,
                floor_mark: false,
                airborne_support_checked: false,
                room_surface_hit: false,
                edge_reset: false,
                bin_contact_checked: false,
                forced_controls: None,
                projectile_contact: false,
            },
            work,
        ))
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.presented
    }

    /// Original scene numbers: alley zero, fishbowl one, underwater two,
    /// bookcase three, cheese four, birdcage five, milk six and hearts seven.
    pub fn scene(&self) -> u16 {
        word(&self.data, 4)
    }

    pub(crate) fn saved_alley_return(&self) -> bool {
        self.data[0x419] != 0
    }

    #[cfg(feature = "dev-tools")]
    pub fn selection_trace_state(&self) -> SelectionTraceState {
        SelectionTraceState {
            last_room: word(&self.data, 0x41d),
            previous_room: word(&self.data, 0x41f),
            random_state: word(&self.data, 0x2ae5),
            difficulty: word(&self.data, 8),
            pending_hearts: self.data[0x418] != 0,
        }
    }

    pub(crate) fn advance_room_tone(&mut self, tick: u16) {
        self.sound.ambient(&mut self.data, tick);
    }

    pub(crate) fn latch_action(&mut self, held: bool) {
        // CS:12d3..1334 converts the Alt key's 00/80 state to 00/10.
        self.data[0x69a] = if held { 0 } else { 16 };
    }

    pub(crate) fn latch_controls(&mut self, controls: Controls) {
        self.data[0x698] = controls.x as u8;
        self.data[0x699] = controls.y as u8;
    }

    pub(crate) fn requested_controls(&self) -> Controls {
        Controls {
            x: self.data[0x698] as i8,
            y: self.data[0x699] as i8,
        }
    }

    pub(crate) fn advance_hearts_arrow(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::HeartsArrows) {
            return Ok(crate::hearts::HeartStep::default());
        }
        self.room_actor(|data, screen, _sound| {
            crate::hearts::advance_arrow(data, screen, tick, _sound)
        })
    }

    pub(crate) fn pickup_hearts_item(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        self.room_actor(|data, screen, _sound| {
            crate::hearts::pickup_item(data, screen, tick, _sound)
        })
    }

    pub(crate) fn place_hearts_item(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::HeartsRivals);
            self.room_actor(|data, screen, _sound| {
                crate::hearts::place_item_dev(data, screen, tick, disabled, _sound)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.room_actor(|data, screen, _sound| {
                crate::hearts::place_item(data, screen, tick, _sound)
            })
        }
    }

    pub(crate) fn advance_hearts_cat(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::HeartsRivals);
            self.room_actor(|data, screen, _sound| {
                crate::hearts::advance_cat_dev(data, screen, tick, disabled, _sound)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.room_actor(|data, screen, _sound| {
                crate::hearts::advance_cat(data, screen, tick, _sound)
            })
        }
    }

    fn reset_sound_state(&mut self) {
        self.air_warning = 0;
        self.sound.stop();
        // CS:58bd initialization; stopping the device is separate from counters.
        for (offset, value) in [
            (0x5927, 0x80),
            (0x5928, 0),
            (0x5929, 0),
            (0x592c, 255),
            (0x592d, 0),
            (0x5920, 0),
            (0x5b07, 0),
            (0x5b0e, 1),
        ] {
            self.data[offset] = value;
        }
        for (offset, value) in [(0x592a, 0x500), (0x5b08, 0), (0x5b0c, 1)] {
            set_word(&mut self.data, offset, value);
        }
    }

    pub(crate) fn completion_waiting(&self) -> bool {
        self.completion.is_some() || self.hearts_completion.is_some() || self.failure.is_some()
    }

    /// Preview the ordinary birdcage success presentation without playing its
    /// objective. This diagnostic uses the same return path as a bird capture.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_completion(&mut self, tick: u16) -> Result<(), String> {
        self.enter_birdcage(tick)?;
        self.data[0x553] = 1;
        self.finish_scene(tick)?;
        Ok(())
    }

    pub(crate) fn failure_poll(&self) -> Option<crate::failure::Poll> {
        self.failure
            .as_ref()
            .and_then(crate::failure::Failure::poll)
    }

    pub(crate) fn completion_poll(&self) -> Option<crate::completion::Poll> {
        self.completion
            .as_ref()
            .and_then(crate::completion::Completion::poll)
            .or_else(|| {
                self.hearts_completion
                    .as_ref()
                    .and_then(crate::hearts_completion::HeartsCompletion::poll)
            })
    }

    pub(crate) fn finish_failure_drawing(&mut self, tick: u16) {
        if let Some(failure) = self.failure.as_mut() {
            failure.finish_drawing(&mut self.data, tick);
        }
    }

    pub(crate) fn advance_completion(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<crate::completion::Work, String> {
        let work = if let Some(completion) = self.completion.as_mut() {
            completion.advance(
                &mut self.data,
                &mut self.screen,
                tick,
                retrace,
                &mut self.sound,
            )?
        } else if let Some(completion) = self.hearts_completion.as_mut() {
            completion.advance(
                &mut self.data,
                &mut self.screen,
                tick,
                retrace,
                &mut self.sound,
            )?
        } else if let Some(failure) = self.failure.as_mut() {
            failure.advance(&mut self.data, &mut self.screen, tick, &mut self.sound)?
        } else {
            return Err("No completion presentation is active".into());
        };
        self.presented = self.screen.clone();
        if work.complete {
            self.completion = None;
            self.hearts_completion = None;
            self.failure = None;
            if self.data[0x1f80] == 0 {
                self.finish_run();
            } else {
                self.return_to_alley()?;
            }
        }
        Ok(work)
    }

    /// Enter the currently supported room directly for diagnostic play.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_birdcage(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_birdcage()
    }

    /// Enter the furnished fishbowl room directly for diagnostic play.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_fishbowl(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_fishbowl()
    }

    fn setup_fishbowl(&mut self) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 1);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::room::draw_fishbowl_room(&mut self.data, &mut self.screen)?;
        crate::room::initialize_room_player(&mut self.data, 1)?;
        crate::room::initialize_broom(&mut self.data)?;
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Direct underwater preview preserves the same alley return coordinates.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_underwater(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_underwater(tick)
    }

    fn setup_underwater(&mut self, tick: u16) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 2);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::fishbowl::draw_underwater(&mut self.data, &mut self.screen)?;
        #[cfg(feature = "dev-tools")]
        crate::fishbowl::initialize_underwater_dev(
            &mut self.data,
            &mut self.screen,
            self.dev_options.actor_disabled(Actor::Eels),
        )?;
        #[cfg(not(feature = "dev-tools"))]
        crate::fishbowl::initialize_underwater(&mut self.data, &mut self.screen)?;
        crate::swimming::initialize_player(&mut self.data, tick)?;
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter the bookcase room directly for diagnostic play.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_bookcase(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_bookcase()
    }

    fn setup_bookcase(&mut self) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 3);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::room::draw_bookcase_room(&mut self.data, &mut self.screen)?;
        crate::room::initialize_room_player(&mut self.data, 3)?;
        crate::room::initialize_broom(&mut self.data)?;
        crate::bookcase::initialize(&mut self.data, &mut self.screen)?;
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::Spider) {
            crate::bookcase::remove_spider(&mut self.data, &mut self.screen)?;
        }
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter the cheese room directly for diagnostic play.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_cheese(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_cheese()
    }

    fn setup_cheese(&mut self) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 4);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::room::draw_cheese_room(&mut self.data, &mut self.screen)?;
        crate::room::initialize_room_player(&mut self.data, 4)?;
        crate::room::initialize_broom(&mut self.data)?;
        crate::cheese::initialize(&mut self.data, &mut self.screen)?;
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter the milk bowls and sleeping dogs room directly for diagnostic play.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_milk(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_milk()
    }

    fn setup_milk(&mut self) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        self.milk_player_image = None;
        set_word(&mut self.data, 4, 6);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::room::draw_milk_room(&mut self.data, &mut self.screen)?;
        crate::milk::initialize(&mut self.data, &mut self.screen)?;
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::MilkDogs) {
            crate::milk::remove_dogs(&mut self.data, &mut self.screen)?;
        }
        crate::room::initialize_room_player(&mut self.data, 6)?;
        crate::room::initialize_broom(&mut self.data)?;
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter scene seven directly for diagnostic play, saving the same return
    /// coordinates as a window entry. Normal progression enters via a window.
    #[cfg(any(feature = "dev-tools", test))]
    pub fn enter_hearts(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_hearts(tick)
    }

    fn setup_hearts(&mut self, tick: u16) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 7);
        set_word(&mut self.data, 6, 0);
        self.data[0x418] = 0;
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        #[cfg(feature = "dev-tools")]
        crate::hearts::initialize_dev(
            &mut self.data,
            &mut self.screen,
            tick,
            self.dev_options.actor_disabled(Actor::HeartsRivals),
        )?;
        #[cfg(not(feature = "dev-tools"))]
        crate::hearts::initialize(&mut self.data, &mut self.screen, tick)?;
        crate::room::initialize_room_player(&mut self.data, 7)?;
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    fn setup_birdcage(&mut self) -> Result<(), String> {
        self.completion = None;
        self.hearts_completion = None;
        set_word(&mut self.data, 4, 5);
        set_word(&mut self.data, 6, 0);
        self.compositor = PlayerCompositor::default();
        self.bin_image = None;
        self.mouse_effect = None;
        crate::room::draw_birdcage_room(&mut self.data, &mut self.screen)?;
        crate::birdcage::initialize(&mut self.data, &mut self.screen)?;
        crate::room::initialize_room_player(&mut self.data, 5)?;
        crate::room::initialize_broom(&mut self.data)?;
        // Start with a dormant roaming dog; ordinary spawning follows its gates.
        crate::dog::initialize(&mut self.data)?;
        self.dog_image = None;
        self.projectile_image = None;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        // CS:07a1 saves the initial background; the first accepted player call
        // supplies the visible cat. Do not draw an extra entry frame here.
        self.compose_actors()
    }

    /// Run a direct-graphics room helper against scenery without a stale cat
    /// background. Recompose its exact current image without idle RNG draws.
    fn room_actor<T>(
        &mut self,
        action: impl FnOnce(&mut [u8], &mut Framebuffer, &mut crate::sound::Sound) -> Result<T, String>,
    ) -> Result<T, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
        if self.scene() == 6 {
            crate::milk::restore_drinking(&mut self.data, &mut self.screen)?;
        }
        let result = action(&mut self.data, &mut self.screen, &mut self.sound)?;
        self.player.apply_actor_state(&self.data)?;
        let image = if self.scene() == 6 && self.data[0x44bd] != 0 {
            if image.is_some() {
                self.milk_player_image = image;
            }
            if self.data[0x43e0] != 0 {
                crate::milk::draw_drinking(&mut self.data, &mut self.screen)?;
            }
            None
        } else {
            image.or_else(|| self.milk_player_image.take())
        };
        if let Some((dimensions, source)) = image {
            self.compositor.draw(
                &mut self.screen,
                pixel_offset(self.player.x, self.player.y),
                dimensions,
                &source,
            )?;
        }
        self.compose_actors()?;
        Ok(result)
    }

    pub(crate) fn advance_milk_drinking(
        &mut self,
        tick: u16,
    ) -> Result<crate::milk::DrinkStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::Broom);
            self.room_actor(|data, screen, _sound| {
                crate::milk::advance_drinking_with_actor_mask(data, screen, tick, disabled, _sound)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        self.room_actor(|data, screen, _sound| {
            crate::milk::advance_drinking(data, screen, tick, _sound)
        })
    }

    pub(crate) fn advance_milk_attention(
        &mut self,
        tick: u16,
    ) -> Result<crate::milk::AttentionStep, String> {
        self.advance_milk_attention_timed(tick, &mut |_, _| tick)
    }

    pub(crate) fn advance_milk_drinking_timed(
        &mut self,
        tick: u16,
        work: &mut impl FnMut(crate::milk::Work, &mut crate::sound::Sound) -> u16,
    ) -> Result<crate::milk::DrinkStep, String> {
        #[cfg(feature = "dev-tools")]
        let disabled = self.dev_options.actor_disabled(Actor::Broom);
        #[cfg(not(feature = "dev-tools"))]
        let disabled = false;
        self.room_actor(|data, screen, sound| {
            crate::milk::advance_drinking_timed(data, screen, tick, disabled, sound, work)
        })
    }

    pub(crate) fn advance_milk_attention_timed(
        &mut self,
        tick: u16,
        work: &mut impl FnMut(crate::milk::Work, &mut crate::sound::Sound) -> u16,
    ) -> Result<crate::milk::AttentionStep, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::MilkDogs) {
            return Ok(crate::milk::AttentionStep::default());
        }
        let suppress = !self.harm_enabled(Harm::MilkDogs);
        let step = self.room_actor(|data, screen, sound| {
            crate::milk::advance_attention_timed(data, screen, tick, suppress, &mut |event| {
                work(event, sound);
            })
        })?;
        if step.warning {
            let now = work(crate::milk::Work::End, &mut self.sound);
            self.sound.milk_warning(&mut self.data, now);
        }
        if let Some(slot) = step.lethal_slot {
            self.compositor.restore(&mut self.screen)?;
            crate::milk::restore_drinking(&mut self.data, &mut self.screen)?;
            self.data[0x44bd] = 0;
            self.milk_player_image = None;
            let offset = word(&self.data, 0x4411 + slot * 2);
            set_word(&mut self.data, 0x44da, offset);
            self.data[0x0e..0x90].fill(0xaa);
            self.screen
                .copy_rect(offset, Dimensions(0x0d05), &[0xaa; 130])?;
            crate::dog::begin_death(&mut self.data)?;
            self.player = Player::from_data(&self.data)?;
            self.compose_actors()?;
        }
        Ok(step)
    }

    pub(crate) fn advance_fishbowl(
        &mut self,
        tick: u16,
    ) -> Result<crate::fishbowl::BowlStep, String> {
        self.room_actor(|data, screen, _sound| crate::fishbowl::advance_bowl(data, screen, tick))
    }

    pub(crate) fn advance_underwater_actors(
        &mut self,
        tick: u16,
    ) -> Result<crate::fishbowl::ActorStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::Eels);
            self.room_actor(|data, screen, _sound| {
                crate::fishbowl::advance_actors_dev(data, screen, tick, disabled)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.room_actor(|data, screen, _sound| {
                crate::fishbowl::advance_actors(data, screen, tick)
            })
        }
    }

    pub(crate) fn advance_water_surface(
        &mut self,
        tick: u16,
    ) -> Result<crate::fishbowl::SurfaceStep, String> {
        self.room_actor(|data, screen, _sound| crate::fishbowl::advance_surface(data, screen, tick))
    }

    pub(crate) fn advance_bookcase_spider(
        &mut self,
        tick: u16,
    ) -> Result<crate::bookcase::SpiderStep, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::Spider) {
            return Ok(crate::bookcase::SpiderStep::default());
        }
        let suppress = !self.harm_enabled(Harm::Spider);
        #[cfg(feature = "dev-tools")]
        let step = {
            let disabled = self.dev_options.actor_disabled(Actor::Broom);
            self.room_actor(|data, screen, _sound| {
                crate::bookcase::advance_spider_dev_with_damage(
                    data, screen, tick, disabled, suppress,
                )
            })?
        };
        #[cfg(not(feature = "dev-tools"))]
        let step = self.room_actor(|data, screen, _sound| {
            crate::bookcase::advance_spider_with_damage(data, screen, tick, suppress)
        })?;
        if step.failure_pending {
            crate::bookcase::draw_spider_failure(&mut self.data, &mut self.screen)?;
            self.sound.spider_start(&mut self.data);
            self.damage_effect = Some(DamageEffect {
                since: tick,
                duration: step.wait_ticks,
                kind: EffectKind::Spider,
            });
            self.compose_actors()?;
        }
        Ok(step)
    }

    pub(crate) fn start_spider_effect_wait(&mut self, tick: u16) {
        if let Some(DamageEffect {
            since,
            kind: EffectKind::Spider,
            ..
        }) = self.damage_effect.as_mut()
        {
            *since = tick;
            set_word(&mut self.data, 0x39c8, tick);
        }
    }

    pub(crate) fn start_eel_effect_wait(&mut self, tick: u16) {
        if let Some(DamageEffect {
            since,
            kind: EffectKind::Eel { palette_tick, .. },
            ..
        }) = self.damage_effect.as_mut()
        {
            *since = tick;
            *palette_tick = tick;
            set_word(&mut self.data, 0x3509, tick);
        }
    }

    pub(crate) fn advance_bookcase_targets(
        &mut self,
        tick: u16,
    ) -> Result<crate::bookcase::TargetStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::Spider);
            self.room_actor(|data, screen, _sound| {
                crate::bookcase::advance_targets_dev(data, screen, tick, disabled, _sound)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.room_actor(|data, screen, _sound| {
                crate::bookcase::advance_targets(data, screen, tick, _sound)
            })
        }
    }

    pub(crate) fn advance_cheese_mice(
        &mut self,
        tick: u16,
    ) -> Result<crate::cheese::MouseStep, String> {
        #[cfg(feature = "dev-tools")]
        {
            let disabled = self.dev_options.actor_disabled(Actor::Broom);
            self.room_actor(|data, screen, _sound| {
                crate::cheese::advance_mice_dev(data, screen, tick, disabled, _sound)
            })
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.room_actor(|data, screen, _sound| {
                crate::cheese::advance_mice(data, screen, tick, _sound)
            })
        }
    }

    pub(crate) fn advance_cheese_transfer(
        &mut self,
        tick: u16,
    ) -> Result<crate::cheese::TransferStep, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
        #[cfg(feature = "dev-tools")]
        let step = crate::cheese::advance_transfer_dev(
            &mut self.data,
            &mut self.screen,
            tick,
            self.dev_options.actor_disabled(Actor::Broom),
        )?;
        #[cfg(not(feature = "dev-tools"))]
        let step = crate::cheese::advance_transfer(&mut self.data, &mut self.screen, tick)?;
        self.player.apply_actor_state(&self.data)?;
        // The hole animation owns the image until its final background-save.
        // The next accepted player call supplies the visible cat again.
        if !step.started && !step.active && !step.finished {
            if let Some((dimensions, source)) = image {
                self.compositor.draw(
                    &mut self.screen,
                    pixel_offset(self.player.x, self.player.y),
                    dimensions,
                    &source,
                )?;
            }
        }
        self.compose_actors()?;
        Ok(step)
    }

    pub(crate) fn advance_room_cage(
        &mut self,
        tick: u16,
    ) -> Result<crate::birdcage::CageStep, String> {
        self.room_actor(|data, screen, _sound| {
            crate::birdcage::advance_cage(data, screen, tick, _sound)
        })
    }

    pub(crate) fn advance_room_fall(
        &mut self,
        tick: u16,
    ) -> Result<crate::birdcage::FallStep, String> {
        self.room_actor(|data, screen, sound| {
            crate::birdcage::advance_fall(data, screen, tick, sound)
        })
    }

    pub(crate) fn advance_room_bird(
        &mut self,
        tick: u16,
    ) -> Result<crate::birdcage::BirdStep, String> {
        self.room_actor(|data, screen, _sound| {
            crate::birdcage::advance_bird(data, screen, tick, _sound)
        })
    }

    pub(crate) fn advance_room_broom(
        &mut self,
        tick: u16,
    ) -> Result<Option<crate::room::BroomUpdate>, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::Broom) {
            return Ok(None);
        }
        self.room_actor(|data, screen, _sound| crate::room::advance_broom(data, screen, tick))
    }

    pub(crate) fn advance_windows(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<crate::windows::Update, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
        let timing = crate::windows::HoldTiming::default();
        #[cfg(feature = "dev-tools")]
        let timing = if self.dev_options.original_window_timing {
            crate::windows::HoldTiming::Original
        } else {
            timing
        };
        #[cfg(feature = "dev-tools")]
        let projectiles_enabled = !self.dev_options.actor_disabled(Actor::WindowProjectiles);
        #[cfg(not(feature = "dev-tools"))]
        let projectiles_enabled = true;
        let update = crate::windows::advance_with_projectiles(
            &mut self.data,
            &mut self.screen,
            &self.player,
            tick,
            retrace,
            timing,
            projectiles_enabled,
        )?;
        if let Some((dimensions, source)) = image {
            self.compositor.draw(
                &mut self.screen,
                pixel_offset(self.player.x, self.player.y),
                dimensions,
                &source,
            )?;
        }
        self.compose_actors()?;
        Ok(update)
    }

    pub(crate) fn take_selection_work(&mut self) -> Option<crate::windows::SelectionWork> {
        self.selection_work.take()
    }

    /// Dispatch only after the original caller's remaining actor work finishes.
    pub(crate) fn finish_scene(&mut self, tick: u16) -> Result<bool, String> {
        if self.completion_waiting() {
            return Ok(false);
        }
        if self.scene() == 0 {
            if self.data[0x551] == 0 {
                return Ok(false);
            }
            self.player.write_data(&mut self.data)?;
            // Dispatch the original weighted selector directly. Its
            // pending-hearts override takes priority over ordinary rooms.
            let (candidate, work) =
                crate::windows::select_room(&mut self.data, &self.player, tick)?;
            match candidate {
                7 => self.setup_hearts(tick)?,
                1 => self.setup_fishbowl()?,
                3 => self.setup_bookcase()?,
                4 => self.setup_cheese()?,
                5 => self.setup_birdcage()?,
                6 => self.setup_milk()?,
                _ => return Err("Window selected an unsupported scene".into()),
            }
            self.selection_work = Some(work);
            return Ok(true);
        }
        // Original scene-one caller tests bowl entry before room termination.
        if self.scene() == 1 && self.data[0x554] != 0 {
            self.setup_underwater(tick)?;
            return Ok(true);
        }
        if self.data[0x552] != 0 {
            self.data[0x419] = 0;
        }
        if self.data[0x551] == 0 && self.data[0x553] == 0 && self.data[0x552] == 0 {
            return Ok(false);
        }
        if self.scene() == 7 {
            self.player.write_data(&mut self.data)?;
            set_word(&mut self.data, 6, 7);
            self.compositor = PlayerCompositor::default();
            self.hearts_completion = Some(crate::hearts_completion::HeartsCompletion::new(
                self.player.x,
                self.player.y,
                self.data[0x553] != 0,
            )?);
            return Ok(false);
        }
        if self.data[0x553] != 0 {
            self.player.write_data(&mut self.data)?;
            let scene = self.scene();
            set_word(&mut self.data, 6, scene);
            self.compositor = PlayerCompositor::default();
            self.completion = Some(crate::completion::Completion::new(
                self.player.x,
                self.player.y,
                scene,
            )?);
            return Ok(false);
        }
        if self.data[0x552] != 0 {
            self.failure = Some(crate::failure::Failure::new(
                self.player.x,
                self.player.y,
                self.infinite_lives(),
            ));
            self.dog_image = None;
            self.projectile_image = None;
            self.compositor = PlayerCompositor::default();
            return Ok(false);
        }
        self.return_to_alley()?;
        Ok(true)
    }

    fn return_to_alley(&mut self) -> Result<(), String> {
        // CS:2210 hides the cat; its previous rendered image must also disappear.
        self.bin_image = None;
        self.milk_player_image = None;
        self.dog_image = None;
        self.projectile_image = None;
        self.player.write_data(&mut self.data)?;
        let scene = self.scene();
        set_word(&mut self.data, 6, scene);
        set_word(&mut self.data, 4, 0);
        self.compositor = PlayerCompositor::default();
        initialize_alley(&mut self.data, &mut self.screen)?;
        draw_ground_objects(&mut self.data, &mut self.screen)?;
        self.data[0x1f81] = 0xff;
        draw_hud(&mut self.data, &mut self.screen)?;
        crate::dog::initialize(&mut self.data)?;
        crate::bin_cat::initialize(&mut self.data)?;
        crate::windows::initialize(&mut self.data)?;
        let saved_x = word(&self.data, 1);
        self.mice = Mice::initialize(&mut self.data, saved_x)?;
        crate::room::initialize_room_player(&mut self.data, 0)?;
        if self.data[0x419] != 0 {
            self.data[0x550] = 2;
            self.data[0x576] = 1;
            self.data[0x578] = 0x20;
        } else {
            self.initialize_ground_entry();
        }
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()?;
        Ok(())
    }

    /// Scheduled scenery work runs after the player, even on passes which
    /// reject movement. Present only a finished band and a recomposed cat.
    pub(crate) fn advance_scenery(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<Option<crate::clothes::Scroll>, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        let result = crate::clothes::advance(
            &mut self.data,
            &mut self.screen,
            &mut self.player,
            tick,
            retrace,
        );
        if let Some((dimensions, source)) = image {
            self.compositor.draw(
                &mut self.screen,
                pixel_offset(self.player.x, self.player.y),
                dimensions,
                &source,
            )?;
        }
        self.compose_actors()?;
        result
    }

    fn draw_actor(&mut self, image: Image) -> Result<(), String> {
        let dimensions = Dimensions(image.dimensions);
        let source = sprite_bytes(&self.data, image.source, dimensions)?;
        match image.blend {
            Blend::And => self.presented.and_rect(image.offset, dimensions, source)?,
            Blend::ZeroTransparent => {
                self.presented
                    .zero_transparent_rect(image.offset, dimensions, source)?
            }
        };
        Ok(())
    }

    /// Keep actor pixels out of the player's saved background and scrolling
    /// bands. Compose finished layers instead of emulating stale draw buffers.
    fn compose_actors(&mut self) -> Result<(), String> {
        self.presented = self.screen.clone();
        if let Some(image) = &self.dog_image {
            if image.opaque {
                self.presented
                    .copy_rect(image.offset, image.dimensions, &image.pixels)?;
            } else {
                self.presented.zero_transparent_rect(
                    image.offset,
                    image.dimensions,
                    &image.pixels,
                )?;
            }
        }
        if let Some(image) = self.projectile_image {
            self.draw_projectile(image)?;
        }
        if self.scene() != 0 {
            return Ok(());
        }
        let bin_image = self.bin_image;
        #[cfg(feature = "dev-tools")]
        let bin_image = bin_image.filter(|_| !self.dev_options.actor_disabled(Actor::BinCat));
        if let Some(image) = bin_image {
            self.draw_actor(Image {
                source: image.source,
                dimensions: image.dimensions.0,
                offset: image.offset,
                blend: Blend::ZeroTransparent,
            })?;
        }
        #[cfg(feature = "dev-tools")]
        let show_mice = !self.dev_options.actor_disabled(Actor::AlleyMice);
        #[cfg(not(feature = "dev-tools"))]
        let show_mice = true;
        if show_mice {
            for image in (*self.mice.images()).into_iter().flatten() {
                self.draw_actor(image)?;
            }
            if let Some(image) = self.mouse_effect {
                self.draw_actor(image)?;
            }
        }
        if let Some(DamageEffect {
            kind: EffectKind::Projectile(image),
            ..
        }) = self.damage_effect
        {
            self.draw_projectile(image)?;
        }
        Ok(())
    }

    pub(crate) fn advance_bin_cat(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<Option<BinAdvance>, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::BinCat) {
            return Ok(None);
        }
        let started = self.data[0x1d59] == 0;
        let previous_rows = (word(&self.data, 0x1d66) >> 8) as u8;
        let Some(update) = crate::bin_cat::advance(&mut self.data, &self.player, tick, retrace)?
        else {
            return Ok(None);
        };
        if update.restore_previous {
            self.bin_image = None;
        }
        if let Some(image) = update.sprite {
            self.bin_image = Some(image);
            set_word(&mut self.data, 0x1d60, image.offset);
            set_word(&mut self.data, 0x1d66, image.dimensions.0);
        }
        self.compose_actors()?;
        Ok(
            (update.sprite.is_some() || update.restore_previous).then_some(BinAdvance {
                started,
                draw_rows: update
                    .sprite
                    .map_or(0, |image| image.dimensions.rows() as u8),
                restore_rows: if update.restore_previous {
                    previous_rows
                } else {
                    0
                },
            }),
        )
    }

    pub(crate) fn advance_mice(&mut self, tick: u16) -> Result<MouseAdvance, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::AlleyMice) {
            return Ok(MouseAdvance::default());
        }
        let Some(update) = self.mice.advance(&mut self.data, &self.player, tick)? else {
            return Ok(MouseAdvance::default());
        };
        let paused = match update.contact {
            Contact::Knockdown { effect } => {
                self.player.knock_down_from_mouse();
                self.mouse_effect = Some(effect);
                true
            }
            Contact::Caught { points } => {
                award_points(&mut self.data, points)?;
                let image = self.compositor.suspend(&mut self.screen)?;
                draw_hud(&mut self.data, &mut self.screen)?;
                if let Some((dimensions, source)) = image {
                    self.compositor.draw(
                        &mut self.screen,
                        pixel_offset(self.player.x, self.player.y),
                        dimensions,
                        &source,
                    )?;
                }
                // CS:593b starts the first note before its tick-driven continuation.
                self.sound.request(&mut self.data, tick, 1000, 750);
                false
            }
            Contact::None | Contact::Blocked => false,
        };
        self.compose_actors()?;
        Ok(MouseAdvance {
            paused,
            caught: matches!(update.contact, Contact::Caught { .. }),
            redraw: update.changed,
        })
    }

    pub(crate) fn native_sound_enabled(&self) -> bool {
        self.data[0] != 0
    }

    pub(crate) fn mouse_effect_random(&mut self, tick: u16) -> bool {
        if self.data[0] != 0 && word(&self.data, 0x5a14) != tick {
            set_word(&mut self.data, 0x5a14, tick);
            let divisor = (next_random(&mut self.data) & 0x70) + 0x200;
            self.sound.tone(divisor, crate::sound::Category::Effects);
            true
        } else {
            false
        }
    }

    pub(crate) fn start_mouse_effect_wait(&mut self, tick: u16) -> bool {
        // CS:25e4 samples the baseline after composing the temporary image.
        set_word(&mut self.data, 0x1f65, tick);
        self.mouse_effect_random(tick)
    }

    pub(crate) fn finish_mouse_effect(&mut self) -> Result<(), String> {
        self.mouse_effect = None;
        self.sound.stop();
        self.compose_actors()
    }

    pub fn rgba(&self) -> Vec<u8> {
        let presentation = self.hearts_completion.as_ref();
        let palette_scene = if self.failure.is_some()
            || self.game_over
            || self.completion.is_some()
            || presentation.is_some_and(|completion| completion.uses_alley_palette())
        {
            0
        } else {
            usize::from(self.scene())
        };
        let mut colors = if palette_scene == 0 {
            self.machine.colors()
        } else if self.machine == Machine::Pcjr {
            [
                hardware_color(0),
                hardware_color(self.data[0x183b + palette_scene]),
                hardware_color(self.data[0x1843 + palette_scene]),
                hardware_color(self.data[0x184b + palette_scene]),
            ]
        } else if self.data[0x1853 + palette_scene] & 1 == 0
            && !presentation.is_some_and(|completion| completion.presentation_palette())
        {
            [
                hardware_color(0),
                hardware_color(2),
                hardware_color(4),
                hardware_color(6),
            ]
        } else {
            self.machine.colors()
        };
        let background = if self.failure.is_some() || self.game_over {
            0
        } else if let Some(completion) = presentation {
            completion.background()
        } else if let Some(completion) = self.completion.as_ref() {
            completion.background()
        } else if self.scene() == 7 && word(&self.data, 0x2e8d) < 8 {
            1
        } else if self.scene() == 2 {
            match self.damage_effect.as_ref().map(|effect| effect.kind) {
                Some(EffectKind::Eel { background, .. }) => background,
                _ => self.air_warning,
            }
        } else {
            0
        };
        if background != 0 {
            // BIOS background/border requests use hardware color numbers,
            // independently of scene palette indices one through three.
            colors[0] = hardware_color(background);
        }
        self.presented
            .indices()
            .into_iter()
            .flat_map(|index| colors[usize::from(index)])
            .collect()
    }
}

fn hardware_color(color: u8) -> [u8; 4] {
    let bright = if color & 8 != 0 { 85 } else { 0 };
    let mut rgb = [
        if color & 4 != 0 { 170 } else { 0 },
        if color & 2 != 0 { 170 } else { 0 },
        if color & 1 != 0 { 170 } else { 0 },
        255,
    ];
    for channel in &mut rgb[..3] {
        *channel += bright;
    }
    if color == 6 {
        rgb[1] = 85;
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_work_is_consumed_once_and_restart_discards_pending_work() {
        let mut alley = Alley::synthetic();
        let work = crate::windows::SelectionWork {
            attempts: 2,
            extra_draws: 1,
            history_rejections: 1,
        };
        alley.selection_work = Some(work);
        assert_eq!(alley.take_selection_work().unwrap().attempts, 2);
        assert!(alley.take_selection_work().is_none());
        alley.selection_work = Some(work);
        set_word(&mut alley.data, 0x28a2, 0xffff);
        set_word(&mut alley.data, 0x5908, 0x7100);
        set_word(&mut alley.data, 0x590e, 0x7100);
        set_word(&mut alley.data, 0x7100, 0xffff);
        alley.reset_run(0, true).unwrap();
        assert!(alley.take_selection_work().is_none());
    }

    #[test]
    fn final_entry_idle_keeps_the_previous_image_offset_but_walking_moves_it() {
        for (x, direction) in [(0, 1), (304, 255)] {
            for walking in [false, true] {
                let mut alley = Alley::synthetic();
                alley.screen.fill_visible(0xff);
                alley.player = Player::at_alley_floor(x);
                alley.player.x = x;
                alley.player.entry_state = 1;
                alley.player.entry_delay = 0;
                alley.player.horizontal_direction = direction;
                alley.player.write_data(&mut alley.data).unwrap();
                alley.data[0x56f] = direction;
                set_word(&mut alley.data, 0x561, 0x0b02);
                alley.player = Player::from_data(&alley.data).unwrap();
                for index in 0..12 {
                    set_word(&mut alley.data, 0xf7a + index * 2, 0x3000);
                }
                for index in 0..8 {
                    set_word(&mut alley.data, 0xf92 + index * 2, 0x3000);
                }
                for index in 0..4 {
                    set_word(&mut alley.data, 0xfa2 + index * 2, 0x3000);
                }
                alley
                    .compositor
                    .draw(
                        &mut alley.screen,
                        pixel_offset(x, 180),
                        Dimensions(0x0b02),
                        &[0xff; 44],
                    )
                    .unwrap();
                alley
                    .update(Controls {
                        x: if walking { direction as i8 } else { 0 },
                        y: 0,
                    })
                    .unwrap();
                let next_x = if direction == 1 {
                    if walking { 16 } else { 8 }
                } else if walking {
                    288
                } else {
                    296
                };
                assert_eq!((alley.player.x, alley.player.entry_state), (next_x, 0));
                let dimensions = Dimensions(if walking { 0x0b03 } else { 0x0c02 });
                let mut expected = Framebuffer::default();
                expected.fill_visible(0xff);
                expected
                    .copy_rect(
                        pixel_offset(if walking { next_x } else { x }, 180),
                        dimensions,
                        &vec![0; dimensions.len()],
                    )
                    .unwrap();
                assert_eq!(alley.screen.bytes(), expected.bytes());
            }
        }
    }

    #[test]
    fn eel_deadline_waits_for_retrace_extra_pulses_and_palette() {
        let mut alley = Alley::synthetic();
        let since = u16::MAX - 5;
        let expired = since.wrapping_add(13);
        set_word(&mut alley.data, 4, 2);
        alley.sound.eel_start(&mut alley.data);
        alley.damage_effect = Some(DamageEffect {
            since,
            duration: 13,
            kind: EffectKind::Eel {
                phase: EelPhase::Poll,
                palette_tick: since,
                background: 0,
            },
        });
        // The native tick test is outside the retrace loop, including at wrap.
        assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
        assert!(!alley.advance_damage_effect(expired, 0, true).unwrap());
        assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
        assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
        assert_eq!(alley.data[0x5acf], 4);
        assert!(matches!(alley.damage_poll(), DamagePoll::EelRestart));
        // Palette parity uses the saved outer-loop BIOS sample, not the
        // newer odd tick supplied while finishing the sequence.
        assert_eq!(&alley.rgba()[..4], &hardware_color(15));
        assert!(alley.advance_damage_effect(expired, 0, false).unwrap());
        assert!(!alley.damage_waiting());
    }
    use crate::assets::word;

    #[test]
    fn later_title_promotes_score_and_resumes_without_reseeding_or_clearing_history() {
        for machine in [Machine::Cga, Machine::Pcjr] {
            let mut alley = Alley::synthetic();
            alley.machine = machine;
            alley.initial_difficulty = 3;
            set_word(&mut alley.data, 0x28a2, 0xffff);
            set_word(&mut alley.data, 0x590e, 0x7100);
            set_word(&mut alley.data, 0x7100, 0xffff);
            set_word(&mut alley.data, 0x6df8, 3);
            set_word(&mut alley.data, 8, 7);
            set_word(&mut alley.data, 0x2ae5, 0xbeef);
            set_word(&mut alley.data, 0x41d, 6);
            set_word(&mut alley.data, 0x41f, 3);
            alley.data[0x1f82] = 2;
            alley.data[0x1f89] = 1;
            alley.data[0x1f80] = 0;
            assert!(alley.game_over_splash().is_err());
            alley.screen.fill_visible(0x55);
            alley.finish_run();
            assert!(
                alley.framebuffer().bytes()[..8000]
                    .iter()
                    .all(|&byte| byte == 0x55)
            );
            let mut splash = alley.game_over_splash().unwrap();
            assert_eq!(splash.data()[0x1f82], 2);
            assert_eq!(splash.data()[0x1f89], 2);
            assert_eq!(splash.data()[0x1f80], 9);
            assert_eq!(word(splash.data(), 0x6df8), 3);
            assert!(alley.start_after_game_over(&splash).is_err());
            splash.advance(1, false, true).unwrap();
            let mut expected_data = alley.data.clone();
            set_word(&mut expected_data, 8, 3);
            set_word(&mut expected_data, 0x2ae5, word(splash.data(), 0x2ae5));
            let mut expected_frame = Framebuffer::default();
            initialize_alley(&mut expected_data, &mut expected_frame).unwrap();
            draw_ground_objects(&mut expected_data, &mut expected_frame).unwrap();
            alley.start_after_game_over(&splash).unwrap();
            assert!(!alley.game_over());
            assert_eq!(word(&alley.data, 8), 3);
            assert_eq!(alley.data[0x1f80], 3);
            assert_eq!(alley.data[0x1f82..0x1f89], [0; 7]);
            assert_eq!(alley.data[0x1f89], 2);
            assert_eq!((word(&alley.data, 0x41d), word(&alley.data, 0x41f)), (6, 3));
            assert_eq!(word(&alley.data, 0x2ae5), word(&expected_data, 0x2ae5));
            assert_eq!(
                (alley.player.entry_state, alley.player.entry_delay),
                (3, 12)
            );
        }
    }

    #[test]
    fn room_floor_mark_work_reports_only_a_newly_drawn_tile() {
        for (last, count, expected) in [(18, 0, true), (17, 2, false), (18, 4, false)] {
            let mut alley = Alley::synthetic();
            set_word(&mut alley.data, 4, 3);
            set_word(&mut alley.data, 0x32b6, last);
            alley.data[0x329f] = count;
            alley.data[0x32c2..0x32cc].fill(0x55);
            for frame in 0..6 {
                set_word(&mut alley.data, 0xf7a + frame * 2, 0x2000);
            }
            alley.data[0x2000..0x2042].fill(0xff);
            alley.player = Player::at_alley_floor(125);
            let outcome = alley.update_at_tick(Controls { x: 1, y: 0 }, 10).unwrap();
            assert_eq!(outcome.floor_mark, expected);
            assert_eq!(word(&alley.data, 0x32b6), 17);
            assert_eq!(alley.data[0x329f], count + u8::from(expected));
            assert_eq!(
                alley.screen.bytes()[0x1e22],
                if expected { 0x55 } else { 0 }
            );
            assert_eq!(alley.player.x, 127);
        }
    }

    #[test]
    fn menu_selection_sets_starting_skill_and_preserves_high_score_on_restart() {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 0x28a2, 0xffff);
        set_word(&mut alley.data, 0x5908, 0x7100);
        set_word(&mut alley.data, 0x590e, 0x7100);
        set_word(&mut alley.data, 0x7100, 0xffff);
        alley.data[0x1f82] = 2;
        alley.data[0x1f89] = 1;
        alley.mouse_effect = Some(Image {
            source: 0,
            dimensions: 0x0801,
            offset: 0,
            blend: Blend::And,
        });
        alley.start_new_game(3, 42).unwrap();
        assert!(alley.mouse_effect.is_none());
        assert_eq!(word(&alley.data, 8), 3);
        assert_eq!(word(&alley.data, 0x6df8), 3);
        assert_eq!(alley.data[0x1f80], 3);
        assert_eq!(alley.data[0x1f82..0x1f89], [0; 7]);
        assert_eq!(alley.data[0x1f89], 2);
        set_word(&mut alley.data, 8, 7);
        alley.restart(0).unwrap();
        assert_eq!(word(&alley.data, 8), 3);
        assert_eq!(word(&alley.data, 0x6df8), 3);
        assert!(alley.start_new_game(8, 0).is_err());
        assert_eq!(alley.initial_difficulty, 3);
    }

    #[test]
    fn alley_reconstruction_discards_the_previous_bin_cat_image() {
        for restart in [false, true] {
            for (x, y, height) in [(24, 146, 1), (24, 130, 13), (100, 138, 9)] {
                let mut alley = Alley::synthetic();
                // Minimal terminated scenery/decoration lists, with authored pixels.
                set_word(&mut alley.data, 0x28a2, 0xffff);
                set_word(&mut alley.data, 0x5908, 0x7100);
                set_word(&mut alley.data, 0x7100, 0xffff);
                alley.data[0x1cf0..0x1d24].fill(0x55);
                alley.data[0x1d59] = 13;
                set_word(&mut alley.data, 0x1d5a, 400);
                let dimensions = Dimensions((height << 8) | 2);
                alley.bin_image = Some(crate::bin_cat::VisibleSprite {
                    source: 0x1cf0,
                    dimensions,
                    offset: pixel_offset(x, y),
                });
                let previous_bin = alley.data[0x1028];
                if restart {
                    alley.data[0x1f80] = 0;
                    alley.finish_run();
                    alley.restart(0).unwrap();
                } else {
                    alley.return_to_alley().unwrap();
                }
                assert_eq!(alley.data[0x1d59], 0);
                assert_eq!(word(&alley.data, 0x1d5a), 400);
                assert_eq!(alley.data[0x1028], previous_bin);
                // The inactive cat must contribute no pixels, even while the
                // retrace/tick gates delay its next animation opportunity.
                assert!(
                    alley.framebuffer().bytes() == alley.screen.bytes(),
                    "inactive bin cat added pixels after alley reconstruction"
                );
                assert!(alley.bin_image.is_none());
                alley.advance_bin_cat(401, false).unwrap();
                alley.advance_bin_cat(400, true).unwrap();
                alley.compose_actors().unwrap();
                assert!(
                    alley.framebuffer().bytes() == alley.screen.bytes(),
                    "inactive bin cat added pixels after alley reconstruction"
                );
            }
        }
    }

    #[test]
    fn drinking_recomposition_preserves_scenery_and_restores_the_normal_cat() {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 4, 6);
        alley.player.x = 144;
        alley.player.y = 136;
        alley.player.vertical_coordinate = 186;
        alley.player.vertical_direction = 0;
        alley.data[0x43e0] = 1;
        alley.data[0x44c4] = 2;
        alley.data[0x4499] = 144;
        set_word(&mut alley.data, 0x4481, 144);
        set_word(&mut alley.data, 0x44a5, 0x410c);
        alley.data[0x410c..0x410c + 120].fill(0x55);
        alley.screen.fill_visible(0xaa);
        alley
            .compositor
            .draw(
                &mut alley.screen,
                pixel_offset(144, 136),
                Dimensions(0x0602),
                &[0; 24],
            )
            .unwrap();
        alley.compose_actors().unwrap();
        let normal = alley.framebuffer().bytes().to_vec();
        alley.latch_action(true);
        let drinking = alley.advance_milk_drinking(1).unwrap();
        assert!(drinking.drew);
        assert!(alley.milk_player_image.is_some());
        assert_eq!(alley.data[0x44bd], 1);
        let drink_frame = alley.framebuffer().bytes().to_vec();
        for _ in 0..3 {
            alley.room_actor(|_, _, _| Ok(())).unwrap();
            assert_eq!(alley.framebuffer().bytes().as_slice(), drink_frame);
        }
        alley.latch_action(false);
        alley.advance_milk_drinking(2).unwrap();
        assert_eq!(alley.data[0x44bd], 0);
        assert!(alley.milk_player_image.is_none());
        assert_eq!(alley.framebuffer().bytes().as_slice(), normal);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn original_window_timing_override_reaches_both_window_kinds() {
        for bonus in [0, 1] {
            for original in [false, true] {
                let mut alley = Alley::synthetic();
                let mut options = alley.dev_options;
                options.original_window_timing = original;
                options.set_disabled_actors("window-projectiles").unwrap();
                alley.set_dev_options(options).unwrap();
                set_word(&mut alley.data, 8, 6);
                set_word(&mut alley.data, 0x181e + 6 * 2, 1);
                set_word(&mut alley.data, 0x2ae5, 0x10);
                set_word(&mut alley.data, 0x1658, 104);
                alley.data[0x1662] = 88;
                alley.data[0x166a] = 1;
                alley.data[0x418] = bonus;
                alley.player.x = 200;
                alley.player.y = 96;
                alley.advance_windows(65530, false).unwrap();
                let delay = if original { 1 } else { 18 };
                assert_eq!(word(&alley.data, 0x166c), delay);
                assert_eq!(word(&alley.data, 0x181e + 6 * 2), 1);
                alley.data[0x1665] = 16;
                alley.data[0x166a] = 1;
                alley.advance_windows(65530, false).unwrap();
                for (elapsed, phase) in [(delay - 1, 15), (delay, 14)] {
                    alley.data[0x166a] = 1;
                    alley
                        .advance_windows(65530u16.wrapping_add(elapsed), false)
                        .unwrap();
                    assert_eq!(alley.data[0x1665], phase);
                }
            }
        }
    }

    #[test]
    fn milk_warning_starts_after_timed_eye_work() {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 4, 6);
        alley.player.y = 136;
        alley.player.vertical_coordinate = 186;
        set_word(&mut alley.data, 0x4441, 1);
        set_word(&mut alley.data, 0x4459, 1);
        set_word(&mut alley.data, 0x43e1, alley.player.x);
        alley.data[0x43f9] = 136;
        set_word(&mut alley.data, 0x44ec, 40);
        alley.data[0x43e0] = 1;
        let mut ended = false;
        let step = alley
            .advance_milk_attention_timed(1, &mut |work, sound| {
                if matches!(work, crate::milk::Work::End) {
                    ended = true;
                    sound.at(7 * crate::timing::BIOS_TICK_PIT_CLOCKS + 100);
                }
                7
            })
            .unwrap();
        assert!(step.warning && ended);
        assert_eq!(word(&alley.data, 0x5ae2), 7);
        assert_eq!(alley.sound.tick(), 9 * crate::timing::BIOS_TICK_PIT_CLOCKS);
        assert_eq!(
            alley.sound.take_busy_ticks(),
            2 * crate::timing::BIOS_TICK_PIT_CLOCKS - 100
        );
    }

    #[test]
    fn scene_palettes_and_carried_gift_change_colors_without_changing_pixels() {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 4, 7);
        set_word(&mut alley.data, 0x2e8d, 0xffff);
        alley.data[0x185a] = 0;
        alley.presented.fill_visible(0x1b);
        let before = alley.framebuffer().bytes().to_vec();
        let rgba = alley.rgba();
        assert_eq!(
            &rgba[..16],
            &[
                0, 0, 0, 255, 0, 170, 0, 255, 170, 0, 0, 255, 170, 85, 0, 255
            ]
        );
        set_word(&mut alley.data, 0x2e8d, 0);
        assert_eq!(&alley.rgba()[..4], &[0, 0, 170, 255]);
        alley.machine = Machine::Pcjr;
        alley.data[0x1842] = 10;
        alley.data[0x184a] = 4;
        alley.data[0x1852] = 15;
        assert_eq!(
            &alley.rgba()[4..16],
            &[85, 255, 85, 255, 170, 0, 0, 255, 255, 255, 255, 255]
        );
        assert_eq!(alley.framebuffer().bytes().as_slice(), before);
    }

    #[test]
    fn pause_captions_are_external_and_bounded() {
        let mut alley = Alley::synthetic();
        alley.data[0x6d91..0x6d96].copy_from_slice(b"test\0");
        alley.data[0x6db2..0x6db9].copy_from_slice(b"resume\0");
        assert_eq!(alley.pause_text().unwrap(), ["test", "resume"]);
        alley.data[0x6d91..0x6db1].fill(b'x');
        alley.data[0x6db1] = 0;
        assert_eq!(alley.pause_text().unwrap()[0].len(), 32);
        alley.data[0x6d91..0x6db2].fill(b'x');
        assert!(alley.pause_text().is_err());
        alley.data.truncate(0x6d90);
        assert!(alley.pause_text().is_err());
    }

    fn actor_fixture() -> Alley {
        let mut alley = Alley::synthetic();
        alley.screen.fill_visible(0xaa);
        initialize_score_state(&mut alley.data).unwrap();
        alley.data[0x1f36..0x1f39].copy_from_slice(&[0, 32, 64]);
        alley.data[0x1f39..0x1f3c].copy_from_slice(&[9, 7, 5]);
        for row in 0..3 {
            set_word(&mut alley.data, 0x1f5f + row * 2, 0x1ee0);
        }
        alley.data[0x1cf0..0x1d24].fill(0xff);
        alley.data[0x1d70..0x1dd0].fill(0xff);
        alley.data[0x1e30..0x1f00].fill(0x55);
        alley.mice = Mice::initialize(&mut alley.data, 8).unwrap();
        set_word(&mut alley.data, 0x1f32, 80);
        set_word(&mut alley.data, 0x1f44, pixel_offset(80, 32));
        alley.player.x = 80;
        alley.player.y = 32;
        alley.player.vertical_coordinate = 82;
        alley
    }

    #[test]
    fn airborne_mouse_catch_updates_the_display_without_losing_lives_or_erasing_player() {
        let mut alley = actor_fixture();
        let dimensions = Dimensions(0x0e03);
        let source = vec![0; dimensions.len()];
        alley
            .compositor
            .draw(&mut alley.screen, pixel_offset(80, 32), dimensions, &source)
            .unwrap();
        assert!(!alley.advance_mice(1).unwrap().paused);
        assert_eq!(alley.data[0x1f51], 1);
        assert_eq!(&alley.data[0x1f82..0x1f89], &[0, 0, 0, 0, 0, 7, 0]);
        assert_eq!(alley.data[0x1f80], 3);
        assert_eq!(
            alley
                .screen
                .read_rect(pixel_offset(80, 32), dimensions)
                .unwrap(),
            source
        );
        assert_ne!(alley.framebuffer().bytes(), alley.screen.bytes());
        alley.compositor.restore(&mut alley.screen).unwrap();
        assert_eq!(
            alley
                .screen
                .read_rect(pixel_offset(80, 32), dimensions)
                .unwrap(),
            vec![0xaa; dimensions.len()]
        );
    }

    #[test]
    fn mouse_knockdown_preserves_motion_history_and_restores_the_clean_layers() {
        let mut alley = actor_fixture();
        alley.player.support_state = 2;
        alley.player.vertical_step = 4;
        alley.player.phase = 77;
        alley.data[0] = 1;
        assert!(alley.advance_mice(1).unwrap().paused);
        assert_eq!(
            (
                alley.player.support_state,
                alley.player.vertical_direction,
                alley.player.horizontal_direction
            ),
            (0, 1, 0)
        );
        assert_eq!((alley.player.vertical_step, alley.player.phase), (4, 77));
        assert_eq!(alley.data[0x1f80], 3);
        assert_ne!(alley.framebuffer().bytes(), alley.screen.bytes());
        let seed = word(&alley.data, 0x2ae5);
        alley.mouse_effect_random(1);
        let first = word(&alley.data, 0x2ae5);
        assert_ne!(first, seed);
        alley.mouse_effect_random(1);
        assert_eq!(word(&alley.data, 0x2ae5), first);
        alley.finish_mouse_effect().unwrap();
        assert_eq!(alley.framebuffer().bytes(), alley.screen.bytes());
    }

    #[test]
    fn mouse_exit_poll_can_observe_a_tick_after_the_sound_poll() {
        use crate::runtime::Runtime;
        use crate::timing::{BIOS_TICK_PIT_CLOCKS, PlayerGate};
        let mut alley = actor_fixture();
        alley.player.support_state = 2;
        alley.data[0] = 1;
        alley.data[0x531] = 255;
        let mut runtime = Runtime::new(Machine::Cga);
        runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS);
        runtime.player_gate = PlayerGate::new(1, 0);
        for _ in 0..4 {
            runtime
                .advance_pass(&mut alley, Controls::default())
                .unwrap();
        }
        assert!(alley.mouse_effect.is_some());
        let start = word(&alley.data, 0x1f65);
        let deadline = u64::from(start + 8) * BIOS_TICK_PIT_CLOCKS;
        runtime
            .clock
            .advance_pit_ticks(deadline - 200 - runtime.clock.elapsed_pit_ticks() as u64);
        set_word(&mut alley.data, 0x5a14, start + 7);
        let seed = word(&alley.data, 0x2ae5);
        let event = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: -1 })
            .unwrap();
        assert!(event.mouse_wait && !event.control_polled);
        assert!(alley.mouse_effect.is_none());
        assert_eq!(word(&alley.data, 0x5a14), start + 7);
        assert_eq!(word(&alley.data, 0x2ae5), seed);
        assert_eq!(runtime.clock.sample().bios.tick_low, start + 8);
    }

    #[test]
    fn eel_wait_baseline_and_palette_sample_follow_setup_across_tick_wrap() {
        let mut alley = Alley::synthetic();
        alley.damage_effect = Some(DamageEffect {
            since: u16::MAX,
            duration: 13,
            kind: EffectKind::Eel {
                phase: EelPhase::Restart,
                palette_tick: u16::MAX,
                background: 0,
            },
        });
        alley.start_eel_effect_wait(0);
        assert_eq!(word(&alley.data, 0x3509), 0);
        assert!(matches!(
            alley.damage_effect,
            Some(DamageEffect {
                since: 0,
                kind: EffectKind::Eel {
                    palette_tick: 0,
                    ..
                },
                ..
            })
        ));
        assert!(!alley.advance_damage_effect(12, 0, false).unwrap());
        assert!(!alley.advance_damage_effect(13, 0, true).unwrap());
        assert!(!alley.advance_damage_effect(13, 0, false).unwrap());
        assert!(!alley.advance_damage_effect(13, 0, false).unwrap());
        assert!(alley.advance_damage_effect(13, 0, false).unwrap());
    }

    #[test]
    fn spider_wait_baseline_follows_completed_drawing_across_tick_wrap() {
        let mut alley = Alley::synthetic();
        alley.damage_effect = Some(DamageEffect {
            since: u16::MAX - 1,
            duration: 9,
            kind: EffectKind::Spider,
        });
        alley.start_spider_effect_wait(0);
        assert_eq!(word(&alley.data, 0x39c8), 0);
        assert!(!alley.advance_damage_effect(8, 0, false).unwrap());
        assert!(alley.advance_damage_effect(9, 0, false).unwrap());
        assert_eq!(alley.data[0x552], 1);
    }

    #[test]
    fn scheduled_mouse_contact_blocks_and_resumes_without_life_loss() {
        use crate::runtime::Runtime;
        use crate::timing::{BIOS_TICK_PIT_CLOCKS, PlayerGate};
        let mut alley = actor_fixture();
        alley.player.support_state = 2;
        alley.data[0x531] = 255;
        let mut runtime = Runtime::new(Machine::Cga);
        runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS);
        runtime.player_gate = PlayerGate::new(1, 0);
        for _ in 0..4 {
            runtime
                .advance_pass(&mut alley, Controls::default())
                .unwrap();
        }
        assert!(alley.mouse_effect.is_some());
        let start = word(&alley.data, 0x1f65);
        let phase = alley.player.phase;
        while alley.mouse_effect.is_some() {
            let event = runtime
                .advance_pass(&mut alley, Controls { x: -1, y: -1 })
                .unwrap();
            assert!(event.mouse_wait);
            assert!(!event.control_polled);
            assert!(!event.actor_group_due);
            assert!(event.acceptance.is_none());
            assert_eq!(alley.player.phase, phase);
        }
        assert_eq!(runtime.clock.sample().bios.tick_low.wrapping_sub(start), 8);
        assert_eq!(alley.data[0x1f80], 3);
        assert_eq!(alley.player.vertical_direction, 1);
        assert_eq!(alley.framebuffer().bytes(), alley.screen.bytes());
    }

    #[test]
    fn lower_support_takeoff_preserves_attachment_then_release_sound() {
        use crate::runtime::{ReferenceCosts, Runtime};
        use crate::timing::{RetraceProfile, VirtualClock};

        for machine in [Machine::Cga, Machine::Pcjr] {
            for legacy in [false, true] {
                for latch in [0, 1] {
                    for enabled in [false, true] {
                        let mut alley = actor_fixture();
                        alley.machine = machine;
                        alley.player.y = 142;
                        alley.player.vertical_coordinate = 192;
                        alley.player.horizontal_direction = 1;
                        alley.data[0xff0] = 20;
                        alley.data[0x127c] = latch;
                        alley.data[0x127d] = 0xa5;
                        alley.data[0] = if enabled { 0xff } else { 0 };
                        alley.data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
                        alley.sound.record(true);
                        let costs = if legacy {
                            ReferenceCosts::dosbox_x_240(machine)
                        } else {
                            ReferenceCosts::ibm_xt(machine)
                        };
                        let mut runtime = Runtime::with_costs(machine, costs);
                        runtime.clock =
                            VirtualClock::with_initial_tick(RetraceProfile::default(), 1000);
                        let event = runtime
                            .advance_pass(&mut alley, Controls { x: 1, y: -1 })
                            .unwrap();
                        assert!(event.acceptance.is_some());
                        assert_eq!(alley.player.vertical_direction, 255);
                        assert_eq!(&alley.data[0x127c..0x127e], &[0, 0xa5]);
                        let writes: Vec<_> = alley.sound.drain().collect();
                        let divisors: Vec<_> = writes
                            .iter()
                            .filter(|w| w.port == 0x42)
                            .map(|w| w.value)
                            .collect::<Vec<_>>()
                            .chunks_exact(2)
                            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                            .collect();
                        let expected = if !enabled {
                            vec![]
                        } else if latch == 0 {
                            vec![0x400, 0x390]
                        } else {
                            vec![0x390]
                        };
                        assert_eq!(divisors, expected);
                        assert_eq!(
                            writes.iter().filter(|w| w.port == 0x43).count(),
                            expected.len()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bin_push_overrides_requested_jump_and_commits_one_shared_random_draw() {
        let mut alley = actor_fixture();
        alley.player.y = 142;
        alley.player.vertical_coordinate = 192;
        alley.data[0xff0] = 20;
        alley.data[0x1d59] = 13;
        alley.data[0x127c] = 1;
        alley.data[0x127d] = 0xa5;
        set_word(&mut alley.data, 0x1d5c, 80);
        alley.data[0x1d5f] = 147;
        set_word(&mut alley.data, 0x1d64, 0x0d02);
        let next = crate::assets::random_successor(word(&alley.data, 0x2ae5));
        let outcome = alley.update(Controls { x: 1, y: -1 }).unwrap();
        assert!(outcome.bin_contact_checked);
        let forced = outcome.forced_controls.unwrap();
        assert_eq!(forced.y, 1);
        assert_eq!(forced.x, if next & 1 == 0 { -1 } else { 1 });
        assert_eq!(word(&alley.data, 0x2ae5), next);
        assert_eq!(alley.data[0x1d58], 1);
        assert_eq!(&alley.data[0x127c..0x127e], &[0, 0xa5]);
        assert_eq!(alley.player.vertical_direction, 1);
        assert_eq!(alley.data[0x1f80], 3);
    }

    #[test]
    fn moving_clothes_recompose_the_carried_cat_without_extra_random_calls_or_ghosts() {
        let mut alley = Alley::synthetic();
        alley.screen.fill_visible(0xaa);
        let dimensions = Dimensions(0x1028);
        let band: Vec<u8> = (0..1280).map(|index| (index % 251) as u8).collect();
        alley.screen.copy_rect(0x140, dimensions, &band).unwrap();
        alley.data[0x531] = 1;
        alley.data[0x532] = 100;
        alley.data[0x529] = 0xff;
        alley.data[0x525] = 2;
        alley.data[0x53a] = 75;
        alley.data[0x53d] = 46;
        set_word(&mut alley.data, 0x51d, 0x140);
        alley.player.x = 80;
        alley.player.y = 8;
        alley.player.vertical_coordinate = 58;
        alley.player.support_state = 2;
        let source = vec![0; Dimensions(0x0e03).len()];
        alley
            .compositor
            .draw(
                &mut alley.screen,
                pixel_offset(80, 8),
                Dimensions(0x0e03),
                &source,
            )
            .unwrap();
        let seed = word(&alley.data, 0x2ae5);
        assert!(alley.advance_scenery(1, false).unwrap().is_some());
        assert_eq!(alley.player.x, 84);
        assert_eq!(word(&alley.data, 0x2ae5), seed);
        assert_eq!(
            alley
                .screen
                .read_rect(pixel_offset(84, 8), Dimensions(0x0e03))
                .unwrap(),
            source
        );
        alley.compositor.restore(&mut alley.screen).unwrap();
        let clean = alley.screen.read_rect(0x140, dimensions).unwrap();
        for row in 0..16 {
            assert_eq!(
                &clean[row * 80 + 1..row * 80 + 80],
                &band[row * 80..row * 80 + 79]
            );
            assert_eq!(clean[row * 80], 0);
        }
    }
}
