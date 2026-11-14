// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later
//! Authored filesystem and ISO fixtures; no game data or archive dependencies.
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const BLOCK: usize = 2048;
static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Tree(PathBuf);
impl Tree {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "reforge-disc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }
    pub fn directory(&self, files: &[(String, Vec<u8>)]) -> PathBuf {
        fs::create_dir(self.path("directory")).unwrap();
        for (path, bytes) in files {
            self.write(&format!("directory/{path}"), bytes);
        }
        self.path("directory")
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub fn dual16(out: &mut [u8], value: u16) {
    out[..2].copy_from_slice(&value.to_le_bytes());
    out[2..4].copy_from_slice(&value.to_be_bytes());
}
pub fn dual32(out: &mut [u8], value: u32) {
    out[..4].copy_from_slice(&value.to_le_bytes());
    out[4..8].copy_from_slice(&value.to_be_bytes());
}
pub fn record(name: &[u8], extent: u32, len: u32, directory: bool) -> Vec<u8> {
    let size = 33 + name.len() + usize::from(name.len().is_multiple_of(2));
    let mut bytes = vec![0; size];
    bytes[0] = size as u8;
    dual32(&mut bytes[2..10], extent);
    dual32(&mut bytes[10..18], len);
    bytes[25] = if directory { 2 } else { 0 };
    dual16(&mut bytes[28..32], 1);
    bytes[32] = name.len() as u8;
    bytes[33..33 + name.len()].copy_from_slice(name);
    bytes
}

/// Small ISO writer with complete nested directory records and two legal PVDs.
/// Every directory occupies one sector; tests keep authored records below that.
pub fn iso(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut directories = BTreeSet::from([String::new()]);
    for (path, _) in files {
        let mut prefix = String::new();
        let parts = path.split('/').collect::<Vec<_>>();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            directories.insert(prefix.clone());
        }
    }
    let directories = directories.into_iter().collect::<Vec<_>>();
    let mut bytes = vec![0; (19 + directories.len()) * BLOCK];
    let mut locations = Vec::new();
    for (_, data) in files {
        locations.push((bytes.len() / BLOCK) as u32);
        bytes.extend(data);
        bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, 0);
    }
    let sectors = (bytes.len() / BLOCK) as u32;
    let pvd = &mut bytes[16 * BLOCK..17 * BLOCK];
    pvd[..7].copy_from_slice(b"\x01CD001\x01");
    dual32(&mut pvd[80..88], sectors);
    dual16(&mut pvd[120..124], 1);
    dual16(&mut pvd[124..128], 1);
    dual16(&mut pvd[128..132], BLOCK as u16);
    pvd[156..190].copy_from_slice(&record(&[0], 19, BLOCK as u32, true));
    pvd[881] = 1;
    bytes.copy_within(16 * BLOCK..17 * BLOCK, 17 * BLOCK);
    bytes[18 * BLOCK..18 * BLOCK + 7].copy_from_slice(b"\xffCD001\x01");
    for (index, directory) in directories.iter().enumerate() {
        let extent = 19 + index;
        let mut data = record(&[0], extent as u32, BLOCK as u32, true);
        let parent = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
        let parent_index = directories.iter().position(|d| d == parent).unwrap();
        data.extend(record(&[1], (19 + parent_index) as u32, BLOCK as u32, true));
        for (child, path) in directories.iter().enumerate().skip(1) {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
            if parent == directory {
                data.extend(record(
                    name.as_bytes(),
                    (19 + child) as u32,
                    BLOCK as u32,
                    true,
                ));
            }
        }
        for ((path, content), &location) in files.iter().zip(&locations) {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
            if parent == directory {
                data.extend(record(
                    format!("{name};1").as_bytes(),
                    location,
                    content.len() as u32,
                    false,
                ));
            }
        }
        assert!(data.len() <= BLOCK);
        bytes[extent * BLOCK..extent * BLOCK + data.len()].copy_from_slice(&data);
    }
    bytes
}
