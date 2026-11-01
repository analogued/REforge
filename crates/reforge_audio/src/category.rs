// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Semantic host audio groups, independent of sound hardware and game execution.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundCategory {
    Music,
    Speech,
    Effects,
}
impl SoundCategory {
    pub const ALL: [Self; 3] = [Self::Music, Self::Speech, Self::Effects];
    pub const fn index(self) -> usize {
        self as usize
    }
    pub const fn key(self) -> &'static str {
        match self {
            Self::Music => "music_volume",
            Self::Speech => "speech_volume",
            Self::Effects => "effects_volume",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Music => "Music",
            Self::Speech => "Speech",
            Self::Effects => "Sound effects",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryVolumes(pub [u8; 3]);
impl Default for CategoryVolumes {
    fn default() -> Self {
        Self([100; 3])
    }
}
impl CategoryVolumes {
    pub fn get(self, category: SoundCategory) -> u8 {
        self.0[category.index()]
    }
    pub fn valid(self) -> bool {
        self.0.iter().all(|v| *v <= 100)
    }
}
