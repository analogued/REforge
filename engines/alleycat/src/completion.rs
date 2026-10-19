// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Ordinary-room success presentation, with external cat faces and BIOS-style text.
//! Tick waits and speaker writes are independent of host rendering.

use crate::assets::{DATA_LENGTH, set_word, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Work {
    pub bytes_drawn: usize,
    pub complete: bool,
    pub graphics: Graphics,
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

#[derive(Clone, Copy, Debug)]
enum Phase {
    Black(Wipe),
    Silhouettes,
    Award,
    Reveal { y: i16, since: Option<u16> },
    Digits,
    Hold { start: u16, sample: Option<u16> },
    Fill(Wipe),
    Complete,
}

pub(crate) struct Completion {
    phase: Phase,
    center: (u16, u8),
    room: u16,
    background: u8,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable completion data segment is truncated".into());
    }
    Ok(())
}

fn prepare_face(data: &mut [u8], mask: u8) {
    // CS:3a96 uses a writable 60-byte scratch sprite at DS:000e.
    for index in 0..60 {
        data[0x0e + index] = data[0x35e0 + index] & mask;
    }
}

fn draw_row(data: &mut [u8], screen: &mut Framebuffer, y: u16) -> Result<usize, String> {
    let base = y * 40;
    set_word(data, 0x369c, base);
    // CS:2b24 consumes byte-width rectangle lists from the executable.
    crate::room::draw_list(data, screen, 0x361c, base)?;
    Ok(usize::from(word(data, 0x361c) as u8) * usize::from(word(data, 0x361c) >> 8) * 4)
}

pub(crate) fn digit(value: u8) -> Result<[u8; 16], String> {
    if value > 9 {
        return Err("Completion score contains an invalid decimal digit".into());
    }
    crate::font::character(b'0' + value)
}

impl Completion {
    pub(crate) fn new(x: u16, y: u8, room: u16) -> Result<Self, String> {
        if !(1..=6).contains(&room) {
            return Err("Ordinary completion requires a room number in 1..6".into());
        }
        if x > 296 || y > 180 {
            return Err("Completion wipe center is outside the ordinary player area".into());
        }
        Ok(Self {
            phase: Phase::Black(Wipe::new(x, y, 0)),
            center: (x, y),
            room,
            background: 0,
        })
    }

    pub(crate) fn background(&self) -> u8 {
        self.background
    }

    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        screen: &mut Framebuffer,
        tick: u16,
        retrace: bool,
        sound: &mut crate::sound::Sound,
    ) -> Result<Work, String> {
        check_data(data)?;
        let mut work = Work::default();
        match self.phase {
            Phase::Black(mut wipe) => {
                work = wipe.advance(data, screen, sound)?;
                self.phase = if work.complete {
                    Phase::Silhouettes
                } else {
                    Phase::Black(wipe)
                };
                work.complete = false;
            }
            Phase::Silhouettes => {
                crate::room::begin_room_completion(data)?;
                prepare_face(data, 0xaa);
                work.graphics.face_preparations = 1;
                work.graphics.score_grid = true;
                data[0x369f] = 0;
                set_word(data, 0x369a, 0);
                set_word(data, 0x5a62, 0);
                data[0x5a82] = 0;
                for y in (0..=176).step_by(16).rev() {
                    work.bytes_drawn += draw_row(data, screen, y)?;
                    work.graphics.score_rows += 1;
                }
                work.graphics.score_bytes = work.bytes_drawn;
                self.phase = Phase::Award;
            }
            Phase::Award => {
                crate::room::award_room_completion(data, self.room, tick)?;
                prepare_face(data, 0xff);
                work.graphics.face_preparations = 1;
                set_word(data, 0x369a, u16::from(data[0x369e]) * 40);
                data[0x369f] = 1;
                set_word(data, 0x5a62, 0);
                data[0x5a82] = 0;
                self.phase = Phase::Reveal {
                    y: 176,
                    since: None,
                };
            }
            Phase::Reveal { y, since } => {
                if let Some(start) = since {
                    if tick.wrapping_sub(start) < 2 {
                        return Ok(work);
                    }
                }
                if y < i16::from(data[0x369e]) {
                    self.phase = Phase::Digits;
                } else {
                    work.bytes_drawn = draw_row(data, screen, y as u16)?;
                    work.graphics.score_rows = 1;
                    work.graphics.score_bytes = work.bytes_drawn;
                    if data[0] != 0 {
                        sound.tone(
                            word(data, 0x5a64 + usize::from(word(data, 0x5a62))),
                            crate::sound::Category::Music,
                        );
                        set_word(data, 0x5a62, word(data, 0x5a62).wrapping_add(2));
                    }
                    set_word(data, 0x3695, tick);
                    self.phase = Phase::Reveal {
                        y: y - 16,
                        since: Some(tick),
                    };
                }
            }
            Phase::Digits => {
                for index in 3..7 {
                    let pixels = digit(data[0x368d + index])?;
                    screen.copy_rect(
                        pixel_offset(144 + (index as u16 - 3) * 8, data[0x369e]),
                        Dimensions(0x0801),
                        &pixels,
                    )?;
                    work.bytes_drawn += pixels.len();
                }
                set_word(data, 0x36a0, 7);
                set_word(data, 0x3695, tick);
                self.phase = Phase::Hold {
                    start: tick,
                    sample: None,
                };
            }
            Phase::Hold { start, sample } => {
                let sampled = if let Some(sample) = sample {
                    sample
                } else {
                    if data[0] != 0 && word(data, 0x5a80) != tick {
                        set_word(data, 0x5a80, tick);
                        data[0x5a82] = data[0x5a82].wrapping_add(1);
                        let index = usize::from(word(data, 0x5a62))
                            + if data[0x5a82] & 1 == 0 { 2 } else { 0 };
                        sound.tone(word(data, 0x5a64 + index), crate::sound::Category::Music);
                    }
                    tick
                };
                // CS:3a1c saves DX before waiting for retrace. Interrupts during
                // the wait do not replace that sample for palette/exit tests.
                if retrace {
                    self.background = if sampled & 4 == 0 { 2 } else { 0 };
                    if sampled.wrapping_sub(start) >= 30 {
                        self.background = 0;
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        self.phase = Phase::Hold {
                            start,
                            sample: None,
                        };
                    }
                } else {
                    self.phase = Phase::Hold {
                        start,
                        sample: Some(sampled),
                    };
                }
            }
            Phase::Fill(mut wipe) => {
                work = wipe.advance(data, screen, sound)?;
                self.phase = if work.complete {
                    Phase::Complete
                } else {
                    Phase::Fill(wipe)
                };
            }
            Phase::Complete => work.complete = true,
        }
        Ok(work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x36d6, 0x6100);
        data[0x6100..0x6107].copy_from_slice(&[0, 0, 0, 3, 5, 0, 0]);
        set_word(&mut data, 0x361c, 0x0c05);
        for (index, offset) in [6, 16, 59, 69].into_iter().enumerate() {
            set_word(&mut data, 0x361e + index * 4, 0x0e);
            set_word(&mut data, 0x3620 + index * 4, offset);
        }
        set_word(&mut data, 0x362e, 0xffff);
        data[0x35e0..0x361c].fill(0xff);
        data[0x1f80] = 3;
        data
    }

    #[test]
    fn zero_width_wipe_still_reports_rows_and_growth_before_drawing() {
        let mut wipe = Wipe::new(160, 96, 0);
        let mut data = data();
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
    fn score_zero_matches_reference_shape_and_rejects_invalid_digits() {
        assert_eq!(
            digit(0).unwrap(),
            [
                0x3f, 0xf0, 0xf0, 0x3c, 0xf0, 0xfc, 0xf3, 0xfc, 0xff, 0x3c, 0xfc, 0x3c, 0x3f, 0xf0,
                0, 0
            ]
        );
        assert!(digit(10).is_err());
    }

    #[test]
    fn wipe_rounds_initial_width_to_zero_then_fills_every_visible_pixel() {
        for (x, y) in [(8, 0), (144, 96), (290, 180)] {
            let mut wipe = Wipe::new(x, y, 0);
            let mut screen = Framebuffer::default();
            screen.fill_visible(0xaa);
            let mut data = data();
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

    #[test]
    fn final_reveal_waits_two_wrapping_ticks_before_digits() {
        let mut data = data();
        data[0x369e] = 176;
        let mut completion = Completion::new(80, 180, 5).unwrap();
        completion.phase = Phase::Reveal {
            y: 176,
            since: None,
        };
        let mut screen = Framebuffer::default();
        assert_eq!(
            completion
                .advance(
                    &mut data,
                    &mut screen,
                    u16::MAX,
                    false,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .bytes_drawn,
            240
        );
        completion
            .advance(
                &mut data,
                &mut screen,
                0,
                false,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Reveal { .. }));
        completion
            .advance(
                &mut data,
                &mut screen,
                1,
                false,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Digits));
    }

    #[test]
    fn hold_latches_tick_until_retrace_and_does_not_end_a_tick_early() {
        let mut data = data();
        data[0] = 1;
        let mut completion = Completion::new(80, 180, 5).unwrap();
        let mut screen = Framebuffer::default();
        completion.phase = Phase::Hold {
            start: u16::MAX - 10,
            sample: None,
        };
        completion
            .advance(
                &mut data,
                &mut screen,
                18,
                false,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        completion
            .advance(
                &mut data,
                &mut screen,
                19,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Hold { .. }));
        assert_eq!(completion.background(), 2); // Latched tick18, not new tick19.
        completion
            .advance(
                &mut data,
                &mut screen,
                19,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Fill(_)));
        assert_eq!(completion.background(), 0);
    }

    #[test]
    fn complete_presentation_awards_once_and_preserves_lives_and_random_state() {
        let mut data = data();
        set_word(&mut data, 0x2ae5, 0xfa59);
        let mut screen = Framebuffer::default();
        let mut completion = Completion::new(80, 180, 5).unwrap();
        let mut final_tick = 0;
        for tick in 0..200 {
            final_tick = tick;
            if completion
                .advance(
                    &mut data,
                    &mut screen,
                    tick,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap()
                .complete
            {
                break;
            }
        }
        assert!(final_tick < 199);
        assert_eq!(
            (word(&data, 0x414), data[0x1f80], word(&data, 0x2ae5)),
            (1, 3, 0xfa59)
        );
        let score = data[0x1f82..0x1f89].to_vec();
        completion
            .advance(
                &mut data,
                &mut screen,
                200,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert_eq!(data[0x1f82..0x1f89], score);
        assert!(screen.indices().iter().all(|pixel| *pixel == 2));
    }
}
