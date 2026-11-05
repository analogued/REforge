// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One-use transfer of Play's verification to its child process. Stdin remains
//! inherited for terminal games. This is neither a persistent verification cache
//! nor a command-line option: ordinary CLI launches still run the detector.

use crate::detection::{self, Detected, Game, verification::stamp};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, UNIX_EPOCH},
};

const ENVIRONMENT: &str = "REFORGE_VERIFIED_LAUNCH";
const MAGIC: &[u8] = b"reforge-verified-launch-v2\0";
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) struct Ticket {
    path: PathBuf,
}

impl Ticket {
    pub fn new(original: &Path, detected: &Detected) -> Result<Self, String> {
        let bytes = encode(original, detected)?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|error| error.to_string())?;
        let name = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("reforge-launch-{name}.bin"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|error| error.to_string())?;
        // Arm cleanup after exclusive creation, including failed writes.
        let ticket = Self { path };
        let written = file.write_all(&bytes);
        drop(file);
        written.map_err(|error| error.to_string())?;
        Ok(ticket)
    }

    pub fn attach(&self, command: &mut Command) {
        command.env(ENVIRONMENT, &self.path);
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn clear(command: &mut Command) {
    command.env_remove(ENVIRONMENT);
}

pub(crate) fn detect(path: &Path) -> Result<Detected, String> {
    let ticket = std::env::var_os(ENVIRONMENT).map(PathBuf::from);
    resolve(path, ticket.as_deref(), detection::detect)
}

fn resolve(
    original: &Path,
    ticket: Option<&Path>,
    detector: impl FnOnce(&Path) -> Result<Detected, String>,
) -> Result<Detected, String> {
    if let Some(ticket) = ticket {
        match receive(ticket, original) {
            Ok(detected) => return Ok(detected),
            Err(error) => {
                eprintln!("REforge: launch handoff unavailable ({error}); verifying original files")
            }
        }
    }
    detector(original)
}

fn receive(ticket: &Path, original: &Path) -> Result<Detected, String> {
    let name = ticket
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("Invalid launch ticket path")?;
    let nonce = name
        .strip_prefix("reforge-launch-")
        .and_then(|name| name.strip_suffix(".bin"))
        .ok_or("Invalid launch ticket name")?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid launch ticket name".into());
    }
    if !fs::symlink_metadata(ticket)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Launch ticket is not a regular file".into());
    }
    let mut bytes = Vec::new();
    File::open(ticket)
        .map_err(|error| error.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Launch ticket is too large".into());
    }
    let detected = decode(&bytes, original)?;
    // Successful removal claims the ticket. A second consumer must verify again.
    fs::remove_file(ticket).map_err(|error| error.to_string())?;
    Ok(detected)
}

fn encode(original: &Path, detected: &Detected) -> Result<Vec<u8>, String> {
    if detected.game != detected.release.game()
        || detected.files.len() != detected.release.files.len()
    {
        return Err("Incomplete verified release".into());
    }
    let mut bytes = MAGIC.to_vec();
    field(&mut bytes, env!("CARGO_PKG_VERSION").as_bytes());
    field(&mut bytes, detected.release.id.as_bytes());
    path_field(&mut bytes, original)?;
    path_field(&mut bytes, &detected.root)?;
    path_field(&mut bytes, &detected.original)?;
    for file in detected.release.files {
        field(&mut bytes, file.name.as_bytes());
        field(&mut bytes, file.blake3.as_bytes());
        let path = detected
            .files
            .get(file.name)
            .ok_or("Missing verified file")?;
        path_field(&mut bytes, path)?;
        let (length, modified) = stamp(path)?;
        if length != file.size {
            return Err("Verified original changed size".into());
        }
        integer(&mut bytes, length);
        let (positive, duration) = match modified.duration_since(UNIX_EPOCH) {
            Ok(duration) => (1, duration),
            Err(error) => (0, error.duration()),
        };
        integer(&mut bytes, positive);
        integer(&mut bytes, duration.as_secs());
        integer(&mut bytes, u64::from(duration.subsec_nanos()));
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Launch ticket is too large".into());
    }
    Ok(bytes)
}

