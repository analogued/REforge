// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Raster decompression, independent of palette and canvas composition.
//! Implemented from the published format description and checked against original
//! data. No third-party decoder implementation is included.

use std::io::{self, ErrorKind};

#[derive(Debug)]
pub(crate) struct Decoded {
    pub pixels: Vec<u8>,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

struct Input<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Input<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .ok_or_else(|| invalid("raster input position overflow"))?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| invalid("truncated raster data"))?;
        self.pos = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn word(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
}

#[derive(Clone, Copy)]
enum LzBitOrder {
    LowFirst16,
    HighFirst32,
}

struct Bits {
    order: LzBitOrder,
    word: u32,
    left: u8,
}

impl Bits {
    fn next(&mut self, input: &mut Input<'_>) -> io::Result<bool> {
        if self.left == 0 {
            (self.word, self.left) = match self.order {
                LzBitOrder::LowFirst16 => (u32::from(input.word()?), 16),
                LzBitOrder::HighFirst32 => {
                    (u32::from_le_bytes(input.take(4)?.try_into().unwrap()), 32)
                }
            };
        }
        self.left -= 1;
        Ok(match self.order {
            LzBitOrder::LowFirst16 => {
                let set = self.word & 1 != 0;
                self.word >>= 1;
                set
            }
            LzBitOrder::HighFirst32 => self.word & (1 << self.left) != 0,
        })
    }
}

fn reserve(len: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| invalid("cannot allocate raster buffer"))?;
    Ok(bytes)
}

fn repeat_backwards(
    output: &mut Vec<u8>,
    distance: usize,
    count: usize,
    limit: usize,
) -> io::Result<()> {
    if distance == 0 || distance > output.len() {
        return Err(invalid("raster back-reference precedes output"));
    }
    if count > limit - output.len() {
        return Err(invalid("raster back-reference exceeds output size"));
    }
    for _ in 0..count {
        output.push(output[output.len() - distance]);
    }
    Ok(())
}

fn hsq(payload: &[u8], expected_len: usize) -> io::Result<Decoded> {
    let mut input = Input {
        bytes: payload,
        pos: 0,
    };
    let len = usize::from(input.word()?);
    if len != expected_len || input.byte()? != 0 {
        return Err(invalid("HSQ expanded size disagrees with expected length"));
    }
    let packed = usize::from(input.word()?);
    input.byte()?;
    if packed < 6 || packed != payload.len() {
        return Err(invalid("HSQ packed size disagrees with raster data"));
    }
    let pixels = expand_lz(&mut input, len, LzBitOrder::LowFirst16)?;
    if pixels.len() != len || input.pos != packed {
        return Err(invalid("HSQ terminator disagrees with declared size"));
    }
    Ok(Decoded { pixels })
}

/// HNM4 IZ reuses the HSQ command encoding with a wider, high-first bit queue.
pub(crate) fn expand_hnm4_keyframe(payload: &[u8], limit: usize) -> io::Result<Vec<u8>> {
    let mut input = Input {
        bytes: payload,
        pos: 0,
    };
    expand_lz(&mut input, limit, LzBitOrder::HighFirst32)
}

fn expand_lz(input: &mut Input<'_>, len: usize, order: LzBitOrder) -> io::Result<Vec<u8>> {
    let mut output = reserve(len)?;
    let mut bits = Bits {
        order,
        word: 0,
        left: 0,
    };
    loop {
        if bits.next(input)? {
            if output.len() == len {
                return Err(invalid("raster literal exceeds output size"));
            }
            output.push(input.byte()?);
            continue;
        }
        let (distance, count) = if bits.next(input)? {
            let pair = input.word()?;
            let mut count = usize::from(pair & 7);
            if count == 0 {
                count = usize::from(input.byte()?);
                if count == 0 {
                    break;
                }
            }
            (8192 - usize::from(pair >> 3), count + 2)
        } else {
            let count = usize::from(bits.next(input)?) * 2 + usize::from(bits.next(input)?) + 2;
            (256 - usize::from(input.byte()?), count)
        };
        repeat_backwards(&mut output, distance, count, len)?;
    }
    Ok(output)
}

/// Expand the second-stage signed-control raster encoding. Output is eight-bit
/// palette indices with no row alignment; geometry belongs to the caller.
pub(crate) fn decode_rle(
    payload: &[u8],
    expected_len: usize,
    row_width: usize,
) -> io::Result<Vec<u8>> {
    if row_width == 0
        || expected_len > usize::from(u16::MAX)
        || !expected_len.is_multiple_of(row_width)
    {
        return Err(invalid("raster exceeds HNM1 length range"));
    }
    let mut input = Input {
        bytes: payload,
        pos: 0,
    };
    let mut output = reserve(expected_len)?;
    while output.len() < expected_len {
        let control = i16::from(input.byte()? as i8);
        let count = if control < 0 {
            1 - control
        } else {
            control + 1
        } as usize;
        if count > expected_len - output.len() || count > row_width - output.len() % row_width {
            return Err(invalid("raster RLE exceeds output size"));
        }
        if control < 0 {
            let pixel = input.byte()?;
            output.resize(output.len() + count, pixel);
        } else {
            output.extend_from_slice(input.take(count)?);
        }
    }
    if input.pos != payload.len() {
        return Err(invalid("trailing raster RLE data"));
    }
    Ok(output)
}

