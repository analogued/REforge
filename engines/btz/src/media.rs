// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Text stores supplied by original DOS media, separate from live VM metadata.

use crate::{Error, image::StoryImage, read_word};

/// Only the three stores invalidated on a native mapped-drive change. This type
/// cannot be mistaken for a complete story with initial variables or wordcode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextStores {
    stores: [Vec<u8>; 3],
}

impl TextStores {
    /// Primary media uses the observed full object container, whose directory
    /// follows its padded initial variable image.
    pub fn from_primary_dos_obj(bytes: Vec<u8>) -> Result<Self, Error> {
        Self::from_story_image(&StoryImage::from_dos_obj(bytes)?)
    }

    pub fn from_story_image(image: &StoryImage) -> Result<Self, Error> {
        Ok(Self {
            stores: [
                image.store(2)?.to_vec(),
                image.store(3)?.to_vec(),
                image.store(4)?.to_vec(),
            ],
        })
    }

    /// In the investigated DOS interpreter, physical disk numbers other than1
    /// place their directory at block0. No initial variable image is assumed.
    /// This follows the verified loader algorithm; no actual secondary release
    /// was supplied, so it does not claim broader media/revision compatibility.
    ///
    /// Required ranges are checked independently. Unreferenced bytes or shared
    /// views are allowed: their exclusion is not established for secondary media.
    pub fn from_secondary_dos_obj(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() < 512 {
            return Err(Error(
                "secondary DOS media lacks a complete directory block".into(),
            ));
        }
        let mut stores: [Vec<u8>; 3] = std::array::from_fn(|_| Vec::new());
        for (index, (start_field, end_field)) in [(0x0c, 0x18), (8, 0x14), (0x0a, 0x16)]
            .into_iter()
            .enumerate()
        {
            let start =
                usize::from(read_word(&bytes, start_field, "secondary media directory")?) * 512;
            let end = usize::from(read_word(&bytes, end_field, "secondary media directory")?) * 512;
            if start > end || end > bytes.len() {
                return Err(Error(format!(
                    "secondary DOS store{} outside media",
                    index + 2
                )));
            }
            stores[index] = bytes[start..end].to_vec();
        }
        Ok(Self { stores })
    }

    pub fn store(&self, id: usize) -> Result<&[u8], Error> {
        id.checked_sub(2)
            .and_then(|index| self.stores.get(index))
            .map(Vec::as_slice)
            .ok_or_else(|| Error("media replacement contains only text stores2,3,4".into()))
    }
}
