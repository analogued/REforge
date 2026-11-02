// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use btz::services::{
    DiskSelection, HostRequest, ServiceMessages, SideEntry, SideMap, TranscriptBuffer,
    TranscriptEvent,
};

fn executable() -> Vec<u8> {
    let mut bytes = vec![0; 32 + 0x300];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
    for (offset, text) in [
        (0x23f, b"Device?\0".as_slice()),
        (0x18f, b"Bad device\0"),
        (0x270, b"Disk %d, please.\0"),
        (0x297, b"Wrong.\0"),
    ] {
        bytes[32 + offset..32 + offset + text.len()].copy_from_slice(text);
    }
    bytes
}
fn side_file(first: &[u8], trailing: &[u8]) -> Vec<u8> {
    let mut bytes = first.to_vec();
    for _ in 1..10 {
        bytes.extend_from_slice(b"0\na\n");
    }
    bytes.extend_from_slice(trailing);
    bytes
}
fn append(buffer: &mut TranscriptBuffer, bytes: &[u8]) {
    for &byte in bytes {
        assert!(
            buffer
                .record(TranscriptEvent::Append(byte))
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn service_requests_use_external_text_and_signed_disk_formatting() {
    let messages = ServiceMessages::from_dos_executable(&executable()).unwrap();
    assert_eq!(
        messages.transcript_name_request(),
        HostRequest::TranscriptName {
            prompt: b"Device?".to_vec(),
            max_bytes: 28
        }
    );
    assert_eq!(messages.printer_error, b"Bad device");
    assert_eq!(
        messages.mount_request(2, 0xffff, b'q'),
        HostRequest::MountDisk {
            logical_slot: 2,
            physical_disk: 0xffff,
            drive: b'q',
            prompt: b"Disk -1, please.".to_vec()
        }
    );
    assert!(ServiceMessages::from_dos_executable(b"MZ").is_err());
}
#[test]
fn side_map_reads_ten_pairs_and_retains_unknown_trailing_word() {
    let map = SideMap::from_bytes(&side_file(b"7\r\nq\r\n", b"\t-2\n")).unwrap();
    assert_eq!(
        map.entries[0],
        SideEntry {
            physical_disk: 7,
            drive: b'q'
        }
    );
    assert_eq!(
        map.entries[9],
        SideEntry {
            physical_disk: 0,
            drive: b'a'
        }
    );
    assert_eq!(map.trailing_value, 0xfffe);
    let map = SideMap::from_bytes(&side_file(b"-1\na\n", b"+3\n")).unwrap();
    assert_eq!(map.entries[0].physical_disk, 0xffff); // First byte must be a digit.
    assert_eq!(map.trailing_value, 0); // Native atoi has no plus-sign branch.
    assert!(SideMap::from_bytes(b"0\na\n").is_err());
}
#[test]
fn side_map_counts_raw_carriage_returns_in_its_five_byte_chunks() {
    // Four digits plus CR exhaust the first raw read. The following LF becomes
    // the first byte of the drive field, rather than being trimmed wholesale.
    let map = SideMap::from_bytes(&side_file(b"1234\r\n", b"99999\n")).unwrap();
    assert_eq!(map.entries[0].physical_disk, 1234);
    assert_eq!(map.entries[0].drive, 10);
    assert_eq!(map.trailing_value, 99999u32 as u16);
}
#[test]
fn selection_requests_missing_media_then_invalidates_only_text_stores_on_drive_change() {
    let mut map = SideMap {
        entries: [SideEntry {
            physical_disk: 0,
            drive: b'a',
        }; 10],
        trailing_value: 0,
    };
    assert_eq!(
        map.selection(0, 1).unwrap(),
        DiskSelection::Ready {
            drive_changed: false,
            invalidated_stores: vec![]
        }
    );
    map.entries[2] = SideEntry {
        physical_disk: 5,
        drive: b'b',
    };
    assert_eq!(
        map.selection(0, 2).unwrap(),
        DiskSelection::RequestMedia {
            logical_slot: 2,
            physical_disk: 5,
            drive: b'b'
        }
    );
    map.entries[2].physical_disk = 0;
    assert_eq!(
        map.selection(0, 2).unwrap(),
        DiskSelection::Ready {
            drive_changed: true,
            invalidated_stores: vec![3, 2, 4]
        }
    );
    assert!(map.selection(10, 0).is_err());
    assert!(map.selection(0, 10).is_err());
}
#[test]
fn transcript_wrap_flushes_to_last_space_then_replays_the_word() {
    let mut buffer = TranscriptBuffer::new();
    buffer.set_enabled(true);
    append(&mut buffer, b"two ");
    buffer.record(TranscriptEvent::MarkSpace).unwrap();
    append(&mut buffer, b"words");
    assert_eq!(
        buffer.record(TranscriptEvent::LineBreak).unwrap(),
        Some(b"two \r\n".to_vec())
    );
    assert_eq!(buffer.pending_bytes(), b"");
    assert_eq!(buffer.last_space(), 0);
    append(&mut buffer, b"words");
    assert_eq!(
        buffer.record(TranscriptEvent::FlushRemainder).unwrap(),
        Some(b"words\r\n".to_vec())
    );
    assert_eq!(
        buffer.record(TranscriptEvent::FlushRemainder).unwrap(),
        Some(b"\r\n".to_vec())
    );
}
#[test]
fn disabled_transcript_still_clears_buffer_at_flush_and_close_does_not_flush() {
    let mut buffer = TranscriptBuffer::new();
    append(&mut buffer, b"hidden");
    buffer.record(TranscriptEvent::MarkSpace).unwrap();
    assert_eq!(buffer.record(TranscriptEvent::LineBreak).unwrap(), None);
    assert!(buffer.pending_bytes().is_empty());
    buffer.set_enabled(true);
    append(&mut buffer, b"partial");
    buffer.set_enabled(false);
    assert_eq!(buffer.pending_bytes(), b"partial");
    buffer.set_enabled(true);
    assert_eq!(
        buffer.record(TranscriptEvent::FlushRemainder).unwrap(),
        Some(b"partial\r\n".to_vec())
    );
}
#[test]
fn transcript_checks_append_contract_and_reserves_lf_and_nul_capacity() {
    let mut buffer = TranscriptBuffer::new();
    buffer.set_enabled(true);
    for byte in [0, 10, 13] {
        assert!(buffer.record(TranscriptEvent::Append(byte)).is_err());
    }
    append(&mut buffer, &[b'x'; 158]);
    assert!(buffer.record(TranscriptEvent::Append(b'x')).is_err());
    assert_eq!(buffer.pending_bytes().len(), 158);
    assert_eq!(
        buffer
            .record(TranscriptEvent::FlushRemainder)
            .unwrap()
            .unwrap()
            .len(),
        160
    );
}
