// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "asset_layout.rs"]
mod layout;

/// Data layout selected by application release detection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DosRelease {
    #[default]
    V100A,
    V100B,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VideoMode {
    #[default]
    Vga,
    Ega,
}

impl VideoMode {
    /// Original drawing routines sometimes select different pens by video mode.
    pub(crate) const fn color(self, vga: u8, ega: u8) -> u8 {
        match self {
            Self::Vga => vga,
            Self::Ega => ega,
        }
    }
}

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
    pub release: DosRelease,
    pub video: VideoMode,
    root: PathBuf,
    banks: BTreeMap<String, Bank>,
    #[cfg(feature = "dev-tools")]
    sources: Vec<PathBuf>,
    pub data_segment: Vec<u8>,
    pub palette: [[u8; 3]; 256],
}

impl Assets {
    #[cfg(test)]
    pub(crate) fn test_data(data_segment: Vec<u8>) -> Self {
        Self {
            release: DosRelease::V100A,
            video: VideoMode::Vga,
            root: PathBuf::new(),
            banks: BTreeMap::new(),
            #[cfg(feature = "dev-tools")]
            sources: Vec::new(),
            data_segment,
            palette: [[0; 3]; 256],
        }
    }

    pub fn open(root: &Path) -> Result<Self> {
        Self::open_with_video(root, VideoMode::Vga)
    }

    /// Explicit raw research entry point; normal play uses catalog-selected roles.
    pub fn open_with_video(root: &Path, video: VideoMode) -> Result<Self> {
        let entries: BTreeMap<_, _> = fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .map(|entry| {
                entry.map_err(|e| e.to_string()).map(|e| {
                    (
                        e.file_name().to_string_lossy().to_ascii_uppercase(),
                        e.path(),
                    )
                })
            })
            .collect::<Result<_>>()?;
        let read_role = |name: &str| {
            read(
                entries
                    .get(name)
                    .ok_or_else(|| format!("Missing research resource {name}"))?,
            )
        };
        let ovl = read_role("OVL0.OVL")?;
        let release = match ovl.get(0xb86e..0xb883) {
            Some(b"Version 1.00A 5/25/90") => DosRelease::V100A,
            Some(b"Version 1.00B 5/29/90") => DosRelease::V100B,
            _ => return Err("Research loader expects DOS 1.00A or 1.00B layout".into()),
        };
        let assets = Self::parse(root.to_path_buf(), read_role, ovl, release, video)?;
        #[cfg(feature = "dev-tools")]
        let assets = {
            let mut assets = assets;
            assets.sources = entries
                .into_iter()
                .filter(|(name, _)| {
                    name == "OVL0.OVL"
                        || name == "CENTURIO.EXE"
                        || name.ends_with(".DIR")
                        || name.ends_with(".DAT")
                })
                .map(|(_, path)| path)
                .collect();
            assets
        };
        Ok(assets)
    }

    /// Load only catalog-selected roles; release identification belongs to the app.
    #[cfg(any(feature = "sdl", test))]
    pub fn load(
        files: &reforge_inputs::ResolvedFiles,
        root: PathBuf,
        release: DosRelease,
        video: VideoMode,
    ) -> Result<Self> {
        let ovl = files.read("OVL0.OVL").map_err(|e| e.to_string())?;
        Self::parse(
            root,
            |name| files.read(name).map_err(|e| e.to_string()),
            ovl,
            release,
            video,
        )
    }

