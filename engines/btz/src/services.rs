// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Verified native service data and algorithms with explicit platform requests.

use crate::{Error, image::dos_data, media::TextStores};

#[derive(Clone, Debug)]
pub enum HostReply {
    Done,
    Cancelled,
    Name(Vec<u8>),
    Bytes(Vec<u8>),
    SideMap(SideMap),
    Mounted {
        stores: TextStores,
        side_map: SideMap,
    },
}

/// Platform work is requested explicitly; original DOS device names are not
/// resolved by the engine, and no file or drive is opened by these models.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostRequest {
    Exit {
        code: u8,
    },
    SaveBookmark {
        data: Vec<u8>,
    },
    LoadBookmark,
    TranscriptName {
        prompt: Vec<u8>,
        max_bytes: usize,
    },
    OpenTranscript {
        path: Vec<u8>,
    },
    WriteTranscript {
        bytes: Vec<u8>,
    },
    CloseTranscript,
    ReadSideMap,
    MountDisk {
        logical_slot: u16,
        physical_disk: u16,
        drive: u8,
        prompt: Vec<u8>,
    },
}

/// Text remains in the supplied executable rather than compiled into the engine.
#[derive(Clone, Debug, Default)]
pub struct ServiceMessages {
    pub printer_prompt: Vec<u8>,
    pub printer_error: Vec<u8>,
    pub disk_prompt: Vec<u8>,
    pub wrong_disk: Vec<u8>,
}
impl ServiceMessages {
    /// Reads the investigated DOS offsets; this does not identify revisions.
    pub fn from_dos_executable(exe: &[u8]) -> Result<Self, Error> {
        Self::from_dos_resources(exe, crate::interpreter::MINDWHEEL_RESOURCES)
    }

