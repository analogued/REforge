// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! External game assets, scenery, actors, player updates, and scene transitions.

use crate::assets::{
    Assets, idle_sources, initialize_alley, next_random, set_word, sprite_bytes, word,
};
use crate::cga::{Dimensions, Framebuffer, PlayerCompositor, pixel_offset};
use crate::decoration::{award_points, draw_ground_objects, draw_hud, initialize_score_state};
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

pub struct Alley {
    data: Vec<u8>,
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
    /// Actual ordinary window destinations while the ordinary room set is incomplete.
    /// Original candidate history remains separate in the external runtime data.
    ordinary_rooms: [u16; 2],
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
        next_random(&mut self.data);
    }

    pub(crate) fn bin_contact_sound_ready(&self) -> bool {
        self.data[0] != 0 && self.data[0x56c] == 0
    }

    #[cfg(test)]
    pub(crate) fn synthetic() -> Self {
        let mut data = vec![0; 0x7130];
        set_word(&mut data, 0x2ae5, 0xfa59);
        data[0x1028] = 0xff;
        data[0x100e] = 7;
        Self {
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
            ordinary_rooms: [0; 2],
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
        // Original startup enables sound. Keep its logical gates even while
        // audio output is deferred: silent helpers still affect shared state.
        data[0] = 0xff;
        data[0x69a] = 16;
        set_word(&mut data, 8, difficulty);
        set_word(&mut data, 0x2ae5, seed);
        set_word(&mut data, 0x6df8, 0);
        data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
        let mut screen = Framebuffer::default();
        initialize_score_state(&mut data)?;
        initialize_alley(&mut data, &mut screen)?;
        draw_ground_objects(&mut data, &mut screen)?;
        draw_hud(&mut data, &mut screen)?;
        crate::bin_cat::initialize(&mut data)?;
        crate::windows::initialize(&mut data)?;
        let mice = Mice::initialize(&mut data, 8)?;
        let mut alley = Self {
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
            ordinary_rooms: [0; 2],
        };
        // The first accepted neutral update establishes the original idle image.
        alley.update(Controls::default())?;
        Ok(alley)
    }

    pub fn update(&mut self, controls: Controls) -> Result<UpdateOutcome, String> {
        let image = if matches!(self.scene(), 3..=5 | 7) {
            let image = self.compositor.suspend(&mut self.screen)?;
            self.player.write_data(&mut self.data)?;
            if matches!(self.scene(), 3..=5) && self.player.floor_track_due(controls) {
                self.data[0x56e] = controls.x as u8;
                crate::room::record_floor_track(&mut self.data, &mut self.screen)?;
            }
            image
        } else {
            None
        };
        let outcome = self.player.update(controls, &self.data)?;
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
                self.compositor
                    .draw_idle(&mut self.screen, offset, top, bottom)?;
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

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.presented
    }

    /// Original scene numbers: alley zero, bookcase three, cheese four, birdcage
    /// five and hearts seven.
    pub fn scene(&self) -> u16 {
        word(&self.data, 4)
    }

    pub(crate) fn advance_room_tone(&mut self, tick: u16) {
        // CS:546d expires a pending asynchronous tone once per distinct tick;
        // skipped ticks are not caught up. Ambient synthesis remains deferred.
        if self.data[0] != 0 && self.data[0x5920] != 0 && word(&self.data, 0x5921) != tick {
            set_word(&mut self.data, 0x5921, tick);
            self.data[0x5920] -= 1;
        }
    }

    pub(crate) fn latch_action(&mut self, held: bool) {
        // CS:12d3..1334 converts the Alt key's 00/80 state to 00/10.
        self.data[0x69a] = if held { 0 } else { 16 };
    }

    pub(crate) fn advance_hearts_arrow(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        self.room_actor(|data, screen| crate::hearts::advance_arrow(data, screen, tick))
    }

    pub(crate) fn pickup_hearts_item(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        self.room_actor(|data, screen| crate::hearts::pickup_item(data, screen, tick))
    }

    pub(crate) fn place_hearts_item(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        self.room_actor(|data, screen| crate::hearts::place_item(data, screen, tick))
    }

    pub(crate) fn advance_hearts_cat(
        &mut self,
        tick: u16,
    ) -> Result<crate::hearts::HeartStep, String> {
        self.room_actor(|data, screen| crate::hearts::advance_cat(data, screen, tick))
    }

    fn reset_sound_state(&mut self) {
        // CS:58bd logical initialization, without synthesizing audio.
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
        self.completion.is_some() || self.hearts_completion.is_some()
    }

    /// Preview the ordinary birdcage success presentation without playing its
    /// objective. This diagnostic uses the same return path as a bird capture.
    pub fn enter_completion(&mut self, tick: u16) -> Result<(), String> {
        self.enter_birdcage(tick)?;
        self.data[0x553] = 1;
        self.finish_scene(tick)?;
        Ok(())
    }

    pub(crate) fn advance_completion(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<crate::completion::Work, String> {
        let work = if let Some(completion) = self.completion.as_mut() {
            completion.advance(&mut self.data, &mut self.screen, tick, retrace)?
        } else if let Some(completion) = self.hearts_completion.as_mut() {
            completion.advance(&mut self.data, &mut self.screen, tick, retrace)?
        } else {
            return Err("No completion presentation is active".into());
        };
        self.presented = self.screen.clone();
        if work.complete {
            self.completion = None;
            self.hearts_completion = None;
            self.return_to_alley()?;
        }
        Ok(work)
    }

    /// Enter the currently supported room directly for diagnostic play.
    pub fn enter_birdcage(&mut self, tick: u16) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        set_word(&mut self.data, 1, self.player.x);
        self.data[3] = self.player.y;
        set_word(&mut self.data, 0x410, tick);
        self.data[0x419] = 1;
        self.setup_birdcage()
    }

    /// Enter the bookcase room directly for diagnostic play.
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
        self.data[0x1cb8] = 0;
        self.data[0x1cbf] = 0;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter the cheese room directly for diagnostic play.
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
        self.data[0x1cb8] = 0;
        self.data[0x1cbf] = 0;
        self.player = Player::from_data(&self.data)?;
        self.reset_sound_state();
        self.compose_actors()
    }

    /// Enter scene seven directly for diagnostic play, saving the same return
    /// coordinates as a window entry. Normal progression enters via a window.
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
        crate::hearts::initialize(&mut self.data, &mut self.screen, tick)?;
        crate::room::initialize_room_player(&mut self.data, 7)?;
        self.data[0x1cb8] = 0;
        self.data[0x1cbf] = 0;
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
        // Shared roaming-dog state is retained as disabled until lethal actors
        // are introduced. It must not stall the cage's nonlethal fall.
        self.data[0x1cb8] = 0;
        self.data[0x1cbf] = 0;
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
        action: impl FnOnce(&mut [u8], &mut Framebuffer) -> Result<T, String>,
    ) -> Result<T, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
        let result = action(&mut self.data, &mut self.screen)?;
        self.player.apply_actor_state(&self.data)?;
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

    pub(crate) fn advance_bookcase_spider(
        &mut self,
        tick: u16,
    ) -> Result<crate::bookcase::SpiderStep, String> {
        self.room_actor(|data, screen| crate::bookcase::advance_spider(data, screen, tick))
    }

    pub(crate) fn advance_bookcase_targets(
        &mut self,
        tick: u16,
    ) -> Result<crate::bookcase::TargetStep, String> {
        self.room_actor(|data, screen| crate::bookcase::advance_targets(data, screen, tick))
    }

    pub(crate) fn advance_cheese_mice(
        &mut self,
        tick: u16,
    ) -> Result<crate::cheese::MouseStep, String> {
        self.room_actor(|data, screen| crate::cheese::advance_mice(data, screen, tick))
    }

    pub(crate) fn advance_cheese_transfer(
        &mut self,
        tick: u16,
    ) -> Result<crate::cheese::TransferStep, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
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
        self.room_actor(|data, screen| crate::birdcage::advance_cage(data, screen, tick))
    }

    pub(crate) fn advance_room_fall(
        &mut self,
        tick: u16,
    ) -> Result<crate::birdcage::FallStep, String> {
        self.room_actor(|data, screen| crate::birdcage::advance_fall(data, screen, tick))
    }

    pub(crate) fn advance_room_bird(
        &mut self,
        tick: u16,
    ) -> Result<crate::birdcage::BirdStep, String> {
        self.room_actor(|data, screen| crate::birdcage::advance_bird(data, screen, tick))
    }

    pub(crate) fn advance_room_broom(
        &mut self,
        tick: u16,
    ) -> Result<Option<crate::room::BroomUpdate>, String> {
        self.room_actor(|data, screen| crate::room::advance_broom(data, screen, tick))
    }

    pub(crate) fn advance_windows(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<crate::windows::Update, String> {
        let image = self.compositor.suspend(&mut self.screen)?;
        self.player.write_data(&mut self.data)?;
        let update = crate::windows::advance(
            &mut self.data,
            &mut self.screen,
            &self.player,
            tick,
            retrace,
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

    /// Temporary selection among implemented rooms. The original candidate
    /// selector still advances its own generator/history before this choice.
    fn playable_destination(&mut self, candidate: u16) -> u16 {
        if candidate == 7 {
            return 7;
        }
        let mut room = 3 + next_random(&mut self.data) % 3;
        if self.ordinary_rooms == [room; 2] {
            room = 3 + (room - 2) % 3;
        }
        self.ordinary_rooms = [self.ordinary_rooms[1], room];
        room
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
            // Keep the original candidate selector independent from the current
            // playable set. Its pending-hearts override takes priority.
            let candidate = crate::windows::select_room(&mut self.data, &self.player, tick)?;
            match self.playable_destination(candidate) {
                7 => self.setup_hearts(tick)?,
                3 => self.setup_bookcase()?,
                4 => self.setup_cheese()?,
                _ => self.setup_birdcage()?,
            }
            return Ok(true);
        }
        if self.data[0x551] == 0 && self.data[0x553] == 0 {
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
        self.return_to_alley()?;
        Ok(true)
    }

    fn return_to_alley(&mut self) -> Result<(), String> {
        self.player.write_data(&mut self.data)?;
        let scene = self.scene();
        set_word(&mut self.data, 6, scene);
        set_word(&mut self.data, 4, 0);
        self.compositor = PlayerCompositor::default();
        initialize_alley(&mut self.data, &mut self.screen)?;
        draw_ground_objects(&mut self.data, &mut self.screen)?;
        self.data[0x1f81] = 0xff;
        draw_hud(&mut self.data, &mut self.screen)?;
        crate::bin_cat::initialize(&mut self.data)?;
        crate::windows::initialize(&mut self.data)?;
        let saved_x = word(&self.data, 1);
        self.mice = Mice::initialize(&mut self.data, saved_x)?;
        crate::room::initialize_room_player(&mut self.data, 0)?;
        self.data[0x550] = 2;
        self.data[0x576] = 1;
        self.data[0x578] = 0x20;
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
        if self.scene() != 0 {
            return Ok(());
        }
        if let Some(image) = self.bin_image {
            self.draw_actor(Image {
                source: image.source,
                dimensions: image.dimensions.0,
                offset: image.offset,
                blend: Blend::ZeroTransparent,
            })?;
        }
        for image in (*self.mice.images()).into_iter().flatten() {
            self.draw_actor(image)?;
        }
        if let Some(image) = self.mouse_effect {
            self.draw_actor(image)?;
        }
        Ok(())
    }

    pub(crate) fn advance_bin_cat(
        &mut self,
        tick: u16,
        retrace: bool,
    ) -> Result<Option<BinAdvance>, String> {
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
                // CS:593b's asynchronous tone state; output synthesis is deferred.
                if self.data[0] != 0 {
                    self.data[0x5920] = 2;
                    set_word(&mut self.data, 0x5921, tick);
                    set_word(&mut self.data, 0x5923, 750);
                }
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

    pub(crate) fn mouse_effect_random(&mut self, tick: u16) {
        if self.data[0] != 0 && word(&self.data, 0x5a14) != tick {
            set_word(&mut self.data, 0x5a14, tick);
            next_random(&mut self.data);
        }
    }

    pub(crate) fn start_mouse_effect_wait(&mut self, tick: u16) {
        // CS:25e4 samples the baseline after composing the temporary image.
        set_word(&mut self.data, 0x1f65, tick);
        self.mouse_effect_random(tick);
    }

    pub(crate) fn finish_mouse_effect(&mut self) -> Result<(), String> {
        self.mouse_effect = None;
        self.compose_actors()
    }

    pub fn rgba(&self) -> Vec<u8> {
        let presentation = self.hearts_completion.as_ref();
        let palette_scene = if self.completion.is_some()
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
        let background = if let Some(completion) = presentation {
            completion.background()
        } else if let Some(completion) = self.completion.as_ref() {
            completion.background()
        } else if self.scene() == 7 && word(&self.data, 0x2e8d) < 8 {
            1
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
    use crate::assets::word;

    #[test]
    fn available_rooms_are_seeded_and_do_not_repeat_three_times() {
        for difficulty in 0..8 {
            let mut alley = Alley::synthetic();
            set_word(&mut alley.data, 8, difficulty);
            alley.data[0x421..0x432]
                .copy_from_slice(&[4, 4, 1, 1, 3, 3, 5, 5, 6, 6, 4, 1, 1, 3, 4, 5, 6]);
            let mut replay = Alley::synthetic();
            replay.data.clone_from(&alley.data);
            let mut rooms = Vec::new();
            for _ in 0..64 {
                let candidate = crate::windows::choose_room(&mut alley.data).unwrap();
                let actual = alley.playable_destination(candidate);
                let replay_candidate = crate::windows::choose_room(&mut replay.data).unwrap();
                assert_eq!(replay_candidate, candidate);
                assert_eq!(replay.playable_destination(replay_candidate), actual);
                // Temporary selection must not rewrite the original candidate history.
                assert_eq!(word(&alley.data, 0x41d), candidate);
                rooms.push(actual);
            }
            assert!([3, 4, 5].into_iter().all(|room| rooms.contains(&room)));
            assert!(
                rooms
                    .windows(3)
                    .all(|rooms| rooms[0] != rooms[1] || rooms[1] != rooms[2])
            );
            assert_eq!(word(&alley.data, 0x2ae5), word(&replay.data, 0x2ae5));
        }
    }

    #[test]
    fn pending_hearts_preserves_random_state_and_actual_room_history() {
        let mut alley = Alley::synthetic();
        alley.ordinary_rooms = [4, 4];
        alley.data[0x418] = 1;
        let seed = word(&alley.data, 0x2ae5);
        let candidate = crate::windows::choose_room(&mut alley.data).unwrap();
        assert_eq!(alley.playable_destination(candidate), 7);
        assert_eq!(word(&alley.data, 0x2ae5), seed);
        assert_eq!(alley.ordinary_rooms, [4, 4]);
        assert_eq!(alley.data[0x418], 0);
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
