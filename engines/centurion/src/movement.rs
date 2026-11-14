// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Campaign-map legion destinations and the unopposed friendly march path.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

#[derive(Debug, PartialEq, Eq)]
pub enum March {
    Ignored,
    Cancelled,
    Moved,
    EncounterPending,
    EmbarkPending,
}

impl Campaign {
    pub fn legion_location(&self, index: usize) -> Result<u8> {
        if index >= 12 || self.data[0x1925 + index * 20] == 255 {
            return Err("No active legion at this index".into());
        }
        let region = self.data[0x1924 + index * 20];
        if region >= 37 {
            return Err("Legion location outside campaign map".into());
        }
        Ok(region)
    }

    pub fn region_coordinate(&self, region: u8) -> Result<(i32, i32)> {
        if region >= 37 {
            return Err("Region outside campaign map".into());
        }
        let at = 0x1ab8 + usize::from(region) * 4;
        Ok((
            i32::from(word(&self.data, at)?),
            i32::from(word(&self.data, at + 2)?),
        ))
    }

    // Sea destinations require an unoccupied fleet, as in 382A..38E4.
    pub fn legion_destinations(&self, index: usize) -> Result<Vec<(u8, bool)>> {
        let source = self.legion_location(index)?;
        let mut destinations = Vec::new();
        for &region in
            &self.data[0x50ec + usize::from(source) * 8..0x50f4 + usize::from(source) * 8]
        {
            if region < 27 {
                destinations.push((region, false));
            } else if source < 27
                && region < 37
                && (0..3).any(|fleet| {
                    let at = 0x1114 + fleet * 12;
                    self.data[at] == region && self.data[at + 2] != 255 && self.data[at + 3] == 255
                })
            {
                destinations.push((region, true));
            }
        }
        Ok(destinations)
    }

    pub fn has_roaming_army(&self, region: u8) -> bool {
        self.roaming_army(region, 0).is_some()
    }

    pub fn roaming_army(&self, region: u8, start: usize) -> Option<usize> {
        (start..11).find(|army| {
            let at = 0xc113 + army * 14;
            self.data[at] == region && word(&self.data, at + 7).unwrap_or(0) != 0
        })
    }

    pub fn march_legion(&mut self, index: usize, destination: u8) -> Result<March> {
        let source = self.legion_location(index)?;
        if source == destination {
            return Ok(March::Cancelled);
        }
        if !self
            .legion_destinations(index)?
            .iter()
            .any(|&(region, _)| region == destination)
        {
            return Ok(March::Ignored);
        }
        if self.data[0x1926 + index * 20] != 0 {
            return Err("Legion is not ready to march".into());
        }
        if destination >= 27 {
            return Ok(March::EmbarkPending);
        }
        // 4B3F checks roaming armies before the province ownership branch.
        // The encounter coordinator spends readiness and resolves every army
        // before deciding whether to negotiate, fight a garrison or complete.
        if self.ownership(destination)? == 0 || self.has_roaming_army(destination) {
            return Ok(March::EncounterPending);
        }
        self.complete_friendly_march(index, destination)?;
        Ok(March::Moved)
    }

    pub(crate) fn complete_friendly_march(&mut self, index: usize, destination: u8) -> Result<()> {
        let source = self.legion_location(index)?;
        if destination >= 27 || self.ownership(destination)? == 0 {
            return Err("No friendly land destination".into());
        }
        let at = 0x1918 + index * 20;
        if source >= 27 {
            let fleet = self.data[at + 17];
            if fleet >= 3 {
                return Err("Embarked legion has no valid fleet".into());
            }
            self.data[0x1117 + usize::from(fleet) * 12] = 255;
            self.data[at + 17] = 255;
        }
        self.data[at + 12] = destination;
        self.data[at + 14] = 1;
        Ok(())
    }
}

