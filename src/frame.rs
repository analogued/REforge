// Copyright (c) 2026 analogued. Project license has not been selected.
//! Packed four-color frame storage and ordinary masked sprite drawing.

use std::io::{self, Write};

use crate::{Error, assets::Sprite};

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;
pub const ROW_BYTES: usize = WIDTH / 4;
pub const FRAME_BYTES: usize = ROW_BYTES * HEIGHT;
pub const PALETTE: [[u8; 3]; 4] = [[0, 0, 0], [85, 255, 255], [255, 85, 255], [255, 255, 255]];

/// Byte-x and scanline-y placement. Width also supplies the object's source stride.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x_bytes: i32,
    pub y: i32,
    pub width_bytes: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    packed: Vec<u8>,
}

impl Frame {
    pub fn from_packed(packed: &[u8]) -> Result<Self, Error> {
        if packed.len() != FRAME_BYTES {
            return Err(Error::InvalidData(format!(
                "a packed frame requires {FRAME_BYTES} bytes"
            )));
        }
        Ok(Self {
            packed: packed.to_vec(),
        })
    }

    /// Import the visible bytes from a mode-4 CGA framebuffer.
    pub fn from_cga(vram: &[u8]) -> Result<Self, Error> {
        if vram.len() < 0x3f40 {
            return Err(Error::InvalidData("truncated CGA framebuffer".into()));
        }
        let mut packed = vec![0; FRAME_BYTES];
        for y in 0..HEIGHT {
            let offset = (y % 2) * 0x2000 + (y / 2) * ROW_BYTES;
            packed[y * ROW_BYTES..(y + 1) * ROW_BYTES]
                .copy_from_slice(&vram[offset..offset + ROW_BYTES]);
        }
        Ok(Self { packed })
    }

    pub fn packed(&self) -> &[u8] {
        &self.packed
    }

    /// Apply a packed-byte mask for procedural world drawing.
    pub(crate) fn mask_byte(&mut self, x_bytes: usize, y: usize, mask: u8) {
        if x_bytes < ROW_BYTES && y < HEIGHT {
            self.packed[y * ROW_BYTES + x_bytes] &= mask;
        }
    }

    /// Export visible pixels into two CGA banks; unused bank bytes are zero.
    pub fn to_cga(&self) -> Vec<u8> {
        let mut vram = vec![0; 0x4000];
        for y in 0..HEIGHT {
            let offset = (y % 2) * 0x2000 + (y / 2) * ROW_BYTES;
            vram[offset..offset + ROW_BYTES]
                .copy_from_slice(&self.packed[y * ROW_BYTES..(y + 1) * ROW_BYTES]);
        }
        vram
    }

    pub fn rgb(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(WIDTH * HEIGHT * 3);
        for byte in &self.packed {
            for shift in [6, 4, 2, 0] {
                rgb.extend_from_slice(&PALETTE[usize::from((byte >> shift) & 3)]);
            }
        }
        rgb
    }

