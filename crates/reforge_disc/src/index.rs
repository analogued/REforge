// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io;
use std::path::Path;

use crate::Region;
use crate::{Limits, invalid, normalize_component, unsupported};

const BLOCK: u64 = 2048;
pub(crate) type Files = BTreeMap<String, Region>;

pub(crate) fn directory(root: &Path, limits: Limits) -> io::Result<Files> {
    fn walk(
        root: &Path,
        prefix: &str,
        depth: usize,
        files: &mut Files,
        seen: &mut BTreeSet<String>,
        limits: Limits,
        budget: &mut usize,
    ) -> io::Result<()> {
        if depth > limits.max_depth {
            return Err(unsupported("disc directory nesting exceeds limit"));
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() || !(kind.is_file() || kind.is_dir()) {
                return Err(unsupported(format!(
                    "non-regular disc entry: {}",
                    entry.path().display()
                )));
            }
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| unsupported("non-ASCII disc filename"))?;
            let key = format!("{prefix}{}", normalize_component(name)?);
            *budget = budget
                .checked_sub(key.len())
                .ok_or_else(|| invalid("disc directory index exceeds byte limit"))?;
            if !seen.insert(key.clone()) {
                return Err(invalid(format!("ambiguous disc path: {key}")));
            }
            if seen.len() > limits.max_entries {
                return Err(invalid("disc entry count exceeds limit"));
            }
            if kind.is_dir() {
                walk(
                    &entry.path(),
                    &format!("{key}/"),
                    depth + 1,
                    files,
                    seen,
                    limits,
                    budget,
                )?;
            } else {
                files.insert(key, Region::path(entry.path(), entry.metadata()?.len()));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    let mut budget = limits.max_index_bytes;
    walk(
        root,
        "",
        0,
        &mut files,
        &mut BTreeSet::new(),
        limits,
        &mut budget,
    )?;
    Ok(files)
}

fn both16(bytes: &[u8]) -> io::Result<u16> {
    let value = u16::from_le_bytes(bytes[..2].try_into().unwrap());
    if value != u16::from_be_bytes(bytes[2..4].try_into().unwrap()) {
        return Err(invalid("ISO little/big-endian values disagree"));
    }
    Ok(value)
}

fn both32(bytes: &[u8]) -> io::Result<u32> {
    let value = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    if value != u32::from_be_bytes(bytes[4..8].try_into().unwrap()) {
        return Err(invalid("ISO little/big-endian values disagree"));
    }
    Ok(value)
}

struct Record<'a> {
    name: &'a [u8],
    region: Region,
    directory: bool,
    extent: u32,
}

fn record<'a>(bytes: &'a [u8], volume: &Region) -> io::Result<Record<'a>> {
    if bytes.len() < 34
        || bytes[0] as usize != bytes.len()
        || bytes[32] == 0
        || 33 + bytes[32] as usize > bytes.len()
    {
        return Err(invalid("invalid ISO directory record length"));
    }
    if bytes[1] != 0 || bytes[25] & 0x80 != 0 || bytes[26] != 0 || bytes[27] != 0 {
        return Err(unsupported(
            "ISO extended attributes, multi-extent or interleaved files",
        ));
    }
    if both16(&bytes[28..32])? != 1 {
        return Err(unsupported("multi-volume ISO"));
    }
    let extent = both32(&bytes[2..10])?;
    let len = both32(&bytes[10..18])?;
    Ok(Record {
        name: &bytes[33..33 + bytes[32] as usize],
        region: volume.slice(u64::from(extent) * BLOCK, u64::from(len))?,
        directory: bytes[25] & 2 != 0,
        extent,
    })
}