    pub fn from_dos_resources(
        exe: &[u8],
        layout: crate::interpreter::DosResources,
    ) -> Result<Self, Error> {
        let data = dos_data(exe)?;
        Ok(Self {
            printer_prompt: c_string(data, layout.service_strings[0])?,
            printer_error: c_string(data, layout.service_strings[1])?,
            disk_prompt: c_string(data, layout.service_strings[2])?,
            wrong_disk: c_string(data, layout.service_strings[3])?,
        })
    }
    pub fn transcript_name_request(&self) -> HostRequest {
        // The DOS branch calls fgets(buffer,29,stdin), including its C terminator.
        HostRequest::TranscriptName {
            prompt: self.printer_prompt.clone(),
            max_bytes: 28,
        }
    }
    pub fn mount_request(&self, slot: u16, number: u16, drive: u8) -> HostRequest {
        let value = (number as i16).to_string().into_bytes();
        let mut prompt = Vec::new();
        let mut position = 0;
        while position < self.disk_prompt.len() {
            if self.disk_prompt[position..].starts_with(b"%d") {
                prompt.extend_from_slice(&value);
                position += 2;
            } else {
                prompt.push(self.disk_prompt[position]);
                position += 1;
            }
        }
        HostRequest::MountDisk {
            logical_slot: slot,
            physical_disk: number,
            drive,
            prompt,
        }
    }
}
fn c_string(data: &[u8], offset: usize) -> Result<Vec<u8>, Error> {
    let bytes = data
        .get(offset..)
        .ok_or_else(|| Error("native service string outside executable".into()))?;
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error("unterminated native service string".into()))?;
    Ok(bytes[..end].to_vec())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SideEntry {
    /// Zero means present; any other word requests the corresponding program disk.
    pub physical_disk: u16,
    pub drive: u8,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SideMap {
    pub entries: [SideEntry; 10],
    /// DS:1f11 is loaded after the ten pairs; its role is not established here.
    pub trailing_value: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiskSelection {
    Ready {
        drive_changed: bool,
        invalidated_stores: Vec<u8>,
    },
    RequestMedia {
        logical_slot: u16,
        physical_disk: u16,
        drive: u8,
    },
}
impl SideMap {
    /// The native loader uses fgets(...,6,...): at most five raw bytes per
    /// chunk, CR discarded but counted, LF retained. Missing fields are checked
    /// instead of reading the native function's stale temporary buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut position = 0;
        let mut entries = [SideEntry {
            physical_disk: 0xffff,
            drive: 0,
        }; 10];
        for entry in &mut entries {
            let number = field(bytes, &mut position)?;
            entry.physical_disk = if number.first().is_some_and(u8::is_ascii_digit) {
                atoi(&number)
            } else {
                0xffff
            };
            let drive = field(bytes, &mut position)?;
            entry.drive = drive[0];
        }
        let trailing_value = atoi(&field(bytes, &mut position)?);
        Ok(Self {
            entries,
            trailing_value,
        })
    }
    /// Call again with reloaded mapping after mounting the requested medium.
    /// Keep the old slot index until Ready, as the native function does.
    pub fn selection(&self, previous: u16, target: u16) -> Result<DiskSelection, Error> {
        let previous = self
            .entries
            .get(usize::from(previous))
            .ok_or_else(|| Error("previous logical disk outside side map".into()))?;
        let entry = self
            .entries
            .get(usize::from(target))
            .ok_or_else(|| Error("logical disk outside side map".into()))?;
        if entry.physical_disk != 0 {
            return Ok(DiskSelection::RequestMedia {
                logical_slot: target,
                physical_disk: entry.physical_disk,
                drive: entry.drive,
            });
        }
        let drive_changed = entry.drive != previous.drive;
        Ok(DiskSelection::Ready {
            drive_changed,
            invalidated_stores: if drive_changed {
                vec![3, 2, 4]
            } else {
                Vec::new()
            },
        })
    }
}
fn field(bytes: &[u8], position: &mut usize) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    for _ in 0..5 {
        let Some(&byte) = bytes.get(*position) else {
            break;
        };
        *position += 1;
        if byte == 0x1a {
            break;
        }
        if byte != 13 {
            output.push(byte);
        }
        if byte == 10 {
            break;
        }
    }
    if output.is_empty() {
        return Err(Error("missing native side-map field".into()));
    }
    Ok(output)
}
fn atoi(bytes: &[u8]) -> u16 {
    let mut position = 0;
    while bytes
        .get(position)
        .is_some_and(|b| matches!(b, b' ' | b'\t'))
    {
        position += 1;
    }
    let negative = bytes.get(position) == Some(&b'-');
    if negative {
        position += 1;
    }
    let mut value = 0u16;
    while let Some(byte) = bytes.get(position).filter(|b| b.is_ascii_digit()) {
        value = value.wrapping_mul(10).wrapping_add(u16::from(*byte - b'0'));
        position += 1;
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// Hooks correspond to the native formatter's transcript operations. Append
/// receives ordinary output bytes only; CR, LF and NUL take other native paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptEvent {
    Append(u8),
    MarkSpace,
    LineBreak,
    FlushRemainder,
}
#[derive(Clone, Debug, Default)]
pub struct TranscriptBuffer {
    enabled: bool,
    bytes: Vec<u8>,
    last_space: usize,
}
impl TranscriptBuffer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// Native open/close does not flush or clear the logical partial line.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    pub fn pending_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn last_space(&self) -> usize {
        self.last_space
    }

    /// Returns bytes for the host to write, already translated from LF to DOS
    /// CRLF. I/O errors remain host results; this method never performs I/O.
    pub fn record(&mut self, event: TranscriptEvent) -> Result<Option<Vec<u8>>, Error> {
        match event {
            TranscriptEvent::Append(byte) => {
                if matches!(byte, 0 | 10 | 13) {
                    return Err(Error("nonordinary byte passed to transcript append".into()));
                }
                // DS:d32..dd1 has160 bytes; reserve LF and terminating C NUL.
                if self.bytes.len() >= 158 {
                    return Err(Error("native transcript buffer exhausted".into()));
                }
                self.bytes.push(byte);
                Ok(None)
            }
            TranscriptEvent::MarkSpace => {
                self.last_space = self.bytes.len();
                Ok(None)
            }
            TranscriptEvent::LineBreak | TranscriptEvent::FlushRemainder => {
                let end = if event == TranscriptEvent::LineBreak {
                    self.last_space
                } else {
                    self.bytes.len()
                };
                let output = if self.enabled {
                    let mut output = self.bytes[..end].to_vec();
                    output.extend_from_slice(b"\r\n");
                    Some(output)
                } else {
                    None
                };
                self.bytes.clear();
                self.last_space = 0;
                Ok(output)
            }
        }
    }
}
