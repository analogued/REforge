// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Shared ordinary-room neutral/failure return presentation (CS:1bf0/1d76).
//! Imagery, speaker sequences and tick gates use the original tables.

use crate::assets::{DATA_LENGTH, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer};
use crate::presentation::{Wipe, Work};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Poll {
    Message,
    Dog,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PresentationWork {
    pub message: bool,
    pub dog: bool,
    pub header: bool,
    pub detail: bool,
    pub poll: Option<Poll>,
    pub tone: bool,
    pub terminal: bool,
}

impl PresentationWork {
    pub(crate) fn bytes(self) -> usize {
        usize::from(self.message) * 192
            + usize::from(self.dog) * 544
            + usize::from(self.header) * 128
            + usize::from(self.detail) * 252
    }
}

enum Phase {
    Black(Wipe),
    Begin,
    Message { since: u16 },
    Dog { since: u16 },
    Fill(Wipe),
    Complete,
}

pub(crate) struct Failure {
    phase: Phase,
    center: (u16, u8),
    preserve_life: bool,
}

impl Failure {
    pub(crate) fn new(x: u16, y: u8, preserve_life: bool) -> Self {
        Self {
            phase: Phase::Black(Wipe::new(x, y, 0)),
            center: (x, y),
            preserve_life,
        }
    }

    pub(crate) fn poll(&self) -> Option<Poll> {
        match self.phase {
            Phase::Message { .. } => Some(Poll::Message),
            Phase::Dog { .. } => Some(Poll::Dog),
            _ => None,
        }
    }

    pub(crate) fn uses_alley_palette(&self) -> bool {
        !matches!(self.phase, Phase::Black(_))
    }

    pub(crate) fn finish_drawing(&mut self, data: &mut [u8], tick: u16) {
        // CS:1dd7 samples each message wait baseline after drawing.
        if let Phase::Message { since } = &mut self.phase {
            *since = tick;
            set_word(data, 0x1830, tick);
        }
    }

    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        screen: &mut Framebuffer,
        tick: u16,
        sound: &mut crate::sound::Sound,
    ) -> Result<Work, String> {
        if data.len() < DATA_LENGTH {
            return Err("Failure data segment is truncated".into());
        }
        let mut work = Work::default();
        match &mut self.phase {
            Phase::Black(wipe) => {
                work = wipe.advance(data, screen, sound)?;
                if work.complete {
                    self.phase = Phase::Begin;
                }
                work.complete = false;
            }
            Phase::Begin => {
                set_word(data, 0x5a85, 0);
                set_word(data, 0x5a83, tick);
                if data[0x552] != 0 {
                    work.bookkeeping.life = Some(rotate_message_and_life(data, self.preserve_life));
                }
                if data[0x552] == 0xdd && word(data, 8) != 0 && data[0x1f80] != 0 {
                    set_word(data, 0x5f60, 0);
                    set_word(data, 0x5f66, tick);
                    work.bytes_drawn = draw_dog(data, screen)?;
                    work.failure.dog = true;
                    self.phase = Phase::Dog { since: tick };
                } else {
                    // CS:1dc6 stores the selected pointer only on the message
                    // branch. The alternate dog leaves it untouched.
                    let source = if data[0x552] != 0 {
                        let index = word(data, 0x1c30).wrapping_sub(2) & 6;
                        word(data, 0x1c26 + usize::from(index))
                    } else {
                        // Neutral return: no rotation and no life loss.
                        0x185b
                    };
                    set_word(data, 0x1c2e, source);
                    set_word(data, 0x1c1b, 0x8080);
                    data[0x1c1d] = 28;
                    set_word(data, 0x1830, tick);
                    work.bytes_drawn = draw_message(data, screen)?;
                    work.failure.message = true;
                    self.phase = Phase::Message { since: tick };
                }
            }
            Phase::Message { since } => {
                work.failure.poll = Some(Poll::Message);
                let tone_cursor = word(data, 0x5a85);
                sound.failure_tone(data, tick);
                work.failure.tone = word(data, 0x5a85) != tone_cursor;
                if tick != *since {
                    let count = data[0x1c1d];
                    let mask = if count <= 20 {
                        word(data, 0x1c1e + usize::from(count & 6))
                    } else {
                        let byte = ((word(data, 0x1c1b) as u8) >> 1) | 0x80;
                        u16::from(byte) * 0x101
                    };
                    set_word(data, 0x1c1b, mask);
                    data[0x1c1d] -= 1;
                    if data[0x1c1d] == 0 {
                        sound.stop();
                        work.failure.terminal = true;
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        *since = tick;
                        set_word(data, 0x1830, tick);
                        work.bytes_drawn = draw_message(data, screen)?;
                        work.failure.message = true;
                    }
                }
            }
            Phase::Dog { since } => {
                work.failure.poll = Some(Poll::Dog);
                let tone_cursor = word(data, 0x5a85);
                sound.failure_tone(data, tick);
                work.failure.tone = word(data, 0x5a85) != tone_cursor;
                if tick.wrapping_sub(*since) >= 4 {
                    *since = tick;
                    set_word(data, 0x5f66, tick);
                    let count = word(data, 0x5f60);
                    if count == 4 {
                        copy(data, screen, 0x5f68, 0x0668, 0x1004)?;
                        work.failure.header = true;
                    }
                    if (8..14).contains(&count) {
                        copy(
                            data,
                            screen,
                            0x5fe8,
                            word(data, 0x60e4 + usize::from(count - 8)),
                            0x1506,
                        )?;
                        work.failure.detail = true;
                    }
                    if count >= 16 {
                        sound.stop();
                        work.failure.terminal = true;
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        work.bytes_drawn = draw_dog(data, screen)?;
                        work.failure.dog = true;
                    }
                }
            }
            Phase::Fill(wipe) => {
                work = wipe.advance(data, screen, sound)?;
                if work.complete {
                    self.phase = Phase::Complete;
                }
            }
            Phase::Complete => work.complete = true,
        }
        work.bytes_drawn = work.bytes_drawn.max(work.failure.bytes());
        Ok(work)
    }
}

