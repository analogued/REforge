// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original paused battlefield controls, order display and cohort commands.
use super::{Preparation, put};
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::session::Input;
use crate::state::Campaign;

fn project(x: i32, y: i32) -> (i32, i32) {
    (164 + (x - y) * 8, (x + y) * 4 - 74)
}

fn brush(frame: &mut Frame, x: i32, y: i32) {
    frame.rect(x - 1, y - 1, 3, 3, 1);
}

fn line(frame: &mut Frame, from: (i32, i32), to: (i32, i32)) {
    // 97B6 includes both endpoints and advances diagonally when error >= 0.
    let (mut x, mut y) = from;
    let (dx, dy) = ((to.0 - x).abs(), (to.1 - y).abs());
    let (sx, sy) = ((to.0 - x).signum(), (to.1 - y).signum());
    let major = dx.max(dy);
    let minor = dx.min(dy);
    let mut error = minor * 2 - major;
    for _ in 0..major {
        brush(frame, x, y);
        if dx > dy {
            x += sx;
        } else {
            y += sy;
        }
        if error >= 0 {
            if dx > dy {
                y += sy;
            } else {
                x += sx;
            }
            error += (minor - major) * 2;
        } else {
            error += minor * 2;
        }
    }
    brush(frame, x, y);
}

impl Preparation {
    pub fn control(&mut self, input: &Input, state: &Campaign, cursor: (i32, i32)) -> Result<bool> {
        if self.motion.finished {
            return Ok(!matches!(
                input,
                Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
            ));
        }
        if self.motion.message.is_some() {
            if matches!(
                input,
                Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
            ) {
                return Ok(false);
            }
            self.motion.message = None;
            if let Some((melee, selected)) = self.motion.pending.take() {
                self.command(melee, selected);
                self.motion.selected = None;
                self.motion.paused = selected.is_some();
            }
            return Ok(true);
        }
        if !self.motion.paused {
            if matches!(input, Input::Escape) {
                return Ok(true);
            }
            if matches!(
                input,
                Input::Space
                    | Input::Enter
                    | Input::Click(_, _)
                    | Input::RightClick(_, _)
                    | Input::Shortcut(b'+' | 16)
            ) {
                self.motion.paused = true;
                return Ok(true);
            }
            return Ok(false);
        }
        if self.motion.show_orders {
            if !matches!(
                input,
                Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
            ) {
                self.motion.show_orders = false;
                return Ok(true);
            }
            return Ok(false);
        }
        if self
            .motion
            .selected
            .is_some_and(|i| self.units[i][11] != 0 || !self.can_command(i))
            && !matches!(
                input,
                Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
            )
        {
            self.motion.selected = None;
            return Ok(true);
        }
        let key = match input {
            Input::Shortcut(key) => key.to_ascii_lowercase(),
            Input::Click(x, y) => {
                let mut key = 0;
                for (row, shortcut) in b"sdmr".iter().enumerate() {
                    let at = 0x748e + row * 8;
                    if *x >= i32::from(word(&self.ds, at)?)
                        && *x <= i32::from(word(&self.ds, at + 4)?)
                        && *y >= i32::from(word(&self.ds, at + 2)?)
                        && *y <= i32::from(word(&self.ds, at + 6)?)
                    {
                        key = *shortcut;
                    }
                }
                if key == 0 {
                    let previous = self.motion.selected;
                    self.finish_selected();
                    self.select_at(*x, *y);
                    if previous == self.motion.selected
                        && previous.is_some_and(|i| self.can_command(i))
                    {
                        self.motion.dragging = true;
                    }
                }
                key
            }
            Input::Escape => {
                if self.motion.selected.is_some() {
                    self.finish_selected();
                } else {
                    self.motion.paused = false;
                }
                return Ok(true);
            }
            Input::Enter if self.motion.selected.is_some() => {
                self.finish_selected();
                return Ok(true);
            }
            Input::Enter | Input::Space if self.motion.selected.is_none() => {
                return self.control(&Input::Click(cursor.0, cursor.1), state, cursor);
            }
            Input::Space if self.motion.selected.is_some() => {
                let index = self.motion.selected.unwrap();
                if self.can_command(index) {
                    self.units[index][6..9].fill(128);
                }
                return Ok(true);
            }
            Input::Up | Input::Down | Input::Left | Input::Right
                if self.motion.selected.is_some() =>
            {
                let index = self.motion.selected.unwrap();
                if self.can_command(index) {
                    // Keyboard arrows extend the current order by a grid square.
                    let (mut x, mut y) = self.order_endpoint(index);
                    match input {
                        Input::Up => y -= 1,
                        Input::Down => y += 1,
                        Input::Left => x -= 1,
                        Input::Right => x += 1,
                        _ => unreachable!(),
                    }
                    self.move_order(index, x, y);
                }
                return Ok(true);
            }
            Input::RightClick(x, y) => {
                if self.motion.selected.is_some() {
                    self.finish_selected();
                } else {
                    return self.control(&Input::Click(*x, *y), state, cursor);
                }
                return Ok(true);
            }
            Input::Mouse(x, y) if self.motion.dragging => {
                let index = self.motion.selected.ok_or("Missing dragged cohort")?;
                let (x, y) = Self::unproject(*x, *y);
                self.move_order(index, x, y);
                return Ok(true);
            }
            Input::Release(_, _) if self.motion.dragging => {
                self.finish_selected();
                return Ok(true);
            }
            _ => 0,
        };
        match key {
            b'd' => {
                self.finish_selected();
                self.motion.paused = false;
            }
            b's' => self.motion.show_orders = true,
            b'm' | b'r' => {
                let selected = self
                    .motion
                    .selected
                    .filter(|&i| self.units[i][11] == 0 && self.can_command(i));
                if self.motion.selected.is_some() && selected.is_none() {
                    return Ok(true);
                }
                if key == b'm' && selected.is_some_and(|i| self.units[i][10] == 3) {
                    return Ok(true);
                }
                let message = if key == b'm' {
                    if let Some(index) = selected {
                        format!(
                            "{}{} {}",
                            state.string(word(&self.ds, 0x74b6)? as usize)?,
                            state.string(word(&self.ds, 0x74ba)? as usize)?,
                            state.roman(index as u8)?
                        )
                    } else {
                        format!(
                            "{}{}",
                            state.string(word(&self.ds, 0x74b6)? as usize)?,
                            state.string(word(&self.ds, 0x74b8)? as usize)?
                        )
                    }
                } else {
                    state.string(if selected.is_some() { 0x74e8 } else { 0x74d4 })?
                };
                self.motion.message = Some(message);
                self.motion.pending = Some((key == b'm', selected));
            }
            _ => {}
        }
        Ok(!matches!(
            input,
            Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
        ))
    }