fn decode(bytes: &[u8], original: &Path) -> Result<Detected, String> {
    let mut reader = Reader(
        bytes
            .strip_prefix(MAGIC)
            .ok_or("Invalid launch ticket version")?,
    );
    if reader.field()? != env!("CARGO_PKG_VERSION").as_bytes() {
        return Err("Launch ticket is from another application version".into());
    }
    let id = reader.field()?;
    let release = detection::RELEASES
        .iter()
        .find(|release| release.id.as_bytes() == id)
        .ok_or("Unknown release in launch ticket")?;
    let anchor = reader.path()?;
    if anchor != std::path::absolute(original).map_err(|error| error.to_string())? {
        return Err("Launch ticket belongs to another original path".into());
    }
    let root = reader.path()?;
    let selected = reader.path()?;
    let mut files = BTreeMap::new();
    for file in release.files {
        if reader.field()? != file.name.as_bytes() || reader.field()? != file.blake3.as_bytes() {
            return Err("Launch ticket does not match the file catalog".into());
        }
        let path = reader.path()?;
        let length = reader.integer()?;
        let positive = reader.integer()?;
        let seconds = reader.integer()?;
        let nanos = reader.integer()?;
        if positive > 1 || nanos >= 1_000_000_000 {
            return Err("Invalid file timestamp in launch ticket".into());
        }
        let duration = Duration::new(seconds, nanos as u32);
        let modified = if positive == 1 {
            UNIX_EPOCH.checked_add(duration)
        } else {
            UNIX_EPOCH.checked_sub(duration)
        }
        .ok_or("File timestamp outside the supported range")?;
        if length != file.size || stamp(&path)? != (length, modified) {
            return Err("Original file metadata changed since launch handoff preparation".into());
        }
        files.insert(file.name, path);
    }
    if !reader.0.is_empty() {
        return Err("Unexpected data in launch ticket".into());
    }
    if !root.is_dir()
        || match release.game() {
            Game::AlleyCat => files.values().next() != Some(&selected),
            _ => selected != root,
        }
    {
        return Err("Invalid original location in launch ticket".into());
    }
    Ok(Detected {
        game: release.game(),
        release,
        root,
        original: selected,
        files,
    })
}

fn integer(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn field(bytes: &mut Vec<u8>, value: &[u8]) {
    integer(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}

fn path_field(bytes: &mut Vec<u8>, path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Launch ticket paths must be absolute".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        field(bytes, path.as_os_str().as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide = path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        field(bytes, &wide);
    }
    #[cfg(not(any(unix, windows)))]
    field(
        bytes,
        path.to_str()
            .ok_or("Non-UTF-8 path on an unsupported host")?
            .as_bytes(),
    );
    Ok(())
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn integer(&mut self) -> Result<u64, String> {
        let (value, rest) = self
            .0
            .split_at_checked(8)
            .ok_or("Truncated launch ticket")?;
        self.0 = rest;
        Ok(u64::from_le_bytes(value.try_into().unwrap()))
    }
    fn field(&mut self) -> Result<&'a [u8], String> {
        let length = usize::try_from(self.integer()?).map_err(|_| "Launch field is too large")?;
        let (value, rest) = self
            .0
            .split_at_checked(length)
            .ok_or("Truncated launch ticket")?;
        self.0 = rest;
        Ok(value)
    }
    fn path(&mut self) -> Result<PathBuf, String> {
        let bytes = self.field()?;
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStrExt;
            PathBuf::from(OsStr::from_bytes(bytes))
        };
        #[cfg(windows)]
        let path = {
            use std::os::windows::ffi::OsStringExt;
            if bytes.len() % 2 != 0 {
                return Err("Invalid native path in launch ticket".into());
            }
            let wide = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            PathBuf::from(std::ffi::OsString::from_wide(&wide))
        };
        #[cfg(not(any(unix, windows)))]
        let path = PathBuf::from(std::str::from_utf8(bytes).map_err(|error| error.to_string())?);
        if !path.is_absolute() {
            return Err("Relative path in launch ticket".into());
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests;
