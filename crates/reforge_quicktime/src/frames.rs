// Copyright (C) 2026 The REforge contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{io, time::Duration};

use nihav_core::formats::RGB555_FORMAT;
use nihav_core::frame::{NAAudioBuffer, NABufferType, NAVideoBuffer};

#[derive(Debug)]
pub enum Frame {
    Video(VideoFrame),
    Audio(AudioFrame),
}

/// One 8-bit component, tightly packed in display order.
#[derive(Debug)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// Full-range YCbCr video components.
#[derive(Debug)]
pub struct VideoFrame {
    pub luma: Plane,
    /// Cb and Cr in that order, absent on grayscale samples.
    pub chroma: Option<[Plane; 2]>,
}

/// Decoded PCM with channels interleaved in source order.
#[derive(Debug)]
pub struct AudioFrame {
    pub sample_rate: u32,
    pub channels: u8,
    pub samples: Vec<f32>,
}

impl AudioFrame {
    pub fn sample_frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.sample_frames() as f64 / f64::from(self.sample_rate))
    }
}

pub(crate) fn video(frame: &NABufferType) -> io::Result<VideoFrame> {
    if let NABufferType::Video16(buffer) = frame {
        return rgb555(buffer.as_ref());
    }
    let buffer = frame.get_vbuf().ok_or_else(unsupported_video)?;
    let info = buffer.get_info();
    let format = info.get_format();
    if !format.model.is_yuv() || format.palette {
        return Err(unsupported_video());
    }
    let read_plane = |component| {
        let (width, height) = buffer.get_dimensions(component);
        let stride = buffer.get_stride(component);
        let offset = buffer.get_offset(component);
        let data = buffer.get_data();
        let size = width
            .checked_mul(height)
            .filter(|&size| size != 0 && size <= data.len() && width <= stride)
            .ok_or_else(invalid_buffer)?;

        let mut pixels = Vec::with_capacity(size);
        for y in 0..height {
            let line = if info.is_flipped() { height - 1 - y } else { y };
            let start = line
                .checked_mul(stride)
                .and_then(|line_offset| offset.checked_add(line_offset))
                .ok_or_else(invalid_buffer)?;
            let row = data
                .get(start..start.saturating_add(width))
                .ok_or_else(invalid_buffer)?;
            pixels.extend_from_slice(row);
        }
        Ok::<_, io::Error>(Plane {
            width,
            height,
            pixels,
        })
    };
    let chroma = match buffer.get_num_components() {
        1 => None,
        3 => Some([read_plane(1)?, read_plane(2)?]),
        _ => return Err(unsupported_video()),
    };
    Ok(VideoFrame {
        luma: read_plane(0)?,
        chroma,
    })
}

fn rgb555(buffer: &NAVideoBuffer<u16>) -> io::Result<VideoFrame> {
    let info = buffer.get_info();
    if info.get_format() != RGB555_FORMAT {
        return Err(unsupported_video());
    }
    let (width, height) = buffer.get_dimensions(0);
    let stride = buffer.get_stride(0);
    let offset = buffer.get_offset(0);
    let data = buffer.get_data();
    let size = width
        .checked_mul(height)
        .filter(|&size| size != 0 && size <= data.len() && width <= stride)
        .ok_or_else(invalid_buffer)?;
    let mut planes = std::array::from_fn(|_| Plane {
        width,
        height,
        pixels: Vec::with_capacity(size),
    });
    for y in 0..height {
        let line = if info.is_flipped() { height - 1 - y } else { y };
        let start = line
            .checked_mul(stride)
            .and_then(|line_offset| offset.checked_add(line_offset))
            .ok_or_else(invalid_buffer)?;
        let row = data
            .get(start..start.saturating_add(width))
            .ok_or_else(invalid_buffer)?;
        for &pixel in row {
            let expand = |shift: u32| {
                let value = i32::from((pixel >> shift) & 31);
                (value << 3) | (value >> 2)
            };
            let (r, g, b) = (expand(10), expand(5), expand(0));
            let values = [
                (77 * r + 150 * g + 29 * b + 128) >> 8,
                128 + ((-43 * r - 85 * g + 128 * b + 128) >> 8),
                128 + ((128 * r - 107 * g - 21 * b + 128) >> 8),
            ];
            for (plane, component) in planes.iter_mut().zip(values) {
                plane.pixels.push(component.clamp(0, 255) as u8);
            }
        }
    }
    let [luma, cb, cr] = planes;
    Ok(VideoFrame {
        luma,
        chroma: Some([cb, cr]),
    })
}

pub(crate) fn audio(frame: &NABufferType) -> io::Result<AudioFrame> {
    match frame {
        NABufferType::AudioI16(buffer) => interleave(buffer, |value| f32::from(value) / 32768.0),
        NABufferType::AudioF32(buffer) => interleave(buffer, |value| value),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported QuickTime decoded audio layout",
        )),
    }
}

fn interleave<T: Copy>(
    buffer: &NAAudioBuffer<T>,
    convert: impl Fn(T) -> f32,
) -> io::Result<AudioFrame> {
    let info = buffer.get_info();
    let sample_rate = info.get_sample_rate();
    let channels = info.get_channels();
    let data = buffer.get_data();
    let channel_count = usize::from(channels);
    let sample_count = buffer
        .get_length()
        .checked_mul(channel_count)
        .filter(|&length| length <= data.len() && channels != 0 && sample_rate != 0)
        .ok_or_else(invalid_buffer)?;
    let mut samples = Vec::with_capacity(sample_count);
    for index in 0..buffer.get_length() {
        let position = index
            .checked_mul(buffer.get_step())
            .ok_or_else(invalid_buffer)?;
        for channel in 0..channel_count {
            let offset = position
                .checked_add(buffer.get_offset(channel))
                .ok_or_else(invalid_buffer)?;
            let sample = *data.get(offset).ok_or_else(invalid_buffer)?;
            samples.push(convert(sample));
        }
    }
    Ok(AudioFrame {
        sample_rate,
        channels,
        samples,
    })
}

fn invalid_buffer() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid QuickTime decoded buffer",
    )
}

fn unsupported_video() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "unsupported QuickTime decoded video layout",
    )
}
