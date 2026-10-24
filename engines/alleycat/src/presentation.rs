// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Shared work descriptions and native wipes for success, failure and room entry.

use crate::assets::{set_word, word};
use crate::framebuffer::{Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Work {
    pub bytes_drawn: usize,
    pub complete: bool,
    pub graphics: Graphics,
    pub hearts: crate::hearts_completion::PresentationWork,
    pub failure: crate::failure::PresentationWork,
    pub bookkeeping: Bookkeeping,
    pub polling: Option<PollWork>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Poll {
    Melody,
    Reveal,
    Dance,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PollWork {
    pub kind: Poll,
    pub tone: bool,
}

/// Arithmetic/state work emitted only by the phase that actually performs it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Bookkeeping {
    pub progress: bool,
    pub award: Option<AwardWork>,
    pub life: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AwardWork {
    pub bonus: u16,
    /// None for an ordinary award; Hearts adds the award this many times.
    pub multiplier: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Graphics {
    pub wipe: Option<WipeWork>,
    pub face_preparations: u8,
    pub score_rows: u8,
    pub score_bytes: usize,
    pub score_grid: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WipeWork {
    pub rows: u16,
    pub bytes: usize,
    pub first: bool,
    pub growing: bool,
    pub clips: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Wipe {
    x: u16,
    y: u8,
    width: u16,
    height: u8,
    edges: u8,
    packed: u8,
    started: bool,
}

impl Wipe {
    pub(crate) fn new(x: u16, y: u8, packed: u8) -> Self {
        Self {
            x: x.wrapping_add(12) & 0xfff0,
            y: y.wrapping_add(8),
            width: 1,
            height: 8,
            edges: 0,
            packed,
            started: false,
        }
    }

    /// CS:1c67. Geometry grows in pixels, but REP STOSW rounds width down to
    /// whole eight-pixel words. The first one-pixel rectangle writes no bytes.
    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        screen: &mut Framebuffer,
        sound: &mut crate::sound::Sound,
    ) -> Result<Work, String> {
        let mut geometry = WipeWork {
            rows: u16::from(self.height),
            bytes: usize::from(self.width >> 3) * 2 * usize::from(self.height),
            first: !self.started,
            growing: self.edges != 15,
            clips: 0,
        };
        self.started = true;
        if data[0] != 0 {
            sound.tone(
                word(data, 0x5a5a + usize::from(word(data, 0x5a56) & 6)),
                crate::sound::Category::Effects,
            );
            set_word(data, 0x5a56, word(data, 0x5a56).wrapping_add(2));
        }
        set_word(data, 0x1832, self.x);
        data[0x1834] = self.y;
        set_word(data, 0x1835, self.width);
        data[0x1837] = self.height;
        data[0x1838] = self.edges;
        set_word(data, 0x1839, u16::from(self.packed) * 0x101);
        let width = usize::from(self.width >> 3) * 2;
        let bytes_drawn = width * usize::from(self.height);
        if width != 0 {
            screen.copy_bytes(
                pixel_offset(self.x, self.y),
                width,
                usize::from(self.height),
                &vec![self.packed; bytes_drawn],
            )?;
        }
        if self.edges == 15 {
            return Ok(Work {
                bytes_drawn,
                complete: true,
                graphics: Graphics {
                    wipe: Some(geometry),
                    ..Graphics::default()
                },
                hearts: Default::default(),
                failure: Default::default(),
                bookkeeping: Default::default(),
                polling: None,
            });
        }
        geometry.clips = self.grow();
        Ok(Work {
            bytes_drawn,
            complete: false,
            graphics: Graphics {
                wipe: Some(geometry),
                ..Graphics::default()
            },
            hearts: Default::default(),
            failure: Default::default(),
            bookkeeping: Default::default(),
            polling: None,
        })
    }
    fn grow(&mut self) -> u8 {
        let mut clips = 0;
        self.width = self.width.wrapping_add(32);
        self.height = self.height.wrapping_add(16);
        let (x, left) = self.x.overflowing_sub(16);
        self.x = if left { 0 } else { x };
        if left {
            clips += 1;
            self.edges |= 1;
        }
        if self.x.wrapping_add(self.width) >= 320 {
            clips += 1;
            self.width = 320 - self.x;
            self.edges |= 2;
        }
        let (y, top) = self.y.overflowing_sub(8);
        self.y = if top { 0 } else { y };
        if top {
            clips += 1;
            self.edges |= 4;
        }
        let (bottom, overflow) = self.y.overflowing_add(self.height);
        if overflow || bottom >= 200 {
            clips += 1;
            self.height = 200 - self.y;
            self.edges |= 8;
        }
        clips
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_width_wipe_still_reports_rows_and_growth_before_drawing() {
        let mut wipe = Wipe::new(160, 96, 0);
        let mut data = vec![0; crate::assets::DATA_LENGTH];
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        let before = screen.bytes().to_vec();
        let mut sound = crate::sound::Sound::default();
        let first = wipe.advance(&mut data, &mut screen, &mut sound).unwrap();
        assert_eq!(screen.bytes().as_slice(), before);
        let first = first.graphics.wipe.unwrap();
        assert_eq!((first.rows, first.bytes), (8, 0));
        assert!(first.first && first.growing);
        let next = wipe.advance(&mut data, &mut screen, &mut sound).unwrap();
        let geometry = next.graphics.wipe.unwrap();
        assert!(!geometry.first);
        assert_eq!((geometry.rows, geometry.bytes), (24, 192));
        assert_eq!(next.bytes_drawn, geometry.bytes);
    }
    #[test]
    fn wipe_rounds_initial_width_to_zero_then_fills_every_visible_pixel() {
        for (x, y) in [(8, 0), (144, 96), (290, 180)] {
            let mut wipe = Wipe::new(x, y, 0);
            let mut screen = Framebuffer::default();
            screen.fill_visible(0xaa);
            let mut data = vec![0; crate::assets::DATA_LENGTH];
            assert_eq!(
                wipe.advance(&mut data, &mut screen, &mut crate::sound::Sound::default())
                    .unwrap()
                    .bytes_drawn,
                0
            );
            for _ in 0..40 {
                if wipe
                    .advance(&mut data, &mut screen, &mut crate::sound::Sound::default())
                    .unwrap()
                    .complete
                {
                    break;
                }
            }
            assert_eq!(wipe.edges, 15);
            assert!(screen.indices().iter().all(|pixel| *pixel == 0));
            assert_eq!((word(&data, 0x1835), data[0x1837]), (320, 200));
        }
    }
}
