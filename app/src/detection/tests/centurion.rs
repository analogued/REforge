// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{collections::HashSet, sync::LazyLock};

fn production() -> &'static Release {
    RELEASES
        .iter()
        .find(|release| release.game() == Game::Centurion)
        .unwrap()
}

fn authored_bytes(index: usize, role: &str) -> Vec<u8> {
    format!("Authored Centurion detector fixture {index:02}: {role}\n").into_bytes()
}

fn synthetic() -> &'static [Release] {
    // The detector's catalog has process lifetime. These fixture fingerprints
    // are generated once from authored text, never original asset contents.
    static CATALOG: LazyLock<Vec<Release>> = LazyLock::new(|| {
        let files = production()
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let bytes = authored_bytes(index, file.name);
                FileRecord {
                    name: file.name,
                    size: bytes.len() as u64,
                    blake3: Box::leak(blake3::hash(&bytes).to_hex().to_string().into_boxed_str()),
                }
            })
            .collect::<Vec<_>>();
        vec![Release {
            id: "synthetic-centurion",
            compatibility_id: "synthetic-centurion",
            edition: &catalog::CENTURION_DOS,
            identity_files: &[],
            files: Box::leak(files.into_boxed_slice()),
        }]
    });
    CATALOG.as_slice()
}

fn fixture(renamed: bool) -> (TestDirectory, Vec<PathBuf>) {
    let directory = TestDirectory::new();
    let paths = production()
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let alias = format!("part-{index:02}.original");
            directory.write(
                if renamed { &alias } else { file.name },
                &authored_bytes(index, file.name),
            )
        })
        .collect();
    (directory, paths)
}

#[test]
fn centurion_catalog_identity_covers_every_runtime_role() {
    let release = production();
    assert_eq!(release.id, "centurion-defender-of-rome-dos-b82e91fe62");
    assert_eq!(identity::release_id(release).unwrap(), release.id);
    assert_eq!(
        release.compatibility_id,
        "centurion-defender-of-rome-dos-100a"
    );
    assert_eq!(
        release.engine(),
        Engine::Centurion(::centurion::assets::DosRelease::V100A)
    );
    assert_eq!(
        release.game().engine(),
        Engine::Centurion(::centurion::assets::DosRelease::V100A)
    );
    assert_eq!(release.game().id(), "centurion-defender-of-rome");
    assert!(!release.game().is_btz());
    assert!(release.identity_files.is_empty());
    let names: HashSet<_> = release.files.iter().map(|file| file.name).collect();
    let mut expected = HashSet::from(["OVL0.OVL".to_string()]);
    for bank in [
        "BATL", "CENT", "MGLAD", "MINIT", "MMAP", "MNAVAL", "MNEGO", "MRACE", "NAVL",
    ] {
        for extension in ["DAT", "DIR"] {
            expected.insert(format!("{bank}.{extension}"));
        }
    }
    assert_eq!(names.len(), 19);
    assert_eq!(names, expected.iter().map(String::as_str).collect());
    assert!(!names.contains("CENTURIO.EXE"));
    assert!(
        release
            .files
            .iter()
            .all(|file| !file.name.ends_with(".CSV"))
    );
}

#[test]
fn centurion_authored_resources_keep_resolved_paths_when_every_file_is_renamed() {
    for renamed in [false, true] {
        let (directory, paths) = fixture(renamed);
        for input in std::iter::once(&directory.0).chain(paths.iter()) {
            let found = detect_with(input, synthetic(), file_size, hash_file).unwrap();
            assert_eq!(found.game, Game::Centurion);
            assert_eq!(
                found.engine(),
                Engine::Centurion(::centurion::assets::DosRelease::V100A)
            );
            assert_eq!(found.original, directory.0);
            assert_eq!(found.files.len(), 19);
            let resolved = found.inputs();
            for (record, actual) in production().files.iter().zip(&paths) {
                assert_eq!(&found.files[record.name], actual);
                assert_eq!(resolved.path(record.name).unwrap(), actual);
            }
        }
        // An unused, invalid DOS launcher does not become a requirement.
        directory.write("CENTURIO.EXE", b"not a DOS executable");
        assert_eq!(
            detect_with(&directory.0, synthetic(), file_size, hash_file)
                .unwrap()
                .files
                .len(),
            19
        );
    }
}

