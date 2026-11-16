// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Transitional private state model: only recover field meanings with evidence.
// Save blocks are read from the original table used by 0000:75a3 / 75e8.
use crate::assets::{Assets, Result, word};

/// Presentation-only provenance of the last deliberate player input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContinueInput {
    Keyboard,
    #[default]
    Mouse,
}

#[derive(Clone)]
pub struct Campaign {
    pub data: Vec<u8>,
    pub sound: crate::sound::Bus,
    pub continue_input: ContinueInput,
    blocks: Vec<(usize, usize)>,
}

impl Campaign {
    #[cfg(test)]
    pub(crate) fn test_data(data: Vec<u8>) -> Self {
        Self {
            data,
            blocks: Vec::new(),
            sound: crate::sound::Bus::default(),
            continue_input: ContinueInput::default(),
        }
    }

    pub fn new(assets: &Assets) -> Result<Self> {
        let mut data = assets.data_segment.clone();
        data.resize(65536, 0);
        // VGA startup at overlay 1:B226 installs these drawing colours.
        // They are not part of the save image.
        for (offset, value) in [(0x6344, 0x74u16), (0x6346, 0x1a), (0x6348, 0x10c)] {
            data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut blocks = Vec::new();
        for offset in (0x5bde..0x5c42).step_by(4) {
            let address = word(&data, offset)? as usize;
            let size = word(&data, offset + 2)? as usize;
            if address + size > data.len() {
                return Err("Save block outside DS".into());
            }
            if size != 0 {
                blocks.push((address, size));
            }
        }
        if blocks.iter().map(|(_, size)| size).sum::<usize>() != 1780 {
            return Err("Unexpected save block table".into());
        }
        let mut state = Self {
            data,
            blocks,
            sound: crate::sound::Bus::default(),
            continue_input: ContinueInput::default(),
        };
        state.load(&assets.resource("CENT", "NEWGAME.LZW")?)?;
        Ok(state)
    }

    pub fn load(&mut self, save: &[u8]) -> Result<()> {
        if save.len() != 1780 || save.get(..4) != Some(b"10A\0") {
            return Err("Expected a 1780-byte DOS 10A save".into());
        }
        let mut cursor = 0;
        for &(address, size) in &self.blocks {
            self.data[address..address + size].copy_from_slice(&save[cursor..cursor + size]);
            cursor += size;
        }
        Ok(())
    }

    pub fn save(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1780);
        for &(address, size) in &self.blocks {
            bytes.extend_from_slice(&self.data[address..address + size]);
        }
        bytes
    }

    pub fn continue_footer(&self) -> Result<String> {
        self.string(match self.continue_input {
            ContinueInput::Keyboard => 0x3e16,
            ContinueInput::Mouse => 0x3e2e,
        })
    }

