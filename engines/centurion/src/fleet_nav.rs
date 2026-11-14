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
fn width(assets: &Assets, text: &str) -> Result<i32> {
    let at = word(&assets.data_segment, 0xa05c)? as usize;
    Ok(text
        .bytes()
        .filter(|b| (32..128).contains(b))
        .map(|b| i32::from(assets.data_segment[at + usize::from(b - 32)]) + 1)
        .sum())
}
pub fn row(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    index: usize,
    y: i32,
) -> Result<()> {
    frame.text(
        assets,
        &state.string(word(&state.data, 0x1100 + index * 2)? as usize)?,
        25,
        y,
        0,
    )?;
    for (kind, x) in [(0, 104), (1, 162), (2, 216)] {
        let count = word(&state.data, 0x110e + index * 12 + kind * 2)?.to_string();
        frame.text(assets, &count, x - width(assets, &count)?, y, 0)?;
    }
    let cargo = state.data[0x1117 + index * 12];
    let legion = if cargo == 255 {
        state.string(0x139e)?
    } else {
        state.roman(state.data[0x1922 + usize::from(cargo) * 20])?
    };
    frame.center_text(assets, &legion, 256, y, 0)?;
    frame.center_text(
        assets,
        &state.string(word(
            &state.data,
            0x1b54 + usize::from(state.data[0x1116 + index * 12]) * 2,
        )? as usize)?,
        295,
        y,
        0,
    )
}
pub fn local(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    ids: &[usize],
    title: Option<&str>,
    selected: Option<usize>,
) -> Result<()> {
    let prefix = if title.is_some() { 10 } else { 0 };
    frame.bevel(2, 0, 317, (ids.len() as i32 + 1) * 10 + 12 + prefix, 26);
    if let Some(title) = title {
        frame.center_text(assets, title, 160, 5, 15)?;
    }
    frame.text(assets, &state.string(0x1314)?, 80, 5 + prefix, 0)?;
    let buttons = assets.sprites("MINIT", "INIT")?;
    for (number, &id) in ids.iter().enumerate() {
        let y = 15 + prefix + number as i32 * 10;
        row(frame, assets, state, id, y)?;
        if let Some(selected) = selected {
            frame.blit(&buttons[1], 8, y);
            if selected == number {
                frame.rect(10, y + 2, 5, 3, 8);
            }
        }
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
    frame.bevel(2, 0, 317, 31, 26);
    frame.text(assets, &state.string(0x1314)?, 80, 5, 0)?;
    row(frame, assets, state, index, 16)?;
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
/// Temporary comparison view: retain the native table and add the DOS notice.
pub fn inspection(
    frame: &mut Frame,
    assets: &Assets,
    state: &Campaign,
    index: usize,
) -> Result<()> {
    let region = state.fleet_location(index)?;
    movement(frame, assets, state, &[], index)?;
    let notice = format!(
        "{}{}{}",
        state.string(word(&state.data, 0x1100 + index * 2)? as usize)?,
        state.string(0x58fb)?,
        state.region_name(region)?,
    );
    crate::legion::message(frame, assets, state, &notice)
}

pub fn complete_list(frame: &mut Frame, assets: &Assets, state: &Campaign) -> Result<()> {
    let ids = state.fleets();
    if ids.is_empty() {
        return Ok(());
    }
    let top = (200 - ((ids.len() as i32 + 3) * 8 + 15)) / 2;
    let bottom = top + (ids.len() as i32 + 3) * 8 + 15;
    frame.bevel(2, top, 317, bottom, 26);
    frame.center_text(
        assets,
        &format!(
            "{}{}",
            state.string(0x13a3)?,
            state.string(word(&state.data, 0x3e40)? as usize)?
        ),
        160,
        top + 5,
        15,
    )?;
    frame.text(assets, &state.string(0x1314)?, 80, top + 22, 0)?;
    for (number, &id) in ids.iter().enumerate() {
        row(frame, assets, state, id, top + 32 + number as i32 * 8)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
