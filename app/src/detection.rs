// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Identify supported originals without depending on private reference paths.

use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

mod catalog;
#[cfg(test)]
use catalog::{ALLEY_CAT_DOS, MACH3_DOS};
pub use catalog::{Edition, GameMetadata, RELEASES};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Game {
    AlleyCat,
    Mach3,
}

impl Game {
    pub const fn id(self) -> &'static str {
        match self {
            Self::AlleyCat => "alley-cat",
            Self::Mach3 => "mach-3",
        }
    }

    pub const fn metadata(self) -> &'static GameMetadata {
        match self {
            Self::AlleyCat => &catalog::ALLEY_CAT,
            Self::Mach3 => &catalog::MACH3,
        }
    }

    pub const fn title(self) -> &'static str {
        self.metadata().title
    }
}

#[derive(Debug)]
pub struct FileRecord {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug)]
pub struct Release {
    /// A reference identifier, not a claim about the commercial version.
    pub id: &'static str,
    /// Verified compatibility of saves and persistent option semantics.
    pub compatibility_id: &'static str,
    pub edition: &'static Edition,
    pub files: &'static [FileRecord],
}

impl Release {
    pub const fn game(&self) -> Game {
        self.edition.game.identity
    }

    pub fn compatible_with(&self, other: &Self) -> bool {
        self.game() == other.game()
            && !self.compatibility_id.is_empty()
            && self.compatibility_id == other.compatibility_id
    }
}

#[derive(Debug)]
pub struct Detected {
    pub game: Game,
    pub release: &'static Release,
    /// The executable for Alley Cat, or the resource directory for Mach 3.
    pub original: PathBuf,
    /// Actual paths selected by full verification, keyed by catalog spelling.
    #[cfg_attr(not(feature = "sdl"), allow(dead_code))]
    pub files: BTreeMap<&'static str, PathBuf>,
}

pub fn detect(path: &Path) -> Result<Detected, String> {
    detect_with(path, RELEASES, file_size, hash_file)
}

struct Candidate {
    release: &'static Release,
    paths: Vec<Vec<PathBuf>>,
}
struct Verified {
    release: &'static Release,
    paths: Vec<PathBuf>,
}

