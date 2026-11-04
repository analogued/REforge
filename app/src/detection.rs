// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

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
mod identity;
#[cfg(test)]
use catalog::{ALLEY_CAT_DOS, MACH3_DOS};
pub use catalog::{Edition, GameMetadata, RELEASES};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Game {
    AlleyCat,
    Mach3,
    Mindwheel,
    Essex,
    Brimstone,
    Breakers,
}

impl Game {
    pub const fn id(self) -> &'static str {
        match self {
            Self::AlleyCat => "alley-cat",
            Self::Mach3 => "mach-3",
            Self::Mindwheel => "mindwheel",
            Self::Essex => "essex",
            Self::Brimstone => "brimstone",
            Self::Breakers => "breakers",
        }
    }

    pub const fn metadata(self) -> &'static GameMetadata {
        match self {
            Self::AlleyCat => &catalog::ALLEY_CAT,
            Self::Mach3 => &catalog::MACH3,
            Self::Mindwheel => &catalog::MINDWHEEL,
            Self::Essex => &catalog::ESSEX,
            Self::Brimstone => &catalog::BRIMSTONE,
            Self::Breakers => &catalog::BREAKERS,
        }
    }

    pub const fn is_btz(self) -> bool {
        matches!(
            self,
            Self::Mindwheel | Self::Essex | Self::Brimstone | Self::Breakers
        )
    }

    pub const fn title(self) -> &'static str {
        self.metadata().title
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FileRecord {
    /// Canonical catalog name and loader role; physical filenames may differ.
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug)]
pub struct Release {
    /// Stable detected-distribution ID; not an official game version number.
    pub id: &'static str,
    /// Verified compatibility of saves and persistent option semantics.
    pub compatibility_id: &'static str,
    pub edition: &'static Edition,
    pub files: &'static [FileRecord],
    /// Names used to derive the stable ID; empty means all required files.
    /// A published ID can retain its original subset as verification expands.
    #[cfg(test)]
    pub identity_files: &'static [&'static str],
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
    /// The executable for Alley Cat, or the resource directory for other games.
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

fn named(path: &Path, name: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(name))
}

fn name_rank(path: &Path, name: &str) -> u8 {
    if path.file_name().is_some_and(|actual| actual == name) {
        0
    } else if named(path, name) {
        1
    } else {
        2
    }
}

type Hashes = HashMap<PathBuf, Result<String, String>>;

fn matches_hash(
    path: &Path,
    file: &FileRecord,
    hashes: &mut Hashes,
    hash: &mut impl FnMut(&Path) -> Result<String, String>,
    diagnostics: &mut Vec<String>,
) -> bool {
    let actual = hashes.entry(path.to_owned()).or_insert_with(|| hash(path));
    match actual {
        Ok(actual) if actual == file.sha256 => true,
        result => {
            // Unrelated same-size files are normal during content searches.
            // Explain failures of expected names without flooding the error.
            if named(path, file.name) && diagnostics.len() < 12 {
                diagnostics.push(match result {
                    Ok(_) => format!("{} SHA-256 mismatch", path.display()),
                    Err(error) => error.clone(),
                });
            }
            false
        }
    }
}

