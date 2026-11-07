// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Translate the restored full-build data layout to the engine's established
//! DOS-conversion offsets. These are format locations, never asset payloads.
//! Keep the complete external segment separately for its restored PSG score.

use super::{set_word, word};

pub(super) const DATA_LENGTH: usize = 0x73f0;

// (legacy start, legacy end, restored start). The conversion removed the leading
// byte, PSG state/score and several small fields; its menu pointer table was
// synthesized separately. Unmapped legacy storage is initially zero.
const REGIONS: &[(usize, usize, usize)] = &[
    (0x0000, 0x0432, 0x0001),
    (0x0433, 0x06b7, 0x0433),
    (0x06b7, 0x0fe2, 0x06d7),
    (0x0fe2, 0x1664, 0x1252),
    (0x1664, 0x1827, 0x18d6),
    (0x1829, 0x37af, 0x1a99),
    (0x37af, 0x6d37, 0x3a2f),
    (0x6d63, 0x6d8d, 0x7028),
    (0x6d8d, 0x6df8, 0x7056),
    (0x6e01, 0x7128, 0x70c1),
];

// Sprite pointers and drawing-list source fields, in legacy coordinates.
// Stride four skips the accompanying dimensions/screen-position word. Listing
// the known fields avoids interpreting arbitrary sprite pixels as addresses.
const POINTERS: &[(usize, usize, usize)] = &[
    (0x09a6, 0x09c0, 2),
    (0x0f7a, 0x0fb6, 2),
    (0x0fc2, 0x0fd2, 2),
    (0x15c8, 0x15d8, 2),
    (0x17c9, 0x17d1, 2),
    (0x1c26, 0x1c2e, 2),
    (0x1f59, 0x1f65, 2),
    (0x218c, 0x218e, 2),
    (0x2194, 0x2196, 2),
    (0x219c, 0x219e, 2),
    (0x21a4, 0x21aa, 4),
    (0x2272, 0x2274, 2),
    (0x227a, 0x2294, 4),
    (0x229a, 0x229c, 2),
    (0x22a2, 0x22a4, 2),
    (0x22aa, 0x22dc, 4),
    (0x2346, 0x2380, 4),
    (0x2386, 0x2388, 2),
    (0x238e, 0x23a4, 4),
    (0x23f0, 0x23f2, 2),
    (0x23f8, 0x2446, 4),
    (0x24b4, 0x24b6, 2),
    (0x24bc, 0x24fa, 4),
    (0x251e, 0x253c, 4),
    (0x2572, 0x25e0, 4),
    (0x2626, 0x2630, 4),
    (0x2636, 0x2650, 2),
    (0x28a2, 0x2900, 4),
    (0x2ad1, 0x2ad9, 2),
    (0x2e20, 0x2e24, 2),
    (0x2e26, 0x2e84, 4),
    (0x3260, 0x3278, 2),
    (0x35d0, 0x35d8, 2),
    (0x36cc, 0x36dc, 2),
    (0x3c24, 0x3c3a, 4),
    (0x3c40, 0x3c52, 4),
    (0x3c58, 0x3c96, 4),
    (0x3c9c, 0x3ca6, 4),
    (0x3d06, 0x3d16, 2),
    (0x3de0, 0x3de4, 2),
    (0x3ec2, 0x3eca, 2),
    (0x40c0, 0x40c8, 2),
    (0x4429, 0x4441, 2),
    (0x44a5, 0x44bd, 2),
    (0x45a8, 0x45b6, 2),
    (0x4a60, 0x4a80, 2),
    (0x4d92, 0x4d98, 2),
    (0x5012, 0x5016, 2),
    (0x52ae, 0x52c0, 2),
    (0x584c, 0x5858, 2),
    (0x5908, 0x5918, 2),
    (0x5f62, 0x5f66, 2),
    (0x6a8f, 0x6a93, 2),
];

// The full build addresses these strings from code. The existing renderer uses
// the DOS conversion's data pointer table; text itself remains externally read.
const MENU_TEXT: &[u16] = &[
    0x6aa0, 0x6ac5, 0x6ae5, 0x6af1, 0x6b00, 0x6b0c, 0x6b1b, 0x6b28, 0x6b4f, 0x6b6d, 0x6b92, 0x6c09,
    0x6c6b, 0x6c8b, 0x6bbb, 0x6be3, 0x6c21, 0x6c46, 0x6caf, 0x6cd2, 0x6cf7, 0x6d1a,
];

