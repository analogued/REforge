// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Short-lived reuse of a completed verification by a queued preview job.

use super::Detected;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug)]
pub(crate) struct Verification {
    original: PathBuf,
    detected: Detected,
    stamps: Vec<(PathBuf, (u64, SystemTime))>,
}

impl Verification {
    pub fn capture(original: &Path, detected: Detected) -> Result<Self, String> {
        let stamps = detected
            .release
            .files
            .iter()
            .map(|file| {
                let path = detected
                    .files
                    .get(file.name)
                    .ok_or("Missing verified file")?;
                let stamp = stamp(path)?;
                if stamp.0 != file.size {
                    return Err("Verified original changed size".into());
                }
                Ok((path.clone(), stamp))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            original: original.to_owned(),
            detected,
            stamps,
        })
    }

    pub fn current_for(&self, original: &Path, release: &str) -> Option<&Detected> {
        (self.original == original
            && self.detected.release.id == release
            && self
                .stamps
                .iter()
                .all(|(path, expected)| stamp(path).is_ok_and(|actual| actual == *expected)))
        .then_some(&self.detected)
    }
}

pub(crate) fn stamp(path: &Path) -> Result<(u64, SystemTime), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Original is no longer a regular file".into());
    }
    Ok((
        metadata.len(),
        metadata.modified().map_err(|error| error.to_string())?,
    ))
}