    fn command(&mut self, melee: bool, selected: Option<usize>) {
        for index in 1..self.units.len() {
            if self.units[index][11] != 0 || selected.is_some_and(|i| i != index) {
                continue;
            }
            let unit = self.units[index];
            let carry = (unit[6] & 0xc0) | u8::from(unit[6] & 63 != 0);
            if unit[13] == 2 {
                self.units[index][6] &= 0xc0;
                self.units[index][13] = 1;
            }
            if melee {
                if self.units[index][13] == 1 {
                    if unit[10] != 3 {
                        self.units[index][13] = 2;
                        self.seek(index);
                    } else {
                        self.units[index][6] &= 0xc0;
                    }
                }
            } else {
                self.retreat(index);
            }
            if selected.is_some() && unit[6] & 63 != 0 {
                self.units[index][7] = self.units[index][6];
                self.units[index][6] = carry;
            }
        }
    }

    fn unproject(x: i32, y: i32) -> (i32, i32) {
        let x = x - 4;
        let y = (y - 2) * 2;
        ((y + x) / 16, (y - x).div_euclid(16) + 20)
    }

    fn select_at(&mut self, x: i32, y: i32) {
        let (x, y) = Self::unproject(x, y);
        let cell = |x: i32, y: i32| -> Option<usize> {
            if !(0..48).contains(&x) || !(0..48).contains(&y) {
                return None;
            }
            let value = usize::from(self.grid[x as usize * 48 + y as usize]);
            (value > 0 && value < self.units.len()).then_some(value)
        };
        self.motion.selected = cell(x, y).or_else(|| {
            (0x74bc..0x74d4).step_by(2).find_map(|at| {
                cell(
                    x + i32::from(self.ds[at] as i8),
                    y + i32::from(self.ds[at + 1] as i8),
                )
            })
        });
        self.motion.order_changed = false;
    }

    fn finish_selected(&mut self) {
        if let Some(index) = self.motion.selected.take()
            && self.motion.order_changed
        {
            put(&mut self.units[index], 4, 65535);
            self.units[index][13] = 1;
        }
        self.motion.order_changed = false;
        self.motion.dragging = false;
    }

