// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

pub fn word(bytes: &[u8], offset: usize) -> Result<u16> {
    let pair = bytes.get(offset..offset + 2).ok_or("Truncated word")?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

fn dword(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes.get(offset..offset + 4).ok_or("Truncated dword")?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

pub fn lzss(payload: &[u8]) -> Result<Vec<u8>> {
    let expected = dword(payload, 0)? as usize;
    if expected > 1_048_576 {
        return Err("Unexpected decompressed resource size".into());
    }
    let mut ring = [b' '; 4096];
    let mut defined = [true; 4096];
    defined[0xfee..].fill(false);
    let mut write = 0xfee;
    let mut source = 4;
    let mut output = Vec::with_capacity(expected);
    while source < payload.len() {
        let flags = payload[source];
        source += 1;
        for bit in 0..8 {
            if source == payload.len() {
                break;
            }
            let (position, count, literal) = if flags & (1 << bit) != 0 {
                let value = payload[source];
                source += 1;
                (0, 1, Some(value))
            } else {
                let low = *payload.get(source).ok_or("Truncated match")?;
                let high = *payload.get(source + 1).ok_or("Truncated match")?;
                source += 2;
                (
                    (low as usize) | (((high & 0xf0) as usize) << 4),
                    (high & 15) as usize + 3,
                    None,
                )
            };
            for n in 0..count {
                let value = if let Some(value) = literal {
                    value
                } else {
                    let address = (position + n) & 4095;
                    if !defined[address] {
                        return Err("Read from undefined dictionary tail".into());
                    }
                    ring[address]
                };
                output.push(value);
                if output.len() > expected {
                    return Err("Resource exceeds declared decoded size".into());
                }
                ring[write] = value;
                defined[write] = true;
                write = (write + 1) & 4095;
            }
        }
    }
    if output.len() != expected {
        return Err(format!(
            "Decoded {} bytes, expected {expected}",
            output.len()
        ));
    }
    Ok(output)
}

#[derive(Clone)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub origin_x: i16,
    pub origin_y: i16,
    pub pixels: Vec<u8>,
    pub visible: Vec<bool>,
}

impl Bitmap {
    fn packed(header: &[u8], data: &[u8], maps: &[u8]) -> Result<Self> {
        if header.len() < 10 {
            return Err("Truncated bitmap header".into());
        }
        let width = word(header, 0)? as usize;
        let stride = header[6] as usize;
        let height = header[7] as usize;
        if width == 0 || width > stride * 2 || data.len() != stride * height {
            return Err(format!(
                "Invalid bitmap dimensions {width}x{height}, stride {stride}, data {}",
                data.len()
            ));
        }
        let lookup_start = header[9] as usize * 16;
        let lookup = maps
            .get(lookup_start..lookup_start + 16)
            .ok_or("Invalid bitmap palette tag")?;
        let mut pixels = Vec::with_capacity(width * height);
        let mut visible = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let packed = data[y * stride + x / 2];
                let nibble = if x % 2 == 0 { packed >> 4 } else { packed & 15 };
                pixels.push(lookup[nibble as usize]);
                visible.push(nibble != header[8]);
            }
        }
        Ok(Self {
            width,
            height,
            origin_x: word(header, 2)? as i16,
            origin_y: word(header, 4)? as i16,
            pixels,
            visible,
        })
    }
}

struct Bank {
    data: Vec<u8>,
    entries: BTreeMap<String, (usize, usize, u8)>,
}

pub struct Assets {
    root: PathBuf,
    banks: BTreeMap<String, Bank>,
    pub data_segment: Vec<u8>,
    pub palette: [[u8; 3]; 256],
}

