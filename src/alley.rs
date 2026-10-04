// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! The initial alley slice: external assets, static scenery, and player updates.

use crate::assets::{Assets, idle_sources, initialize_alley, next_random, set_word, sprite_bytes};
use crate::cga::{Dimensions, Framebuffer, PlayerCompositor, pixel_offset};
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
        initialize_alley(&mut data, &mut screen)?;
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

    pub fn rgba(&self) -> Vec<u8> {
        let colors = self.machine.colors();
        self.screen
            .indices()
            .into_iter()
            .flat_map(|index| colors[usize::from(index)])
            .collect()
    }
}
