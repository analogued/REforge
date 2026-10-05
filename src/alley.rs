// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! External alley assets, animated scenery, and player updates.

use crate::assets::{Assets, idle_sources, initialize_alley, next_random, set_word, sprite_bytes};
use crate::cga::{Dimensions, Framebuffer, PlayerCompositor, pixel_offset};
use crate::decoration::{draw_ground_objects, draw_hud, initialize_score_state};
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
    compositor: PlayerCompositor,
    pub player: Player,
    machine: Machine,
}

impl Alley {
    pub fn machine(&self) -> Machine {
        self.machine
    }

    pub(crate) fn advance_random(&mut self) {
        next_random(&mut self.data);
    }

    #[cfg(test)]
    pub(crate) fn synthetic() -> Self {
        let mut data = vec![0; 0x7130];
        set_word(&mut data, 0x2ae5, 0xfa59);
        Self {
            data,
            screen: Framebuffer::default(),
            compositor: PlayerCompositor::default(),
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
        set_word(&mut data, 8, difficulty);
        set_word(&mut data, 0x2ae5, seed);
        set_word(&mut data, 0x6df8, 0);
        data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
        let mut screen = Framebuffer::default();
        initialize_score_state(&mut data)?;
        initialize_alley(&mut data, &mut screen)?;
        draw_ground_objects(&mut data, &mut screen)?;
        draw_hud(&mut data, &mut screen)?;
        let mut alley = Self {
            data,
            screen,
            compositor: PlayerCompositor::default(),
            player: Player::at_alley_floor(8),
            machine,
        };
        // The first accepted neutral update establishes the original idle image.
        alley.update(Controls::default())?;
        Ok(alley)
    }

    pub fn update(&mut self, controls: Controls) -> Result<UpdateOutcome, String> {
        let outcome = self.player.update(controls, &self.data)?;
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
        Ok(outcome)
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.screen
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
        result
    }

    pub fn rgba(&self) -> Vec<u8> {
        let colors = self.machine.colors();
        self.screen
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
