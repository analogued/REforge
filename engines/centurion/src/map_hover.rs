// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional map names, with border continuity independent of map selection.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::campaign::{Symbol, Unit};
use crate::graphics::{Frame, HEIGHT, WIDTH};
use crate::state::Campaign;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Clear,
    Border,
    Place(u8),
    Units(u8),
}

#[derive(Default)]
struct Focus {
    region: Option<u8>,
    units: bool,
}

impl Focus {
    fn update(&mut self, target: Target) {
        self.units = false;
        match target {
            Target::Clear => self.region = None,
            Target::Border => {}
            Target::Place(region) => self.region = Some(region),
            Target::Units(region) => {
                self.region = Some(region);
                self.units = true;
            }
        }
    }
}

pub(crate) struct Hover {
    borders: Vec<bool>,
    focus: Focus,
}

impl Hover {
    pub fn new(state: &Campaign, map: &Bitmap) -> Result<Self> {
        let mut borders = vec![false; WIDTH * HEIGHT];
        // These isolated artwork paths also drive the game's ownership borders.
        // A pen alone is insufficient: terrain uses the same brown.
        for province in 0..27 {
            let x = i32::from(word(&state.data, 0x3c52 + province * 4)?);
            let y = i32::from(word(&state.data, 0x3c54 + province * 4)?);
            if !(0..319).contains(&x) || !(0..199).contains(&y) {
                continue;
            }
            let pen = map.pixels[y as usize * WIDTH + x as usize];
            let mut pending = vec![(x, y)];
            while let Some((x, y)) = pending.pop() {
                if !(0..319).contains(&x) || !(0..199).contains(&y) {
                    continue;
                }
                let at = y as usize * WIDTH + x as usize;
                if borders[at] || map.pixels[at] != pen {
                    continue;
                }
                borders[at] = true;
                pending.extend(neighbours((x, y)));
            }
        }
        // Sea separators share the no-region mask value with unnamed water.
        // Seed actual joins between named regions, then include their black
        // bends/endpoints without growing through unnamed water or coast shading.
        let mut regions = Vec::with_capacity(WIDTH * HEIGHT);
        for y in 0..HEIGHT as i32 {
            for x in 0..WIDTH as i32 {
                regions.push(state.region_at(x, y)?);
            }
        }
        let mut pending = Vec::new();
        for y in 0..HEIGHT as i32 {
            for x in 0..WIDTH as i32 {
                let at = y as usize * WIDTH + x as usize;
                if regions[at].is_some() {
                    continue;
                }
                let mut first = None;
                for (nx, ny) in neighbours((x, y)) {
                    if let Some(n) = pixel_index((nx, ny)).and_then(|i| regions[i]) {
                        if first.is_some_and(|previous| previous != n) {
                            borders[at] = true;
                            pending.extend(neighbours((x, y)));
                            break;
                        }
                        first = Some(n);
                    }
                }
            }
        }
        while let Some(point) = pending.pop() {
            let Some(at) = pixel_index(point) else {
                continue;
            };
            if borders[at] || regions[at].is_some() || map.pixels[at] != 0 {
                continue;
            }
            borders[at] = true;
            pending.extend(neighbours(point));
        }
        Ok(Self {
            borders,
            focus: Focus::default(),
        })
    }

    pub fn clear(&mut self) {
        self.focus.update(Target::Clear);
    }

