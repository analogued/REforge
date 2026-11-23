// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original speaker sequencing through the shared PIT, digital wiring and PCM filter.
use crate::assets::{Assets, Result};
use crate::audio::{SAMPLE_RATE, tick_frame};
use crate::intro::PIT_HZ;
use crate::speaker::Sequencer;
use reforge_audio::{
    render::{BlipRenderer, Config, MAX_LEVEL, SignalRenderer},
    speaker::{Events, PcSpeaker},
};
use std::collections::VecDeque;

pub struct Music {
    sequence: Sequencer,
    device: PcSpeaker,
    renderer: BlipRenderer,
    samples: VecDeque<i16>,
    frame: u64,
    finished_at: Option<u64>,
}

impl Music {
    pub fn song(assets: &Assets, id: u8) -> Result<Self> {
        Self::from_sequencer(Sequencer::song(assets, id)?)
    }

    pub fn from_sequencer(sequence: Sequencer) -> Result<Self> {
        // Model the driver's low two port-61 bits, not unrelated refresh/status pins.
        let device = PcSpeaker::new(0, [true; 3]);
        let renderer = BlipRenderer::new(
            Config {
                clock_hz: reforge_audio::PIT_HZ as u32,
                sample_rate: SAMPLE_RATE,
                chunk_ticks: 4096,
            },
            0,
            0,
        )
        .map_err(|e| e.to_string())?;
        let mut music = Self {
            sequence,
            device,
            renderer,
            samples: VecDeque::new(),
            frame: 0,
            finished_at: None,
        };
        music.apply()?;
        Ok(music)
    }

