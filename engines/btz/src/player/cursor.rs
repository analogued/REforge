// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Presentation-only cursor shared by command input and windowed file dialogs.

use crate::{input::InputEditor, text::TextScreen};
use reforge_settings::FileCursorStyle;
use std::{borrow::Cow, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cursor {
    pub row: usize,
    pub column: usize,
    pub height: usize,
}

impl Cursor {
    pub(crate) fn at(
        row: usize,
        column: usize,
        style: FileCursorStyle,
        elapsed: Duration,
    ) -> Option<Self> {
        // The CGA reference toggles the BIOS cursor every eight video frames.
        // Breakers also paints an underscore, retaining one row in the off phase.
        let raised = (elapsed.as_nanos() * 60 / (8 * 1_000_000_000)) & 1 != 0;
        let height = match style {
            FileCursorStyle::SteadyBlock => 8,
            FileCursorStyle::PulsingUnderline => 1 + usize::from(raised),
            FileCursorStyle::BlinkingUnderline if raised => 2,
            FileCursorStyle::BlinkingUnderline => return None,
        };
        Some(Self {
            row,
            column,
            height,
        })
    }
}

pub(crate) fn game_cells<'a>(
    screen: &'a TextScreen,
    input: &InputEditor,
    style: FileCursorStyle,
    elapsed: Duration,
) -> (Cow<'a, [u8]>, Option<Cursor>) {
    let cells = screen.dos_cells();
    let (row, column) = input.cursor();
    let offset = (row * 80 + column) * 2;
    if cells.get(offset) != Some(&0xdb) {
        return (Cow::Borrowed(cells), None);
    }
    // The native editor paints a destructive block at its next insertion point.
    // Replace only that marker in the presentation copy, even in a hidden phase.
    let mut presented = cells.to_vec();
    presented[offset] = b' ';
    (
        Cow::Owned(presented),
        Cursor::at(row, column, style, elapsed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        input::{InputAction, Key},
        interpreter::*,
    };

    #[test]
    fn game_cursor_follows_typing_scrolling_and_reset_without_changing_native_cells() {
        for definition in [&MINDWHEEL_DOS, &ESSEX_DOS, &BRIMSTONE_DOS, &BREAKERS_DOS] {
            let mut input = InputEditor::with_layout(definition.layout);
            let mut screen = TextScreen::with_layout(definition.layout);
            for key in [Key::Character(b'x'), Key::Backspace, Key::Enter]
                .into_iter()
                .cycle()
                .take(18)
            {
                for action in input.process_key(key) {
                    screen.apply_input(&action, &[]).unwrap();
                }
                if input.line_ready() {
                    input.acknowledge_line().unwrap();
                }
                let native = screen.dos_cells().to_vec();
                let (row, column) = input.cursor();
                let offset = (row * 80 + column) * 2;
                assert_eq!(native[offset], 0xdb);
                for style in [
                    FileCursorStyle::PulsingUnderline,
                    FileCursorStyle::BlinkingUnderline,
                    FileCursorStyle::SteadyBlock,
                ] {
                    for ms in [0, 134] {
                        let (presented, cursor) =
                            game_cells(&screen, &input, style, Duration::from_millis(ms));
                        assert_eq!(presented[offset], b' ');
                        assert_eq!(presented[..offset], native[..offset]);
                        assert_eq!(presented[offset + 1..], native[offset + 1..]);
                        if let Some(cursor) = cursor {
                            assert_eq!((cursor.row, cursor.column), (row, column));
                        }
                        assert_eq!(screen.dos_cells(), native);
                    }
                }
            }
            input.rebuild_frame();
            screen = TextScreen::with_layout(definition.layout);
            let (_, cursor) = game_cells(
                &screen,
                &input,
                FileCursorStyle::SteadyBlock,
                Duration::ZERO,
            );
            let cursor = cursor.unwrap();
            assert_eq!(
                (cursor.row, cursor.column),
                (definition.layout.input_top, 1)
            );
        }
    }

    #[test]
    fn other_block_glyphs_and_overwritten_cursor_cells_are_preserved() {
        let input = InputEditor::new();
        let mut screen = TextScreen::default();
        screen
            .apply_input(
                &InputAction::Put {
                    row: 2,
                    column: 3,
                    byte: 0xdb,
                },
                &[],
            )
            .unwrap();
        let (cells, cursor) = game_cells(
            &screen,
            &input,
            FileCursorStyle::BlinkingUnderline,
            Duration::ZERO,
        );
        assert_eq!(cells[(2 * 80 + 3) * 2], 0xdb);
        assert_eq!(cells[(19 * 80 + 1) * 2], b' ');
        assert!(cursor.is_none());
        screen
            .apply_input(
                &InputAction::Put {
                    row: 19,
                    column: 1,
                    byte: b'X',
                },
                &[],
            )
            .unwrap();
        let (cells, cursor) = game_cells(
            &screen,
            &input,
            FileCursorStyle::SteadyBlock,
            Duration::ZERO,
        );
        assert_eq!(cells, screen.dos_cells());
        assert!(cursor.is_none());
    }
}
