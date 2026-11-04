// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

static STRUCTURED: &[Release] = &[Release {
    id: "synthetic-structured",
    compatibility_id: "mach-3-dos",
    edition: &MACH3_DOS,
    identity_files: &[],
    files: &[
        FileRecord {
            name: "GAME.EXE",
            size: 3,
            sha256: XYZ,
        },
        FileRecord {
            name: "DATA/ROOMS.DAT",
            size: 3,
            sha256: DAT,
        },
        FileRecord {
            name: "AUDIO/MUSIC.DAT",
            size: 3,
            sha256: ABC,
        },
    ],
}];

#[test]
fn renamed_files_can_move_between_root_and_each_catalog_directory() {
    // Exercise every placement across the three permitted locations, retaining
    // one file per location so nested file selections have a unique game root.
    for locations in [
        ["", "DATA", "AUDIO"],
        ["", "AUDIO", "DATA"],
        ["DATA", "", "AUDIO"],
        ["DATA", "AUDIO", ""],
        ["AUDIO", "", "DATA"],
        ["AUDIO", "DATA", ""],
    ] {
        let directory = TestDirectory::new();
        let paths = [b"xyz", b"dat", b"abc"]
            .iter()
            .enumerate()
            .map(|(i, bytes)| {
                directory.write(
                    format!("{}/chunk-{i}", locations[i]).trim_start_matches('/'),
                    *bytes,
                )
            })
            .collect::<Vec<_>>();
        for input in std::iter::once(&directory.0).chain(paths.iter()) {
            let found = detect_with(input, STRUCTURED, file_size, hash_file).unwrap();
            assert_eq!(found.root, directory.0);
            assert_eq!(found.original, directory.0);
            for (file, path) in STRUCTURED[0].files.iter().zip(&paths) {
                assert_eq!(found.files[file.name], *path);
            }
        }
    }
}

#[test]
fn missing_catalog_directories_are_optional_when_files_have_moved_to_root() {
    let directory = TestDirectory::new();
    directory.write("start", b"xyz");
    directory.write("rooms", b"dat");
    directory.write("music", b"abc");
    assert_eq!(
        detect_with(&directory.0, STRUCTURED, file_size, hash_file)
            .unwrap()
            .root,
        directory.0
    );
}

#[test]
fn nested_directory_components_and_file_anchors_ignore_ascii_case() {
    static NESTED: &[Release] = &[Release {
        files: &[
            STRUCTURED[0].files[0],
            STRUCTURED[0].files[1],
            FileRecord {
                name: "AUDIO/DISC1/MUSIC.DAT",
                size: 3,
                sha256: ABC,
            },
        ],
        ..STRUCTURED[0]
    }];
    let directory = TestDirectory::new();
    directory.write("game.exe", b"xyz");
    directory.write("dAtA/rooms.dat", b"dat");
    let music = directory.write("audio/disc1/renamed", b"abc");
    for input in [&directory.0, &music] {
        let found = detect_with(input, NESTED, file_size, hash_file).unwrap();
        assert_eq!(found.root, directory.0);
        assert_eq!(found.files["AUDIO/DISC1/MUSIC.DAT"], music);
    }
    assert!(
        detect_with(music.parent().unwrap(), NESTED, file_size, hash_file).is_err(),
        "a directory selection must not search ancestors"
    );
    // Traversing AUDIO to reach DISC1 does not authorize files directly in AUDIO.
    fs::rename(&music, directory.0.join("audio/MUSIC.DAT")).unwrap();
    assert!(detect_with(&directory.0, NESTED, file_size, hash_file).is_err());
}

#[test]
fn unrelated_directories_and_unlisted_descendants_are_never_searched() {
    for hidden in ["UNKNOWN", "DATA/UNKNOWN", "AUDIO/UNKNOWN"] {
        let directory = TestDirectory::new();
        directory.write("GAME.EXE", b"xyz");
        directory.write("DATA/ROOMS.DAT", b"dat");
        directory.write(&format!("{hidden}/MUSIC.DAT"), b"abc");
        let mut inspected = Vec::new();
        assert!(
            detect_with(
                &directory.0,
                STRUCTURED,
                |path| {
                    inspected.push(path.to_owned());
                    file_size(path)
                },
                hash_file
            )
            .is_err()
        );
        assert!(!inspected.iter().any(|path| path.ends_with("MUSIC.DAT")));
    }
}

