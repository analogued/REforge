// Copyright (c) 2026 analogued. Project license has not been selected.
//! Host volume classification. This never selects, skips or reschedules sound.

use super::Resource;
use reforge_audio::category::SoundCategory;

/// Announcements include the intro/pre-game, gameplay completion and ending
/// samples. The intro's repeated musical fragments remain Music, including
/// their partial-length calls. Reverse playback retains the source's category.
pub(super) fn sample_category(resource: Resource) -> SoundCategory {
    match resource {
        Resource::Data7 | Resource::Data8 | Resource::Data9 | Resource::DataA | Resource::DataB => {
            SoundCategory::Speech
        }
        Resource::DataC
        | Resource::DataD
        | Resource::DataE
        | Resource::DataF
        | Resource::DataG
        | Resource::DataH => SoundCategory::Music,
    }
}
