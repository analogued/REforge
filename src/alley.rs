// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! External alley assets, animated scenery, and player updates.

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
    fn colors(self) -> [[u8; 4]; 4] {
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
        };
        // The first accepted neutral update establishes the original idle image.
        alley.update(Controls::default())?;
        Ok(alley)
    }

    pub fn update(&mut self, controls: Controls) -> Result<UpdateOutcome, String> {
        let outcome = self.player.update(controls, &self.data)?;
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
            Draw::Keep => {}
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
        let colors = self.machine.colors();
        self.presented
            .indices()
            .into_iter()
            .flat_map(|index| colors[usize::from(index)])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::word;

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