impl Assets {
    pub fn open(root: &Path) -> Result<Self> {
        let ovl = read(&root.join("OVL0.OVL"))?;
        if ovl.len() != 96_272 || ovl.get(0xb86e..0xb883) != Some(b"Version 1.00A 5/25/90") {
            return Err("This prototype requires the selected DOS 1.00A reference; OVL0.OVL does not match its layout".into());
        }
        let data_segment = ovl[0xb400..].to_vec();
        let mut banks = BTreeMap::new();
        for name in [
            "CENT", "MINIT", "MMAP", "BATL", "MGLAD", "MRACE", "MNAVAL", "NAVL", "MNEGO",
        ] {
            let directory = read(&root.join(format!("{name}.DIR")))?;
            let data = read(&root.join(format!("{name}.DAT")))?;
            if directory.len() % 20 != 0 || data.get(..4) != Some(&[0xca, 0xca, 0xd0, 0xd0]) {
                return Err(format!("Invalid {name} resource bank"));
            }
            let mut entries = BTreeMap::new();
            let mut ranges = Vec::new();
            for record in directory.as_chunks::<20>().0 {
                let start = dword(record, 0)? as usize;
                let size = word(record, 4)? as usize;
                let raw = &record[6..19];
                let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
                let member = std::str::from_utf8(&raw[..end])
                    .map_err(|e| e.to_string())?
                    .to_uppercase();
                if start < 4
                    || start + size > data.len()
                    || entries.insert(member, (start, size, record[19])).is_some()
                {
                    return Err(format!("Invalid or duplicate {name} index entry"));
                }
                ranges.push((start, size));
            }
            ranges.sort_unstable();
            let mut cursor = 4;
            for (start, size) in ranges {
                if start != cursor {
                    return Err(format!("Gap or overlap in {name}"));
                }
                cursor += size;
            }
            if cursor != data.len() {
                return Err(format!("Unindexed bytes in {name}"));
            }
            banks.insert(name.into(), Bank { data, entries });
        }
        let mut assets = Self {
            root: root.to_path_buf(),
            banks,
            data_segment,
            palette: [[0; 3]; 256],
        };
        let palette = assets.resource("CENT", "CMAP.BIN")?;
        if palette.len() != 750 {
            return Err("Unexpected CMAP.BIN length".into());
        }
        for (index, rgb) in palette.as_chunks::<3>().0.iter().enumerate() {
            if rgb.iter().any(|v| *v > 63) {
                return Err("Invalid VGA DAC value".into());
            }
            assets.palette[index] = [expand(rgb[0]), expand(rgb[1]), expand(rgb[2])];
        }
        // CMAP defines 250 DAC entries. The remaining entries are not inferred.
        Ok(assets)
    }

    pub fn resource(&self, bank: &str, name: &str) -> Result<Vec<u8>> {
        let bank = self.banks.get(bank).ok_or("Unknown bank")?;
        let &(start, size, flags) = bank
            .entries
            .get(name)
            .ok_or_else(|| format!("Missing resource {name}"))?;
        let payload = &bank.data[start..start + size];
        if flags == 0x80 {
            lzss(payload)
        } else {
            Ok(payload.to_vec())
        }
    }

    pub fn bitmap(&self, bank: &str, name: &str) -> Result<Bitmap> {
        let payload = self.resource(bank, name)?;
        let compressed = payload.get(10..).ok_or("Truncated CBM")?;
        let data = lzss(compressed)?;
        Bitmap::packed(&payload[..10], &data, &self.data_segment[0x60c0..0x6300])
    }

    pub fn sprites(&self, bank: &str, stem: &str) -> Result<Vec<Bitmap>> {
        self.sprites_with_pens(bank, stem, None)
    }

