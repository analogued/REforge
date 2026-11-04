// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn compatibility_is_explicit_and_never_crosses_game_identity() {
    let release = &RELEASES[0];
    let mut other = Release {
        id: "synthetic-different-packaging",
        compatibility_id: release.compatibility_id,
        edition: release.edition,
        identity_files: &[],
        files: &[],
    };
    assert!(release.compatible_with(&other));
    other.compatibility_id = "synthetic-incompatible-saves";
    assert!(!release.compatible_with(&other));
    other.compatibility_id = release.compatibility_id;
    other.edition = &MACH3_DOS;
    assert!(!release.compatible_with(&other));
    other.edition = release.edition;
    other.compatibility_id = "";
    assert!(!other.compatible_with(&other));
}

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const DEF: &str = "cb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34";
const DAT: &str = "947d5a35ff2fe522fda5b431af955e3b27955ebc18c9e3684b07b51ae112461f";
const XYZ: &str = "3608bca1e44ea6c4d268eb6db02260269892c0b42b86bbf1e77a6fa16c3c9282";
static SYNTHETIC: &[Release] = &[
    Release {
        id: "synthetic-cat-a",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_DOS,
        identity_files: &[],
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 3,
            sha256: ABC,
        }],
    },
    Release {
        id: "synthetic-cat-b",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_DOS,
        identity_files: &[],
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 3,
            sha256: DEF,
        }],
    },
    Release {
        id: "synthetic-mach",
        compatibility_id: "mach-3-dos",
        edition: &MACH3_DOS,
        identity_files: &[],
        files: &[
            FileRecord {
                name: "MACH3.EXE",
                size: 3,
                sha256: XYZ,
            },
            FileRecord {
                name: "DATA1",
                size: 3,
                sha256: DAT,
            },
        ],
    },
];

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "reforge-detection-{}-{suffix}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn detect(&self, path: &Path) -> Result<Detected, String> {
        detect_with(path, SYNTHETIC, file_size, hash_file)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn btz_games_require_distinct_files_for_all_three_records() {
    static FILES: &[FileRecord] = &[
        FileRecord {
            name: "ADISK.OBJ",
            size: 3,
            sha256: ABC,
        },
        FileRecord {
            name: "GO.EXE",
            size: 3,
            sha256: XYZ,
        },
        FileRecord {
            name: "SIDE.TXT",
            size: 3,
            sha256: ABC,
        },
    ];
    static CANDIDATES: &[Release] = &[
        Release {
            id: "synthetic-mindwheel",
            compatibility_id: "synthetic-mindwheel",
            edition: &catalog::MINDWHEEL_DOS,
            identity_files: &[],
            files: FILES,
        },
        Release {
            id: "synthetic-essex",
            compatibility_id: "synthetic-essex",
            edition: &catalog::ESSEX_DOS,
            identity_files: &[],
            files: FILES,
        },
        Release {
            id: "synthetic-brimstone",
            compatibility_id: "synthetic-brimstone",
            edition: &catalog::BRIMSTONE_DOS,
            identity_files: &[],
            files: FILES,
        },
    ];
    for candidate in CANDIDATES {
        let game = candidate.game();
        let release = std::slice::from_ref(candidate);
        let directory = TestDirectory::new();
        let data = directory.write("adisk.obj", b"abc");
        let executable = directory.write("Go.ExE", b"xyz");
        let missing = detect_with(&directory.0, release, file_size, hash_file).unwrap_err();
        assert!(missing.contains("SIDE.TXT"), "{missing}");
        let map = directory.write("SiDe.TxT", b"abc");
        for anchor in [&directory.0, &data, &executable, &map] {
            let result = detect_with(anchor, release, file_size, hash_file).unwrap();
            assert_eq!(result.game, game);
            assert_eq!(result.original, directory.0);
            assert_eq!(result.files["ADISK.OBJ"], data);
            assert_eq!(result.files["GO.EXE"], executable);
            assert_eq!(result.files["SIDE.TXT"], map);
        }
        for (contents, diagnostic) in [
            (b"bad".as_slice(), "SHA-256 mismatch"),
            (b"wrong size".as_slice(), "expected 3 bytes"),
        ] {
            fs::write(&map, contents).unwrap();
            let error = detect_with(&directory.0, release, file_size, hash_file).unwrap_err();
            assert!(
                error.contains("SiDe.TxT") && error.contains(diagnostic),
                "{error}"
            );
        }
    }
}