#[test]
fn another_releases_directories_cannot_broaden_the_search() {
    static CATALOG: &[Release] = &[
        Release {
            files: &[STRUCTURED[0].files[0], STRUCTURED[0].files[1]],
            ..STRUCTURED[0]
        },
        Release {
            id: "unrelated",
            files: &[FileRecord {
                name: "OTHER/FILE",
                size: 3,
                sha256: DEF,
            }],
            ..STRUCTURED[0]
        },
    ];
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("OTHER/ROOMS.DAT", b"dat");
    assert!(detect_with(&directory.0, CATALOG, file_size, hash_file).is_err());
}

#[test]
fn expected_full_paths_are_reserved_before_same_basename_or_renamed_copies() {
    static IDENTICAL: &[Release] = &[Release {
        files: &[
            STRUCTURED[0].files[0],
            FileRecord {
                name: "DATA/SHARED",
                size: 3,
                sha256: ABC,
            },
            FileRecord {
                name: "AUDIO/SHARED",
                size: 3,
                sha256: ABC,
            },
        ],
        ..STRUCTURED[0]
    }];
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    let audio = directory.write("AUDIO/SHARED", b"abc");
    let renamed = directory.write("DATA/renamed", b"abc");
    let found = detect_with(&directory.0, IDENTICAL, file_size, hash_file).unwrap();
    assert_eq!(found.files["AUDIO/SHARED"], audio);
    assert_eq!(found.files["DATA/SHARED"], renamed);
    let data = directory.write("DATA/SHARED", b"abc");
    directory.write("SHARED", b"abc");
    let mut hashed = Vec::new();
    let found = detect_with(&directory.0, IDENTICAL, file_size, |path| {
        hashed.push(path.to_owned());
        hash_file(path)
    })
    .unwrap();
    assert_eq!(found.files["DATA/SHARED"], data);
    assert_eq!(found.files["AUDIO/SHARED"], audio);
    assert_eq!(hashed.len(), 3, "the expected paths are the fast path");
}

#[test]
fn nested_selected_file_must_itself_verify() {
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("DATA/ROOMS.DAT", b"dat");
    directory.write("AUDIO/MUSIC.DAT", b"abc");
    let bad = directory.write("DATA/unrelated", b"bad");
    assert!(detect_with(&bad, STRUCTURED, file_size, hash_file).is_err());
}

#[test]
fn ambiguous_game_roots_are_reported_instead_of_guessed() {
    let directory = TestDirectory::new();
    let exe = directory.write("DATA/start", b"xyz");
    directory.write("DATA/rooms", b"dat");
    directory.write("DATA/music", b"abc");
    let error = detect_with(&exe, STRUCTURED, file_size, hash_file).unwrap_err();
    assert!(error.contains("Ambiguous"), "{error}");
    assert!(
        error.contains(&format!("at {}", directory.0.display())),
        "{error}"
    );
    assert!(
        error.contains(&format!("at {}", directory.0.join("DATA").display())),
        "{error}"
    );
    assert!(detect_with(&directory.0, STRUCTURED, file_size, hash_file).is_ok());
}

#[test]
fn malformed_catalog_paths_cannot_escape_the_root() {
    for name in [
        "",
        "/GAME",
        "../GAME",
        "DATA/../../GAME",
        "DATA//GAME",
        "DATA/./GAME",
        "C:/GAME",
        "DATA\\GAME",
        "GAME/",
    ] {
        let files = [FileRecord {
            name,
            size: 3,
            sha256: ABC,
        }];
        assert!(search::catalog_directories(&files).is_err(), "{name:?}");
    }
}

