// Copyright (c) 2026 analogued
// The project license has not yet been chosen.

//! Decode the original packed graphics into row-major CGA bytes.

use std::fs;
use std::path::Path;

use crate::Error;

const PRIMARY_BYTES: usize = 28_960;
const ALTERNATE_BYTES: usize = 16_000;
const SPRITE_COUNT: usize = 190;
const FIRST_PIXELS: usize = 0x207;
const HEADER_BYTES: usize = 7;

/// A color sprite and its paired mask, with each row padded to an even width.
#[derive(Debug)]
pub struct Sprite {
    pub width_bytes: usize,
    pub height: usize,
    /// Runtime color descriptor: three unsigned fields, then four signed fields.
    pub descriptor: [i16; 7],
    pub color: Vec<u8>,
    pub mask: Vec<u8>,
}

#[derive(Debug)]
pub struct GraphicsAssets {
    /// The 320x200 background followed by twelve 288x15 terrain frames.
    pub primary: Vec<u8>,
    /// The alternative 320x200 background.
    pub alternate: Vec<u8>,
    pub sprites: Vec<Sprite>,
}

impl GraphicsAssets {
    /// Decode DATA2 through DATA5 without changing the supplied bytes.
    pub fn decode(data2: &[u8], data3: &[u8], data4: &[u8], data5: &[u8]) -> Result<Self, Error> {
        check_length(data2, PRIMARY_BYTES, "DATA2")?;
        check_length(data3, ALTERNATE_BYTES, "DATA3")?;
        let colors = records(data4, "DATA4")?;
        let masks = records(data5, "DATA5")?;
        let mut sprites = Vec::with_capacity(SPRITE_COUNT);
        for (index, (color, mask)) in colors.iter().zip(&masks).enumerate() {
            if (color.width, color.height) != (mask.width, mask.height) {
                return Err(invalid(format!(
                    "sprite {index}: DATA4 and DATA5 dimensions differ"
                )));
            }
            let width_bytes = (color.width + 1) & !1;
            let mut descriptor = color.header.map(i16::from);
            descriptor[3] = width_bytes as i16;
            for field in &mut descriptor[3..] {
                *field = (*field as u8 as i8) as i16;
            }
            sprites.push(Sprite {
                width_bytes,
                height: color.height,
                descriptor,
                color: padded_pixels(color, width_bytes, 0),
                mask: padded_pixels(mask, width_bytes, 0xff),
            });
        }
        Ok(Self {
            primary: data2.iter().copied().map(interleave).collect(),
            alternate: data3.iter().copied().map(interleave).collect(),
            sprites,
        })
    }

    /// Read DOS resource names case-insensitively, rejecting ambiguous matches.
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, Error> {
        Self::load_with(|name| read_resource(directory.as_ref(), name))
    }

    pub(crate) fn load_with(
        mut read: impl FnMut(&str) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        let data2 = read("DATA2")?;
        let data3 = read("DATA3")?;
        let data4 = read("DATA4")?;
        let data5 = read("DATA5")?;
        Self::decode(&data2, &data3, &data4, &data5)
    }
}

/// Read a DOS resource name case-insensitively, rejecting ambiguous matches.
pub(crate) fn read_resource(directory: &Path, name: &str) -> Result<Vec<u8>, Error> {
    let mut path = None;
    for entry in fs::read_dir(directory).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        if filename.eq_ignore_ascii_case(name) {
            if path.is_some() {
                return Err(invalid(format!("ambiguous resource name {name}")));
            }
            path = Some(entry.path());
        }
    }
    let path = path.ok_or_else(|| invalid(format!("missing resource {name}")))?;
    fs::read(path).map_err(Error::Io)
}

struct Record<'a> {
    header: [u8; HEADER_BYTES],
    width: usize,
    height: usize,
    pixels: &'a [u8],
}

fn records<'a>(input: &'a [u8], name: &str) -> Result<Vec<Record<'a>>, Error> {
    let table_bytes = (SPRITE_COUNT + 1) * 2;
    if input.len() < table_bytes {
        return Err(invalid(format!("{name}: truncated sprite offset table")));
    }
    let word = |index: usize| u16::from_le_bytes([input[index * 2], input[index * 2 + 1]]);
    let base = word(0);
    if base == 0 || word(SPRITE_COUNT) != 0 {
        return Err(invalid(format!(
            "{name}: expected {SPRITE_COUNT} sprite offsets and a zero sentinel"
        )));
    }
    let mut result = Vec::with_capacity(SPRITE_COUNT);
    let mut previous_end = FIRST_PIXELS - HEADER_BYTES;
    for index in 0..SPRITE_COUNT {
        let pointer = word(index);
        let relative = pointer
            .checked_sub(base)
            .ok_or_else(|| invalid(format!("{name} sprite {index}: offset precedes table base")))?;
        let start = usize::from(relative) + FIRST_PIXELS;
        let header_start = start - HEADER_BYTES;
        if pointer == 0 || header_start < previous_end {
            return Err(invalid(format!(
                "{name} sprite {index}: overlapping or unordered record"
            )));
        }
        let header: [u8; HEADER_BYTES] = input
            .get(header_start..start)
            .ok_or_else(|| invalid(format!("{name} sprite {index}: truncated header")))?
            .try_into()
            .expect("the header range has seven bytes");
        let width = usize::from(header[3]);
        let height = usize::from(header[4]);
        if width == 0 || width >= 128 || height == 0 {
            return Err(invalid(format!(
                "{name} sprite {index}: invalid dimensions {width} bytes by {height} rows"
            )));
        }
        // Byte dimensions bound this multiplication and the u16 offset bounds start.
        let end = start + width * height;
        let pixels = input
            .get(start..end)
            .ok_or_else(|| invalid(format!("{name} sprite {index}: truncated pixels")))?;
        result.push(Record {
            header,
            width,
            height,
            pixels,
        });
        previous_end = end;
    }
    Ok(result)
}