#[test]
fn every_centurion_authored_role_is_required_and_fully_hashed() {
    let (directory, paths) = fixture(false);
    for (index, file) in production().files.iter().enumerate() {
        let path = &paths[index];
        let bytes = authored_bytes(index, file.name);
        fs::remove_file(path).unwrap();
        let missing = detect_with(&directory.0, synthetic(), file_size, hash_file).unwrap_err();
        assert!(missing.contains(file.name), "{missing}");
        let mut corrupted = bytes.clone();
        corrupted[0] ^= 1;
        fs::write(path, corrupted).unwrap();
        let bad_hash = detect_with(&directory.0, synthetic(), file_size, hash_file).unwrap_err();
        assert!(
            bad_hash.contains(file.name) && bad_hash.contains("BLAKE3 mismatch"),
            "{bad_hash}"
        );
        fs::write(path, b"wrong size").unwrap();
        let bad_size = detect_with(&directory.0, synthetic(), file_size, hash_file).unwrap_err();
        assert!(
            bad_size.contains(file.name) && bad_size.contains("expected"),
            "{bad_size}"
        );
        fs::write(path, bytes).unwrap();
    }
    let mut hashed = HashSet::new();
    let detected = detect_with(&directory.0, synthetic(), file_size, |path| {
        hashed.insert(path.to_path_buf());
        hash_file(path)
    })
    .unwrap();
    assert_eq!(detected.files.len(), 19);
    assert_eq!(hashed, paths.into_iter().collect());
}

#[test]
fn production_centurion_routing_uses_the_complete_catalog_manifest() {
    let release = production();
    let directory = TestDirectory::new();
    let paths = release
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            directory.write(
                &format!("chunk-{index:02}"),
                &authored_bytes(index, file.name),
            )
        })
        .collect::<Vec<_>>();
    // Exercise the actual release routing using authored files and synthetic
    // I/O metadata. No original resources or machine-specific paths are fixtures.
    let record = |path: &Path| {
        paths
            .iter()
            .position(|actual| actual == path)
            .map(|index| release.files[index])
    };
    let mut hashed = HashSet::new();
    let found = detect_with(
        &directory.0,
        RELEASES,
        |path| Ok(record(path).unwrap().size),
        |path| {
            hashed.insert(path.to_owned());
            Ok(record(path).unwrap().blake3.to_owned())
        },
    )
    .unwrap();
    assert_eq!(found.release.id, release.id);
    assert_eq!(
        found.engine(),
        Engine::Centurion(::centurion::assets::DosRelease::V100A)
    );
    assert_eq!(found.files.len(), release.files.len());
    assert_eq!(hashed, paths.iter().cloned().collect());
    for (file, actual) in release.files.iter().zip(&paths) {
        assert_eq!(&found.files[file.name], actual);
    }
}

fn production_b() -> &'static Release {
    RELEASES
        .iter()
        .find(|release| {
            release.engine() == Engine::Centurion(::centurion::assets::DosRelease::V100B)
        })
        .unwrap()
}

fn synthetic_b() -> &'static [Release] {
    static CATALOG: LazyLock<Vec<Release>> = LazyLock::new(|| {
        let files = production_b()
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let bytes = authored_bytes(index, file.name);
                FileRecord {
                    name: file.name,
                    size: bytes.len() as u64,
                    blake3: Box::leak(blake3::hash(&bytes).to_hex().to_string().into_boxed_str()),
                }
            })
            .collect::<Vec<_>>();
        vec![Release {
            id: "synthetic-centurion-100b",
            compatibility_id: "synthetic-centurion",
            edition: &catalog::CENTURION_DOS_100B,
            identity_files: &[],
            files: Box::leak(files.into_boxed_slice()),
        }]
    });
    CATALOG.as_slice()
}

fn fixture_b(renamed: bool) -> (TestDirectory, Vec<PathBuf>) {
    let directory = TestDirectory::new();
    let paths = production_b()
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            directory.write(
                &if renamed {
                    format!("b-part-{index:02}.original")
                } else {
                    file.name.to_lowercase()
                },
                &authored_bytes(index, file.name),
            )
        })
        .collect();
    (directory, paths)
}

