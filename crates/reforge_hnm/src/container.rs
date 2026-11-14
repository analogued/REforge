// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::{self, Read, Seek, SeekFrom};

use crate::{Chunk, Frame, HEIGHT, Palette, Video, WIDTH, codec, invalid, unsupported};

/// An explicitly selected HNM1 stream, with bounded per-superchunk allocation.
/// Its input must be the isolated movie region, not an enclosing archive file.
/// Offset-table entries are validated; decoding stays sequential because updates
/// and palette changes can depend on preceding frames and inherited game state.
pub struct Hnm1<R> {
    reader: R,
    header_size: u64,
    offsets: Vec<u32>,
    initial_palette: Palette,
    palette: Palette,
    index: usize,
    failed: bool,
}

impl<R: Read + Seek> Hnm1<R> {
    pub fn new(mut reader: R, inherited_palette: Palette) -> io::Result<Self> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut size = [0; 2];
        reader.read_exact(&mut size)?;
        let header_size = u16::from_le_bytes(size) as usize;
        if header_size < 4 || header_size as u64 > file_size {
            return Err(invalid("invalid HNM1 header length"));
        }
        let mut header = vec![0; header_size - 2];
        reader.read_exact(&mut header)?;
        let mut palette = inherited_palette;
        let mut pos = palette_block(&header, &mut palette)?;
        // The first index entry is zero. Alignment padding is encoded as FF.
        while pos < header.len() && header[pos] == 0xff {
            pos += 1;
        }
        let table = &header[pos..];
        if table.len() < 8 || !table.len().is_multiple_of(4) {
            return Err(invalid("invalid HNM1 frame index length"));
        }
        let offsets: Vec<_> = table
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v))
            .collect();
        if offsets[0] != 0
            || offsets
                .windows(2)
                .any(|p| p[1] <= p[0] || p[1] - p[0] > u16::MAX as u32)
            || u64::from(*offsets.last().unwrap()) != file_size - header_size as u64
        {
            return Err(invalid("invalid HNM1 frame offsets or file length"));
        }
        Ok(Self {
            reader,
            header_size: header_size as u64,
            offsets,
            initial_palette: palette.clone(),
            palette,
            index: 0,
            failed: false,
        })
    }

    pub fn frame_count(&self) -> usize {
        self.offsets.len() - 1
    }
    pub fn next_index(&self) -> usize {
        self.index
    }
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Reset stream/palette decoding. A separate compositor must also reset to
    /// its inherited background when replaying deltas from the beginning.
    pub fn rewind(&mut self) {
        self.index = 0;
        self.palette = self.initial_palette.clone();
        self.failed = false;
    }

    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Decode one superchunk. Errors stop iteration until an explicit rewind;
    /// partially decoded state is never committed as a successful frame.
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        if self.failed {
            return Err(invalid("HNM1 decoder stopped after a previous failure"));
        }
        if self.index == self.frame_count() {
            return Ok(None);
        }
        let result = self.read_frame();
        if result.is_err() {
            self.failed = true;
        }
        result.map(Some)
    }

    fn read_frame(&mut self) -> io::Result<Frame> {
        let start = self.offsets[self.index];
        let len = (self.offsets[self.index + 1] - start) as usize;
        self.reader
            .seek(SeekFrom::Start(self.header_size + u64::from(start)))?;
        let mut packet = vec![0; len];
        self.reader.read_exact(&mut packet)?;
        if len < 2 || u16::from_le_bytes(packet[..2].try_into().unwrap()) as usize != len {
            return Err(invalid("HNM1 superchunk size disagrees with frame index"));
        }
        let mut palette = self.palette.clone();
        let mut pos = 2;
        let mut chunks = Vec::new();
        let mut video = None;
        while pos < packet.len() {
            if packet.len() - pos < 4 {
                return Err(invalid("truncated HNM1 chunk header"));
            }
            let word = u16::from_le_bytes(packet[pos..pos + 2].try_into().unwrap());
            if word & 0x7000 == 0 {
                video = decode_video(&packet[pos..])?;
                break;
            }
            let tag = [packet[pos], packet[pos + 1]];
            let len = u16::from_le_bytes(packet[pos + 2..pos + 4].try_into().unwrap()) as usize;
            if len < 4 || len > packet.len() - pos {
                return Err(invalid("invalid HNM1 typed chunk size"));
            }
            let data = packet[pos + 4..pos + len].to_vec();
            if &tag == b"pl" {
                let consumed = palette_block(&data, &mut palette)?;
                if data[consumed..].iter().any(|v| *v != 0xff) {
                    return Err(invalid("non-padding bytes after HNM1 palette update"));
                }
            }
            chunks.push(Chunk { tag, data });
            pos += len;
        }
        let frame = Frame {
            index: self.index,
            palette: palette.clone(),
            chunks,
            video,
        };
        self.index += 1;
        self.palette = palette;
        Ok(frame)
    }
}

