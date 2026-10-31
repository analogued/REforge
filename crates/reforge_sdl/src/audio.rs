// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Main-thread queued PCM playback, independent of engine clocks.
//!
//! SDL converts the fixed mono signed 16-bit input format to the host device's
//! negotiated format. No application callback runs on the audio thread.

use reforge_settings::AudioSettings;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioStreamOwner};
use std::{cell::RefCell, rc::Rc, time::Duration};
pub mod queue;

/// Initial/resume buffering for host scheduling jitter, independent of game time.
pub const DEFAULT_PREFILL: Duration = Duration::from_millis(50);
const MAX_QUEUE: Duration = Duration::from_millis(250);

fn prefill_samples(rate: u32, prefill: Duration) -> usize {
    // Round up fractional samples, within the endpoint's quarter-second ceiling.
    (u128::from(rate) * prefill.as_nanos())
        .div_ceil(1_000_000_000)
        .min(u128::from(rate / 4)) as usize
}

/// Names are saved instead of transient SDL device instance IDs.
pub fn playback_devices() -> Result<Vec<String>, String> {
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let audio = sdl.audio().map_err(|e| e.to_string())?;
    let mut names = audio
        .audio_playback_device_ids()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|id| id.name().ok())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

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
        Self::on_device(sample_rate, None)
    }
    pub fn on_device(sample_rate: u32, name: Option<&str>) -> Result<Self, String> {
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
        let device = if let Some(name) = name {
            let found = audio
                .audio_playback_device_ids()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|id| id.name().is_ok_and(|n| n == name))
                .collect::<Vec<_>>();
            if found.len() != 1 {
                return Err(format!(
                    "Output device {name:?} is unavailable or its name is ambiguous; choose System default"
                ));
            }
            AudioDevice::new(found[0], audio)
        } else {
            audio.default_playback_device()
        };
        let stream = device
            .open_device_stream(Some(&spec))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            sample_rate,
            max_queued_samples: sample_rate as usize / 4,
            _sdl: sdl,
        })
    }

    pub fn set_gain(&self, gain: f32) -> Result<(), String> {
        self.stream.set_gain(gain).map_err(|e| e.to_string())
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

/// A main-thread playback endpoint shared with the settings/window owner.
/// Engines retain synthesis and timeline mapping. [`queue::Queue`] owns streaming
/// buffering/recovery, with shared defaults and explicit engine overrides.
#[derive(Clone)]
pub struct Playback(Rc<RefCell<PlaybackState>>);
struct PlaybackState {
    settings: AudioSettings,
    output: Option<AudioOutput>,
    rate: Option<u32>,
    focused: bool,
    generation: u64,
    prefill: Duration,
}
impl Playback {
    /// Create an inactive endpoint without initializing SDL audio.
    pub fn new(settings: AudioSettings) -> Self {
        Self(Rc::new(RefCell::new(PlaybackState {
            settings,
            output: None,
            rate: None,
            focused: true,
            generation: 0,
            prefill: DEFAULT_PREFILL,
        })))
    }
    pub fn settings(&self) -> AudioSettings {
        self.0.borrow().settings.clone()
    }
    pub fn generation(&self) -> u64 {
        self.0.borrow().generation
    }
    /// Override startup/resume buffering for an engine with a specific need.
    /// Its queue target must accommodate this duration. This does not change
    /// synthesis, pause state or already-playing audio, and is not a saved option.
    pub fn set_prefill(&self, prefill: Duration) -> Result<(), String> {
        if prefill > MAX_QUEUE {
            return Err("audio prefill exceeds the 250 ms queue ceiling".into());
        }
        self.0.borrow_mut().prefill = prefill;
        Ok(())
    }
    /// Required mono input samples, available after the engine registers its rate.
    pub fn prefill_samples(&self) -> Option<usize> {
        let state = self.0.borrow();
        state.rate.map(|rate| prefill_samples(rate, state.prefill))
    }
    pub fn prefill_duration(&self) -> Duration {
        self.0.borrow().prefill
    }
    /// Resume a primed stream without waiting; false means disabled/unavailable
    /// output or insufficient queued data. Keep servicing it until ready. Engines
    /// decide when startup/recovery may resume, and retain their own pause logic.
    pub fn resume_when_ready(&self) -> Result<bool, String> {
        let mut state = self.0.borrow_mut();
        let Some(rate) = state.rate else {
            return Ok(false);
        };
        let required = prefill_samples(rate, state.prefill);
        let Some(output) = &mut state.output else {
            return Ok(false);
        };
        if output.queued_samples()? < required {
            return Ok(false);
        }
        output.set_paused(false)?;
        Ok(true)
    }
    pub fn available(&self) -> bool {
        self.0.borrow().output.is_some()
    }
    /// Register the engine's PCM format. Startup may fall back to the default
    /// device while retaining a temporarily unavailable named-device preference.
    pub fn start(&self, rate: u32) -> Result<(), String> {
        if rate == 0 || rate > i32::MAX as u32 {
            return Err("invalid audio sample rate".into());
        }

        self.0.borrow_mut().rate = Some(rate);
        let settings = self.settings();
        match self.configure(settings.clone(), true) {
            Err(error) if settings.device.is_some() => {
                eprintln!("REforge: {error}; using system default output");
                let output = AudioOutput::new(rate)?;
                let mut state = self.0.borrow_mut();
                output.set_gain(settings.gain(state.focused))?;
                state.output = Some(output);
                state.generation += 1;
                Ok(())
            }
            result => result,
        }
    }
    /// Failed device creation retains the previous endpoint and preferences.
    pub(crate) fn apply(&self, settings: AudioSettings) -> Result<(), String> {
        self.configure(settings, false)
    }
    fn configure(&self, settings: AudioSettings, force: bool) -> Result<(), String> {
        if settings.volume > 100 || !settings.categories.valid() {
            return Err("audio volumes must be 0..100".into());
        }
        let mut state = self.0.borrow_mut();
        let reopen = state.rate.is_some()
            && settings.enabled
            && (force || !state.settings.enabled || state.settings.device != settings.device);
        let reset = reopen
            || settings.categories != state.settings.categories
            || settings.renderer != state.settings.renderer
            || settings.enabled != state.settings.enabled;
        let candidate = if reopen {
            Some(AudioOutput::on_device(
                state.rate.unwrap(),
                settings.device.as_deref(),
            )?)
        } else {
            None
        };
        let gain = settings.gain(state.focused);
        if let Some(output) = &candidate {
            output.set_gain(gain)?;
        } else if let Some(output) = state.output.as_mut() {
            if reset {
                output.set_paused(true)?;
                output.clear()?;
            }
            output.set_gain(gain)?;
        }
        if let Some(output) = candidate {
            state.output = Some(output);
        }
        if reset {
            state.generation += 1;
        }
        state.settings = settings;
        Ok(())
    }
    pub(crate) fn focus(&self, focused: bool) {
        let mut state = self.0.borrow_mut();
        state.focused = focused;
        if let Some(output) = &state.output {
            if let Err(error) = output.set_gain(state.settings.gain(focused)) {
                eprintln!("REforge: audio focus mute failed: {error}");
                state.output = None;
                state.generation += 1;
            }
        }
    }
    pub fn stop(&self) {
        let mut state = self.0.borrow_mut();
        state.output = None;
        state.generation += 1;
    }
    pub fn queue(&self, samples: &[i16]) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        if !state.settings.enabled {
            return Ok(());
        }
        state.output.as_mut().map_or(Ok(()), |o| o.queue(samples))
    }
    pub fn queued_samples(&self) -> Result<usize, String> {
        self.0
            .borrow()
            .output
            .as_ref()
            .map_or(Ok(0), AudioOutput::queued_samples)
    }
    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state
            .output
            .as_mut()
            .map_or(Ok(()), |o| o.set_paused(paused))
    }
    pub fn clear(&self) -> Result<(), String> {
        self.0
            .borrow_mut()
            .output
            .as_mut()
            .map_or(Ok(()), AudioOutput::clear)
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
    fn prefill_duration_converts_rates_without_exceeding_queue_capacity() {
        assert_eq!(prefill_samples(48_000, DEFAULT_PREFILL), 2400);
        assert_eq!(prefill_samples(44_100, DEFAULT_PREFILL), 2205);
        assert_eq!(prefill_samples(44_100, Duration::from_millis(5)), 221);
        assert_eq!(prefill_samples(11_025, MAX_QUEUE), 2756);
    }

    #[test]
    fn shared_prefill_default_override_and_device_reopen() {
        const TEST: &str = "audio::tests::shared_prefill_default_override_and_device_reopen";
        if std::env::var(CHILD_MARKER).as_deref() != Ok(TEST) {
            run_child(TEST, "dummy");
            return;
        }
        let host = Playback::new(Default::default());
        assert_eq!(host.prefill_samples(), None);
        assert!(!host.resume_when_ready().unwrap());
        host.start(44_100).unwrap();
        assert_eq!(host.prefill_samples(), Some(2205));
        host.queue(&vec![1000; 2204]).unwrap();
        assert!(!host.resume_when_ready().unwrap());
        assert!(
            host.0
                .borrow()
                .output
                .as_ref()
                .unwrap()
                .stream
                .device_paused()
                .unwrap()
        );
        host.queue(&[1000]).unwrap();
        assert!(host.resume_when_ready().unwrap());
        assert!(
            !host
                .0
                .borrow()
                .output
                .as_ref()
                .unwrap()
                .stream
                .device_paused()
                .unwrap()
        );
        host.set_paused(true).unwrap();
        host.clear().unwrap();
        host.set_prefill(Duration::from_millis(5)).unwrap();
        assert!(host.set_prefill(Duration::from_secs(1)).is_err());
        assert_eq!(host.clone().prefill_samples(), Some(221));
        host.queue(&[1000; 220]).unwrap();
        assert!(!host.resume_when_ready().unwrap());
        host.queue(&[1000]).unwrap();
        assert!(host.resume_when_ready().unwrap());
        // Opening another device/rate retains the engine override, recalculated
        // in input samples rather than copied from the previous stream.
        host.start(48_000).unwrap();
        assert_eq!(host.prefill_samples(), Some(240));
        assert!(!host.resume_when_ready().unwrap());
        host.set_prefill(DEFAULT_PREFILL).unwrap();
        assert_eq!(host.prefill_samples(), Some(2400));
        host.apply(AudioSettings {
            enabled: false,
            ..host.settings()
        })
        .unwrap();
        assert!(!host.resume_when_ready().unwrap());
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
    fn shared_playback_gain_focus_device_failure_and_recovery() {
        const TEST: &str = "audio::tests::shared_playback_gain_focus_device_failure_and_recovery";
        if std::env::var(CHILD_MARKER).as_deref() != Ok(TEST) {
            run_child(TEST, "dummy");
            return;
        }
        let mut settings = AudioSettings {
            volume: 40,
            ..Default::default()
        };
        let host = Playback::new(settings.clone());
        host.start(48_000).unwrap();
        let gain = || {
            host.0
                .borrow()
                .output
                .as_ref()
                .unwrap()
                .stream
                .get_gain()
                .unwrap()
        };
        assert_eq!(gain(), 0.4);
        host.queue(&[1000; 32]).unwrap();
        let generation = host.generation();
        host.focus(false);
        assert_eq!(gain(), 0.0);
        assert_eq!(host.queued_samples().unwrap(), 32);
        host.focus(true);
        assert_eq!(gain(), 0.4);
        settings.volume = 50;
        settings.mute_unfocused = false;
        host.focus(false);
        host.apply(settings.clone()).unwrap();
        assert_eq!(gain(), 0.5);
        assert_eq!(host.generation(), generation);
        settings.categories.0[1] = 30;
        host.apply(settings.clone()).unwrap();
        assert_eq!(
            gain(),
            0.5,
            "category volume never changes stream/master gain"
        );
        assert!(host.generation() > generation);
        assert_eq!(host.queued_samples().unwrap(), 0);
        host.queue(&[1000; 32]).unwrap();
        let mut invalid = settings.clone();
        invalid.categories.0[0] = 101;
        assert!(host.apply(invalid).is_err());
        assert_eq!(host.settings(), settings);
        let mut bad = settings.clone();
        bad.device = Some("reforge-nonexistent-output".into());
        assert!(host.apply(bad).is_err());
        assert_eq!(host.settings(), settings);
        assert_eq!(host.queued_samples().unwrap(), 32);
        settings.device = Some(playback_devices().unwrap().into_iter().next().unwrap());
        host.apply(settings.clone()).unwrap();
        assert_eq!(host.queued_samples().unwrap(), 0);
        assert!(
            host.0
                .borrow()
                .output
                .as_ref()
                .unwrap()
                .stream
                .device_paused()
                .unwrap()
        );
        assert!(host.generation() > generation);
        settings.enabled = false;
        host.apply(settings.clone()).unwrap();
        host.queue(&[1000; 32]).unwrap();
        assert_eq!(host.queued_samples().unwrap(), 0);
        assert_eq!(gain(), 0.0);
        settings.enabled = true;
        host.apply(settings.clone()).unwrap();
        assert_eq!(gain(), 0.5);
        host.stop();
        assert!(!host.available());
        host.apply(settings).unwrap(); // unrelated Apply must not require a device
        assert!(!host.available());
        host.start(48_000).unwrap();
        assert!(host.available());
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
