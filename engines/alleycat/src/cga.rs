// Copyright (c) 2026 analogued. License has not yet been selected.

//! The original packed, two-bank 320 by 200 framebuffer and rectangle operations.

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;
pub const MEMORY_SIZE: usize = 16_384;

#[derive(Clone)]
pub struct Framebuffer {
    bytes: [u8; MEMORY_SIZE],
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self {
            bytes: [0; MEMORY_SIZE],
        }
    }
}

/// The original CX rectangle argument: words in the low byte, rows in the high.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dimensions(pub u16);

impl Dimensions {
    pub fn width_bytes(self) -> usize {
        usize::from(self.0 as u8) * 2
    }
    pub fn rows(self) -> usize {
        usize::from(self.0 >> 8)
    }
    pub fn len(self) -> usize {
        self.width_bytes() * self.rows()
    }

    pub fn is_empty(self) -> bool {
        self.width_bytes() == 0 || self.rows() == 0
    }
}

/// Pixel X is rounded down to a packed-byte boundary, as the original does.
pub fn pixel_offset(x: u16, y: u8) -> u16 {
    u16::from(y & 1) * 0x2000 + u16::from(y >> 1) * 80 + (x >> 2)
}

fn next_row(offset: usize) -> usize {
    let toggled = offset ^ 0x2000;
    toggled + if toggled & 0x2000 == 0 { 80 } else { 0 }
}

impl Framebuffer {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bytes = bytes
            .try_into()
            .map_err(|_| "CGA memory must contain exactly 16384 bytes")?;
        Ok(Self { bytes })
    }

    pub fn bytes(&self) -> &[u8; MEMORY_SIZE] {
        &self.bytes
    }

    pub fn fill_visible(&mut self, packed: u8) {
        self.bytes[..8000].fill(packed);
        self.bytes[0x2000..0x2000 + 8000].fill(packed);
    }

    pub fn indices(&self) -> Vec<u8> {
        (0..HEIGHT)
            .flat_map(|y| {
                (0..WIDTH).map(move |x| {
                    let byte = self.bytes[usize::from(pixel_offset(x as u16, y as u8))];
                    (byte >> (6 - 2 * (x & 3))) & 3
                })
            })
            .collect()
    }

    fn row_offsets(&self, offset: u16, width: usize, rows: usize) -> Result<Vec<usize>, String> {
        if width == 0 || rows == 0 {
            return Err("CGA rectangles must have nonzero dimensions".into());
        }
        let mut start = usize::from(offset);
        let mut offsets = Vec::with_capacity(rows);
        for _ in 0..rows {
            if start.checked_add(width).is_none_or(|end| end > MEMORY_SIZE) {
                return Err("CGA rectangle exceeds framebuffer memory".into());
            }
            offsets.push(start);
            start = next_row(start);
        }
        Ok(offsets)
    }

    pub fn copy_bytes(
        &mut self,
        offset: u16,
        width: usize,
        rows: usize,
        source: &[u8],
    ) -> Result<(), String> {
        let offsets = self.row_offsets(offset, width, rows)?;
        if source.len() != width * rows {
            return Err("CGA rectangle source length disagrees with dimensions".into());
        }
        for (row, start) in offsets.into_iter().enumerate() {
            self.bytes[start..start + width]
                .copy_from_slice(&source[row * width..(row + 1) * width]);
        }
        Ok(())
    }

    pub fn copy_rect(
        &mut self,
        offset: u16,
        dimensions: Dimensions,
        source: &[u8],
    ) -> Result<(), String> {
        self.copy_bytes(offset, dimensions.width_bytes(), dimensions.rows(), source)
    }

    pub fn read_rect(&self, offset: u16, dimensions: Dimensions) -> Result<Vec<u8>, String> {
        let width = dimensions.width_bytes();
        let mut result = Vec::with_capacity(dimensions.len());
        for start in self.row_offsets(offset, width, dimensions.rows())? {
            result.extend_from_slice(&self.bytes[start..start + width]);
        }
        Ok(result)
    }

    pub fn and_rect(
        &mut self,
        offset: u16,
        dimensions: Dimensions,
        source: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.composite_rect(offset, dimensions, source, |screen, sprite| screen & sprite)
    }

    pub fn zero_transparent_rect(
        &mut self,
        offset: u16,
        dimensions: Dimensions,
        source: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.composite_rect(offset, dimensions, source, |screen, sprite| {
            let mut result = screen;
            for shift in [0, 2, 4, 6] {
                let mask = 3 << shift;
                if sprite & mask != 0 {
                    result = (result & !mask) | (sprite & mask);
                }
            }
            result
        })
    }

    fn composite_rect(
        &mut self,
        offset: u16,
        dimensions: Dimensions,
        source: &[u8],
        operation: impl Fn(u8, u8) -> u8,
    ) -> Result<Vec<u8>, String> {
        let width = dimensions.width_bytes();
        let offsets = self.row_offsets(offset, width, dimensions.rows())?;
        if source.len() != dimensions.len() {
            return Err("CGA sprite source length disagrees with dimensions".into());
        }
        let mut background = Vec::with_capacity(source.len());
        for (row, start) in offsets.into_iter().enumerate() {
            for column in 0..width {
                let screen = &mut self.bytes[start + column];
                background.push(*screen);
                *screen = operation(*screen, source[row * width + column]);
            }
        }
        Ok(background)
    }
}

