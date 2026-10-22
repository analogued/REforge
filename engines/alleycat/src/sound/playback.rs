// Copyright (c) 2026 analogued. Project license has not been selected.
//! Host queue adapter: never changes the game's speaker program or clock.
use super::{
    Sound,
    render::{RATE, Renderer},
};
use reforge_sdl::audio::Playback;
use std::collections::VecDeque;

// Keep several device periods available before starting/resuming the stream.
// Short prefills can underrun as the device pulls audio, making the opening
// notes discontinuous despite correct generated PCM.
const PREFILL_SAMPLES: usize = RATE as usize / 20;

pub struct Player {
    renderer: Renderer,
    host: Playback,
    generation: u64,
    paused: bool,
    failed: bool,
    pending: VecDeque<i16>,
}
impl Player {
    pub fn new(host: Playback) -> Result<Self, String> {
        if let Err(error) = host.start(RATE) {
            eprintln!("REforge: audio unavailable; simulation continues: {error}");
        }
        Ok(Self {
            renderer: Renderer::new(host.settings().categories)?,
            generation: host.generation(),
            host,
            paused: false,
            failed: false,
            pending: VecDeque::new(),
        })
    }
    pub fn pause(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        if paused {
            let _ = self.host.set_paused(true);
        }
    }
    pub fn reset(&mut self) -> Result<(), String> {
        self.host.set_paused(true)?;
        self.host.clear()?;
        self.pending.clear();
        self.renderer = Renderer::new(self.host.settings().categories)?;
        Ok(())
    }
    pub fn service(&mut self, sound: &mut Sound, until: u64) {
        if self.failed {
            sound.drain().for_each(drop);
            return;
        }
        if let Err(error) = self.feed(sound, until) {
            eprintln!("REforge: speaker playback stopped; simulation continues: {error}");
            self.host.stop();
            self.failed = true;
        }
    }
    fn feed(&mut self, sound: &mut Sound, until: u64) -> Result<(), String> {
        let changed = self.generation != self.host.generation();
        // Render old committed work with its old gain, then change levels at
        // that boundary. Discard stale presentation audio after a settings edit.
        let mut pcm = self.renderer.render(sound.drain(), until)?;
        if changed {
            self.renderer.set_volumes(self.host.settings().categories)?;
            self.generation = self.host.generation();
            pcm.clear();
            self.pending.clear();
        }
        if !self.host.settings().enabled || !self.host.available() {
            self.pending.clear();
            return Ok(());
        }
        // A native blocking helper can commit more than one presentation queue
        // of future sound. Feed it progressively while the host catches up to
        // that committed game time, rather than cutting the beginning off.
        self.pending.extend(pcm);
        let overflow = self.pending.len().saturating_sub(RATE as usize * 2);
        self.pending.drain(..overflow);
        if self.paused {
            return Ok(());
        }
        let queued = self.host.queued_samples()?;
        let count = self
            .pending
            .len()
            .min((RATE as usize / 10).saturating_sub(queued));
        let buffer = self.pending.make_contiguous();
        self.host.queue(&buffer[..count])?;
        self.pending.drain(..count);
        if self.host.queued_samples()? >= PREFILL_SAMPLES {
            self.host.set_paused(false)?;
        }
        Ok(())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.host.set_paused(true);
        let _ = self.host.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_prefills_before_device_consumption() {
        const TEST: &str = "sound::playback::tests::startup_prefills_before_device_consumption";
        const MARKER: &str = "REFORGE_ALLEYCAT_AUDIO_TEST";
        if std::env::var(MARKER).as_deref() != Ok(TEST) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--test-threads=1"])
                .env(MARKER, TEST)
                .env("SDL_AUDIO_DRIVER", "dummy")
                .env("SDL_AUDIODRIVER", "dummy")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let host = Playback::new(Default::default());
        let mut player = Player::new(host.clone()).unwrap();
        player.reset().unwrap();
        let mut sound = Sound::default();
        sound.record(true);
        sound.tone(1000, super::super::Category::Music);
        for ms in [20, 40] {
            player.service(&mut sound, reforge_audio::PIT_HZ * ms / 1000);
            let queued = host.queued_samples().unwrap();
            assert!(queued > 480 && queued < PREFILL_SAMPLES);
            std::thread::sleep(std::time::Duration::from_millis(30));
            assert_eq!(
                host.queued_samples().unwrap(),
                queued,
                "short prefill must remain paused"
            );
        }
        player.service(&mut sound, reforge_audio::PIT_HZ * 60 / 1000);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while host.queued_samples().unwrap() >= PREFILL_SAMPLES {
            assert!(
                std::time::Instant::now() < deadline,
                "prefilled stream must start playing"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!player.failed);
    }

    #[test]
    fn blocking_program_survives_queue_budget_and_pause() {
        const TEST: &str =
            "sound::playback::tests::blocking_program_survives_queue_budget_and_pause";
        const MARKER: &str = "REFORGE_ALLEYCAT_AUDIO_TEST";
        // Never initialize a real playback device or alter the test process's
        // SDL environment; the dummy backend lives only in this child.
        if std::env::var(MARKER).as_deref() != Ok(TEST) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--test-threads=1"])
                .env(MARKER, TEST)
                .env("SDL_AUDIO_DRIVER", "dummy")
                .env("SDL_AUDIODRIVER", "dummy")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let host = Playback::new(Default::default());
        let mut player = Player::new(host.clone()).unwrap();
        assert!(host.available());
        let mut data = vec![0; crate::assets::DATA_LENGTH];
        data[0] = 255;
        let mut sound = Sound::default();
        sound.record(true);
        sound.hearts_sweep(&mut data);
        let until = sound.tick();
        player.service(&mut sound, until);
        assert!(!player.failed);
        // Keep at least one further queue budget pending. Its exact duration
        // depends on the guest execution profile, not the buffering contract.
        assert!(player.pending.len() > RATE as usize / 10);
        assert!(host.queued_samples().unwrap() <= RATE as usize / 10);
        player.pause(true);
        let retained = player.pending.len();
        player.service(&mut sound, until);
        assert_eq!(player.pending.len(), retained);
        host.clear().unwrap();
        player.pause(false);
        player.service(&mut sound, until);
        assert_eq!(player.pending.len(), retained - RATE as usize / 10);
        player.reset().unwrap();
        assert!(player.pending.is_empty());
        assert_eq!(host.queued_samples().unwrap(), 0);
    }
}
