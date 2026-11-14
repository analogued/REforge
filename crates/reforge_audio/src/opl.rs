// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later
//! Replaceable OPL register-to-PCM rendering. Engines own sequencing, instruments,
//! clocks and register event ordering; this layer never advances game logic.
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid(&'static str),
    Backend(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => f.write_str(s),
            Self::Backend(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for Error {}

/// Render to interleaved stereo signed16 PCM at the fixed instance sample rate.
/// Render up to an event's sample boundary, then apply ordered writes. Both OPL3
/// register banks are available; reset leaves the chip in OPL2 compatibility mode.
pub trait Opl {
    fn sample_rate(&self) -> u32;
    fn reset(&mut self);
    fn write(&mut self, register: u16, value: u8) -> Result<(), Error>;
    fn render(&mut self, output: &mut [i16]) -> Result<(), Error>;
}

pub struct Nuked {
    chip: nuked_opl3::Opl3Chip,
    rate: u32,
}
impl Nuked {
    /// Common host PCM rates. The range avoids degenerate upstream resampler ratios.
    pub fn new(sample_rate: u32) -> Result<Self, Error> {
        if !(8000..=192000).contains(&sample_rate) {
            return Err(Error::Invalid("OPL sample rate must be 8000..192000 Hz"));
        }
        Ok(Self {
            chip: nuked_opl3::Opl3Chip::new(sample_rate),
            rate: sample_rate,
        })
    }
}
impl Opl for Nuked {
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn reset(&mut self) {
        self.chip.reset(self.rate);
    }
    fn write(&mut self, register: u16, value: u8) -> Result<(), Error> {
        if register > 0x1ff {
            return Err(Error::Invalid("OPL register exceeds two banks"));
        }
        self.chip.write_register(register, value);
        Ok(())
    }
    fn render(&mut self, output: &mut [i16]) -> Result<(), Error> {
        if output.is_empty() {
            return Ok(());
        }
        if output.len() % 2 != 0 {
            return Err(Error::Invalid("OPL stereo buffer has an odd sample count"));
        }
        self.chip
            .generate_stream(output)
            .map_err(|e| Error::Backend(format!("{e:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tone(opl: &mut dyn Opl) {
        for (r, v) in [
            (0x105, 1),
            (0x120, 1),
            (0x123, 1),
            (0x140, 0x20),
            (0x143, 0),
            (0x160, 0xf0),
            (0x163, 0xf0),
            (0x180, 0x77),
            (0x183, 0x77),
            (0x1c0, 0x31),
            (0x1a0, 0x98),
            (0x1b0, 0x31),
        ] {
            opl.write(r, v).unwrap();
        }
    }
    #[test]
    fn second_bank_tone_is_chunk_invariant_and_reset_reproducible() {
        let mut a = Nuked::new(48000).unwrap();
        tone(&mut a);
        let mut whole = vec![0; 8192];
        a.render(&mut whole).unwrap();
        assert!(whole.iter().any(|v| *v != 0));
        let mut b = Nuked::new(48000).unwrap();
        tone(&mut b);
        let mut split = vec![0; 8192];
        for part in split.chunks_mut(34) {
            b.render(part).unwrap();
        }
        assert_eq!(whole, split);
        a.reset();
        tone(&mut a);
        a.render(&mut split).unwrap();
        assert_eq!(whole, split);
    }
    #[test]
    fn bad_rates_registers_and_sample_layout_are_rejected() {
        assert!(Nuked::new(0).is_err());
        let mut opl = Nuked::new(48000).unwrap();
        assert_eq!(opl.sample_rate(), 48000);
        assert!(opl.write(0x200, 1).is_err());
        assert!(opl.render(&mut [0]).is_err());
        opl.render(&mut []).unwrap();
    }
}