    pub fn write_ppm(&self, mut output: impl Write) -> io::Result<()> {
        write!(output, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
        output.write_all(&self.rgb())
    }

    pub fn draw_sprite(&mut self, sprite: &Sprite, rect: Rect) -> Result<(), Error> {
        self.draw_sprite_clipped(
            sprite,
            rect,
            Rect {
                x_bytes: 0,
                y: 0,
                width_bytes: ROW_BYTES,
                height: HEIGHT,
            },
        )
    }

    /// Draw within a byte-aligned region, retaining pixels outside that region.
    pub fn draw_sprite_clipped(
        &mut self,
        sprite: &Sprite,
        rect: Rect,
        clip: Rect,
    ) -> Result<(), Error> {
        let size = rect.width_bytes.checked_mul(rect.height);
        if rect.width_bytes == 0
            || rect.height == 0
            || rect.width_bytes > sprite.width_bytes
            || rect.height > sprite.height
            || size.is_none_or(|size| size > sprite.color.len() || size > sprite.mask.len())
        {
            return Err(Error::InvalidData("invalid sprite draw dimensions".into()));
        }
        let x = i64::from(rect.x_bytes);
        let y = i64::from(rect.y);
        // Dimensions are bounded by decoded sprite buffers before converting.
        let width = i64::try_from(rect.width_bytes)
            .map_err(|_| Error::InvalidData("sprite width is too large".into()))?;
        let height = i64::try_from(rect.height)
            .map_err(|_| Error::InvalidData("sprite height is too large".into()))?;
        let clip_width = i64::try_from(clip.width_bytes)
            .map_err(|_| Error::InvalidData("clip width is too large".into()))?;
        let clip_height = i64::try_from(clip.height)
            .map_err(|_| Error::InvalidData("clip height is too large".into()))?;
        let clip_x = i64::from(clip.x_bytes);
        let clip_y = i64::from(clip.y);
        let left = x.max(clip_x).clamp(0, ROW_BYTES as i64) as usize;
        let right = x
            .saturating_add(width)
            .min(clip_x.saturating_add(clip_width))
            .clamp(0, ROW_BYTES as i64) as usize;
        let top = y.max(clip_y).clamp(0, HEIGHT as i64) as usize;
        let bottom = y
            .saturating_add(height)
            .min(clip_y.saturating_add(clip_height))
            .clamp(0, HEIGHT as i64) as usize;
        for dy in top..bottom {
            for dx in left..right {
                let source = (dy as i64 - y) as usize * rect.width_bytes + (dx as i64 - x) as usize;
                let destination = dy * ROW_BYTES + dx;
                self.packed[destination] =
                    (self.packed[destination] & sprite.mask[source]) | sprite.color[source];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(width: usize, height: usize, color: Vec<u8>, mask: Vec<u8>) -> Sprite {
        Sprite {
            width_bytes: width,
            height,
            descriptor: [0; 7],
            color,
            mask,
        }
    }

    #[test]
    fn cga_banks_and_pixel_order() {
        let mut vram = vec![0; 0x4000];
        vram[0] = 0x1b;
        vram[0x2000] = 0xe4;
        vram[80] = 0x55;
        vram[0x3f3f] = 0xff;
        let frame = Frame::from_cga(&vram).unwrap();
        assert_eq!(&frame.packed()[..1], &[0x1b]);
        assert_eq!(frame.packed()[80], 0xe4);
        assert_eq!(frame.packed()[160], 0x55);
        assert_eq!(frame.packed()[FRAME_BYTES - 1], 0xff);
        assert_eq!(
            &frame.rgb()[..12],
            &[0, 0, 0, 85, 255, 255, 255, 85, 255, 255, 255, 255]
        );
        assert_eq!(frame.to_cga(), vram);
        assert!(Frame::from_cga(&vram[..0x3f3f]).is_err());
    }

    #[test]
    fn masks_clip_with_original_source_stride() {
        let mut frame = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let sprite = sprite(2, 2, vec![0xf0, 0, 0, 0x05], vec![0, 255, 255, 0xf0]);
        frame
            .draw_sprite(
                &sprite,
                Rect {
                    x_bytes: -1,
                    y: -1,
                    width_bytes: 2,
                    height: 2,
                },
            )
            .unwrap();
        assert_eq!(frame.packed()[0], 0xa5);
        assert!(frame.packed()[1..].iter().all(|byte| *byte == 0xaa));
        frame
            .draw_sprite(
                &sprite,
                Rect {
                    x_bytes: 79,
                    y: 199,
                    width_bytes: 2,
                    height: 2,
                },
            )
            .unwrap();
        assert_eq!(frame.packed()[FRAME_BYTES - 1], 0xf0);
        assert!(
            frame.packed()[1..FRAME_BYTES - 1]
                .iter()
                .all(|byte| *byte == 0xaa)
        );
    }

    #[test]
    fn object_width_overrides_descriptor_stride() {
        let mut frame = Frame::from_packed(&vec![0; FRAME_BYTES]).unwrap();
        let sprite = sprite(4, 2, vec![1, 2, 3, 4, 5, 6, 7, 8], vec![0; 8]);
        frame
            .draw_sprite(
                &sprite,
                Rect {
                    x_bytes: 0,
                    y: 0,
                    width_bytes: 3,
                    height: 2,
                },
            )
            .unwrap();
        assert_eq!(&frame.packed()[..4], &[1, 2, 3, 0]);
        assert_eq!(&frame.packed()[80..84], &[4, 5, 6, 0]);
        let before = frame.clone();
        assert!(
            frame
                .draw_sprite(
                    &sprite,
                    Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: usize::MAX,
                        height: 2
                    }
                )
                .is_err()
        );
        assert_eq!(frame, before);
    }

    #[test]
    fn viewport_clipping_preserves_surround_and_original_source_rows() {
        let mut frame = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let sprite = sprite(4, 2, vec![1, 2, 3, 4, 5, 6, 7, 8], vec![0; 8]);
        frame
            .draw_sprite_clipped(
                &sprite,
                Rect {
                    x_bytes: 2,
                    y: 10,
                    width_bytes: 4,
                    height: 2,
                },
                Rect {
                    x_bytes: 3,
                    y: 11,
                    width_bytes: 2,
                    height: 1,
                },
            )
            .unwrap();
        assert_eq!(
            &frame.packed()[11 * 80 + 2..11 * 80 + 6],
            &[0xaa, 6, 7, 0xaa]
        );
        assert!(frame.packed()[..11 * 80 + 3].iter().all(|b| *b == 0xaa));
        assert!(frame.packed()[11 * 80 + 5..].iter().all(|b| *b == 0xaa));
    }
}
