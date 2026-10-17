// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Shared ordinary-room failure presentation (CS:1bf0/1d76).
//! Imagery, speaker sequences and tick gates use the original tables.

use crate::assets::{DATA_LENGTH, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer};
use crate::completion::{Wipe, Work};

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
                let index = word(data, 0x1c30) & 6;
                set_word(data, 0x1c30, word(data, 0x1c30).wrapping_add(2));
                set_word(data, 0x1c2e, word(data, 0x1c26 + usize::from(index)));
                if !self.preserve_life && data[0x1f80] != 0 {
                    data[0x1f80] -= 1;
                }
                if data[0x552] == 0xdd && word(data, 8) != 0 && data[0x1f80] != 0 {
                    set_word(data, 0x5f60, 0);
                    set_word(data, 0x5f66, tick);
                    work.bytes_drawn = draw_dog(data, screen)?;
                    self.phase = Phase::Dog { since: tick };
                } else {
                    set_word(data, 0x1c1b, 0x8080);
                    data[0x1c1d] = 28;
                    set_word(data, 0x1830, tick);
                    work.bytes_drawn = draw_message(data, screen)?;
                    self.phase = Phase::Message { since: tick };
                }
            }
            Phase::Message { since } => {
                sound.failure_tone(data, tick);
                if tick != *since {
                    *since = tick;
                    set_word(data, 0x1830, tick);
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
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        work.bytes_drawn = draw_message(data, screen)?;
                    }
                }
            }
            Phase::Dog { since } => {
                sound.failure_tone(data, tick);
                if tick.wrapping_sub(*since) >= 4 {
                    *since = tick;
                    set_word(data, 0x5f66, tick);
                    let count = word(data, 0x5f60);
                    if count == 4 {
                        copy(data, screen, 0x5f68, 0x0668, 0x1004)?;
                    }
                    if (8..14).contains(&count) {
                        copy(
                            data,
                            screen,
                            0x5fe8,
                            word(data, 0x60e4 + usize::from(count - 8)),
                            0x1506,
                        )?;
                    }
                    if count >= 16 {
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        work.bytes_drawn = draw_dog(data, screen)?;
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
        Ok(work)
    }
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
