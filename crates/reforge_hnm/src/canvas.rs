// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{HEIGHT, Video, WIDTH, invalid, unsupported};
use std::io;

/// Ordinary HNM1 bitmap composition on a 320x200 indexed background. An engine
/// may instead consume Video directly for its own placement/scaling policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Canvas {
    pixels: Vec<u8>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT],
        }
    }
}

impl Canvas {
    pub fn from_pixels(pixels: Vec<u8>) -> io::Result<Self> {
        if pixels.len() != WIDTH * HEIGHT {
            return Err(invalid("HNM1 background must be 320x200"));
        }
        Ok(Self { pixels })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn apply(&mut self, update: &Video) -> io::Result<()> {
        let width = update.width as usize;
        let height = update.height as usize;
        if width == 0
            || height == 0
            || width > WIDTH
            || height > HEIGHT
            || update.pixels.len() != width * height
        {
            return Err(invalid("invalid HNM1 bitmap dimensions or raster"));
        }
        if update.flags & !0x8600 != 0 || !matches!(update.mode, 0xfe | 0xff) {
            return Err(unsupported("unsupported HNM1 bitmap flags or drawing mode"));
        }
        // Direct-to-framebuffer expansion is opaque, including zero pixels,
        // regardless of the nominal drawing mode. It leaves the remaining tail.
        if update.flags & 0x0400 != 0 {
            self.pixels[..update.pixels.len()].copy_from_slice(&update.pixels);
            return Ok(());
        }
        let x = i32::from(update.x as i16);
        let y = i32::from(update.y as i16);
        for row in 0..height {
            let target_y = y + row as i32;
            if !(0..HEIGHT as i32).contains(&target_y) {
                continue;
            }
            for column in 0..width {
                let target_x = x + column as i32;
                if !(0..WIDTH as i32).contains(&target_x) {
                    continue;
                }
                let pixel = update.pixels[row * width + column];
                if pixel != 0 || update.mode == 0xfe {
                    self.pixels[target_y as usize * WIDTH + target_x as usize] = pixel;
                }
            }
        }
        Ok(())
    }
}
