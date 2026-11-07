// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

static EXTENDED_EDITION: Edition = Edition {
    engine: Engine::AlleyCat(alleycat::assets::InputFormat::CatWithPatch),
    ..ALLEY_CAT_DOS
};
static EXTENSIONS: &[Release] = &[
    Release {
        id: "base",
        ..SYNTHETIC[0]
    },
    Release {
        id: "extended",
        edition: &EXTENDED_EDITION,
        files: &[
            FileRecord {
                name: "CAT.EXE",
                size: 3,
                blake3: ABC,
            },
            FileRecord {
                name: "PATCH.DIF",
                size: 3,
                blake3: DEF,
            },
        ],
        ..SYNTHETIC[0]
    },
];

#[test]
fn a_verified_extension_wins_for_directory_and_either_file_anchor() {
    let directory = TestDirectory::new();
    let executable = directory.write("start.bin", b"abc");
    let patch = directory.write("restoration.bin", b"def");
    for anchor in [&directory.0, &executable, &patch] {
        let mut reads = BTreeMap::new();
        let detected = detect_with(anchor, EXTENSIONS, file_size, |path| {
            *reads.entry(path.to_owned()).or_insert(0) += 1;
            hash_file(path)
        })
        .unwrap();
        assert_eq!(detected.release.id, "extended");
        assert_eq!(detected.original, executable);
        assert_eq!(detected.files["CAT.EXE"], executable);
        assert_eq!(detected.files["PATCH.DIF"], patch);
        assert_eq!(detected.engine(), EXTENDED_EDITION.engine);
        assert!(reads.values().all(|count| *count == 1));
    }
}

#[test]
fn absent_or_corrupt_additional_data_does_not_override_the_verified_base() {
    let directory = TestDirectory::new();
    let executable = directory.write("CAT.EXE", b"abc");
    assert_eq!(
        detect_with(&directory.0, EXTENSIONS, file_size, hash_file)
            .unwrap()
            .release
            .id,
        "base"
    );
    let patch = directory.write("PATCH.DIF", b"bad");
    for anchor in [&directory.0, &executable] {
        assert_eq!(
            detect_with(anchor, EXTENSIONS, file_size, hash_file)
                .unwrap()
                .release
                .id,
            "base"
        );
    }
    assert!(detect_with(&patch, EXTENSIONS, file_size, hash_file).is_err());
}

#[test]
fn independent_executable_forms_remain_ambiguous_and_explicitly_selectable() {
    static ALTERNATIVES: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release { ..EXTENSIONS[1] },
        Release {
            id: "standalone",
            files: &[FileRecord {
                name: "OTHER.EXE",
                size: 3,
                blake3: XYZ,
            }],
            ..SYNTHETIC[0]
        },
    ];
    let directory = TestDirectory::new();
    let base = directory.write("CAT.EXE", b"abc");
    directory.write("PATCH.DIF", b"def");
    let other = directory.write("OTHER.EXE", b"xyz");
    let error = detect_with(&directory.0, ALTERNATIVES, file_size, hash_file).unwrap_err();
    assert!(error.contains("Ambiguous") && error.contains("extended"));
    assert!(error.contains("standalone"));
    assert_eq!(
        detect_with(&base, ALTERNATIVES, file_size, hash_file)
            .unwrap()
            .release
            .id,
        "extended"
    );
    assert_eq!(
        detect_with(&other, ALTERNATIVES, file_size, hash_file)
            .unwrap()
            .release
            .id,
        "standalone"
    );
}

#[test]
fn incompatible_releases_and_distinct_games_do_not_suppress_a_subset() {
    static INCOMPATIBLE: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release {
            compatibility_id: "incompatible",
            ..EXTENSIONS[1]
        },
    ];
    static OTHER_GAME: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release {
            edition: &MACH3_DOS,
            ..EXTENSIONS[1]
        },
    ];
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("PATCH.DIF", b"def");
    for catalog in [INCOMPATIBLE, OTHER_GAME] {
        assert!(
            detect_with(&directory.0, catalog, file_size, hash_file)
                .unwrap_err()
                .contains("Ambiguous")
        );
    }
}