pub fn render(
    assets: &Assets,
    state: &Campaign,
    frame: &mut Frame,
    sprites: &[Bitmap],
    index: usize,
) -> Result<()> {
    // 3EA3 draws the selected legion at the top of the otherwise normal map.
    frame.bevel(2, 0, 317, 31, 26);
    frame.text(assets, &state.string(0x1efa)?, 115, 5, 0)?;
    crate::legion::row(frame, assets, state, index, 16, true)?;
    for (region, sea) in state.legion_destinations(index)? {
        let (mut x, mut y) = state.region_coordinate(region)?;
        if sea {
            x -= 3;
            y -= 4;
        }
        frame.blit(&sprites[3], x, y);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn campaign() -> Campaign {
        let mut ds = vec![0; 65536];
        ds[0x50ec..0x50f4].copy_from_slice(&[11, 31, 255, 255, 255, 255, 255, 255]);
        ds[0x1929] = 255;
        ds[0x5285 + 11 * 32] = 1;
        for fleet in 0..3 {
            ds[0x1116 + fleet * 12] = 255;
        }
        Campaign::test_data(ds)
    }

    #[test]
    fn march_changes_only_location_and_readiness() {
        let mut state = campaign();
        let before = state.data.clone();
        assert_eq!(state.march_legion(0, 11).unwrap(), March::Moved);
        let differences: Vec<_> = before
            .iter()
            .zip(&state.data)
            .enumerate()
            .filter_map(|(i, (a, b))| (a != b).then_some(i))
            .collect();
        assert_eq!(differences, [0x1924, 0x1926]);
        assert_eq!(state.data[0x1924], 11);
        assert_eq!(state.data[0x1926], 1);
    }

    #[test]
    fn cancellation_invalid_target_and_pending_encounters_preserve_state() {
        let mut state = campaign();
        let before = state.data.clone();
        assert_eq!(state.march_legion(0, 0).unwrap(), March::Cancelled);
        assert_eq!(state.march_legion(0, 4).unwrap(), March::Ignored);
        assert_eq!(state.march_legion(0, 31).unwrap(), March::Ignored);
        assert_eq!(state.data, before);
        state.data[0x5285 + 11 * 32] = 0;
        let before = state.data.clone();
        assert_eq!(state.march_legion(0, 11).unwrap(), March::EncounterPending);
        assert_eq!(state.data, before);
        state.data[0x5285 + 11 * 32] = 2;
        state.data[0xc113] = 11;
        state.data[0xc11a] = 1;
        let before = state.data.clone();
        assert_eq!(state.march_legion(0, 11).unwrap(), March::EncounterPending);
        assert_eq!(state.data, before);
    }

    #[test]
    fn sea_marker_requires_available_empty_fleet() {
        let mut state = campaign();
        assert_eq!(state.legion_destinations(0).unwrap(), [(11, false)]);
        state.data[0x1114] = 31;
        state.data[0x1116] = 0;
        state.data[0x1117] = 255;
        assert_eq!(
            state.legion_destinations(0).unwrap(),
            [(11, false), (31, true)]
        );
        let before = state.data.clone();
        assert_eq!(state.march_legion(0, 31).unwrap(), March::EmbarkPending);
        assert_eq!(state.data, before);
        state.data[0x1117] = 1;
        assert_eq!(state.legion_destinations(0).unwrap(), [(11, false)]);
    }

    #[test]
    fn friendly_landing_releases_the_transport_without_moving_it() {
        let mut state = campaign();
        state.data[0x1924] = 31;
        state.data[0x1929] = 0;
        state.data[0x1114] = 31;
        state.data[0x1116] = 0;
        state.data[0x1117] = 0;
        state.data[0x50ec + 31 * 8..0x50f4 + 31 * 8].fill(255);
        state.data[0x50ec + 31 * 8] = 11;
        let mut expected = state.data.clone();
        expected[0x1924] = 11;
        expected[0x1926] = 1;
        expected[0x1929] = 255;
        expected[0x1117] = 255;
        assert_eq!(state.march_legion(0, 11).unwrap(), March::Moved);
        assert_eq!(state.data, expected);
    }
}