pub(crate) fn decode(payload: &[u8], expected_len: usize) -> io::Result<Decoded> {
    let header = payload
        .get(..6)
        .ok_or_else(|| invalid("truncated raster header"))?;
    let checksum = header
        .iter()
        .fold(0u8, |sum, &value| sum.wrapping_add(value));
    match checksum {
        171 => hsq(payload, expected_len),
        _ => Err(io::Error::new(
            ErrorKind::Unsupported,
            "unsupported HNM1 raster compression checksum",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hsq_block(body: &[u8], len: u16) -> Vec<u8> {
        let mut x = Vec::from(len.to_le_bytes());
        x.push(0);
        x.extend(u16::try_from(body.len() + 6).unwrap().to_le_bytes());
        x.push(171u8.wrapping_sub(x.iter().fold(0u8, |sum, &v| sum.wrapping_add(v))));
        x.extend(body);
        x
    }

    #[test]
    fn hsq_literals_and_overlapping_copies() {
        // Literal A, then short distance-one copy of three bytes, then end.
        let decoded = decode(&hsq_block(&[0x51, 0, b'A', 255, 0, 0, 0], 4), 4).unwrap();
        assert_eq!(decoded.pixels, b"AAAA");
    }

    #[test]
    fn hsq_long_copy_and_extended_length() {
        // Literal X, long distance-one copy with explicit length 8+2, end.
        let x = hsq_block(&[0x15, 0, b'X', 0xf8, 0xff, 8, 0, 0, 0], 11);
        assert_eq!(decode(&x, 11).unwrap().pixels, vec![b'X'; 11]);
    }

    #[test]
    fn hsq_flag_words_refill_without_consuming_literal_bytes() {
        let mut body = vec![255, 255];
        body.extend([b'Q'; 16]);
        body.extend([5, 0, b'Q', 0, 0, 0]);
        assert_eq!(
            decode(&hsq_block(&body, 17), 17).unwrap().pixels,
            vec![b'Q'; 17]
        );
    }

    #[test]
    fn hsq_rejects_truncation_bad_spans_and_sizes() {
        let x = hsq_block(&[0x51, 0, b'A', 255, 0, 0, 0], 4);
        for len in 0..x.len() {
            assert!(decode(&x[..len], 4).is_err(), "truncated length {len}");
        }
        assert!(decode(&x, usize::MAX).is_err());
        assert!(decode(&x, 3).is_err());
        // Back-reference before any literal output.
        assert!(decode(&hsq_block(&[0, 0, 255], 2), 2).is_err());
        // Terminator before declared expanded length.
        assert!(decode(&hsq_block(&[2, 0, 0, 0, 0], 1), 1).is_err());
        // A valid stream whose declared output cannot contain its copy.
        assert!(decode(&hsq_block(&[0x51, 0, b'A', 255, 0, 0, 0], 3), 3).is_err());
    }

    #[test]
    fn hsq_does_not_read_beyond_declared_packed_span() {
        // Missing terminator cannot be satisfied with bytes outside packed_len.
        let mut x = hsq_block(&[1, 0, b'A'], 1);
        x.extend([0, 0, 0]);
        assert!(decode(&x, 1).is_err());
    }

    #[test]
    fn unverified_codec_types_are_explicitly_unsupported() {
        for checksum in [172u8, 173] {
            let x = [1, 0, 0, 0, 0, checksum - 1];
            assert_eq!(decode(&x, 1).unwrap_err().kind(), ErrorKind::Unsupported);
        }
    }

    #[test]
    fn rle_literal_and_repeat_runs_preserve_pixel_values() {
        let x = [2, 7, 0, 255, 0xff, 9, 0, 4];
        assert_eq!(decode_rle(&x, 6, 6).unwrap(), [7, 0, 255, 9, 9, 4]);
        // Extreme signed controls use 129 repeated or 128 literal pixels.
        assert_eq!(decode_rle(&[0x80, 17], 129, 129).unwrap(), vec![17; 129]);
        let mut literal = vec![127];
        literal.extend(0..128);
        assert_eq!(
            decode_rle(&literal, 128, 128).unwrap(),
            (0..128).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rle_is_not_aligned_or_padded_by_the_decoder() {
        // An odd-width 7x7 image needs exactly 49 decoded bytes, not 56.
        assert_eq!(
            decode_rle(&[0xfa, 12].repeat(7), 49, 7).unwrap(),
            vec![12; 49]
        );
        assert!(decode_rle(&[0xd0, 12], 49, 7).is_err());
        assert!(decode_rle(&[0xd0, 12], 56, 56).is_err());
        assert!(decode_rle(&[0xd0, 12, 0], 49, 49).is_err());
    }

    #[test]
    fn rle_rejects_overflows_and_missing_payloads() {
        assert!(decode_rle(&[0xff, 7], 1, 1).is_err());
        assert!(decode_rle(&[0xff], 2, 2).is_err());
        assert!(decode_rle(&[2, 1, 2], 3, 3).is_err());
        assert!(decode_rle(&[], 1, 1).is_err());
        assert!(decode_rle(&[], usize::MAX, 1).is_err());
        assert!(decode_rle(&[], 0, 1).unwrap().is_empty());
    }
}
