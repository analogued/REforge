// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned, sample-timestamped playback. No game state or host clock is accessed.
use crate::assets::Result;
use crate::audio::SAMPLE_RATE;
use crate::audio_playback::Music;
use crate::intro::{PIT_DIVISOR, PIT_HZ};
use std::collections::VecDeque;

pub const MAX_PENDING: usize = 256;

pub fn clock_ticks(frames: u64) -> u64 {
    (u128::from(frames) * PIT_HZ / (u128::from(SAMPLE_RATE) * PIT_DIVISOR)) as u64
}

pub enum Change {
    Start { id: u8, music: Box<Music> },
    Play { id: u8, song: Vec<u8> },
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Finished,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub frame: u64,
    pub track: u8,
    pub kind: CompletionKind,
}

pub struct Request {
    pub frame: u64,
    pub change: Change,
}

/// Source-frame positions, not a measurement of the DAC/speaker latency.
#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub rendered: u64,
    pub track: Option<u8>,
    pub track_started: u64,
    pub playing: bool,
    pub finished_at: Option<u64>,
    pub applied: u64,
    pub late_frames: u64,
}

#[derive(Default)]
pub struct Timeline {
    music: Option<Music>,
    pending: VecDeque<Request>,
    progress: Progress,
    music_origin: u64,
    completions: VecDeque<Completion>,
    reported_completion: Option<u64>,
}

impl Timeline {
    pub fn schedule(&mut self, request: Request) -> Result<()> {
        if self.pending.len() == MAX_PENDING {
            return Err("Audio command queue is full".into());
        }
        // Keep arrival order for commands sharing a sample boundary.
        let at = self.pending.partition_point(|r| r.frame <= request.frame);
        self.pending.insert(at, request);
        Ok(())
    }

    pub fn progress(&self) -> Progress {
        self.progress
    }

    pub fn pop_completion(&mut self) -> Option<Completion> {
        self.completions.pop_front()
    }

    fn apply(&mut self, request: Request) -> Result<()> {
        self.progress.late_frames = self
            .progress
            .late_frames
            .saturating_add(self.progress.rendered.saturating_sub(request.frame));
        match request.change {
            Change::Start { id, music } => {
                if music.position_frames() != 0 {
                    return Err("Initial audio player has already advanced".into());
                }
                self.music_origin = self.progress.rendered;
                self.music = Some(*music);
                self.progress.track = Some(id);
                self.progress.track_started = self.progress.rendered;
                self.reported_completion = None;
            }
            Change::Play { id, song } => {
                self.music
                    .as_mut()
                    .ok_or("Missing audio player")?
                    .play_data(song)?;
                self.progress.track = Some(id);
                self.progress.track_started = self.progress.rendered;
                self.reported_completion = None;
            }
            Change::Stop => {
                if let Some(music) = &mut self.music {
                    music.stop()?;
                    self.reported_completion = None;
                }
                self.update_progress(CompletionKind::Stopped)?;
            }
        }
        self.progress.applied += 1;
        self.update_progress(CompletionKind::Finished)
    }

