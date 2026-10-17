// Copyright (c) 2026 analogued. Project license has not been selected.
//! Replaceable timed-level rendering. Renderers observe committed simulation;
//! they never advance devices, execute handlers, or choose game sound actions.
mod mixer;
pub use mixer::CategoryMixer;

#[cfg(feature = "blip")]
mod blip;
#[cfg(feature = "blip")]
pub use blip::BlipRenderer;
#[cfg(feature = "blip")]
mod categorized;
#[cfg(feature = "blip")]
pub use categorized::CategorizedRenderer;

use std::{fmt, io};

/// Presentation amplitude units, with headroom for impulse/filter arithmetic.
/// This scale is not a voltage, acoustic amplitude, or hardware quantization.
pub const MAX_LEVEL: i16 = 8192;

/// Emit bounded blocks of mono signed 16-bit samples at the renderer's rate.
/// The receiver decides whether to retain, write, queue, or discard them.
pub type Output<'a> = dyn FnMut(&[i16]) -> Result<(), Error> + 'a;

/// All timestamps are absolute integer source-clock pulses. Edges at a chunk's
/// end belong to the following chunk at offset zero; equal-time levels remain
/// ordered. Output rate and clock frequency are fixed for one renderer instance.
pub trait SignalRenderer {
    fn level_at(&mut self, tick: u64, level: i16, output: &mut Output<'_>) -> Result<(), Error>;
    fn finish_until(&mut self, tick: u64, output: &mut Output<'_>) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub clock_hz: u32,
    pub sample_rate: u32,
    /// Maximum local synthesis interval. Independent of game-update duration.
    pub chunk_ticks: u32,
}

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Backend(&'static str),
    Io(io::Error),
    /// A failed output may have consumed a partial block; retry is not safe.
    Stopped,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) | Self::Backend(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
            Self::Stopped => f.write_str("renderer stopped after a previous failure"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
