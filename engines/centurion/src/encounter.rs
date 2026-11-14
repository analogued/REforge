// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! A player's march: ordered roaming-army interceptions, then the province.
use crate::assets::{Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Warn(Option<usize>),
    Fight(Option<usize>),
    Friendly,
    Negotiate,
    Defeated,
}

pub struct Encounter {
    pub legion: usize,
    pub province: u8,
    pub background: Frame,
    pub battlefield_background: Frame,
    next_army: usize,
    pending: Option<Option<usize>>,
    successful: bool,
}

impl Encounter {
    pub fn new(
        legion: usize,
        province: u8,
        background: Frame,
        battlefield_background: Frame,
    ) -> Self {
        Self {
            legion,
            province,
            background,
            battlefield_background,
            next_army: 0,
            pending: None,
            successful: true,
        }
    }

    pub fn next(&mut self, state: &Campaign) -> Result<Action> {
        if let Some(army) = self.pending.take() {
            return Ok(Action::Fight(army));
        }
        if let Some(army) = state.roaming_army(self.province, self.next_army) {
            self.next_army = army + 1;
            self.pending = Some(Some(army));
            return Ok(Action::Warn(Some(army)));
        }
        if !self.successful {
            return Ok(Action::Defeated);
        }
        if state.ownership(self.province)? != 0 {
            return Ok(Action::Friendly);
        }
        if state.data[0x5287 + usize::from(self.province) * 32] != 0 {
            self.pending = Some(None);
            return Ok(Action::Warn(None));
        }
        Ok(Action::Negotiate)
    }

    pub fn battle_returned(&mut self, state: &mut Campaign, army: usize, victory: bool) {
        // 4B88 keeps scanning even after a loss/cancellation. A later win cannot
        // undo the failed march; victory deactivates this army, not its nation.
        if victory {
            state.data[0xc11a + army * 14..0xc11c + army * 14].fill(0);
        } else {
            self.successful = false;
        }
    }

    pub fn notice(&self, state: &Campaign, army: Option<usize>) -> Result<String> {
        let legion = state.legion_name(self.legion)?;
        let destination = state.province_name(self.province)?;
        if let Some(army) = army {
            let origin = word(&state.data, 0xc114 + army * 14)?;
            if origin >= 27 {
                return Err("Roaming army origin outside province table".into());
            }
            Ok(format!(
                "{}{}{}{}{}{}{}{}",
                state.string(word(&state.data, 0x3d7a)? as usize)?,
                state.province_name(origin as u8)?,
                state.string(word(&state.data, 0x3d7e)? as usize)?,
                destination,
                state.string(word(&state.data, 0x3d82)? as usize)?,
                legion,
                state.string(word(&state.data, 0x3d88)? as usize)?,
                state.string(word(&state.data, 0x3d8c)? as usize)?,
            ))
        } else {
            Ok(format!(
                "{}{}{}{}",
                legion,
                state.string(word(&state.data, 0x4ece)? as usize)?,
                destination,
                state.string(word(&state.data, 0x4ed2)? as usize)?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interceptions_scan_in_slot_order_and_a_later_win_cannot_reverse_a_loss() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        for army in [1, 4] {
            state.data[0xc113 + army * 14] = 11;
            state.data[0xc11a + army * 14] = 1;
        }
        state.data[0x5285 + 11 * 32] = 1;
        let mut march = Encounter::new(0, 11, Frame::new(), Frame::new());
        assert_eq!(march.next(&state).unwrap(), Action::Warn(Some(1)));
        assert_eq!(march.next(&state).unwrap(), Action::Fight(Some(1)));
        march.battle_returned(&mut state, 1, false);
        assert_eq!(march.next(&state).unwrap(), Action::Warn(Some(4)));
        assert_eq!(march.next(&state).unwrap(), Action::Fight(Some(4)));
        march.battle_returned(&mut state, 4, true);
        assert_eq!(state.data[0xc11a + 14], 1);
        assert_eq!(state.data[0xc11a + 4 * 14], 0);
        assert_eq!(march.next(&state).unwrap(), Action::Defeated);
    }

    #[test]
    fn province_ownership_and_forced_battle_are_considered_after_all_armies() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        state.data[0x5287 + 11 * 32] = 1;
        let mut march = Encounter::new(0, 11, Frame::new(), Frame::new());
        assert_eq!(march.next(&state).unwrap(), Action::Warn(None));
        assert_eq!(march.next(&state).unwrap(), Action::Fight(None));
        state.data[0x5285 + 11 * 32] = 2;
        let mut march = Encounter::new(0, 11, Frame::new(), Frame::new());
        assert_eq!(march.next(&state).unwrap(), Action::Friendly);
        state.data[0x5285 + 11 * 32] = 0;
        state.data[0x5287 + 11 * 32] = 0;
        assert_eq!(march.next(&state).unwrap(), Action::Negotiate);
    }
}