    fn can_command(&self, index: usize) -> bool {
        let unit = self.units[index];
        if unit[11] != 0 || matches!(unit[13], 3 | 4) {
            return false;
        }
        let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
        if self.ds[0x1a20 + commander * 6] != 0 {
            return false;
        }
        let general = self
            .units
            .iter()
            .skip(1)
            .find(|u| u[10] == 3 && u[11] == 0)
            .unwrap();
        let x = i32::from(unit[2]) - i32::from(general[2]);
        let y = i32::from(unit[3]) - i32::from(general[3]);
        let radius = i32::from(self.ds[0x1a1f + commander * 6]);
        x * x + y * y <= radius * radius
    }

    fn order_endpoint(&self, index: usize) -> (i32, i32) {
        let unit = self.units[index];
        let (mut x, mut y) = (i32::from(unit[2]), i32::from(unit[3]));
        if !self.motion.order_changed {
            return (x, y);
        }
        let amount = i32::from(unit[6] & 63);
        match unit[6] >> 6 {
            0 => x += amount,
            1 => y += amount,
            2 => x -= amount,
            _ => y -= amount,
        }
        (x, y)
    }

    fn move_order(&mut self, index: usize, x: i32, y: i32) {
        let unit = self.units[index];
        let dx = i32::from(unit[2]) - x;
        let dy = i32::from(unit[3]) - y;
        let direction = if dy.abs() > dx.abs() {
            if dy > 0 { 3 } else { 1 }
        } else if dx > 0 {
            2
        } else {
            0
        };
        let distance = dx.abs().max(dy.abs()) as u8;
        self.units[index][6] = (direction << 6) | distance;
        self.units[index][7..9].fill(128);
        self.motion.order_changed = true;
    }

    pub(super) fn draw_selection(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        state: &Campaign,
    ) -> Result<()> {
        if self.motion.show_orders {
            for index in 1..self.units.len() {
                if self.units[index][11] == 0 {
                    self.draw_orders(frame, index);
                }
            }
        }
        if let Some(index) = self.motion.selected {
            self.draw_info(frame, assets, state, index)?;
            if self.units[index][11] == 0 {
                self.draw_orders(frame, index);
            }
        }
        Ok(())
    }

    fn draw_orders(&self, frame: &mut Frame, index: usize) {
        let unit = self.units[index];
        if unit[13] == 4 {
            return;
        }
        let (mut x, mut y) = (i32::from(unit[2]), i32::from(unit[3]));
        let center = |x, y, direction| {
            let mut unit = unit;
            unit[14] = direction;
            let (w, h) = self.footprint(&unit);
            (
                164 + (x - y) * 8 + 4 * (w as i32 - h as i32),
                (x + y) * 4 + 2 * (w + h) as i32 - 78,
            )
        };
        let first = center(x, y, unit[14]);
        let mut previous = Some(first);
        for &command in &unit[6..9] {
            if command == 128 {
                brush(frame, first.0, first.1);
                previous = None;
                continue;
            }
            let amount = i32::from(command & 63);
            match command >> 6 {
                0 => x += amount,
                1 => y += amount,
                2 => x -= amount,
                _ => y -= amount,
            }
            let next = center(x, y, command >> 6);
            if let Some(from) = previous {
                line(frame, from, next);
            }
            previous = Some(next);
        }
        if unit[10] == 3 {
            let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
            let mut x = 0;
            let mut y = i32::from(self.ds[0x1a1f + commander * 6]);
            let mut error = 3 - 2 * y;
            while x <= y {
                for (dx, dy) in [
                    (x, y),
                    (y, x),
                    (-x, y),
                    (-y, x),
                    (x, -y),
                    (y, -x),
                    (-x, -y),
                    (-y, -x),
                ] {
                    let (px, py) = project(i32::from(unit[2]) + dx, i32::from(unit[3]) + dy);
                    brush(frame, px, py);
                }
                if error < 0 {
                    error += 5 + 4 * x;
                } else {
                    error += 10 + 4 * (x - y);
                    y -= 1;
                }
                x += 1;
            }
        }
    }

