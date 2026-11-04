// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded discovery within one release's catalog-defined directory layout.

use super::*;

pub(super) fn catalog_directories(files: &[FileRecord]) -> Result<Vec<PathBuf>, String> {
    let mut directories = vec![PathBuf::new()];
    for file in files {
        let parts = file.name.split('/').collect::<Vec<_>>();
        if parts.iter().any(|part| {
            part.is_empty() || matches!(*part, "." | "..") || part.contains(['\\', ':'])
        }) {
            return Err(format!("Invalid catalog relative path: {}", file.name));
        }
        directories.push(parts[..parts.len() - 1].iter().collect());
    }
    directories.sort();
    directories.dedup();
    Ok(directories)
}

pub(super) fn roots(parent: &Path, file_input: bool, directories: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = vec![parent.to_owned()];
    if file_input {
        for directory in directories {
            let mut root = parent;
            let matches = directory.components().rev().all(|part| {
                if !root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        part.as_os_str()
                            .to_str()
                            .is_some_and(|part| name.eq_ignore_ascii_case(part))
                    })
                {
                    return false;
                }
                root = root.parent().unwrap_or(root);
                true
            });
            if matches {
                let canonical = fs::canonicalize(root).ok();
                if !roots.iter().any(|existing| {
                    existing == root
                        || canonical.as_ref().is_some_and(|canonical| {
                            fs::canonicalize(existing).ok().as_ref() == Some(canonical)
                        })
                }) {
                    roots.push(root.to_owned());
                }
            }
        }
    }
    roots
}

// A selected file may use a directory alias, but another file symlink pointing
// at the same bytes must not replace that explicitly selected entry.
pub(super) fn same_entry(a: &Path, b: &Path) -> bool {
    a == b
        || (a.file_name() == b.file_name()
            && a.parent()
                .and_then(|parent| fs::canonicalize(parent).ok())
                .is_some_and(|parent| {
                    b.parent().and_then(|parent| fs::canonicalize(parent).ok()) == Some(parent)
                }))
}

#[derive(Default)]
pub(super) struct Directories {
    entries: HashMap<PathBuf, Result<Vec<PathBuf>, String>>,
}

impl Directories {
    pub(super) fn read(&mut self, directory: &Path) -> Result<Vec<PathBuf>, String> {
        self.entries
            .entry(directory.to_owned())
            .or_insert_with(|| {
                let mut paths = fs::read_dir(directory)
                    .map_err(|error| {
                        format!(
                            "Cannot read original directory {}: {error}",
                            directory.display()
                        )
                    })?
                    .map(|entry| {
                        entry.map(|entry| entry.path()).map_err(|error| {
                            format!("Cannot read original directory entry: {error}")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                paths.sort();
                Ok(paths)
            })
            .clone()
    }

    pub(super) fn entries(
        &mut self,
        root: &Path,
        directories: &[PathBuf],
        diagnostics: &mut Vec<String>,
    ) -> Vec<PathBuf> {
        let mut entries = Vec::new();
        for directory in directories {
            let mut locations = vec![root.to_owned()];
            for part in directory.components() {
                let mut next = Vec::new();
                for location in locations {
                    for entry in self.read_or_report(&location, diagnostics) {
                        if entry.is_dir()
                            && entry
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| {
                                    part.as_os_str()
                                        .to_str()
                                        .is_some_and(|part| name.eq_ignore_ascii_case(part))
                                })
                        {
                            next.push(entry);
                        }
                    }
                }
                locations = next;
            }
            for location in locations {
                entries.extend(self.read_or_report(&location, diagnostics));
            }
        }
        entries.sort();
        entries.dedup();
        entries
    }

    fn read_or_report(&mut self, directory: &Path, diagnostics: &mut Vec<String>) -> Vec<PathBuf> {
        match self.read(directory) {
            Ok(entries) => entries,
            Err(error) => {
                if diagnostics.len() < 12 && !diagnostics.contains(&error) {
                    diagnostics.push(error);
                }
                Vec::new()
            }
        }
    }
}
