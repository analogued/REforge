// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original PC speaker event selection, independent of PIT execution and PCM.
//! Note divisors remain in the external driver; no driver instructions execute.

use crate::assets::{Assets, Result, word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortWrite {
    pub tick: u64,
    pub port: u16,
    pub value: u8,
}

pub struct Sequencer {
    driver: Vec<u8>,
    song: Vec<u8>,
    start: usize,
    position: usize,
    pub tick: u64,
    wait: u8,
    command: u8,
    channel: u8,
    selected_channel: u8,
    transpose: u8,
    ignored: bool,
    two_bytes: bool,
    enabled: bool,
    control: u8,
    playing: bool,
    writes: Vec<PortWrite>,
}

impl Sequencer {
    pub fn song(assets: &Assets, id: u8) -> Result<Self> {
        Self::new(
            assets.resource("CENT", "SPEAKER.LZW")?,
            crate::music::song_data(assets, id)?,
        )
    }

    pub fn new(driver: Vec<u8>, song: Vec<u8>) -> Result<Self> {
        if driver.len() != 0x2da {
            return Err("Unexpected PC speaker driver length".into());
        }
        let (start, selected_channel, transpose) = header(&song)?;
        let mut result = Self {
            command: driver[0xe0],
            channel: driver[0xe1],
            ignored: driver[0xe3] != 0,
            two_bytes: driver[0xe4] != 0,
            enabled: driver[0xe8] == 1,
            driver,
            song,
            start,
            position: start,
            tick: 0,
            wait: 0,
            selected_channel,
            transpose,
            control: 0,
            playing: true,
            writes: Vec::new(),
        };
        result.initialize();
        Ok(result)
    }

    fn write(&mut self, port: u16, value: u8) {
        // Only the speaker-control bits are modeled here. Refresh, keyboard and
        // other machine-specific port-61 inputs are not synthesizer controls.
        if port == 0x61 {
            self.control = value & 3;
        }
        self.writes.push(PortWrite {
            tick: self.tick,
            port,
            value,
        });
    }

    fn initialize(&mut self) {
        // Driver 010E..0165 toggles both speaker bits, disables them, then loads
        // channel two in mode three. Repeated writes retain their ordering.
        self.write(0x61, self.control | 3);
        self.silence();
        self.write(0x43, 0xb6);
        self.write(0x42, 2);
        self.write(0x42, 0);
    }

    fn silence(&mut self) {
        self.write(0x61, self.control & !3);
    }

    pub fn take_writes(&mut self) -> Vec<PortWrite> {
        std::mem::take(&mut self.writes)
    }

    pub fn restart(&mut self, song: Vec<u8>) -> Result<()> {
        let (start, selected_channel, transpose) = header(&song)?;
        self.song = song;
        self.start = start;
        self.position = start;
        self.selected_channel = selected_channel;
        self.transpose = transpose;
        self.wait = 0;
        self.playing = true;
        // The original initializer does not clear command/channel or the two
        // running-status skip flags, including after interruptions or EOF.
        self.initialize();
        Ok(())
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.silence();
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    fn byte(&mut self) -> Result<u8> {
        let value = *self
            .song
            .get(self.position)
            .ok_or("Truncated PC speaker music event")?;
        self.position += 1;
        Ok(value)
    }

    fn skip(&mut self, length: usize) -> Result<()> {
        for _ in 0..length {
            self.byte()?;
        }
        Ok(())
    }

    fn note(&mut self, note: u8, velocity: u8) -> Result<()> {
        if self.channel != self.selected_channel {
            return Ok(());
        }
        // Driver 02B9 does not require a matching active pitch: any note-off
        // on the selected channel disables the speaker, including velocity zero.
        if self.command == 0x80 || velocity == 0 || !self.enabled {
            self.silence();
            return Ok(());
        }
        // Both the transposition and doubled table index wrap in BL, not BX.
        let index = note.wrapping_add(self.transpose).wrapping_mul(2);
        let divisor = if index == 0xc0 {
            // The intro reaches one word beyond the 96-entry pitch table.
            // Driver initialization overwrites that adjacent D4/D5 pair with
            // this song's selected channel and transposition, so reading the
            // untouched driver bytes would produce the wrong PIT divisor.
            u16::from_le_bytes([self.selected_channel, self.transpose])
        } else {
            word(&self.driver, 0x14 + usize::from(index))?
        };
        self.write(0x61, self.control | 3);
        self.write(0x42, divisor as u8);
        self.write(0x42, (divisor >> 8) as u8);
        Ok(())
    }

    pub fn advance(&mut self) -> Result<()> {
        self.tick += 1;
        if !self.playing {
            return Ok(());
        }
        if self.wait != 0 {
            self.wait -= 1;
            return Ok(());
        }
        // Valid original streams have finite work per tick. Reject malformed
        // zero-delay loops rather than hanging the audio callback.
        for _ in 0..65536 {
            let first = self.byte()?;
            let skip_running = if first & 0x80 != 0 {
                if first == 0xfc {
                    if self.byte()? == 0x80 {
                        self.position = self.start;
                        continue;
                    }
                    self.stop();
                    return Ok(());
                }
                self.command = first & 0xf0;
                self.channel = first & 15;
                false
            } else {
                self.position -= 1;
                self.ignored
            };
            if skip_running {
                self.skip(if self.two_bytes { 2 } else { 1 })?;
            } else if self.command == 0xe0 {
                self.ignored = true;
                self.two_bytes = true;
                self.skip(2)?;
            } else {
                self.ignored = false;
                self.two_bytes = self.command != 0xc0;
                match self.command {
                    0xc0 => self.skip(1)?,
                    0x80 | 0x90 => {
                        let note = self.byte()?;
                        let velocity = self.byte()?;
                        self.note(note, velocity)?;
                    }
                    _ => {
                        self.skip(2)?;
                        self.ignored = true;
                    }
                }
            }
            let delay = self.byte()?;
            if delay != 0 {
                self.wait = delay - 1;
                return Ok(());
            }
        }
        Err("PC speaker music exceeded per-tick event limit".into())
    }
}

fn header(song: &[u8]) -> Result<(usize, u8, u8)> {
    if song.len() < 0x4d {
        return Err("Truncated PC speaker music header".into());
    }
    let start = usize::from(word(song, 0)?);
    if start < 0x4d || start >= song.len() {
        return Err("Invalid PC speaker music event offset".into());
    }
    Ok((start, song[0x4b], song[0x4c]))
}

#[cfg(test)]
pub(crate) fn fixture_driver() -> Vec<u8> {
    let mut driver = vec![0; 0x2da];
    driver[0xe8] = 1;
    driver[0x14 + 60 * 2..0x16 + 60 * 2].copy_from_slice(&0x1234_u16.to_le_bytes());
    driver
}

#[cfg(test)]
pub(crate) fn fixture_song(events: &[u8]) -> Vec<u8> {
    let mut song = vec![0; 0x4d];
    song[..2].copy_from_slice(&0x4d_u16.to_le_bytes());
    song.extend_from_slice(events);
    song
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(events: &[u8]) -> Sequencer {
        Sequencer::new(fixture_driver(), fixture_song(events)).unwrap()
    }

    fn writes(sequence: &mut Sequencer) -> Vec<(u64, u16, u8)> {
        sequence
            .take_writes()
            .into_iter()
            .map(|w| (w.tick, w.port, w.value))
            .collect()
    }

    #[test]
    fn initialization_running_status_delays_and_loop_keep_ordered_writes() {
        let mut sequence = fixture(&[0x90, 60, 100, 2, 60, 0, 1, 0xfc, 0x80]);
        assert_eq!(
            writes(&mut sequence),
            [
                (0, 0x61, 3),
                (0, 0x61, 0),
                (0, 0x43, 0xb6),
                (0, 0x42, 2),
                (0, 0x42, 0)
            ]
        );
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(1, 0x61, 3), (1, 0x42, 0x34), (1, 0x42, 0x12)]
        );
        sequence.advance().unwrap();
        assert!(writes(&mut sequence).is_empty());
        sequence.advance().unwrap();
        assert_eq!(writes(&mut sequence), [(3, 0x61, 0)]);
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(4, 0x61, 3), (4, 0x42, 0x34), (4, 0x42, 0x12)]
        );
        assert!(sequence.is_playing());
    }

    #[test]
    fn channel_selection_transposition_wrap_and_unmatched_pitch_note_off() {
        let mut driver = fixture_driver();
        let index = 120_u8.wrapping_add(200).wrapping_mul(2);
        let at = 0x14 + usize::from(index);
        driver[at..at + 2].copy_from_slice(&0xabcd_u16.to_le_bytes());
        let mut song =
            fixture_song(&[0x91, 120, 127, 0, 0x92, 120, 1, 1, 0x82, 3, 127, 1, 0xfc, 0]);
        song[0x4b] = 2;
        song[0x4c] = 200;
        let mut sequence = Sequencer::new(driver, song).unwrap();
        sequence.take_writes();
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(1, 0x61, 3), (1, 0x42, 0xcd), (1, 0x42, 0xab)]
        );
        sequence.advance().unwrap();
        assert_eq!(writes(&mut sequence), [(2, 0x61, 0)]);
        sequence.advance().unwrap();
        assert_eq!(writes(&mut sequence), [(3, 0x61, 0)]);
        assert!(!sequence.is_playing());
    }

    #[test]
    fn pitch_table_overrun_reads_the_current_songs_channel_and_transposition() {
        let mut driver = fixture_driver();
        driver[0xd4..0xd6].copy_from_slice(&0xbeef_u16.to_le_bytes());
        let mut song = fixture_song(&[0x92, 95, 127, 1, 0xfc, 0]);
        song[0x4b] = 2;
        song[0x4c] = 1;
        let mut sequence = Sequencer::new(driver, song).unwrap();
        sequence.take_writes();
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(1, 0x61, 3), (1, 0x42, 2), (1, 0x42, 1)]
        );
        let mut song = fixture_song(&[0x93, 96, 127, 1, 0xfc, 0]);
        song[0x4b] = 3;
        sequence.restart(song).unwrap();
        sequence.take_writes();
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(2, 0x61, 3), (2, 0x42, 3), (2, 0x42, 0)]
        );
    }

    #[test]
    fn programs_pitch_bends_and_other_channels_do_not_create_voices() {
        let mut sequence = fixture(&[
            0xb0, 1, 2, 0, 3, 4, 0, 0xc0, 6, 0, 7, 0, 0xe0, 0, 64, 0, 1, 32, 0, 0x91, 60, 127, 0,
            0x81, 60, 0, 0, 0x90, 60, 127, 1, 0xfc, 0,
        ]);
        sequence.take_writes();
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(1, 0x61, 3), (1, 0x42, 0x34), (1, 0x42, 0x12)]
        );
        sequence.advance().unwrap();
        assert_eq!(writes(&mut sequence), [(2, 0x61, 0)]);
        assert!(!sequence.is_playing());
    }

    #[test]
    fn restart_preserves_running_status_and_resets_delay_without_resetting_clock() {
        let mut sequence = fixture(&[0x90, 60, 100, 20, 0xfc, 0]);
        sequence.take_writes();
        sequence.advance().unwrap();
        sequence.take_writes();
        sequence.stop();
        assert_eq!(writes(&mut sequence), [(1, 0x61, 0)]);
        sequence.advance().unwrap();
        assert_eq!(sequence.tick, 2);
        assert!(writes(&mut sequence).is_empty());
        sequence
            .restart(fixture_song(&[60, 0, 1, 0xfc, 0]))
            .unwrap();
        let reset = writes(&mut sequence);
        assert!(reset.iter().all(|w| w.0 == 2));
        assert_eq!(reset.len(), 5);
        sequence.advance().unwrap();
        assert_eq!(writes(&mut sequence), [(3, 0x61, 0)]);
        assert!(sequence.is_playing());
    }

    #[test]
    fn restart_retains_ignored_running_status_and_new_header_channel() {
        let mut sequence = fixture(&[0xb0, 1, 2, 20, 0xfc, 0]);
        sequence.take_writes();
        sequence.advance().unwrap();
        assert!(writes(&mut sequence).is_empty());
        let mut song = fixture_song(&[3, 4, 0, 0x91, 60, 127, 1, 0xfc, 0]);
        song[0x4b] = 1;
        sequence.restart(song).unwrap();
        sequence.take_writes();
        sequence.advance().unwrap();
        assert_eq!(
            writes(&mut sequence),
            [(2, 0x61, 3), (2, 0x42, 0x34), (2, 0x42, 0x12)]
        );
    }

    #[test]
    fn invalid_headers_events_and_zero_delay_loops_fail() {
        assert!(Sequencer::new(vec![0; 10], fixture_song(&[0xfc, 0])).is_err());
        assert!(Sequencer::new(fixture_driver(), vec![0; 20]).is_err());
        let mut sequence = fixture(&[0xfc, 0]);
        let mut invalid = fixture_song(&[0xfc, 0]);
        invalid[..2].copy_from_slice(&2_u16.to_le_bytes());
        assert!(sequence.restart(invalid).is_err());
        assert!(sequence.is_playing());
        let mut sequence = fixture(&[0x90, 60]);
        assert!(sequence.advance().is_err());
        let mut sequence = fixture(&[0xfc]);
        assert!(sequence.advance().is_err());
        let mut sequence = fixture(&[0xfc, 0x80]);
        assert!(sequence.advance().is_err());
    }
}
