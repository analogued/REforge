// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Curated games, editions, reusable specs and detected file distributions.
//! Engines own runtime choices, palettes, geometry and preview generation.

use super::{FileRecord, Game, Release};

/// Shared game identity and editorial data, independent of platform or edition.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct GameMetadata {
    pub identity: Game,
    pub title: &'static str,
    pub genres: &'static [&'static str],
    /// Short editorial launcher line; must fit in two lines at minimum width.
    pub tagline: &'static str,
}

/// Reusable catalogued presentation features, independent of active options.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct SpecsProfile {
    /// Authoring/validation identity; runtime links use direct references.
    #[allow(dead_code)]
    pub id: &'static str,
    pub video_features: &'static [&'static str],
    pub audio_features: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Dos,
}

/// Publication/edition facts shared by one or more detected distributions.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct Edition {
    /// Internal catalog name; not a claim about an official version designation.
    #[allow(dead_code)]
    pub id: &'static str,
    pub game: &'static GameMetadata,
    /// Stored for cataloguing; launcher placement has not yet been selected.
    #[allow(dead_code)]
    pub platform: Platform,
    /// An edition-specific title; None uses the canonical game title.
    pub title: Option<&'static str>,
    pub year: Option<u16>,
    pub developers: &'static [&'static str],
    pub publishers: &'static [&'static str],
    /// Optional display label, independent of edition and release identities.
    pub label: Option<&'static str>,
    pub specs: &'static SpecsProfile,
}

