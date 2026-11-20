// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::{self, Read, Seek, SeekFrom};

use crate::{HEIGHT, Palette, WIDTH, codec, container::palette_block, invalid, unsupported};

const HEADER_SIZE: u64 = 64;
const KEYFRAME_HEADER_SIZE: usize = 4;
const CHUNK_HEADER_SIZE: usize = 8;

/// One HNM4 picture and accumulated palette. Pixels are tightly packed in
/// display row order. Palette channels retain their encoded VGA DAC values.
#[derive(Debug)]
pub struct Hnm4Frame<'a> {
    pub index: usize,
    pub palette: &'a Palette,
    pub pixels: &'a [u8],
}

/// Standard double-buffered HNM4 video in an isolated, seekable movie region.
/// The TAB trailer is outside the frame area. Sound and timing are caller policy.
pub struct Hnm4<R> {
    reader: R,
    width: usize,
    height: usize,
    frame_count: usize,
    frame_end: u64,
    position: u64,
    index: usize,
    failed: bool,
    initial_palette: Palette,
    palette: Palette,
    packet: Vec<u8>,
    target: Vec<u8>,
    previous: Vec<u8>,
    pixels: Vec<u8>,
}

impl<R: Read + Seek> Hnm4<R> {
    pub fn new(mut reader: R, inherited_palette: Palette) -> io::Result<Self> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut header = [0; HEADER_SIZE as usize];
        reader.read_exact(&mut header)?;
        if &header[..4] != b"HNM4" {
            return Err(invalid("missing HNM4 signature"));
        }

        let width = usize::from(u16::from_le_bytes(header[8..10].try_into().unwrap()));
        let height = usize::from(u16::from_le_bytes(header[10..12].try_into().unwrap()));
        if width == 0
            || width > WIDTH
            || height == 0
            || height > HEIGHT
            || !height.is_multiple_of(2)
        {
            return Err(unsupported("unsupported HNM4 picture geometry"));
        }
        let raster_size = width * height;
        if u32_from(&header[28..]) as usize != raster_size {
            return Err(invalid("HNM4 allocation disagrees with picture geometry"));
        }

        let declared_size = u64::from(u32_from(&header[12..]));
        let table_offset = u64::from(u32_from(&header[20..]));
        let frame_end = if table_offset == 0 {
            declared_size
        } else {
            table_offset
        };
        let frame_count = u32_from(&header[16..]) as usize;
        if !(HEADER_SIZE..=file_size).contains(&declared_size)
            || !(HEADER_SIZE..=declared_size).contains(&frame_end)
            || frame_count == 0
            || frame_count as u64 > (frame_end - HEADER_SIZE) / 12
        {
            return Err(invalid("invalid HNM4 frame area or file size"));
        }

