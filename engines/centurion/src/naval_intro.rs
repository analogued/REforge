// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Naval briefing pennant and two independent VGA palette rings (C105..C273).
use crate::assets::{Assets, Bitmap, Result, VideoMode, word};
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

    pub fn advance(&mut self, s: &mut Campaign, video: VideoMode) {
        self.elapsed = self.elapsed.wrapping_add(1);
        // The palette routine has its own timestamp; the initial call is due
        // immediately after the image loads, then strictly more than four ticks.
        if video == VideoMode::Vga && (self.elapsed - 1).is_multiple_of(5) {
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
            f.rect(38, 2, 31, 14, assets.video.color(128, 11));
            f.blit(&self.flags[p], 38, 5);
        }
        let mut palette = assets.palette;
        if self.rotated && assets.video == crate::assets::VideoMode::Vga {
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
    fn ega_animates_flags_on_cyan_without_running_vga_palette_rings() {
        for (video, clear, phases) in [(VideoMode::Vga, 128, [4, 4]), (VideoMode::Ega, 11, [0, 0])]
        {
            let mut assets = Assets::test_data(Vec::new());
            assets.video = video;
            let mut state = Campaign::test_data(vec![0; 65536]);
            let flag = Bitmap {
                width: 1,
                height: 1,
                origin_x: 0,
                origin_y: 0,
                pixels: vec![15],
                visible: vec![true],
            };
            let mut scene = Briefing {
                image: vec![3; 64000],
                flags: vec![flag; 4],
                elapsed: 0,
                pennant: None,
                rotated: false,
            };
            for _ in 0..16 {
                scene.advance(&mut state, video);
            }
            assert_eq!(scene.pennant, Some(0));
            assert_eq!(&state.data[0x9816..0x9818], &phases);
            // Isolate the clear/flag composition from VGA's DAC tables here.
            scene.rotated = false;
            let frame = scene.render(&assets, &state).unwrap();
            assert_eq!(frame.pixels[2 * 320 + 38], clear);
            assert_eq!(frame.pixels[15 * 320 + 68], clear);
            assert_eq!(frame.pixels[2 * 320 + 69], 3);
            assert_eq!(frame.pixels[5 * 320 + 38], 15);
        }
    }

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
        scene.advance(&mut s, VideoMode::Vga);
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        assert_eq!(scene.pennant, None);
        for _ in 0..3 {
            scene.advance(&mut s, VideoMode::Vga);
        }
        assert_eq!(scene.pennant, Some(1));
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        for _ in 0..147 {
            scene.advance(&mut s, VideoMode::Vga);
        }
        // Thirty palette updates restore both rings together; the next is 1,1.
        assert_eq!(&s.data[0x9816..0x9818], &[1, 1]);
        scene.reset();
        assert_eq!(scene.pennant, None);
        scene.advance(&mut s, VideoMode::Vga);
        assert_eq!(&s.data[0x9816..0x9818], &[2, 2]);
    }
}
