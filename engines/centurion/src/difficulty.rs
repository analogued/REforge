// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later
//! Original global presets and independently applied combat/game difficulty.
use crate::assets::{Result, word};
use crate::state::Campaign;

pub const MENU: usize = 0x5b8;
pub const TUNING_MENU: usize = 0x5ca;
pub const LEVEL_MENU: usize = 0x5dc;
const LEVELS: usize = 0x48e;

impl Campaign {
    pub fn difficulty_preset(&self) -> Option<usize> {
        let levels = &self.data[LEVELS..LEVELS + 4];
        (levels[0] < 4 && levels.iter().all(|value| *value == levels[0]))
            .then_some(usize::from(levels[0]))
    }

    pub fn difficulty_level(&self, category: usize) -> Result<usize> {
        if category >= 4 || self.data[LEVELS + category] >= 4 {
            return Err("Invalid difficulty category or level".into());
        }
        Ok(usize::from(self.data[LEVELS + category]))
    }

    pub fn set_difficulty_preset(&mut self, level: usize) -> Result<()> {
        if level >= 4 {
            return Err("Invalid difficulty level".into());
        }
        self.data[LEVELS..LEVELS + 4].fill(level as u8);
        Ok(())
    }

    pub fn set_difficulty_level(&mut self, category: usize, level: usize) -> Result<()> {
        if category >= 4 || level >= 4 {
            return Err("Invalid difficulty category or level".into());
        }
        self.data[LEVELS + category] = level as u8;
        Ok(())
    }

    pub fn difficulty_label(&self, category: usize) -> Result<String> {
        self.string(word(&self.data, 0x484 + self.difficulty_level(category)? * 2)? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_changes_only_one_category_and_presets_restore_uniform_levels() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        for level in 0..4 {
            state.set_difficulty_preset(level).unwrap();
            assert_eq!(state.difficulty_preset(), Some(level));
            for category in 0..4 {
                let before = state.data.clone();
                let custom = (level + 1) % 4;
                state.set_difficulty_level(category, custom).unwrap();
                assert_eq!(state.difficulty_level(category).unwrap(), custom);
                assert_eq!(state.difficulty_preset(), None);
                assert!(
                    before
                        .iter()
                        .zip(&state.data)
                        .enumerate()
                        .all(|(at, (a, b))| { *a == *b || at == LEVELS + category })
                );
                state.set_difficulty_preset(level).unwrap();
            }
        }
    }

    #[test]
    fn invalid_tuning_does_not_change_campaign_data() {
        let mut state = Campaign::test_data(vec![0; 65536]);
        let before = state.data.clone();
        assert!(state.set_difficulty_preset(4).is_err());
        assert!(state.set_difficulty_level(4, 0).is_err());
        assert!(state.set_difficulty_level(0, 4).is_err());
        assert!(state.difficulty_level(4).is_err());
        assert_eq!(state.data, before);
    }
}
