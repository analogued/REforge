// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! ANSI presentation of the shared DOS character screens and host cursor choice.

use reforge_settings::FileCursorStyle;
use std::io::{self, Write};

#[derive(Default)]
pub(crate) struct Screen {
    cells: Vec<u8>,
    cursor: Option<(usize, usize, u8)>,
}

impl Screen {
    pub(crate) fn invalidate(&mut self) {
        self.cells.clear();
    }

    pub(crate) fn draw(
        &mut self,
        output: &mut impl Write,
        cells: &[u8],
        position: Option<(usize, usize)>,
        style: FileCursorStyle,
    ) -> io::Result<()> {
        if cells.len() != 4000 || position.is_some_and(|(r, c)| r >= 25 || c >= 80) {
            return Err(io::Error::other(
                "invalid terminal character screen or cursor",
            ));
        }
        let redraw = self.cells != cells;
        if redraw {
            write!(output, "\x1b[?25l")?;
            let mut attribute = None;
            for (row, line) in cells.as_chunks::<160>().0.iter().enumerate() {
                write!(output, "\x1b[{};1H", row + 1)?;
                for cell in line.as_chunks::<2>().0 {
                    if attribute != Some(cell[1]) {
                        // DOS and ANSI order their blue/red palette bits differently.
                        let ansi = [0, 4, 2, 6, 1, 5, 3, 7];
                        let fg =
                            if cell[1] & 8 != 0 { 90 } else { 30 } + ansi[usize::from(cell[1] & 7)];
                        let bg = 40 + ansi[usize::from((cell[1] >> 4) & 7)];
                        if cell[1] & 0x80 != 0 {
                            write!(output, "\x1b[0;5;{fg};{bg}m")?;
                        } else {
                            write!(output, "\x1b[0;{fg};{bg}m")?;
                        }
                        attribute = Some(cell[1]);
                    }
                    write!(output, "{}", crate::text::cp437(cell[0]))?;
                }
            }
            write!(output, "\x1b[0m")?;
            self.cells.clear();
            self.cells.extend_from_slice(cells);
        }
        // DECSCUSR supports a real underline and block while preserving glyphs.
        // Pulsing underline falls back to a steady underline in terminal play.
        let shape = match style {
            FileCursorStyle::SteadyBlock => 2,
            FileCursorStyle::BlinkingUnderline => 3,
            FileCursorStyle::PulsingUnderline => 4,
        };
        let cursor = position.map(|(row, column)| (row, column, shape));
        if redraw || self.cursor != cursor {
            if let Some((row, column, shape)) = cursor {
                write!(
                    output,
                    "\x1b[{shape} q\x1b[{};{}H\x1b[?25h",
                    row + 1,
                    column + 1
                )?;
            } else {
                write!(output, "\x1b[?25l")?;
            }
            self.cursor = cursor;
        }
        output.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_pages_keep_inverse_selection_and_native_cursor_shapes() {
        let mut cells = [b' ', 7].repeat(2000);
        cells[(18 * 80 + 3) * 2..(18 * 80 + 3) * 2 + 2].copy_from_slice(&[b'A', 0x70]);
        for (style, shape) in [
            (FileCursorStyle::PulsingUnderline, 4),
            (FileCursorStyle::BlinkingUnderline, 3),
            (FileCursorStyle::SteadyBlock, 2),
        ] {
            let mut screen = Screen::default();
            let mut bytes = Vec::new();
            screen
                .draw(&mut bytes, &cells, Some((18, 3)), style)
                .unwrap();
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains("\x1b[0;30;47mA"));
            assert!(text.contains(&format!("\x1b[{shape} q\x1b[19;4H\x1b[?25h")));
            assert!(!text.contains('\n'));
            let mut unchanged = Vec::new();
            screen
                .draw(&mut unchanged, &cells, Some((18, 3)), style)
                .unwrap();
            assert!(unchanged.is_empty());
            screen.draw(&mut unchanged, &cells, None, style).unwrap();
            assert_eq!(unchanged, b"\x1b[?25l");
            screen.invalidate();
            unchanged.clear();
            screen.draw(&mut unchanged, &cells, None, style).unwrap();
            assert!(unchanged.len() > 2000);
        }
    }
}
