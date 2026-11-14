// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::assets::{Assets, Bitmap, Result, word};
use std::fs;
use std::path::Path;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;

#[derive(Clone)]
pub struct Frame {
    pub pixels: Vec<u8>,
    pub palette: Option<crate::water::Palette>,
}

impl Frame {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT],
            palette: None,
        }
    }

    pub fn point(&mut self, x: i32, y: i32, color: u8) {
        if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
            self.pixels[y as usize * WIDTH + x as usize] = color;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u8) {
        for py in y.max(0)..(y + height).min(HEIGHT as i32) {
            for px in x.max(0)..(x + width).min(WIDTH as i32) {
                self.point(px, py, color);
            }
        }
    }

    pub fn blit(&mut self, bitmap: &Bitmap, x: i32, y: i32) {
        let left = x - bitmap.origin_x as i32;
        let top = y - bitmap.origin_y as i32;
        for sy in 0..bitmap.height {
            for sx in 0..bitmap.width {
                let index = sy * bitmap.width + sx;
                if bitmap.visible[index] {
                    self.point(left + sx as i32, top + sy as i32, bitmap.pixels[index]);
                }
            }
        }
    }

    pub fn flood_replace(&mut self, x: i32, y: i32, color: u8) {
        if !(0..319).contains(&x) || !(0..199).contains(&y) {
            return;
        }
        let old = self.pixels[y as usize * WIDTH + x as usize];
        if old == color {
            return;
        }
        let mut pending = vec![(x, y)];
        while let Some((x, y)) = pending.pop() {
            if !(0..319).contains(&x) || !(0..199).contains(&y) {
                continue;
            }
            let offset = y as usize * WIDTH + x as usize;
            if self.pixels[offset] != old {
                continue;
            }
            self.pixels[offset] = color;
            pending.extend([(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]);
        }
    }

    pub fn bevel(&mut self, left: i32, top: i32, right: i32, bottom: i32, fill: u8) {
        self.rect(left, top, right - left + 1, bottom - top + 1, fill);
        for (x, y0, y1) in [
            (left, top, bottom),
            (left + 1, top + 1, bottom - 1),
            (right, top, bottom),
            (right - 1, top + 1, bottom - 1),
            (left + 3, top + 3, bottom - 3),
            (right - 3, top + 3, bottom - 3),
        ] {
            self.rect(x, y0, 1, y1 - y0 + 1, 8);
        }
        for (y, x0, x1, color) in [
            (top + 3, left + 3, right - 3, 8),
            (bottom - 3, left + 3, right - 3, 8),
            (bottom, left, right, 0),
            (bottom - 1, left + 1, right - 1, 0),
            (top, left, right, 15),
            (top + 1, left + 1, right - 1, 15),
        ] {
            self.rect(x0, y, x1 - x0 + 1, 1, color);
        }
    }

    pub fn center_text(
        &mut self,
        assets: &Assets,
        text: &str,
        center: i32,
        y: i32,
        color: u8,
    ) -> Result<()> {
        let widths = word(&assets.data_segment, 0xa05c)? as usize;
        let tracking = assets.data_segment[0xa064] as i32;
        let width: i32 = text
            .bytes()
            .filter(|v| (32..128).contains(v))
            .map(|v| assets.data_segment[widths + (v - 32) as usize] as i32 + tracking)
            .sum();
        self.text(assets, text, center - width / 2, y, color)
    }

    pub fn text(&mut self, assets: &Assets, text: &str, x: i32, y: i32, color: u8) -> Result<()> {
        self.text_font(assets, text, x, y, color, 0xa05c)
    }

    pub fn text_font(
        &mut self,
        assets: &Assets,
        text: &str,
        mut x: i32,
        mut y: i32,
        color: u8,
        record: usize,
    ) -> Result<()> {
        let ds = &assets.data_segment;
        let widths = word(ds, record)? as usize;
        let bearings = word(ds, record + 2)? as usize;
        let glyphs = word(ds, record + 4)? as usize;
        let height = ds[record + 6] as usize;
        let tracking = ds[record + 8] as i32;
        let shift = ds[record + 9];
        if shift != 0 {
            return Err("Prototype font path expects the default eight-bit font".into());
        }
        let left = x;
        for byte in text.bytes() {
            if byte == b'\n' {
                x = left;
                y += (height + ds[record + 7] as usize) as i32;
                continue;
            }
            if !(32..128).contains(&byte) {
                continue;
            }
            let glyph = (byte - 32) as usize;
            let width = ds[widths + glyph] as usize;
            let top = y + ds[bearings + glyph] as i32;
            for row in 0..height {
                let bits = ds[glyphs + glyph * height + row];
                for column in 0..width.min(8) {
                    if bits & (0x80 >> column) != 0 {
                        self.point(x + column as i32, top + row as i32, color);
                    }
                }
            }
            x += width as i32 + tracking;
        }
        Ok(())
    }

    pub fn center_lines(
        &mut self,
        assets: &Assets,
        text: &str,
        center: i32,
        mut y: i32,
        color: u8,
    ) -> Result<()> {
        for line in text.split('\n') {
            self.center_text(assets, line, center, y, color)?;
            y += 11;
        }
        Ok(())
    }

    pub fn palette<'a>(&'a self, assets: &'a Assets) -> &'a crate::water::Palette {
        self.palette.as_ref().unwrap_or(&assets.palette)
    }

    pub fn export(&self, assets: &Assets, directory: &Path, name: &str) -> Result<()> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        fs::write(directory.join(format!("{name}.idx")), &self.pixels)
            .map_err(|e| e.to_string())?;
        let palette = self.palette(assets);
        let bytes: Vec<u8> = palette.iter().flatten().copied().collect();
        fs::write(directory.join(format!("{name}.pal")), bytes).map_err(|e| e.to_string())?;
        let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for pixel in &self.pixels {
            ppm.extend_from_slice(&palette[*pixel as usize]);
        }
        fs::write(directory.join(format!("{name}.ppm")), ppm).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_clipping() {
        let bitmap = Bitmap {
            width: 2,
            height: 2,
            origin_x: 1,
            origin_y: 1,
            pixels: vec![1, 2, 3, 4],
            visible: vec![true; 4],
        };
        let mut frame = Frame::new();
        frame.blit(&bitmap, 0, 0);
        assert_eq!(frame.pixels[0], 4);
        assert_eq!(frame.pixels.iter().filter(|v| **v != 0).count(), 1);
    }
}