#[test]
fn identifies_multiple_releases_and_normalizes_alley_cat_to_file() {
    let directory = TestDirectory::new();
    let cat = directory.write("cAt.eXe", b"abc");
    directory.write("unrelated.txt", b"unrelated authored bytes");
    for path in [&directory.0, &cat] {
        let result = directory.detect(path).unwrap();
        assert_eq!(result.game, Game::AlleyCat);
        assert_eq!(result.release.id, "synthetic-cat-a");
        assert_eq!(result.original, cat);
    }
    fs::write(&cat, b"def").unwrap();
    assert_eq!(
        directory.detect(&cat).unwrap().release.id,
        "synthetic-cat-b"
    );
}

#[test]
fn mach_file_anchors_find_siblings_and_normalize_to_directory() {
    let directory = TestDirectory::new();
    let exe = directory.write("mach3.exe", b"xyz");
    let data = directory.write("dAtA1", b"dat");
    for path in [&directory.0, &exe, &data] {
        let result = directory.detect(path).unwrap();
        assert_eq!(result.game, Game::Mach3);
        assert_eq!(result.original, directory.0);
    }
}

#[test]
fn file_anchors_disambiguate_a_directory_containing_both_games() {
    let directory = TestDirectory::new();
    let cat = directory.write("CAT.EXE", b"abc");
    let mach = directory.write("MACH3.EXE", b"xyz");
    directory.write("DATA1", b"dat");
    assert!(
        directory
            .detect(&directory.0)
            .unwrap_err()
            .contains("Ambiguous")
    );
    assert_eq!(directory.detect(&cat).unwrap().game, Game::AlleyCat);
    assert_eq!(directory.detect(&mach).unwrap().game, Game::Mach3);
    let unrelated = directory.write("OTHER.EXE", b"bad");
    assert!(
        directory
            .detect(&unrelated)
            .unwrap_err()
            .contains("Unknown")
    );
}

#[test]
fn distinguishes_incomplete_unknown_size_and_hash_mismatches() {
    let directory = TestDirectory::new();
    assert!(
        directory
            .detect(&directory.0)
            .unwrap_err()
            .contains("Unknown")
    );
    let mach = directory.write("MACH3.EXE", b"xyz");
    let error = directory.detect(&mach).unwrap_err();
    assert!(
        error.contains("Incomplete") && error.contains("DATA1"),
        "{error}"
    );
    directory.write("DATA1", b"a");
    let error = directory.detect(&mach).unwrap_err();
    assert!(
        error.contains("mismatched") && error.contains("expected 3 bytes"),
        "{error}"
    );
    directory.write("DATA1", b"bad");
    let error = directory.detect(&mach).unwrap_err();
    assert!(
        error.contains("mismatched") && error.contains("SHA-256 mismatch"),
        "{error}"
    );
}

#[test]
fn incomplete_other_game_does_not_prevent_a_verified_match() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("MACH3.EXE", b"xyz");
    assert_eq!(directory.detect(&directory.0).unwrap().game, Game::AlleyCat);
}

#[test]
fn malformed_unrelated_candidates_do_not_hide_a_verified_game() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("MACH3.EXE", b"xyz");
    directory.write("DATA1", b"bad");
    directory.write("data1", b"bad");
    if fs::read_dir(&directory.0).unwrap().count() == 4 {
        assert_eq!(directory.detect(&directory.0).unwrap().game, Game::AlleyCat);
    }
    fs::remove_file(directory.0.join("DATA1")).unwrap();
    let other = directory.0.join("data1");
    if other.exists() {
        fs::remove_file(&other).unwrap();
    }
    fs::create_dir(directory.0.join("DATA1")).unwrap();
    assert_eq!(directory.detect(&directory.0).unwrap().game, Game::AlleyCat);
}

