// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Startup VGA palette cycling, recovered from overlay 1:B8CF..B96F.
use crate::assets::{Assets, Result, word};
use crate::session::Phase;

pub type Palette = [[u8; 3]; 256];

pub struct Water {
    colors: [[u8; 3]; 5],
    indices: [usize; 5],
    rotation: Option<u8>,
    until_step: u32,
}

impl Water {
    pub fn load(assets: &Assets) -> Result<Self> {
        let mut colors = [[0; 3]; 5];
        let mut indices = [0; 5];
        for index in 0..5 {
            indices[index] = word(&assets.data_segment, 0x6ad8 + index * 2)? as usize;
            if indices[index] >= 256 {
                return Err("Water palette index out of range".into());
            }
            for (component, output) in colors[index].iter_mut().enumerate() {
                let dac = word(&assets.data_segment, 0x6aba + index * 6 + component * 2)?;
                if dac > 63 {
                    return Err("Water DAC component out of range".into());
                }
                *output = ((dac << 2) | (dac >> 4)) as u8;
            }
        }
        Ok(Self {
            colors,
            indices,
            rotation: None,
            until_step: 1,
        })
    }

    pub fn enabled(phase: Phase) -> bool {
        matches!(phase, Phase::Title | Phase::Credits | Phase::Prologue)
    }

    pub fn reset(&mut self) {
        self.rotation = None;
        self.until_step = 1;
    }

    pub fn advance(&mut self, ticks: u32) {
        if ticks < self.until_step {
            self.until_step -= ticks;
            return;
        }
        let remaining = ticks - self.until_step;
        let steps = 1 + remaining / 5;
        // Original increments 0 -> 1 -> 2 -> 3 -> 4 -> 0. It indexes
        // destinations modulo FOUR, so rotations 4 and 0 look identical.
        self.rotation = Some(((u32::from(self.rotation.unwrap_or(0)) + steps) % 5) as u8);
        self.until_step = 5 - remaining % 5;
    }

    pub fn palette(&self, base: &Palette, phase: Phase) -> Palette {
        let mut palette = *base;
        if Self::enabled(phase)
            && let Some(rotation) = self.rotation
        {
            // Five writes to four destinations: the fifth overwrites the
            // first. Preserve this quirk rather than rotating five entries.
            for color in 0..5 {
                let index = self.indices[(rotation as usize + color) % 4];
                palette[index] = self.colors[color];
            }
        }
        // B8CF restores all FIVE original colors on exit; CMAP has exactly
        // those same values. Returning base also leaves every other entry intact.
        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water() -> Water {
        Water {
            colors: [[1; 3], [2; 3], [3; 3], [4; 3], [5; 3]],
            indices: [162, 163, 164, 165, 166],
            rotation: None,
            until_step: 1,
        }
    }

    #[test]
    fn original_fifth_write_and_repeated_rotation_are_preserved() {
        let base = [[9; 3]; 256];
        let mut water = water();
        assert_eq!(water.palette(&base, Phase::Title), base);
        water.advance(1);
        let palette = water.palette(&base, Phase::Title);
        assert_eq!(
            &palette[162..167],
            &[[4; 3], [5; 3], [2; 3], [3; 3], [9; 3]]
        );
        assert_eq!(water.palette(&base, Phase::Intro), base);
        water.advance(15); // Rotation 4.
        let fourth = water.palette(&base, Phase::Title);
        water.advance(5); // Rotation 0 repeats the same visible arrangement.
        assert_eq!(water.palette(&base, Phase::Credits), fourth);
        water.reset();
        assert_eq!(water.palette(&base, Phase::Map), base);
    }

    #[test]
    fn animation_does_not_depend_on_render_or_tick_chunk_sizes() {
        let mut large = water();
        let mut small = water();
        large.advance(43_699);
        for _ in 0..43_699 {
            small.advance(1);
        }
        assert_eq!(large.rotation, small.rotation);
        assert_eq!(large.until_step, small.until_step);
    }
}
