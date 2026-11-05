// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Load and expand the external DOS program, including its supported PKLITE wrapper.

mod pklite;

use crate::Error;
use std::path::Path;

pub const DATA_SEGMENT_OFFSET: usize = 0x95a0;

/// Borrow the data segment only after validating the expanded image boundary.
pub fn data_segment(program: &[u8]) -> Result<&[u8], Error> {
    program
        .get(DATA_SEGMENT_OFFSET..)
        .ok_or_else(|| Error::InvalidData("truncated program data segment".into()))
}

fn word(data: &[u8], offset: usize) -> Result<u16, Error> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| Error::InvalidData("truncated DOS program data".into()))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Decode the supported wrappers without executing or relocating DOS code.
/// Source and destination bounds are checked for every record.
pub fn expand_program(raw: &[u8]) -> Result<Vec<u8>, Error> {
    if pklite::recognizes(raw) {
        expand_reverse_rle(&pklite::unpack(raw)?)
    } else {
        expand_reverse_rle(raw)
    }
}

fn expand_reverse_rle(raw: &[u8]) -> Result<Vec<u8>, Error> {
    if word(raw, 0)? != 0x5a4d || word(raw, 6)? != 0 || word(raw, 20)? != 16 {
        return Err(Error::InvalidData("unsupported DOS program loader".into()));
    }
    let header = usize::from(word(raw, 8)?) * 16;
    if header < 28 {
        return Err(Error::InvalidData("invalid DOS header size".into()));
    }
    let image = raw
        .get(header..)
        .ok_or_else(|| Error::InvalidData("truncated DOS load image".into()))?;
    let stub_offset = usize::from(word(raw, 22)?) * 16;
    let stub = image
        .get(stub_offset..)
        .ok_or_else(|| Error::InvalidData("missing DOS loader".into()))?;
    if word(stub, 14)? != 0x4252 {
        return Err(Error::InvalidData("unsupported reverse-RLE marker".into()));
    }
    let size = usize::from(word(stub, 12)?) * 16;
    if size == 0 || size > 1_048_576 {
        return Err(Error::InvalidData("invalid expanded DOS image size".into()));
    }
    let mut result = vec![0; size];
    let copied = image.len().min(size);
    result[..copied].copy_from_slice(&image[..copied]);
    // Exclusive source pointer, initially just before the loader alignment fill.
    let mut source = stub_offset;
    while source > 0 && image[source - 1] == 255 {
        source -= 1;
    }
    let mut destination = size;
    loop {
        let record = source
            .checked_sub(3)
            .ok_or_else(|| Error::InvalidData("truncated reverse-RLE record".into()))?;
        let opcode = image[source - 1];
        let count = usize::from(word(image, record)?);
        source = record;
        destination = destination
            .checked_sub(count)
            .ok_or_else(|| Error::InvalidData("reverse-RLE destination underflow".into()))?;
        match opcode & 0xfe {
            0xb0 => {
                source = source
                    .checked_sub(1)
                    .ok_or_else(|| Error::InvalidData("truncated reverse-RLE fill".into()))?;
                result[destination..destination + count].fill(image[source]);
            }
            0xb2 => {
                let start = source
                    .checked_sub(count)
                    .ok_or_else(|| Error::InvalidData("truncated reverse-RLE literal".into()))?;
                result[destination..destination + count].copy_from_slice(&image[start..source]);
                source = start;
            }
            _ => {
                return Err(Error::InvalidData(format!(
                    "invalid reverse-RLE opcode {opcode:02x}"
                )));
            }
        }
        if opcode & 1 != 0 {
            break;
        }
    }
    if source != destination {
        return Err(Error::InvalidData(
            "reverse-RLE prefix bounds disagree".into(),
        ));
    }
    Ok(result)
}

/// Find MACH3.EXE case-insensitively in the configured external asset directory.
pub fn load_program(directory: &Path) -> Result<Vec<u8>, Error> {
    let matches = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("MACH3.EXE"))
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(Error::InvalidData(
            "expected one MACH3.EXE in the asset directory".into(),
        ));
    }
    expand_program(&std::fs::read(&matches[0])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> Vec<u8> {
        let mut raw = vec![0; 32];
        for (offset, value) in [(0, 0x5a4d_u16), (8, 2), (20, 16), (22, 3)] {
            raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        raw.extend([0x11; 16]);
        raw.extend([0xaa, 16, 0, 0xb1]);
        raw.extend([0xbb; 16]);
        raw.extend([16, 0, 0xb2]);
        raw.extend([0xcc, 16, 0, 0xb0]);
        raw.resize(32 + 48, 255);
        let mut stub = [0; 16];
        stub[12..14].copy_from_slice(&4_u16.to_le_bytes());
        stub[14..16].copy_from_slice(&0x4252_u16.to_le_bytes());
        raw.extend(stub);
        raw
    }

    #[test]
    fn expands_reverse_records_and_preserves_the_untouched_prefix() {
        let mut expected = vec![0x11; 16];
        expected.extend([0xaa; 16]);
        expected.extend([0xbb; 16]);
        expected.extend([0xcc; 16]);
        assert_eq!(expand_program(&program()).unwrap(), expected);
    }

    #[test]
    fn pklite_then_reverse_rle_matches_the_unwrapped_program() {
        let raw = program();
        let packed = pklite::tests::literals(&raw);
        assert_eq!(
            expand_program(&packed).unwrap(),
            expand_program(&raw).unwrap()
        );
    }

    #[test]
    fn rejects_truncation_bad_markers_and_overrunning_records() {
        let p = program();
        for end in 0..p.len() {
            assert!(expand_program(&p[..end]).is_err());
        }
        for (offset, value) in [
            (32 + 42, 0xaf),
            (32 + 46 + 14, 0),
            (32 + 40, 255),
            (32 + 41, 255),
        ] {
            let mut corrupt = p.clone();
            corrupt[offset] = value;
            assert!(expand_program(&corrupt).is_err());
        }
    }
}
