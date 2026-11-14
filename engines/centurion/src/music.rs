// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Centurion's tick sequencer and AdLib voice allocation, independent of synthesis.
//! Tables remain external original data; driver instructions are never executed.
use crate::assets::{Assets, Result, word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterWrite {
    pub tick: u64,
    pub register: u8,
    pub value: u8,
}

pub struct Sequencer {
    driver: Vec<u8>,
    song: Vec<u8>,
    start: usize,
    position: usize,
    pub tick: u64,
    wait: u8,
    status: u8,
    ignored: bool,
    two_bytes: bool,
    playing: bool,
    channel: [u8; 9],
    active: [bool; 9],
    note: [u8; 9],
    transpose: [u8; 9],
    level: [u8; 9],
    frequency: [u16; 9],
    age: [u16; 9],
    writes: Vec<RegisterWrite>,
}

impl Sequencer {
    pub fn intro(assets: &Assets) -> Result<Self> {
        Self::song(assets, 0)
    }

    pub fn song(assets: &Assets, id: u8) -> Result<Self> {
        Self::new(
            assets.resource("CENT", "ADLIB.LZW")?,
            song_data(assets, id)?,
        )
    }

    fn new(driver: Vec<u8>, song: Vec<u8>) -> Result<Self> {
        if driver.len() != 0x97a || song.len() < 0x2b {
            return Err("Unexpected AdLib driver or song header".into());
        }
        let start = word(&song, 0)? as usize;
        if start < 0x2b || start >= song.len() {
            return Err("Invalid music event offset".into());
        }
        let mut channel = [0; 9];
        channel.copy_from_slice(&song[0x22..0x2b]);
        let mut result = Self {
            driver,
            song,
            start,
            position: start,
            tick: 0,
            wait: 0,
            status: 0,
            ignored: false,
            two_bytes: false,
            playing: true,
            channel,
            active: [false; 9],
            note: [0; 9],
            transpose: [0; 9],
            level: [0; 9],
            frequency: [0; 9],
            age: [0; 9],
            writes: Vec::new(),
        };
        result.silence();
        for voice in 0..9 {
            if result.channel[voice] == 9 {
                result.instrument(voice, result.driver[0x392 + voice])?;
            }
        }
        Ok(result)
    }

    fn write(&mut self, register: u8, value: u8) {
        self.writes.push(RegisterWrite {
            tick: self.tick,
            register,
            value,
        });
    }

    fn silence(&mut self) {
        // Driver 0929: order matters, including overlapping clear/mute tables.
        self.write(1, 0x20);
        self.write(4, 0xe0);
        for index in (0..=100).rev() {
            self.write(self.driver[0xd6 + index], 0);
        }
        for index in (0..18).rev() {
            self.write(self.driver[0x13b + index], 0x3f);
        }
    }

    pub fn take_writes(&mut self) -> Vec<RegisterWrite> {
        std::mem::take(&mut self.writes)
    }

    pub fn restart(&mut self, song: Vec<u8>) -> Result<()> {
        if song.len() < 0x2b {
            return Err("Truncated music header".into());
        }
        let start = usize::from(word(&song, 0)?);
        if start < 0x2b || start >= song.len() {
            return Err("Invalid music event offset".into());
        }
        self.channel.copy_from_slice(&song[0x22..0x2b]);
        self.song = song;
        self.start = start;
        self.position = start;
        self.wait = 0;
        self.playing = true;
        self.active.fill(false);
        self.note.fill(0);
        self.age.fill(0);
        self.silence();
        for voice in 0..9 {
            if self.channel[voice] == 9 {
                self.instrument(voice, self.driver[0x392 + voice])?;
            }
        }
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
            .ok_or("Truncated music event")?;
        self.position += 1;
        Ok(value)
    }

    fn instrument(&mut self, voice: usize, program: u8) -> Result<()> {
        // Driver 07fb: program remap -> instrument pointer -> 12-byte definition.
        let mapped = *self
            .driver
            .get(0x14f + program as usize)
            .ok_or("Invalid music program")?;
        let address = word(&self.driver, 0x167 + mapped as usize * 2)? as usize;
        let definition: [u8; 12] = self
            .driver
            .get(address..address + 12)
            .ok_or("Invalid instrument definition")?
            .try_into()
            .unwrap();
        let modulator = self.driver[0x34a + voice];
        let carrier = self.driver[0x353 + voice];
        for (offset, value) in [0x20, 0x40, 0x60, 0x80].into_iter().zip(&definition[..4]) {
            self.write(offset + modulator, *value);
        }
        for (offset, value) in [0x20, 0x40, 0x60, 0x80].into_iter().zip(&definition[4..8]) {
            self.write(offset + carrier, *value);
        }
        self.level[voice] = definition[5];
        self.write(0xc0 + voice as u8, definition[8]);
        self.transpose[voice] = definition[9];
        self.write(0xe0 + modulator, definition[10]);
        self.write(0xe0 + carrier, definition[11]);
        Ok(())
    }

    fn note_frequency(&self, voice: usize, note: u8) -> Result<u16> {
        // BL addition and shift wrap at eight bits in the original driver.
        let index = note.wrapping_add(self.transpose[voice]).wrapping_mul(2);
        word(&self.driver, 0x14 + index as usize)
    }

    fn play_voice(&mut self, voice: usize, frequency: u16, velocity: u8, on: bool) {
        if on {
            self.write(
                0x40 + self.driver[0x353 + voice],
                self.level[voice].wrapping_sub(velocity >> 2),
            );
        }
        self.write(0xa0 + voice as u8, frequency as u8);
        let high = (frequency >> 8) as u8 | if on { 0x20 } else { 0 };
        self.write(0xb0 + voice as u8, high);
        self.frequency[voice] = u16::from(frequency as u8) | (u16::from(high) << 8);
    }

    fn note_off(&mut self, channel: u8, note: u8) -> Result<()> {
        if let Some(voice) = (0..9).find(|&v| self.channel[v] == channel && self.note[v] == note) {
            self.active[voice] = false;
            let pitch = if channel == 9 {
                self.driver[0x389 + voice]
            } else {
                self.note[voice]
            };
            self.note[voice] = 0;
            self.age[voice] = 0;
            let frequency = self.note_frequency(voice, pitch)?;
            self.play_voice(voice, frequency, 0, false);
        }
        Ok(())
    }

    fn note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<()> {
        if velocity == 0 {
            return self.note_off(channel, note);
        }
        if channel == 9 {
            if let Some(voice) =
                (0..9).find(|&v| self.channel[v] == 9 && self.driver[0x380 + v] == note)
            {
                self.active[voice] = true;
                self.note[voice] = note;
                let frequency = self.note_frequency(voice, self.driver[0x389 + voice])?;
                self.play_voice(voice, frequency, velocity, true);
            }
            return Ok(());
        }
        let free = (0..9).find(|&v| self.channel[v] == channel && !self.active[v]);
        let voice = if let Some(voice) = free {
            Some(voice)
        } else {
            // Strictly greater age; ties retain the first voice, age zero cannot win.
            let mut oldest = 0;
            let mut selected = None;
            for voice in 0..9 {
                if self.channel[voice] == channel && self.age[voice] > oldest {
                    oldest = self.age[voice];
                    selected = Some(voice);
                }
            }
            if let Some(voice) = selected {
                self.play_voice(voice, 0, 0, false);
                self.age[voice] = 0;
            }
            selected
        };
        if let Some(voice) = voice {
            self.active[voice] = true;
            self.note[voice] = note;
            let frequency = self.note_frequency(voice, note)?;
            self.play_voice(voice, frequency, velocity, true);
        }
        Ok(())
    }

    pub fn advance(&mut self) -> Result<()> {
        self.tick += 1;
        if !self.playing {
            return Ok(());
        }
        for voice in 0..9 {
            if self.active[voice] {
                self.age[voice] = self.age[voice].wrapping_add(1);
            }
        }
        if self.wait != 0 {
            self.wait -= 1;
            return Ok(());
        }
        // Bound malformed zero-delay/loop streams; valid original music is finite per tick.
        for _ in 0..65536 {
            let first = self.byte()?;
            if first & 0x80 != 0 {
                if first == 0xfc {
                    if self.byte()? == 0x80 {
                        self.position = self.start;
                        continue;
                    }
                    self.playing = false;
                    return Ok(());
                }
                self.status = first;
            } else {
                self.position -= 1;
                if self.ignored {
                    self.position += if self.two_bytes { 2 } else { 1 };
                    let delay = self.byte()?;
                    if delay != 0 {
                        self.wait = delay - 1;
                        return Ok(());
                    }
                    continue;
                }
            }
            let channel = self.status & 15;
            let command = self.status & 0xf0;
            if command == 0xe0 {
                // Preserve driver 07a3's integer frequency scaling and stored base pitch.
                self.two_bytes = true;
                let lo = self.byte()?;
                let hi = self.byte()?;
                let factor = u16::from(lo) | (u16::from(hi) << 7);
                for voice in 0..9 {
                    if self.channel[voice] == channel && self.active[voice] {
                        let stored = self.frequency[voice];
                        let pitch = (((u32::from(stored & 0x3ff) * u32::from(factor)) >> 13)
                            as u16)
                            | (stored & 0xfc00);
                        self.write(0xa0 + voice as u8, pitch as u8);
                        self.write(0xb0 + voice as u8, (pitch >> 8) as u8);
                    }
                }
            } else {
                self.ignored = false;
                self.two_bytes = command != 0xc0;
                match command {
                    0xc0 => {
                        let program = self.byte()?;
                        for voice in 0..9 {
                            if channel != 9 && self.channel[voice] == channel {
                                self.instrument(voice, program)?;
                            }
                        }
                    }
                    0x90 => {
                        let note = self.byte()?;
                        let velocity = self.byte()?;
                        self.note_on(channel, note, velocity)?;
                    }
                    0x80 => {
                        let note = self.byte()?;
                        self.byte()?;
                        self.note_off(channel, note)?;
                    }
                    _ => {
                        self.byte()?;
                        self.byte()?;
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
        Err("Music exceeded per-tick event limit".into())
    }
}

/// Resident 5EB5 resolves the seventeen scores through DS:3EF0. The original
/// bank numbers differ from the native directory names; strings stay external.
pub fn song_data(assets: &Assets, id: u8) -> Result<Vec<u8>> {
    if id >= 17 {
        return Err("Music index outside the seventeen original scores".into());
    }
    let at = 0x3ef0 + usize::from(id) * 4;
    let bank = match word(&assets.data_segment, at + 2)? {
        0 => "MRACE",
        2 => "BATL",
        3 => "CENT",
        5 => "MMAP",
        6 => "MGLAD",
        7 => "MNEGO",
        8 => "MNAVAL",
        _ => return Err("Unknown original music resource bank".into()),
    };
    let name = usize::from(word(&assets.data_segment, at)?);
    let bytes = assets
        .data_segment
        .get(name..)
        .ok_or("Music name outside original data".to_string())?;
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or("Unterminated music name")?;
    let stem = std::str::from_utf8(&bytes[..end]).map_err(|e| e.to_string())?;
    assets.resource(bank, &format!("{stem}.LZW").to_uppercase())
}

#[cfg(test)]
pub(crate) fn fixture() -> Sequencer {
    let mut driver = vec![0; 0x97a];
    driver[0x167..0x169].copy_from_slice(&0x200u16.to_le_bytes());
    driver[0x200..0x20c]
        .copy_from_slice(&[0x21, 0x10, 0xf2, 0x74, 0x21, 0x3f, 0xf2, 0x74, 0, 0, 0, 0]);
    driver[0x14 + 60 * 2..0x16 + 60 * 2].copy_from_slice(&0x1578u16.to_le_bytes());
    driver[0x353] = 3;
    Sequencer::new(driver, fixture_song()).unwrap()
}

#[cfg(test)]
pub(crate) fn fixture_song() -> Vec<u8> {
    let mut song = vec![0; 64];
    song[..2].copy_from_slice(&64u16.to_le_bytes());
    song[0x23..0x2b].fill(0xff);
    // One voice: program, note, two ticks, running-status note-off, loop.
    song.extend_from_slice(&[0xc0, 0, 0, 0x90, 60, 100, 2, 60, 0, 1, 0xfc, 0x80]);
    song
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_status_delay_and_loop_preserve_voice_events() {
        let mut sequence = fixture();
        sequence.take_writes();
        sequence.advance().unwrap();
        let writes = sequence.take_writes();
        assert_eq!(writes.len(), 14); // Instrument then velocity and key-on.
        assert_eq!(
            writes.last().unwrap(),
            &RegisterWrite {
                tick: 1,
                register: 0xb0,
                value: 0x35
            }
        );
        sequence.advance().unwrap();
        assert!(sequence.take_writes().is_empty());
        sequence.advance().unwrap();
        assert_eq!(
            sequence.take_writes(),
            [
                RegisterWrite {
                    tick: 3,
                    register: 0xa0,
                    value: 0x78
                },
                RegisterWrite {
                    tick: 3,
                    register: 0xb0,
                    value: 0x15
                },
            ]
        );
        sequence.advance().unwrap();
        assert_eq!(sequence.take_writes().last().unwrap().value, 0x35);
    }

    #[test]
    fn truncated_and_empty_loop_events_fail() {
        let mut sequence = fixture();
        sequence.song.truncate(sequence.start + 1);
        assert!(sequence.advance().is_err());
        let mut sequence = fixture();
        sequence.song.truncate(sequence.start);
        sequence.song.extend_from_slice(&[0xfc, 0x80]);
        assert!(sequence.advance().is_err());
    }
}
