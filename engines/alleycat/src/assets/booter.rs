// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Load the single-sided raw disk layout selected by application detection.
//! These geometry and transformation checks do not identify a release.

const SECTOR_LENGTH: usize = 512;
const SECTORS_PER_CYLINDER: usize = 9;
const IMAGE_LENGTH: usize = 40 * SECTORS_PER_CYLINDER * SECTOR_LENGTH;
const LOAD_LENGTH: usize = 0x10000;
pub(super) const DATA_OFFSET: usize = 0x6740;

fn extract(image: &[u8]) -> Result<Vec<u8>, String> {
    if image.len() != IMAGE_LENGTH {
        return Err("PC Booter assets require a 184320-byte raw disk image".into());
    }
    let mut loaded = Vec::with_capacity(LOAD_LENGTH);
    // The loader reads head zero, cylinders 2..17, sectors 1..8. Sector nine
    // separates each 4-KiB group on disk and is never part of the loaded image.
    for cylinder in 2..=17 {
        let start = cylinder * SECTORS_PER_CYLINDER * SECTOR_LENGTH;
        loaded.extend_from_slice(&image[start..start + 8 * SECTOR_LENGTH]);
    }
    Ok(loaded)
}

fn transform(loaded: &mut [u8]) {
    let mut state = 0xfa59;
    // CX=3fff, DI+=4: the final four-byte group is left untouched. Keep the
    // complete 64 KiB until this pass finishes, including the residual tail.
    for offset in (0..0xfffc).step_by(4) {
        state = super::random_successor(state);
        loaded[offset] ^= state as u8;
    }
}