    pub fn sprites_with_pens(
        &self,
        bank: &str,
        stem: &str,
        pens: Option<&[u8; 16]>,
    ) -> Result<Vec<Bitmap>> {
        let headers = self.resource(bank, &format!("{stem}HDR.LZW"))?;
        let data = self.resource(bank, &format!("{stem}DAT.LZW"))?;
        if headers.len() % 14 != 0 {
            return Err("Invalid sprite header array".into());
        }
        headers
            .as_chunks::<14>()
            .0
            .iter()
            .map(|header| {
                // The DOS loader reserves a two-byte size word before the
                // decoded DAT payload; header offsets include that word.
                let start = (word(header, 10)? as usize)
                    .checked_sub(2)
                    .ok_or("Sprite offset precedes the payload")?;
                let size = header[6] as usize * header[7] as usize;
                let payload = data
                    .get(start..start + size)
                    .ok_or("Sprite pixels out of range")?;
                if let Some(pens) = pens {
                    let mut mapped_header = *header;
                    mapped_header[8] = pens[usize::from(header[8] & 15)];
                    let mapped: Vec<u8> = payload
                        .iter()
                        .map(|byte| {
                            pens[usize::from(byte >> 4)] << 4 | pens[usize::from(byte & 15)]
                        })
                        .collect();
                    Bitmap::packed(&mapped_header, &mapped, &self.data_segment[0x60c0..0x6300])
                } else {
                    Bitmap::packed(header, payload, &self.data_segment[0x60c0..0x6300])
                }
            })
            .collect()
    }

    pub fn cached_bitmap(&self, bank: &str, name: &str) -> Result<Bitmap> {
        let payload = self.resource(bank, name)?;
        let mut bitmap = self.bitmap(bank, name)?;
        // 0000:151c puts the packed input at packed_size-1 in the expanded
        // allocation. 88a7 expands forward and reads each source byte twice.
        // For the final pair, its first output overwrites the source before
        // the low nibble is read. Preserve this original aliasing behavior.
        if bitmap.width == payload[6] as usize * 2 && bitmap.pixels.len() >= 2 {
            let end = bitmap.pixels.len();
            let nibble = bitmap.pixels[end - 2] & 15;
            bitmap.pixels[end - 1] =
                self.data_segment[0x60c0 + payload[9] as usize * 16 + nibble as usize];
        }
        Ok(bitmap)
    }

    pub fn validate_all(&self) -> Result<(usize, usize, usize, usize)> {
        let (mut raw, mut streams, mut bitmaps, mut sprites) = (0, 0, 0, 0);
        for (bank, contents) in &self.banks {
            for (name, &(_, _, flags)) in &contents.entries {
                if let Some(stem) = name.strip_suffix("HDR.LZW") {
                    sprites += self.sprites(bank, stem)?.len();
                }
                match flags {
                    0 => {
                        self.resource(bank, name)?;
                        raw += 1;
                    }
                    0x80 => {
                        self.resource(bank, name)?;
                        streams += 1;
                    }
                    0xc0 => {
                        self.bitmap(bank, name)
                            .map_err(|e| format!("{bank}/{name}: {e}"))?;
                        bitmaps += 1;
                    }
                    _ => return Err(format!("Unknown resource flags {flags:#x}")),
                }
            }
        }
        Ok((raw, streams, bitmaps, sprites))
    }

    pub fn original(&self) -> &Path {
        &self.root
    }
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn expand(value: u8) -> u8 {
    (value << 2) | (value >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_match() {
        assert_eq!(
            lzss(&[7, 0, 0, 0, 1, b'A', 0xee, 0xf3]).unwrap(),
            b"AAAAAAA"
        );
    }

    #[test]
    fn invalid_streams() {
        assert!(lzss(&[3, 0, 0, 0, 0, 1]).is_err());
        assert!(lzss(&[3, 0, 0, 0, 0, 0xff, 0xf0]).is_err());
        assert!(lzss(&[2, 0, 0, 0, 1, b'A']).is_err());
    }

    #[test]
    fn packed_pixels_keep_transparency_before_remap() {
        let mut lookup: Vec<u8> = (0..16).collect();
        lookup[1] = 0; // A visible pen may map to zero; it remains visible.
        let bitmap =
            Bitmap::packed(&[3, 0, 1, 0, 2, 0, 2, 1, 0, 0], &[0x01, 0x20], &lookup).unwrap();
        assert_eq!(bitmap.pixels, [0, 0, 2]);
        assert_eq!(bitmap.visible, [false, true, true]);
        assert_eq!((bitmap.origin_x, bitmap.origin_y), (1, 2));
    }
}