/// Saves and restores the player's previous rectangle before each redraw.
#[derive(Default)]
pub struct PlayerCompositor {
    previous: Option<PlayerRectangle>,
}

struct PlayerRectangle {
    offset: u16,
    dimensions: Dimensions,
    background: Vec<u8>,
    source: Vec<u8>,
}

impl PlayerCompositor {
    pub(crate) fn dimensions(&self) -> Option<Dimensions> {
        self.previous.as_ref().map(|image| image.dimensions)
    }

    pub fn restore(&mut self, framebuffer: &mut Framebuffer) -> Result<(), String> {
        self.suspend(framebuffer)?;
        Ok(())
    }

    /// Remove the current sprite before modifying scenery; returning its exact
    /// pixels avoids selecting another idle image and advancing random state.
    pub(crate) fn suspend(
        &mut self,
        framebuffer: &mut Framebuffer,
    ) -> Result<Option<(Dimensions, Vec<u8>)>, String> {
        if let Some(previous) = self.previous.take() {
            framebuffer.copy_rect(previous.offset, previous.dimensions, &previous.background)?;
            return Ok(Some((previous.dimensions, previous.source)));
        }
        Ok(None)
    }

    pub fn draw(
        &mut self,
        framebuffer: &mut Framebuffer,
        offset: u16,
        dimensions: Dimensions,
        source: &[u8],
    ) -> Result<(), String> {
        // Validate before restoring so malformed input cannot erase the current cat.
        framebuffer.row_offsets(offset, dimensions.width_bytes(), dimensions.rows())?;
        if source.len() != dimensions.len() {
            return Err("CGA player source length disagrees with dimensions".into());
        }
        self.restore(framebuffer)?;
        let background = framebuffer.and_rect(offset, dimensions, source)?;
        self.previous = Some(PlayerRectangle {
            offset,
            dimensions,
            background,
            source: source.to_vec(),
        });
        Ok(())
    }

    pub fn draw_idle(
        &mut self,
        framebuffer: &mut Framebuffer,
        offset: u16,
        top: &[u8],
        bottom: &[u8],
    ) -> Result<(), String> {
        let source: Vec<u8> = top.iter().chain(bottom).copied().collect();
        self.draw(framebuffer, offset, Dimensions(0x0c02), &source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_use_two_banks_and_most_significant_pair_first() {
        let mut screen = Framebuffer::default();
        screen.copy_bytes(0, 1, 3, &[0x1b, 0xe4, 0x55]).unwrap();
        let indices = screen.indices();
        assert_eq!(&indices[..4], &[0, 1, 2, 3]);
        assert_eq!(&indices[320..324], &[3, 2, 1, 0]);
        assert_eq!(&indices[640..644], &[1, 1, 1, 1]);
        assert_eq!(pixel_offset(3, 1), 0x2000);
        assert_eq!(pixel_offset(4, 2), 81);
    }

    #[test]
    fn odd_row_rectangles_round_trip_across_banks() {
        let mut screen = Framebuffer::default();
        let bytes: Vec<u8> = (1..=12).collect();
        screen
            .copy_rect(0x2051, Dimensions(0x0302), &bytes)
            .unwrap();
        assert_eq!(screen.read_rect(0x2051, Dimensions(0x0302)).unwrap(), bytes);
        assert_eq!(&screen.bytes()[161..165], &[5, 6, 7, 8]);
        assert_eq!(&screen.bytes()[0x20a1..0x20a5], &[9, 10, 11, 12]);
    }

    #[test]
    fn and_player_composition_restores_previous_position_exactly() {
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        let original = screen.bytes().to_vec();
        let mut player = PlayerCompositor::default();
        player
            .draw(&mut screen, 0, Dimensions(0x0101), &[0x1b, 0xff])
            .unwrap();
        assert_eq!(&screen.bytes()[..2], &[0x0a, 0xaa]);
        player
            .draw(&mut screen, 4, Dimensions(0x0101), &[0, 0])
            .unwrap();
        assert_eq!(&screen.bytes()[..2], &[0xaa, 0xaa]);
        player.restore(&mut screen).unwrap();
        assert_eq!(screen.bytes().as_slice(), original);
    }

    #[test]
    fn transparent_and_and_compositors_have_different_contracts() {
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xff);
        let saved = screen
            .zero_transparent_rect(0, Dimensions(0x0101), &[0x1b, 0])
            .unwrap();
        assert_eq!(saved, [0xff, 0xff]);
        assert_eq!(&screen.bytes()[..2], &[0xdb, 0xff]);
    }

    #[test]
    fn invalid_rectangles_do_not_partially_modify_screen() {
        let mut screen = Framebuffer::default();
        assert!(
            screen
                .copy_rect(0x3fff, Dimensions(0x0101), &[1, 2])
                .is_err()
        );
        assert!(screen.copy_rect(0, Dimensions(0), &[]).is_err());
        assert!(screen.copy_rect(0, Dimensions(0x0101), &[1]).is_err());
        assert!(screen.bytes().iter().all(|&byte| byte == 0));
    }
}
