// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read CAT.EXE from the detector-selected MBR/FAT16 container, without booting it.
//! This is a bounded root-file reader, not a release detector or a disk emulator.

const SECTOR: usize = 512;
const INVALID: &str = "Invalid or truncated DOS disk image";

fn span(data: &[u8], offset: usize, length: usize) -> Result<&[u8], &'static str> {
    data.get(offset..offset.checked_add(length).ok_or(INVALID)?)
        .ok_or(INVALID)
}
fn word(data: &[u8], offset: usize) -> Result<usize, &'static str> {
    let bytes = span(data, offset, 2)?;
    Ok(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
}
fn dword(data: &[u8], offset: usize) -> Result<usize, &'static str> {
    let bytes: [u8; 4] = span(data, offset, 4)?.try_into().map_err(|_| INVALID)?;
    usize::try_from(u32::from_le_bytes(bytes)).map_err(|_| INVALID)
}
fn sectors(count: usize) -> Result<usize, &'static str> {
    count.checked_mul(SECTOR).ok_or(INVALID)
}

pub(super) fn cat(image: &[u8]) -> Result<Vec<u8>, &'static str> {
    if span(image, 510, 2)? != [0x55, 0xaa] || span(image, 446 + 4, 1)? != [0x06] {
        return Err("DOS disk image requires its MBR FAT16 partition");
    }
    let partition = span(
        image,
        sectors(dword(image, 454)?)?,
        sectors(dword(image, 458)?)?,
    )?;
    if span(partition, 510, 2)? != [0x55, 0xaa] || word(partition, 11)? != SECTOR {
        return Err("DOS disk image requires 512-byte FAT16 sectors");
    }
    let cluster_sectors = usize::from(span(partition, 13, 1)?[0]);
    let reserved = word(partition, 14)?;
    let fats = usize::from(span(partition, 16, 1)?[0]);
    let entries = word(partition, 17)?;
    let fat_sectors = word(partition, 22)?;
    if !cluster_sectors.is_power_of_two()
        || cluster_sectors > 128
        || reserved == 0
        || !(1..=2).contains(&fats)
        || entries == 0
        || fat_sectors == 0
    {
        return Err("Invalid FAT16 geometry in DOS disk image");
    }
    let total = match word(partition, 19)? {
        0 => dword(partition, 32)?,
        total => total,
    };
    let volume = span(partition, 0, sectors(total)?)?;
    let fat = span(volume, sectors(reserved)?, sectors(fat_sectors)?)?;
    // These u16 geometry fields and the bounded FAT count fit usize on 32-bit
    // hosts; the total volume and every actual read are checked separately.
    let root_sector = reserved + fats * fat_sectors;
    let root = span(volume, sectors(root_sector)?, entries * 32)?;
    let data_sector = root_sector + (entries * 32).div_ceil(SECTOR);
    let data = volume.get(sectors(data_sector)?..).ok_or(INVALID)?;
    let cluster_size = sectors(cluster_sectors)?;
    let cluster_count = data.len() / cluster_size;
    if !(4085..65525).contains(&cluster_count) || fat.len() / 2 < cluster_count + 2 {
        return Err("DOS disk image does not contain a bounded FAT16 volume");
    }
    let mut selected = None;
    for entry in root.as_chunks::<32>().0 {
        if entry[0] == 0 {
            break;
        }
        if entry[0] != 0xe5 && entry[11] & 0x18 == 0 && &entry[..11] == b"CAT     EXE" {
            if selected.is_some() {
                return Err("DOS disk image has multiple CAT.EXE directory entries");
            }
            selected = Some((word(entry, 26)?, dword(entry, 28)?));
        }
    }
    let (mut cluster, length) = selected.ok_or("DOS disk image has no root CAT.EXE")?;
    if length == 0 || length > data.len() {
        return Err("Invalid CAT.EXE length in DOS disk image");
    }
    let mut visited = vec![false; cluster_count + 2];
    let mut executable = Vec::with_capacity(length);
    loop {
        if !(2..cluster_count + 2).contains(&cluster) || cluster >= 0xfff0 || visited[cluster] {
            return Err("Invalid or cyclic CAT.EXE cluster chain in DOS disk image");
        }
        visited[cluster] = true;
        let block = span(data, (cluster - 2) * cluster_size, cluster_size)?;
        executable.extend_from_slice(&block[..block.len().min(length - executable.len())]);
        cluster = word(fat, cluster * 2)?;
        if executable.len() == length {
            return if cluster >= 0xfff8 {
                Ok(executable)
            } else {
                Err("CAT.EXE cluster chain exceeds its file length")
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set16(bytes: &mut [u8], offset: usize, value: usize) {
        bytes[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    fn set32(bytes: &mut [u8], offset: usize, value: usize) {
        bytes[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    fn fixture() -> (Vec<u8>, Vec<u8>) {
        let mut image = vec![0; 3 * 1024 * 1024];
        image[510..512].copy_from_slice(&[0x55, 0xaa]);
        image[450] = 6;
        set32(&mut image, 454, 8);
        set32(&mut image, 458, 6136);
        let volume = &mut image[4096..];
        volume[510..512].copy_from_slice(&[0x55, 0xaa]);
        set16(volume, 11, 512);
        volume[13] = 1;
        set16(volume, 14, 1);
        volume[16] = 2;
        set16(volume, 17, 512);
        set16(volume, 19, 6136);
        set16(volume, 22, 24);
        let payload: Vec<_> = (0..1200).map(|i| (i % 251) as u8).collect();
        let root = &mut volume[49 * SECTOR..];
        root[..11].copy_from_slice(b"CAT     EXE");
        set16(root, 26, 2);
        set32(root, 28, payload.len());
        for (index, cluster) in [2, 5, 3].into_iter().enumerate() {
            let next = [5, 3, 0xffff][index];
            set16(volume, SECTOR + cluster * 2, next);
            let start = (81 + cluster - 2) * SECTOR;
            let part = &payload[index * SECTOR..payload.len().min((index + 1) * SECTOR)];
            volume[start..start + part.len()].copy_from_slice(part);
        }
        (image, payload)
    }

    #[test]
    fn follows_fragmented_chain_and_ignores_cluster_padding() {
        let (image, payload) = fixture();
        assert_eq!(cat(&image).unwrap(), payload);
    }

    #[test]
    fn resolved_image_uses_the_conversion_layout_without_restored_music() {
        use crate::assets::{Assets, DATA_LENGTH, InputFormat};
        let (mut image, _) = fixture();
        let mut executable = vec![0; 0x300 + DATA_LENGTH];
        executable[..2].copy_from_slice(b"MZ");
        set16(&mut executable, 8, 0x20);
        executable[0x300..].fill(0xa5);
        let clusters = executable.len().div_ceil(SECTOR);
        set32(&mut image, 0x7200 + 28, executable.len());
        for index in 0..clusters {
            let cluster = index + 2;
            set16(
                &mut image,
                4096 + SECTOR + cluster * 2,
                if index + 1 == clusters {
                    0xffff
                } else {
                    cluster + 1
                },
            );
            let part = &executable[index * SECTOR..executable.len().min((index + 1) * SECTOR)];
            let start = 0xb200 + index * SECTOR;
            image[start..start + part.len()].copy_from_slice(part);
        }
        let path =
            std::env::temp_dir().join(format!("reforge-dos-image-{}.material", std::process::id()));
        std::fs::write(&path, &image).unwrap();
        let files = reforge_inputs::ResolvedFiles::new([("alley_cat.img", path.clone())]);
        let result = Assets::from_files(&files, InputFormat::DosDiskImage);
        std::fs::remove_file(path).unwrap();
        let assets = result.unwrap();
        assert_eq!(assets.input_format(), InputFormat::DosDiskImage);
        assert_eq!(
            assets.data(),
            Assets::from_executable(&executable).unwrap().data()
        );
        assert!(assets.restored_data().is_none());
        assert!(
            assets
                .title_music(crate::alley::Machine::Pcjr)
                .unwrap()
                .is_none()
        );
        assert!(Assets::from_files(&Default::default(), InputFormat::DosDiskImage).is_err());
    }

    #[test]
    fn rejects_truncated_images_invalid_geometry_and_missing_files() {
        let (image, _) = fixture();
        for length in [0, 511, 4096, image.len() - 1] {
            assert!(cat(&image[..length]).is_err());
        }
        for (offset, value) in [
            (510, 0),
            (450, 0),
            (4096 + 13, 0),
            (4096 + 12, 0),
            (4096 + 510, 0),
            (0x7200, 0),
        ] {
            let mut broken = image.clone();
            broken[offset] = value;
            assert!(cat(&broken).is_err(), "offset {offset:#x}");
        }
        let mut huge = image;
        set32(&mut huge, 458, u32::MAX as usize);
        assert!(cat(&huge).is_err());
    }

    #[test]
    fn rejects_bad_cyclic_short_and_overlong_chains_and_duplicate_files() {
        let (image, _) = fixture();
        for next in [0, 1, 2, 0xfff7, 0xffff] {
            let mut broken = image.clone();
            set16(&mut broken, 4096 + SECTOR + 2 * 2, next);
            assert!(cat(&broken).is_err());
        }
        let mut overlong = image.clone();
        set16(&mut overlong, 4096 + SECTOR + 3 * 2, 4);
        assert!(cat(&overlong).is_err());
        let mut duplicate = image.clone();
        duplicate.copy_within(0x7200..0x7220, 0x7220);
        assert!(cat(&duplicate).unwrap_err().contains("multiple"));
    }
}