        // Gold's final IU fill can touch one unpresented pixel pair. Keep
        // that pair inside the allocation, separate from the visible raster.
        let backing_size = raster_size + usize::from(width == WIDTH && height == HEIGHT) * 2;
        Ok(Self {
            reader,
            width,
            height,
            frame_count,
            frame_end,
            position: HEADER_SIZE,
            index: 0,
            failed: false,
            initial_palette: inherited_palette.clone(),
            palette: inherited_palette,
            packet: Vec::new(),
            target: vec![0; backing_size],
            previous: vec![0; backing_size],
            pixels: vec![0; raster_size],
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn next_index(&self) -> usize {
        self.index
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn rewind(&mut self) {
        self.position = HEADER_SIZE;
        self.index = 0;
        self.failed = false;
        self.palette = self.initial_palette.clone();
        self.target.fill(0);
        self.previous.fill(0);
        self.pixels.fill(0);
    }

    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Decode the next picture. A malformed packet stops decoding until rewind.
    pub fn next_frame(&mut self) -> io::Result<Option<Hnm4Frame<'_>>> {
        if self.failed {
            return Err(invalid("HNM4 decoder stopped after a previous failure"));
        }
        if self.index == self.frame_count {
            return Ok(None);
        }
        if let Err(error) = self.decode_frame() {
            self.failed = true;
            return Err(io::Error::new(
                error.kind(),
                format!("HNM4 frame {}: {error}", self.index),
            ));
        }
        let index = self.index;
        self.index += 1;
        Ok(Some(Hnm4Frame {
            index,
            palette: &self.palette,
            pixels: &self.pixels,
        }))
    }

    fn decode_frame(&mut self) -> io::Result<()> {
        let mut size = [0; 4];
        self.reader.seek(SeekFrom::Start(self.position))?;
        self.reader.read_exact(&mut size)?;
        let length = u24_from(&size);
        let end = self.position + length as u64;
        if length < 4 + CHUNK_HEADER_SIZE
            || end > self.frame_end
            || (self.index + 1 == self.frame_count && end != self.frame_end)
        {
            return Err(invalid("invalid HNM4 superchunk size"));
        }
        self.packet.resize(length - 4, 0);
        self.reader.read_exact(&mut self.packet)?;

        let mut palette = self.palette.clone();
        let visible = self.pixels.len();
        let mut picture = false;
        let mut chunks = self.packet.as_slice();
        while !chunks.is_empty() {
            let head = chunks
                .get(..CHUNK_HEADER_SIZE)
                .ok_or_else(|| invalid("truncated HNM4 chunk header"))?;
            let length = u24_from(head);
            let body = chunks
                .get(CHUNK_HEADER_SIZE..length)
                .ok_or_else(|| invalid("invalid HNM4 chunk size"))?;
            match &head[4..6] {
                b"PL" if head[6] == 0 => {
                    palette_block(body, &mut palette)?;
                }
                b"IZ" if !picture => {
                    let data = body
                        .get(KEYFRAME_HEADER_SIZE..)
                        .ok_or_else(|| invalid("truncated HNM4 keyframe header"))?;
                    let raster = codec::expand_hnm4_keyframe(data, visible)?;
                    self.target[..visible].copy_from_slice(&raster);
                    self.previous[..visible].copy_from_slice(&raster);
                    picture = true;
                }
                b"IU" if !picture && self.index != 0 && head[6] & 1 == 0 => {
                    decode_interframe(body, self.width, visible, &mut self.target, &self.previous)?;
                    picture = true;
                }
                b"SD" => {}
                b"IU" | b"IZ" => return Err(invalid("unexpected HNM4 picture chunk")),
                _ => return Err(unsupported("unsupported HNM4 chunk")),
            }
            chunks = &chunks[length..];
        }
        if !picture {
            return Err(invalid("HNM4 superchunk has no picture"));
        }
        for (interlaced, output) in self.target[..visible]
            .chunks_exact(self.width * 2)
            .zip(self.pixels.chunks_exact_mut(self.width * 2))
        {
            let (even, odd) = output.split_at_mut(self.width);
            for ((pair, upper), lower) in interlaced.as_chunks::<2>().0.iter().zip(even).zip(odd) {
                *upper = pair[0];
                *lower = pair[1];
            }
        }
        // The next IU writes into the older buffer. A skip keeps that
        // buffer's own pixels, while a previous-frame copy reads the latest.
        std::mem::swap(&mut self.target, &mut self.previous);
        self.palette = palette;
        self.position = end;
        Ok(())
    }
}

fn u32_from(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

fn u24_from(bytes: &[u8]) -> usize {
    (u32_from(bytes) & 0xff_ffff) as usize
}

fn decode_interframe(
    mut input: &[u8],
    width: usize,
    visible: usize,
    target: &mut [u8],
    previous: &[u8],
) -> io::Result<()> {
    let mut dest = 0;
    loop {
        let Some((&command, tail)) = input.split_first() else {
            return Err(invalid("HNM4 interframe has no terminator"));
        };
        input = tail;
        let pairs = usize::from(command & 31);
        if pairs != 0 {
            let reference = u16::from_le_bytes(take(&mut input)?);
            copy_pairs(
                command,
                reference,
                width,
                dest,
                &mut target[..visible],
                &previous[..visible],
            )?;
            dest += pairs * 2;
            continue;
        }

        match command {
            0x00 => {
                let pair = take::<2>(&mut input)?;
                let end = interframe_end(dest, 2, visible)?;
                target[dest..end].copy_from_slice(&pair);
                dest = end;
            }
            0x20 => {
                let [pairs] = take(&mut input)?;
                dest = terminal_skip_or_fill(dest, pairs, visible, target.len(), input)?;
            }
            0x40 => {
                let pairs = u16::from_le_bytes(take(&mut input)?);
                dest = interframe_end(dest, usize::from(pairs) * 2, visible)?;
            }
            0x60 => {
                let [pairs, index] = take(&mut input)?;
                if pairs == 0 {
                    return Err(invalid("zero-length HNM4 interframe fill"));
                }
                let end = terminal_skip_or_fill(dest, pairs, visible, target.len(), input)?;
                target[dest..end].fill(index);
                dest = end;
            }
            _ => return Ok(()),
        }
    }
}

fn terminal_skip_or_fill(
    dest: usize,
    pairs: u8,
    visible: usize,
    backing_size: usize,
    remaining: &[u8],
) -> io::Result<usize> {
    let end = interframe_end(dest, usize::from(pairs) * 2, backing_size)?;
    if end > visible
        && (dest != visible - 2 || remaining.first().is_none_or(|next| next & 0x9f != 0x80))
    {
        return Err(invalid(
            "unexpected HNM4 interframe command outside visible raster",
        ));
    }
    Ok(end)
}

fn interframe_end(dest: usize, len: usize, limit: usize) -> io::Result<usize> {
    let end = dest + len;
    if end > limit {
        return Err(invalid("HNM4 interframe destination exceeds raster"));
    }
    Ok(end)
}

fn take<const N: usize>(input: &mut &[u8]) -> io::Result<[u8; N]> {
    let (head, tail) = input
        .split_first_chunk()
        .ok_or_else(|| invalid("truncated HNM4 interframe command"))?;
    *input = tail;
    Ok(*head)
}

fn copy_pairs(
    command: u8,
    reference: u16,
    width: usize,
    mut dest: usize,
    target: &mut [u8],
    previous: &[u8],
) -> io::Result<()> {
    let pairs = usize::from(command & 31);
    interframe_end(dest, pairs * 2, target.len())?;
    let from_previous = command & 0x20 != 0;
    let backline = command & 0x40 != 0;
    let line_delta = if backline { 1 - 2 * width as i32 } else { 1 };
    let step = if command & 0x80 != 0 { -2 } else { 2 };
    // A backline command puts the earlier line first by default.
    let swap = (reference & 1 != 0) ^ backline;
    let mut source = dest as i32 + i32::from(reference & !1) - 32_768;
    for _ in 0..pairs {
        let first = source;
        let second = source + line_delta;
        if first < 0
            || second < 0
            || first as usize >= target.len()
            || second as usize >= target.len()
        {
            return Err(invalid("HNM4 interframe reference exceeds raster"));
        }
        let raster = if from_previous { previous } else { &*target };
        let a = raster[first as usize];
        let b = raster[second as usize];
        let pair = if swap { [b, a] } else { [a, b] };
        target[dest..dest + 2].copy_from_slice(&pair);
        dest += 2;
        source += step;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hnm4_accepts_one_terminal_pair_for_byte_skip_or_fill() {
        let visible = WIDTH * HEIGHT;
        let mut target = vec![5; visible + 2];
        let previous = vec![0; visible + 2];
        let skip_to_last_pair = [0x40, 0xff, 0x7c];
        let mut packet = Vec::from(skip_to_last_pair);
        packet.extend([0x20, 2, 0x80]);
        decode_interframe(&packet, WIDTH, visible, &mut target, &previous).unwrap();
        assert_eq!(target[visible - 2..], [5, 5, 5, 5]);

        packet.truncate(skip_to_last_pair.len());
        packet.extend([0x60, 2, 17, 0x80]);
        decode_interframe(&packet, WIDTH, visible, &mut target, &previous).unwrap();
        assert_eq!(target[visible - 2..], [17, 17, 17, 17]);

        let malformed: [&[u8]; 5] = [
            &[0x20, 3, 0x80],
            &[0x40, 2, 0, 0x80],
            &[0x20, 2, 0x20, 0, 0x80],
            &[0x20, 2],
            &[0x60, 0, 0, 0x80],
        ];
        for invalid in malformed {
            packet.truncate(skip_to_last_pair.len());
            packet.extend(invalid);
            assert!(decode_interframe(&packet, WIDTH, visible, &mut target, &previous).is_err());
        }
    }
}