    pub fn update(
        &mut self,
        state: &Campaign,
        sprites: &[Bitmap],
        cursor: (i32, i32),
    ) -> Result<()> {
        let Some(at) = pixel_index(cursor) else {
            self.clear();
            return Ok(());
        };
        for address in [0x3c42, 0x3c4a] {
            let left = i32::from(word(&state.data, address)?);
            let top = i32::from(word(&state.data, address + 2)?);
            let right = i32::from(word(&state.data, address + 4)?);
            let bottom = i32::from(word(&state.data, address + 6)?);
            if (left..=right).contains(&cursor.0) && (top..=bottom).contains(&cursor.1) {
                self.clear();
                return Ok(());
            }
        }
        // Trim transparent outer padding, but keep the symbol's interior holes
        // in its hit area so moving within one symbol does not change the panel.
        // Reverse paint order agrees with overlapping symbols across regions.
        for symbol in crate::campaign::symbols(state)?.iter().rev() {
            let location = crate::campaign::coordinate(state, usize::from(symbol.region))?;
            if hit(&sprites[symbol.sprite], location, cursor) {
                self.focus.update(Target::Units(symbol.region));
                return Ok(());
            }
        }
        let target = if self.borders[at] {
            Target::Border
        } else if let Some(region) = state.region_at(cursor.0, cursor.1)? {
            if state.region_name(region)?.trim().is_empty() {
                Target::Clear
            } else {
                Target::Place(region)
            }
        } else {
            Target::Clear
        };
        self.focus.update(target);
        Ok(())
    }

    pub fn render(&self, assets: &Assets, state: &Campaign, frame: &mut Frame) -> Result<()> {
        let Some(region) = self.focus.region else {
            return Ok(());
        };
        let rows = if self.focus.units {
            unit_names(state, &crate::campaign::symbols(state)?, region)?
        } else {
            Vec::new()
        };
        let groups = rows
            .iter()
            .map(|row| wrap(assets, row))
            .collect::<Result<Vec<_>>>()?;
        let lines = fit_rows(groups, wrap(assets, &state.region_name(region)?)?);
        let width = lines.iter().try_fold(70, |width, (line, _)| {
            Ok::<_, String>(width.max(text_width(assets, line)? + 16))
        })?;
        frame.bevel(2, 0, 2 + width - 1, 21 + (lines.len() as i32 - 1) * 11, 26);
        for (index, (line, color)) in lines.iter().enumerate() {
            frame.text(assets, line, 10, 6 + index as i32 * 11, *color)?;
        }
        Ok(())
    }
}

fn fit_rows(groups: Vec<Vec<String>>, place: Vec<String>) -> Vec<(String, u8)> {
    // Seventeen original-font rows fit above the bottom screen edge. Keep the
    // place name and whole unit groups; crowded stacks get a bounded count.
    let limit = 17_usize.saturating_sub(place.len());
    let overflow = groups.iter().map(Vec::len).sum::<usize>() > limit;
    let budget = limit.saturating_sub(usize::from(overflow));
    let count = groups.len();
    let mut shown = 0;
    let mut lines = Vec::new();
    for group in groups {
        if lines.len() + group.len() > budget {
            break;
        }
        lines.extend(group.into_iter().map(|line| (line, 15)));
        shown += 1;
    }
    if shown < count {
        lines.push((format!("+{} more", count - shown), 15));
    }
    lines.extend(place.into_iter().map(|line| (line, 0)));
    lines
}

fn pixel_index((x, y): (i32, i32)) -> Option<usize> {
    ((0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y))
        .then(|| y as usize * WIDTH + x as usize)
}

