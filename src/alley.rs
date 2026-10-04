// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! The initial alley slice: external assets, static scenery, and player updates.

use crate::assets::{Assets, idle_sources, initialize_alley, set_word, sprite_bytes};
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
        let offset = pixel_offset(self.player.x, self.player.y);
        match outcome.draw {
            Draw::Keep => {}
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