#[cfg(unix)]
#[test]
fn case_variant_directories_are_searched_independently() {
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    let exact = directory.write("DATA/ROOMS.DAT", b"dat");
    let lower = directory.write("data/rooms.dat", b"dat");
    directory.write("AUDIO/MUSIC.DAT", b"abc");
    if exact.canonicalize().unwrap() == lower.canonicalize().unwrap() {
        return;
    }
    let found = detect_with(&directory.0, STRUCTURED, file_size, hash_file).unwrap();
    assert_eq!(found.files["DATA/ROOMS.DAT"], exact);
    fs::write(&exact, b"bad").unwrap();
    for input in [&directory.0, &lower] {
        assert_eq!(
            detect_with(input, STRUCTURED, file_size, hash_file)
                .unwrap()
                .files["DATA/ROOMS.DAT"],
            lower
        );
    }
}

#[cfg(unix)]
#[test]
fn directory_and_file_symlinks_keep_the_logical_companion_layout() {
    let directory = TestDirectory::new();
    let external = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("AUDIO/MUSIC.DAT", b"abc");
    let target = external.write("physical-rooms", b"dat");
    fs::create_dir(directory.0.join("DATA")).unwrap();
    let link = directory.0.join("DATA/ROOMS.DAT");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(
        detect_with(&link, STRUCTURED, file_size, hash_file)
            .unwrap()
            .root,
        directory.0
    );
    fs::remove_file(&link).unwrap();
    fs::remove_dir(directory.0.join("DATA")).unwrap();
    std::os::unix::fs::symlink(&external.0, directory.0.join("DATA")).unwrap();
    let nested = directory.0.join("DATA/physical-rooms");
    let found = detect_with(&nested, STRUCTURED, file_size, hash_file).unwrap();
    assert_eq!(found.root, directory.0);
    assert_eq!(found.files["DATA/ROOMS.DAT"], nested);
}

#[cfg(unix)]
#[test]
fn directory_aliases_cannot_count_one_file_as_two_required_records() {
    static IDENTICAL: &[Release] = &[Release {
        files: &[
            STRUCTURED[0].files[0],
            FileRecord {
                name: "DATA/ONE",
                size: 3,
                sha256: ABC,
            },
            FileRecord {
                name: "AUDIO/TWO",
                size: 3,
                sha256: ABC,
            },
        ],
        ..STRUCTURED[0]
    }];
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("DATA/ONE", b"abc");
    std::os::unix::fs::symlink(directory.0.join("DATA"), directory.0.join("AUDIO")).unwrap();
    assert!(detect_with(&directory.0, IDENTICAL, file_size, hash_file).is_err());
    directory.write("DATA/TWO", b"abc");
    assert!(detect_with(&directory.0, IDENTICAL, file_size, hash_file).is_ok());
}

#[cfg(unix)]
#[test]
fn root_aliases_do_not_create_false_ambiguity() {
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("ROOMS.DAT", b"dat");
    directory.write("AUDIO/MUSIC.DAT", b"abc");
    std::os::unix::fs::symlink(&directory.0, directory.0.join("DATA")).unwrap();
    let selected = directory.0.join("DATA/ROOMS.DAT");
    let found = detect_with(&selected, STRUCTURED, file_size, hash_file).unwrap();
    assert_eq!(
        found.root.canonicalize().unwrap(),
        directory.0.canonicalize().unwrap()
    );
}

#[test]
fn metadata_and_hashes_are_cached_across_releases_sharing_catalog_folders() {
    static SHARED: &[Release] = &[
        Release {
            id: "first",
            ..STRUCTURED[0]
        },
        Release {
            id: "second",
            ..STRUCTURED[0]
        },
    ];
    let directory = TestDirectory::new();
    directory.write("GAME.EXE", b"xyz");
    directory.write("DATA/ROOMS.DAT", b"dat");
    directory.write("AUDIO/MUSIC.DAT", b"abc");
    let mut sizes = HashMap::new();
    let mut hashes = HashMap::new();
    let error = detect_with(
        &directory.0,
        SHARED,
        |path| {
            *sizes.entry(path.to_owned()).or_insert(0) += 1;
            file_size(path)
        },
        |path| {
            *hashes.entry(path.to_owned()).or_insert(0) += 1;
            hash_file(path)
        },
    )
    .unwrap_err();
    assert!(error.contains("Ambiguous"), "{error}");
    assert!(sizes.values().all(|count| *count == 1));
    assert_eq!(hashes.len(), 3);
    assert!(hashes.values().all(|count| *count == 1));
}