/// CS:1d8e..1da9. Keep the state prefix separate from drawing and sound setup.
fn rotate_message_and_life(data: &mut [u8], preserve_life: bool) -> bool {
    set_word(data, 0x1c30, word(data, 0x1c30).wrapping_add(2));
    let had_life = data[0x1f80] != 0;
    if !preserve_life && had_life {
        data[0x1f80] -= 1;
    }
    had_life
}

fn copy(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    destination: u16,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    screen.copy_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

fn draw_message(data: &mut [u8], screen: &mut Framebuffer) -> Result<usize, String> {
    let source = sprite_bytes(data, word(data, 0x1c2e), Dimensions(0x0c08))?.to_vec();
    let mask = word(data, 0x1c1b).to_le_bytes();
    for (index, byte) in source.iter().enumerate() {
        data[0x0e + index] = *byte & mask[index & 1];
    }
    copy(data, screen, 0x0e, 0x0ed0, 0x0c08)?;
    Ok(192)
}

fn draw_dog(data: &mut [u8], screen: &mut Framebuffer) -> Result<usize, String> {
    let count = word(data, 0x5f60);
    set_word(data, 0x5f60, count.wrapping_add(2));
    copy(
        data,
        screen,
        word(data, 0x5f62 + usize::from(count & 2)),
        0x0a74,
        0x4404,
    )?;
    Ok(544)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternate_dog_advances_the_cursor_without_storing_a_message_pointer() {
        let mut data = vec![0; DATA_LENGTH];
        data[0x1f80] = 3;
        data[0x552] = 0xdd;
        set_word(&mut data, 8, 1);
        set_word(&mut data, 0x1c2e, 0xa55a);
        set_word(&mut data, 0x5f62, 0x3000);
        let mut presentation = Failure::new(160, 96, false);
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        for tick in 0..16 {
            presentation
                .advance(&mut data, &mut screen, tick, &mut sound)
                .unwrap();
        }
        assert_eq!(data[0x1f80], 2);
        assert_eq!(word(&data, 0x1c30), 2);
        assert_eq!(word(&data, 0x1c2e), 0xa55a);
        assert_eq!(word(&data, 0x5f60), 2);
    }

    #[test]
    fn neutral_return_uses_the_shared_reveal_without_rotating_or_spending_lives() {
        let mut data = vec![0; DATA_LENGTH];
        data[0x1f80] = 3;
        set_word(&mut data, 0x1c30, 6);
        data[0x185b..0x191b].fill(0xff);
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        let mut presentation = Failure::new(160, 96, false);
        let mut draws = 0;
        let mut complete = false;
        for tick in 0..128 {
            let work = presentation
                .advance(&mut data, &mut screen, tick, &mut sound)
                .unwrap();
            assert!(work.bookkeeping.life.is_none());
            draws += u16::from(work.failure.message);
            if work.complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
        assert_eq!(draws, 28);
        assert_eq!(data[0x1f80], 3);
        assert_eq!(word(&data, 0x1c30), 6);
        assert_eq!(word(&data, 0x1c2e), 0x185b);
        assert_eq!(data[0x1c1d], 0);
    }

    #[test]
    fn dog_frames_report_their_header_and_detail_copies() {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x5f62, 0x3000);
        set_word(&mut data, 0x5f64, 0x3220);
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        let mut failure = Failure {
            phase: Phase::Dog {
                since: u16::MAX - 3,
            },
            center: (100, 100),
            preserve_life: false,
        };
        set_word(&mut data, 0x5f60, 4);
        let header = failure
            .advance(&mut data, &mut screen, 0, &mut sound)
            .unwrap();
        assert!(header.failure.header && header.failure.dog && !header.failure.detail);
        assert_eq!(header.bytes_drawn, 128 + 544);
        set_word(&mut data, 0x5f60, 8);
        set_word(&mut data, 0x60e4, 0x1000);
        let detail = failure
            .advance(&mut data, &mut screen, 4, &mut sound)
            .unwrap();
        assert!(detail.failure.detail && detail.failure.dog && !detail.failure.header);
        assert_eq!(detail.bytes_drawn, 252 + 544);
    }

    #[test]
    fn final_message_stops_sound_without_replacing_its_last_draw_baseline() {
        let mut data = vec![0; DATA_LENGTH];
        data[0x1c1d] = 1;
        set_word(&mut data, 0x1830, u16::MAX);
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        sound.gate(3);
        sound.record(true);
        let mut failure = Failure {
            phase: Phase::Message { since: u16::MAX },
            center: (100, 100),
            preserve_life: false,
        };
        let work = failure
            .advance(&mut data, &mut screen, 0, &mut sound)
            .unwrap();
        assert!(work.failure.terminal && !work.failure.message);
        assert_eq!(data[0x1c1d], 0);
        assert_eq!(word(&data, 0x1830), u16::MAX);
        let writes: Vec<_> = sound.drain().collect();
        assert_eq!(writes.len(), 1);
        assert_eq!((writes[0].port, writes[0].value & 3), (0x61, 0));
    }

    #[test]
    fn message_wait_starts_after_its_image_has_finished_drawing() {
        let mut data = vec![0; DATA_LENGTH];
        data[0x1c1d] = 3;
        set_word(&mut data, 0x1c2e, 0x3000);
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        let mut failure = Failure {
            phase: Phase::Message { since: u16::MAX },
            center: (100, 100),
            preserve_life: false,
        };
        assert!(
            failure
                .advance(&mut data, &mut screen, 0, &mut sound)
                .unwrap()
                .failure
                .message
        );
        failure.finish_drawing(&mut data, 1);
        assert_eq!(word(&data, 0x1830), 1);
        assert!(
            !failure
                .advance(&mut data, &mut screen, 1, &mut sound)
                .unwrap()
                .failure
                .message
        );
        assert_eq!(data[0x1c1d], 2);
        assert!(
            failure
                .advance(&mut data, &mut screen, 2, &mut sound)
                .unwrap()
                .failure
                .message
        );
    }

    #[test]
    fn room_failure_loses_one_life_and_infinite_lives_only_suppresses_the_decrement() {
        for preserve in [false, true] {
            let mut data = vec![0; DATA_LENGTH];
            data[0x1f80] = 3;
            data[0x552] = 1;
            for index in (0..8).step_by(2) {
                set_word(&mut data, 0x1c26 + index, 0x2000);
            }
            data[0x2000..0x20c0].fill(0xff);
            let mut screen = Framebuffer::default();
            let mut failure = Failure::new(100, 100, preserve);
            let mut complete = false;
            for tick in 0..100 {
                complete = failure
                    .advance(
                        &mut data,
                        &mut screen,
                        tick,
                        &mut crate::sound::Sound::default(),
                    )
                    .unwrap()
                    .complete;
                if complete {
                    break;
                }
            }
            assert!(complete);
            assert_eq!(data[0x1f80], if preserve { 3 } else { 2 });
            assert_eq!(word(&data, 0x1c30), 2);
        }
    }
}
