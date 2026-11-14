// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::session::Phase;

// Original IRQ0 divider and durations recovered from overlay 1:B694..BA72.
pub const PIT_HZ: u128 = 1_193_182;
pub const PIT_DIVISOR: u128 = 0x3fff;
pub const NARRATION_START: u32 = 504;
pub const LINE_DELAY: u32 = 144;
pub const INTRO_END: u32 = 2664;
pub const TITLE_DURATION: u32 = 3024 - INTRO_END;
pub const CREDIT_DURATION: u32 = 288;
pub const PROLOGUE_DURATION: u32 = 720;

pub fn clock_ticks(nanoseconds: u128) -> u64 {
    (nanoseconds * PIT_HZ / (PIT_DIVISOR * 1_000_000_000)) as u64
}

pub struct Intro {
    narrative: Vec<String>,
    credits: Vec<(String, String)>,
    prologue: String,
    border: Vec<Bitmap>,
    pub water: crate::water::Water,
}

fn original_string(assets: &Assets, offset: usize) -> Result<String> {
    let data = assets
        .data_segment
        .get(offset..)
        .ok_or("Intro string outside data segment")?;
    let end = data
        .iter()
        .position(|b| *b == 0)
        .ok_or("Unterminated intro string")?;
    String::from_utf8(data[..end].to_vec()).map_err(|e| e.to_string())
}

impl Intro {
    pub fn load(assets: &Assets) -> Result<Self> {
        let mut narrative = Vec::new();
        for offset in (0x6a5c..0x6a6c).step_by(2) {
            narrative.push(original_string(
                assets,
                word(&assets.data_segment, offset)? as usize,
            )?);
        }
        let mut credits = Vec::new();
        for offset in (0x6a6e..0x6a9a).step_by(4) {
            credits.push((
                original_string(assets, word(&assets.data_segment, offset)? as usize)?,
                original_string(assets, word(&assets.data_segment, offset + 2)? as usize)?,
            ));
        }
        Ok(Self {
            narrative,
            credits,
            prologue: original_string(assets, word(&assets.data_segment, 0x6a6c)? as usize)?,
            border: assets.sprites("CENT", "MMENU")?,
            water: crate::water::Water::load(assets)?,
        })
    }

    pub fn next(&self, phase: Phase) -> Option<(u32, Phase)> {
        match phase {
            Phase::Intro => Some((INTRO_END, Phase::Title)),
            Phase::Title => Some((TITLE_DURATION, Phase::Credits)),
            Phase::Credits => Some((CREDIT_DURATION * self.credits.len() as u32, Phase::Prologue)),
            Phase::Prologue => Some((PROLOGUE_DURATION, Phase::Map)),
            _ => None,
        }
    }

    pub fn render(
        &self,
        assets: &Assets,
        frame: &mut Frame,
        phase: Phase,
        ticks: u32,
    ) -> Result<()> {
        if phase == Phase::Intro && ticks >= NARRATION_START {
            let count =
                ((ticks - NARRATION_START) / LINE_DELAY + 1).min(self.narrative.len() as u32);
            for (line, text) in self.narrative.iter().take(count as usize).enumerate() {
                frame.text(assets, text, 5, 5 + line as i32 * 11, 6)?;
            }
        } else if matches!(phase, Phase::Credits | Phase::Prologue) {
            // Original 30E6 window: fill first, then four MMENU border sprites.
            frame.rect(88, 43, 143, 111, 26);
            for border in &self.border {
                frame.blit(border, 84, 40);
            }
            if phase == Phase::Credits {
                let index = (ticks / CREDIT_DURATION) as usize;
                let (heading, names) = &self.credits[index.min(self.credits.len() - 1)];
                frame.center_lines(assets, heading, 159, 47, 1)?;
                let top = 99 - names.split('\n').count() as i32 * 11 / 2;
                frame.center_lines(assets, names, 159, top, 0)?;
            } else {
                let top = 99 - self.prologue.split('\n').count() as i32 * 11 / 2;
                frame.center_lines(assets, &self.prologue, 159, top, 15)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_tick_rate_has_no_per_frame_rounding_loss() {
        assert_eq!(clock_ticks(1_000_000_000), 72);
        assert_eq!(clock_ticks(60_000_000_000), 4369);
        assert_eq!(clock_ticks(1_000_000_000 * PIT_DIVISOR), PIT_HZ as u64);
    }
}
