// Copyright (c) 2026 analogued. Project license has not been selected.
//! Headerless sample resources and explicitly supplied bytes outside them.

use std::{collections::BTreeMap, path::Path};

use crate::{Error, assets::read_resource};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Resource {
    Data7,
    Data8,
    Data9,
    DataA,
    DataB,
    DataC,
    DataD,
    DataE,
    DataF,
    DataG,
    DataH,
}

impl Resource {
    pub const ALL: [Self; 11] = [
        Self::Data7,
        Self::Data8,
        Self::Data9,
        Self::DataA,
        Self::DataB,
        Self::DataC,
        Self::DataD,
        Self::DataE,
        Self::DataF,
        Self::DataG,
        Self::DataH,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Data7 => "DATA7",
            Self::Data8 => "DATA8",
            Self::Data9 => "DATA9",
            Self::DataA => "DATAA",
            Self::DataB => "DATAB",
            Self::DataC => "DATAC",
            Self::DataD => "DATAD",
            Self::DataE => "DATAE",
            Self::DataF => "DATAF",
            Self::DataG => "DATAG",
            Self::DataH => "DATAH",
        }
    }

    pub const fn bytes(self) -> usize {
        match self {
            Self::Data7 => 11_872,
            Self::Data8 => 9_723,
            Self::Data9 => 10_874,
            Self::DataA => 19_690,
            Self::DataB => 15_500,
            Self::DataC => 8_700,
            Self::DataD => 34_458,
            Self::DataE => 8_700,
            Self::DataF => 8_664,
            Self::DataG => 8_802,
            Self::DataH => 8_927,
        }
    }

    pub fn from_pointer_word(pointer: u16) -> Option<Self> {
        pointer
            .checked_sub(0x0f46)
            .filter(|offset| offset % 4 == 0)
            .and_then(|offset| Self::ALL.get(usize::from(offset / 4)).copied())
    }
}

#[derive(Debug)]
pub struct SoundAssets {
    samples: BTreeMap<Resource, Vec<u8>>,
}

impl SoundAssets {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, Error> {
        let mut samples = BTreeMap::new();
        for resource in Resource::ALL {
            let bytes = read_resource(directory.as_ref(), resource.name())?;
            if bytes.len() != resource.bytes() {
                return Err(Error::InvalidData(format!(
                    "{}: expected {} raw sample bytes, got {}",
                    resource.name(),
                    resource.bytes(),
                    bytes.len()
                )));
            }
            samples.insert(resource, bytes);
        }
        Ok(Self { samples })
    }

    /// Original amplitudes remain eight-bit; lookup happens only at a PIT write.
    pub fn sample(&self, resource: Resource) -> &[u8] {
        &self.samples[&resource]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteOrigin {
    Asset,
    ExternalMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryByte {
    pub value: u8,
    pub origin: ByteOrigin,
}

/// A source reader may also resolve the original routine's allocation-boundary
/// reads. It must fail if those bytes are unknown, instead of inventing padding.
pub trait SampleMemory {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, Error>;
}

impl SampleMemory for SoundAssets {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, Error> {
        self.sample(resource)
            .get(usize::from(offset))
            .copied()
            .map(|value| MemoryByte {
                value,
                origin: ByteOrigin::Asset,
            })
            .ok_or_else(|| {
                Error::InvalidData(format!(
                    "unknown source byte {}:{offset}; outside asset",
                    resource.name()
                ))
            })
    }
}

/// Context supplied by the engine or an explicitly identified diagnostic run.
/// External bytes never override bytes that actually belong to an asset.
pub struct SampleBytes<'a> {
    assets: &'a SoundAssets,
    external: BTreeMap<(Resource, u16), u8>,
}

impl<'a> SampleBytes<'a> {
    pub fn new(assets: &'a SoundAssets) -> Self {
        Self {
            assets,
            external: BTreeMap::new(),
        }
    }

    pub fn bind_external_byte(
        &mut self,
        resource: Resource,
        offset: u16,
        value: u8,
    ) -> Result<(), Error> {
        if usize::from(offset) < self.assets.sample(resource).len() {
            return Err(Error::InvalidData(
                "external memory cannot replace sample asset bytes".into(),
            ));
        }
        self.external.insert((resource, offset), value);
        Ok(())
    }
}

impl SampleMemory for SampleBytes<'_> {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, Error> {
        if let Some(&value) = self.assets.sample(resource).get(usize::from(offset)) {
            return Ok(MemoryByte {
                value,
                origin: ByteOrigin::Asset,
            });
        }
        self.external
            .get(&(resource, offset))
            .copied()
            .map(|value| MemoryByte {
                value,
                origin: ByteOrigin::ExternalMemory,
            })
            .ok_or_else(|| {
                Error::InvalidData(format!(
                    "unknown allocation byte {}:{offset}",
                    resource.name()
                ))
            })
    }
}