pub(super) fn decode(image: &[u8]) -> Result<Vec<u8>, String> {
    let mut loaded = extract(image)?;
    transform(&mut loaded);
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, InputFormat, set_word};

    fn synthetic_data() -> Vec<u8> {
        // 0101 is a valid full-build pointer and a valid nonzero completion Y
        // coordinate, so the normalizer can check every field without assets.
        let mut data = vec![1; super::super::restored::DATA_LENGTH];
        for channel in 0..3 {
            let state = 0x1231 + channel * 9;
            set_word(&mut data, 0x124c + channel * 2, state as u16);
            data[state + 4] = 0x80 + channel as u8 * 0x20;
            let notes = 0x1100 + channel * 8;
            let durations = 0x1140 + channel * 8;
            set_word(&mut data, state + 5, notes as u16);
            set_word(&mut data, state + 7, durations as u16);
            data[notes..notes + 3].copy_from_slice(&[2, 0, 0x86]);
            data[durations..durations + 2].copy_from_slice(&[3, 1]);
        }
        for (index, limit) in [0, 2, 4, 4].into_iter().enumerate() {
            set_word(&mut data, 0x1256 + index * 2, limit);
        }
        data
    }

    fn synthetic_image(data: &[u8]) -> Vec<u8> {
        let mut loaded = vec![0xa5; LOAD_LENGTH];
        loaded[DATA_OFFSET..DATA_OFFSET + data.len()].copy_from_slice(data);
        transform(&mut loaded);
        let mut image = vec![0xe3; IMAGE_LENGTH];
        for (index, group) in loaded
            .as_chunks::<{ 8 * SECTOR_LENGTH }>()
            .0
            .iter()
            .enumerate()
        {
            let start = (index + 2) * SECTORS_PER_CYLINDER * SECTOR_LENGTH;
            image[start..start + group.len()].copy_from_slice(group);
        }
        image
    }

    #[test]
    fn extracts_all_loaded_sectors_and_skips_each_ninth_sector() {
        let mut image = vec![0; IMAGE_LENGTH];
        for (index, sector) in image
            .as_chunks_mut::<SECTOR_LENGTH>()
            .0
            .iter_mut()
            .enumerate()
        {
            sector.fill(index as u8);
        }
        let loaded = extract(&image).unwrap();
        assert_eq!(loaded.len(), LOAD_LENGTH);
        for (index, sector) in loaded.as_chunks::<SECTOR_LENGTH>().0.iter().enumerate() {
            let cylinder = 2 + index / 8;
            let disk_sector = cylinder * 9 + index % 8;
            assert!(sector.iter().all(|&byte| byte == disk_sector as u8));
        }
    }

    #[test]
    fn rejects_short_and_incompatible_raw_image_lengths() {
        for length in [0, 1, 512, 0x10000, IMAGE_LENGTH - 1, IMAGE_LENGTH + 1] {
            assert!(decode(&vec![0; length]).is_err());
        }
        // A complete image still needs valid game-data structures at the next
        // parser layer; no embedded boot signature or fingerprint is required.
        assert_eq!(decode(&vec![0; IMAGE_LENGTH]).unwrap().len(), LOAD_LENGTH);
    }

    #[test]
    fn feedback_known_answer_preserves_stride_and_last_four_bytes() {
        let mut loaded = vec![0xa5; LOAD_LENGTH];
        transform(&mut loaded);
        for (index, mask) in [0x2c, 0x96, 0x4b, 0xa5, 0xd2, 0xe9, 0xf4, 0xfa]
            .into_iter()
            .enumerate()
        {
            assert_eq!(loaded[index * 4], 0xa5 ^ mask);
        }
        assert_eq!(loaded[0xfff8], 0xa5 ^ 0x37);
        for (offset, &byte) in loaded.iter().enumerate() {
            if offset % 4 != 0 || offset >= 0xfffc {
                assert_eq!(byte, 0xa5, "unexpected change at {offset:#06x}");
            }
        }
        transform(&mut loaded);
        assert_eq!(loaded, vec![0xa5; LOAD_LENGTH]);
    }

    #[test]
    fn disk_data_reuses_full_build_normalization_and_title_music() {
        let data = synthetic_data();
        let image = synthetic_image(&data);
        let assets = Assets::from_booter_image(&image).unwrap();
        assert_eq!(assets.input_format(), InputFormat::PcBooter);
        assert_eq!(assets.restored_data(), Some(data.as_slice()));

        let mut executable = vec![0; 0x6940 + data.len()];
        executable[..2].copy_from_slice(b"MZ");
        executable[8..10].copy_from_slice(&0x20u16.to_le_bytes());
        executable[0x6940..].copy_from_slice(&data);
        let restored = Assets::from_restored_executable(&executable).unwrap();
        assert_eq!(assets.data(), restored.data());
        assert_eq!(assets.data()[0], 1);
        assert_eq!(crate::assets::word(assets.data(), 0x0f7a), 0x0100);
        assert_eq!(crate::assets::word(assets.data(), 0x361e), 0x0100);
        assert!(
            assets
                .title_music(crate::alley::Machine::Cga)
                .unwrap()
                .is_none()
        );
        assert!(
            assets
                .title_music(crate::alley::Machine::Pcjr)
                .unwrap()
                .is_some()
        );

        let path = std::env::temp_dir().join(format!(
            "reforge-booter-{}-{}.raw",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &image).unwrap();
        let files = reforge_inputs::ResolvedFiles::new([("disk1.img", path.clone())]);
        let resolved = Assets::from_files(&files, InputFormat::PcBooter);
        std::fs::remove_file(path).unwrap();
        assert_eq!(resolved.unwrap().data(), assets.data());
        assert!(Assets::from_files(&Default::default(), InputFormat::PcBooter).is_err());
    }

    #[test]
    fn full_build_constructor_rejects_malformed_data_after_disk_decode() {
        assert!(Assets::from_full_data(&[0; 1], InputFormat::PcBooter).is_err());
        let mut data = synthetic_data();
        set_word(&mut data, 0x0f7a + 0x20, 0xffff);
        assert!(Assets::from_booter_image(&synthetic_image(&data)).is_err());
        let mut data = synthetic_data();
        set_word(&mut data, 0x361e + 0x270, 0);
        assert!(Assets::from_booter_image(&synthetic_image(&data)).is_err());
    }
}
