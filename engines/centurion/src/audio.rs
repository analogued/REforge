// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Replaceable OPL synthesis behind a timed register-write boundary.
use crate::assets::{Assets, Result};
use crate::intro::{PIT_DIVISOR, PIT_HZ};
use crate::music::{RegisterWrite, Sequencer};
use reforge_audio::opl::{Nuked, Opl};
use std::io::Write;
use std::path::Path;

pub const SAMPLE_RATE: u32 = 48_000;

pub fn tick_frame(tick: u64) -> u64 {
    (u128::from(tick) * PIT_DIVISOR * u128::from(SAMPLE_RATE)).div_ceil(PIT_HZ) as u64
}

pub struct Music {
    sequencer: Sequencer,
    synth: Box<dyn Opl + Send>,
    frame: u64,
    finished_at: Option<u64>,
}
impl Music {
    pub fn song(assets: &Assets, id: u8) -> Result<Self> {
        Self::from_sequencer(Sequencer::song(assets, id)?)
    }
    /// Prepare on the game thread, then move the owned player to audio playback.
    pub fn from_sequencer(sequencer: Sequencer) -> Result<Self> {
        let mut result = Self {
            sequencer,
            synth: Box::new(Nuked::new(SAMPLE_RATE).map_err(|e| e.to_string())?),
            frame: 0,
            finished_at: None,
        };
        // Shared reset leaves NEW disabled. Centurion's u8 register events below
        // always target bank zero, preserving the original OPL2 behavior.
        result.synth.reset();
        result.apply()?;
        Ok(result)
    }
    /// Switch music using resource data already read on the game thread.
    #[cfg(any(feature = "sdl", test))]
    pub fn play_data(&mut self, song: Vec<u8>) -> Result<()> {
        self.sequencer.restart(song)?;
        self.finished_at = None;
        self.apply()
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn stop(&mut self) -> Result<()> {
        self.sequencer.stop();
        self.finished_at = Some(self.frame);
        self.apply()
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn is_playing(&self) -> bool {
        self.sequencer.is_playing()
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn position_frames(&self) -> u64 {
        self.frame
    }
    /// Sequencer completion/explicit-stop boundary in the rendered sample clock.
    /// OPL release tails may continue after this position.
    #[cfg(any(feature = "sdl", test))]
    pub fn finished_at(&self) -> Option<u64> {
        self.finished_at
    }
    fn apply(&mut self) -> Result<()> {
        // A tick's ordered writes are the replaceable backend's input buffer.
        for RegisterWrite {
            register, value, ..
        } in self.sequencer.take_writes()
        {
            self.synth
                .write(u16::from(register), value)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn render(&mut self, output: &mut [i16]) -> Result<()> {
        if !output.len().is_multiple_of(2) {
            return Err("Stereo buffer has odd length".into());
        }
        let mut offset = 0;
        while offset < output.len() {
            let next = tick_frame(self.sequencer.tick + 1);
            if self.frame == next {
                let was_playing = self.sequencer.is_playing();
                self.sequencer.advance()?;
                if was_playing && !self.sequencer.is_playing() {
                    self.finished_at = Some(self.frame);
                }
                self.apply()?;
                continue;
            }
            let frames = ((output.len() - offset) / 2).min((next - self.frame) as usize);
            self.synth
                .render(&mut output[offset..offset + frames * 2])
                .map_err(|e| e.to_string())?;
            self.frame += frames as u64;
            offset += frames * 2;
        }
        Ok(())
    }
}

pub fn export(
    assets: &Assets,
    song: u8,
    ticks: u32,
    wav: Option<&Path>,
    trace: Option<&Path>,
) -> Result<()> {
    if ticks == 0 || ticks > 72_000 {
        return Err("Music ticks must be 1..72000".into());
    }
    if let Some(path) = trace {
        let mut sequencer = Sequencer::song(assets, song)?;
        let mut file =
            std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
        writeln!(file, "tick\tregister\tvalue").map_err(|e| e.to_string())?;
        for tick in 0..=ticks {
            if tick != 0 {
                sequencer.advance()?;
            }
            for write in sequencer.take_writes() {
                writeln!(file, "{}\t{}\t{}", write.tick, write.register, write.value)
                    .map_err(|e| e.to_string())?;
            }
        }
        file.flush().map_err(|e| e.to_string())?;
    }
    if let Some(path) = wav {
        let frames = tick_frame(u64::from(ticks));
        let data_bytes = u32::try_from(frames * 4).map_err(|e| e.to_string())?;
        let mut file =
            std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
        let mut header = Vec::new();
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(data_bytes + 36).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&2u16.to_le_bytes());
        header.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        header.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
        header.extend_from_slice(&4u16.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_bytes.to_le_bytes());
        file.write_all(&header).map_err(|e| e.to_string())?;
        let mut music = Music::song(assets, song)?;
        let mut remaining = frames;
        let mut peak = 0;
        while remaining != 0 {
            let count = remaining.min(2048) as usize;
            let mut samples = vec![0; count * 2];
            music.render(&mut samples)?;
            let bytes: Vec<_> = samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect();
            peak = peak.max(
                samples
                    .iter()
                    .map(|s| i32::from(*s).abs())
                    .max()
                    .unwrap_or(0),
            );
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            remaining -= count as u64;
        }
        file.flush().map_err(|e| e.to_string())?;
        if peak == 0 {
            return Err("Music export was silent".into());
        }
        println!(
            "music-check: frames={frames} peak={peak} wav={}",
            path.display()
        );
    }
    Ok(())
}

/// Private conformance harness: the same play/tick/stop schedule as the DOS client.
pub fn export_suite(assets: &Assets, path: &Path) -> Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    writeln!(file, "pass\tscore\tevent\ttick\tregister\tvalue").map_err(|e| e.to_string())?;
    let mut player = Sequencer::intro(assets)?;
    player.take_writes();
    for pass in 0..2 {
        for id in 0..17 {
            let data = crate::music::song_data(assets, id)?;
            let mut pos = usize::from(crate::assets::word(&data, 0)?);
            let mut status = 0;
            let mut ticks = 1u32;
            loop {
                if data[pos] & 128 != 0 {
                    status = data[pos];
                    pos += 1;
                    if status == 252 {
                        break;
                    }
                }
                pos += if status & 240 == 192 { 1 } else { 2 };
                ticks += u32::from(data[pos]);
                pos += 1;
            }
            if pass == 0 || id == 0 {
                player = Sequencer::song(assets, id)?;
            } else {
                player.tick = 0;
                player.restart(data)?;
            }
            for event in 0..=ticks * 2 + 21 {
                let kind = if event == 0 {
                    "play"
                } else if event == ticks * 2 + 21 {
                    player.stop();
                    "stop"
                } else {
                    player.advance()?;
                    "tick"
                };
                for write in player.take_writes() {
                    writeln!(
                        file,
                        "{pass}\t{id}\t{kind}\t{}\t{}\t{}",
                        write.tick, write.register, write.value
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    file.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reforge_audio::opl::Error;

    fn fixture() -> Music {
        Music::from_sequencer(crate::music::fixture()).unwrap()
    }

    struct Failing {
        write: bool,
    }
    impl Opl for Failing {
        fn sample_rate(&self) -> u32 {
            SAMPLE_RATE
        }
        fn reset(&mut self) {}
        fn write(&mut self, _: u16, _: u8) -> std::result::Result<(), Error> {
            if self.write {
                Err(Error::Backend("test register failure".into()))
            } else {
                Ok(())
            }
        }
        fn render(&mut self, _: &mut [i16]) -> std::result::Result<(), Error> {
            Err(Error::Backend("test PCM failure".into()))
        }
    }

    #[test]
    fn shared_backend_errors_reach_the_player_caller() {
        let mut music = fixture();
        music.synth = Box::new(Failing { write: true });
        assert_eq!(
            music.play_data(crate::music::fixture_song()).unwrap_err(),
            "test register failure"
        );
        assert_eq!(music.stop().unwrap_err(), "test register failure");
        music.sequencer = crate::music::fixture();
        music.sequencer.take_writes();
        music.frame = tick_frame(1);
        assert_eq!(
            music.render(&mut [0; 2]).unwrap_err(),
            "test register failure"
        );
        music.synth = Box::new(Failing { write: false });
        assert_eq!(music.render(&mut [0; 2]).unwrap_err(), "test PCM failure");
    }

    #[test]
    fn owned_shared_player_moves_to_audio_thread_without_changing_pcm() {
        let mut expected = vec![0; 16_000];
        fixture().render(&mut expected).unwrap();
        let music = fixture();
        let (actual, position) = std::thread::spawn(move || {
            let mut music = music;
            let mut actual = vec![0; 16_000];
            music.render(&mut actual).unwrap();
            (actual, music.position_frames())
        })
        .join()
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(position, 8_000);
    }

    #[test]
    fn completion_cues_use_exact_sample_boundaries_independent_of_buffering() {
        let mut song = crate::music::fixture_song();
        // The same authored voice stream, ending instead of looping at tick 4.
        *song.last_mut().unwrap() = 0;
        let mut whole = fixture();
        let mut split = fixture();
        for player in [&mut whole, &mut split] {
            player.play_data(song.clone()).unwrap();
            assert_eq!(player.finished_at(), None);
        }
        let mut expected = vec![0; 20_000];
        let mut actual = vec![0; expected.len()];
        whole.render(&mut expected).unwrap();
        for part in actual.chunks_mut(34) {
            split.render(part).unwrap();
        }
        assert_eq!(actual, expected);
        assert_eq!(whole.finished_at(), Some(tick_frame(4)));
        assert_eq!(split.finished_at(), whole.finished_at());
        whole.render(&mut [0; 200]).unwrap();
        assert_eq!(whole.finished_at(), Some(tick_frame(4)));
        whole.stop().unwrap();
        assert_eq!(whole.finished_at(), Some(10_100));
        whole.play_data(crate::music::fixture_song()).unwrap();
        assert_eq!(whole.finished_at(), None);
        assert_eq!(whole.position_frames(), 10_100);
    }

    #[test]
    fn interruptions_keep_the_sample_clock_and_are_buffer_independent() {
        let mut whole = fixture();
        let mut split = fixture();
        let mut first = vec![0; 1666];
        let mut other = vec![0; first.len()];
        whole.render(&mut first).unwrap();
        for chunk in other.chunks_mut(34) {
            split.render(chunk).unwrap();
        }
        assert_eq!(first, other);
        let frame = whole.frame;
        for player in [&mut whole, &mut split] {
            player.stop().unwrap();
            assert!(!player.is_playing());
            player
                .sequencer
                .restart(crate::music::fixture_song())
                .unwrap();
            player.apply().unwrap();
            assert_eq!(player.frame, frame);
        }
        let mut expected = vec![0; 32_000];
        let mut actual = vec![0; expected.len()];
        whole.render(&mut expected).unwrap();
        for chunk in actual.chunks_mut(218) {
            split.render(chunk).unwrap();
        }
        assert_eq!(expected, actual);
        assert!(actual.iter().any(|v| *v != 0));
    }

    #[test]
    fn pcm_and_register_timing_are_independent_of_host_buffer_size() {
        let mut whole = fixture();
        let mut split = fixture();
        let mut expected = vec![0; 12_000];
        whole.render(&mut expected).unwrap();
        let mut actual = vec![0; expected.len()];
        for chunk in actual.chunks_mut(194) {
            split.render(chunk).unwrap();
        }
        assert_eq!(actual, expected);
        assert!(actual.iter().any(|sample| *sample != 0));
        assert_eq!(whole.sequencer.tick, split.sequencer.tick);
        assert_eq!(tick_frame(1_193_182), 0x3fff * 48_000);
        assert!(split.render(&mut [0; 3]).is_err());
    }
}
