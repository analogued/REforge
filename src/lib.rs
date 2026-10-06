// Copyright (c) 2026 analogued. Project license has not been selected.
//! DOS CGA asset decoding, composition, and bounded opening simulation.
#![forbid(unsafe_code)]

pub mod assets;
pub mod frame;
pub mod opening;
pub mod rules;
pub mod scene;
pub mod session_capture;
pub mod startup;
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