#[test]
fn exact_case_is_preferred_then_alphabetical_variants_with_directory_or_file_input() {
    let directory = TestDirectory::new();
    let exact = directory.write("CAT.EXE", b"abc");
    let lower = directory.write("cat.exe", b"abc");
    let mixed = directory.write("Cat.Exe", b"abc");
    if fs::read_dir(&directory.0).unwrap().count() != 3 {
        return;
    }
    for input in [&directory.0, &exact] {
        let mut hashed = Vec::new();
        let result = detect_with(input, &SYNTHETIC[..1], file_size, |path| {
            hashed.push(path.to_owned());
            hash_file(path)
        })
        .unwrap();
        assert_eq!(hashed, std::slice::from_ref(&exact));
        assert_eq!(result.original, exact);
        assert_eq!(result.files["CAT.EXE"], exact);
    }
    fs::write(&exact, b"bad").unwrap();
    {
        let input = &directory.0;
        let mut hashed = Vec::new();
        let result = detect_with(input, &SYNTHETIC[..1], file_size, |path| {
            hashed.push(path.to_owned());
            hash_file(path)
        })
        .unwrap();
        assert_eq!(hashed, [exact.clone(), mixed.clone()]);
        assert_eq!(result.original, mixed);
    }
    fs::remove_file(&exact).unwrap();
    fs::write(&mixed, b"bad").unwrap();
    let mut hashed = Vec::new();
    let result = detect_with(&directory.0, &SYNTHETIC[..1], file_size, |path| {
        hashed.push(path.to_owned());
        hash_file(path)
    })
    .unwrap();
    assert_eq!(hashed, [mixed, lower.clone()]);
    assert_eq!(result.original, lower);
}

#[test]
fn case_variants_skip_wrong_sizes_and_nonfiles_before_hashing() {
    let directory = TestDirectory::new();
    let exact = directory.write("CAT.EXE", b"wrong size");
    let lower = directory.write("cat.exe", b"abc");
    if fs::read_dir(&directory.0).unwrap().count() != 2 {
        return;
    }
    for nonfile in [false, true] {
        if nonfile {
            fs::remove_file(&exact).unwrap();
            fs::create_dir(&exact).unwrap();
        }
        let mut hashed = Vec::new();
        let result = detect_with(&directory.0, &SYNTHETIC[..1], file_size, |path| {
            hashed.push(path.to_owned());
            hash_file(path)
        })
        .unwrap();
        assert_eq!(hashed, std::slice::from_ref(&lower));
        assert_eq!(result.original, lower);
    }
}

#[test]
fn multifile_case_variants_are_verified_independently_and_returned_to_loader() {
    let directory = TestDirectory::new();
    directory.write("MACH3.EXE", b"xyz");
    directory.write("DATA1", b"bad");
    let data = directory.write("data1", b"dat");
    if fs::read_dir(&directory.0).unwrap().count() != 3 {
        return;
    }
    let result = directory.detect(&directory.0).unwrap();
    assert_eq!(result.game, Game::Mach3);
    assert_eq!(result.files["DATA1"], data);
}

#[test]
fn case_variants_continue_after_read_errors_and_report_all_failed_hashes() {
    let directory = TestDirectory::new();
    let exact = directory.write("CAT.EXE", b"abc");
    let lower = directory.write("cat.exe", b"abc");
    if fs::read_dir(&directory.0).unwrap().count() != 2 {
        return;
    }
    let result = detect_with(&directory.0, &SYNTHETIC[..1], file_size, |path| {
        if path == exact {
            Err("simulated unreadable exact spelling".into())
        } else {
            hash_file(path)
        }
    })
    .unwrap();
    assert_eq!(result.original, lower);
    fs::write(&exact, b"bad").unwrap();
    fs::write(&lower, b"bad").unwrap();
    let error = directory.detect(&directory.0).unwrap_err();
    assert!(
        error.contains("CAT.EXE SHA-256 mismatch") && error.contains("cat.exe SHA-256 mismatch")
    );
}

#[test]
fn different_releases_in_case_variants_remain_ambiguous() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("cat.exe", b"def");
    if fs::read_dir(&directory.0).unwrap().count() != 2 {
        return;
    }
    let error = directory.detect(&directory.0).unwrap_err();
    assert!(
        error.contains("Ambiguous original data")
            && error.contains("synthetic-cat-a")
            && error.contains("synthetic-cat-b")
    );
}

