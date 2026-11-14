// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! HNM1 decoding, separate from game sequencing, audio playback and presentation.
//!
//! This is the headerless-tag HNM1 family, not HNM0, HNM4 or HNM6. Callers
//! explicitly select this decoder after resolving their original resource.
//! Frames have no implicit playback rate; the caller owns the original timing.
#![forbid(unsafe_code)]

mod canvas;
mod codec;
mod container;

pub use canvas::Canvas;
pub use container::Hnm1;
use std::io;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;

/// Palette entries retain their encoded channel values (VGA DAC values in Dune).
/// Undefined entries must be inherited from the engine, not invented by the codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub colors: [[u8; 3]; 256],
    pub defined: [bool; 256],
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            colors: [[0; 3]; 256],
            defined: [false; 256],
        }
    }
}

impl Palette {
    pub fn from_colors(colors: [[u8; 3]; 256]) -> Self {
        Self {
            colors,
            defined: [true; 256],
        }
    }
}

/// Typed chunk bytes, in file order. In particular, `sd` contains fragments of
/// a VOC stream, not necessarily independently decodable PCM or complete files.
/// Game metadata (`pt`, `kl`) is preserved without assigning engine semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub tag: [u8; 2],
    pub data: Vec<u8>,
}

/// Decoded bitmap update. Outer flags/mode remain available to game consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Video {
    pub width: u16,
    pub height: u16,
    pub flags: u16,
    pub mode: u8,
    pub x: u16,
    pub y: u16,
    pub pixels: Vec<u8>,
}

/// One superchunk, not an invented fixed-duration video frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub index: usize,
    pub palette: Palette,
    pub chunks: Vec<Chunk>,
    pub video: Option<Video>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}
