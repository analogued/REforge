// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Original resources selected by the application detector.
//!
//! Canonical catalog names identify resource roles, not physical filenames.
//! Engines read these exact paths without rediscovery or release fingerprinting.
//! Construction does not verify contents: the application supplies verified
//! results, while raw parser tests and research tools may supply authored data.
//! Parsers must still check formats, bounds and required resources.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub struct ResolvedFiles {
    paths: BTreeMap<String, PathBuf>,
}

impl ResolvedFiles {
    pub fn new(names: impl IntoIterator<Item = (impl Into<String>, PathBuf)>) -> Self {
        Self {
            paths: names
                .into_iter()
                .map(|(name, path)| (name.into(), path))
                .collect(),
        }
    }

    /// Look up an exact canonical role. Never fall back to directory searches,
    /// case folding, canonicalization or a different file with a familiar name.
    pub fn path(&self, name: &str) -> io::Result<&Path> {
        self.paths.get(name).map(PathBuf::as_path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Missing resolved original resource {name}"),
            )
        })
    }

    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        fs::read(self.path(name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_preserve_selected_paths_without_reconstructing_names() {
        let selected = PathBuf::from("installation/subdir/../alias/start.bin");
        let files = ResolvedFiles::new([("DATA/GAME.EXE", selected.clone())]);
        assert_eq!(files.path("DATA/GAME.EXE").unwrap(), selected);
        assert_eq!(
            files.path("data/game.exe").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            files.read("OTHER.DAT").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