#[cfg(any(feature = "sdl", test))]
impl Edition {
    pub fn title(&self) -> &'static str {
        self.title.unwrap_or(self.game.title)
    }
    pub fn attribution(&self) -> String {
        let developers = self.developers.join(", ");
        let publishers = self.publishers.join(", ");
        if developers.is_empty() || self.developers == self.publishers {
            publishers
        } else if publishers.is_empty() {
            developers
        } else {
            format!("{developers} / {publishers}")
        }
    }

    /// Short year/developer line for sidebar entries and gallery cards.
    pub fn library_line(&self) -> String {
        let credit = self.developers.join(", ");
        self.year
            .map(|year| year.to_string())
            .into_iter()
            .chain((!credit.is_empty()).then_some(credit))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn detail_line(&self) -> String {
        self.year
            .map(|year| year.to_string())
            .into_iter()
            .chain(
                [self.attribution(), self.game.genres.join(", ")]
                    .into_iter()
                    .filter(|part| !part.is_empty()),
            )
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn features_line(&self) -> String {
        self.specs
            .video_features
            .iter()
            .chain(self.specs.audio_features)
            .copied()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub static ALLEY_CAT: GameMetadata = GameMetadata {
    identity: Game::AlleyCat,
    title: "Alley Cat",
    genres: &["Arcade action"],
    tagline: "A small cat, a very busy neighbourhood.",
};
pub static MACH3: GameMetadata = GameMetadata {
    identity: Game::Mach3,
    title: "Mach 3",
    genres: &["3D rail shooter"],
    tagline: "Back into the cockpit.",
};

pub static MINDWHEEL: GameMetadata = GameMetadata {
    identity: Game::Mindwheel,
    title: "Mindwheel",
    genres: &["Interactive fiction"],
    tagline: "A journey through four minds.",
};
pub static ESSEX: GameMetadata = GameMetadata {
    identity: Game::Essex,
    title: "Essex",
    genres: &["Interactive fiction"],
    tagline: "Board the Starship Essex.",
};
pub static ESSEX_DOS: Edition = Edition {
    id: "essex-dos",
    game: &ESSEX,
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BRIMSTONE: GameMetadata = GameMetadata {
    identity: Game::Brimstone,
    title: "Brimstone: The Dream of Gawain",
    genres: &["Interactive fiction"],
    tagline: "Guide Sir Gawain through the underworld of Ulro.",
};
pub static BRIMSTONE_DOS: Edition = Edition {
    id: "brimstone-dos",
    game: &BRIMSTONE,
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BREAKERS: GameMetadata = GameMetadata {
    identity: Game::Breakers,
    title: "Breakers",
    genres: &["Interactive fiction"],
    tagline: "Find your way through an alien world.",
};
pub static BREAKERS_DOS: Edition = Edition {
    id: "breakers-dos",
    game: &BREAKERS,
    platform: Platform::Dos,
    title: None,
    year: Some(1986),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BTZ_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "btz-dos-text",
    video_features: &["80×25 text"],
    audio_features: &["PC speaker"],
};
pub static MINDWHEEL_DOS: Edition = Edition {
    id: "mindwheel-dos",
    game: &MINDWHEEL,
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};

pub static ALLEY_CAT_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "alley-cat-dos",
    video_features: &["320×200", "CGA / PCjr"],
    audio_features: &["PC speaker"],
};
pub static MACH3_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "mach-3-dos",
    video_features: &["CGA colour / 640×200 monochrome"],
    audio_features: &["PC speaker"],
};

pub static ALLEY_CAT_DOS: Edition = Edition {
    id: "alley-cat-dos",
    game: &ALLEY_CAT,
    platform: Platform::Dos,
    title: None,
    year: Some(1984),
    developers: &["Synapse Software"],
    publishers: &["IBM"],
    label: None,
    specs: &ALLEY_CAT_DOS_SPECS,
};
pub static MACH3_DOS: Edition = Edition {
    id: "mach-3-dos",
    game: &MACH3,
    platform: Platform::Dos,
    title: None,
    year: Some(1987),
    developers: &["Loriciels"],
    publishers: &["Loriciels"],
    label: None,
    specs: &MACH3_DOS_SPECS,
};

// These fingerprints identify the verified inputs used by the engines. Their
// historical commercial release/version identities remain unknown. Only files
// needed by the current implementations are required; unrelated extras are fine.
pub static RELEASES: &[Release] = &[
    Release {
        id: "alley-cat-dos-06ff7fe280",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 55_067,
            sha256: "4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd",
            blake3: "8b6c0f5151075e596296675648eb75b4527e228f74aff1b5db2403f03ff84cf2",
        }],
    },
    Release {
        id: "mach-3-dos-5f6ec0fc8b",
        compatibility_id: "mach-3-dos",
        edition: &MACH3_DOS,
        // Retain the published identity while verifying all playback resources.
        #[cfg(test)]
        identity_files: &[
            "MACH3.EXE",
            "DATA1",
            "DATA2",
            "DATA3",
            "DATA4",
            "DATA5",
            "DATA6",
        ],
        files: &[
            FileRecord {
                name: "MACH3.EXE",
                size: 43_008,
                sha256: "0cc5c73476ec31ebc52f5c3a66be58e08244c85b2e8178abac65178a37474fe1",
                blake3: "4d91473bd4444c22a44f51fc2a2e5d13206d06ad74a5f8c31438e7edf35efb4c",
            },
            FileRecord {
                name: "DATA1",
                size: 16_384,
                sha256: "e791d95500402c5c38574c349c65b402ba75302795eaf7b44c069ed8c3588ff0",
                blake3: "aec159c7e7942e4ad57047d92aa2ddbaea64c898f76dd7265e1abff16d88f3ba",
            },
            FileRecord {
                name: "DATA2",
                size: 28_960,
                sha256: "eac1b4e82b6b2a88bee2d8b523e16bf949382572d973cd551ceab1b962c01e50",
                blake3: "7a0a8d67855bcda56384bd4ea18c92a35d302ea2a8a2c9bf5691b8353fe9ab44",
            },
            FileRecord {
                name: "DATA3",
                size: 16_000,
                sha256: "8733be0ec3afd731bca5fa17fae661675c186d1e84a0941b80b77e0a3d2c9bc8",
                blake3: "3acee17ded55afccbea75d458ad8ec1fcc2d8d03244052db8caedd51be01576b",
            },
            FileRecord {
                name: "DATA4",
                size: 25_704,
                sha256: "a7cc941fa54ea1c3cddfcbc11a568d021f33d46a179c0d673db64a4222fd6580",
                blake3: "38b303f55bf076cc2118483f782cce4516378586f168211d0c54b2559f1fb260",
            },
            FileRecord {
                name: "DATA5",
                size: 25_704,
                sha256: "3e1ab7a0e8cd3c4fd44486c6a402506257f13defe2949a49cbdd4565087678cb",
                blake3: "668bad553bb947b8d5be1e0e3653574197e606911d898199ebc04e43c06df8e1",
            },
            FileRecord {
                name: "DATA6",
                size: 16_384,
                sha256: "4b44015ccb5eb001d03670ad5fa46879912625f54a236d44e38556de8efbb2ce",
                blake3: "0fedddf4a90f96b4abbf049618d13c24cfb323f92d060e73e4293777b3e2c72b",
            },
            FileRecord {
                name: "DATA7",
                size: 11_872,
                sha256: "88b521976ab9d47a67f2d371da7028594968305e34e9f061a6e6a52a7f3afd9d",
                blake3: "cf2e9a8fc84770206b7e269edbf47a7679442d0e4388a2614a2c428752afcb6e",
            },
            FileRecord {
                name: "DATA8",
                size: 9_723,
                sha256: "68b3bfb36bc85b8c567ee68ff98d0edc61de95ec0070456c6832b26ee7163976",
                blake3: "b601c1c491a1433d9ac86e6c76c6285223c7d383af84989e88d29225c85c3adf",
            },
            FileRecord {
                name: "DATA9",
                size: 10_874,
                sha256: "bdaff248f4b33cf1da700c0db6dde9116b5a1d0344c83b760e0ab2a5a0b2c31a",
                blake3: "b5c94cbe9cec91f03cce4f93ee1ab94437c77e03d66c4950667ca5f06b5a1132",
            },
            FileRecord {
                name: "DATAA",
                size: 19_690,
                sha256: "979b8318e180d96774352d3e22cc4dc8ba10ec0e943dcc40a10362ba695a7924",
                blake3: "82e54ad4fda7019226d9d9c6d117773d594207c3354c4a938618477aedfb8d45",
            },
            FileRecord {
                name: "DATAB",
                size: 15_500,
                sha256: "5ef430757ba03d01d1f101be1f4c1975c50653f1a5d8980c724f0af47aa601f7",
                blake3: "7b4dfac8d1f5d76fa47f4b885cbb6658282e859b9e45732a093818fa7c59e771",
            },
            FileRecord {
                name: "DATAC",
                size: 8_700,
                sha256: "33cc28ee9d3517f0a97de663b0cfd66ffbeb51165bec6a6c0aefac005e18efec",
                blake3: "cebec6155b621780bb5e187ef1a6f901ae8958c3535f431067bc082fdd6ba0c4",
            },
            FileRecord {
                name: "DATAD",
                size: 34_458,
                sha256: "14fba165214fff3399d43e4ca8a88fcfeeb489ad7184db22d522ba324559a252",
                blake3: "170af134372197d306ee2bf03cd07a67a3744ce54c21d291a5d488a5ae591309",
            },
            FileRecord {
                name: "DATAE",
                size: 8_700,
                sha256: "9a47f6c72e76658a64dbdf7c03d7b7f171307cf390d4c0a305e59f9e9697d2f8",
                blake3: "b3165eaa7b890afc277838e88b27b12835ec573424d4d417e310cce8e4fa54f0",
            },
            FileRecord {
                name: "DATAF",
                size: 8_664,
                sha256: "916208d62c199735000dfd07d67b8a0742aa6726b32c220987994b235705aa4f",
                blake3: "d8274e09437bcab97400c6736abf472c87af22d01be57f7fd114db9e3dbbfbbb",
            },
            FileRecord {
                name: "DATAG",
                size: 8_802,
                sha256: "586c17f80e4d015e91e91830ae17ae0510d2c71a9e8f4745d80d0a22b0ac615b",
                blake3: "989dce0c45fbf05b00d5c71ebcc3e03d0a17a43efa05df906759cc7eb23f0b09",
            },
            FileRecord {
                name: "DATAH",
                size: 8_927,
                sha256: "b383c090a7cbccb08394f5f403eb77cc0e541d2996eea6d5485844e16e6bf00d",
                blake3: "21ea06799ffafa57e890c51b7cb8724b86f92d8f51f19edb850d3e38dbf3abcf",
            },
        ],
    },
    Release {
        id: "mindwheel-dos-2f53a47931",
        compatibility_id: "mindwheel-dos",
        edition: &MINDWHEEL_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 188_928,
                sha256: "4572f6fae68fa81be3e723c0a4a90994fd2c9f1219636a334d2db0761db0b091",
                blake3: "4d07f7201d6143b9b6e648986cb9574929429b05be6eafa0cc10cfc490ced644",
            },
            FileRecord {
                name: "GO.EXE",
                size: 33_280,
                sha256: "268edea9a2ed7df72cd41f2d3c767a57e6ebbe019037b46638c06be5c8512bcb",
                blake3: "abc0678c92dc91f39a0203c19e80afed9564c32250db21216c904427b5cdd756",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,
                sha256: "40f185dc69d51a5f98769f5d89f10405f29132ebbbdd73290c7f139a12efd1e7",
                blake3: "a138ce6054a71b74b1f8dfd56908cfb463ab45c249f19b4379e48f86c0a907c3",
            },
        ],
    },
    Release {
        id: "essex-dos-2ba69b4250",
        compatibility_id: "essex-dos",
        edition: &ESSEX_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 180736,
                sha256: "06579988cbfd31342d741cbb729898aea575d95fbd4558c09e0982d92abb6541",
                blake3: "f04cdf377f6f48459a499b66b5ea2f4f4eab3232bfbcfb237d0cb8f0425cd107",
            },
            FileRecord {
                name: "GO.EXE",
                size: 35328,
                sha256: "caf6b08490f4d343ab9c4507ba591f66b5fc47d1807aac6dfcd232ec114003bd",
                blake3: "1d3d082d6ebc8f7c5a1c49ca7c776653d0b3755068ac2fb972f02b6b13feea98",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,
                sha256: "12f0b71b9be8bb76619bc86057b8edbf82c22ae136843fa779b54eb329586800",
                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
        ],
    },
    Release {
        id: "brimstone-dos-0020ff40a4",
        compatibility_id: "brimstone-dos",
        edition: &BRIMSTONE_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 183_296,
                sha256: "34f6cd14b2f0d6fd1d5b8d6c99891fb603ae4cf692599e039c8eecff1c3baef4",
                blake3: "d473d1d4f19a41900e32ee088c6e5c5d0b118e0a310e1c1d5f1e0c474e03692c",
            },
            FileRecord {
                name: "GO.EXE",
                size: 35_328,
                sha256: "33e398866bc736f3475c36002591c218dfdc2cd54d33b87ed7c0b1d3e9ab46b7",
                blake3: "758b53ef59d01a191270b6971fea44b1e0e1dfcd4720674afd6e376d1d0ae8e5",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,
                sha256: "12f0b71b9be8bb76619bc86057b8edbf82c22ae136843fa779b54eb329586800",
                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
        ],
    },
    Release {
        id: "breakers-dos-acb87c72b9",
        compatibility_id: "breakers-dos",
        edition: &BREAKERS_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 232960,
                sha256: "0c8fb003bedfb212b601c8bb79c816283447b39597d99dbb47ca32d53fab7ae0",
                blake3: "79f1afccc92c2201b4fb50311065d01810757587ed6b8a795a1452ce413f3e67",
            },
            FileRecord {
                name: "RUN.EXE",
                size: 35328,
                sha256: "4a667bcb840a72357613a04ba2b43c341b58c9a9ea08513433750930f2bd64bb",
                blake3: "95aa4027bef41da21a3fa636be571ab503a0fc3da1353150f750714cd5acb6ac",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,
                sha256: "12f0b71b9be8bb76619bc86057b8edbf82c22ae136843fa779b54eb329586800",
                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
            FileRecord {
                name: "TITLE.EXE",
                size: 3584,
                sha256: "88ff29bec4e1987ad02bd01273fef502262393d39edeed53a179064b3a994ef5",
                blake3: "d49cde59b5c1502dfbd939c90676b4176329b5101bb99821dd4f284704176278",
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn edition_lines_handle_multiple_credits_and_absent_facts() {
        static SHARED_GAME: GameMetadata = GameMetadata {
            genres: &["Action", "Adventure"],
            ..ALLEY_CAT
        };
        static EMPTY_SPECS: SpecsProfile = SpecsProfile {
            id: "synthetic-empty-specs",
            video_features: &[],
            audio_features: &[],
        };
        let mut edition = Edition {
            game: &SHARED_GAME,
            developers: &["Studio A", "Studio B"],
            publishers: &["Publisher A", "Publisher B"],
            label: Some("Reissue"),
            ..ALLEY_CAT_DOS
        };
        assert_eq!(edition.library_line(), "1984 · Studio A, Studio B");
        assert_eq!(
            edition.detail_line(),
            "1984 · Studio A, Studio B / Publisher A, Publisher B · Action, Adventure"
        );
        edition.publishers = edition.developers;
        assert_eq!(edition.attribution(), "Studio A, Studio B");
        edition.developers = &[];
        assert_eq!(edition.attribution(), "Studio A, Studio B");
        edition.year = None;
        edition.publishers = &[];
        edition.specs = &EMPTY_SPECS;
        assert!(edition.library_line().is_empty());
        assert!(edition.attribution().is_empty());
        assert_eq!(edition.detail_line(), "Action, Adventure");
        assert!(edition.features_line().is_empty());
        edition.developers = &["Studio A"];
        assert_eq!(edition.attribution(), "Studio A");
    }

    #[test]
    fn distributions_share_editions_and_editions_share_games_and_specs() {
        static REPRINT: Edition = Edition {
            id: "synthetic-reprint",
            title: Some("Localized test title"),
            year: Some(1985),
            publishers: &["Test publisher"],
            ..ALLEY_CAT_DOS
        };
        let original = &RELEASES[0];
        let mut another = Release {
            id: "synthetic-other-executable",
            #[cfg(test)]
            identity_files: &[],
            files: &[],
            ..*original
        };
        assert!(std::ptr::eq(another.edition, original.edition));
        // Same edition does not imply compatibility.
        another.compatibility_id = "synthetic-incompatible-saves";
        assert!(!another.compatible_with(original));
        another.edition = &REPRINT;
        assert!(std::ptr::eq(another.edition.game, original.edition.game));
        assert!(std::ptr::eq(another.edition.specs, original.edition.specs));
        assert_eq!(another.game(), original.game());
        assert_eq!(another.edition.title(), "Localized test title");
        assert_eq!(original.edition.title(), "Alley Cat");
        assert_eq!(another.edition.year, Some(1985));
        assert_eq!(original.edition.year, Some(1984));
        assert_ne!(another.edition.publishers, original.edition.publishers);
        // Different editions can explicitly share a compatibility group.
        another.compatibility_id = original.compatibility_id;
        assert!(another.compatible_with(original));
    }

    #[test]
    fn spec_changes_are_explicit_and_independent_of_shared_game_data() {
        static OTHER_SPECS: SpecsProfile = SpecsProfile {
            id: "synthetic-other-video",
            video_features: &["Synthetic video mode"],
            audio_features: ALLEY_CAT_DOS_SPECS.audio_features,
        };
        let edition = Edition {
            specs: &OTHER_SPECS,
            ..ALLEY_CAT_DOS
        };
        assert!(std::ptr::eq(edition.game, ALLEY_CAT_DOS.game));
        assert!(!std::ptr::eq(edition.specs, ALLEY_CAT_DOS.specs));
        assert_eq!(edition.features_line(), "Synthetic video mode · PC speaker");
        assert_eq!(
            ALLEY_CAT_DOS.features_line(),
            "320×200 · CGA / PCjr · PC speaker"
        );
    }

    #[test]
    fn catalog_links_have_unique_ids_and_structured_values() {
        fn register<T>(
            records: &mut BTreeMap<&'static str, &'static T>,
            id: &'static str,
            record: &'static T,
        ) {
            assert!(!id.is_empty());
            assert!(
                id.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            );
            if let Some(previous) = records.insert(id, record) {
                assert!(
                    std::ptr::eq(previous, record),
                    "different records reuse ID {id}"
                );
            }
        }
        let (mut games, mut editions, mut profiles) =
            (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        for release in RELEASES {
            let edition = release.edition;
            let game = edition.game;
            let specs = edition.specs;
            register(&mut games, game.identity.id(), game);
            register(&mut editions, edition.id, edition);
            register(&mut profiles, specs.id, specs);
            assert!(std::ptr::eq(game.identity.metadata(), game));
            assert!(!game.title.trim().is_empty());
            assert!(!edition.title().trim().is_empty());
            assert!(edition.year.is_none_or(|year| year > 0));
            for values in [
                edition.developers,
                edition.publishers,
                game.genres,
                specs.video_features,
                specs.audio_features,
            ] {
                for value in values {
                    assert!(!value.is_empty());
                    assert_eq!(*value, value.trim());
                    assert!(!value.contains(" · "), "store separate list items");
                }
            }
        }
        assert_eq!(ALLEY_CAT_DOS.platform, Platform::Dos);
        assert_eq!(MACH3_DOS.platform, Platform::Dos);
    }
}
