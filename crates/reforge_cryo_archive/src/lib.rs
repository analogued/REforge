// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later

//! Ordered resource directories used by Cryo's DOS CD games.

use std::io;

use reforge_disc::Region;

const DIRECTORY_SIZE: usize = 65_536;
const RECORD_SIZE: usize = 25;
const COUNT_SIZE: usize = 2;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub region: Region,
}

/// Read the directory of a caller-selected archive. Entries retain their
/// original order, spelling, and DOS path separators, including duplicates.
/// The returned regions can be opened independently for bounded reads.
pub fn read_directory(archive: &Region) -> io::Result<Vec<Entry>> {
    let header = archive.read_at(0, DIRECTORY_SIZE)?;
    let count = u16::from_le_bytes(header[..COUNT_SIZE].try_into().unwrap()) as usize;
    let records = header[COUNT_SIZE..].as_chunks::<RECORD_SIZE>().0;
    let records = records.get(..count).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Cryo archive directory exceeds header",
        )
    })?;
    let mut entries = Vec::new();

    for record in records {
        if record.iter().all(|&byte| byte == 0) {
            break;
        }
        let name = &record[..16];
        let end = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        let name = std::str::from_utf8(&name[..end])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let len = u32::from_le_bytes(record[16..20].try_into().unwrap());
        let offset = u32::from_le_bytes(record[20..24].try_into().unwrap());
        if u64::from(offset) < DIRECTORY_SIZE as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Resource {name} overlaps index"),
            ));
        }
        let region = archive
            .slice(u64::from(offset), u64::from(len))
            .map_err(|error| io::Error::new(error.kind(), format!("{name}: {error}")))?;
        entries.push(Entry {
            name: name.to_owned(),
            region,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, File},
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "reforge-cryo-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("archive"), bytes).unwrap();
            Self(path)
        }

        fn directory(&self) -> io::Result<Vec<Entry>> {
            let archive = Region::from_file(File::open(self.0.join("archive"))?)?;
            read_directory(&archive)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn record(name: &[u8], len: u32, offset: u32) -> [u8; RECORD_SIZE] {
        let mut record = [0; RECORD_SIZE];
        record[..name.len()].copy_from_slice(name);
        record[16..20].copy_from_slice(&len.to_le_bytes());
        record[20..24].copy_from_slice(&offset.to_le_bytes());
        record
    }

    #[test]
    fn preserves_names_and_duplicates_before_zero_record() {
        let start = DIRECTORY_SIZE as u32;
        let mut bytes = vec![5, 0];
        bytes.extend_from_slice(&record(br"SD\Test.VOC", 1, start));
        bytes.extend_from_slice(&record(br"SD\Test.VOC", 1, start + 1));
        bytes.extend_from_slice(&record(b"0123456789ABCDEF", 1, start + 2));
        bytes.extend_from_slice(&[0; RECORD_SIZE]);
        bytes.extend_from_slice(&record(b"IGNORED.BIN", 1, start + 3));
        bytes.resize(DIRECTORY_SIZE, 0);
        bytes.extend_from_slice(b"abcd");

        let fixture = Fixture::new(&bytes);
        let entries = fixture.directory().unwrap();
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, [r"SD\Test.VOC", r"SD\Test.VOC", "0123456789ABCDEF"]);
        for (entry, byte) in entries.iter().zip(b"abc") {
            assert_eq!(entry.region.read_at(0, 1).unwrap(), [*byte]);
            assert!(entry.region.read_at(0, 2).is_err());
        }
    }

    #[test]
    fn rejects_directory_and_member_extents_outside_bounds() {
        let start = DIRECTORY_SIZE as u32;
        let mut bytes = vec![0; DIRECTORY_SIZE];
        bytes[..2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(Fixture::new(&bytes).directory().is_err());

        bytes[..2].copy_from_slice(&1_u16.to_le_bytes());
        for member in [
            record(b"OVERLAP", 1, start - 1),
            record(b"TRUNCATED", 1, start),
        ] {
            bytes[2..2 + RECORD_SIZE].copy_from_slice(&member);
            assert!(Fixture::new(&bytes).directory().is_err());
        }
    }
}