#[test]
fn alternative_extensions_remain_ambiguous_until_one_manifest_contains_both() {
    static ALTERNATIVES: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release { ..EXTENSIONS[1] },
        Release {
            id: "other-extension",
            files: &[
                EXTENSIONS[1].files[0],
                FileRecord {
                    name: "SOUND.DAT",
                    size: 3,
                    blake3: XYZ,
                },
            ],
            ..EXTENSIONS[1]
        },
    ];
    static COMBINED: &[Release] = &[
        Release { ..ALTERNATIVES[0] },
        Release { ..ALTERNATIVES[1] },
        Release { ..ALTERNATIVES[2] },
        Release {
            id: "combined",
            files: &[
                EXTENSIONS[1].files[0],
                EXTENSIONS[1].files[1],
                ALTERNATIVES[2].files[1],
            ],
            ..EXTENSIONS[1]
        },
    ];
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("PATCH.DIF", b"def");
    directory.write("SOUND.DAT", b"xyz");
    let error = detect_with(&directory.0, ALTERNATIVES, file_size, hash_file).unwrap_err();
    assert!(error.contains("Ambiguous"));
    assert!(error.contains("extended") && error.contains("other-extension"));
    assert_eq!(
        detect_with(&directory.0, COMBINED, file_size, hash_file)
            .unwrap()
            .release
            .id,
        "combined"
    );
}

#[test]
fn extensions_must_share_roles_and_selected_paths_not_merely_contents() {
    static DIFFERENT_ROLE: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release {
            files: &[
                FileRecord {
                    name: "OTHER.EXE",
                    ..EXTENSIONS[1].files[0]
                },
                EXTENSIONS[1].files[1],
            ],
            ..EXTENSIONS[1]
        },
    ];
    static DIFFERENT_PATH: &[Release] = &[
        Release { ..EXTENSIONS[0] },
        Release {
            files: &[
                EXTENSIONS[1].files[0],
                FileRecord {
                    name: "DATA/PATCH.DIF",
                    ..EXTENSIONS[1].files[1]
                },
            ],
            ..EXTENSIONS[1]
        },
    ];
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("PATCH.DIF", b"def");
    assert!(
        detect_with(&directory.0, DIFFERENT_ROLE, file_size, hash_file)
            .unwrap_err()
            .contains("Ambiguous")
    );
    // The common role in this catalog is still selected from the same logical
    // path, even though the extension's extra resource lives in a subdirectory.
    fs::remove_file(directory.0.join("PATCH.DIF")).unwrap();
    let patch = directory.write("DATA/renamed-patch", b"def");
    let detected = detect_with(&directory.0, DIFFERENT_PATH, file_size, hash_file).unwrap();
    assert_eq!(detected.release.id, "extended");
    assert_eq!(detected.files["DATA/PATCH.DIF"], patch);
    // A different selected path must not be treated as the same installation.
    let base = Verified {
        release: &EXTENSIONS[0],
        root: directory.0.clone(),
        paths: vec![directory.0.join("CAT.EXE")],
    };
    let alternate = Verified {
        release: &EXTENSIONS[1],
        root: directory.0.clone(),
        paths: vec![directory.0.join("another-copy"), patch],
    };
    assert!(!verified_extension(&base, &alternate));
}

#[cfg(unix)]
#[test]
fn verified_extensions_preserve_logical_symlink_paths() {
    let directory = TestDirectory::new();
    let baseline = TestDirectory::new();
    baseline.write("cat", b"abc");
    baseline.write("patch", b"def");
    let executable = directory.0.join("CAT.EXE");
    let patch = directory.0.join("PATCH.DIF");
    std::os::unix::fs::symlink(baseline.0.join("cat"), &executable).unwrap();
    std::os::unix::fs::symlink(baseline.0.join("patch"), &patch).unwrap();
    for anchor in [&directory.0, &executable, &patch] {
        let detected = detect_with(anchor, EXTENSIONS, file_size, hash_file).unwrap();
        assert_eq!(detected.release.id, "extended");
        assert_eq!(detected.original, executable);
        assert_eq!(detected.files["PATCH.DIF"], patch);
    }
}
