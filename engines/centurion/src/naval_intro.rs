// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Naval briefing pennant and two independent VGA palette rings (C105..C273).
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

pub struct Briefing {
    image: Vec<u8>,
    flags: Vec<Bitmap>,
    elapsed: u32,
    pennant: Option<usize>,
    rotated: bool,
}

impl Briefing {
    pub fn new(assets: &Assets) -> Result<Self> {
        let image = assets.resource("NAVL", "GALLEYS.LZW")?;
        if image.len() != 64000 {
            return Err("Invalid naval briefing image".into());
        }
        Ok(Self {
            image,
            flags: assets.sprites("MNAVAL", "FLAG")?,
            elapsed: 0,
            pennant: None,
            rotated: false,
        })
    }

    pub fn reset(&mut self) {
        self.elapsed = 0;
        self.pennant = None;
        self.rotated = false;
    }

    pub fn advance(&mut self, s: &mut Campaign) {
        self.elapsed = self.elapsed.wrapping_add(1);
        // The palette routine has its own timestamp; the initial call is due
        // immediately after the image loads, then strictly more than four ticks.
        if (self.elapsed - 1).is_multiple_of(5) {
            s.data[0x9816] = (s.data[0x9816] + 1) % 6;
            s.data[0x9817] = (s.data[0x9817] + 1) % 5;
            self.rotated = true;
        }
        if self.elapsed.is_multiple_of(4) {
            self.pennant = Some(self.pennant.map_or(1, |p| (p + 1) % 4));
        }
    }

    pub fn render(&self, assets: &Assets, s: &Campaign) -> Result<Frame> {
        let mut f = Frame::new();
        f.pixels.copy_from_slice(&self.image);
        if let Some(p) = self.pennant {
            f.rect(38, 2, 31, 14, 128);
            f.blit(&self.flags[p], 38, 5);
        }
        let mut palette = assets.palette;
        if self.rotated {
            for (index_table, colors, count, phase) in [
                (0x9762, 0x9778, 6, s.data[0x9816]),
                (0x976e, 0x979c, 5, s.data[0x9817]),
            ] {
                for color in 0..count {
                    let index = usize::from(word(
                        &s.data,
                        index_table + (usize::from(phase) + color) % count * 2,
                    )?);
                    if index >= 256 {
                        return Err("Naval flag palette index out of range".into());
                    }
                    for (component, output) in palette[index].iter_mut().enumerate() {
                        let dac = word(&s.data, colors + color * 6 + component * 2)?;
                        if dac > 63 {
                            return Err("Naval flag DAC component out of range".into());
                        }
                        *output = ((dac << 2) | (dac >> 4)) as u8;
                    }
                }
            }
        }
        f.palette = Some(palette);
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pennant_and_palette_clocks_are_independent_and_reentry_keeps_ring_phases() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        let mut scene = Briefing {
            image: Vec::new(),
            flags: Vec::new(),
            elapsed: 0,
            pennant: None,
            rotated: false,
        };
        scene.advance(&mut s);
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        assert_eq!(scene.pennant, None);
        for _ in 0..3 {
            scene.advance(&mut s);
        }
        assert_eq!(scene.pennant, Some(1));
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        for _ in 0..147 {
            scene.advance(&mut s);
        }
        // Thirty palette updates restore both rings together; the next is 1,1.
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        scene.reset();
        assert_eq!(scene.pennant, None);
        scene.advance(&mut s);
        assert_eq!(&s.data[0x9816..0x9818], &[2, 2]);
    }
}
