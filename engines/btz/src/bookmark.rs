// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The observed DOS bookmark stream; host paths and filesystem I/O stay external.
//!
//! The format saves variable bytes, both full stack arrays, grammar history and
//! the selected physical data disk. It does not contain IP, operands, RNG state,
//! screen/input buffers or grammar match flags. Restore continues the current
//! native service with the saved stacks rather than replacing the current IP.

use crate::{Error, read_word, vm::VmState};

pub const STACK_SLOTS: usize = 128;
pub const HISTORY_SLOTS: usize = 100;
pub const HEADER_BYTES: usize = 722;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bookmark {
    pub value_slots: [u16; STACK_SLOTS],
    pub value_depth: u16,
    pub return_slots: [u16; STACK_SLOTS],
    pub return_depth: u16,
    pub history_head: u16,
    pub history: [u16; HISTORY_SLOTS],
    pub selected_disk: u16,
    pub variables: Vec<u8>,
}

impl Bookmark {
    /// Constructs a compatible save from logical VM state. Native inactive stack
    /// slots may contain stale words; this constructor emits zero for them.
    /// Decoding and re-encoding instead preserves every inactive slot exactly.
    pub fn from_vm(
        state: &VmState,
        history: [u16; HISTORY_SLOTS],
        history_head: usize,
        selected_disk: u16,
    ) -> Result<Self, Error> {
        if state.values.len() > STACK_SLOTS || state.returns.len() > STACK_SLOTS {
            return Err(Error("bookmark stack depth exceeds native capacity".into()));
        }
        let mut value_slots = [0; STACK_SLOTS];
        value_slots[..state.values.len()].copy_from_slice(&state.values);
        let mut return_slots = [0; STACK_SLOTS];
        return_slots[..state.returns.len()].copy_from_slice(&state.returns);
        let bookmark = Self {
            value_slots,
            value_depth: state.values.len() as u16,
            return_slots,
            return_depth: state.returns.len() as u16,
            history_head: u16::try_from(history_head)
                .map_err(|_| Error("bookmark history head exceeds word range".into()))?,
            history,
            selected_disk,
            variables: state.variables.clone(),
        };
        bookmark.validate()?;
        Ok(bookmark)
    }

    pub fn values(&self) -> Result<&[u16], Error> {
        self.value_slots
            .get(..usize::from(self.value_depth))
            .ok_or_else(|| Error("invalid bookmark value-stack depth".into()))
    }

    pub fn returns(&self) -> Result<&[u16], Error> {
        self.return_slots
            .get(..usize::from(self.return_depth))
            .ok_or_else(|| Error("invalid bookmark return-stack depth".into()))
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.values()?;
        self.returns()?;
        if usize::from(self.history_head) >= HISTORY_SLOTS {
            return Err(Error("invalid bookmark grammar history head".into()));
        }
        validate_extent(self.variables.len())
    }

    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.variables.len());
        let mut put = |word: u16| bytes.extend_from_slice(&word.to_le_bytes());
        put(self.variables.len() as u16);
        for &value in &self.value_slots {
            put(value);
        }
        put(self.value_depth);
        for &value in &self.return_slots {
            put(value);
        }
        put(self.return_depth);
        put(self.history_head);
        for &value in &self.history {
            put(value);
        }
        put(self.selected_disk);
        bytes.extend_from_slice(&self.variables);
        Ok(bytes)
    }

    /// Decodes the exact fixed-size DOS stream for the supplied story extent.
    /// Extent is the only release check present in the original format; there
    /// is no game ID, checksum or interpreter revision field. Checked decoding
    /// rejects malformed/trailing bytes rather than reproducing native partial
    /// restore or stale reads on truncated input.
    pub fn decode(bytes: &[u8], expected_variable_bytes: usize) -> Result<Self, Error> {
        validate_extent(expected_variable_bytes)?;
        let extent = usize::from(read_word(bytes, 0, "bookmark variable extent")?);
        if extent != expected_variable_bytes {
            return Err(Error(
                "bookmark variable extent does not match story".into(),
            ));
        }
        if bytes.len() != HEADER_BYTES + extent {
            return Err(Error("bookmark length does not match DOS layout".into()));
        }
        let mut offset = 2;
        let mut next = || {
            let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            offset += 2;
            value
        };
        let value_slots = std::array::from_fn(|_| next());
        let value_depth = next();
        let return_slots = std::array::from_fn(|_| next());
        let return_depth = next();
        let history_head = next();
        let history = std::array::from_fn(|_| next());
        let selected_disk = next();
        let bookmark = Self {
            value_slots,
            value_depth,
            return_slots,
            return_depth,
            history_head,
            history,
            selected_disk,
            variables: bytes[HEADER_BYTES..].to_vec(),
        };
        bookmark.validate()?;
        Ok(bookmark)
    }
}

fn validate_extent(extent: usize) -> Result<(), Error> {
    if extent < 0x114 || extent > usize::from(u16::MAX) || extent % 2 != 0 {
        return Err(Error("invalid bookmark variable extent".into()));
    }
    Ok(())
}