fn palette_block(bytes: &[u8], palette: &mut Palette) -> io::Result<usize> {
    let mut pos = 0;
    loop {
        let range = bytes
            .get(pos..pos + 2)
            .ok_or_else(|| invalid("truncated HNM1 palette range"))?;
        pos += 2;
        if range == [0xff, 0xff] {
            return Ok(pos);
        }
        let start = range[0] as usize;
        let count = if range[1] == 0 {
            256
        } else {
            range[1] as usize
        };
        if start + count > 256 {
            return Err(invalid("HNM1 palette range exceeds 256 colors"));
        }
        let values = bytes
            .get(pos..pos + count * 3)
            .ok_or_else(|| invalid("truncated HNM1 palette colors"))?;
        for (index, color) in values.as_chunks::<3>().0.iter().enumerate() {
            palette.colors[start + index].copy_from_slice(color);
            palette.defined[start + index] = true;
        }
        pos += count * 3;
    }
}

fn decode_video(bytes: &[u8]) -> io::Result<Option<Video>> {
    let word = u16::from_le_bytes(bytes[..2].try_into().unwrap());
    let width = word & 0x1ff;
    let flags = word & !0x1ff;
    let height = u16::from(bytes[2]);
    let mode = bytes[3];
    if width == 0 || height == 0 {
        if bytes.len() != 4 {
            return Err(invalid("invalid HNM1 empty bitmap"));
        }
        return Ok(None);
    }
    if width as usize > WIDTH || height as usize > HEIGHT {
        return Err(invalid("HNM1 bitmap exceeds 320x200 viewport"));
    }
    if flags & !0x8600 != 0 || !matches!(mode, 0xfe | 0xff) {
        return Err(unsupported("unsupported HNM1 bitmap flags or drawing mode"));
    }
    let mut body = bytes[4..].to_vec();
    if flags & 0x0600 != 0 {
        if body.len() < 6 {
            return Err(invalid("truncated HNM1 compressed bitmap"));
        }
        let len = u16::from_le_bytes(body[..2].try_into().unwrap()) as usize;
        body = codec::decode(&body, len)?.pixels;
    }
    let (x, y, skip) = if flags & 0x0400 != 0 {
        (0, 0, 0)
    } else {
        if body.len() < 4 {
            return Err(invalid("truncated HNM1 bitmap coordinates"));
        }
        (
            u16::from_le_bytes(body[..2].try_into().unwrap()),
            u16::from_le_bytes(body[2..4].try_into().unwrap()),
            4,
        )
    };
    let expected = width as usize * height as usize;
    let pixels = if flags & 0x8000 != 0 && flags & 0x0400 == 0 {
        codec::decode_rle(&body[skip..], expected, width as usize)?
    } else {
        if body.len() - skip != expected {
            return Err(invalid("HNM1 raster length disagrees with dimensions"));
        }
        body[skip..].to_vec()
    };
    Ok(Some(Video {
        width,
        height,
        flags,
        mode,
        x,
        y,
        pixels,
    }))
}
