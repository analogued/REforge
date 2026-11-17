// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Adapt release-owned initialization data to the native runtime's DS layout.
use super::{DosRelease, Result};

pub(super) fn normalize(data: &mut [u8], release: DosRelease) -> Result<()> {
    if release == DosRelease::V100A {
        return Ok(());
    }
    if data.len() < 0x9714 {
        return Err("Truncated 1.00B naval data".into());
    }
    // 1.00B moves the re-engagement question before the outcome strings.
    // Retain supplied text, version/credits, save blocks and campaign balance.
    let question = data[0x8eee..0x8f3a].to_vec();
    let outcomes = data[0x8f3a..0x9396].to_vec();
    let parts = data[0x9396..0x93a6].to_vec();
    data[0x8eee..0x934a].copy_from_slice(&outcomes);
    data[0x934a..0x935a].copy_from_slice(&parts);
    data[0x935a..0x93a6].copy_from_slice(&question);
    // Footer entries reference five words in the moved naval scratch block.
    // The native simulation owns its transient layout; saves are unchanged.
    for at in (0x970a..0x9714).step_by(2) {
        let pointer = u16::from_le_bytes([data[at], data[at + 1]])
            .checked_sub(6)
            .ok_or("Invalid 1.00B naval footer pointer")?;
        data[at..at + 2].copy_from_slice(&pointer.to_le_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_text_moves_without_touching_campaign_version_or_font_data() {
        let mut data = vec![0x55; 0xc410];
        data[0x8eee..0x8f3a].fill(b'?');
        data[0x8f3a..0x9396].fill(b'!');
        data[0x9396..0x93a6].fill(b':');
        for at in (0x970a..0x9714).step_by(2) {
            data[at..at + 2].copy_from_slice(&0xc3b6_u16.to_le_bytes());
        }
        let before = data.clone();
        normalize(&mut data, DosRelease::V100B).unwrap();
        assert!(data[0x8eee..0x934a].iter().all(|&v| v == b'!'));
        assert!(data[0x934a..0x935a].iter().all(|&v| v == b':'));
        assert!(data[0x935a..0x93a6].iter().all(|&v| v == b'?'));
        for at in (0x970a..0x9714).step_by(2) {
            assert_eq!(u16::from_le_bytes([data[at], data[at + 1]]), 0xc3b0);
        }
        assert_eq!(&data[..0x8eee], &before[..0x8eee]);
        assert_eq!(&data[0x93a6..0x970a], &before[0x93a6..0x970a]);
        assert_eq!(&data[0x9714..], &before[0x9714..]);
        let mut reference = before.clone();
        normalize(&mut reference, DosRelease::V100A).unwrap();
        assert_eq!(reference, before);
    }
}
