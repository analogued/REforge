// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded compatibility data for the SHA-256 to BLAKE3 identity transition.
//! These opaque aliases need no old hashing algorithm or per-file metadata.
use reforge_settings::library::ReleaseAlias;

pub(crate) const RELEASES: &[ReleaseAlias] = &[
    ReleaseAlias {
        game_id: "alley-cat",
        old_release_id: "alley-cat-dos-06ff7fe280",
        new_release_id: "alley-cat-dos-7b92bb401f",
    },
    ReleaseAlias {
        game_id: "mach-3",
        old_release_id: "mach-3-dos-5f6ec0fc8b",
        new_release_id: "mach-3-dos-3d1f0c95f9",
    },
    ReleaseAlias {
        game_id: "mindwheel",
        old_release_id: "mindwheel-dos-2f53a47931",
        new_release_id: "mindwheel-dos-057c9533e4",
    },
    ReleaseAlias {
        game_id: "essex",
        old_release_id: "essex-dos-2ba69b4250",
        new_release_id: "essex-dos-226e565be2",
    },
    ReleaseAlias {
        game_id: "brimstone",
        old_release_id: "brimstone-dos-0020ff40a4",
        new_release_id: "brimstone-dos-d3bec28797",
    },
    ReleaseAlias {
        game_id: "breakers",
        old_release_id: "breakers-dos-acb87c72b9",
        new_release_id: "breakers-dos-d5569f4b23",
    },
];

// Matching the complete new cache key also binds the full file manifest,
// visual variant and recipe revision. A changed recipe never reuses an alias.
#[cfg(feature = "sdl")]
pub(crate) const PREVIEWS: &[(&str, &str)] = &[
    (
        "8fafb859f49fb1d4f413c98e0f6385b1245df94ea58d70cfc11c6873cb97ea50",
        "09d2dd43ea1da1f76c61bb2090639831fe3edde8fdcd744b7f8a310dd2994c31",
    ),
    (
        "0fea2a89192fed28a45b79ecf4037c21f882be6b245df140e993ec361871be1b",
        "5e4094eea5650f575eb9c5499ff59dcb62a7f4a5aa7fdfb9111697e1282dc4db",
    ),
    (
        "498002753aab0d55ea785f60a031b1f5864e43d4d55b8c341a93edcaec18f749",
        "14b37c16f15003365116411f980295d88365a9494ee3224865b9ac41a14970e5",
    ),
    (
        "64843ebe82584b41f43a11905ede1b5dca90758d748e4f5504e21c2054313ad3",
        "4fb64c076851c836f5b2364dc87280fae8d8d24aaab35898961d8e4904dbe385",
    ),
    (
        "2ba86830d34364eef6948a8fe0dc198fef10dc50a8f8e3817c30515661f9a33b",
        "753c86b0b10f0da5431eee52851a4fb7fd80cbc414a31d053ee739e9f01f0078",
    ),
    (
        "3e15cd5fde27137fa1e7ffff693da8233c2ea82e4a4d39948010e1fb0f231eea",
        "b52b8bbadf4db2f97c5a6715dd8a65cdf539e2b7391a56038eeb63141b46f842",
    ),
    (
        "10c8636834f281c2e4ff3712df93ae4a279be51bdfcc8ae0badf09cfe58a9277",
        "a3c9af6daf79589b6d5f549df68d235d19aa532689ca2b01ce6d66fd0c1ea418",
    ),
    (
        "b3def47f58106a486b47d5075ebefde65e4bc451b7c4ccdb24cf8b48b86edefe",
        "09481a94571c5dc818df90d2cdf7b1bc74fc227b34c9c90db15cb7e6f41b45e3",
    ),
    (
        "498002753aab0d55ea785f60a031b1f5864e43d4d55b8c341a93edcaec18f749",
        "72da4f22b4e7475d073a63294c1037251195ef8d5a37ac93d9b9af1482a3385f",
    ),
    (
        "64843ebe82584b41f43a11905ede1b5dca90758d748e4f5504e21c2054313ad3",
        "0524f364b1fcbd209afbeaf5c4a2ebf115cb5b5c556ba87c2285c52619c5b252",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_are_unambiguous_one_step_translations_to_current_releases() {
        let mut old = std::collections::HashSet::new();
        for alias in RELEASES {
            assert!(old.insert((alias.game_id, alias.old_release_id)));
            assert_ne!(alias.old_release_id, alias.new_release_id);
            assert!(
                super::super::RELEASES
                    .iter()
                    .any(|release| release.id == alias.new_release_id
                        && release.game().id() == alias.game_id)
            );
            assert!(!RELEASES.iter().any(|other| other.game_id == alias.game_id
                && other.old_release_id == alias.new_release_id));
        }
    }
}
