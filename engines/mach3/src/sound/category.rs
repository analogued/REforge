// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host volume classification. This never selects, skips or reschedules sound.

use super::Resource;
use reforge_audio::category::SoundCategory;

/// Announcements include the intro/pre-game, gameplay completion and ending
/// samples. Intro music includes DATAB's forward/reverse bookends and the
/// repeated musical fragments, including their partial-length calls.
pub(super) fn sample_category(resource: Resource) -> SoundCategory {
    match resource {
        Resource::Data7 | Resource::Data8 | Resource::Data9 | Resource::DataA => {
            SoundCategory::Speech
        }
        Resource::DataB
        | Resource::DataC
        | Resource::DataD
        | Resource::DataE
        | Resource::DataF
        | Resource::DataG
        | Resource::DataH => SoundCategory::Music,
    }
}
