// Copyright (c) 2026 analogued. Project license has not been selected.
//! Host queue adapter: never changes the game's speaker program or clock.
use super::{
    Sound,
    render::{RATE, Renderer},
};
use reforge_sdl::audio::{
    Playback,
    queue::{Policy, Queue},
};

pub struct Player {
    renderer: Renderer,
    host: Playback,
    generation: u64,
    failed: bool,
    queue: Queue,
    generated: u64,
}
impl Player {
    pub fn new(host: Playback) -> Result<Self, String> {
        if let Err(error) = host.start(RATE) {
            eprintln!("REforge: audio unavailable; simulation continues: {error}");
        }
        let queue = Queue::new(RATE, Policy::for_output(&host))?;
        Ok(Self {
            renderer: Renderer::new(host.settings().categories)?,
            generation: host.generation(),
            host,
            failed: false,
            queue,
            generated: 0,
        })
    }
    pub fn pause(&mut self, paused: bool) {
        if let Err(error) = self.queue.set_paused(&mut self.host, paused) {
            self.fail(error);
        }
    }
    pub fn rebase_presentation(&mut self, nanos: u128) {
        self.queue.rebase(presentation_sample(nanos));
    }
    fn fail(&mut self, error: String) {
        eprintln!("REforge: speaker playback stopped; simulation continues: {error}");
        self.host.stop();
        self.failed = true;
    }
    pub fn reset(&mut self) -> Result<(), String> {
        self.queue.reset(&mut self.host, 0)?;
        self.generated = 0;
        self.renderer = Renderer::new(self.host.settings().categories)?;
        Ok(())
    }
    pub fn service(&mut self, sound: &mut Sound, until: u64, presentation_nanos: u128) {
        if self.failed {
            sound.drain().for_each(drop);
            return;
        }
        if let Err(error) = self.feed(sound, until, presentation_nanos) {
            self.fail(error);
        }
    }
    fn feed(
        &mut self,
        sound: &mut Sound,
        until: u64,
        presentation_nanos: u128,
    ) -> Result<(), String> {
        let changed = self.generation != self.host.generation();
        // Synthesis always executes all committed device work, including work
        // whose host presentation is muted, reset or expired.
        let start = self.generated;
        let pcm = self.renderer.render(sound.drain(), until)?;
        self.generated = self
            .generated
            .checked_add(pcm.len() as u64)
            .ok_or("audio position overflow")?;
        if changed {
            self.renderer.set_volumes(self.host.settings().categories)?;
            self.generation = self.host.generation();
            self.queue.reset(&mut self.host, self.generated)?;
        }
        if changed || !self.host.settings().enabled || !self.host.available() {
            return Ok(());
        }
        self.queue.push(start, pcm)?;
        // The runner supplies its paced virtual target, not the possibly-ahead
        // end of a blocking helper. Its stall clamp and pause mapping remain
        // engine policy; no discarded host time enters this sample position.
        let position = presentation_sample(presentation_nanos);
        self.queue.service(&mut self.host, position)
    }
}
fn presentation_sample(nanos: u128) -> u64 {
    (nanos.saturating_mul(u128::from(RATE)) / 1_000_000_000).min(u128::from(u64::MAX)) as u64
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
        let prefill = host.prefill_samples().unwrap();
        let mut sound = Sound::default();
        sound.record(true);
        sound.tone(1000, super::super::Category::Music);
        for ms in [20, 40] {
            player.service(
                &mut sound,
                reforge_audio::PIT_HZ * ms / 1000,
                u128::from(ms) * 1_000_000,
            );
            let queued = host.queued_samples().unwrap();
            assert_eq!(queued, 0);
            assert!(
                player.queue.pending_samples() > 480 && player.queue.pending_samples() < prefill
            );
            std::thread::sleep(std::time::Duration::from_millis(30));
            assert_eq!(
                host.queued_samples().unwrap(),
                queued,
                "short prefill must remain paused"
            );
        }
        player.service(&mut sound, reforge_audio::PIT_HZ * 60 / 1000, 60_000_000);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while host.queued_samples().unwrap() >= prefill {
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
        sound.tone(1000, super::super::Category::Music);
        sound.at(reforge_audio::PIT_HZ * 3);
        sound.stop();
        let until = sound.tick();
        player.service(&mut sound, until, 0);
        assert!(!player.failed);
        // Keep at least one further queue budget pending. Its exact duration
        // depends on the guest execution profile, not the buffering contract.
        assert!(player.queue.pending_samples() > RATE as usize * 2);
        assert!(host.queued_samples().unwrap() <= RATE as usize / 10);
        assert_eq!(player.queue.stats().expired, 0);
        assert_eq!(
            player.queue.pending_samples() as u64 + player.queue.stats().submitted,
            player.generated
        );
        player.pause(true);
        let retained = player.queue.pending_samples();
        player.service(&mut sound, until, 0);
        assert_eq!(player.queue.pending_samples(), retained);
        host.clear().unwrap();
        player.pause(false);
        player.service(&mut sound, until, 0);
        assert_eq!(
            player.queue.pending_samples(),
            retained - RATE as usize / 10
        );
        player.reset().unwrap();
        assert!(player.queue.pending_samples() == 0);
        assert_eq!(host.queued_samples().unwrap(), 0);
    }
}
