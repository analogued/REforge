// Copyright (c) 2026 analogued. Project license has not been selected.
//! DOS CGA asset decoding, composition, and bounded opening simulation.
#![forbid(unsafe_code)]

pub mod assets;
#[cfg(feature = "viewer")]
pub mod cli;
pub mod frame;
#[cfg(all(feature = "viewer", feature = "dev-tools"))]
mod input_trace;
pub mod live;
pub mod opening;
pub mod rules;
pub mod scene;
pub mod session_capture;
pub mod sound;
pub mod startup;
pub mod timer;
#[cfg(all(feature = "viewer", feature = "dev-tools"))]
mod timing_trace;
#[cfg(feature = "viewer")]
pub mod viewer;

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
