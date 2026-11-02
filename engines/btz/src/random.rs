// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Observed DOS generator, including its bounded rejection loop and idle steps.

use crate::{Error, image::dos_data};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RandomState {
    pub seed: u32,
    pub counter: u16,
}

#[derive(Clone, Debug)]
pub struct DosRandom {
    state: RandomState,
    adjustment: u32,
}

impl DosRandom {
    /// Uses the investigated DOS data layout; this does not detect revisions.
    pub fn from_dos_executable(exe: &[u8]) -> Result<Self, Error> {
        let bytes = dos_data(exe)?
            .get(0x6c7..0x6cb)
            .ok_or_else(|| Error("missing DOS random adjustment".into()))?;
        Ok(Self::from_state(
            RandomState {
                seed: 0x14e,
                counter: 0,
            },
            u32::from_le_bytes(bytes.try_into().unwrap()),
        ))
    }

    pub fn from_state(state: RandomState, adjustment: u32) -> Self {
        Self { state, adjustment }
    }

    pub fn state(&self) -> RandomState {
        self.state
    }

    /// One original generator call. Native key-wait loops also make these calls.
    pub fn idle_step(&mut self) -> u16 {
        let high = self.state.seed >> 16;
        let low_product = (self.state.seed & 0xffff).wrapping_mul(0x41a7);
        let carry = low_product >> 16;
        let high_product = high.wrapping_mul(0x41a7).wrapping_add(carry);
        let folded = high_product >> 15;
        let mut next = (low_product & 0xffff)
            .wrapping_sub(self.adjustment)
            .wrapping_add((high_product & 0x7fff) << 16)
            .wrapping_add(folded);
        // Preserve the observed signed comparison and addition, including the
        // adjustment stored in the supplied EXE. Do not substitute a host RNG.
        if (next as i32) > (self.adjustment as i32) {
            next = next.wrapping_add(self.adjustment);
        }
        self.state.seed = next;
        (next as i16).wrapping_abs() as u16
    }

    pub fn bounded(&mut self, maximum: u16) -> Result<u16, Error> {
        let mask = u16::MAX >> maximum.max(1).leading_zeros();
        // A malformed restored state must not trap the host indefinitely.
        for _ in 0..1_000_000 {
            let counter = self.state.counter;
            self.state.counter = counter.wrapping_add(1);
            let result = (self.idle_step() ^ counter) & mask;
            if result <= maximum {
                return Ok(result);
            }
        }
        Err(Error(
            "DOS bounded random rejection budget exhausted".into(),
        ))
    }
}
