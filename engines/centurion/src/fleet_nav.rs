// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Campaign fleet information, selection and sea movement (2B40..30E4).
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

impl Campaign {
    pub fn fleets(&self) -> Vec<usize> {
        (0..3)
            .filter(|i| self.data[0x1116 + i * 12] != 255)
            .collect()
    }
    pub fn local_fleets(&self, region: u8) -> Vec<usize> {
        self.fleets()
            .into_iter()
            .filter(|i| self.data[0x1114 + i * 12] == region)
            .collect()
    }
    pub fn fleet_location(&self, index: usize) -> Result<u8> {
        if index >= 6 || self.data[0x1116 + index * 12] == 255 {
            return Err("No active fleet".into());
        }
        let region = self.data[0x1114 + index * 12];
        if !(27..37).contains(&region) {
            return Err("Fleet is outside sea regions".into());
        }
        Ok(region)
    }
    pub fn fleet_destinations(&self, index: usize) -> Result<Vec<u8>> {
        let source = self.fleet_location(index)?;
        Ok(
            self.data[0x50ec + usize::from(source) * 8..0x50f4 + usize::from(source) * 8]
                .iter()
                .copied()
                .filter(|p| (27..37).contains(p))
                .collect(),
        )
    }
    pub fn complete_fleet_move(&mut self, index: usize, destination: u8) -> Result<()> {
        if index >= 3
            || !self.fleet_destinations(index)?.contains(&destination)
            || self.data[0x1116 + index * 12] != 0
        {
            return Err("Fleet cannot sail to destination".into());
        }
        self.finish_fleet_sailing(index, destination)
    }
    pub fn finish_fleet_sailing(&mut self, index: usize, destination: u8) -> Result<()> {
        if index >= 3 || !self.fleet_destinations(index)?.contains(&destination) {
            return Err("Invalid fleet destination".into());
        }
        let at = 0x1114 + index * 12;
        let cargo = self.data[at + 3];
        if cargo != 255 && cargo >= 12 {
            return Err("Invalid transported legion".into());
        }
        self.data[at] = destination;
        self.data[at + 2] = 1;
        if cargo != 255 {
            if cargo >= 12 {
                return Err("Invalid transported legion".into());
            }
            self.data[0x1924 + usize::from(cargo) * 20] = destination;
        }
        Ok(())
    }
}
const ROMAN_COLUMNS: [(&str, i32); 6] = [
    ("Fleet", 41),
    ("Triremes", 100),
    ("Quinqueremes", 159),
    ("Galleons", 214),
    ("Legion", 252),
    ("Status", 292),
];
const ENEMY_COLUMNS: [(&str, i32); 5] = [
    ("Fleet", 53),
    ("Triremes", 118),
    ("Quinqueremes", 177),
    ("Galleons", 236),
    ("Troops", 288),
];
// These gaps include glyph advance/tracking, matching center_text's rounding.
const ROMAN_GAPS: [i32; 5] = [4, 4, 4, 8, 4];

