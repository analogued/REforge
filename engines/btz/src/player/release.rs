// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Loading profiles for the currently supported DOS releases.

use crate::interpreter::{self, DosResources, Interpreter};

/// The application's detected game selects its known DOS loading profile.
/// Callers verify the required files before loading; this is not a detector or
/// an official interpreter version designation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DosRelease {
    Mindwheel,
    Essex,
    Brimstone,
    Breakers,
}

impl DosRelease {
    pub fn from_game_id(game_id: &str) -> Option<Self> {
        match game_id {
            "mindwheel" => Some(Self::Mindwheel),
            "essex" => Some(Self::Essex),
            "brimstone" => Some(Self::Brimstone),
            "breakers" => Some(Self::Breakers),
            _ => None,
        }
    }

    pub fn interpreter(self) -> (&'static Interpreter, DosResources) {
        match self {
            Self::Mindwheel => (
                &interpreter::MINDWHEEL_DOS,
                interpreter::MINDWHEEL_RESOURCES,
            ),
            Self::Essex => (&interpreter::ESSEX_DOS, interpreter::ESSEX_RESOURCES),
            Self::Brimstone => (
                &interpreter::BRIMSTONE_DOS,
                interpreter::BRIMSTONE_RESOURCES,
            ),
            Self::Breakers => (&interpreter::BREAKERS_DOS, interpreter::BREAKERS_RESOURCES),
        }
    }
}