#[test]
fn insufficient_file_counts_are_rejected_before_hashing() {
    let directory = TestDirectory::new();
    directory.write("MACH3.EXE", b"xyz");
    let result = detect_with(&directory.0, &SYNTHETIC[2..], file_size, |_| {
        panic!("must not hash incomplete candidates")
    });
    assert!(result.unwrap_err().contains("Incomplete"));
}

#[test]
fn size_filters_all_candidates_before_any_hash_reads() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    directory.write("MACH3.EXE", b"xyz");
    directory.write("DATA1", b"abcd");
    let sizes_read = std::cell::Cell::new(0);
    let hashes_read = std::cell::Cell::new(0);
    let result = detect_with(
        &directory.0,
        SYNTHETIC,
        |path| {
            sizes_read.set(sizes_read.get() + 1);
            file_size(path)
        },
        |path| {
            assert_eq!(
                sizes_read.get(),
                3,
                "all candidates must have passed the size phase"
            );
            assert_ne!(
                path.file_name().unwrap(),
                "DATA1",
                "wrong-sized files must never be hashed"
            );
            hashes_read.set(hashes_read.get() + 1);
            hash_file(path)
        },
    )
    .unwrap();
    assert_eq!(result.game, Game::AlleyCat);
    assert_eq!(
        hashes_read.get(),
        2,
        "same-size candidates are hashed at most once across releases"
    );
}

#[test]
fn no_hashes_are_read_when_all_candidates_fail_size_checks() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abcd");
    directory.write("MACH3.EXE", b"wrong size");
    directory.write("DATA1", b"abcd");
    let result = detect_with(&directory.0, SYNTHETIC, file_size, |_| {
        panic!("size-mismatched candidates must not be hashed")
    });
    assert!(result.unwrap_err().contains("mismatched"));
}

#[test]
fn matching_multiple_release_records_is_ambiguous() {
    static OVERLAPPING: &[Release] = &[
        Release {
            id: "first",
            compatibility_id: "alley-cat-dos",
            edition: &ALLEY_CAT_DOS,
            identity_files: &[],
            files: &[FileRecord {
                name: "CAT.EXE",
                size: 3,
                sha256: ABC,
            }],
        },
        Release {
            id: "second",
            compatibility_id: "alley-cat-dos",
            edition: &ALLEY_CAT_DOS,
            identity_files: &[],
            files: &[FileRecord {
                name: "CAT.EXE",
                size: 3,
                sha256: ABC,
            }],
        },
    ];
    let directory = TestDirectory::new();
    let cat = directory.write("CAT.EXE", b"abc");
    let mut reads = 0;
    let result = detect_with(&cat, OVERLAPPING, file_size, |path| {
        reads += 1;
        hash_file(path)
    });
    let error = result.unwrap_err();
    assert!(error.contains("Ambiguous") && error.contains("first") && error.contains("second"));
    assert_eq!(reads, 1);
}

#[test]
fn missing_paths_and_directories_named_like_files_are_rejected() {
    let directory = TestDirectory::new();
    let missing = directory.0.join("absent");
    assert!(
        directory
            .detect(&missing)
            .unwrap_err()
            .contains("Cannot inspect original path")
    );
    fs::create_dir(directory.0.join("CAT.EXE")).unwrap();
    assert!(
        directory
            .detect(&directory.0)
            .unwrap_err()
            .contains("not a regular file")
    );
}

#[test]
fn hash_cache_is_scoped_to_one_detection_call() {
    let directory = TestDirectory::new();
    let cat = directory.write("CAT.EXE", b"abc");
    assert!(directory.detect(&cat).is_ok());
    fs::write(&cat, b"bad").unwrap();
    assert!(
        directory
            .detect(&cat)
            .unwrap_err()
            .contains("SHA-256 mismatch")
    );
}

