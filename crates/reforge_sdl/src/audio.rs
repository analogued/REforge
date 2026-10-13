// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Main-thread queued PCM playback, independent of engine clocks.
//!
//! SDL converts the fixed mono signed 16-bit input format to the host device's
//! negotiated format. No application callback runs on the audio thread.

use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};

/// Owns a default playback stream and the SDL context that keeps it alive.
/// Construct, use and drop on the main thread, like [`crate::Window`].
///
/// Playback starts paused. Queue a small initial buffer before resuming it.
/// At most 250 milliseconds of input can be queued; callers choose their own
/// smaller target. On overflow, queueing returns an error without changing the
/// stream. The caller can explicitly [`Self::clear`] stale audio and queue fresh
/// samples. Queueing never waits for playback or advances an engine clock.
pub struct AudioOutput {
    // Drop the stream (and its device) before the last SDL context.
    stream: AudioStreamOwner,
    sample_rate: u32,
    max_queued_samples: usize,
    _sdl: sdl3::Sdl,
}

impl AudioOutput {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        let rate = i32::try_from(sample_rate)
            .ok()
            .filter(|rate| *rate > 0)
            .ok_or("audio sample rate must be positive and fit in a signed 32-bit integer")?;
        let sdl = sdl3::init().map_err(|error| error.to_string())?;
        let audio = sdl.audio().map_err(|error| error.to_string())?;
        let spec = AudioSpec {
            freq: Some(rate),
            channels: Some(1),
            format: Some(AudioFormat::s16_sys()),
        };
        // The default device ID opens a single stream-owned logical device.
        // AudioStreamOwner retains the audio subsystem and closes the device
        // when dropped. Its spec describes our input, not a required host format.
        let stream = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            sample_rate,
            max_queued_samples: sample_rate as usize / 4,
            _sdl: sdl,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Hard upper bound in mono input samples, independent of host format.
    pub fn max_queued_samples(&self) -> usize {
        self.max_queued_samples
    }

    /// Copy PCM samples into SDL's queue, rejecting a buffer over the limit.
    /// Rejection retains existing audio; recovery and expired-sample decisions
    /// belong to the caller. SDL owns the copied bytes after this returns.
    pub fn queue(&mut self, samples: &[i16]) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        let queued = self.queued_samples()?;
        if samples.len() > self.max_queued_samples.saturating_sub(queued) {
            return Err(format!(
                "audio queue limit exceeded: {queued} queued + {} new samples, maximum {}",
                samples.len(),
                self.max_queued_samples
            ));
        }
        // The quarter-second limit at any accepted i32 rate also ensures the
        // wrapper's signed 32-bit byte length cannot overflow.
        self.stream
            .put_data_i16(samples)
            .map_err(|error| error.to_string())
    }

    /// Remaining unconverted input samples, rather than host-device bytes.
    /// SDL may already have consumed samples into a device buffer; this is not
    /// an exact measure of sound still audible at the speakers.
    pub fn queued_samples(&self) -> Result<usize, String> {
        self.stream
            .queued_bytes()
            .map(|bytes| bytes as usize / size_of::<i16>())
            .map_err(|error| error.to_string())
    }

    /// Pausing retains queued data. Clear explicitly when it has expired.
    pub fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        if paused {
            self.stream.pause()
        } else {
            self.stream.resume()
        }
        .map_err(|error| error.to_string())
    }

    /// Discard all pending stream data, retaining the current pause state.
    /// Audio already handed to the host device cannot be recalled.
    pub fn clear(&mut self) -> Result<(), String> {
        self.stream.clear().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    const CHILD_MARKER: &str = "REFORGE_AUDIO_TEST_CHILD";

    // Configure SDL only in fresh child processes: ordinary tests never open
    // host audio and never mutate environment shared with renderer tests.
    fn run_child(test: &str, driver: &str) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env(CHILD_MARKER, test)
            .env("SDL_AUDIODRIVER", driver)
            .env("SDL_AUDIO_DRIVER", driver)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "audio test child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn invalid_rates_fail_before_initializing_sdl() {
        assert!(AudioOutput::new(0).is_err());
        assert!(AudioOutput::new(u32::MAX).is_err());
    }

    #[test]
    fn dummy_audio_queue_pause_clear_and_lifetimes() {
        const TEST: &str = "audio::tests::dummy_audio_queue_pause_clear_and_lifetimes";
        if std::env::var(CHILD_MARKER).as_deref() != Ok(TEST) {
            run_child(TEST, "dummy");
            return;
        }
        let sdl = sdl3::init().unwrap();
        for _ in 0..2 {
            let mut output = AudioOutput::new(44_100).unwrap();
            assert_eq!(output.sample_rate(), 44_100);
            assert_eq!(output.max_queued_samples(), 11_025);
            assert!(output.stream.device_paused().unwrap());
            let (source, destination) = output.stream.get_format().unwrap();
            let source = source.unwrap();
            assert_eq!(source.freq, Some(44_100));
            assert_eq!(source.channels, Some(1));
            assert_eq!(source.format, Some(AudioFormat::s16_sys()));
            assert!(destination.is_some());
            assert_eq!(output.queued_samples().unwrap(), 0);
            output.queue(&[]).unwrap();
            let samples = vec![1_000; output.max_queued_samples()];
            output.queue(&samples).unwrap();
            assert_eq!(output.queued_samples().unwrap(), samples.len());
            assert!(output.queue(&[1]).is_err());
            assert_eq!(output.queued_samples().unwrap(), samples.len());
            thread::sleep(Duration::from_millis(30));
            assert_eq!(output.queued_samples().unwrap(), samples.len());
            output.set_paused(false).unwrap();
            assert!(!output.stream.device_paused().unwrap());
            let deadline = Instant::now() + Duration::from_secs(2);
            while output.queued_samples().unwrap() == samples.len() {
                assert!(Instant::now() < deadline, "dummy playback did not progress");
                thread::sleep(Duration::from_millis(5));
            }
            output.set_paused(true).unwrap();
            let paused_samples = output.queued_samples().unwrap();
            thread::sleep(Duration::from_millis(30));
            assert_eq!(output.queued_samples().unwrap(), paused_samples);
            output.clear().unwrap();
            assert_eq!(output.queued_samples().unwrap(), 0);
            assert!(output.stream.device_paused().unwrap());
            assert!(output.queue(&vec![0; samples.len() + 1]).is_err());
            assert_eq!(output.queued_samples().unwrap(), 0);
            output.queue(&samples[..32]).unwrap();
            assert_eq!(output.queued_samples().unwrap(), 32);
        }
        // Dropping/reopening AudioOutput leaves another SDL owner usable.
        let audio = sdl.audio().unwrap();
        assert_eq!(audio.current_audio_driver(), "dummy");
    }

    #[test]
    fn unavailable_audio_backend_returns_error() {
        const TEST: &str = "audio::tests::unavailable_audio_backend_returns_error";
        if std::env::var(CHILD_MARKER).as_deref() != Ok(TEST) {
            run_child(TEST, "reforge-no-such-audio-driver");
            return;
        }
        let error = match AudioOutput::new(48_000) {
            Ok(_) => panic!("unavailable audio driver unexpectedly opened"),
            Err(error) => error,
        };
        assert!(!error.is_empty());
    }
}
