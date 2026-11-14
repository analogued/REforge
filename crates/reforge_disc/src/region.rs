// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::invalid;

#[derive(Clone, Debug)]
enum Backing {
    Path(PathBuf),
    Open(Arc<Mutex<File>>),
}

/// A physical byte range. Subranges never read outside their enclosing container.
#[derive(Clone, Debug)]
pub struct Region {
    backing: Backing,
    start: u64,
    len: u64,
}

impl Region {
    pub(crate) fn path(path: PathBuf, len: u64) -> Self {
        Self {
            backing: Backing::Path(path),
            start: 0,
            len,
        }
    }

    pub fn from_file(file: File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid("disc source is not a regular file"));
        }
        let len = metadata.len();
        Ok(Self {
            backing: Backing::Open(Arc::new(Mutex::new(file))),
            start: 0,
            len,
        })
    }

    /// Length of the region, independent of its enclosing physical file.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create a checked subrange without reading or copying its payload.
    pub fn slice(&self, start: u64, len: u64) -> io::Result<Self> {
        if start > self.len || len > self.len - start {
            return Err(invalid("byte range exceeds container"));
        }
        Ok(Self {
            backing: self.backing.clone(),
            start: self.start + start,
            len,
        })
    }

    /// Open an independent logical position. Cloned regions can safely share
    /// the same physical file while readers seek within their own ranges.
    pub fn open(&self) -> io::Result<RegionReader> {
        let file = match &self.backing {
            Backing::Open(file) => file.clone(),
            Backing::Path(path) => {
                let file = File::open(path)?;
                if !file.metadata()?.is_file() || file.metadata()?.len() < self.start + self.len {
                    return Err(invalid("source file changed or was truncated"));
                }
                Arc::new(Mutex::new(file))
            }
        };
        Ok(RegionReader {
            file,
            start: self.start,
            len: self.len,
            pos: 0,
        })
    }

    pub fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let region = self.slice(offset, len as u64)?;
        let mut bytes = vec![0; len];
        region.open()?.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

/// An independent read/seek cursor confined to one region. Seeking beyond
/// its logical end is permitted, as for `File`; subsequent reads return EOF.
/// Negative or overflowing seeks fail without changing the cursor.
#[derive(Debug)]
pub struct RegionReader {
    file: Arc<Mutex<File>>,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for RegionReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.len.saturating_sub(self.pos);
        let count = remaining.min(buf.len() as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("source lock poisoned"))?;
        file.seek(SeekFrom::Start(self.start + self.pos))?;
        let read = file.read(&mut buf[..count])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source file truncated",
            ));
        }
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for RegionReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(pos) => i128::from(pos),
            SeekFrom::End(delta) => i128::from(self.len) + i128::from(delta),
            SeekFrom::Current(delta) => i128::from(self.pos) + i128::from(delta),
        };
        self.pos = u64::try_from(pos)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
        Ok(self.pos)
    }
}
