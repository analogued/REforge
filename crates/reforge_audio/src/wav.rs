// Copyright (c) 2026 analogued. Project license has not been selected.
//! Streaming mono 16-bit PCM RIFF/WAVE output.

use std::io::{self, Seek, SeekFrom, Write};

const HEADER_BYTES: u64 = 44;
const MAX_DATA_BYTES: u64 = u32::MAX as u64 - 36;

/// Writes PCM in bounded blocks and patches RIFF lengths on explicit completion.
pub struct WavWriter<W: Write + Seek> {
    writer: W,
    start: u64,
    data_bytes: u64,
    failed: bool,
}

impl<W: Write + Seek> WavWriter<W> {
    pub fn new(mut writer: W, sample_rate: u32) -> io::Result<Self> {
        let byte_rate = sample_rate
            .checked_mul(2)
            .filter(|_| sample_rate != 0)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid WAV sample rate")
            })?;
        let start = writer.stream_position()?;
        start.checked_add(HEADER_BYTES).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "WAV stream position overflows")
        })?;
        let mut header = [0; HEADER_BYTES as usize];
        header[..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&36_u32.to_le_bytes());
        header[8..16].copy_from_slice(b"WAVEfmt ");
        header[16..20].copy_from_slice(&16_u32.to_le_bytes());
        header[20..22].copy_from_slice(&1_u16.to_le_bytes());
        header[22..24].copy_from_slice(&1_u16.to_le_bytes());
        header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        header[32..34].copy_from_slice(&2_u16.to_le_bytes());
        header[34..36].copy_from_slice(&16_u16.to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        writer.write_all(&header)?;
        Ok(Self {
            writer,
            start,
            data_bytes: 0,
            failed: false,
        })
    }

    pub fn write_samples(&mut self, samples: &[i16]) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("WAV sample write previously failed"));
        }
        let next_bytes = u64::try_from(samples.len())
            .ok()
            .and_then(|len| len.checked_mul(2))
            .and_then(|bytes| self.data_bytes.checked_add(bytes))
            .filter(|&bytes| bytes <= MAX_DATA_BYTES)
            .filter(|&bytes| self.start.checked_add(HEADER_BYTES + bytes).is_some())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "WAV exceeds RIFF size limit")
            })?;
        let mut bytes = [0_u8; 8192];
        for chunk in samples.chunks(bytes.len() / 2) {
            for (sample, encoded) in chunk.iter().zip(bytes.chunks_exact_mut(2)) {
                encoded.copy_from_slice(&sample.to_le_bytes());
            }
            if let Err(error) = self.writer.write_all(&bytes[..chunk.len() * 2]) {
                self.failed = true;
                return Err(error);
            }
        }
        self.data_bytes = next_bytes;
        Ok(())
    }

    /// Patches the header, flushes, and returns the stream at the WAV's end.
    pub fn finish(mut self) -> io::Result<W> {
        if self.failed {
            return Err(io::Error::other("WAV sample write previously failed"));
        }
        self.writer.seek(SeekFrom::Start(self.start + 4))?;
        self.writer
            .write_all(&((36 + self.data_bytes) as u32).to_le_bytes())?;
        self.writer.seek(SeekFrom::Start(self.start + 40))?;
        self.writer
            .write_all(&(self.data_bytes as u32).to_le_bytes())?;
        self.writer
            .seek(SeekFrom::Start(self.start + HEADER_BYTES + self.data_bytes))?;
        self.writer.flush()?;
        Ok(self.writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn riff_limit_is_checked_before_writing() {
        let mut wav = WavWriter::new(Cursor::new(Vec::new()), 48_000).unwrap();
        wav.data_bytes = MAX_DATA_BYTES - 1; // The largest even PCM data length.
        assert_eq!(
            wav.write_samples(&[1]).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(wav.writer.get_ref().len(), HEADER_BYTES as usize);
    }
}
