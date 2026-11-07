// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded BSDIFF40 application for an externally supplied restoration patch.
//! This decoder contains no patch, executable or original game bytes.

use bzip2::read::BzDecoder;
use std::io::Read;

// Bound expanded streams before allocating or following external control tuples,
// including tuples that only move the input position without producing output.
const MAX_BYTES: usize = 2 * 1024 * 1024;

fn number(bytes: &[u8]) -> Result<i64, String> {
    let bytes: [u8; 8] = bytes.try_into().map_err(|_| "Truncated BSDIFF40 integer")?;
    let mut magnitude = bytes;
    magnitude[7] &= 0x7f;
    let value = i64::from_le_bytes(magnitude);
    Ok(if bytes[7] & 0x80 != 0 { -value } else { value })
}

fn length(bytes: &[u8]) -> Result<usize, String> {
    usize::try_from(number(bytes)?).map_err(|_| "Negative BSDIFF40 length".into())
}

fn expand(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    BzDecoder::new(bytes)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|error| format!("Cannot decompress BSDIFF40 stream: {error}"))?;
    if output.len() > MAX_BYTES {
        return Err("BSDIFF40 stream exceeds restoration size limit".into());
    }
    Ok(output)
}

pub(super) fn apply(old: &[u8], patch: &[u8]) -> Result<Vec<u8>, String> {
    let header = patch.get(..32).ok_or("Truncated BSDIFF40 header")?;
    if &header[..8] != b"BSDIFF40" {
        return Err("Restoration patch is not BSDIFF40".into());
    }
    let control_len = length(&header[8..16])?;
    let diff_len = length(&header[16..24])?;
    let output_len = length(&header[24..32])?;
    if output_len > MAX_BYTES {
        return Err("BSDIFF40 output exceeds restoration size limit".into());
    }
    let control_end = 32usize
        .checked_add(control_len)
        .ok_or("BSDIFF40 control length overflows")?;
    let diff_end = control_end
        .checked_add(diff_len)
        .ok_or("BSDIFF40 difference length overflows")?;
    let control = expand(
        patch
            .get(32..control_end)
            .ok_or("Truncated BSDIFF40 control stream")?,
    )?;
    let diff = expand(
        patch
            .get(control_end..diff_end)
            .ok_or("Truncated BSDIFF40 difference stream")?,
    )?;
    let extra = expand(
        patch
            .get(diff_end..)
            .ok_or("Truncated BSDIFF40 extra stream")?,
    )?;
    if control.len() % 24 != 0 {
        return Err("Truncated BSDIFF40 control tuple".into());
    }
    let mut output = Vec::with_capacity(output_len);
    let mut old_position = 0i64;
    let mut diff_position = 0usize;
    let mut extra_position = 0usize;
    let mut tuples = control.as_chunks::<24>().0.iter();
    while output.len() < output_len {
        let tuple = tuples.next().ok_or("BSDIFF40 output is incomplete")?;
        let add = length(&tuple[..8])?;
        let copy = length(&tuple[8..16])?;
        let seek = number(&tuple[16..24])?;
        if add > output_len - output.len() || copy > output_len - output.len() - add {
            return Err("BSDIFF40 control exceeds output bounds".into());
        }
        let diff_end = diff_position
            .checked_add(add)
            .ok_or("BSDIFF40 difference position overflows")?;
        let deltas = diff
            .get(diff_position..diff_end)
            .ok_or("Truncated BSDIFF40 difference data")?;
        for (index, &delta) in deltas.iter().enumerate() {
            let position = old_position
                .checked_add(index as i64)
                .ok_or("BSDIFF40 input position overflows")?;
            let original = usize::try_from(position)
                .ok()
                .and_then(|position| old.get(position))
                .copied()
                .unwrap_or(0);
            output.push(original.wrapping_add(delta));
        }
        diff_position = diff_end;
        let extra_end = extra_position
            .checked_add(copy)
            .ok_or("BSDIFF40 extra position overflows")?;
        output.extend_from_slice(
            extra
                .get(extra_position..extra_end)
                .ok_or("Truncated BSDIFF40 extra data")?,
        );
        extra_position = extra_end;
        old_position = old_position
            .checked_add(add as i64)
            .and_then(|position| position.checked_add(seek))
            .ok_or("BSDIFF40 input seek overflows")?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bzip2::{Compression, write::BzEncoder};
    use std::io::Write;

    fn encode_number(value: i64) -> [u8; 8] {
        let mut bytes = value.unsigned_abs().to_le_bytes();
        if value < 0 {
            bytes[7] |= 0x80;
        }
        bytes
    }

    fn compressed(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = BzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn fixture(tuples: &[[i64; 3]], diff: &[u8], extra: &[u8], size: i64) -> Vec<u8> {
        let controls: Vec<_> = tuples
            .iter()
            .flatten()
            .flat_map(|&number| encode_number(number))
            .collect();
        let controls = compressed(&controls);
        let diff = compressed(diff);
        let mut patch = b"BSDIFF40".to_vec();
        patch.extend_from_slice(&encode_number(controls.len() as i64));
        patch.extend_from_slice(&encode_number(diff.len() as i64));
        patch.extend_from_slice(&encode_number(size));
        patch.extend_from_slice(&controls);
        patch.extend_from_slice(&diff);
        patch.extend_from_slice(&compressed(extra));
        patch
    }

    #[test]
    fn applies_differences_extras_negative_seeks_and_out_of_range_input() {
        let patch = fixture(
            &[[2, 1, -3], [2, 0, 0], [0, 0, 10], [1, 0, 0]],
            &[255, 2, 7, 8, 9],
            &[99],
            6,
        );
        assert_eq!(apply(&[1, 2], &patch).unwrap(), [0, 4, 99, 7, 9, 9]);
    }

    #[test]
    fn rejects_bad_headers_bounds_streams_and_control_lengths() {
        assert!(apply(&[], b"invalid").is_err());
        assert!(apply(&[], &fixture(&[[1, 0, 0]], &[1], &[], 0x20_0001)).is_err());
        assert!(apply(&[], &fixture(&[[-1, 0, 0]], &[], &[], 1)).is_err());
        assert!(apply(&[], &fixture(&[[2, 0, 0]], &[1, 2], &[], 1)).is_err());
        assert!(apply(&[], &fixture(&[[1, 0, 0]], &[], &[], 1)).is_err());
        assert!(apply(&[], &fixture(&[[0, 1, 0]], &[], &[], 1)).is_err());
        assert!(apply(&[], &fixture(&[[0, 0, 0]], &[], &[], 1)).is_err());
        let mut broken = fixture(&[[1, 0, 0]], &[1], &[], 1);
        broken[32] ^= 0xff;
        assert!(apply(&[], &broken).is_err());
    }

    #[test]
    fn limits_decompression_and_checks_seek_overflow() {
        let expanded = compressed(&vec![0; MAX_BYTES + 1]);
        assert!(expand(&expanded).is_err());
        let patch = fixture(&[[0, 0, i64::MAX], [1, 0, 1]], &[1], &[], 1);
        assert!(apply(&[], &patch).is_err());
    }
}
