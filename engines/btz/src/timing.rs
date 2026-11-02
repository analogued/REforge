// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Observed DOS read deadline and submission-result calculations.
//!
//! Clock ticks and the stale native stack word are explicit adapter inputs.
//! This module does not invent a wall-clock schedule or a DOS heap layout.

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadPoll {
    Wait,
    Submitted { value: u16 },
    TimedOut { value: u16 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DosReadTiming {
    timeout: u16,
    deadline: u32,
    counter_word: u16,
    formula: crate::interpreter::SubmittedRead,
}

impl DosReadTiming {
    /// The native loop sign-extends its 16-bit timeout before adding it to the
    /// BIOS tick counter. It does not normalize the deadline at BIOS midnight.
    pub fn new(timeout: u16, bios_ticks: u32, counter_word: u16) -> Self {
        Self::with_formula(
            timeout,
            bios_ticks,
            counter_word,
            crate::interpreter::SubmittedRead::ShiftEight,
        )
    }

    pub fn with_formula(
        timeout: u16,
        bios_ticks: u32,
        counter_word: u16,
        formula: crate::interpreter::SubmittedRead,
    ) -> Self {
        Self {
            formula,
            timeout,
            deadline: bios_ticks.wrapping_add(timeout as i16 as i32 as u32),
            counter_word,
        }
    }

    pub fn timeout(&self) -> u16 {
        self.timeout
    }
    pub fn deadline(&self) -> u32 {
        self.deadline
    }
    pub fn counter_word(&self) -> u16 {
        self.counter_word
    }

    /// In this conversion, the low stack word is uninitialized while its high
    /// word is explicitly zeroed. The selected interpreter applies its integer
    /// conversion, then subtracts one when nonzero. This is not elapsed time.
    pub fn submitted_value(&self) -> u16 {
        self.formula.value(self.counter_word)
    }

    /// Submitted input wins over the deadline check. Otherwise ffff disables
    /// timeout, and the clock/deadline comparison has native signed-long rules.
    /// A Wait iteration then polls a key batch and advances RNG once externally.
    pub fn poll(&self, bios_ticks: u32, ready: bool) -> ReadPoll {
        if ready {
            ReadPoll::Submitted {
                value: self.submitted_value(),
            }
        } else if self.timeout != 0xffff && (bios_ticks as i32) >= (self.deadline as i32) {
            ReadPoll::TimedOut {
                value: self.timeout,
            }
        } else {
            ReadPoll::Wait
        }
    }

    /// Converts an explicit observed submission result into one representative
    /// scratch word. Native submission results are limited to0..254; a timeout
    /// return is separate and can contain any original 16-bit timeout value.
    pub fn counter_word_for_value(value: u16) -> Result<u16, Error> {
        match value {
            0 => Ok(0),
            1..=254 => Ok((value + 1) << 8),
            _ => Err(Error(
                "submission result exceeds native stale-word range".into(),
            )),
        }
    }
}
