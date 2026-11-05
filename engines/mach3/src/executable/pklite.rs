// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Decode the inspected PKLITE 1.15 large-model wrapper as a bounded data format.
//! This does not execute the stub or emulate DOS. Other PKLITE layouts are unsupported.

use super::word;
use crate::Error;

const STREAM_OFFSET: usize = 0x310;
const MAX_IMAGE: usize = 1_048_576;

pub(super) fn recognizes(raw: &[u8]) -> bool {
    raw.get(30..36) == Some(b"PKLITE")
}

fn invalid(message: &str) -> Error {
    Error::InvalidData(format!("PKLITE: {message}"))
}

fn mz_size(last: u16, pages: u16) -> Result<usize, Error> {
    if pages == 0 || last >= 512 {
        return Err(invalid("invalid MZ size"));
    }
    Ok(usize::from(pages - 1) * 512 + if last == 0 { 512 } else { usize::from(last) })
}

pub(super) fn unpack(raw: &[u8]) -> Result<Vec<u8>, Error> {
    // The version/flags and loader layout identify the unencrypted large-model
    // stream inspected for this game. Do not infer the layout from text alone.
    if word(raw, 0)? != 0x5a4d
        || word(raw, 28)? != 0x210f
        || !recognizes(raw)
        || word(raw, 8)? != 8
        || word(raw, 6)? != 1
        || word(raw, 20)? != 0x100
        || word(raw, 22)? != 0xfff0
        || word(raw, 24)? != 0x52
        || word(raw, 0x52)? != 7
        || word(raw, 0x54)? != 0
        || raw.get(0xb6..0xb9) != Some(&[0xb9, 0x23, 0x01])
        || raw.get(0xbc..0xbf) != Some(&[0xbe, 0x44, 0x01])
        || raw.get(0x142) != Some(&0xa4)
    {
        return Err(invalid("unsupported wrapper"));
    }
    if mz_size(word(raw, 2)?, word(raw, 4)?)? != raw.len() {
        return Err(invalid("outer MZ size does not match input"));
    }
    // Saved original header starts at 0x56, immediately after the outer MZ
    // relocation entry. It omits the original MZ magic and overlay word.
    let mut header = vec![0; 28];
    header[..2].copy_from_slice(b"MZ");
    header[2..26].copy_from_slice(
        raw.get(0x56..0x6e)
            .ok_or_else(|| invalid("missing original header"))?,
    );
    let header_size = usize::from(word(&header, 8)?) * 16;
    let total = mz_size(word(&header, 2)?, word(&header, 4)?)?;
    if !(28..=4096).contains(&header_size)
        || total <= header_size
        || total - header_size > MAX_IMAGE
        || word(&header, 6)? != 0
    {
        return Err(invalid("unsupported original image bounds or relocations"));
    }
    let expected = total - header_size;
    let mut stream = Stream::new(raw, STREAM_OFFSET)?;
    let mut image = Vec::with_capacity(expected);
    loop {
        if stream.bit()? == 0 {
            if image.len() == expected {
                return Err(invalid("literal exceeds original image"));
            }
            image.push(stream.byte()?);
            continue;
        }
        let mut length = stream.length()?;
        if length == 25 {
            match stream.byte()? {
                255 => break,
                254 => continue, // Segment normalization; no output or bit reset.
                extra => length += usize::from(extra),
            }
        }
        let distance = stream.distance(length)?;
        if distance == 0 || distance > image.len() {
            return Err(invalid("back-reference precedes output"));
        }
        if length > expected - image.len() {
            return Err(invalid("back-reference exceeds original image"));
        }
        // Overlap is intentional: a reference can repeat its own new bytes.
        for _ in 0..length {
            image.push(image[image.len() - distance]);
        }
    }
    if image.len() != expected || stream.byte()? != 0 {
        return Err(invalid(
            "wrong image length or unexpected inner relocations",
        ));
    }
    // The termination trailer restores SS:SP and CS:IP. Check it against the
    // saved header even though this loader will never execute those addresses.
    for offset in [14, 16, 22, 20] {
        if stream.word()? != word(&header, offset)? {
            return Err(invalid(
                "entry or stack trailer differs from original header",
            ));
        }
    }
    if stream.cursor != raw.len() {
        return Err(invalid("unexpected trailing data"));
    }
    header.resize(header_size, 0);
    header.extend(image);
    Ok(header)
}

struct Stream<'a> {
    raw: &'a [u8],
    cursor: usize,
    control: u16,
    remaining: u8,
}

impl<'a> Stream<'a> {
    fn new(raw: &'a [u8], cursor: usize) -> Result<Self, Error> {
        let mut stream = Self {
            raw,
            cursor,
            control: 0,
            remaining: 16,
        };
        stream.control = stream.word()?;
        Ok(stream)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        let value = *self
            .raw
            .get(self.cursor)
            .ok_or_else(|| invalid("truncated stream"))?;
        self.cursor += 1;
        Ok(value)
    }