fn legacy_pointer(pointer: u16) -> Result<u16, String> {
    let pointer = usize::from(pointer);
    REGIONS
        .iter()
        .find_map(|&(start, end, source)| {
            (source..source + end - start)
                .contains(&pointer)
                .then(|| (pointer - source + start) as u16)
        })
        .ok_or_else(|| format!("Restored asset pointer {pointer:#06x} is outside mapped data"))
}

pub(super) fn normalize(restored: &[u8]) -> Result<Vec<u8>, String> {
    if restored.len() != DATA_LENGTH {
        return Err("Restored game-data segment has an invalid length".into());
    }
    let mut data = vec![0; super::DATA_LENGTH];
    for &(start, end, source) in REGIONS {
        data[start..end].copy_from_slice(&restored[source..source + end - start]);
    }
    for &(start, end, stride) in POINTERS {
        for offset in (start..end).step_by(stride) {
            let pointer = legacy_pointer(word(&data, offset))?;
            set_word(&mut data, offset, pointer);
        }
    }

    // The full build places keyboard and PSG sequencer defaults here. They are
    // mutable zero-initialized storage in the established runtime layout.
    data[0x06bf..0x06c7].fill(0);
    data[0x0fe8..0x0fee].fill(0);

    // Repack the short legacy effect/count table from the external full table.
    // Do not copy the full build's extra tail fields over adjacent legacy data.
    let count = word(&data, 0x1824);
    set_word(&mut data, 0x1828, count);
    set_word(&mut data, 0x182a, word(restored, 0x1a9e));
    set_word(&mut data, 0x182c, count);
    set_word(&mut data, 0x182e, word(restored, 0x1aa0));

    // Completion drawing uses the conversion's one-row-earlier Y convention.
    // Preserve that coordinate contract rather than changing gameplay/rendering
    // semantics simply because the restored executable's layout was selected.
    for offset in (0x361e..0x362e).step_by(4) {
        let y = word(&data, offset)
            .checked_sub(1)
            .ok_or("Restored completion Y coordinate is outside its format")?;
        set_word(&mut data, offset, y);
    }
    for (index, &pointer) in MENU_TEXT.iter().enumerate() {
        set_word(&mut data, 0x6d37 + index * 2, pointer);
    }
    // Keep the conversion's final menu line on the same row as the existing
    // menu position with that row; the full build's last line uses another row.
    let last_position = word(&data, 0x6d6b);
    set_word(&mut data, 0x6d8d, last_position);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restored_fixture() -> Vec<u8> {
        let mut restored = vec![0; DATA_LENGTH];
        for &(start, end, stride) in POINTERS {
            for offset in (start..end).step_by(stride) {
                let &(region, _, source) = REGIONS
                    .iter()
                    .find(|&&(a, b, _)| (a..b).contains(&offset))
                    .unwrap();
                set_word(&mut restored, offset - region + source, 0x06d7);
            }
        }
        for offset in (0x361e..0x362e).step_by(4) {
            set_word(&mut restored, offset + 0x270, 10);
        }
        restored
    }

    #[test]
    fn translates_only_documented_pointer_fields_and_preserves_asset_bytes() {
        let mut restored = restored_fixture();
        restored[0x06d7] = 0xa7;
        set_word(&mut restored, 0x1252 + 0x100, 0x06d7);
        let legacy = normalize(&restored).unwrap();
        assert_eq!(legacy[0x06b7], 0xa7);
        assert_eq!(word(&legacy, 0x0f7a), 0x06b7);
        assert_eq!(word(&legacy, 0x10e2), 0x06d7);
        assert_eq!(word(&legacy, 0x361e), 9);
        assert_eq!(word(&legacy, 0x6d37), MENU_TEXT[0]);
        assert_eq!(word(&legacy, 0x6d8d), word(&legacy, 0x6d6b));
    }

    #[test]
    fn rejects_truncated_segments_invalid_pointers_and_bad_coordinates() {
        assert!(normalize(&[0; 1]).is_err());
        let mut restored = restored_fixture();
        set_word(&mut restored, 0x0f7a + 0x20, 0xffff);
        assert!(normalize(&restored).is_err());
        let mut restored = restored_fixture();
        set_word(&mut restored, 0x361e + 0x270, 0);
        assert!(normalize(&restored).is_err());
    }
}
