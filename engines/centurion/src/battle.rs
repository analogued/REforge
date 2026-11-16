// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original land-battle preparation, simulation, commands and campaign results.
mod combat;
mod controls;
mod motion;
mod outcome;
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::province::Menu;
use crate::rng::OriginalRng;
use crate::state::Campaign;
pub use motion::Motion;

pub const FORMATION_MENU: usize = 0x73b6;
pub const TACTICS_MENU: usize = 0x73c8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Formation,
    Tactics,
    Deployment,
}

pub struct Preparation {
    pub stage: Stage,
    pub province: u8,
    pub enemy_province: u8,
    pub army: Option<usize>,
    pub formation: usize,
    pub enemy_formation: usize,
    pub enemy_tactic: usize,
    pub tactic: usize,
    pub units: Vec<[u8; 18]>,
    pub motion: Motion,
    pub outcome: Option<outcome::Outcome>,
    pub legion: usize,
    ds: Vec<u8>,
    terrain: Bitmap,
    sprites: Vec<Bitmap>,
    button: Bitmap,
    commands: Vec<u8>,
    grid: Vec<u8>,
}

fn put(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn depth_sort(ids: &mut [usize], units: &[[u8; 18]]) {
    // BD67's in-place partition deliberately swaps equal depths too. Routing
    // units may overlap; a stable/library sort changes their pixel overlap.
    if ids.len() < 2 {
        return;
    }
    let key = |i: usize| u16::from(units[i][2]) | u16::from(units[i][3]) << 8;
    let pivot = key(ids[(ids.len() - 1) / 2]);
    let (mut left, mut right) = (0usize, ids.len() - 1);
    loop {
        while key(ids[left]) < pivot {
            left += 1;
        }
        while key(ids[right]) > pivot {
            right -= 1;
        }
        if left > right {
            break;
        }
        ids.swap(left, right);
        left += 1;
        if right == 0 {
            break;
        }
        right -= 1;
        if left > right {
            break;
        }
    }
    if right > 0 {
        depth_sort(&mut ids[..=right], units);
    }
    if left < ids.len() {
        depth_sort(&mut ids[left..], units);
    }
}

impl Preparation {
    pub fn grid(&self) -> &[u8] {
        &self.grid
    }

    pub fn new(
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        legion: usize,
        province: u8,
        previous: &Frame,
    ) -> Result<Self> {
        Self::against(assets, state, rng, legion, province, None, previous)
    }

    pub fn against(
        assets: &Assets,
        state: &mut Campaign,
        rng: &mut OriginalRng,
        legion: usize,
        province: u8,
        army: Option<usize>,
        previous: &Frame,
    ) -> Result<Self> {
        state.sound.stop();
        if army.is_none() {
            state.legion_location(legion)?;
        } else if legion > 12 || (legion == 12 && province != 0) {
            return Err("Invalid intercepted legion slot".into());
        }
        if province >= 27 {
            return Err("Invalid land battle province".into());
        }
        let mut ds = state.data.clone();
        let base = usize::from(province) * 32;
        if let Some(army) = army {
            if army >= 11 || word(&ds, 0xc11a + army * 14)? == 0 {
                return Err("No active roaming army at this destination".into());
            }
            let record: [u8; 12] = ds[0xc114 + army * 14..0xc120 + army * 14]
                .try_into()
                .unwrap();
            ds[0xc29a..0xc2a6].copy_from_slice(&record);
        } else {
            put(&mut ds, 0xc29a, u16::from(province));
            for (to, from) in [(0xc29c, 0x5272 + base), (0xc29e, 0x5270 + base)] {
                let value = word(&ds, from)?;
                put(&mut ds, to, value);
            }
            put(&mut ds, 0xc2a0, 65535);
            ds[0xc2a2] = ds[0x527c + base];
            ds[0xc2a3] = ds[0x527b + base];
            ds[0xc2a4] = ds[0x527a + base];
        }
        let enemy_province = word(&ds, 0xc29a)?;
        if enemy_province >= 27 {
            return Err("Roaming army origin outside province table".into());
        }
        let enemy_province = enemy_province as u8;
        let terrain = assets.bitmap(
            "BATL",
            if ds[0x5279 + base] == 1 {
                "DESERT.CBM"
            } else {
                "BATTLENW.CBM"
            },
        )?;
        let mut terrain = terrain;
        let mut frame = previous.clone();
        frame.blit(&terrain, 0, 0);
        if ds[0x5279 + base] != 1 {
            frame.blit(&assets.bitmap("BATL", "WATER.CBM")?, 0, 0);
        }
        terrain.pixels = frame.pixels;
        terrain.visible.fill(true);
        let barbarian = (11..=14).contains(&enemy_province) || (23..=26).contains(&enemy_province);
        let mut pens: [u8; 16] = ds[0x7252..0x7262].try_into().unwrap();
        pens[4] = ds[0x7262 + usize::from(enemy_province)];
        let mut sprites = Vec::new();
        for (stem, remap) in [
            ("RI", false),
            ("RC", false),
            ("RG", false),
            (if barbarian { "BI" } else { "RI" }, true),
            ("RC", true),
            (if barbarian { "BG" } else { "RG" }, true),
            ("EE", false),
        ] {
            sprites.extend(assets.sprites_with_pens("BATL", stem, remap.then_some(&pens))?);
        }
        let at = 0x1918 + legion * 20;
        let infantry = word(&ds, at + 6)?;
        let cavalry = word(&ds, at + 8)?;
        let cohort = infantry.wrapping_add(11) / 12;
        let cohort = cohort
            .max(word(&ds, 0x7402 + usize::from(ds[0x48e]) * 2)?)
            .min(500);
        if cohort == 0 {
            return Err("Cannot deploy an empty infantry cohort".into());
        }
        let mut scene = Self {
            stage: Stage::Formation,
            province,
            enemy_province,
            army,
            formation: 0,
            enemy_formation: 0,
            enemy_tactic: 0,
            tactic: 0,
            units: vec![[0; 18]],
            motion: Motion::default(),
            outcome: None,
            legion,
            terrain,
            sprites,
            grid: vec![0; 48 * 48],
            commands: assets.resource("BATL", "TACTICS.LZW")?,
            button: assets
                .sprites("MINIT", "INIT")?
                .into_iter()
                .nth(1)
                .ok_or("Missing selection button")?,
            ds: Vec::new(),
        };
        put(
            &mut ds,
            0x7400,
            infantry.div_ceil(cohort) + cavalry.div_ceil(150) + 1,
        );
        let enemy_infantry = word(&ds, 0xc29e)?;
        let enemy_cavalry = word(&ds, 0xc29c)?;
        let elephants = u16::from(ds[0xc2a2]);
        put(
            &mut ds,
            0x73fe,
            enemy_infantry.div_ceil(500) + enemy_cavalry.div_ceil(150) + elephants.div_ceil(20) + 1,
        );
        scene.ds = ds;
        scene.reset_grid();
        let courage = state.data[at + 11];
        scene.add_units(cavalry, 150, 0, 0, courage);
        scene.add_units(infantry, cohort, 2, 0, courage);
        scene.add_units(5, 5, 3, 0, courage);
        let courage = scene.ds[0xc2a4];
        scene.add_units(elephants, 20, 1, 1, courage);
        scene.add_units(enemy_cavalry, 150, 0, 1, courage);
        scene.add_units(enemy_infantry, 500, 2, 1, courage);
        scene.add_units(5, 5, 3, 1, courage);
        let shape = usize::from(state.data[0x5286 + usize::from(enemy_province) * 32]);
        let enemy_units = word(&scene.ds, 0x73fe)?;
        let size_class = usize::from((enemy_units.wrapping_sub(2) / 4) as u8);
        scene.enemy_formation = scene.weighted(rng, 0x77f0 + (shape * 4 + size_class) * 5, 5)?;
        let count = word(&state.data, 0xc0f8)?.wrapping_add(1);
        put(&mut state.data, 0xc0f8, count);
        Ok(scene)
    }

    fn add_units(&mut self, mut count: u16, limit: u16, kind: u8, side: u8, courage: u8) {
        while count > 0 {
            let mut unit = [0; 18];
            put(&mut unit, 0, count.min(limit));
            put(&mut unit, 4, 65535);
            unit[6..9].fill(128);
            unit[9] = self.ds[0x7244 + usize::from(kind) * 2 + usize::from(side)];
            unit[10] = kind;
            unit[11] = side;
            unit[12] = courage;
            unit[14] = if side == 0 { 3 } else { 1 };
            self.units.push(unit);
            count = count.saturating_sub(limit);
        }
    }

    fn weighted(&self, rng: &mut OriginalRng, at: usize, count: usize) -> Result<usize> {
        let value = usize::from(rng.bounded(100));
        let mut sum = 0;
        for row in 0..count {
            sum += usize::from(
                *self
                    .ds
                    .get(at + row)
                    .ok_or("Formation probabilities outside data")?,
            );
            if sum > value {
                return Ok(row);
            }
        }
        Err("Invalid battle probability distribution".into())
    }

    fn reset_grid(&mut self) {
        self.grid.fill(0);
        let mut block = |x: usize, y: usize| self.grid[x * 48 + y] = 254;
        for n in 0..19 {
            block(n, 18 - n);
        }
        for n in 0..24 {
            block(n, n + 23);
            block(43 - n, 23 - n);
        }
        for n in 0..20 {
            block(n + 24, 47 - n);
        }
        for n in 19..=21 {
            block(0, n);
        }
        block(19, 0);
    }

    fn footprint(&self, unit: &[u8; 18]) -> (usize, usize) {
        let kind = usize::from(unit[10]);
        let (w, h) = (
            usize::from(self.ds[0x6ae8 + kind]),
            usize::from(self.ds[0x6aec + kind]),
        );
        if unit[14] & 1 == 0 { (h, w) } else { (w, h) }
    }

    fn free(&self, index: usize) -> bool {
        let unit = &self.units[index];
        let (w, h) = self.footprint(unit);
        for x in usize::from(unit[2])..usize::from(unit[2]) + w {
            for y in usize::from(unit[3])..usize::from(unit[3]) + h {
                if x >= 48 || y >= 48 {
                    return false;
                }
                let occupied = self.grid[x * 48 + y];
                if occupied != 0 && usize::from(occupied) != index {
                    return false;
                }
            }
        }
        true
    }

    fn occupy(&mut self, index: usize) {
        let unit = &self.units[index];
        let (w, h) = self.footprint(unit);
        for x in usize::from(unit[2])..(usize::from(unit[2]) + w).min(48) {
            for y in usize::from(unit[3])..(usize::from(unit[3]) + h).min(48) {
                if self.grid[x * 48 + y] == 0 {
                    self.grid[x * 48 + y] = index as u8;
                }
            }
        }
    }

    fn position(
        &mut self,
        index: usize,
        table: usize,
        slot: usize,
        side: u8,
        first: bool,
    ) -> Result<()> {
        let origin = word(&self.ds, table + 0x24 + usize::from(side) * 2)?;
        let point = word(&self.ds, table + 2 + slot * 2)?;
        let (ox, oy) = (origin as u8, (origin >> 8) as u8);
        let (x, y) = (point as u8, (point >> 8) as u8);
        self.units[index][2] = if side == 0 {
            ox.wrapping_add(x)
        } else {
            ox.wrapping_sub(x)
        };
        self.units[index][3] = if side == 0 {
            oy.wrapping_add(y)
        } else {
            oy.wrapping_sub(y)
        };
        if side == 1 {
            let kind = usize::from(self.units[index][10]);
            self.units[index][2] = self.units[index][2].wrapping_sub(if first {
                1
            } else {
                self.ds[0x6ae8 + kind] - 1
            });
            if first || kind != 2 {
                self.units[index][3] = self.units[index][3].wrapping_sub(1);
            }
        }
        Ok(())
    }

    fn layout(&mut self, formation: usize, side: u8) -> Result<()> {
        let table = word(&self.ds, 0x76ec + formation * 2)? as usize;
        let ids: Vec<_> = (1..self.units.len())
            .filter(|&i| self.units[i][11] == side)
            .collect();
        let mut elephant = 0;
        for &index in &ids {
            if self.units[index][10] == 1 {
                self.units[index][2] = self.ds[0x73da + formation * 6 + elephant];
                self.units[index][3] = self.ds[0x73f8 + formation];
                self.units[index][6] = 254;
                elephant += 1;
                self.occupy(index);
            }
        }
        let regular: Vec<_> = ids
            .into_iter()
            .filter(|&i| self.units[i][10] != 1)
            .collect();
        for (group, &index) in regular
            .iter()
            .filter(|&&i| self.units[i][10] == 0)
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .iter()
            .enumerate()
        {
            let slot = usize::from(self.ds[table + 0x28 + group]);
            self.position(index, table, slot, side, true)?;
            self.units[index][6] = slot as u8;
            self.occupy(index);
        }
        let mut slot = 0;
        for &index in &regular {
            if self.units[index][6] != 128 {
                continue;
            }
            if self.units[index][10] == 3 {
                self.position(index, table, 16, side, false)?;
                self.units[index][6] = 16;
            } else {
                loop {
                    if slot > 17 {
                        return Err("Battle formation has no free deployment slot".into());
                    }
                    self.position(index, table, slot, side, false)?;
                    slot += 1;
                    if self.free(index) {
                        self.units[index][6] = (slot - 1) as u8;
                        break;
                    }
                }
            }
            self.occupy(index);
        }
        Ok(())
    }

    pub fn select_formation(&mut self, rng: &mut OriginalRng, formation: usize) -> Result<()> {
        if formation >= 4 {
            return Err("Invalid Roman formation".into());
        }
        self.formation = formation;
        self.reset_grid();
        for unit in self.units.iter_mut().skip(1) {
            unit[2] = 0;
            unit[3] = 0;
            unit[6..9].fill(128);
        }
        self.layout(formation, 0)?;
        self.layout(self.enemy_formation, 1)?;
        let shape = usize::from(self.ds[0x5286 + usize::from(self.enemy_province) * 32]);
        let probabilities =
            word(&self.ds, 0x7986 + (shape * 5 + self.enemy_formation) * 2)? as usize;
        self.enemy_tactic = if probabilities == 0 {
            usize::from(rng.bounded(4))
        } else {
            self.weighted(rng, probabilities + formation * 5, 5)?
        };
        self.stage = Stage::Tactics;
        Ok(())
    }

    pub fn battle_music(&self) -> u8 {
        let province = word(&self.ds, 0xc29a).unwrap_or(0);
        if (11..=14).contains(&province) || (23..=26).contains(&province) {
            10
        } else {
            12
        }
    }

    pub fn select_tactic(&mut self, tactic: usize) -> Result<()> {
        if tactic >= 5 {
            return Err("Invalid tactic".into());
        }
        self.tactic = tactic;
        for (side, formation, tactic) in [
            (0, self.formation, tactic),
            (1, self.enemy_formation, self.enemy_tactic),
        ] {
            let at = 0x76f6 + (formation * 5 + tactic) * 10;
            for unit in self.units.iter_mut().skip(1).filter(|u| u[11] == side) {
                let slot = usize::from(unit[6]);
                unit[6] = 128;
                if slot == 254 {
                    unit[6] = 75;
                    unit[13] = 1;
                    continue;
                }
                for command in 0..3 {
                    let table = word(&self.ds, at + 2 + command * 2)?;
                    if table == 65535 {
                        unit[6 + command] = 128;
                        continue;
                    }
                    let value = *self
                        .commands
                        .get(usize::from(table) + slot)
                        .ok_or("Tactic order outside resource")?;
                    let direction = if side == 1 {
                        ((value >> 6) + 2) & 3
                    } else {
                        value >> 6
                    };
                    unit[6 + command] = (direction << 6) | (value & 63);
                }
                unit[13] = 1;
            }
        }
        self.stage = Stage::Deployment;
        Ok(())
    }

    pub fn menu(&self, state: &Campaign) -> Result<Menu> {
        let mut menu = Menu::read(
            state,
            if self.stage == Stage::Formation {
                FORMATION_MENU
            } else {
                TACTICS_MENU
            },
        )?;
        if self.stage != Stage::Formation {
            for (row, item) in menu.items.iter_mut().take(4).enumerate() {
                item.label = state
                    .string(word(&self.ds, 0x76f6 + (self.formation * 5 + row) * 10)? as usize)?;
            }
        }
        Ok(menu)
    }

    pub fn render(
        &self,
        assets: &Assets,
        state: &Campaign,
        selected: Option<usize>,
    ) -> Result<Frame> {
        let mut frame = self.field_frame()?;
        if self.stage == Stage::Deployment {
            self.draw_controls(&mut frame, assets, state)?;
            return Ok(frame);
        }
        self.draw_menu(&mut frame, assets, state, selected)?;
        Ok(frame)
    }

    pub fn field_frame(&self) -> Result<Frame> {
        let mut frame = Frame::new();
        frame.blit(&self.terrain, 0, 0);
        if self.stage != Stage::Formation {
            self.draw_units(&mut frame)?;
        }
        Ok(frame)
    }

    fn draw_menu(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        state: &Campaign,
        selected: Option<usize>,
    ) -> Result<()> {
        let menu = self.menu(state)?;
        let (left, top, right, bottom) = menu.rect;
        frame.bevel(left, top, right, bottom, 26);
        frame.center_text(assets, &menu.title, (left + right) / 2, top + 6, 15)?;
        crate::province::decorate_menu(
            frame,
            &self.button,
            state,
            &menu.items.iter().map(|item| item.rect).collect::<Vec<_>>(),
            selected,
            menu.joined_decoration,
        );
        for (row, item) in menu.items.iter().enumerate() {
            let (x, y, _, _) = item.rect;
            frame.text(
                assets,
                &item.label,
                x + 11,
                y,
                if selected == Some(row) { 15 } else { 0 },
            )?;
        }
        Ok(())
    }

    fn draw_units(&self, frame: &mut Frame) -> Result<()> {
        let units = if self.motion.drawn.is_empty() {
            &self.units
        } else {
            &self.motion.drawn
        };
        let mut ids: Vec<_> = (1..units.len()).filter(|&i| units[i][13] != 4).collect();
        depth_sort(&mut ids, units);
        for index in ids {
            let unit = &units[index];
            let kind = usize::from(unit[9]);
            let sequence = word(&self.ds, 0x6da6 + kind * 2)? as usize;
            let animation = if self.ds[sequence] == 255 {
                unit[15]
            } else {
                self.ds[sequence + usize::from(unit[15])]
            };
            let sprite = word(&self.ds, 0x6b28 + kind * 2)? as usize
                + usize::from(animation)
                + if unit[11] == 1 { 191 } else { 0 };
            let offset = word(&self.ds, 0x719c + kind * 2)? as usize + usize::from(unit[15]) * 2;
            let x = 164
                + (i32::from(unit[2] as i8) - i32::from(unit[3] as i8)) * 8
                + i32::from(self.ds[offset] as i8);
            let y = (i32::from(unit[2] as i8) + i32::from(unit[3] as i8)) * 4 - 78
                + i32::from(self.ds[offset + 1] as i8);
            frame.blit(
                self.sprites
                    .get(sprite)
                    .ok_or("Battle sprite outside loaded banks")?,
                x,
                y,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn synthetic() -> Preparation {
        let bitmap = Bitmap {
            width: 1,
            height: 1,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![0],
            visible: vec![false],
        };
        let mut ds = vec![0; 65536];
        ds[0x6ae8..0x6aec].copy_from_slice(&[2, 2, 2, 1]);
        ds[0x6aec..0x6af0].copy_from_slice(&[2, 3, 2, 2]);
        Preparation {
            stage: Stage::Formation,
            province: 11,
            enemy_province: 11,
            army: None,
            formation: 0,
            enemy_formation: 0,
            enemy_tactic: 0,
            tactic: 0,
            units: vec![[0; 18]],
            motion: Motion::default(),
            outcome: None,
            legion: 0,
            ds,
            terrain: bitmap.clone(),
            sprites: Vec::new(),
            button: bitmap,
            commands: vec![0; 64],
            grid: vec![0; 48 * 48],
        }
    }

    #[test]
    fn troop_split_preserves_totals_partial_last_unit_and_initial_flags() {
        let mut scene = synthetic();
        scene.add_units(4200, 500, 2, 0, 75);
        assert_eq!(scene.units.len(), 10);
        assert_eq!(
            scene
                .units
                .iter()
                .map(|u| word(u, 0).unwrap() as usize)
                .sum::<usize>(),
            4200
        );
        assert_eq!(word(scene.units.last().unwrap(), 0).unwrap(), 200);
        for unit in scene.units.iter().skip(1) {
            assert_eq!(&unit[6..9], &[128, 128, 128]);
            assert_eq!(
                (unit[10], unit[11], unit[12], unit[13], unit[14]),
                (2, 0, 75, 0, 3)
            );
            assert_eq!(word(unit, 4).unwrap(), 65535);
        }
    }

    #[test]
    fn footprints_respect_blocked_cells_and_never_overwrite_occupied_units() {
        let mut scene = synthetic();
        scene.reset_grid();
        assert_eq!(scene.grid[18], 254);
        scene.add_units(20, 20, 1, 1, 60);
        scene.add_units(20, 20, 1, 1, 60);
        for unit in scene.units.iter_mut().skip(1) {
            unit[2] = 10;
            unit[3] = 20;
        }
        assert!(scene.free(1));
        scene.occupy(1);
        assert!(!scene.free(2));
        scene.occupy(2);
        assert_eq!(scene.grid.iter().filter(|&&owner| owner == 1).count(), 6);
        assert_eq!(scene.grid.iter().filter(|&&owner| owner == 2).count(), 0);
        scene.units[2][2] = 47;
        assert!(!scene.free(2));
    }

    #[test]
    fn tactic_sentinels_keep_other_orders_and_enemy_directions_rotate() {
        let mut scene = synthetic();
        for side in 0..2 {
            scene.add_units(100, 100, 2, side, 75);
            scene.units.last_mut().unwrap()[6] = 2;
        }
        // All three absent orders must remain distinct per-byte sentinels.
        for command in 0..3 {
            put(&mut scene.ds, 0x76f8 + command * 2, 65535);
        }
        put(&mut scene.ds, 0x76f8, 0);
        scene.commands[2] = 0x85;
        scene.select_tactic(0).unwrap();
        assert_eq!(&scene.units[1][6..9], &[0x85, 128, 128]);
        assert_eq!(&scene.units[2][6..9], &[0x05, 128, 128]);
        assert_eq!(scene.units[1][13], 1);
        assert_eq!(scene.units[2][13], 1);
    }

    #[test]
    fn equal_depth_sprites_follow_the_original_partition_order() {
        let units = [[0; 18]; 4];
        let mut ids = vec![1, 2, 3];
        depth_sort(&mut ids, &units);
        assert_eq!(ids, [3, 2, 1]);
    }

    #[test]
    fn battle_ticks_are_chunk_independent_and_paused_time_does_not_accumulate() {
        fn moving() -> Preparation {
            let mut scene = synthetic();
            scene.stage = Stage::Deployment;
            scene.ds[0x6bd0] = 2;
            for side in 0..2 {
                scene.add_units(100, 100, 2, side, 75);
                let i = scene.units.len() - 1;
                scene.units[i][2] = if side == 0 { 10 } else { 30 };
                scene.units[i][3] = if side == 0 { 25 } else { 10 };
                scene.units[i][6] = if side == 0 { 0xc3 } else { 0x80 };
                scene.units[i][13] = 1;
                put(&mut scene.units[i], 4, 0);
                scene.occupy(i);
            }
            scene
        }
        let (mut whole, mut split) = (moving(), moving());
        let mut state = Campaign::test_data(vec![0; 65536]);
        let mut other_state = Campaign::test_data(vec![0; 65536]);
        let mut rng = OriginalRng {
            first: 7,
            second: 11,
        };
        let mut other_rng = rng;
        whole.advance(72, &mut state, &mut rng).unwrap();
        for _ in 0..72 {
            split.advance(1, &mut other_state, &mut other_rng).unwrap();
        }
        assert_eq!(whole.units, split.units);
        assert_eq!(whole.grid, split.grid);
        assert_eq!(whole.motion.iteration, 6);
        assert_eq!((rng.first, rng.second), (other_rng.first, other_rng.second));
        split.motion.paused = true;
        split
            .advance(720, &mut other_state, &mut other_rng)
            .unwrap();
        assert_eq!(split.units, whole.units);
        split.motion.paused = false;
        split.advance(11, &mut other_state, &mut other_rng).unwrap();
        assert_eq!(split.units, whole.units);
        split.advance(1, &mut other_state, &mut other_rng).unwrap();
        assert_eq!(split.motion.iteration, 7);
    }

    #[test]
    fn an_already_destroyed_side_counts_the_initial_end_check_before_updates() {
        let mut scene = synthetic();
        scene.stage = Stage::Deployment;
        scene.ds[0x6bd0] = 1;
        for (kind, side, x, y) in [(3, 0, 10, 25), (2, 1, 30, 10)] {
            scene.add_units(5, 5, kind, side, 75);
            let unit = scene.units.last_mut().unwrap();
            unit[2] = x;
            unit[3] = y;
            unit[13] = 1;
            put(unit, 4, 0);
        }
        let mut state = Campaign::test_data(scene.ds.clone());
        let mut rng = OriginalRng {
            first: 7,
            second: 11,
        };
        scene.advance(30 * 12, &mut state, &mut rng).unwrap();
        assert_eq!(scene.motion.iteration, 30);
        assert!(scene.motion.finished);
        assert!(scene.outcome.is_none());
        scene.advance(11, &mut state, &mut rng).unwrap();
        assert!(scene.outcome.is_none());
        scene.advance(1, &mut state, &mut rng).unwrap();
        assert!(!scene.outcome.as_ref().unwrap().victory);
    }
}