    fn update_progress(&mut self, kind: CompletionKind) -> Result<()> {
        self.progress.playing = self.music.as_ref().is_some_and(Music::is_playing);
        self.progress.finished_at = self
            .music
            .as_ref()
            .and_then(Music::finished_at)
            .map(|frame| self.music_origin + frame);
        if let (Some(frame), Some(track)) = (self.progress.finished_at, self.progress.track)
            && self.reported_completion != Some(frame)
        {
            if self.completions.len() == MAX_PENDING {
                return Err("Audio completion queue is full".into());
            }
            self.completions
                .push_back(Completion { frame, track, kind });
            self.reported_completion = Some(frame);
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn render(&mut self, output: &mut [i16]) -> Result<()> {
        self.render_with_gain(output, |_| 1.0)
    }

    /// Host gain follows each scheduled score boundary without altering synthesis.
    pub fn render_with_gain(
        &mut self,
        output: &mut [i16],
        gain: impl Fn(Option<u8>) -> f32,
    ) -> Result<()> {
        if !output.len().is_multiple_of(2) {
            return Err("Stereo buffer has odd length".into());
        }
        let mut offset = 0;
        while offset < output.len() {
            while self
                .pending
                .front()
                .is_some_and(|r| r.frame <= self.progress.rendered)
            {
                let request = self.pending.pop_front().unwrap();
                self.apply(request)?;
            }
            let remaining = (output.len() - offset) / 2;
            let frames = self.pending.front().map_or(remaining, |r| {
                remaining.min((r.frame - self.progress.rendered) as usize)
            });
            let part = &mut output[offset..offset + frames * 2];
            if let Some(music) = &mut self.music {
                music.render(part)?;
            } else {
                part.fill(0);
            }
            let gain = gain(self.progress.track);
            if gain != 1.0 {
                for sample in part {
                    *sample = (f32::from(*sample) * gain)
                        .round()
                        .clamp(i16::MIN as f32, i16::MAX as f32)
                        as i16;
                }
            }
            self.progress.rendered += frames as u64;
            offset += frames * 2;
            self.update_progress(CompletionKind::Finished)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tick_frame;

    fn start() -> Request {
        Request {
            frame: 0,
            change: Change::Start {
                id: 0,
                music: Box::new(Music::from_sequencer(crate::music::fixture()).unwrap()),
            },
        }
    }

    #[test]
    fn host_gain_changes_at_score_boundaries_without_changing_execution() {
        let mut original = Timeline::default();
        let mut scaled = Timeline::default();
        for timeline in [&mut original, &mut scaled] {
            timeline.schedule(start()).unwrap();
            timeline
                .schedule(Request {
                    frame: 1000,
                    change: Change::Play {
                        id: 5,
                        song: crate::music::fixture_song(),
                    },
                })
                .unwrap();
        }
        let mut expected = vec![0; 4000];
        original.render(&mut expected).unwrap();
        assert!(expected[..2000].iter().any(|sample| *sample != 0));
        let mut actual = vec![0; 4000];
        scaled
            .render_with_gain(
                &mut actual,
                |track| if track == Some(5) { 0.0 } else { 1.0 },
            )
            .unwrap();
        assert_eq!(&actual[..2000], &expected[..2000]);
        assert!(actual[2000..].iter().all(|sample| *sample == 0));
        assert_eq!(original.progress().rendered, scaled.progress().rendered);
        assert_eq!(original.progress().applied, scaled.progress().applied);
        assert_eq!(
            original.progress().track_started,
            scaled.progress().track_started
        );
    }

    #[test]
    fn timestamped_changes_split_pcm_at_exact_samples_independent_of_chunks() {
        let mut whole = Timeline::default();
        let mut split = Timeline::default();
        for timeline in [&mut whole, &mut split] {
            timeline.schedule(start()).unwrap();
            // Deliberately enqueue out of sample order; same-frame order is FIFO.
            timeline
                .schedule(Request {
                    frame: 4000,
                    change: Change::Stop,
                })
                .unwrap();
            timeline
                .schedule(Request {
                    frame: 2000,
                    change: Change::Play {
                        id: 1,
                        song: crate::music::fixture_song(),
                    },
                })
                .unwrap();
            timeline
                .schedule(Request {
                    frame: 4000,
                    change: Change::Play {
                        id: 2,
                        song: crate::music::fixture_song(),
                    },
                })
                .unwrap();
        }
        let mut a = vec![0; 20_000];
        let mut b = vec![0; a.len()];
        whole.render(&mut a).unwrap();
        for part in b.chunks_mut(206) {
            split.render(part).unwrap();
        }
        let mut direct = Music::from_sequencer(crate::music::fixture()).unwrap();
        let mut expected = vec![0; a.len()];
        direct.render(&mut expected[..4000]).unwrap();
        direct.play_data(crate::music::fixture_song()).unwrap();
        direct.render(&mut expected[4000..8000]).unwrap();
        direct.stop().unwrap();
        direct.play_data(crate::music::fixture_song()).unwrap();
        direct.render(&mut expected[8000..]).unwrap();
        assert_eq!(a, expected);
        assert_eq!(a, b);
        assert_eq!(whole.progress().rendered, 10_000);
        assert_eq!(whole.progress().track, Some(2));
        assert_eq!(whole.progress().track_started, 4000);
        assert_eq!(whole.progress().applied, 4);
        assert_eq!(whole.progress().late_frames, 0);
    }

    #[test]
    fn late_commands_apply_next_without_dropping_or_rewinding_samples() {
        let mut timeline = Timeline::default();
        timeline.schedule(start()).unwrap();
        timeline.render(&mut [0; 2000]).unwrap();
        timeline
            .schedule(Request {
                frame: 500,
                change: Change::Stop,
            })
            .unwrap();
        timeline.render(&mut [0; 2]).unwrap();
        assert_eq!(timeline.progress().rendered, 1001);
        assert_eq!(timeline.progress().late_frames, 500);
        assert_eq!(timeline.progress().finished_at, Some(1000));
        assert!(!timeline.progress().playing);
        assert!(timeline.render(&mut [0; 3]).is_err());
    }

    #[test]
    fn completion_cues_survive_a_track_switch_before_the_game_polls() {
        let mut timeline = Timeline::default();
        timeline.schedule(start()).unwrap();
        let mut terminal = crate::music::fixture_song();
        *terminal.last_mut().unwrap() = 0;
        timeline
            .schedule(Request {
                frame: 1000,
                change: Change::Play {
                    id: 1,
                    song: terminal,
                },
            })
            .unwrap();
        // The song starts at music tick 2 (frame tick_frame(2)), ending four ticks later.
        timeline
            .schedule(Request {
                frame: 6000,
                change: Change::Play {
                    id: 2,
                    song: crate::music::fixture_song(),
                },
            })
            .unwrap();
        timeline
            .schedule(Request {
                frame: 7000,
                change: Change::Stop,
            })
            .unwrap();
        timeline.render(&mut [0; 16000]).unwrap();
        let first = timeline.pop_completion().unwrap();
        assert_eq!(first.track, 1);
        assert_eq!(first.kind, CompletionKind::Finished);
        assert!(first.frame > 1000 && first.frame < 6000);
        assert_eq!(
            timeline.pop_completion(),
            Some(Completion {
                frame: 7000,
                track: 2,
                kind: CompletionKind::Stopped
            })
        );
        assert_eq!(timeline.pop_completion(), None);
    }

    #[test]
    fn silent_audio_clock_keeps_game_time_moving_and_queue_is_bounded() {
        let mut timeline = Timeline::default();
        let mut silence = [7; 96];
        timeline.render(&mut silence).unwrap();
        assert_eq!(silence, [0; 96]);
        assert_eq!(timeline.progress().rendered, 48);
        for frame in 0..MAX_PENDING {
            timeline
                .schedule(Request {
                    frame: frame as u64 + 1000,
                    change: Change::Stop,
                })
                .unwrap();
        }
        assert!(
            timeline
                .schedule(Request {
                    frame: 2000,
                    change: Change::Stop
                })
                .is_err()
        );
        for tick in [0, 1, 432, 2664, 72000, 1_193_182] {
            assert_eq!(clock_ticks(tick_frame(tick)), tick);
        }
    }
}