    pub fn string(&self, offset: usize) -> Result<String> {
        let bytes = self.data.get(offset..).ok_or("String outside state")?;
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or("Unterminated state string")?;
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    pub fn ownership(&self, province: u8) -> Result<u8> {
        if province >= 27 {
            return Err("Not a land province".into());
        }
        Ok(self.data[0x5285 + province as usize * 32])
    }

    pub fn owned_provinces(&self) -> Vec<u8> {
        self.data[0x5718..0x5733]
            .iter()
            .copied()
            .filter(|province| *province < 27 && self.data[0x5285 + *province as usize * 32] != 0)
            .collect()
    }

    pub fn morale(&self, province: u8) -> Result<usize> {
        self.ownership(province)?;
        let base = province as usize * 32;
        let value = i16::from(self.data[0x527d + base] as i8)
            + i16::from(self.data[0x5280 + base] as i8)
            - i16::from(self.data[0x527e + base] as i8);
        Ok((value / 25).clamp(0, 3) as usize)
    }

    pub fn tribute(&self, province: u8) -> Result<u8> {
        self.ownership(province)?;
        Ok(self.data[0x5282 + province as usize * 32])
    }

    pub fn set_tribute(&mut self, province: u8, level: u8) -> Result<()> {
        if self.ownership(province)? == 0 || level > 4 {
            return Err("Invalid tribute selection".into());
        }
        self.data[0x5282 + province as usize * 32] = level;
        Ok(())
    }

    pub fn set_tribute_policy(&mut self, level: u8) -> Result<()> {
        if level > 4 {
            return Err("Invalid tribute policy".into());
        }
        self.data[0x5fc0] = level;
        for province in 0..27 {
            if self.ownership(province)? != 0 {
                self.set_tribute(province, level)?;
            }
        }
        Ok(())
    }

    pub fn province_at(&self, x: i32, y: i32) -> Result<Option<u8>> {
        Ok(self.region_at(x, y)?.filter(|region| *region < 27))
    }

    pub fn region_at(&self, x: i32, y: i32) -> Result<Option<u8>> {
        if !(0..320).contains(&x) || !(0..200).contains(&y) {
            return Ok(None);
        }
        let mut cursor = word(&self.data, 0x3a96 + y as usize * 2)? as usize;
        let mut edge = 0;
        while edge < 319 {
            let region = *self.data.get(cursor).ok_or("Map mask outside DS")?;
            let run = *self.data.get(cursor + 1).ok_or("Map mask outside DS")? as i32;
            if run == 0 {
                return Err("Zero-length map mask run".into());
            }
            edge += run;
            if x < edge {
                return Ok((region < 37).then_some(region));
            }
            cursor += 2;
        }
        Ok(None)
    }

    pub fn region_name(&self, region: u8) -> Result<String> {
        if region >= 37 {
            return Err("Region outside campaign map".into());
        }
        self.string(word(&self.data, 0x5222 + usize::from(region) * 2)? as usize)
    }
    /// Resident 720B: the first roaming army, if present, precedes the place name.
    pub fn region_information(&self, region: u8) -> Result<String> {
        let name = self.region_name(region)?;
        if let Some(army) = self.roaming_army(region, 0) {
            let origin = word(&self.data, 0xc114 + army * 14)?;
            if origin >= 27 {
                return Err("Roaming army origin outside province table".into());
            }
            return Ok(format!(
                "{}{}{}{}",
                self.string(0x58e3)?,
                self.province_name(origin as u8)?,
                self.string(0x58f6)?,
                name,
            ));
        }
        Ok(name)
    }
    pub fn province_name(&self, province: u8) -> Result<String> {
        if province >= 27 {
            return Err("Not a land province".into());
        }
        self.string(word(&self.data, 0x5222 + province as usize * 2)? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_prompt_tracks_transient_input_without_changing_campaign_bytes() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        for (address, text) in [
            (0x3e16, "Hit any key to continue"),
            (0x3e2e, "Click to continue"),
        ] {
            state.data[address..address + text.len()].copy_from_slice(text.as_bytes());
        }
        state.blocks = vec![(0, 16), (0x3e16, 48)];
        let bytes = state.data.clone();
        let saved = state.save();
        assert_eq!(state.continue_footer().unwrap(), "Click to continue");
        state.continue_input = ContinueInput::Keyboard;
        assert_eq!(state.continue_footer().unwrap(), "Hit any key to continue");
        let copy = state.clone();
        assert_eq!(copy.continue_input, ContinueInput::Keyboard);
        assert_eq!(state.save(), saved);
        assert_eq!(state.data, bytes);
    }

    #[test]
    fn region_information_names_the_first_local_roaming_army_without_mutation() {
        let mut data = vec![0; 65536];
        for (address, text) in [
            (0xd000, "Mauretania"),
            (0xd020, "Numidia"),
            (0xd040, "Mare Nostrum"),
            (0x58e3, "Army from\n"),
            (0x58f6, "\nin "),
        ] {
            data[address..address + text.len()].copy_from_slice(text.as_bytes());
        }
        data[0x5222 + 2 * 2..0x5224 + 2 * 2].copy_from_slice(&0xd000_u16.to_le_bytes());
        data[0x5222 + 3 * 2..0x5224 + 3 * 2].copy_from_slice(&0xd020_u16.to_le_bytes());
        data[0x5222 + 30 * 2..0x5224 + 30 * 2].copy_from_slice(&0xd040_u16.to_le_bytes());
        let mut state = Campaign::test_data(data);
        assert_eq!(state.region_information(30).unwrap(), "Mare Nostrum");
        assert!(state.region_information(37).is_err());
        assert_eq!(state.region_information(2).unwrap(), "Mauretania");
        state.data[0xc113] = 2;
        state.data[0xc114..0xc116].copy_from_slice(&3_u16.to_le_bytes());
        state.data[0xc11a..0xc11c].copy_from_slice(&1_u16.to_le_bytes());
        let before = state.data.clone();
        assert_eq!(
            state.region_information(2).unwrap(),
            "Army from\nNumidia\nin Mauretania"
        );
        assert_eq!(state.data, before);
        state.data[0xc113] = 30;
        assert_eq!(
            state.region_information(30).unwrap(),
            "Army from\nNumidia\nin Mare Nostrum"
        );
    }

    fn synthetic() -> Campaign {
        let mut state = Campaign {
            data: vec![0; 65536],
            blocks: Vec::new(),
            sound: crate::sound::Bus::default(),
            continue_input: ContinueInput::default(),
        };
        for province in 0..27 {
            state.data[0x5282 + province * 32] = 1;
        }
        for (province, status) in [(0, 4), (2, 1), (7, 2), (8, 3)] {
            state.data[0x5285 + province * 32] = status;
        }
        state.data[0x5fc0] = 1;
        state
    }

    #[test]
    fn tribute_policy_updates_all_owned_statuses_and_leaves_foreign_provinces_alone() {
        let mut state = synthetic();
        let before = state.data.clone();
        state.set_tribute_policy(3).unwrap();
        let changed: Vec<_> = before
            .iter()
            .zip(&state.data)
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        assert_eq!(changed, [0x5282, 0x52c2, 0x5362, 0x5382, 0x5fc0]);
        assert_eq!(state.tribute(1).unwrap(), 1);
        let unchanged = state.data.clone();
        assert!(state.set_tribute_policy(5).is_err());
        assert!(state.set_tribute(1, 2).is_err());
        assert!(state.set_tribute(27, 2).is_err());
        assert_eq!(state.data, unchanged);
        state.set_tribute(2, 0).unwrap();
        assert_eq!(state.tribute(2).unwrap(), 0);
        assert_eq!(state.data[0x5fc0], 3);
    }

    #[test]
    fn province_morale_uses_signed_bytes_truncation_and_clamping() {
        let mut state = synthetic();
        for (loyalty, adjustment, tax, expected) in [
            (99, 0, 0, 3),
            (24, 0, 0, 0),
            (25, 0, 0, 1),
            (50, 0, 0, 2),
            (75, 0, 0, 3),
            (20, -30, 10, 0),
            (127, 127, -128, 3),
        ] {
            state.data[0x527d] = loyalty as u8;
            state.data[0x5280] = adjustment as u8;
            state.data[0x527e] = tax as u8;
            assert_eq!(state.morale(0).unwrap(), expected);
        }
    }
}