#[test]
fn public_catalog_describes_unique_games_and_well_formed_fingerprints() {
    assert_eq!(RELEASES.len(), 6);
    let mut ids = std::collections::HashSet::new();
    for release in RELEASES {
        assert!(ids.insert(release.id), "duplicate detected-release ID");
        assert_eq!(release.id, identity::release_id(release).unwrap());
        assert!(!release.game().id().is_empty());
        for file in release.files {
            assert!(file.size > 0);
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(file.sha256, file.sha256.to_ascii_lowercase());
            assert_eq!(
                release
                    .files
                    .iter()
                    .filter(|other| other.name.eq_ignore_ascii_case(file.name))
                    .count(),
                1
            );
        }
    }
    let mach = RELEASES
        .iter()
        .find(|release| release.game() == Game::Mach3)
        .unwrap();
    assert_eq!(mach.files.len(), 18);
    assert_eq!(mach.identity_files.len(), 7);
    let mindwheel = RELEASES
        .iter()
        .find(|release| release.game() == Game::Mindwheel)
        .unwrap();
    assert_eq!(
        mindwheel
            .files
            .iter()
            .map(|file| file.name)
            .collect::<Vec<_>>(),
        ["ADISK.OBJ", "GO.EXE", "SIDE.TXT"]
    );
    let breakers = RELEASES
        .iter()
        .find(|release| release.game() == Game::Breakers)
        .unwrap();
    assert_eq!(
        breakers
            .files
            .iter()
            .map(|file| file.name)
            .collect::<Vec<_>>(),
        ["ADISK.OBJ", "RUN.EXE", "SIDE.TXT", "TITLE.EXE"]
    );
}

#[test]
fn arbitrary_names_extensions_and_swapped_names_use_verified_contents() {
    for names in [
        ["start.exe", "DATA1"],
        ["MACH3.EXE", "saved.bin"],
        ["first", "second"],
        ["DATA1", "MACH3.EXE"],
    ] {
        let directory = TestDirectory::new();
        let exe = directory.write(names[0], b"xyz");
        let data = directory.write(names[1], b"dat");
        directory.write("unrelated", b"not a catalog file");
        for input in [&directory.0, &exe, &data] {
            let detected = directory.detect(input).unwrap();
            assert_eq!(detected.release.id, "synthetic-mach");
            assert_eq!(detected.files["MACH3.EXE"], exe);
            assert_eq!(detected.files["DATA1"], data);
        }
    }
}

#[test]
fn corrupt_expected_names_do_not_hide_renamed_verified_files() {
    let directory = TestDirectory::new();
    directory.write("MACH3.EXE", b"bad");
    directory.write("DATA1", b"wrong size");
    let exe = directory.write("start.bin", b"xyz");
    let data = directory.write("resources", b"dat");
    let detected = directory.detect(&directory.0).unwrap();
    assert_eq!(detected.files["MACH3.EXE"], exe);
    assert_eq!(detected.files["DATA1"], data);
    assert!(directory.detect(&directory.0.join("MACH3.EXE")).is_err());
}

#[test]
fn named_fast_path_avoids_hashing_duplicate_and_unrelated_files() {
    let directory = TestDirectory::new();
    let exe = directory.write("MACH3.EXE", b"xyz");
    let data = directory.write("DATA1", b"dat");
    directory.write("backup", b"xyz");
    directory.write("unrelated", b"bad");
    directory.write("large", &[0; 100]);
    let mut read = Vec::new();
    let detected = detect_with(&directory.0, &SYNTHETIC[2..], file_size, |path| {
        read.push(path.to_owned());
        hash_file(path)
    })
    .unwrap();
    assert_eq!(read, [exe.clone(), data.clone()]);
    assert_eq!(detected.files["MACH3.EXE"], exe);
    assert_eq!(detected.files["DATA1"], data);
}

#[test]
fn renamed_fallback_is_deterministic_and_hashes_each_path_at_most_once() {
    let directory = TestDirectory::new();
    let first = directory.write("a-copy", b"abc");
    directory.write("z-copy", b"abc");
    directory.write("not-supported", b"bad");
    let mut counts = BTreeMap::new();
    let detected = detect_with(&directory.0, &SYNTHETIC[..2], file_size, |path| {
        *counts.entry(path.to_owned()).or_insert(0) += 1;
        hash_file(path)
    })
    .unwrap();
    assert_eq!(detected.original, first);
    assert!(counts.values().all(|count| *count == 1));
}

