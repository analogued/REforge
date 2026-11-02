// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use reforge_audio::wav::WavWriter;
use std::io::{self, Cursor, Seek, SeekFrom, Write};

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn mono_pcm_header_and_signed_little_endian_samples_are_exact() {
    let mut wav = WavWriter::new(Cursor::new(Vec::new()), 48_000).unwrap();
    wav.write_samples(&[i16::MIN, -1]).unwrap();
    wav.write_samples(&[]).unwrap();
    wav.write_samples(&[0, i16::MAX]).unwrap();
    let cursor = wav.finish().unwrap();
    assert_eq!(cursor.position(), 52);
    let bytes = cursor.into_inner();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(word(&bytes, 4), 44);
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(word(&bytes, 16), 16);
    assert_eq!(&bytes[20..24], &[1, 0, 1, 0]);
    assert_eq!(word(&bytes, 24), 48_000);
    assert_eq!(word(&bytes, 28), 96_000);
    assert_eq!(&bytes[32..40], &[2, 0, 16, 0, b'd', b'a', b't', b'a']);
    assert_eq!(word(&bytes, 40), 8);
    assert_eq!(&bytes[44..], &[0, 128, 255, 255, 0, 0, 255, 127]);
}

#[test]
fn empty_output_and_nonzero_stream_start_have_correct_lengths() {
    let mut cursor = Cursor::new(vec![0xaa; 7]);
    cursor.set_position(7);
    let bytes = WavWriter::new(cursor, 8_000)
        .unwrap()
        .finish()
        .unwrap()
        .into_inner();
    assert_eq!(&bytes[..7], &[0xaa; 7]);
    assert_eq!(bytes.len(), 51);
    assert_eq!(word(&bytes, 11), 36);
    assert_eq!(word(&bytes, 47), 0);
    for rate in [0, u32::MAX] {
        assert!(WavWriter::new(Cursor::new(Vec::new()), rate).is_err());
    }
}

struct BoundedWrite {
    cursor: Cursor<Vec<u8>>,
    fail_data: bool,
    fail_seek: bool,
    fail_flush: bool,
}

impl Write for BoundedWrite {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert!(bytes.len() <= 8192);
        if self.fail_data && self.cursor.position() >= 44 {
            return Err(io::Error::other("data write failed"));
        }
        self.cursor.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::other("flush failed"))
        } else {
            Ok(())
        }
    }
}

impl Seek for BoundedWrite {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if self.fail_seek {
            Err(io::Error::other("seek failed"))
        } else {
            self.cursor.seek(position)
        }
    }
}

fn output(fail_data: bool, fail_seek: bool, fail_flush: bool) -> BoundedWrite {
    BoundedWrite {
        cursor: Cursor::new(Vec::new()),
        fail_data,
        fail_seek,
        fail_flush,
    }
}

#[test]
fn large_inputs_use_bounded_writes() {
    let mut wav = WavWriter::new(output(false, false, false), 44_100).unwrap();
    wav.write_samples(&vec![1234; 20_000]).unwrap();
    let bytes = wav.finish().unwrap().cursor.into_inner();
    assert_eq!(bytes.len(), 40_044);
    assert_eq!(word(&bytes, 40), 40_000);
}

#[test]
fn stream_errors_propagate_and_failed_pcm_cannot_finish() {
    assert!(WavWriter::new(output(false, true, false), 48_000).is_err());
    let mut wav = WavWriter::new(output(true, false, false), 48_000).unwrap();
    assert!(wav.write_samples(&[1]).is_err());
    assert!(wav.write_samples(&[]).is_err());
    assert!(wav.finish().is_err());
    assert!(
        WavWriter::new(output(false, false, true), 48_000)
            .unwrap()
            .finish()
            .is_err()
    );
}
