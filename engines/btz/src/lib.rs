// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Experimental DOS BTZ core and native services; other revisions remain unvalidated.
#![deny(unsafe_code)]

pub mod bookmark;
#[cfg(feature = "frontend")]
pub mod cli;
pub mod decode;
#[cfg(feature = "sdl")]
pub mod display;
pub mod image;
pub mod input;
#[cfg(feature = "sdl")]
pub mod launcher;
pub mod media;
pub mod parser;
#[cfg(feature = "frontend")]
pub mod player;
pub mod random;
pub mod services;
pub mod session;
#[cfg(feature = "sdl")]
mod sound;
pub mod text;
pub mod timing;
pub mod vm;
#[cfg(feature = "sdl")]
pub mod windowed;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub(crate) fn read_word(bytes: &[u8], address: usize, space: &str) -> Result<u16, Error> {
    let pair = bytes
        .get(address..address.saturating_add(2))
        .ok_or_else(|| Error(format!("{space} word outside image at {address:#x}")))?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramAddr(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VarAddr(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectId(pub u16);
