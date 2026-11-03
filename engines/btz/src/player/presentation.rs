// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! External DOS title data presented before starting the story interpreter.

use crate::{Error, image::dos_data, text::TextScreen};
use std::time::Duration;

pub struct TitleSequence {
    frames: [TextScreen; 3],
}

impl TitleSequence {
    pub fn from_breakers_executable(exe: &[u8]) -> Result<Self, Error> {
        let data = dos_data(exe)?;
        let mut title = TextScreen::blank();
        for (row, offset) in (7..=13).zip([4, 0x44, 0x84, 0xc4, 0x104, 0x144, 0x184]) {
            paint_string(&mut title, data, offset, row, 9)?;
        }
        let mut copyright = title.clone();
        paint_string(&mut copyright, data, 0x1c4, 17, 19)?;
        let mut credits = TextScreen::blank();
        for (row, offset) in [
            (7, 0x1ef),
            (8, 0x210),
            (9, 0x231),
            (11, 0x252),
            (12, 0x277),
            (13, 0x29c),
            (15, 0x2c1),
            (16, 0x2f6),
            (17, 0x32b),
        ] {
            paint_string(&mut credits, data, offset, row, 15)?;
        }
        Ok(Self {
            frames: [title, copyright, credits],
        })
    }

    /// Each native title stage waits for sixteen BIOS timer transitions.
    pub fn frame(&self, elapsed: Duration) -> Option<&TextScreen> {
        let stage = elapsed.as_nanos() * 1_193_182 / (16 * 65_536 * 1_000_000_000u128);
        (stage < 3).then(|| &self.frames[stage as usize])
    }

    pub fn preview(&self) -> &TextScreen {
        &self.frames[1]
    }
}

fn paint_string(
    screen: &mut TextScreen,
    data: &[u8],
    offset: usize,
    row: usize,
    column: usize,
) -> Result<(), Error> {
    let rest = data
        .get(offset..)
        .ok_or_else(|| Error("title string outside executable".into()))?;
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error("unterminated title string".into()))?;
    if column + end > 80 || rest[..end].iter().any(|&b| b < 32) {
        return Err(Error("invalid title row".into()));
    }
    for (i, &byte) in rest[..end].iter().enumerate() {
        screen.apply_input(
            &crate::input::InputAction::Put {
                row,
                column: column + i,
                byte,
            },
            &[],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_title_rows_and_bios_stage_durations_are_checked() {
        let mut exe = vec![0; 32 + 0x360];
        exe[..2].copy_from_slice(b"MZ");
        exe[8..10].copy_from_slice(&2u16.to_le_bytes());
        for offset in [
            4, 0x44, 0x84, 0xc4, 0x104, 0x144, 0x184, 0x1c4, 0x1ef, 0x210, 0x231, 0x252, 0x277,
            0x29c, 0x2c1, 0x2f6, 0x32b,
        ] {
            exe[32 + offset..32 + offset + 4].copy_from_slice(b"ROW\0");
        }
        let sequence = TitleSequence::from_breakers_executable(&exe).unwrap();
        let title = sequence.frame(Duration::ZERO).unwrap().dos_cells();
        assert_eq!(title[(7 * 80 + 9) * 2], b'R');
        assert_eq!(title[(17 * 80 + 19) * 2], b' ');
        assert_eq!(
            sequence
                .frame(Duration::from_millis(900))
                .unwrap()
                .dos_cells(),
            sequence.preview().dos_cells()
        );
        assert_eq!(sequence.preview().dos_cells()[(17 * 80 + 19) * 2], b'R');
        assert_eq!(
            sequence
                .frame(Duration::from_millis(1800))
                .unwrap()
                .dos_cells()[(7 * 80 + 15) * 2],
            b'R'
        );
        assert!(sequence.frame(Duration::from_millis(2700)).is_none());
        assert!(sequence.frame(Duration::from_secs(86_400)).is_none());
        assert!(TitleSequence::from_breakers_executable(&exe[..100]).is_err());
        exe[32 + 4] = 13;
        assert!(TitleSequence::from_breakers_executable(&exe).is_err());
    }
}