    fn word(&mut self) -> Result<u16, Error> {
        let low = self.byte()?;
        Ok(u16::from_le_bytes([low, self.byte()?]))
    }

    fn bit(&mut self) -> Result<u16, Error> {
        let bit = self.control & 1;
        self.control >>= 1;
        self.remaining -= 1;
        if self.remaining == 0 {
            // The stub eagerly fetches the next word while retaining the last
            // carry bit. Delaying this until the next bit changes literal order.
            self.control = self.word()?;
            self.remaining = 16;
        }
        Ok(bit)
    }

    fn bits(&mut self, count: u8) -> Result<u16, Error> {
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | self.bit()?;
        }
        Ok(value)
    }

    fn length(&mut self) -> Result<usize, Error> {
        let mut code = self.bits(2)?;
        if code >= 2 {
            return Ok(usize::from(code));
        }
        code = (code << 1) | self.bit()?;
        if code == 0 {
            return Ok(4);
        }
        code = (code << 1) | self.bit()?;
        if code < 5 {
            return Ok(usize::from(code + 3));
        }
        code = (code << 1) | self.bit()?;
        if code < 13 {
            return Ok(usize::from(code - 2));
        }
        code = ((code & 3) << 1) | self.bit()?;
        if code < 5 {
            return Ok(if code == 4 { 25 } else { usize::from(code + 9) });
        }
        code = (code << 1) | self.bit()?;
        if code < 13 {
            return Ok(usize::from(code + 3));
        }
        code = ((code & 3) << 1) | self.bit()?;
        if code < 5 {
            return Ok(usize::from(code + 14));
        }
        code = (code << 1) | self.bit()?;
        Ok(usize::from(code + 9))
    }

    fn distance(&mut self, length: usize) -> Result<usize, Error> {
        let high = if length == 2 || self.bit()? != 0 {
            0
        } else {
            let mut code = self.bits(3)?;
            if code < 2 {
                code + 1
            } else {
                code = (code << 1) | self.bit()?;
                if code < 8 {
                    code - 1
                } else {
                    code = (code << 1) | self.bit()?;
                    if code < 23 {
                        code - 9
                    } else {
                        ((code << 1) | self.bit()?) & 0x1f
                    }
                }
            }
        };
        Ok(usize::from(high) * 256 + usize::from(self.byte()?))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    struct Writer {
        bytes: Vec<u8>,
        control: usize,
        used: u8,
    }
    impl Writer {
        fn new() -> Self {
            Self {
                bytes: vec![0, 0],
                control: 0,
                used: 0,
            }
        }
        fn bit(&mut self, bit: u8) {
            self.bytes[self.control + usize::from(self.used / 8)] |= bit << (self.used % 8);
            self.used += 1;
            if self.used == 16 {
                self.control = self.bytes.len();
                self.bytes.extend([0, 0]);
                self.used = 0;
            }
        }
        fn bits(&mut self, value: usize, count: u8) {
            for shift in (0..count).rev() {
                self.bit(((value >> shift) & 1) as u8);
            }
        }
        fn code(&mut self, code: &str) {
            for bit in code.bytes() {
                self.bit(bit - b'0');
            }
        }
        fn literal(&mut self, byte: u8) {
            self.bit(0);
            self.bytes.push(byte);
        }
        fn length(&mut self, length: usize) {
            self.bit(1);
            let codes = [
                "10",
                "11",
                "000",
                "0010",
                "0011",
                "0100",
                "01010",
                "01011",
                "01100",
                "011010",
                "011011",
                "0111010",
                "0111011",
                "0111100",
                "01111010",
                "01111011",
                "01111100",
                "011111010",
                "011111011",
                "011111100",
                "011111101",
                "011111110",
                "011111111",
            ];
            if length <= 24 {
                self.code(codes[length - 2]);
            } else {
                self.code("011100");
                self.bytes.push((length - 25) as u8);
            }
        }
        fn reference(&mut self, length: usize, distance: usize) {
            self.length(length);
            let high = distance >> 8;
            if length != 2 {
                if high == 0 {
                    self.bit(1);
                } else {
                    self.bit(0);
                    match high {
                        1..=2 => self.bits(high - 1, 3),
                        3..=6 => self.bits(high + 1, 4),
                        7..=13 => self.bits(high + 9, 5),
                        14..=31 => self.bits(high + 32, 6),
                        _ => panic!("invalid authored distance"),
                    }
                }
            }
            self.bytes.push(distance as u8);
        }
        fn marker(&mut self, byte: u8) {
            self.bit(1);
            self.code("011100");
            self.bytes.push(byte);
        }
    }

    fn put(bytes: &mut [u8], at: usize, value: usize) {
        bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    fn set_size(bytes: &mut [u8], size: usize) {
        put(bytes, 2, size % 512);
        put(bytes, 4, size.div_ceil(512));
    }
    fn original_header(image_size: usize) -> Vec<u8> {
        let mut header = vec![0; 32];
        header[..2].copy_from_slice(b"MZ");
        set_size(&mut header, 32 + image_size);
        for (at, value) in [(8, 2), (14, 32), (16, 128), (20, 16), (22, 3), (24, 28)] {
            put(&mut header, at, value);
        }
        header
    }
    fn wrapper(header: &[u8], mut stream: Writer) -> Vec<u8> {
        stream.marker(255);
        stream.bytes.push(0); // No original MZ relocations.
        for at in [14, 16, 22, 20] {
            stream.bytes.extend_from_slice(&header[at..at + 2]);
        }
        let mut raw = vec![0; STREAM_OFFSET];
        raw[..2].copy_from_slice(b"MZ");
        for (at, value) in [
            (6, 1),
            (8, 8),
            (20, 0x100),
            (22, 0xfff0),
            (24, 0x52),
            (28, 0x210f),
            (0x52, 7),
        ] {
            put(&mut raw, at, value);
        }
        raw[30..36].copy_from_slice(b"PKLITE");
        raw[0x56..0x6e].copy_from_slice(&header[2..26]);
        raw[0xb6..0xb9].copy_from_slice(&[0xb9, 0x23, 0x01]);
        raw[0xbc..0xbf].copy_from_slice(&[0xbe, 0x44, 0x01]);
        raw[0x142] = 0xa4;
        raw.extend(stream.bytes);
        let size = raw.len();
        set_size(&mut raw, size);
        raw
    }

    pub(in crate::executable) fn literals(original: &[u8]) -> Vec<u8> {
        let header_size = usize::from(word(original, 8).unwrap()) * 16;
        let mut stream = Writer::new();
        for byte in &original[header_size..] {
            stream.literal(*byte);
        }
        let mut header = original[..header_size].to_vec();
        set_size(&mut header, original.len());
        wrapper(&header, stream)
    }

    #[test]
    fn literal_refills_and_segment_markers_preserve_byte_order() {
        let expected: Vec<_> = (0..70).map(|i| (i * 37) as u8).collect();
        let header = original_header(expected.len());
        let mut stream = Writer::new();
        for (i, byte) in expected.iter().enumerate() {
            stream.literal(*byte);
            if i == 15 || i == 32 {
                stream.marker(254);
            }
        }
        let decoded = unpack(&wrapper(&header, stream)).unwrap();
        assert_eq!(&decoded[..32], header);
        assert_eq!(&decoded[32..], expected);
    }

    #[test]
    fn every_length_code_and_extended_length_allows_overlapping_copies() {
        for length in 2..=278 {
            let mut stream = Writer::new();
            stream.literal(b'Q');
            stream.reference(length, 1);
            let decoded = unpack(&wrapper(&original_header(1 + length), stream)).unwrap();
            assert_eq!(&decoded[32..], vec![b'Q'; 1 + length], "length {length}");
        }
    }

    #[test]
    fn every_distance_prefix_selects_the_expected_earlier_bytes() {
        let prefix: Vec<_> = (0..8192).map(|i| (i * 29 + i / 257) as u8).collect();
        for high in 0..32 {
            for low in [1, 255] {
                let distance = high * 256 + low;
                let mut stream = Writer::new();
                for byte in &prefix {
                    stream.literal(*byte);
                }
                stream.reference(3, distance);
                let mut expected = prefix.clone();
                for _ in 0..3 {
                    expected.push(expected[expected.len() - distance]);
                }
                let decoded = unpack(&wrapper(&original_header(expected.len()), stream)).unwrap();
                assert_eq!(&decoded[32..], expected, "distance {distance}");
            }
        }
    }

    #[test]
    fn malformed_bounds_references_and_trailers_fail_without_panicking() {
        let mut stream = Writer::new();
        for byte in 0..40 {
            stream.literal(byte);
        }
        let raw = wrapper(&original_header(40), stream);
        for end in 0..raw.len() {
            let mut truncated = raw[..end].to_vec();
            if end >= 6 {
                set_size(&mut truncated, end);
            }
            assert!(unpack(&truncated).is_err(), "length {end}");
        }
        for distance in [0, 2] {
            let mut stream = Writer::new();
            stream.literal(7);
            stream.reference(3, distance);
            assert!(unpack(&wrapper(&original_header(4), stream)).is_err());
        }
        for expected in [1, 2, 4, 8] {
            let mut stream = Writer::new();
            stream.literal(7);
            stream.reference(2, 1);
            assert!(unpack(&wrapper(&original_header(expected), stream)).is_err());
        }
        for at in [
            0,
            8,
            20,
            22,
            28,
            0x52,
            0x58,
            0x5c,
            0xb6,
            0xbc,
            0x142,
            raw.len() - 1,
        ] {
            let mut corrupt = raw.clone();
            corrupt[at] ^= 1;
            assert!(unpack(&corrupt).is_err(), "offset {at:x}");
        }
        let mut trailing = raw.clone();
        trailing.push(0);
        let size = trailing.len();
        set_size(&mut trailing, size);
        assert!(unpack(&trailing).is_err());
        let mut huge = raw.clone();
        put(&mut huge, 0x58, u16::MAX as usize);
        assert!(unpack(&huge).is_err());
    }
}