pub(crate) fn iso(path: &Path, limits: Limits) -> io::Result<Files> {
    let image = Region::from_file(File::open(path)?)?;
    let mut primary = None;
    let mut terminated = false;
    for sector in 16..80 {
        let descriptor = image.read_at(sector * BLOCK, BLOCK as usize)?;
        if &descriptor[1..7] != b"CD001\x01" {
            return Err(invalid("invalid ISO volume descriptor"));
        }
        match descriptor[0] {
            1 => {
                if let Some(previous) = &primary {
                    if previous != &descriptor {
                        return Err(invalid("conflicting ISO primary descriptors"));
                    }
                } else {
                    primary = Some(descriptor);
                }
            }
            255 => {
                terminated = true;
                break;
            }
            _ => {}
        }
    }
    if !terminated {
        return Err(invalid("missing ISO descriptor terminator"));
    }
    let primary = primary.ok_or_else(|| unsupported("ISO has no primary volume descriptor"))?;
    if both16(&primary[128..132])? != BLOCK as u16 {
        return Err(unsupported("ISO logical block size is not 2048"));
    }
    if both16(&primary[120..124])? != 1 || both16(&primary[124..128])? != 1 {
        return Err(unsupported("multi-volume ISO"));
    }
    let volume = image.slice(0, u64::from(both32(&primary[80..88])?) * BLOCK)?;
    let root_len = primary[156] as usize;
    if root_len > primary.len() - 156 {
        return Err(invalid("invalid ISO root record"));
    }
    let root = record(&primary[156..156 + root_len], &volume)?;
    if !root.directory || root.name != [0] {
        return Err(invalid("invalid ISO root directory"));
    }
    let mut index = IsoIndex {
        volume,
        files: BTreeMap::new(),
        paths: BTreeSet::new(),
        directories: BTreeSet::new(),
        budget: limits.max_index_bytes,
        limits,
    };
    index.walk(root.region, root.extent, "", 0)?;
    Ok(index.files)
}

struct IsoIndex {
    volume: Region,
    files: Files,
    paths: BTreeSet<String>,
    directories: BTreeSet<u32>,
    budget: usize,
    limits: Limits,
}

impl IsoIndex {
    fn walk(&mut self, region: Region, extent: u32, prefix: &str, depth: usize) -> io::Result<()> {
        if depth > self.limits.max_depth || !self.directories.insert(extent) {
            return Err(invalid("ISO directory cycle or excessive nesting"));
        }
        let size = usize::try_from(region.len()).map_err(|_| invalid("ISO directory too large"))?;
        self.budget = self
            .budget
            .checked_sub(size)
            .ok_or_else(|| invalid("ISO directory metadata exceeds limit"))?;
        let bytes = region.read_at(0, size)?;
        let mut pos = 0;
        while pos < bytes.len() {
            let len = bytes[pos] as usize;
            if len == 0 {
                pos = (pos / BLOCK as usize + 1) * BLOCK as usize;
                continue;
            }
            if len > bytes.len() - pos || pos % BLOCK as usize + len > BLOCK as usize {
                return Err(invalid("ISO directory record crosses boundary"));
            }
            let entry = record(&bytes[pos..pos + len], &self.volume)?;
            pos += len;
            if entry.name == [0] || entry.name == [1] {
                continue;
            }
            let text = std::str::from_utf8(entry.name)
                .map_err(|_| unsupported("non-ASCII ISO identifier"))?;
            let name = if entry.directory {
                text
            } else {
                let (name, version) = text.split_once(';').unwrap_or((text, "1"));
                if version != "1" {
                    return Err(unsupported("ISO file version is not 1"));
                }
                name.strip_suffix('.').unwrap_or(name)
            };
            let key = format!("{prefix}{}", normalize_component(name)?);
            if !self.paths.insert(key.clone()) {
                return Err(invalid(format!("ambiguous ISO path: {key}")));
            }
            if self.paths.len() > self.limits.max_entries {
                return Err(invalid("ISO entry count exceeds limit"));
            }
            if entry.directory {
                self.walk(entry.region, entry.extent, &format!("{key}/"), depth + 1)?;
            } else {
                self.files.insert(key, entry.region);
            }
        }
        Ok(())
    }
}
