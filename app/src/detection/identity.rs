// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Catalog authoring/validation only. Runtime detection uses the assigned IDs
//! and verifies the original files against their full BLAKE3 fingerprints.
//! SHA-256 metadata keeps the version-one identity encoding stable.

use super::{FileRecord, Release, catalog::Platform};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"reforge-detected-release-v1\0";
const SUFFIX_LENGTH: usize = 10;

pub(super) fn release_id(release: &Release) -> Result<String, String> {
    super::search::catalog_directories(release.files)?;
    let platform = match release.edition.platform {
        Platform::Dos => "dos",
    };
    let identity_files = if release.identity_files.is_empty() {
        release.files.to_vec()
    } else {
        release
            .identity_files
            .iter()
            .map(|name| {
                release
                    .files
                    .iter()
                    .find(|file| file.name.eq_ignore_ascii_case(name))
                    .copied()
                    .ok_or_else(|| format!("Unknown identity filename {name}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let fingerprint = manifest_fingerprint(release.edition.platform, &identity_files)?;
    Ok(format!(
        "{}-{platform}-{}",
        release.game().id(),
        &fingerprint[..SUFFIX_LENGTH]
    ))
}

fn manifest_fingerprint(platform: Platform, files: &[FileRecord]) -> Result<String, String> {
    if files.is_empty() {
        return Err("A detected release must have required files".into());
    }
    let mut records = files
        .iter()
        .map(|file| {
            let name = match platform {
                // Normalize canonical DOS catalog names, not physical aliases
                // selected by detection. Future platforms choose their own rules.
                Platform::Dos => file.name.to_ascii_lowercase(),
            };
            (name, file)
        })
        .collect::<Vec<_>>();
    records.sort_by(|(a, _), (b, _)| a.cmp(b));
    if records.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("Duplicate required filename after platform normalization".into());
    }
    let mut digest = Sha256::new();
    digest.update(DOMAIN);
    digest.update((records.len() as u64).to_le_bytes());
    for (name, file) in records {
        if name.is_empty() {
            return Err("A required filename cannot be empty".into());
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("Invalid SHA-256 for {}", file.name));
        }
        // Explicit byte lengths and little-endian integers make the encoding
        // unambiguous and independent of the host architecture/locale.
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(file.size.to_le_bytes());
        digest.update(file.sha256.to_ascii_lowercase().as_bytes());
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_identity_can_survive_expanded_verification_and_reordering() {
        static ORIGINAL: &[FileRecord] = &[FileRecord {
            name: "GAME",
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            blake3: "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        }];
        static EXPANDED: &[FileRecord] = &[
            FileRecord {
                name: "SOUND",
                size: 3,
                sha256: "3608bca1e44ea6c4d268eb6db02260269892c0b42b86bbf1e77a6fa16c3c9282",
                blake3: "f006b5ee4890b66656cf6c23998e25196a163644665dc9d4b47da1fca3037023",
            },
            ORIGINAL[0],
        ];
        let original = Release {
            files: ORIGINAL,
            identity_files: &[],
            ..super::super::RELEASES[0]
        };
        let mut expanded = Release {
            files: EXPANDED,
            identity_files: &["game"],
            ..original
        };
        assert_eq!(release_id(&original), release_id(&expanded));
        expanded.identity_files = &[];
        assert_ne!(release_id(&original), release_id(&expanded));
        expanded.identity_files = &["missing"];
        assert!(release_id(&expanded).is_err());
        expanded.identity_files = &["GAME", "game"];
        assert!(release_id(&expanded).is_err());
    }

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const XYZ: &str = "3608bca1e44ea6c4d268eb6db02260269892c0b42b86bbf1e77a6fa16c3c9282";
    fn file(name: &'static str, size: u64, sha256: &'static str) -> FileRecord {
        FileRecord {
            name,
            size,
            sha256,
            blake3: "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        }
    }
    fn fingerprint(files: &[FileRecord]) -> String {
        manifest_fingerprint(Platform::Dos, files).unwrap()
    }

    #[test]
    fn dos_filename_case_and_manifest_order_do_not_change_identity() {
        let first = fingerprint(&[file("CAT.EXE", 3, ABC), file("Data.Bin", 4, XYZ)]);
        let second = fingerprint(&[file("DATA.bin", 4, XYZ), file("cat.exe", 3, ABC)]);
        assert_eq!(first, second);
    }

    #[test]
    fn every_required_file_field_and_membership_affect_identity() {
        let base = fingerprint(&[file("GAME.EXE", 3, ABC), file("DATA", 4, XYZ)]);
        let variants = [
            vec![file("OTHER.EXE", 3, ABC), file("DATA", 4, XYZ)],
            vec![file("GAME.EXE", 4, ABC), file("DATA", 4, XYZ)],
            vec![file("GAME.EXE", 3, XYZ), file("DATA", 4, XYZ)],
            vec![file("GAME.EXE", 3, ABC), file("DATA", 4, ABC)],
            vec![file("GAME.EXE", 3, ABC)],
            vec![
                file("GAME.EXE", 3, ABC),
                file("DATA", 4, XYZ),
                file("EXTRA", 1, ABC),
            ],
        ];
        for variant in variants {
            assert_ne!(base, fingerprint(&variant));
        }
    }

    #[test]
    fn hex_spelling_is_normalized_but_non_ascii_names_remain_exact() {
        assert_eq!(
            fingerprint(&[file("game.exe", 3, ABC)]),
            fingerprint(&[file(
                "game.exe",
                3,
                "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
            )])
        );
        assert_ne!(
            fingerprint(&[file("Ä.dat", 3, ABC)]),
            fingerprint(&[file("ä.dat", 3, ABC)])
        );
    }

    #[test]
    fn malformed_or_ambiguous_manifests_are_rejected() {
        for files in [
            vec![],
            vec![file("", 3, ABC)],
            vec![file("GAME.EXE", 3, "not-a-hash")],
            vec![file(
                "GAME.EXE",
                3,
                "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg",
            )],
            vec![file("GAME.EXE", 3, ABC), file("game.exe", 3, ABC)],
        ] {
            assert!(manifest_fingerprint(Platform::Dos, &files).is_err());
        }
    }

    #[test]
    fn metadata_and_assigned_names_do_not_enter_the_fingerprint() {
        static EDITION: super::super::Edition = super::super::Edition {
            id: "synthetic-renamed-edition",
            title: Some("Synthetic display title"),
            year: Some(2000),
            publishers: &["Synthetic publisher"],
            ..*super::super::RELEASES[0].edition
        };
        let base = &super::super::RELEASES[0];
        let other = Release {
            id: "synthetic-other-assigned-name",
            compatibility_id: "synthetic-other-compatibility-group",
            edition: &EDITION,
            ..*base
        };
        assert_eq!(release_id(base), release_id(&other));
    }

    #[test]
    fn changing_the_runtime_fingerprint_keeps_version_one_identity() {
        let base = file("GAME.EXE", 3, ABC);
        let changed = FileRecord {
            blake3: "another-verification-fingerprint",
            ..base
        };
        assert_eq!(fingerprint(&[base]), fingerprint(&[changed]));
    }
}
