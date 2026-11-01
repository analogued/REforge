// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! CS:1bf0 entries with no intervening return presentation.

use crate::framebuffer::Framebuffer;
use crate::presentation::{Wipe, Work};
use crate::sound::Sound;

enum Phase {
    Black(Wipe),
    Fill(Wipe),
}

pub(crate) struct Entry {
    destination: u16,
    center: (u16, u8),
    phase: Phase,
}

impl Entry {
    pub(crate) fn new(destination: u16, x: u16, y: u8) -> Result<Self, String> {
        if !(1..=7).contains(&destination) || x > 304 || y > 198 {
            return Err("Room transition destination or origin is out of range".into());
        }
        Ok(Self {
            destination,
            center: (x, y),
            phase: Phase::Black(Wipe::new(x, y, 0)),
        })
    }

    pub(crate) fn destination(&self) -> u16 {
        self.destination
    }

    /// CS:1d31 changes palette after the black wipe, before the colour wipe.
    pub(crate) fn palette_destination(&self) -> Option<u16> {
        matches!(self.phase, Phase::Fill(_)).then_some(self.destination)
    }

    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        screen: &mut Framebuffer,
        sound: &mut Sound,
    ) -> Result<Work, String> {
        match &mut self.phase {
            Phase::Black(wipe) => {
                let mut work = wipe.advance(data, screen, sound)?;
                if work.complete {
                    let packed = if matches!(self.destination, 2 | 7) {
                        0x55
                    } else {
                        0xaa
                    };
                    self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, packed));
                }
                work.complete = false;
                Ok(work)
            }
            Phase::Fill(wipe) => wipe.advance(data, screen, sound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{DATA_LENGTH, word};

    #[test]
    fn entries_change_palette_between_complete_wipes_and_keep_bank_padding() {
        for destination in 1..=7 {
            let mut data = vec![0; DATA_LENGTH];
            data[0] = 0xff;
            let mut sound = Sound::default();
            sound.record(true);
            let mut screen = Framebuffer::from_bytes(&[0x37; 16384]).unwrap();
            let mut entry = Entry::new(destination, 160, 96).unwrap();
            let mut steps = 0;
            let mut black_steps = 0;
            loop {
                let black = entry.palette_destination().is_none();
                let work = entry.advance(&mut data, &mut screen, &mut sound).unwrap();
                steps += 1;
                if black {
                    black_steps += 1;
                    assert!(!work.complete);
                }
                if work.complete {
                    break;
                }
                assert!(steps < 64);
            }
            assert_eq!(steps, 2 * black_steps);
            assert_eq!(black_steps, 15);
            assert_eq!(word(&data, 0x5a56), 2 * steps);
            assert_eq!(
                sound.drain().filter(|w| w.port == 0x43).count(),
                usize::from(steps)
            );
            let packed = if matches!(destination, 2 | 7) {
                0x55
            } else {
                0xaa
            };
            for bank in [0, 8192] {
                assert!(
                    screen.bytes()[bank..bank + 8000]
                        .iter()
                        .all(|&b| b == packed)
                );
                assert!(
                    screen.bytes()[bank + 8000..bank + 8192]
                        .iter()
                        .all(|&b| b == 0x37)
                );
            }
        }
    }
}
