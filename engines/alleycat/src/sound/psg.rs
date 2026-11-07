// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Restored title's three-voice program; all notes remain in external inputs.

use std::sync::Arc;

/// PIT channel-zero divisor installed by the restored title, CS:16e9.
pub const TITLE_DIVISOR: u16 = 0xcfff;

#[derive(Clone, Debug)]
struct Track {
    notes: Vec<u8>,
    durations: Vec<u8>,
    latch: u8,
}

/// Checked canonical data-segment tables used by CS:173e/178d.
#[derive(Clone, Debug)]
pub struct Music {
    pitches: [u16; 67],
    tracks: [Track; 3],
    limits: [u16; 4],
}

fn word(data: &[u8], address: usize) -> Result<u16, String> {
    let bytes = data
        .get(address..address + 2)
        .ok_or("Restored title music exceeds external data")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

impl Music {
    /// Parse the restored executable's full canonical DS, before legacy-layout
    /// normalization. This validates structure, not release identity.
    pub fn from_data(data: &[u8]) -> Result<Self, String> {
        let mut pitches = [0; 67];
        for (i, pitch) in pitches.iter_mut().enumerate() {
            *pitch = word(data, 0x1014 + i * 2)?;
            if *pitch > 0x3ff {
                return Err("Restored title pitch exceeds PSG divider".into());
            }
        }
        let mut tracks = Vec::with_capacity(3);
        for channel in 0..3 {
            let state = usize::from(word(data, 0x124c + channel * 2)?);
            let latch = *data
                .get(state + 4)
                .ok_or("Restored title state exceeds external data")?;
            if latch != 0x80 + channel as u8 * 0x20 {
                return Err("Invalid restored title channel latch".into());
            }
            let notes_at = usize::from(word(data, state + 5)?);
            let durations_at = usize::from(word(data, state + 7)?);
            let tail = data
                .get(notes_at..)
                .ok_or("Restored title notes exceed external data")?;
            let length = tail
                .iter()
                .position(|note| *note == 0x86)
                .ok_or("Restored title track has no loop marker")?;
            if length == 0 || length > u16::MAX as usize {
                return Err("Invalid restored title track length".into());
            }
            let notes = tail[..length].to_vec();
            if notes.iter().any(|note| *note > 0x84 || note & 1 != 0) {
                return Err("Invalid restored title pitch-table offset".into());
            }
            let durations = data
                .get(durations_at..durations_at + length)
                .ok_or("Restored title durations exceed external data")?
                .to_vec();
            tracks.push(Track {
                notes,
                durations,
                latch,
            });
        }
        let mut limits = [0; 4];
        for (i, limit) in limits.iter_mut().enumerate() {
            *limit = word(data, 0x1256 + i * 2)?;
            if *limit > 4 || *limit & 1 != 0 {
                return Err("Invalid restored title channel-enable limit".into());
            }
        }
        Ok(Self {
            pitches,
            tracks: tracks.try_into().expect("three parsed tracks"),
            limits,
        })
    }
}

#[cfg(test)]
pub(crate) fn synthetic_music() -> Arc<Music> {
    Arc::new(Music::from_data(&tests::fixture()).unwrap())
}

#[derive(Default)]
struct Voice {
    remaining: u8,
    attenuation: u8,
    index: usize,
}

pub(super) struct Sequencer {
    music: Arc<Music>,
    voices: [Voice; 3],
    stage: u16,
    last_tick: u16,
}

impl Sequencer {
    pub(super) fn new(music: Arc<Music>, tick: u16) -> Self {
        Self {
            music,
            voices: std::array::from_fn(|_| Voice {
                attenuation: 15,
                ..Voice::default()
            }),
            stage: 0,
            last_tick: tick,
        }
    }

    /// One invocation after a BIOS tick change, not catch-up for skipped ticks.
    pub(super) fn step(&mut self, tick: u16, mut write: impl FnMut(u8)) {
        if tick == self.last_tick {
            return;
        }
        self.last_tick = tick;
        for (channel, (voice, track)) in self.voices.iter_mut().zip(&self.music.tracks).enumerate()
        {
            if voice.remaining != 0 {
                voice.remaining -= 1;
                if voice.remaining != 0 {
                    if voice.attenuation < 15 {
                        voice.attenuation += 2;
                        write(track.latch | 0x10 | voice.attenuation);
                    } else {
                        write(track.latch | 0x1f);
                    }
                    continue;
                }
            }
            if voice.index == track.notes.len() {
                voice.index = 0;
                if channel == 0 {
                    self.stage = self.stage.wrapping_add(2);
                }
            }
            let note = track.notes[voice.index];
            voice.remaining = track.durations[voice.index];
            voice.index += 1;
            if note == 0 {
                // The native rest does not modify the stored attenuation.
                write(track.latch | 0x1f);
            } else if self.music.limits[usize::from((self.stage & 6) / 2)] >= channel as u16 * 2 {
                voice.attenuation = 3;
                write(track.latch | 0x13);
                let divider = self.music.pitches[usize::from(note / 2)];
                write(track.latch | (divider as u8 & 15));
                write((divider >> 4) as u8 & 0x3f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fixture() -> Vec<u8> {
        let mut data = vec![0; 0x1260];
        let mut set = |at: usize, value: u16| {
            data[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        set(0x1016, 0x123);
        for channel in 0..3 {
            let state = 0x1231 + channel * 9;
            set(0x124c + channel * 2, state as u16);
            set(state + 5, (0x1100 + channel * 8) as u16);
            set(state + 7, (0x1140 + channel * 8) as u16);
        }
        for (i, limit) in [0, 2, 4, 4].into_iter().enumerate() {
            set(0x1256 + 2 * i, limit);
        }
        for channel in 0..3 {
            data[0x1235 + channel * 9] = 0x80 + channel as u8 * 0x20;
            data[0x1100 + channel * 8..0x1103 + channel * 8].copy_from_slice(&[2, 0, 0x86]);
            data[0x1140 + channel * 8..0x1142 + channel * 8].copy_from_slice(&[3, 1]);
        }
        data
    }

    #[test]
    fn duration_envelope_rests_loops_and_staged_voices_follow_native_program() {
        let music = Arc::new(Music::from_data(&fixture()).unwrap());
        let mut seq = Sequencer::new(music, 10);
        let mut out = Vec::new();
        seq.step(10, |v| out.push(v));
        assert!(out.is_empty());
        let mut step = |tick| {
            let mut out = Vec::new();
            seq.step(tick, |v| out.push(v));
            out
        };
        assert_eq!(step(11), [0x93, 0x83, 0x12]);
        assert_eq!(step(12), [0x95, 0xbf, 0xdf]);
        assert_eq!(step(13), [0x97, 0xbf, 0xdf]);
        assert_eq!(step(14), [0x9f, 0xbf, 0xdf]);
        assert_eq!(step(15), [0x93, 0x83, 0x12, 0xb3, 0xa3, 0x12]);
        step(16);
        step(17);
        step(18);
        assert_eq!(
            step(19),
            [0x93, 0x83, 0x12, 0xb3, 0xa3, 0x12, 0xd3, 0xc3, 0x12]
        );
    }

    #[test]
    fn skipped_and_wrapped_bios_ticks_execute_once_per_invocation() {
        let music = Arc::new(Music::from_data(&fixture()).unwrap());
        let mut seq = Sequencer::new(music, u16::MAX);
        let mut out = Vec::new();
        seq.step(0, |v| out.push(v));
        assert_eq!(out.len(), 3);
        out.clear();
        seq.step(100, |v| out.push(v));
        assert_eq!(out, [0x95, 0xbf, 0xdf]);
        out.clear();
        seq.step(100, |v| out.push(v));
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_empty_loops_invalid_notes_and_out_of_bounds_tracks() {
        let mut data = fixture();
        data[0x1100] = 0x86;
        assert!(Music::from_data(&data).is_err());
        data[0x1100] = 3;
        assert!(Music::from_data(&data).is_err());
        data[0x1100] = 2;
        data[0x1236..0x1238].copy_from_slice(&0xffffu16.to_le_bytes());
        assert!(Music::from_data(&data).is_err());
    }

    #[test]
    fn sound_lifecycle_records_timer_program_and_native_mute_without_catchup() {
        use crate::sound::Sound;
        let mut sound = Sound::default();
        let mut data = vec![0; crate::assets::DATA_LENGTH];
        data[0] = 255;
        sound.set_psg_music(Some(Arc::new(Music::from_data(&fixture()).unwrap())));
        sound.record(true);
        sound.begin_psg_title(50);
        let init: Vec<_> = sound.drain().map(|w| (w.port, w.value)).collect();
        assert_eq!(
            init,
            [
                (0xc0, 0x9f),
                (0xc0, 0xbf),
                (0xc0, 0xdf),
                (0xc0, 0xff),
                (0x43, 0x36),
                (0x40, 0xff),
                (0x40, 0xcf)
            ]
        );
        sound.splash(&mut data, 50).unwrap();
        assert_eq!(sound.drain().count(), 0);
        sound.splash(&mut data, 51).unwrap();
        assert_eq!(
            sound.drain().map(|w| w.value).collect::<Vec<_>>(),
            [0x93, 0x83, 0x12]
        );
        data[0] = 0;
        sound.splash(&mut data, 100).unwrap();
        assert_eq!(sound.drain().count(), 0);
        data[0] = 255;
        sound.splash(&mut data, 101).unwrap();
        assert_eq!(
            sound.drain().map(|w| w.value).collect::<Vec<_>>(),
            [0x95, 0xbf, 0xdf]
        );
        sound.finish_psg_title();
        let end: Vec<_> = sound.drain().map(|w| (w.port, w.value)).collect();
        assert_eq!(&end[4..], [(0x43, 0x36), (0x40, 0), (0x40, 0)]);
    }
}