fn neighbours((x, y): (i32, i32)) -> [(i32, i32); 4] {
    [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
}

fn hit(bitmap: &Bitmap, anchor: (i32, i32), cursor: (i32, i32)) -> bool {
    let x = cursor.0 - anchor.0 + i32::from(bitmap.origin_x);
    let y = cursor.1 - anchor.1 + i32::from(bitmap.origin_y);
    if x < 0 || y < 0 || x as usize >= bitmap.width || y as usize >= bitmap.height {
        return false;
    }
    let mut left = bitmap.width;
    let mut right = 0;
    let mut top = bitmap.height;
    let mut bottom = 0;
    for (index, &visible) in bitmap.visible.iter().enumerate() {
        if visible {
            left = left.min(index % bitmap.width);
            right = right.max(index % bitmap.width);
            top = top.min(index / bitmap.width);
            bottom = bottom.max(index / bitmap.width);
        }
    }
    (left..=right).contains(&(x as usize)) && (top..=bottom).contains(&(y as usize))
}

fn unit_names(state: &Campaign, symbols: &[Symbol], region: u8) -> Result<Vec<String>> {
    let local: Vec<_> = symbols.iter().filter(|s| s.region == region).collect();
    let mut names = Vec::new();
    let mut transported = Vec::new();
    for symbol in &local {
        if let Unit::Fleet(index) = symbol.unit {
            let mut name = state.string(word(&state.data, 0x1100 + index * 2)? as usize)?;
            let cargo = usize::from(state.data[0x1117 + index * 12]);
            if index < 3 && local.iter().any(|s| s.unit == Unit::Legion(cargo)) {
                name.push_str(": ");
                name.push_str(&state.legion_name(cargo)?);
                transported.push(cargo);
            }
            names.push(name);
        }
    }
    for symbol in local {
        match symbol.unit {
            Unit::Legion(index) if !transported.contains(&index) => {
                names.push(state.legion_name(index)?);
            }
            Unit::Army(index) => {
                let origin = word(&state.data, 0xc114 + index * 14)?;
                if origin >= 27 {
                    return Err("Roaming army origin outside province table".into());
                }
                let name = format!(
                    "{}{}",
                    state.string(0x58e3)?,
                    state.province_name(origin as u8)?
                );
                names.push(name.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            _ => {}
        }
    }
    Ok(names)
}

fn text_width(assets: &Assets, value: &str) -> Result<i32> {
    let widths = word(&assets.data_segment, 0xa05c)? as usize;
    Ok(value
        .bytes()
        .filter(|b| (32..128).contains(b))
        .map(|b| {
            i32::from(assets.data_segment[widths + usize::from(b - 32)])
                + i32::from(assets.data_segment[0xa064])
        })
        .sum())
}

fn wrap(assets: &Assets, value: &str) -> Result<Vec<String>> {
    let mut lines = vec![String::new()];
    for word in value.split_whitespace() {
        let last = lines.last_mut().unwrap();
        let candidate = if last.is_empty() {
            word.to_owned()
        } else {
            format!("{last} {word}")
        };
        if text_width(assets, &candidate)? <= 205 || last.is_empty() {
            *last = candidate;
        } else {
            lines.push(word.to_owned());
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borders_preserve_only_existing_context_and_never_resurrect_it() {
        let mut focus = Focus::default();
        focus.update(Target::Border);
        assert_eq!(focus.region, None);
        focus.update(Target::Place(4));
        for _ in 0..20 {
            focus.update(Target::Border);
            assert_eq!(focus.region, Some(4));
        }
        focus.update(Target::Place(12));
        assert_eq!(focus.region, Some(12));
        focus.update(Target::Clear);
        focus.update(Target::Border);
        assert_eq!(focus.region, None);
        focus.update(Target::Units(32));
        assert!(focus.units);
        focus.update(Target::Border);
        assert_eq!(focus.region, Some(32));
        assert!(!focus.units);
    }

    #[test]
    fn unit_hits_trim_outer_padding_and_keep_transparent_interior_stable() {
        let bitmap = Bitmap {
            width: 3,
            height: 2,
            origin_x: 2,
            origin_y: 1,
            pixels: vec![0; 6],
            visible: vec![false, true, false, false, false, true],
        };
        assert!(hit(&bitmap, (40, 50), (39, 49)));
        assert!(hit(&bitmap, (40, 50), (40, 50)));
        assert!(hit(&bitmap, (40, 50), (39, 50)));
        assert!(!hit(&bitmap, (40, 50), (38, 49)));
        assert!(!hit(&bitmap, (40, 50), (41, 50)));
        assert_eq!(pixel_index((-1, 0)), None);
        assert_eq!(pixel_index((320, 0)), None);
    }

    #[test]
    fn border_cache_separates_paths_and_separator_bends_from_unnamed_water() {
        let mut data = vec![0; 65536];
        let put = |data: &mut [u8], at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        data[0xd000..0xd008].copy_from_slice(&[0, 100, 255, 1, 1, 99, 255, 120]);
        data[0xd010..0xd018].copy_from_slice(&[0, 100, 255, 3, 1, 97, 255, 120]);
        for y in 0..200 {
            put(
                &mut data,
                0x3a96 + y * 2,
                if y == 50 { 0xd010 } else { 0xd000 },
            );
        }
        for province in 0..27 {
            put(&mut data, 0x3c52 + province * 4, 20);
            put(&mut data, 0x3c54 + province * 4, 20);
        }
        let mut map = Bitmap {
            width: WIDTH,
            height: HEIGHT,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![111; WIDTH * HEIGHT],
            visible: vec![true; WIDTH * HEIGHT],
        };
        for (x, y, pen) in [
            (20, 20, 104),
            (20, 21, 104),
            (21, 21, 104),
            (60, 60, 104),  // Same pen, separate terrain component.
            (100, 35, 106), // Colored join between two named regions.
            (100, 49, 0),
            (100, 50, 0),
            (101, 50, 0),
            (102, 50, 0),
        ] {
            map.pixels[y * WIDTH + x] = pen;
        }
        let hover = Hover::new(&Campaign::test_data(data), &map).unwrap();
        for point in [
            (20, 20),
            (21, 21),
            (100, 35),
            (100, 50),
            (101, 50),
            (102, 50),
        ] {
            assert!(hover.borders[pixel_index(point).unwrap()], "{point:?}");
        }
        for point in [(60, 60), (250, 35), (250, 50)] {
            assert!(!hover.borders[pixel_index(point).unwrap()], "{point:?}");
        }
    }

    #[test]
    fn fleet_rows_pair_each_embarked_legion_once_and_leave_other_units_visible() {
        let mut data = vec![0; 65536];
        let put = |data: &mut [u8], at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        for (at, value) in [
            (0xd000, ""),
            (0xd010, "IV"),
            (0xd020, "VIII"),
            (0xd100, "Fleet I"),
            (0xd110, "Fleet II"),
            (0xd120, "Mauretania"),
            (0x1918, " Ital"),
            (0x192c, " Germ"),
            (0x58e3, "Marauding Army\nof "),
        ] {
            data[at..at + value.len()].copy_from_slice(value.as_bytes());
        }
        for at in [0x4f82, 0x4f6e] {
            put(&mut data, at, 0xd000);
        }
        put(&mut data, 0x4f5a + 4 * 2, 0xd010);
        put(&mut data, 0x4f5a + 8 * 2, 0xd020);
        put(&mut data, 0x1100, 0xd100);
        put(&mut data, 0x1102, 0xd110);
        put(&mut data, 0x5222 + 16 * 2, 0xd120);
        put(&mut data, 0xc114, 16);
        data[0x1922] = 4;
        data[0x1936] = 8;
        data[0x1117] = 0;
        data[0x1123] = 1;
        let mut state = Campaign::test_data(data);
        let symbols = [
            Unit::Fleet(0),
            Unit::Fleet(1),
            Unit::Legion(0),
            Unit::Legion(1),
            Unit::Army(0),
        ]
        .map(|unit| Symbol {
            unit,
            region: 32,
            sprite: 0,
        });
        assert_eq!(
            unit_names(&state, &symbols, 32).unwrap(),
            [
                "Fleet I: IV Ital",
                "Fleet II: VIII Germ",
                "Marauding Army of Mauretania"
            ]
        );
        state.data[0x1117] = 255;
        assert_eq!(
            unit_names(&state, &symbols, 32).unwrap(),
            [
                "Fleet I",
                "Fleet II: VIII Germ",
                "IV Ital",
                "Marauding Army of Mauretania"
            ]
        );
    }

    #[test]
    fn crowded_panels_keep_whole_unit_groups_and_the_place_name_on_screen() {
        let units = (0..20).map(|i| vec![format!("Unit {i}")]).collect();
        let lines = fit_rows(units, vec!["Sea".into()]);
        assert_eq!(lines.len(), 17);
        assert_eq!(lines[15], ("+5 more".into(), 15));
        assert_eq!(lines[16], ("Sea".into(), 0));
        assert!(21 + (lines.len() as i32 - 1) * 11 < HEIGHT as i32);
        let groups = (0..10)
            .map(|_| vec!["Long unit".into(), "continuation".into()])
            .collect();
        let lines = fit_rows(groups, vec!["Sea".into()]);
        assert_eq!(lines[14], ("+3 more".into(), 15));
        assert_eq!(lines[15], ("Sea".into(), 0));
    }
}