    fn parse(
        root: PathBuf,
        read_role: impl Fn(&str) -> Result<Vec<u8>>,
        ovl: Vec<u8>,
        release: DosRelease,
        video: VideoMode,
    ) -> Result<Self> {
        if release == DosRelease::V100A && video == VideoMode::Ega {
            return Err("The 1.00A distribution does not contain EGA graphics".into());
        }
        // Runtime tables occupy the resident DOS data segment, including palette
        // maps through 0x6300. Content fingerprints are deliberately absent here.
        let mut data_segment = ovl
            .get(0xb400..)
            .filter(|ds| ds.len() >= 0x6300)
            .ok_or("Truncated resident data segment")?
            .to_vec();
        layout::normalize(&mut data_segment, release)?;
        if video == VideoMode::Ega {
            // EGA draws the stored nibble directly; VGA alone uses palette tags.
            for map in data_segment[0x60c0..0x6300].as_chunks_mut::<16>().0 {
                for (index, color) in map.iter_mut().enumerate() {
                    *color = index as u8;
                }
            }
        }

        let mut banks = BTreeMap::new();
        for name in [
            "CENT", "MINIT", "MMAP", "BATL", "MGLAD", "MRACE", "MNAVAL", "NAVL", "MNEGO",
        ] {
            let physical = if video == VideoMode::Ega && name.starts_with('M') {
                format!("E{}", &name[1..])
            } else {
                name.to_string()
            };
            let directory = read_role(&format!("{physical}.DIR"))?;
            let data = read_role(&format!("{physical}.DAT"))?;
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
            release,
            video,
            root,
            banks,
            #[cfg(feature = "dev-tools")]
            sources: Vec::new(),
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
        if video == VideoMode::Ega {
            // Keep logical drawing pens used by native scenes, resolving them
            // to EGA attributes. The two UI fills are mode-dependent DOS pens.
            for index in 16..256 {
                let pen = match index {
                    26 => 7,
                    116 => 3,
                    _ => index & 15,
                };
                assets.palette[index] = assets.palette[pen];
            }
        }
        // VGA's CMAP defines 250 DAC entries; the remaining VGA entries stay zero.
        Ok(assets)
    }

    pub fn resource(&self, bank: &str, name: &str) -> Result<Vec<u8>> {
        if self.video == VideoMode::Ega {
            if bank == "MINIT" && name == "WORLD.LZW" {
                return Ok(self.bitmap("MINIT", "WORLD.CBM")?.pixels);
            }
            if bank == "NAVL" && name == "GALLEYS.LZW" {
                return Ok(self.bitmap("MNAVAL", "GALLEYS.CBM")?.pixels);
            }
        }
        let ega_menu;
        let name = if self.video == VideoMode::Ega && bank == "CENT" && name.starts_with("MMENU") {
            ega_menu = format!("EMENU{}", &name[5..]);
            ega_menu.as_str()
        } else {
            name
        };
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
        if self.video == VideoMode::Vga
            && bitmap.width == payload[6] as usize * 2
            && bitmap.pixels.len() >= 2
        {
            let end = bitmap.pixels.len();
            let nibble = bitmap.pixels[end - 2] & 15;
            bitmap.pixels[end - 1] =
                self.data_segment[0x60c0 + payload[9] as usize * 16 + nibble as usize];
        }
        Ok(bitmap)
    }

    /// Visit each stored bitmap/sprite frame once, excluding packed row padding.
    /// Raw full-screen backgrounds have no transparency mask.
    #[cfg(feature = "sdl")]
    pub(crate) fn visit_stored_artwork(
        &self,
        mut include: impl FnMut(&[u8], Option<&[bool]>),
    ) -> Result<()> {
        for (bank, contents) in &self.banks {
            for (name, &(_, _, flags)) in &contents.entries {
                if flags == 0xc0 {
                    let bitmap = self.bitmap(bank, name)?;
                    include(&bitmap.pixels, Some(&bitmap.visible));
                }
                if let Some(stem) = name.strip_suffix("HDR.LZW") {
                    for bitmap in self.sprites(bank, stem)? {
                        include(&bitmap.pixels, Some(&bitmap.visible));
                    }
                }
            }
        }
        for (bank, name) in [("MINIT", "WORLD.LZW"), ("NAVL", "GALLEYS.LZW")] {
            let pixels = self.resource(bank, name)?;
            if pixels.len() != 320 * 200 {
                return Err(format!("Unexpected {bank}/{name} background size"));
            }
            include(&pixels, None);
        }
        Ok(())
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

    #[cfg(feature = "dev-tools")]
    pub(crate) fn recording_sources(&self) -> &[PathBuf] {
        &self.sources
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

    #[test]
    fn ega_uses_selected_banks_direct_pens_and_packed_backgrounds() {
        // Authored two-pixel artwork: the nonzero palette tag would produce
        // different pixels if EGA accidentally reused VGA's tag remapping.
        let cbm = [2, 0, 0, 0, 0, 0, 1, 1, 0, 7, 1, 0, 0, 0, 1, 0x12];
        let bank = |members: &[(&str, &[u8])]| {
            let mut data = vec![0xca, 0xca, 0xd0, 0xd0];
            let mut directory = Vec::new();
            for &(name, payload) in members {
                let mut entry = [0; 20];
                entry[..4].copy_from_slice(&(data.len() as u32).to_le_bytes());
                entry[4..6].copy_from_slice(&(payload.len() as u16).to_le_bytes());
                entry[6..6 + name.len()].copy_from_slice(name.as_bytes());
                directory.extend(entry);
                data.extend_from_slice(payload);
            }
            (data, directory)
        };
        let mut roles = BTreeMap::new();
        for name in [
            "CENT", "EINIT", "EMAP", "BATL", "EGLAD", "ERACE", "ENAVAL", "NAVL", "ENEGO",
        ] {
            let members: Vec<(&str, &[u8])> = match name {
                "CENT" => vec![("CMAP.BIN", &[0; 750]), ("EMENUHDR.LZW", b"EGA menu")],
                "EINIT" => vec![("WORLD.CBM", &cbm)],
                "EMAP" => vec![("MAP.CBM", &cbm)],
                "ENAVAL" => vec![("GALLEYS.CBM", &cbm)],
                _ => Vec::new(),
            };
            let (data, directory) = bank(&members);
            roles.insert(format!("{name}.DAT"), data);
            roles.insert(format!("{name}.DIR"), directory);
        }
        let mut ovl = vec![0; 0xb400 + 0xc410];
        ovl[0xb400 + 0x60c0..0xb400 + 0x6300].fill(99);
        for at in (0x970a..0x9714).step_by(2) {
            ovl[0xb400 + at..0xb400 + at + 2].copy_from_slice(&0xc3b6_u16.to_le_bytes());
        }
        let load = |roles: &BTreeMap<String, Vec<u8>>| {
            Assets::parse(
                PathBuf::new(),
                |role| {
                    roles
                        .get(role)
                        .cloned()
                        .ok_or_else(|| format!("Missing selected {role}"))
                },
                ovl.clone(),
                DosRelease::V100B,
                VideoMode::Ega,
            )
        };
        let assets = load(&roles).unwrap();
        assert_eq!(assets.resource("MINIT", "WORLD.LZW").unwrap(), [1, 2]);
        assert_eq!(assets.resource("NAVL", "GALLEYS.LZW").unwrap(), [1, 2]);
        assert_eq!(
            assets.resource("CENT", "MMENUHDR.LZW").unwrap(),
            b"EGA menu"
        );
        assert_eq!(
            assets.cached_bitmap("MMAP", "MAP.CBM").unwrap().pixels,
            [1, 2]
        );
        // An unselected VGA substitute must never mask a missing EGA role.
        let removed = roles.remove("EMAP.DAT").unwrap();
        roles.insert("MMAP.DAT".into(), removed);
        assert!(
            load(&roles)
                .err()
                .unwrap()
                .contains("Missing selected EMAP.DAT")
        );
    }

    #[test]
    fn selected_roles_load_renamed_resources_without_executable_or_fingerprints() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "centurion-inputs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let _cleanup = Cleanup(directory.clone());
        let mut selected = Vec::new();
        let mut add = |role: &str, bytes: &[u8]| {
            let path = directory.join(format!("selected-{}", selected.len()));
            fs::write(&path, bytes).unwrap();
            selected.push((role.to_owned(), path));
        };
        // Deliberately absent version label: this is authored format data, not
        // an original release fingerprint or a runtime identity requirement.
        add("OVL0.OVL", &vec![0; 0xb400 + 0xc410]);
        for bank in [
            "CENT", "MINIT", "MMAP", "BATL", "MGLAD", "MRACE", "MNAVAL", "NAVL", "MNEGO",
        ] {
            let mut data = vec![0xca, 0xca, 0xd0, 0xd0];
            let mut index = vec![];
            if bank == "CENT" {
                data.extend([0; 750]);
                index.resize(20, 0);
                index[..4].copy_from_slice(&4u32.to_le_bytes());
                index[4..6].copy_from_slice(&750u16.to_le_bytes());
                index[6..14].copy_from_slice(b"CMAP.BIN");
            }
            add(&format!("{bank}.DAT"), &data);
            add(&format!("{bank}.DIR"), &index);
        }
        assert_eq!(selected.len(), 19);
        let files = reforge_inputs::ResolvedFiles::new(selected.clone());
        let loaded =
            Assets::load(&files, directory.clone(), DosRelease::V100A, VideoMode::Vga).unwrap();
        assert_eq!(loaded.palette, [[0; 3]; 256]);
        selected.retain(|(role, _)| role != "MINIT.DAT");
        fs::write(directory.join("MINIT.DAT"), [0xca, 0xca, 0xd0, 0xd0]).unwrap();
        let error = Assets::load(
            &reforge_inputs::ResolvedFiles::new(selected),
            directory,
            DosRelease::V100A,
            VideoMode::Vga,
        )
        .err()
        .unwrap();
        assert!(error.contains("Missing resolved original resource MINIT.DAT"));
    }
}