#[test]
fn renamed_other_release_remains_ambiguous_even_with_a_complete_named_match() {
    let directory = TestDirectory::new();
    let cat = directory.write("CAT.EXE", b"abc");
    let other = directory.write("alternate", b"def");
    assert!(
        directory
            .detect(&directory.0)
            .unwrap_err()
            .contains("Ambiguous")
    );
    assert_eq!(
        directory.detect(&cat).unwrap().release.id,
        "synthetic-cat-a"
    );
    assert_eq!(
        directory.detect(&other).unwrap().release.id,
        "synthetic-cat-b"
    );
}

#[test]
fn explicit_file_is_verified_and_used_instead_of_an_identical_named_copy() {
    let directory = TestDirectory::new();
    directory.write("CAT.EXE", b"abc");
    let copy = directory.write("start", b"abc");
    let detected = directory.detect(&copy).unwrap();
    assert_eq!(detected.original, copy);
    assert_eq!(detected.files["CAT.EXE"], copy);
    fs::write(&copy, b"bad").unwrap();
    assert!(
        directory.detect(&copy).is_err(),
        "must not silently use CAT.EXE instead"
    );
}

#[test]
fn identical_required_contents_still_need_distinct_files_and_preserve_named_roles() {
    static RELEASE: &[Release] = &[Release {
        id: "two-identical-files",
        compatibility_id: "synthetic",
        edition: &MACH3_DOS,
        identity_files: &[],
        files: &[
            FileRecord {
                name: "A",
                size: 3,
                sha256: ABC,
            },
            FileRecord {
                name: "B",
                size: 3,
                sha256: ABC,
            },
        ],
    }];
    let directory = TestDirectory::new();
    let b = directory.write("B", b"abc");
    assert!(detect_with(&directory.0, RELEASE, file_size, hash_file).is_err());
    let other = directory.write("z", b"abc");
    for anchor in [&directory.0, &b, &other] {
        let detected = detect_with(anchor, RELEASE, file_size, hash_file).unwrap();
        assert_eq!(detected.files["A"], other);
        assert_eq!(detected.files["B"], b);
    }
}

#[test]
fn renamed_file_in_a_subdirectory_does_not_satisfy_a_required_record() {
    let directory = TestDirectory::new();
    fs::create_dir(directory.0.join("nested")).unwrap();
    fs::write(directory.0.join("nested/start"), b"abc").unwrap();
    assert!(directory.detect(&directory.0).is_err());
}

#[cfg(unix)]
#[test]
fn detection_matches_non_utf8_aliases_by_contents() {
    use std::os::unix::ffi::OsStringExt;
    let directory = TestDirectory::new();
    let path = directory
        .0
        .join(std::ffi::OsString::from_vec(vec![b'g', 0xff]));
    fs::write(&path, b"abc").unwrap();
    for input in [&directory.0, &path] {
        assert_eq!(directory.detect(input).unwrap().original, path);
    }
}

#[test]
fn stable_identity_selection_never_exempts_other_required_files_from_verification() {
    static RELEASE: &[Release] = &[Release {
        id: "stable-legacy-id",
        compatibility_id: "synthetic",
        edition: &MACH3_DOS,
        identity_files: &["MACH3.EXE"],
        files: &[
            FileRecord {
                name: "MACH3.EXE",
                size: 3,
                sha256: ABC,
            },
            FileRecord {
                name: "SOUND",
                size: 3,
                sha256: XYZ,
            },
        ],
    }];
    let directory = TestDirectory::new();
    directory.write("start", b"abc");
    assert!(detect_with(&directory.0, RELEASE, file_size, hash_file).is_err());
    let sound = directory.write("audio.blob", b"bad");
    assert!(detect_with(&directory.0, RELEASE, file_size, hash_file).is_err());
    fs::write(&sound, b"xyz").unwrap();
    let detected = detect_with(&directory.0, RELEASE, file_size, hash_file).unwrap();
    assert_eq!(detected.release.id, "stable-legacy-id");
    assert_eq!(detected.files["SOUND"], sound);
}
