// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! DOS CGA asset decoding, composition, and reconstructed gameplay.
#![forbid(unsafe_code)]

pub mod assets;
#[cfg(feature = "sdl")]
pub mod cli;
pub mod executable;
pub mod frame;
pub mod gameplay;
#[cfg(all(feature = "sdl", feature = "dev-tools"))]
mod input_trace;
#[cfg(feature = "sdl")]
pub mod launcher;
pub mod live;
#[cfg(feature = "dev-tools")]
pub mod motion;
#[cfg(feature = "sdl")]
pub mod platform;
pub mod scene;
pub mod session_replay;
pub mod sound;
pub mod startup;
pub mod timer;
#[cfg(all(feature = "sdl", feature = "dev-tools"))]
mod timing_trace;

#[derive(Debug)]
pub enum Error {
    InvalidData(String),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidData(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidData(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
