// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Curated release records. Engines own runtime choices, palettes and previews.

use super::{FileRecord, Game, Release};

/// Player-facing facts and editorial text for one detected release.
/// Never use these display values as game, release or configuration identities.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct ReleaseMetadata {
    pub title: &'static str,
    /// Year of this release, not the first release on another platform.
    pub year: Option<u16>,
    pub developers: &'static [&'static str],
    pub publishers: &'static [&'static str],
    pub genres: &'static [&'static str],
    /// Optional edition label; not a detection or compatibility identifier.
    pub edition: Option<&'static str>,
    /// Catalogued presentation features, not the currently selected options.
    pub video_features: &'static [&'static str],
    pub audio_features: &'static [&'static str],
    /// Short editorial launcher line, distinct from a possible future synopsis.
    pub tagline: &'static str,
}

#[cfg(any(feature = "sdl", test))]
impl ReleaseMetadata {
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
                [self.attribution(), self.genres.join(", ")]
                    .into_iter()
                    .filter(|part| !part.is_empty()),
            )
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn features_line(&self) -> String {
        self.video_features
            .iter()
            .chain(self.audio_features)
            .copied()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub static ALLEY_CAT: ReleaseMetadata = ReleaseMetadata {
    title: "Alley Cat",
    year: Some(1984),
    developers: &["Synapse Software"],
    publishers: &["IBM"],
    genres: &["Arcade action"],
    edition: None,
    video_features: &["320×200", "CGA / PCjr"],
    audio_features: &["PC speaker"],
    tagline: "Back to the alley",
};
pub static MACH3: ReleaseMetadata = ReleaseMetadata {
    title: "Mach 3",
    year: Some(1987),
    developers: &["Loriciels"],
    publishers: &["Loriciels"],
    genres: &["3D rail shooter"],
    edition: None,
    video_features: &["CGA colour / 640×200 monochrome"],
    audio_features: &["PC speaker", "Enhanced PCM"],
    tagline: "Back into the cockpit",
};
#[cfg(feature = "sdl")]
pub static UNKNOWN: ReleaseMetadata = ReleaseMetadata {
    title: "Unknown game",
    year: None,
    developers: &[],
    publishers: &[],
    genres: &[],
    edition: None,
    video_features: &[],
    audio_features: &[],
    tagline: "Original files unavailable",
};

// These fingerprints identify the verified inputs used by the engines. Their
// historical commercial release/version identities remain unknown. Only files
// needed by the current implementations are required; unrelated extras are fine.
pub static RELEASES: &[Release] = &[
    Release {
        id: "alley-cat-verified-reference",
        compatibility_id: "alley-cat-dos-original",
        game: Game::AlleyCat,
        metadata: &ALLEY_CAT,
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 55_067,
            sha256: "4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd",
        }],
    },
    Release {
        id: "mach-3-verified-reference",
        compatibility_id: "mach-3-dos-original",
        game: Game::Mach3,
        metadata: &MACH3,
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

    #[test]
    fn release_lines_handle_multiple_credits_and_absent_facts() {
        let mut metadata = ReleaseMetadata {
            developers: &["Studio A", "Studio B"],
            publishers: &["Publisher A", "Publisher B"],
            genres: &["Action", "Adventure"],
            edition: Some("Reissue"),
            ..ALLEY_CAT
        };
        assert_eq!(metadata.library_line(), "1984 · Studio A, Studio B");
        assert_eq!(
            metadata.detail_line(),
            "1984 · Studio A, Studio B / Publisher A, Publisher B · Action, Adventure"
        );
        metadata.publishers = metadata.developers;
        assert_eq!(metadata.attribution(), "Studio A, Studio B");
        metadata.developers = &[];
        assert_eq!(metadata.attribution(), "Studio A, Studio B");
        metadata.year = None;
        metadata.publishers = &[];
        metadata.video_features = &[];
        metadata.audio_features = &[];
        assert!(metadata.library_line().is_empty());
        assert!(metadata.attribution().is_empty());
        assert_eq!(metadata.detail_line(), "Action, Adventure");
        assert!(metadata.features_line().is_empty());
        metadata.developers = &["Studio A"];
        assert_eq!(metadata.attribution(), "Studio A");
    }

    #[test]
    fn catalog_metadata_uses_structured_nonempty_values() {
        for release in RELEASES {
            let metadata = release.metadata;
            assert!(!metadata.title.trim().is_empty());
            assert!(metadata.year.is_none_or(|year| year > 0));
            for values in [
                metadata.developers,
                metadata.publishers,
                metadata.genres,
                metadata.video_features,
                metadata.audio_features,
            ] {
                for value in values {
                    assert!(!value.is_empty());
                    assert_eq!(*value, value.trim());
                    assert!(!value.contains(" · "), "store separate list items");
                }
            }
        }
    }
}
