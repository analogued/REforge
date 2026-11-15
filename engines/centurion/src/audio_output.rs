// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Centurion's owned music commands/cues, adapted to the shared PCM transport.
use crate::assets::{Assets, Result};
use crate::audio::{Music, SAMPLE_RATE};
use crate::audio_timeline::{Change, Completion, MAX_PENDING, Progress, Request, Timeline};
use reforge_sdl::audio::callback::{Output as Device, Source, Spec};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};

struct Status {
    gains: [f32; 2],
    progress: Progress,
    completions: VecDeque<Completion>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            gains: [1.0; 2],
            progress: Progress::default(),
            completions: VecDeque::new(),
        }
    }
}

struct Playback {
    inbox: mpsc::Receiver<Request>,
    timeline: Timeline,
    status: Arc<Mutex<Status>>,
}
impl Source for Playback {
    type Sample = i16;

    fn render(&mut self, output: &mut [i16]) -> Result<()> {
        for _ in 0..MAX_PENDING {
            match self.inbox.try_recv() {
                Ok(request) => self.timeline.schedule(request)?,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        }
        let gains = self.status.lock().map_err(|e| e.to_string())?.gains;
        // Scene themes and battle fanfares use Music; short racing, arena and
        // naval cues use Effects. Both remain one original interrupting score.
        self.timeline.render_with_gain(output, |track| {
            gains[usize::from(
                track.is_some_and(|id| !matches!(id, 0 | 1 | 2 | 4 | 8 | 9 | 10 | 11 | 12 | 13)),
            )]
        })?;
        let mut status = self.status.lock().map_err(|e| e.to_string())?;
        status.progress = self.timeline.progress();
        while let Some(cue) = self.timeline.pop_completion() {
            if status.completions.len() == MAX_PENDING {
                return Err("Audio completion mailbox is full".into());
            }
            status.completions.push_back(cue);
        }
        Ok(())
    }
}

pub struct Output {
    device: Device<Playback>,
    sender: mpsc::SyncSender<Request>,
    status: Arc<Mutex<Status>>,
    prepared: bool,
    resumed: bool,
}
impl Output {
    pub fn progress(&self) -> Result<Progress> {
        Ok(self.status.lock().map_err(|e| e.to_string())?.progress)
    }

    /// The engine retains its sequence-end/explicit-stop semantics. Release cues
    /// only once their source frame has been consumed by the shared transport.
    pub fn take_completions(&mut self, consumed: u64) -> Result<Vec<Completion>> {
        let mut status = self.status.lock().map_err(|e| e.to_string())?;
        let mut ready = Vec::new();
        while status
            .completions
            .front()
            .is_some_and(|cue| cue.frame <= consumed)
        {
            ready.push(status.completions.pop_front().unwrap());
        }
        Ok(ready)
    }

    pub fn is_active(&self) -> bool {
        self.resumed && self.status.lock().is_ok_and(|s| s.progress.playing)
    }

    pub fn new() -> Result<Self> {
        let (sender, inbox) = mpsc::sync_channel(MAX_PENDING);
        let status = Arc::new(Mutex::new(Status::default()));
        let playback = Playback {
            inbox,
            timeline: Timeline::default(),
            status: status.clone(),
        };
        let device = Device::new(
            Spec {
                sample_rate: SAMPLE_RATE,
                channels: 2,
            },
            playback,
        )?;
        Ok(Self {
            device,
            sender,
            status,
            prepared: false,
            resumed: false,
        })
    }

    pub fn set_host_audio(
        &mut self,
        settings: &reforge_settings::AudioSettings,
        focused: bool,
        mute: bool,
    ) -> Result<()> {
        use reforge_settings::SoundCategory::{Effects, Music};
        let master = if mute { 0.0 } else { settings.gain(focused) };
        self.status.lock().map_err(|e| e.to_string())?.gains = [Music, Effects]
            .map(|category| master * f32::from(settings.categories.get(category)) / 100.0);
        Ok(())
    }

    pub fn switch_device(&mut self, name: Option<&str>) -> Result<()> {
        self.device.switch_device(name)
    }

    pub fn clock_frames(&mut self) -> Result<u64> {
        Ok(self.device.position()?.consumed_frames)
    }

    pub fn pump(
        &mut self,
        assets: &Assets,
        commands: Vec<crate::sound::Command>,
        enabled: bool,
        frame: u64,
    ) -> Result<()> {
        if !enabled {
            if self.resumed {
                self.device.pause()?;
            }
            self.resumed = false;
            return Ok(());
        }
        for command in commands {
            // Original resource decoding and first chip construction stay on
            // the game thread; only owned Centurion data crosses the mailbox.
            let change = match command {
                crate::sound::Command::Play(id) if !self.prepared => {
                    let music = Box::new(Music::song(assets, id)?);
                    self.prepared = true;
                    Change::Start { id, music }
                }
                crate::sound::Command::Play(id) => Change::Play {
                    id,
                    song: crate::music::song_data(assets, id)?,
                },
                crate::sound::Command::Stop => Change::Stop,
            };
            self.sender
                .try_send(Request { frame, change })
                .map_err(|e| e.to_string())?;
        }
        if !self.resumed {
            self.device.resume()?;
            self.resumed = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_timeline::CompletionKind;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn sdl_callback_continues_during_main_thread_stalls_and_detaches_safely() {
        // SDL environment is process-global: isolate it rather than mutating the
        // environment underneath parallel Rust tests or the player's process.
        if std::env::var_os("CENTURION_CALLBACK_CHILD").is_none() {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "audio_output::tests::sdl_callback_continues_during_main_thread_stalls_and_detaches_safely", "--nocapture"])
                .env("CENTURION_CALLBACK_CHILD", "1")
                .env("SDL_AUDIODRIVER", "dummy")
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let mut output = Output::new().unwrap();
        output
            .sender
            .try_send(Request {
                frame: 0,
                change: Change::Start {
                    id: 0,
                    music: Box::new(Music::from_sequencer(crate::music::fixture()).unwrap()),
                },
            })
            .unwrap();
        output.device.resume().unwrap();
        output.resumed = true;
        std::thread::sleep(Duration::from_millis(80));
        let before = output.clock_frames().unwrap();
        std::thread::sleep(Duration::from_millis(250));
        let after = output.clock_frames().unwrap();
        assert!(after > before + SAMPLE_RATE as u64 / 5);
        assert!(output.is_active());
        output
            .sender
            .try_send(Request {
                frame: after + 4800,
                change: Change::Stop,
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let deadline = Instant::now() + Duration::from_secs(2);
        let cues = loop {
            let clock = output.clock_frames().unwrap();
            let cues = output.take_completions(clock).unwrap();
            if !cues.is_empty() {
                break cues;
            }
            assert!(Instant::now() < deadline, "missing stop completion");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].kind, CompletionKind::Stopped);
        assert_eq!(cues[0].frame, after + 4800);
        assert!(!output.is_active());
        drop(output);
        // A callback failure is returned to the main thread, without unwinding
        // through C or silently dropping malformed music.
        let mut output = Output::new().unwrap();
        output
            .sender
            .try_send(Request {
                frame: 0,
                change: Change::Play {
                    id: 0,
                    song: Vec::new(),
                },
            })
            .unwrap();
        output.device.resume().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Err(message) = output.clock_frames() {
                assert!(message.contains("Missing audio player"));
                break;
            }
            assert!(Instant::now() < deadline, "missing callback error");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