fn padded_pixels(record: &Record<'_>, width_bytes: usize, padding: u8) -> Vec<u8> {
    let mut decoded = vec![padding; width_bytes * record.height];
    for (source, destination) in record
        .pixels
        .chunks_exact(record.width)
        .zip(decoded.chunks_exact_mut(width_bytes))
    {
        for (source, destination) in source.iter().zip(destination) {
            *destination = interleave(*source);
        }
    }
    decoded
}

fn interleave(byte: u8) -> u8 {
    // Output bits 7..0 come from input bits 3,7,2,6,1,5,0,4.
    ((byte & 0x08) << 4)
        | ((byte & 0x80) >> 1)
        | ((byte & 0x04) << 3)
        | ((byte & 0x40) >> 2)
        | ((byte & 0x02) << 2)
        | ((byte & 0x20) >> 3)
        | ((byte & 0x01) << 1)
        | ((byte & 0x10) >> 4)
}

fn check_length(input: &[u8], expected: usize, name: &str) -> Result<(), Error> {
    if input.len() != expected {
        return Err(invalid(format!(
            "{name}: expected {expected} bytes, found {}",
            input.len()
        )));
    }
    Ok(())
}

fn invalid(message: String) -> Error {
    Error::InvalidData(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sprite_file(header: [u8; 7], pixels: &[u8]) -> Vec<u8> {
        assert_eq!(
            pixels.len(),
            usize::from(header[3]) * usize::from(header[4])
        );
        let mut input = vec![0; FIRST_PIXELS - HEADER_BYTES];
        for index in 0..SPRITE_COUNT {
            let (header, pixels) = if index == 0 {
                (header, pixels)
            } else {
                ([0, 0, 0, 2, 1, 0, 0], &[0, 0][..])
            };
            let start = input.len() + HEADER_BYTES;
            let pointer = u16::try_from(0x2000 + start - FIRST_PIXELS).unwrap();
            input[index * 2..index * 2 + 2].copy_from_slice(&pointer.to_le_bytes());
            input.extend_from_slice(&header);
            input.extend_from_slice(pixels);
        }
        input
    }

    fn valid_sprites() -> Vec<u8> {
        sprite_file([0, 0, 0, 2, 1, 0, 0], &[0, 0])
    }

    fn decode_pair(color: &[u8], mask: &[u8]) -> Result<GraphicsAssets, Error> {
        GraphicsAssets::decode(
            &vec![0; PRIMARY_BYTES],
            &vec![0; ALTERNATE_BYTES],
            color,
            mask,
        )
    }

    fn assert_invalid(result: Result<GraphicsAssets, Error>, message: &str) {
        match result {
            Err(Error::InvalidData(error)) => assert!(error.contains(message), "{error}"),
            _ => panic!("expected InvalidData containing {message}"),
        }
    }

    #[test]
    fn interleaves_each_source_bit_into_cga_pixels() {
        let expected = [0x02, 0x08, 0x20, 0x80, 0x01, 0x04, 0x10, 0x40];
        for (bit, output) in expected.into_iter().enumerate() {
            assert_eq!(interleave(1 << bit), output);
        }
        assert_eq!(interleave(0xff), 0xff);
        let mut primary = vec![0; PRIMARY_BYTES];
        primary[0] = 0x08;
        primary[PRIMARY_BYTES - 1] = 0x80;
        let mut alternate = vec![0; ALTERNATE_BYTES];
        alternate[0] = 0x04;
        let raw = valid_sprites();
        let assets = GraphicsAssets::decode(&primary, &alternate, &raw, &raw).unwrap();
        assert_eq!(assets.primary[0], 0x80);
        assert_eq!(assets.primary[PRIMARY_BYTES - 1], 0x40);
        assert_eq!(assets.alternate[0], 0x20);
        assert_eq!(assets.sprites.len(), SPRITE_COUNT);
    }

    #[test]
    fn pads_rows_and_sign_extends_color_descriptors() {
        let color = sprite_file([255, 128, 200, 3, 2, 255, 128], &[8, 128, 4, 64, 2, 32]);
        let mask = sprite_file([0, 0, 0, 3, 2, 0, 0], &[1, 16, 255, 0, 8, 128]);
        let assets = decode_pair(&color, &mask).unwrap();
        let sprite = &assets.sprites[0];
        assert_eq!(sprite.width_bytes, 4);
        assert_eq!(sprite.height, 2);
        assert_eq!(sprite.descriptor, [255, 128, 200, 4, 2, -1, -128]);
        assert_eq!(sprite.color, [128, 64, 32, 0, 16, 8, 4, 0]);
        assert_eq!(sprite.mask, [2, 1, 255, 255, 0, 128, 64, 255]);
    }

    #[test]
    fn keeps_dimensions_unsigned_when_descriptors_are_signed() {
        let raw = sprite_file([0, 0, 0, 127, 128, 0, 0], &vec![0; 127 * 128]);
        let assets = decode_pair(&raw, &raw).unwrap();
        assert_eq!(assets.sprites[0].width_bytes, 128);
        assert_eq!(assets.sprites[0].height, 128);
        assert_eq!(assets.sprites[0].descriptor[3..5], [-128, -128]);
        assert_eq!(assets.sprites[0].color.len(), 128 * 128);
    }

    #[test]
    fn loads_case_insensitive_names_and_rejects_missing_or_ambiguous_files() {
        struct TestDirectory(PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("mach3-assets-test-{}-{suffix}", std::process::id())),
        );
        fs::create_dir(&directory.0).unwrap();
        fs::write(directory.0.join("DaTa2"), vec![0; PRIMARY_BYTES]).unwrap();
        fs::write(directory.0.join("data3"), vec![0; ALTERNATE_BYTES]).unwrap();
        let raw = valid_sprites();
        fs::write(directory.0.join("Data4"), &raw).unwrap();
        assert_invalid(GraphicsAssets::load(&directory.0), "missing resource DATA5");
        fs::write(directory.0.join("DATA5"), raw).unwrap();
        assert_eq!(
            GraphicsAssets::load(&directory.0).unwrap().sprites.len(),
            SPRITE_COUNT
        );
        fs::write(directory.0.join("DATA2"), vec![0; PRIMARY_BYTES]).unwrap();
        // Case-insensitive filesystems cannot store both spellings separately.
        let primary_matches = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_str().unwrap().eq_ignore_ascii_case("DATA2"))
            .count();
        if primary_matches == 2 {
            assert_invalid(
                GraphicsAssets::load(&directory.0),
                "ambiguous resource name DATA2",
            );
        }
    }

    #[test]
    fn rejects_wrong_background_lengths() {
        let raw = valid_sprites();
        assert_invalid(
            GraphicsAssets::decode(&[], &vec![0; ALTERNATE_BYTES], &raw, &raw),
            "DATA2",
        );
        assert_invalid(
            GraphicsAssets::decode(
                &vec![0; PRIMARY_BYTES],
                &vec![0; ALTERNATE_BYTES + 1],
                &raw,
                &raw,
            ),
            "DATA3",
        );
    }

    #[test]
    fn rejects_truncated_tables_headers_and_pixels() {
        let raw = valid_sprites();
        assert_invalid(decode_pair(&raw[..100], &raw), "offset table");
        assert_invalid(
            decode_pair(&raw[..FIRST_PIXELS - 1], &raw),
            "truncated header",
        );
        assert_invalid(decode_pair(&raw[..raw.len() - 1], &raw), "truncated pixels");
    }

    #[test]
    fn rejects_missing_sentinel_and_invalid_offsets() {
        let raw = valid_sprites();
        let mut corrupt = raw.clone();
        corrupt[SPRITE_COUNT * 2] = 1;
        assert_invalid(decode_pair(&corrupt, &raw), "zero sentinel");
        let mut corrupt = raw.clone();
        corrupt[2..4].copy_from_slice(&0x1fffu16.to_le_bytes());
        assert_invalid(decode_pair(&corrupt, &raw), "precedes table base");
        let mut corrupt = raw.clone();
        corrupt[2..4].copy_from_slice(&0x2000u16.to_le_bytes());
        assert_invalid(decode_pair(&corrupt, &raw), "overlapping");
        let mut corrupt = raw.clone();
        corrupt[..2].copy_from_slice(&0u16.to_le_bytes());
        assert_invalid(decode_pair(&corrupt, &raw), "sprite offsets");
    }

    #[test]
    fn rejects_invalid_and_unpaired_dimensions() {
        let raw = valid_sprites();
        for (field, value) in [(3, 0), (3, 128), (4, 0)] {
            let mut corrupt = raw.clone();
            corrupt[FIRST_PIXELS - HEADER_BYTES + field] = value;
            assert_invalid(decode_pair(&corrupt, &raw), "invalid dimensions");
        }
        let other = sprite_file([0, 0, 0, 1, 2, 0, 0], &[0, 0]);
        assert_invalid(decode_pair(&other, &raw), "dimensions differ");
    }
}
