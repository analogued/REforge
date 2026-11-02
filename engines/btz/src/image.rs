// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only DOS object container with the five stores observed in Mindwheel.

use std::ops::Range;

use crate::{Error, ProgramAddr, read_word};

pub(crate) fn dos_data(exe: &[u8]) -> Result<&[u8], Error> {
    if exe.get(..2) != Some(b"MZ") {
        return Err(Error(
            "interpreter data requires a DOS MZ executable".into(),
        ));
    }
    let base = usize::from(read_word(exe, 8, "MZ header")?) * 16
        + usize::from(read_word(exe, 0x0e, "MZ stack segment")?) * 16;
    exe.get(base..)
        .ok_or_else(|| Error("interpreter data segment outside executable".into()))
}

#[derive(Clone, Debug)]
pub struct StoryImage {
    bytes: Vec<u8>,
    variable_bytes: usize,
    regions: [Range<usize>; 5],
    text_overrides: Option<[Vec<u8>; 3]>,
}

impl StoryImage {
    /// Parses the observed DOS layout; this is not game/revision detection.
    pub fn from_dos_obj(bytes: Vec<u8>) -> Result<Self, Error> {
        let variable_bytes = usize::from(read_word(&bytes, 0, "initial variables")?);
        if variable_bytes < 0x114 || variable_bytes % 2 != 0 {
            return Err(Error("invalid initial variable extent".into()));
        }
        let header = variable_bytes.div_ceil(512) * 512;
        let starts = [4, 0x0e, 0x0c, 8, 0x0a];
        let ends = [0x10, 0x1a, 0x18, 0x14, 0x16];
        let mut regions: [Range<usize>; 5] = std::array::from_fn(|_| 0..0);
        for i in 0..5 {
            let start = usize::from(read_word(&bytes, header + starts[i], "directory")?) * 512;
            let end = usize::from(read_word(&bytes, header + ends[i], "directory")?) * 512;
            if start > end || end > bytes.len() {
                return Err(Error(format!("invalid store {i} extent")));
            }
            regions[i] = start..end;
        }
        let mut physical = regions.clone();
        physical.sort_by_key(|r| r.start);
        if physical[0].start != header
            || physical.windows(2).any(|w| w[0].end != w[1].start)
            || physical[4].end != bytes.len()
        {
            return Err(Error("stores do not cover the object image".into()));
        }
        Ok(Self {
            bytes,
            variable_bytes,
            regions,
            text_overrides: None,
        })
    }

    pub fn initial_variables(&self) -> &[u8] {
        &self.bytes[..self.variable_bytes]
    }

    pub fn store(&self, store: usize) -> Result<&[u8], Error> {
        if (2..=4).contains(&store)
            && let Some(text) = &self.text_overrides
        {
            return Ok(&text[store - 2]);
        }
        let region = self
            .regions
            .get(store)
            .ok_or_else(|| Error("invalid store ID".into()))?;
        Ok(&self.bytes[region.clone()])
    }

    /// Disk selection invalidates the three text stores only. Program, grammar
    /// and initial-variable metadata remain those of the boot image.
    pub(crate) fn replace_text_stores(
        &mut self,
        disk: &crate::media::TextStores,
    ) -> Result<(), Error> {
        self.text_overrides = Some([
            disk.store(2)?.to_vec(),
            disk.store(3)?.to_vec(),
            disk.store(4)?.to_vec(),
        ]);
        Ok(())
    }

    pub fn program(&self) -> &[u8] {
        &self.bytes[self.regions[0].clone()]
    }

    pub fn program_word(&self, address: ProgramAddr) -> Result<u16, Error> {
        read_word(self.program(), usize::from(address.0), "program")
    }
}