fn detect_with(
    path: &Path,
    releases: &'static [Release],
    mut size: impl FnMut(&Path) -> Result<u64, String>,
    mut hash: impl FnMut(&Path) -> Result<String, String>,
) -> Result<Detected, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect original path {}: {error}", path.display()))?;
    let (directory, anchor) = if metadata.is_dir() {
        (path, None)
    } else if metadata.is_file() {
        let anchor = path.file_name().and_then(|name| name.to_str());
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        (directory, anchor)
    } else {
        return Err(format!(
            "Original path {} must be a file or directory",
            path.display()
        ));
    };
    // Non-UTF-8 names cannot name any of the ASCII DOS files in the catalog.
    if metadata.is_file() && anchor.is_none() {
        return Err(unknown(path, "unsupported file name"));
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "Cannot read original directory {}: {error}",
                directory.display()
            )
        })?
        .map(|entry| {
            entry
                .map(|entry| (entry.file_name(), entry.path()))
                .map_err(|error| format!("Cannot read original directory entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Resolve every candidate's required names before inspecting any sizes.
    // An explicit file restricts the catalog to releases requiring that name;
    // its siblings still supply the remaining required originals.
    let mut named = Vec::new();
    let mut incomplete = Vec::new();
    for release in releases {
        if anchor.is_some_and(|anchor| {
            !release
                .files
                .iter()
                .any(|file| file.name.eq_ignore_ascii_case(anchor))
        }) {
            continue;
        }
        let mut paths = Vec::new();
        let mut missing = Vec::new();
        for file in release.files {
            let mut matches = entries
                .iter()
                .filter(|(name, _)| {
                    name.to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case(file.name))
                })
                .collect::<Vec<_>>();
            // Prefer the catalog's exact spelling. Other ASCII case variants
            // have a deterministic alphabetical order, independent of read_dir.
            matches.sort_by(|(a, _), (b, _)| {
                (a.to_str() != Some(file.name))
                    .cmp(&(b.to_str() != Some(file.name)))
                    .then_with(|| a.cmp(b))
            });
            if matches.is_empty() {
                missing.push(file.name);
            } else {
                paths.push(matches.into_iter().map(|(_, path)| path.clone()).collect());
            }
        }
        if missing.is_empty() {
            named.push(Candidate { release, paths });
        } else if !paths.is_empty() {
            incomplete.push(format!(
                "{}: missing {}",
                release.game().title(),
                missing.join(", ")
            ));
        }
    }
    if named.is_empty() {
        return if incomplete.is_empty() {
            Err(unknown(path, "no supported original file names"))
        } else {
            Err(format!(
                "Incomplete original data in {}: {}",
                directory.display(),
                incomplete.join("; ")
            ))
        };
    }

    // Filter all complete candidates by size before hashing any candidate.
    let mut sizes = HashMap::new();
    let mut sized = Vec::new();
    let mut mismatches = Vec::new();
    for mut candidate in named {
        let mut valid = true;
        for (file, paths) in candidate.release.files.iter().zip(&mut candidate.paths) {
            paths.retain(|path| {
                let actual = sizes.entry(path.clone()).or_insert_with(|| size(path));
                match actual {
                    Ok(actual) if *actual == file.size => true,
                    Ok(actual) => {
                        mismatches.push(format!(
                            "{}: {} expected {} bytes, found {actual}",
                            candidate.release.id,
                            path.display(),
                            file.size
                        ));
                        false
                    }
                    Err(error) => {
                        mismatches.push(error.clone());
                        false
                    }
                }
            });
            if paths.is_empty() {
                valid = false;
            }
        }
        if valid {
            sized.push(candidate);
        }
    }

    let mut hashes = HashMap::new();
    let mut verified = Vec::new();
    for candidate in sized {
        let mut selected = Vec::new();
        for (file, paths) in candidate.release.files.iter().zip(&candidate.paths) {
            let selected_path = paths.iter().find(|path| {
                match hashes.entry((*path).clone()).or_insert_with(|| hash(path)) {
                    Ok(actual) if actual == file.sha256 => true,
                    Ok(_) => {
                        mismatches.push(format!(
                            "{}: {} SHA-256 mismatch",
                            candidate.release.id,
                            path.display()
                        ));
                        false
                    }
                    Err(error) => {
                        mismatches.push(error.clone());
                        false
                    }
                }
            });
            if let Some(path) = selected_path {
                selected.push(path.clone());
            } else {
                break;
            }
        }
        if selected.len() == candidate.release.files.len() {
            verified.push(Verified {
                release: candidate.release,
                paths: selected,
            });
        }
    }
    match verified.as_slice() {
        [candidate] => {
            let original = match candidate.release.game() {
                Game::AlleyCat => candidate.paths[0].clone(),
                Game::Mach3 => directory.to_path_buf(),
            };
            Ok(Detected {
                game: candidate.release.game(),
                release: candidate.release,
                original,
                files: candidate
                    .release
                    .files
                    .iter()
                    .zip(&candidate.paths)
                    .map(|(file, path)| (file.name, path.clone()))
                    .collect(),
            })
        }
        [] => Err(unknown(path, &mismatches.join("; "))),
        _ => Err(format!(
            "Ambiguous original data in {}: matched {}; select a game's original file with --original",
            directory.display(),
            verified
                .iter()
                .map(|candidate| format!(
                    "{} ({})",
                    candidate.release.game().title(),
                    candidate.release.id
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn unknown(path: &Path, reason: &str) -> String {
    format!(
        "Unknown or mismatched original data at {}: {reason}",
        path.display()
    )
}

fn file_size(path: &Path) -> Result<u64, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect original file {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(unknown(path, "required original is not a regular file"));
    }
    Ok(metadata.len())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Cannot open original file {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read original file {}: {error}", path.display()))?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn compatibility_is_explicit_and_never_crosses_game_identity() {
        let release = &RELEASES[0];
        let mut other = Release {
            id: "synthetic-different-packaging",
            compatibility_id: release.compatibility_id,
            edition: release.edition,
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
    const XYZ: &str = "3608bca1e44ea6c4d268eb6db02260269892c0b42b86bbf1e77a6fa16c3c9282";
    static SYNTHETIC: &[Release] = &[
        Release {
            id: "synthetic-cat-a",
            compatibility_id: "alley-cat-dos",
            edition: &ALLEY_CAT_DOS,
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
            files: &[FileRecord {
                name: "CAT.EXE",
                size: 3,
                sha256: XYZ,
            }],
        },
        Release {
            id: "synthetic-mach",
            compatibility_id: "mach-3-dos",
            edition: &MACH3_DOS,
            files: &[
                FileRecord {
                    name: "MACH3.EXE",
                    size: 3,
                    sha256: XYZ,
                },
                FileRecord {
                    name: "DATA1",
                    size: 3,
                    sha256: ABC,
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
        fs::write(&cat, b"xyz").unwrap();
        assert_eq!(
            directory.detect(&cat).unwrap().release.id,
            "synthetic-cat-b"
        );
    }

    #[test]
    fn mach_file_anchors_find_siblings_and_normalize_to_directory() {
        let directory = TestDirectory::new();
        let exe = directory.write("mach3.exe", b"xyz");
        let data = directory.write("dAtA1", b"abc");
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
        directory.write("DATA1", b"abc");
        assert!(
            directory
                .detect(&directory.0)
                .unwrap_err()
                .contains("Ambiguous")
        );
        assert_eq!(directory.detect(&cat).unwrap().game, Game::AlleyCat);
        assert_eq!(directory.detect(&mach).unwrap().game, Game::Mach3);
        let unrelated = directory.write("OTHER.EXE", b"abc");
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
        for input in [&directory.0, &exact, &lower] {
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
        for input in [&directory.0, &exact, &lower] {
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
        let data = directory.write("data1", b"abc");
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
            error.contains("CAT.EXE SHA-256 mismatch")
                && error.contains("cat.exe SHA-256 mismatch")
        );
    }

    #[test]
    fn different_releases_in_case_variants_remain_ambiguous() {
        let directory = TestDirectory::new();
        directory.write("CAT.EXE", b"abc");
        directory.write("cat.exe", b"xyz");
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
    fn names_filter_all_candidates_before_any_size_or_hash_reads() {
        let directory = TestDirectory::new();
        directory.write("MACH3.EXE", b"xyz");
        let result = detect_with(
            &directory.0,
            SYNTHETIC,
            |_| panic!("must not inspect sizes for incomplete candidates"),
            |_| panic!("must not hash incomplete candidates"),
        );
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
                assert_eq!(
                    path.file_name().unwrap(),
                    "CAT.EXE",
                    "size-mismatched candidate must never be hashed"
                );
                hashes_read.set(hashes_read.get() + 1);
                hash_file(path)
            },
        )
        .unwrap();
        assert_eq!(result.game, Game::AlleyCat);
        assert_eq!(
            hashes_read.get(),
            1,
            "shared file hashes are cached across releases"
        );
    }

    #[test]
    fn no_hashes_are_read_when_all_candidates_fail_size_checks() {
        let directory = TestDirectory::new();
        directory.write("CAT.EXE", b"abcd");
        directory.write("MACH3.EXE", b"xyz");
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
        assert_eq!(RELEASES.len(), 2);
        for release in RELEASES {
            assert!(release.id.contains("verified-reference"));
            assert!(!release.game().id().is_empty());
            for file in release.files {
                assert!(file.size > 0);
                assert_eq!(file.sha256.len(), 64);
                assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
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
        assert_eq!(mach.files.len(), 7);
    }
}
