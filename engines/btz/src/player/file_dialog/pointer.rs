// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Pointer feedback stays separate from keyboard focus and filename selection.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Entry(usize),
    Name,
    Accept,
    Cancel,
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Pointer {
    pub(super) hover: Option<Target>,
    pub(super) pressed: Option<Target>,
}

impl FileDialog {
    pub(super) fn hovered(&self, target: Target) -> bool {
        self.pointer.hover == Some(target)
    }

    fn pressed(&self, target: Target) -> bool {
        self.hovered(target) && self.pointer.pressed == Some(target)
    }

    pub(super) fn marked(&self, index: usize) -> bool {
        (self.focus == Focus::List && self.selected == index)
            || self.selected_path.as_ref() == Some(&self.entries[index].path)
    }

    pub(super) fn buttons(&self) -> [(Target, usize, &'static str, bool); 2] {
        if self.confirm.is_some() {
            [
                (Target::Yes, LEFT, "[ Yes, replace ]", self.yes),
                (
                    Target::No,
                    LEFT + 19,
                    if self.fixed {
                        "[ No, cancel ]"
                    } else {
                        "[ No, choose another ]"
                    },
                    !self.yes,
                ),
            ]
        } else {
            [
                (
                    Target::Accept,
                    LEFT,
                    self.operation.accept(),
                    self.focus == Focus::Accept,
                ),
                (
                    Target::Cancel,
                    LEFT + self.operation.accept().len() + 3,
                    "[ Cancel ]",
                    self.focus == Focus::Cancel,
                ),
            ]
        }
    }

    pub(super) fn button(
        &self,
        screen: &mut Screen,
        column: usize,
        label: &str,
        target: Target,
        focus: bool,
    ) {
        let inverse = focus || self.pressed(target);
        screen.text(column, 20, label, inverse, 0);
        if self.hovered(target) && !inverse {
            screen.attribute(column, 20, label.len(), 15);
        }
    }

    pub(crate) fn clear_pointer(&mut self) {
        self.pointer = Pointer::default();
    }
}

#[cfg(any(feature = "sdl", test))]
impl FileDialog {
    fn target(&self, column: usize, row: usize) -> Option<Target> {
        if row == 20 {
            return self
                .buttons()
                .into_iter()
                .find_map(|(target, start, label, _)| {
                    (start..start + label.len())
                        .contains(&column)
                        .then_some(target)
                });
        }
        if self.confirm.is_some() {
            return None;
        }
        if !(LEFT..LEFT + WIDTH).contains(&column) {
            return None;
        }
        match row {
            9..=14 => {
                let index = self.top + row - 9;
                (index < self.entries.len()).then_some(Target::Entry(index))
            }
            18 => Some(Target::Name),
            _ => None,
        }
    }

    pub(crate) fn pointer_move(&mut self, position: Option<(usize, usize)>) {
        self.pointer.hover = position.and_then(|(column, row)| self.target(column, row));
    }

    pub(crate) fn pointer_down(&mut self, position: Option<(usize, usize)>) {
        self.pointer_move(position);
        self.pointer.pressed = self.pointer.hover;
    }

    pub(crate) fn pointer_up(
        &mut self,
        position: Option<(usize, usize)>,
        double: bool,
    ) -> Option<Action> {
        self.pointer_move(position);
        let pressed = self.pointer.pressed.take();
        // The second click on Save must not fall through onto Yes when the
        // first click opened an overwrite question at the same coordinates.
        if double
            && matches!(
                pressed,
                Some(Target::Accept | Target::Cancel | Target::Yes | Target::No)
            )
        {
            return None;
        }
        if pressed.is_some() && pressed == self.pointer.hover {
            let (column, row) = position?;
            self.click(column, row, double)
        } else {
            None
        }
    }

    pub(crate) fn text_pointer(&self) -> bool {
        self.hovered(Target::Name) && self.confirm.is_none()
    }

    pub(crate) fn pointer_state(&self) -> Pointer {
        self.pointer
    }

    pub(crate) fn click(&mut self, column: usize, row: usize, double: bool) -> Option<Action> {
        match self.target(column, row)? {
            Target::Yes => {
                self.yes = true;
                self.input(Input::Enter)
            }
            Target::No => self.decline(),
            Target::Entry(index) => {
                self.focus = Focus::List;
                self.select(index);
                if double { self.submit() } else { None }
            }
            Target::Name => {
                self.focus = Focus::Name;
                self.anchor = None;
                self.caret = (self.visible_start() + column - LEFT).min(self.name.len());
                None
            }
            Target::Accept => {
                self.focus = Focus::Accept;
                self.submit()
            }
            Target::Cancel => Some(Action::Cancel),
        }
    }
}

#[cfg(feature = "sdl")]
impl FileDialog {
    /// Paint additional monochrome feedback into the host copy, before the
    /// insertion cursor. No story cells or native bookmark bytes are changed.
    pub(crate) fn paint_feedback(&self, pixels: &mut [u8]) {
        const STRIDE: usize = 640 * 3;
        for (offset, _) in self.entries.iter().skip(self.top).take(ROWS).enumerate() {
            let index = self.top + offset;
            let active =
                self.focus == Focus::List && self.selected == index && self.confirm.is_none();
            let marked = self.marked(index);
            if !active && (marked || self.hovered(Target::Entry(index))) {
                let shade = if marked { 51 } else { 34 };
                for y in (9 + offset) * 8..(10 + offset) * 8 {
                    for x in LEFT * 8..(LEFT + WIDTH) * 8 {
                        let pixel = &mut pixels[y * STRIDE + x * 3..y * STRIDE + x * 3 + 3];
                        if pixel == [0, 0, 0] {
                            pixel.fill(shade);
                        }
                    }
                }
            }
        }
        if self.confirm.is_none() {
            let shade = if self.hovered(Target::Name) { 255 } else { 85 };
            let selection = self.selection();
            for x in LEFT * 8..(LEFT + WIDTH) * 8 {
                let column = self.visible_start() + x / 8 - LEFT;
                if self.focus == Focus::Name
                    && selection
                        .as_ref()
                        .is_some_and(|range| range.contains(&column))
                {
                    continue;
                }
                let pixel = &mut pixels[151 * STRIDE + x * 3..151 * STRIDE + x * 3 + 3];
                if pixel == [0, 0, 0] {
                    pixel.fill(shade);
                }
            }
        }
        for (target, column, label, focus) in self.buttons() {
            let width = label.len();
            if !self.hovered(target) {
                continue;
            }
            // Reverse-video buttons brighten their background; ordinary hovered
            // buttons already use foreground 15 in the character buffer.
            let inverse = self.pressed(target) || focus;
            if inverse {
                for y in 160..168 {
                    for x in column * 8..(column + width) * 8 {
                        let pixel = &mut pixels[y * STRIDE + x * 3..y * STRIDE + x * 3 + 3];
                        if pixel == [170, 170, 170] {
                            pixel.fill(255);
                        }
                    }
                }
            }
            if self.pressed(target) {
                for y in (161..168).rev() {
                    let source = (y - 1) * STRIDE + column * 8 * 3;
                    pixels.copy_within(source..source + width * 8 * 3, y * STRIDE + column * 8 * 3);
                }
                pixels[160 * STRIDE + column * 8 * 3..160 * STRIDE + (column + width) * 8 * 3]
                    .fill(255);
            }
        }
    }
}
