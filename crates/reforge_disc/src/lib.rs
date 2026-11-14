// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only files from a cooked ISO-9660 image or an extracted disc directory.
//!
//! Indexing reads directory metadata, not file payloads. A [`Region`] opens an
//! independent bounded [`std::io::Read`] + [`std::io::Seek`] stream, so engines
//! can parse large data files without installation or whole-file extraction.
//! Game identification, archive decoding and resource precedence remain callers'
//! responsibilities. The application detector selects the physical source.

mod index;
mod region;

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

pub use region::{Region, RegionReader};

/// Bounds for indexing metadata. These do not limit individual file payloads.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum directory-record bytes read from ISO directories, or normalized
    /// full-path bytes indexed while walking an extracted directory.
    pub max_index_bytes: usize,
    /// Maximum file and directory entries, excluding ISO's `.` and `..` records.
    pub max_entries: usize,
    /// Maximum directory depth below the selected root, whose depth is zero.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_index_bytes: 16 * 1024 * 1024,
            max_entries: 100_000,
            max_depth: 16,
        }
    }
}

/// An ordered index of full, root-relative logical file paths.
///
/// Paths use `/` separators and ASCII uppercase keys. Directory hierarchy is
/// preserved: no basename flattening, game-specific search or archive expansion
/// occurs. The source is opened read-only and never mounted or installed.
#[derive(Debug)]
pub struct Disc {
    source: PathBuf,
    files: BTreeMap<String, Region>,
}

impl Disc {
    /// Open a caller-selected source with the default metadata bounds.
    pub fn open(source: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_limits(source, Limits::default())
    }

    /// Open a directory tree or a cooked, 2048-byte-sector ISO image.
    ///
    /// This performs format and bounds checks, not release identification or
    /// content verification. No filename extension is required for an image.
    pub fn with_limits(source: impl AsRef<Path>, limits: Limits) -> io::Result<Self> {
        let selected = source.as_ref();
        let load = || {
            let source = selected.canonicalize()?;
            let metadata = source.metadata()?;
            let files = if metadata.is_dir() {
                index::directory(&source, limits)?
            } else if metadata.is_file() {
                index::iso(&source, limits)?
            } else {
                return Err(unsupported(
                    "disc source is not a regular file or directory",
                ));
            };
            Ok(Self { source, files })
        };
        load().map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", selected.display()))
        })
    }

    /// The canonical physical source selected at construction.
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Borrow the ordered file index. Regions are cheap to clone and remain
    /// usable after the index is dropped.
    pub fn files(&self) -> &BTreeMap<String, Region> {
        &self.files
    }

    /// Transfer the index to a caller building its own resource resolution.
    pub fn into_files(self) -> BTreeMap<String, Region> {
        self.files
    }

    /// Find a complete logical path, ASCII case-insensitively. Lookup accepts
    /// only `/`-separated relative components; it does not resolve host paths,
    /// `.`/`..`, backslashes, or an ISO `;1` version suffix.
    pub fn get(&self, path: &str) -> io::Result<&Region> {
        let key = path
            .split('/')
            .map(normalize_component)
            .collect::<io::Result<Vec<_>>>()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
            .join("/");
        self.files.get(&key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: missing disc file {key}", self.source.display()),
            )
        })
    }
}

fn normalize_component(name: &str) -> io::Result<String> {
    if name.is_empty()
        || name.len() > 255
        || matches!(name, "." | "..")
        || name
            .bytes()
            .any(|b| !(32..=126).contains(&b) || b"\\/:*?\"<>|;".contains(&b))
    {
        return Err(invalid(format!("invalid disc path component: {name:?}")));
    }
    Ok(name.to_ascii_uppercase())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}