    fn observe(&mut self, events: Events) -> Result<()> {
        if let Some(edge) = events.drive {
            let samples = &mut self.samples;
            self.renderer
                .level_at(
                    edge.tick,
                    if edge.high { MAX_LEVEL } else { 0 },
                    &mut |pcm| {
                        samples.extend(pcm);
                        Ok(())
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn apply(&mut self) -> Result<()> {
        for write in self.sequence.take_writes() {
            let events = self
                .device
                .write_port(write.port, write.value)
                .map_err(|e| e.to_string())?;
            self.observe(events)?;
        }
        Ok(())
    }

    fn advance_to(&mut self, target: u64, end_frame: u64) -> Result<()> {
        loop {
            let next_frame = tick_frame(
                self.sequence
                    .tick
                    .checked_add(1)
                    .ok_or("Speaker clock overflow")?,
            );
            // Match AdLib's output-frame boundary: a callback at the end of a
            // requested buffer belongs to the next buffer, after any command
            // scheduled there. The PIT still advances at its native frequency.
            let next_sequence =
                u64::try_from((u128::from(next_frame) * PIT_HZ).div_ceil(u128::from(SAMPLE_RATE)))
                    .map_err(|e| e.to_string())?;
            let horizon = target.min(next_sequence);
            while self.device.pit().tick() < horizon {
                let events = self.device.advance_to(horizon).map_err(|e| e.to_string())?;
                self.observe(events)?;
            }
            if horizon == next_sequence && next_frame < end_frame {
                let playing = self.sequence.is_playing();
                self.sequence.advance()?;
                if playing && !self.sequence.is_playing() {
                    self.finished_at = Some(tick_frame(self.sequence.tick));
                }
                // Device transitions at this clock precede the ordered driver writes.
                self.apply()?;
            }
            if horizon == target {
                break;
            }
        }
        let samples = &mut self.samples;
        self.renderer
            .finish_until(target, &mut |pcm| {
                samples.extend(pcm);
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    #[cfg(any(feature = "sdl", test))]
    pub fn play_data(&mut self, song: Vec<u8>) -> Result<()> {
        self.sequence.restart(song)?;
        self.finished_at = None;
        self.apply()
    }

    #[cfg(any(feature = "sdl", test))]
    pub fn stop(&mut self) -> Result<()> {
        self.sequence.stop();
        self.finished_at = Some(self.frame);
        self.apply()
    }

    #[cfg(any(feature = "sdl", test))]
    pub fn is_playing(&self) -> bool {
        self.finished_at.is_none_or(|end| self.frame < end)
    }

    #[cfg(any(feature = "sdl", test))]
    pub fn position_frames(&self) -> u64 {
        self.frame
    }

    #[cfg(any(feature = "sdl", test))]
    pub fn finished_at(&self) -> Option<u64> {
        self.finished_at.filter(|end| *end <= self.frame)
    }

    pub fn render(&mut self, output: &mut [i16]) -> Result<()> {
        if !output.len().is_multiple_of(2) {
            return Err("Stereo buffer has odd length".into());
        }
        // Commit only the requested sample horizon. Stop/restart commands can arrive
        // at any host frame boundary; do not synthesize future music ahead of them.
        for stereo in output.chunks_mut(512) {
            let frames = stereo.len() / 2;
            let end = self
                .frame
                .checked_add(frames as u64)
                .ok_or("Speaker frame overflow")?;
            let clock = (u128::from(end) * PIT_HZ).div_ceil(u128::from(SAMPLE_RATE));
            self.advance_to(u64::try_from(clock).map_err(|e| e.to_string())?, end)?;
            if self.samples.len() < frames {
                return Err("Speaker renderer produced too few frames".into());
            }
            for pair in stereo.as_chunks_mut::<2>().0 {
                let sample = self.samples.pop_front().unwrap();
                pair.fill(sample);
            }
            self.frame = end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player(events: &[u8]) -> Music {
        Music::from_sequencer(
            Sequencer::new(
                crate::speaker::fixture_driver(),
                crate::speaker::fixture_song(events),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn looping() -> Music {
        player(&[0x90, 60, 100, 2, 60, 0, 1, 0xfc, 0x80])
    }
    #[test]
    fn speaker_pcm_is_stereo_and_independent_of_callback_chunks() {
        let mut whole = looping();
        let mut split = looping();
        let mut expected = vec![0; 40000];
        let mut actual = vec![0; expected.len()];
        whole.render(&mut expected).unwrap();
        for chunk in actual.chunks_mut(34) {
            split.render(chunk).unwrap();
        }
        assert_eq!(expected, actual);
        assert!(actual.iter().any(|s| *s != 0));
        assert!(
            actual
                .as_chunks::<2>()
                .0
                .iter()
                .all(|pair| pair[0] == pair[1])
        );
        assert_eq!(whole.device, split.device);
        assert_eq!(whole.position_frames(), 20000);
        assert!(whole.render(&mut [0; 3]).is_err());
    }
    #[test]
    fn a_stop_before_the_next_sample_boundary_prevents_early_event_consumption() {
        // Logical tick16 lies between output frames10545 and10546. Rendering
        // through10545 must leave that callback pending, just as AdLib does.
        let mut music = player(&[0x90, 60, 100, 15, 61, 100, 1, 0xfc, 0]);
        music.render(&mut vec![0; 10545 * 2]).unwrap();
        assert_eq!(music.sequence.tick, 15);
        let divider = music.device.pit().channel(2).unwrap().programmed_count;
        music.stop().unwrap();
        music.render(&mut [0; 4]).unwrap();
        assert_eq!(music.sequence.tick, 16);
        assert_eq!(
            music.device.pit().channel(2).unwrap().programmed_count,
            divider
        );
        assert_eq!(music.device.control() & 3, 0);
    }

    #[test]
    fn completion_and_interruptions_preserve_sample_clock_and_device_phase() {
        let mut whole = player(&[0x90, 60, 100, 2, 0x80, 60, 1, 1, 0xfc, 0]);
        let mut split = player(&[0x90, 60, 100, 2, 0x80, 60, 1, 1, 0xfc, 0]);
        let mut expected = vec![0; 10000];
        let mut actual = vec![0; expected.len()];
        whole.render(&mut expected).unwrap();
        for chunk in actual.chunks_mut(218) {
            split.render(chunk).unwrap();
        }
        assert_eq!(expected, actual);
        assert_eq!(whole.finished_at(), Some(tick_frame(4)));
        assert_eq!(whole.finished_at(), split.finished_at());
        for music in [&mut whole, &mut split] {
            music.stop().unwrap();
            assert_eq!(music.finished_at(), Some(5000));
            let tick = music.device.pit().tick();
            music
                .play_data(crate::speaker::fixture_song(&[
                    0x90, 60, 100, 2, 60, 0, 1, 0xfc, 0x80,
                ]))
                .unwrap();
            assert_eq!(music.device.pit().tick(), tick);
            assert_eq!(music.position_frames(), 5000);
            assert_eq!(music.finished_at(), None);
        }
        whole.render(&mut expected).unwrap();
        for chunk in actual.chunks_mut(14) {
            split.render(chunk).unwrap();
        }
        assert_eq!(expected, actual);
        assert_eq!(whole.device, split.device);
    }
}