#[test]
fn centurion_100b_has_a_distinct_profile_and_complete_vga_ega_manifest() {
    let a = production();
    let b = production_b();
    assert_eq!(b.id, "centurion-defender-of-rome-dos-4d5fd76b5e");
    assert_eq!(identity::release_id(b).unwrap(), b.id);
    assert_eq!(b.edition.label, Some("DOS 1.00B"));
    assert_eq!(b.edition.specs.video_features, ["320×200", "VGA", "EGA"]);
    assert_eq!(b.files.len(), 31);
    assert!(a.compatible_with(b) && b.compatible_with(a));
    let names: HashSet<_> = b.files.iter().map(|file| file.name).collect();
    let mut expected: HashSet<String> = a.files.iter().map(|file| file.name.into()).collect();
    for bank in ["EINIT", "EMAP", "EGLAD", "ERACE", "ENAVAL", "ENEGO"] {
        for extension in ["DAT", "DIR"] {
            expected.insert(format!("{bank}.{extension}"));
        }
    }
    assert_eq!(names, expected.iter().map(String::as_str).collect());
    assert!(!names.contains("CENTURIO.EXE"));
    assert!(b.files.iter().all(|file| !file.name.ends_with(".CSV")));
}

#[test]
fn centurion_100b_detection_preserves_renamed_and_lowercase_role_paths() {
    for renamed in [false, true] {
        let (directory, paths) = fixture_b(renamed);
        for input in std::iter::once(&directory.0).chain(paths.iter()) {
            let found = detect_with(input, synthetic_b(), file_size, hash_file).unwrap();
            assert_eq!(
                found.engine(),
                Engine::Centurion(::centurion::assets::DosRelease::V100B)
            );
            assert_eq!(found.files.len(), 31);
            for (file, actual) in production_b().files.iter().zip(&paths) {
                assert_eq!(found.inputs().path(file.name).unwrap(), actual);
            }
        }
    }
}

#[test]
fn every_centurion_100b_resource_is_required_and_fully_hashed() {
    let (directory, paths) = fixture_b(false);
    for (index, file) in production_b().files.iter().enumerate() {
        let path = &paths[index];
        let bytes = authored_bytes(index, file.name);
        fs::remove_file(path).unwrap();
        let error = detect_with(&directory.0, synthetic_b(), file_size, hash_file).unwrap_err();
        assert!(error.contains(file.name), "{error}");
        let mut corrupt = bytes.clone();
        corrupt[0] ^= 1;
        fs::write(path, corrupt).unwrap();
        let error = detect_with(&directory.0, synthetic_b(), file_size, hash_file).unwrap_err();
        assert!(
            error.contains(path.to_str().unwrap()) && error.contains("BLAKE3 mismatch"),
            "{error}"
        );
        fs::write(path, b"wrong size").unwrap();
        let error = detect_with(&directory.0, synthetic_b(), file_size, hash_file).unwrap_err();
        assert!(
            error.contains(path.to_str().unwrap()) && error.contains("expected"),
            "{error}"
        );
        fs::write(path, bytes).unwrap();
    }
    let mut hashed = HashSet::new();
    let found = detect_with(&directory.0, synthetic_b(), file_size, |path| {
        hashed.insert(path.to_path_buf());
        hash_file(path)
    })
    .unwrap();
    assert_eq!(found.files.len(), 31);
    assert_eq!(hashed, paths.into_iter().collect());
}

#[test]
fn production_centurion_100b_routing_uses_every_required_role() {
    let release = production_b();
    let directory = TestDirectory::new();
    let paths = release
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            directory.write(
                &format!("b-chunk-{index:02}"),
                &authored_bytes(index, file.name),
            )
        })
        .collect::<Vec<_>>();
    let record = |path: &Path| {
        paths
            .iter()
            .position(|actual| actual == path)
            .map(|index| release.files[index])
    };
    let mut hashed = HashSet::new();
    let found = detect_with(
        &directory.0,
        RELEASES,
        |path| Ok(record(path).unwrap().size),
        |path| {
            hashed.insert(path.to_path_buf());
            Ok(record(path).unwrap().blake3.to_owned())
        },
    )
    .unwrap();
    assert_eq!(found.release.id, release.id);
    assert_eq!(
        found.engine(),
        Engine::Centurion(::centurion::assets::DosRelease::V100B)
    );
    assert_eq!(hashed, paths.iter().cloned().collect());
}