fn detect_with(
    path: &Path,
    releases: &'static [Release],
    mut size: impl FnMut(&Path) -> Result<u64, String>,
    mut hash: impl FnMut(&Path) -> Result<String, String>,
) -> Result<Detected, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect original path {}: {error}", path.display()))?;
    let directory = if metadata.is_dir() {
        path
    } else if metadata.is_file() {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    } else {
        return Err(format!(
            "Original path {} must be a file or directory",
            path.display()
        ));
    };
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "Cannot read original directory {}: {error}",
                directory.display()
            )
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("Cannot read original directory entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    let anchor = if metadata.is_file() {
        // Use read_dir's spelling of this exact entry, including non-UTF-8 names
        // and paths supplied without an explicit parent (foo vs ./foo).
        let canonical = fs::canonicalize(path).map_err(|error| {
            format!(
                "Cannot resolve selected original {}: {error}",
                path.display()
            )
        })?;
        Some(
            entries
                .iter()
                .find(|entry| entry.file_name() == path.file_name())
                .or_else(|| {
                    entries
                        .iter()
                        .find(|entry| entry.file_name() == canonical.file_name())
                })
                .cloned()
                .ok_or_else(|| unknown(path, "selected file disappeared during detection"))?,
        )
    } else {
        None
    };
    // Inspect metadata once, then hash only sizes relevant to a complete
    // candidate. Directories and other nonregular entries never reach hashing.
    let sizes: HashMap<_, _> = entries
        .iter()
        .map(|path| (path.clone(), size(path)))
        .collect();
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    let mut incomplete = Vec::new();
    for release in releases {
        if let Some(anchor) = &anchor
            && !release
                .files
                .iter()
                .any(|file| sizes[anchor].as_ref().is_ok_and(|s| *s == file.size))
        {
            continue;
        }
        let mut paths = Vec::new();
        for file in release.files {
            let mut matches = Vec::new();
            for path in &entries {
                match &sizes[path] {
                    Ok(actual) if *actual == file.size => matches.push(path.clone()),
                    actual if named(path, file.name) && diagnostics.len() < 12 => {
                        diagnostics.push(match actual {
                            Ok(actual) => format!(
                                "{} expected {} bytes, found {actual}",
                                path.display(),
                                file.size
                            ),
                            Err(error) => error.clone(),
                        });
                    }
                    _ => {}
                }
            }
            matches.sort_by(|a, b| {
                name_rank(a, file.name)
                    .cmp(&name_rank(b, file.name))
                    .then_with(|| a.cmp(b))
            });
            paths.push(matches);
        }
        // Every catalog record requires a distinct directory entry, even when
        // two required files happen to contain identical bytes.
        let enough_files = release.files.iter().zip(&paths).all(|(file, matches)| {
            matches.len()
                >= release
                    .files
                    .iter()
                    .filter(|other| other.size == file.size)
                    .count()
        });
        if enough_files {
            candidates.push(Candidate { release, paths });
        } else if release
            .files
            .iter()
            .any(|file| entries.iter().any(|path| named(path, file.name)))
        {
            let missing = release
                .files
                .iter()
                .filter(|file| !entries.iter().any(|path| named(path, file.name)))
                .map(|file| file.name)
                .collect::<Vec<_>>();
            incomplete.push(format!(
                "{}: missing verified files for {}",
                release.game().title(),
                if missing.is_empty() {
                    "the required file set".into()
                } else {
                    missing.join(", ")
                }
            ));
        }
    }

    let mut hashes = Hashes::new();
    let mut verified = Vec::new();
    for candidate in candidates {
        let mut selected = vec![None; candidate.release.files.len()];
        if let Some(anchor) = &anchor {
            let mut roles = candidate
                .release
                .files
                .iter()
                .enumerate()
                .collect::<Vec<_>>();
            roles.sort_by_key(|(_, file)| name_rank(anchor, file.name));
            let role = roles.into_iter().find(|(index, file)| {
                candidate.paths[*index].contains(anchor)
                    && matches_hash(anchor, file, &mut hashes, &mut hash, &mut diagnostics)
            });
            let Some((index, _)) = role else {
                continue;
            };
            selected[index] = Some(anchor.clone());
        }
        // Reserve valid expected names for all roles before filling any gaps.
        // Content groups cannot overlap except for identical fingerprints, so
        // assigning an unused matching file never requires backtracking.
        for fallback in [false, true] {
            for (index, (file, paths)) in candidate
                .release
                .files
                .iter()
                .zip(&candidate.paths)
                .enumerate()
            {
                if selected[index].is_some() {
                    continue;
                }
                let found = paths.iter().find(|path| {
                    (fallback || named(path, file.name))
                        && !selected
                            .iter()
                            .any(|selected| selected.as_ref() == Some(*path))
                        && matches_hash(path, file, &mut hashes, &mut hash, &mut diagnostics)
                });
                if let Some(path) = found {
                    selected[index] = Some(path.clone());
                }
            }
        }
        if selected.iter().all(Option::is_some) {
            verified.push(Verified {
                release: candidate.release,
                paths: selected.into_iter().map(Option::unwrap).collect(),
            });
        } else if selected.iter().any(Option::is_some) {
            let missing = candidate
                .release
                .files
                .iter()
                .zip(&selected)
                .filter(|(_, path)| path.is_none())
                .map(|(file, _)| file.name)
                .collect::<Vec<_>>();
            incomplete.push(format!(
                "{}: missing verified files for {}",
                candidate.release.game().title(),
                missing.join(", ")
            ));
        }
    }
    match verified.as_slice() {
        [candidate] => {
            let original = match candidate.release.game() {
                Game::AlleyCat => candidate.paths[0].clone(),
                Game::Mach3 | Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers => {
                    directory.to_path_buf()
                }
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
        [] if !diagnostics.is_empty() => Err(unknown(path, &diagnostics.join("; "))),
        [] if !incomplete.is_empty() => Err(format!(
            "Incomplete original data in {}: {}",
            directory.display(),
            incomplete.join("; ")
        )),
        [] => Err(unknown(
            path,
            "no complete supported original file set matched",
        )),
        _ => Err(format!(
            "Ambiguous original data in {}: matched {}; select a distinguishing original file with --original",
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
mod tests;
