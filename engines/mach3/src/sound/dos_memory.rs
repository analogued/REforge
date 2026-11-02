// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The surviving DOS allocation-name bytes reached by DATAB reverse playback.

use super::{ByteOrigin, MemoryByte, Resource, SampleMemory, SoundAssets};
use crate::Error;

/// Sample memory with an explicit DOS allocation-name bootstrap context.
///
/// The original allocator at `5c2b` requests `byte_count >> 4` paragraphs.
/// DATAB's 15,500-byte load therefore overruns its 15,488-byte allocation,
/// overwriting bytes 0 through 11 of the following DATAC MCB. Reverse playback
/// starts at offset 15,500: MCB byte 12, or byte 4 of its eight-byte owner name.
///
/// This models the DOS allocation policy that propagates the PSP MCB owner name
/// to the adjacent DATAC allocation. The caller supplies the actual eight-byte
/// name field, including padding; it is not inferred from the sample assets.
/// It establishes only the surviving name suffix, not a general DOS heap or an
/// allocation policy shared by every DOS environment. Unknown bytes still fail.
///
/// The modeled name propagation and layout can be independently checked in
/// DOSBox-X revision `478c860050b20afdb58bce28f1b43163952fc541`:
/// [EXEC naming](https://github.com/joncampbell123/dosbox-x/blob/478c860050b20afdb58bce28f1b43163952fc541/src/dos/dos_execute.cpp#L897),
/// [allocation naming](https://github.com/joncampbell123/dosbox-x/blob/478c860050b20afdb58bce28f1b43163952fc541/src/dos/dos_memory.cpp#L231),
/// and [MCB layout](https://github.com/joncampbell123/dosbox-x/blob/478c860050b20afdb58bce28f1b43163952fc541/include/dos_mcb.h#L68).
pub struct DosSampleMemory<'a> {
    assets: &'a SoundAssets,
    program_name: [u8; 8],
}

impl<'a> DosSampleMemory<'a> {
    pub fn new(assets: &'a SoundAssets, program_name: [u8; 8]) -> Self {
        Self {
            assets,
            program_name,
        }
    }
}

impl SampleMemory for DosSampleMemory<'_> {
    fn read(&self, resource: Resource, offset: u16) -> Result<MemoryByte, Error> {
        let asset = self.assets.sample(resource);
        if let Some(&value) = asset.get(usize::from(offset)) {
            return Ok(MemoryByte {
                value,
                origin: ByteOrigin::Asset,
            });
        }
        // Only DATAB's following allocation and preserved name suffix are
        // established. Do not synthesize header fields or other heap contents.
        let allocated_bytes = Resource::DataB.bytes() & !15;
        if resource == Resource::DataB {
            let mcb_offset = usize::from(offset).checked_sub(allocated_bytes);
            if let Some(mcb_offset @ 12..=15) = mcb_offset {
                return Ok(MemoryByte {
                    value: self.program_name[mcb_offset - 8],
                    origin: ByteOrigin::ExternalMemory,
                });
            }
        }
        Err(Error::InvalidData(format!(
            "unknown DOS allocation byte {}:{offset}",
            resource.name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "mach3-dos-sample-memory-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            for resource in Resource::ALL {
                fs::write(path.join(resource.name()), vec![0xa5; resource.bytes()]).unwrap();
            }
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn resolves_surviving_name_suffix_and_preserves_asset_bytes() {
        let fixture = Fixture::new();
        let assets = SoundAssets::load(&fixture.0).unwrap();
        let memory = DosSampleMemory::new(&assets, *b"MACH3\0\0\0");
        assert_eq!(
            memory.read(Resource::DataB, 15_500).unwrap(),
            MemoryByte {
                value: b'3',
                origin: ByteOrigin::ExternalMemory,
            }
        );
        for offset in 15_501..=15_503 {
            assert_eq!(memory.read(Resource::DataB, offset).unwrap().value, 0);
        }
        assert_eq!(
            memory.read(Resource::DataB, 15_499).unwrap(),
            MemoryByte {
                value: 0xa5,
                origin: ByteOrigin::Asset,
            }
        );
        assert_eq!(
            memory.read(Resource::DataC, 0).unwrap().origin,
            ByteOrigin::Asset
        );
    }

    #[test]
    fn caller_name_context_determines_boundary_value() {
        let fixture = Fixture::new();
        let assets = SoundAssets::load(&fixture.0).unwrap();
        let memory = DosSampleMemory::new(&assets, *b"GAMEZ   ");
        assert_eq!(memory.read(Resource::DataB, 15_500).unwrap().value, b'Z');
        assert_eq!(memory.read(Resource::DataB, 15_501).unwrap().value, b' ');
    }

    #[test]
    fn rejects_unestablished_external_memory() {
        let fixture = Fixture::new();
        let assets = SoundAssets::load(&fixture.0).unwrap();
        let memory = DosSampleMemory::new(&assets, *b"MACH3\0\0\0");
        assert!(memory.read(Resource::DataB, 15_504).is_err());
        assert!(
            memory
                .read(Resource::DataC, Resource::DataC.bytes() as u16)
                .is_err()
        );
        assert!(
            memory
                .read(Resource::DataA, Resource::DataA.bytes() as u16)
                .is_err()
        );
    }
}