    fn draw_info(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        state: &Campaign,
        index: usize,
    ) -> Result<()> {
        let unit = self.units[index];
        let left = if project(i32::from(unit[2]), i32::from(unit[3])).0 < 159 {
            247
        } else {
            2
        };
        if unit[10] == 3 {
            let commander = usize::from(self.ds[0x1928 + self.legion * 20]);
            let text = if unit[11] == 1 {
                state.string(word(&self.ds, 0x7542)? as usize)?
            } else {
                format!(
                    "{}{}{}{}{}",
                    state.string(word(&self.ds, 0x1a1c + commander * 6)? as usize)?,
                    state.string(0x7529)?,
                    self.ds[0x1a1e + commander * 6],
                    state.string(0x7534)?,
                    self.ds[0x1a1f + commander * 6]
                )
            };
            let widths = word(&assets.data_segment, 0xa05c)? as usize;
            let width = text
                .lines()
                .map(|line| {
                    line.bytes()
                        .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                        .sum::<i32>()
                })
                .max()
                .unwrap_or(0)
                / 2
                * 2
                + 14;
            let left = if left == 247 { 317 - width } else { 2 };
            frame.bevel(
                left,
                24,
                left + width,
                24 + text.lines().count() as i32 * 11 / 2 * 2 + 14,
                26,
            );
            for (row, line) in text.lines().enumerate() {
                frame.center_text(assets, line, left + width / 2, 31 + row as i32 * 11, 15)?;
            }
        } else {
            frame.bevel(left, 24, left + 70, 64, 26);
            let name = if unit[11] == 0 {
                format!(
                    "{}{}",
                    state.roman(index as u8)?,
                    state.string(word(&self.ds, 0x74ba)? as usize)?
                )
            } else {
                state.string(0x751a)?
            };
            frame.text(assets, &name, left + 5, 29, 15)?;
            frame.text(
                assets,
                &format!(
                    "{}{}",
                    word(&unit, 0)?,
                    state.string(word(&self.ds, 0x753c + usize::from(unit[10]) * 2)? as usize)?
                ),
                left + 5,
                40,
                15,
            )?;
            let category = usize::from(unit[12] / 25);
            let (text, color) = if unit[13] == 3 {
                (state.string(0x7520)?, 4)
            } else {
                (
                    state.string(word(&self.ds, 0x7486 + category * 2)? as usize)?,
                    word(&self.ds, 0x7544 + category * 2)? as u8,
                )
            };
            frame.text(assets, &text, left + 5, 51, color)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_radius_stop_and_committed_move_preserve_original_order_flags() {
        let mut scene = super::super::tests::synthetic();
        scene.ds[0x1a1f] = 4;
        for (kind, y) in [(2, 20), (3, 23)] {
            scene.add_units(5, 5, kind, 0, 75);
            let unit = scene.units.last_mut().unwrap();
            unit[2] = 10;
            unit[3] = y;
            unit[13] = 2;
            unit[6] = 0xc8;
            put(unit, 4, 0);
        }
        scene.occupy(1);
        scene.occupy(2);
        let state = Campaign::test_data(vec![0; 65536]);
        assert!(scene.can_command(1));
        scene.motion.paused = true;
        scene.motion.selected = Some(1);
        scene.control(&Input::Space, &state, (0, 0)).unwrap();
        assert_eq!(&scene.units[1][6..9], &[128; 3]);
        assert_eq!(word(&scene.units[1], 4).unwrap(), 0);
        scene.control(&Input::Up, &state, (0, 0)).unwrap();
        assert_eq!(scene.units[1][6], 0xc1);
        assert_eq!(word(&scene.units[1], 4).unwrap(), 0);
        scene.control(&Input::Enter, &state, (0, 0)).unwrap();
        assert_eq!(word(&scene.units[1], 4).unwrap(), 65535);
        assert_eq!(scene.units[1][13], 1);
        assert!(scene.motion.paused);
        scene.units[1][3] = 10;
        assert!(!scene.can_command(1));
        scene.units[1][3] = 20;
        scene.ds[0x1a20] = 2;
        assert!(!scene.can_command(1));
        scene.ds[0x1a20] = 0;
        scene
            .control(&Input::Click(84, 46), &state, (84, 46))
            .unwrap();
        assert_eq!(scene.motion.selected, Some(1));
        scene
            .control(&Input::Click(84, 46), &state, (84, 46))
            .unwrap();
        scene
            .control(&Input::Mouse(108, 58), &state, (108, 58))
            .unwrap();
        assert_eq!(scene.units[1][6], 3);
        scene
            .control(&Input::Release(108, 58), &state, (108, 58))
            .unwrap();
        assert_eq!(scene.motion.selected, None);
        assert!(!scene.motion.dragging);
        assert_eq!(word(&scene.units[1], 4).unwrap(), 65535);
    }
}
