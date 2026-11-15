// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Demand-driven PCM playback, independent of engine clocks and sequencing.
//!
//! The endpoint and SDL lifetimes stay on the main thread. SDL's callback owns
//! the PCM source. Engines retain their commands, clocks and completion cues.

use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::sys::audio::{self as raw, SDL_AudioStream};
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

const CHUNK_FRAMES: usize = 1024;
mod sealed {
    pub trait Sealed {}
    impl Sealed for i16 {}
    impl Sealed for f32 {}
}
/// Supported native-endian PCM types. Sealed so SDL format and byte size agree.
pub trait Sample: sealed::Sealed + Copy + Default + Send + 'static {
    const FORMAT: AudioFormat;
}
impl Sample for i16 {
    const FORMAT: AudioFormat = AudioFormat::s16_sys();
}
impl Sample for f32 {
    const FORMAT: AudioFormat = AudioFormat::f32_sys();
}

/// Owned PCM production, serialized by SDL's callback lock.
///
/// Fill the entire buffer with complete interleaved frames, according to
/// [`Spec::channels`]. Avoid blocking, IO and unbounded work. Errors stop further
/// production and are reported by the endpoint. Sources own sequencing, mixing,
/// completion cues and any mailbox communicating with game state.
pub trait Source: Send + 'static {
    type Sample: Sample;
    fn render(&mut self, output: &mut [Self::Sample]) -> Result<(), String>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Spec {
    /// Fixed PCM rate, 8,000 through 192,000 Hz. SDL converts as needed.
    pub sample_rate: u32,
    /// One or two interleaved values per sample frame.
    pub channels: u8,
}
impl Spec {
    fn validate(self) -> Result<Self, String> {
        if !(8000..=192000).contains(&self.sample_rate) {
            return Err("callback audio sample rate must be 8000..192000 Hz".into());
        }
        if !matches!(self.channels, 1 | 2) {
            return Err("callback audio channels must be one or two".into());
        }
        Ok(self)
    }
    fn frame_bytes<T: Sample>(self) -> usize {
        usize::from(self.channels) * size_of::<T>()
    }
}
/// Positions count complete source sample frames, independent of channel count
/// and sample type. A frame contains one value per channel.
///
/// Consumption is generated minus queued input, not a DAC/speaker cursor. SDL
/// may already have converted/passed data to the device; device latency is excluded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Position {
    pub generated_frames: u64,
    pub queued_frames: u64,
    pub consumed_frames: u64,
}
#[derive(Default)]
struct Status {
    generated: u64,
    error: Option<String>,
}
struct State<S: Source> {
    source: S,
    spec: Spec,
    scratch: Box<[S::Sample]>,
    status: Arc<Mutex<Status>>,
}
impl<S: Source> State<S> {
    fn fill(&mut self, stream: *mut SDL_AudioStream, bytes: i32) -> Result<(), String> {
        if self
            .status
            .lock()
            .map_err(|e| e.to_string())?
            .error
            .is_some()
        {
            return Ok(());
        }
        let frame_bytes = self.spec.frame_bytes::<S::Sample>();
        let channels = usize::from(self.spec.channels);
        let mut remaining = (bytes.max(0) as usize).div_ceil(frame_bytes);
        while remaining != 0 {
            let frames = remaining.min(CHUNK_FRAMES);
            let pcm = &mut self.scratch[..frames * channels];
            self.source.render(pcm)?;
            // SAFETY: SDL serializes this live stream's callback. The sealed PCM
            // type agrees with its input spec; SDL copies initialized full frames.
            if !unsafe {
                raw::SDL_PutAudioStreamData(
                    stream,
                    pcm.as_ptr().cast(),
                    (frames * frame_bytes) as i32,
                )
            } {
                return Err(sdl3::get_error().to_string());
            }
            let mut status = self.status.lock().map_err(|e| e.to_string())?;
            status.generated = status
                .generated
                .checked_add(frames as u64)
                .ok_or("callback audio position overflow")?;
            remaining -= frames;
        }
        Ok(())
    }
}
unsafe extern "C" fn callback<S: Source>(
    userdata: *mut c_void,
    stream: *mut SDL_AudioStream,
    additional: i32,
    _: i32,
) {
    // SAFETY: Output retains this stable allocation until unregistering waits for
    // in-flight calls. SDL serializes invocations; only callbacks touch the source.
    let state = unsafe { &mut *userdata.cast::<State<S>>() };
    let failure = match catch_unwind(AssertUnwindSafe(|| state.fill(stream, additional))) {
        Ok(Ok(())) => return,
        Ok(Err(message)) => message,
        Err(_) => "callback audio source panicked".into(),
    };
    // Do not unwind across C or invoke a failed/panicked source again.
    if let Ok(mut status) = state.status.lock() {
        status.error.get_or_insert(failure);
    }
}
// Open a separately owned logical device, paused before any stream is bound.
fn open_device(
    audio: &sdl3::AudioSubsystem,
    input: &AudioSpec,
    name: Option<&str>,
) -> Result<AudioDevice, String> {
    let selected = if let Some(name) = name {
        let found = audio
            .audio_playback_device_ids()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|id| id.name().is_ok_and(|candidate| candidate == name))
            .collect::<Vec<_>>();
        if found.len() != 1 {
            return Err(format!(
                "Output device {name:?} is unavailable or its name is ambiguous; choose System default"
            ));
        }
        Some(found[0])
    } else {
        None
    };
    let device =
        AudioDevice::open_playback(audio, selected.as_ref(), input).map_err(|e| e.to_string())?;
    if !device.pause() {
        return Err(sdl3::get_error().to_string());
    }
    Ok(device)
}
/// Main-thread-owned callback endpoint. It starts paused; [`Self::resume`] starts
/// production. Pausing retains source/queued data. Engines choose clock policies.
pub struct Output<S: Source> {
    stream: Option<AudioStreamOwner>,
    device: AudioDevice,
    paused: bool,
    _state: Box<State<S>>,
    status: Arc<Mutex<Status>>,
    spec: Spec,
    consumed: u64,
    _sdl: sdl3::Sdl,
}
impl<S: Source> Output<S> {
    pub fn new(spec: Spec, source: S) -> Result<Self, String> {
        Self::open(spec, source, None)
    }
    /// Open the system default, or the playback device with this exact unique
    /// name. A missing or ambiguous name is an error; it never silently falls back.
    pub fn open(spec: Spec, source: S, device_name: Option<&str>) -> Result<Self, String> {
        let spec = spec.validate()?;
        let sdl = sdl3::init().map_err(|e| e.to_string())?;
        let audio = sdl.audio().map_err(|e| e.to_string())?;
        let input = AudioSpec {
            freq: Some(spec.sample_rate as i32),
            channels: Some(i32::from(spec.channels)),
            format: Some(S::Sample::FORMAT),
        };
        let device = open_device(&audio, &input, device_name)?;
        // Independent ownership lets this stream retain its input queue when
        // changing logical devices. SDL_OpenAudioDeviceStream ties them together.
        let stream = audio
            .new_playback_stream(&input, None)
            .map_err(|e| e.to_string())?;
        device.bind_stream(&stream).map_err(|e| e.to_string())?;
        let status = Arc::new(Mutex::new(Status::default()));
        let mut state = Box::new(State {
            source,
            spec,
            scratch: vec![S::Sample::default(); CHUNK_FRAMES * usize::from(spec.channels)]
                .into_boxed_slice(),
            status: status.clone(),
        });
        // SAFETY: stream starts paused. Boxed userdata stays live through callback
        // detachment and stream destruction, including early error paths.
        if !unsafe {
            raw::SDL_SetAudioStreamGetCallback(
                stream.raw(),
                Some(callback::<S>),
                (&mut *state as *mut State<S>).cast(),
            )
        } {
            return Err(sdl3::get_error().to_string());
        }
        Ok(Self {
            stream: Some(stream),
            device,
            paused: true,
            _state: state,
            status,
            spec,
            consumed: 0,
            _sdl: sdl,
        })
    }
    fn check_error(&self) -> Result<(), String> {
        let status = self.status.lock().map_err(|e| e.to_string())?;
        match &status.error {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }
    pub fn resume(&mut self) -> Result<(), String> {
        self.check_error()?;
        if !self.device.resume() {
            return Err(sdl3::get_error().to_string());
        }
        self.paused = false;
        Ok(())
    }
    pub fn pause(&mut self) -> Result<(), String> {
        if !self.device.pause() {
            return Err(sdl3::get_error().to_string());
        }
        self.paused = true;
        self.check_error()
    }
    /// Move the existing source and queued PCM to another playback device.
    /// Paused/running state and frame counters are retained. Opening or selecting
    /// an unavailable device leaves the current device playing as before.
    pub fn switch_device(&mut self, device_name: Option<&str>) -> Result<(), String> {
        self.check_error()?;
        let audio = self._sdl.audio().map_err(|e| e.to_string())?;
        let input = AudioSpec {
            freq: Some(self.spec.sample_rate as i32),
            channels: Some(i32::from(self.spec.channels)),
            format: Some(S::Sample::FORMAT),
        };
        let replacement = open_device(&audio, &input, device_name)?;
        self.rebind(replacement)
    }
    fn rebind(&mut self, replacement: AudioDevice) -> Result<(), String> {
        // Validate running state before interrupting the working endpoint. The
        // replacement has no bound stream yet and cannot invoke the source.
        if !self.paused && !replacement.resume() {
            return Err(sdl3::get_error().to_string());
        }
        if !self.device.pause() {
            return Err(sdl3::get_error().to_string());
        }
        let stream = self.stream.as_ref().unwrap();
        // SAFETY: this independently owned live stream can be rebound. Device
        // operations stay outside its callback lock to avoid lock inversion.
        unsafe { raw::SDL_UnbindAudioStream(stream.raw()) };
        if let Err(error) = replacement.bind_stream(stream) {
            // Keep the original logical device alive until the transfer commits.
            // SDL does not provide atomic rebinding, so restore it on failure.
            if let Err(restore_error) = self.device.bind_stream(stream) {
                self.paused = true;
                return Err(format!(
                    "{error}; restoring output device failed: {restore_error}"
                ));
            }
            if !self.paused && !self.device.resume() {
                self.paused = true;
                return Err(format!(
                    "{error}; resuming output device failed: {}",
                    sdl3::get_error()
                ));
            }
            return Err(error.to_string());
        }
        self.device = replacement;
        Ok(())
    }
    /// Coherent source-generated/queued snapshot, serialized with the callback.
    /// Estimated consumption is monotonic; pause freezes generation/consumption.
    pub fn position(&mut self) -> Result<Position, String> {
        let stream = self.stream.as_ref().unwrap().raw();
        // SAFETY: live owned stream; its lock serializes the byte query/counter
        // snapshot with production. Only stream-local operations run while held,
        // avoiding stream/device lock inversion.
        unsafe {
            if !raw::SDL_LockAudioStream(stream) {
                return Err(sdl3::get_error().to_string());
            }
            let result = (|| {
                let queued = raw::SDL_GetAudioStreamQueued(stream);
                if queued < 0 {
                    return Err(sdl3::get_error().to_string());
                }
                let status = self.status.lock().map_err(|e| e.to_string())?;
                if let Some(error) = &status.error {
                    return Err(error.clone());
                }
                let queued_frames = queued as u64 / self.spec.frame_bytes::<S::Sample>() as u64;
                Ok(Position {
                    generated_frames: status.generated,
                    queued_frames,
                    consumed_frames: status.generated.saturating_sub(queued_frames),
                })
            })();
            raw::SDL_UnlockAudioStream(stream);
            let mut position = result?;
            self.consumed = self.consumed.max(position.consumed_frames);
            position.consumed_frames = self.consumed;
            Ok(position)
        }
    }
}
impl<S: Source> Drop for Output<S> {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            // SAFETY: detachment waits for in-flight calls while userdata is live.
            // Drop the stream/device before the source or SDL context is released.
            unsafe {
                raw::SDL_SetAudioStreamGetCallback(stream.raw(), None, std::ptr::null_mut());
            }
            drop(stream);
        }
    }
}
#[cfg(test)]
mod tests;