fn fit_roman_columns(widths: &[[i32; 6]]) -> [i32; 6] {
    let mut centers = ROMAN_COLUMNS.map(|(_, center)| center);
    // Keep Fleet and Status fixed; make only the necessary leftward adjustments.
    for column in (1..5).rev() {
        let distance = widths
            .iter()
            .filter(|line| line[column] != 0 && line[column + 1] != 0)
            .map(|line| (line[column] + 1) / 2 + line[column + 1] / 2 + ROMAN_GAPS[column])
            .max()
            .unwrap_or(0);
        centers[column] = centers[column].min(centers[column + 1] - distance);
    }
    centers
}
fn text_width(assets: &Assets, text: &str) -> Result<i32> {
    let widths = usize::from(word(&assets.data_segment, 0xa05c)?);
    let tracking = i32::from(assets.data_segment[0xa064]);
    Ok(text
        .bytes()
        .filter(|b| (32..128).contains(b))
        .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + tracking)
        .sum())
}
struct Table {
    columns: Vec<(&'static str, i32)>,
    rows: Vec<[String; 6]>,
}
impl Table {
    fn read(assets: &Assets, state: &Campaign, ids: &[usize]) -> Result<Self> {
        let rows = ids
            .iter()
            .map(|&index| {
                Ok([
                    state.string(word(&state.data, 0x1100 + index * 2)? as usize)?,
                    word(&state.data, 0x110e + index * 12)?.to_string(),
                    word(&state.data, 0x1110 + index * 12)?.to_string(),
                    word(&state.data, 0x1112 + index * 12)?.to_string(),
                    cargo_label(state, index)?,
                    if index < 3 {
                        state.string(word(
                            &state.data,
                            0x1b54 + usize::from(state.data[0x1116 + index * 12]) * 2,
                        )? as usize)?
                    } else {
                        String::new()
                    },
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        let columns = if ids.iter().all(|&id| id < 3) {
            let mut widths = Vec::with_capacity(rows.len() + 1);
            let mut header = [0; 6];
            for i in 1..6 {
                header[i] = text_width(assets, ROMAN_COLUMNS[i].0)?;
            }
            widths.push(header);
            for row in &rows {
                let mut line = [0; 6];
                for i in 0..6 {
                    line[i] = text_width(assets, &row[i])?;
                }
                widths.push(line);
            }
            let centers = fit_roman_columns(&widths);
            ROMAN_COLUMNS
                .iter()
                .zip(centers)
                .map(|(&(label, _), center)| (label, center))
                .collect()
        } else {
            ENEMY_COLUMNS.to_vec()
        };
        Ok(Self { columns, rows })
    }
    fn headings(&self, frame: &mut Frame, assets: &Assets, y: i32) -> Result<()> {
        for &(label, center) in &self.columns[1..] {
            frame.center_text(assets, label, center, y, 0)?;
        }
        Ok(())
    }
    fn row(
        &self,
        frame: &mut Frame,
        assets: &Assets,
        index: usize,
        y: i32,
        color: u8,
    ) -> Result<()> {
        for ((_, center), text) in self.columns.iter().zip(&self.rows[index]) {
            frame.center_text(assets, text, *center, y, color)?;
        }
        Ok(())
    }
}
fn cargo_label(state: &Campaign, index: usize) -> Result<String> {
    let cargo = state.data[0x1117 + index * 12];
    if cargo == 255 {
        return state.string(0x139e);
    }
    if index < 3 {
        return state.legion_name(usize::from(cargo));
    }
    // Eleven roaming armies are followed by the three permanent fleets' armies.
    // Their cargo IDs 11..=13 are valid even though they cannot roam on land.
    if cargo >= 14 {
        return Err("Invalid transported enemy army".into());
    }
    let army = 0xc114 + usize::from(cargo) * 14;
    let troops = u32::from(word(&state.data, army + 2)?)
        + u32::from(word(&state.data, army + 4)?)
        + u32::from(state.data[army + 8]);
    Ok(troops.to_string())
}
pub fn local(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    ids: &[usize],
    title: Option<&str>,
    selected: Option<usize>,
) -> Result<()> {
    let table = Table::read(assets, state, ids)?;
    let prefix = if title.is_some() { 10 } else { 0 };
    frame.bevel(2, 0, 317, (ids.len() as i32 + 1) * 10 + 12 + prefix, 26);
    if let Some(title) = title {
        frame.center_text(assets, title, 160, 5, 15)?;
    }
    table.headings(frame, assets, 5 + prefix)?;
    let buttons = assets.sprites("MINIT", "INIT")?;
    if selected.is_some() {
        crate::province::decorate_menu(
            frame,
            &buttons[1],
            state,
            &(0..ids.len())
                .map(|i| {
                    let y = 15 + prefix + i as i32 * 10;
                    (8, y - 1, 311, y + 9)
                })
                .collect::<Vec<_>>(),
            selected,
            true,
        );
    }
    for number in 0..ids.len() {
        let y = 15 + prefix + number as i32 * 10;
        table.row(
            frame,
            assets,
            number,
            y,
            if selected == Some(number) { 15 } else { 0 },
        )?;
    }
    Ok(())
}
pub fn movement(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    sprites: &[crate::assets::Bitmap],
    index: usize,
) -> Result<()> {
    let table = Table::read(assets, state, &[index])?;
    frame.bevel(2, 0, 317, 31, 26);
    table.headings(frame, assets, 5)?;
    table.row(frame, assets, 0, 16, 0)?;
    for target in if sprites.is_empty() {
        Vec::new()
    } else {
        state.fleet_destinations(index)?
    } {
        let (x, y) = state.region_coordinate(target)?;
        frame.blit(&sprites[7], x, y);
    }
    Ok(())
}
/// Show the original fleet/location notice, detailed table, or both together.
pub fn inspection(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    index: usize,
    information: reforge_settings::FleetInformation,
) -> Result<()> {
    let region = state.fleet_location(index)?;
    frame.acknowledgment_popup = true;
    if information != reforge_settings::FleetInformation::Original {
        movement(frame, assets, state, &[], index)?;
    }
    if information == reforge_settings::FleetInformation::Detailed {
        return Ok(());
    }
    let notice = format!(
        "{}{}{}",
        state.string(word(&state.data, 0x1100 + index * 2)? as usize)?,
        state.string(0x58fb)?,
        state.region_name(region)?,
    );
    crate::legion::message(frame, assets, state, &notice)
}

/// Roaming armies at sea have stored troops, but no persistent ship counts.
/// Inspecting them must not create the random fleet used for a naval battle.
pub fn region_inspection(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    region: u8,
    information: reforge_settings::FleetInformation,
) -> Result<()> {
    let notice = state.region_information(region)?;
    if information != reforge_settings::FleetInformation::Original
        && (27..37).contains(&region)
        && let Some(army) = state.roaming_army(region, 0)
    {
        let at = 0xc114 + army * 14;
        let origin = word(&state.data, at)?;
        // region_information has already validated the original province ID.
        let name = format!(
            "{}{}",
            state.string(0x58e3)?.replace('\n', " "),
            state.province_name(origin as u8)?,
        );
        frame.acknowledgment_popup = true;
        frame.bevel(2, 0, 317, 31, 26);
        for (label, center, count) in [
            ("Infantry", 185, word(&state.data, at + 2)?),
            ("Cavalry", 235, word(&state.data, at + 4)?),
            ("Elephants", 289, u16::from(state.data[at + 8])),
        ] {
            frame.center_text(assets, label, center, 5, 0)?;
            frame.center_text(assets, &count.to_string(), center, 16, 0)?;
        }
        frame.center_text(assets, &name, 84, 16, 0)?;
        if information == reforge_settings::FleetInformation::Detailed {
            return Ok(());
        }
    }
    crate::legion::message(frame, assets, state, &notice)
}

pub fn complete_list(frame: &mut Frame, assets: &Assets, state: &Campaign) -> Result<()> {
    let ids = state.fleets();
    if ids.is_empty() {
        return Ok(());
    }
    let table = Table::read(assets, state, &ids)?;
    frame.acknowledgment_popup = true;
    let top = (200 - ((ids.len() as i32 + 3) * 8 + 15)) / 2;
    let bottom = top + (ids.len() as i32 + 3) * 8 + 15;
    frame.bevel(2, top, 317, bottom, 26);
    frame.center_text(
        assets,
        &format!("{}{}", state.string(0x13a3)?, state.continue_footer()?),
        160,
        top + 5,
        15,
    )?;
    table.headings(frame, assets, top + 22)?;
    for number in 0..ids.len() {
        table.row(frame, assets, number, top + 32 + number as i32 * 8, 0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER_WIDTHS: [i32; 6] = [0, 41, 60, 34, 27, 28];

    #[test]
    fn ordinary_names_keep_defaults_and_viii_gets_earlier_clearance() {
        // Authored text advances model short, medium and wide fleet-table cells.
        let mut widths = vec![
            HEADER_WIDTHS,
            [29, 11, 11, 10, 37, 25],
            [33, 5, 5, 6, 37, 29],
            [37, 5, 5, 5, 37, 35],
        ];
        assert_eq!(fit_roman_columns(&widths), [41, 100, 159, 214, 252, 292]);
        for row in &mut widths[1..] {
            row[4] = 45;
        }
        assert_eq!(fit_roman_columns(&widths), [41, 100, 159, 210, 248, 292]);
    }

    #[test]
    fn fitting_uses_actual_status_pairs_and_shifts_more_for_wider_counts() {
        let long_forming = [
            HEADER_WIDTHS,
            [29, 11, 11, 10, 63, 25],
            [33, 5, 5, 6, 63, 29],
            [37, 5, 5, 5, 63, 35],
        ];
        assert_eq!(
            fit_roman_columns(&long_forming),
            [41, 89, 144, 195, 239, 292]
        );
        let long_ready = [
            HEADER_WIDTHS,
            [29, 18, 18, 18, 63, 25],
            [33, 5, 10, 6, 37, 29],
            [37, 5, 5, 5, 21, 35],
        ];
        assert_eq!(fit_roman_columns(&long_ready), [41, 90, 145, 196, 244, 292]);
        let mut more_galleons = long_forming;
        for row in &mut more_galleons[1..] {
            row[3] = 18;
        }
        assert_eq!(
            fit_roman_columns(&more_galleons),
            [41, 85, 140, 191, 239, 292]
        );
    }

    #[test]
    fn legal_cell_widths_keep_frame_margins_and_intercolumn_clearance() {
        for legion in 21..=63 {
            for status in [25, 29, 35] {
                for a in [5, 11, 15, 18, 30] {
                    for b in [5, 11, 15, 18, 30] {
                        for c in [5, 11, 15, 18, 30] {
                            let lines = [HEADER_WIDTHS, [37, a, b, c, legion, status]];
                            let centers = fit_roman_columns(&lines);
                            assert_eq!((centers[0], centers[5]), (41, 292));
                            for line in lines {
                                for i in 0..6 {
                                    if line[i] == 0 {
                                        continue;
                                    }
                                    let left = centers[i] - line[i] / 2;
                                    assert!(left >= 6);
                                    assert!(centers[i] + (line[i] + 1) / 2 <= 315);
                                    if i != 0 && line[i - 1] != 0 {
                                        let previous_right = centers[i - 1] + (line[i - 1] + 1) / 2;
                                        assert!(left - previous_right >= ROMAN_GAPS[i - 1]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cargo_reads_full_roman_names_and_enemy_troops_from_separate_tables() {
        let mut ds = vec![0; 65536];
        let put = |data: &mut [u8], at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        ds[0xd000] = b'I';
        for address in [0x4f82, 0x4f6e] {
            put(&mut ds, address, 0xd010);
        }
        put(&mut ds, 0x4f5c, 0xd000);
        for (index, suffix) in [" Ital", " Aeg", " Narb"].iter().enumerate() {
            let legion = 0x1918 + index * 20;
            ds[legion..legion + suffix.len()].copy_from_slice(suffix.as_bytes());
            ds[legion + 10] = 1;
            ds[0x1117 + index * 12] = index as u8;
        }
        ds[0x1117 + 3 * 12] = 10;
        let army = 0xc114 + 10 * 14;
        put(&mut ds, army + 2, 4200);
        put(&mut ds, army + 4, 790);
        ds[army + 8] = 10;
        ds[0x139e..0x13a2].copy_from_slice(b"None");
        ds[0x1117 + 4 * 12] = 255;
        let mut state = Campaign::test_data(ds);
        for (index, name) in ["I Ital", "I Aeg", "I Narb"].iter().enumerate() {
            assert_eq!(cargo_label(&state, index).unwrap(), *name);
        }
        assert_eq!(cargo_label(&state, 3).unwrap(), "5000");
        assert_eq!(cargo_label(&state, 4).unwrap(), "None");
        for (fleet, army) in [(3, 11), (4, 12), (5, 13)] {
            state.data[0x1117 + fleet * 12] = army as u8;
            let base = 0xc114 + army * 14;
            put(&mut state.data, base + 2, 4200);
            put(&mut state.data, base + 4, 790);
            state.data[base + 8] = 10;
            assert_eq!(cargo_label(&state, fleet).unwrap(), "5000");
        }
        state.data[0x1117 + 3 * 12] = 14;
        assert!(cargo_label(&state, 3).is_err());
        state.data[0x1117] = 12;
        assert!(cargo_label(&state, 0).is_err());
    }

    #[test]
    fn inspection_modes_compose_without_changing_campaign_state() {
        use reforge_settings::FleetInformation::{Both, Detailed, Original};
        let mut ds = vec![0; 65536];
        let put = |data: &mut [u8], at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        // Authored three-pixel glyphs make text differences visible without
        // requiring original font resources, banks or game assets.
        for (offset, value) in [(0xa05c, 0xe000), (0xa05e, 0xe100), (0xa060, 0xe200)] {
            put(&mut ds, offset, value);
        }
        ds[0xa062] = 1;
        ds[0xa064] = 1;
        ds[0xe000..0xe060].fill(3);
        for glyph in 0..96 {
            ds[0xe200 + glyph] = (glyph as u8).wrapping_mul(29);
        }
        let assets = Assets::test_data(ds.clone());
        for (at, text) in [
            (0xd000, "Enemy fleet"),
            (0xd040, "Western sea"),
            (0x58fb, " in "),
            (0x1314, "Ships"),
            (0x139e, "None"),
            (0x3e16, "Continue"),
            (0x3e2e, "Click"),
            (0xd080, "Ready"),
        ] {
            ds[at..at + text.len()].copy_from_slice(text.as_bytes());
        }
        put(&mut ds, 0x1100 + 3 * 2, 0xd000);
        put(&mut ds, 0x5222 + 32 * 2, 0xd040);
        put(&mut ds, 0x1b54, 0xd080);
        let fleet = 0x110e + 3 * 12;
        put(&mut ds, fleet, 1);
        ds[fleet + 6] = 32;
        ds[fleet + 8] = 0;
        ds[fleet + 9] = 255;
        let mut state = Campaign::test_data(ds);
        let before = state.data.clone();
        let draw = |state: &Campaign, mode| {
            let mut frame = Frame::new();
            frame.pixels.fill(7);
            inspection(&mut frame, &assets, state, 3, mode).unwrap();
            assert!(frame.acknowledgment_popup);
            frame.pixels
        };
        let original = draw(&state, Original);
        let detailed = draw(&state, Detailed);
        let both = draw(&state, Both);
        assert_eq!(both[..40 * 320], detailed[..40 * 320]);
        assert_eq!(both[40 * 320..], original[40 * 320..]);
        assert!(original[..40 * 320].iter().all(|pixel| *pixel == 7));
        assert!(detailed[60 * 320..].iter().all(|pixel| *pixel == 7));
        assert!(original[60 * 320..].iter().any(|pixel| *pixel != 7));
        assert!(detailed[..40 * 320].iter().any(|pixel| *pixel != 7));
        assert_eq!(
            state.data, before,
            "all views leave campaign state unchanged"
        );
        put(&mut state.data, fleet, 99);
        assert_eq!(
            draw(&state, Original),
            original,
            "DOS notice exposes no ship count"
        );
        assert_ne!(draw(&state, Both), both);
        assert_ne!(
            draw(&state, Detailed),
            detailed,
            "detailed table shows ship count changes"
        );
    }

    #[test]
    fn sea_army_inspection_composes_stored_troops_without_creating_a_fleet() {
        use reforge_settings::FleetInformation::{Both, Detailed, Original};
        let mut ds = vec![0; 65536];
        let put = |data: &mut [u8], at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        for (offset, value) in [(0xa05c, 0xe000), (0xa05e, 0xe100), (0xa060, 0xe200)] {
            put(&mut ds, offset, value);
        }
        ds[0xa062] = 1;
        ds[0xa064] = 1;
        ds[0xe000..0xe060].fill(3);
        for glyph in 0..96 {
            ds[0xe200 + glyph] = (glyph as u8).wrapping_mul(29);
        }
        let assets = Assets::test_data(ds.clone());
        for (at, text) in [
            (0xd000, "Mauretania"),
            (0xd040, "Mare Nostrum"),
            (0x58e3, "Marauding Army\nof "),
            (0x58f6, "\nin "),
            (0x3e16, "Hit any key"),
            (0x3e2e, "Click"),
        ] {
            ds[at..at + text.len()].copy_from_slice(text.as_bytes());
        }
        put(&mut ds, 0x5222 + 16 * 2, 0xd000);
        put(&mut ds, 0x5222 + 30 * 2, 0xd040);
        let mut state = Campaign::test_data(ds);
        let draw = |state: &Campaign, region, mode| {
            let mut frame = Frame::new();
            frame.pixels.fill(7);
            region_inspection(&mut frame, &assets, state, region, mode).unwrap();
            assert!(frame.acknowledgment_popup);
            frame.pixels
        };
        // Empty seas and land keep their original notices under every setting.
        for region in [16, 30] {
            assert_eq!(draw(&state, region, Original), draw(&state, region, Both));
            assert_eq!(
                draw(&state, region, Original),
                draw(&state, region, Detailed)
            );
        }
        // Exercise both ends of the roaming table, including explicit zeros and
        // full-width fields. Inspection changes no bytes, even unsaved scratch.
        for army in [0, 10] {
            let at = 0xc114 + army * 14;
            state.data[at - 1] = 30;
            put(&mut state.data, at, 16);
            put(&mut state.data, at + 2, 3200);
            put(&mut state.data, at + 6, 1);
            let before = state.data.clone();
            let original = draw(&state, 30, Original);
            let detailed = draw(&state, 30, Detailed);
            let both = draw(&state, 30, Both);
            assert_eq!(both[..32 * 320], detailed[..32 * 320]);
            assert_eq!(both[32 * 320..], original[32 * 320..]);
            assert!(original[..32 * 320].iter().all(|p| *p == 7));
            assert!(detailed[32 * 320..].iter().all(|p| *p == 7));
            assert_ne!(detailed, original);
            assert_eq!(state.data, before);
            put(&mut state.data, at + 2, u16::MAX);
            put(&mut state.data, at + 4, u16::MAX);
            state.data[at + 8] = u8::MAX;
            assert_eq!(draw(&state, 30, Original), original);
            assert_ne!(draw(&state, 30, Detailed), detailed);
            assert_ne!(draw(&state, 30, Both), both);
            state.data[at - 1] = 16;
            assert_eq!(draw(&state, 16, Original), draw(&state, 16, Detailed));
            put(&mut state.data, at + 6, 0);
        }
    }

    #[test]
    fn sailing_moves_cargo_without_spending_the_legions_own_action() {
        let mut ds = vec![0; 65536];
        ds[0x1114] = 32;
        ds[0x1117] = 2;
        ds[0x194c] = 32;
        ds[0x194e] = 0;
        ds[0x50ec + 32 * 8..0x50f4 + 32 * 8].copy_from_slice(&[0, 21, 22, 30, 31, 33, 255, 255]);
        let mut state = Campaign::test_data(ds);
        let before = state.data.clone();
        state.complete_fleet_move(0, 31).unwrap();
        let changed: Vec<_> = before
            .iter()
            .zip(&state.data)
            .enumerate()
            .filter_map(|(i, (a, b))| (a != b).then_some(i))
            .collect();
        assert_eq!(changed, [0x1114, 0x1116, 0x194c]);
        let after = state.data.clone();
        assert!(state.complete_fleet_move(0, 30).is_err());
        assert_eq!(state.data, after);
    }
}
