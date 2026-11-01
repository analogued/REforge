// Copyright (c) 2026 analogued. Project license has not yet been selected.
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

pub static ALLEY_CAT_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "alley-cat-dos",
    video_features: &["320×200", "CGA / PCjr"],
    audio_features: &["PC speaker"],
};
pub static MACH3_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "mach-3-dos",
    video_features: &["CGA colour / 640×200 monochrome"],
    audio_features: &["PC speaker", "Enhanced PCM"],
};

pub static ALLEY_CAT_DOS: Edition = Edition {
    id: "alley-cat-dos-reference",
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
    id: "mach-3-dos-reference",
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
        id: "alley-cat-verified-reference",
        compatibility_id: "alley-cat-dos-original",
        edition: &ALLEY_CAT_DOS,
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 55_067,
            sha256: "4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd",
        }],
    },
    Release {
        id: "mach-3-verified-reference",
        compatibility_id: "mach-3-dos-original",
        edition: &MACH3_DOS,
        files: &[
            FileRecord {
                name: "MACH3.EXE",
                size: 43_008,
                sha256: "0cc5c73476ec31ebc52f5c3a66be58e08244c85b2e8178abac65178a37474fe1",
            },
            FileRecord {
                name: "DATA1",
                size: 16_384,
                sha256: "e791d95500402c5c38574c349c65b402ba75302795eaf7b44c069ed8c3588ff0",
            },
            FileRecord {
                name: "DATA2",
                size: 28_960,
                sha256: "eac1b4e82b6b2a88bee2d8b523e16bf949382572d973cd551ceab1b962c01e50",
            },
            FileRecord {
                name: "DATA3",
                size: 16_000,
                sha256: "8733be0ec3afd731bca5fa17fae661675c186d1e84a0941b80b77e0a3d2c9bc8",
            },
            FileRecord {
                name: "DATA4",
                size: 25_704,
                sha256: "a7cc941fa54ea1c3cddfcbc11a568d021f33d46a179c0d673db64a4222fd6580",
            },
            FileRecord {
                name: "DATA5",
                size: 25_704,
                sha256: "3e1ab7a0e8cd3c4fd44486c6a402506257f13defe2949a49cbdd4565087678cb",
            },
            FileRecord {
                name: "DATA6",
                size: 16_384,
                sha256: "4b44015ccb5eb001d03670ad5fa46879912625f54a236d44e38556de8efbb2ce",
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
