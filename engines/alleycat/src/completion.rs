// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary-room success presentation, with external cat faces and BIOS-style text.
//! Tick waits and speaker writes are independent of host rendering.

use crate::assets::{DATA_LENGTH, set_word, word};
use crate::framebuffer::{Dimensions, Framebuffer, pixel_offset};

use crate::presentation::{AwardWork, Poll, PollWork, Wipe, Work};

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

    pub(crate) fn uses_alley_palette(&self) -> bool {
        !matches!(self.phase, Phase::Black(_))
    }

    pub(crate) fn poll(&self) -> Option<Poll> {
        match self.phase {
            Phase::Reveal { since: Some(_), .. } => Some(Poll::Reveal),
            _ => None,
        }
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
                work.bookkeeping.progress = true;
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
                work.bookkeeping.award = Some(AwardWork {
                    bonus: word(data, 0x3697),
                    multiplier: None,
                });
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
                    work.polling = Some(PollWork {
                        kind: Poll::Reveal,
                        tone: false,
                    });
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

    #[test]
    fn reveal_poll_selection_tracks_only_the_two_tick_wait_at_wrap() {
        let mut c = Completion::new(100, 100, 1).unwrap();
        c.phase = Phase::Reveal {
            y: -16,
            since: Some(u16::MAX),
        };
        let mut data = vec![0; DATA_LENGTH];
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        assert_eq!(c.poll(), Some(Poll::Reveal));
        let waiting = c
            .advance(&mut data, &mut screen, 0, true, &mut sound)
            .unwrap();
        assert_eq!(waiting.polling.unwrap().kind, Poll::Reveal);
        assert_eq!(waiting.bytes_drawn, 0);
        c.advance(&mut data, &mut screen, 1, true, &mut sound)
            .unwrap();
        assert_eq!(c.poll(), None);
        assert!(matches!(c.phase, Phase::Digits));
    }

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
        let mut progress_work = 0;
        let mut awards = 0;
        for tick in 0..200 {
            final_tick = tick;
            let work = completion
                .advance(
                    &mut data,
                    &mut screen,
                    tick,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap();
            progress_work += usize::from(work.bookkeeping.progress);
            awards += usize::from(work.bookkeeping.award.is_some());
            if work.complete {
                break;
            }
        }
        assert!(final_tick < 199);
        assert_eq!((progress_work, awards), (1, 1));
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
