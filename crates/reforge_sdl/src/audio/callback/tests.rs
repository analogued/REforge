// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

const CHILD: &str = "REFORGE_CALLBACK_TEST_CHILD";
fn child(test: &str, driver: &str) -> bool {
    if std::env::var(CHILD).as_deref() == Ok(test) {
        return false;
    }
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, test)
        .env("SDL_AUDIODRIVER", driver)
        .env("SDL_AUDIO_DRIVER", driver)
        .env("SDL_VIDEODRIVER", "dummy")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "audio callback deadline expired");
        thread::sleep(Duration::from_millis(2));
    }
}

struct Probe<T: Sample> {
    values: [T; 2],
    channels: usize,
    frames: Arc<AtomicU64>,
    drops: Arc<AtomicUsize>,
}
impl<T: Sample> Source for Probe<T> {
    type Sample = T;
    fn render(&mut self, output: &mut [T]) -> Result<(), String> {
        assert_eq!(output.len() % self.channels, 0);
        assert!(!output.is_empty());
        assert!(output.len() <= CHUNK_FRAMES * self.channels);
        for (index, value) in output.iter_mut().enumerate() {
            *value = self.values[index % self.channels];
        }
        self.frames
            .fetch_add((output.len() / self.channels) as u64, Ordering::SeqCst);
        Ok(())
    }
}
impl<T: Sample> Drop for Probe<T> {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn exercise<T: Sample>(rate: u32, channels: u8, values: [T; 2]) {
    let frames = Arc::new(AtomicU64::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = Probe {
        values,
        channels: channels as usize,
        frames: frames.clone(),
        drops: drops.clone(),
    };
    let mut output = Output::new(
        Spec {
            sample_rate: rate,
            channels,
        },
        source,
    )
    .unwrap();
    let (input, _) = output.stream.as_ref().unwrap().get_format().unwrap();
    let input = input.unwrap();
    assert_eq!(input.freq, Some(rate as i32));
    assert_eq!(input.channels, Some(channels as i32));
    assert_eq!(input.format, Some(T::FORMAT));
    thread::sleep(Duration::from_millis(25));
    assert_eq!(output.position().unwrap(), Position::default());
    output.resume().unwrap();
    until(|| output.position().unwrap().generated_frames > 0);
    output.pause().unwrap();
    let paused = output.position().unwrap();
    assert_eq!(frames.load(Ordering::SeqCst), paused.generated_frames);
    assert_eq!(
        paused.generated_frames - paused.queued_frames,
        paused.consumed_frames
    );
    thread::sleep(Duration::from_millis(40));
    assert_eq!(output.position().unwrap(), paused);
    output.resume().unwrap();
    // No servicing of the endpoint/window thread during this interval.
    thread::sleep(Duration::from_millis(250));
    output.pause().unwrap();
    let advanced = output.position().unwrap();
    assert!(advanced.consumed_frames > paused.consumed_frames + u64::from(rate) / 8);
    assert_eq!(advanced.generated_frames, frames.load(Ordering::SeqCst));
    assert!(advanced.consumed_frames <= advanced.generated_frames);
    drop(output);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn formats_channels_positions_and_pause_work_without_main_thread_refills() {
    const TEST: &str = "audio::callback::tests::formats_channels_positions_and_pause_work_without_main_thread_refills";
    if child(TEST, "dummy") {
        return;
    }
    exercise(8000, 1, [123i16, -123]);
    exercise(44100, 2, [123i16, -123]);
    exercise(48000, 2, [0.25f32, -0.5]);
}

struct Fault {
    panic: bool,
    calls: Arc<AtomicUsize>,
}
impl Source for Fault {
    type Sample = i16;
    fn render(&mut self, _: &mut [i16]) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic, "authored callback panic");
        Err("authored callback failure".into())
    }
}
#[test]
fn source_errors_and_panics_are_reported_and_latched_without_ffi_unwinding() {
    const TEST: &str = "audio::callback::tests::source_errors_and_panics_are_reported_and_latched_without_ffi_unwinding";
    if child(TEST, "dummy") {
        return;
    }
    for panic in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut output = Output::new(
            Spec {
                sample_rate: 48000,
                channels: 2,
            },
            Fault {
                panic,
                calls: calls.clone(),
            },
        )
        .unwrap();
        output.resume().unwrap();
        let mut message = None;
        until(|| {
            message = output.position().err();
            message.is_some()
        });
        let message = message.unwrap();
        assert!(message.contains(if panic {
            "source panicked"
        } else {
            "authored callback failure"
        }));
        thread::sleep(Duration::from_millis(30));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(output.resume().unwrap_err(), message);
        assert_eq!(output.pause().unwrap_err(), message);
    }
}

struct Slow {
    started: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}
impl Source for Slow {
    type Sample = f32;
    fn render(&mut self, output: &mut [f32]) -> Result<(), String> {
        self.started.store(true, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(30));
        output.fill(0.0);
        self.finished.store(true, Ordering::SeqCst);
        Ok(())
    }
}
impl Drop for Slow {
    fn drop(&mut self) {
        assert!(self.finished.load(Ordering::SeqCst));
        self.dropped.store(true, Ordering::SeqCst);
    }
}
#[test]
fn active_callback_detaches_before_source_drop_and_preserves_other_sdl_owners() {
    const TEST: &str = "audio::callback::tests::active_callback_detaches_before_source_drop_and_preserves_other_sdl_owners";
    if child(TEST, "dummy") {
        return;
    }
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    for _ in 0..4 {
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut output = Output::new(
            Spec {
                sample_rate: 48000,
                channels: 2,
            },
            Slow {
                started: started.clone(),
                finished,
                dropped: dropped.clone(),
            },
        )
        .unwrap();
        output.resume().unwrap();
        until(|| started.load(Ordering::SeqCst));
        drop(output);
        assert!(dropped.load(Ordering::SeqCst));
        // The output's SDL owner is not the final owner: video remains usable.
        let window = video
            .window("callback owner test", 16, 16)
            .hidden()
            .build()
            .unwrap();
        drop(window);
    }
}

#[test]
fn invalid_specs_fail_before_audio_initialization() {
    for spec in [
        Spec {
            sample_rate: 0,
            channels: 1,
        },
        Spec {
            sample_rate: u32::MAX,
            channels: 2,
        },
        Spec {
            sample_rate: 48000,
            channels: 0,
        },
        Spec {
            sample_rate: 48000,
            channels: 3,
        },
    ] {
        let source = Fault {
            panic: false,
            calls: Arc::default(),
        };
        assert!(Output::new(spec, source).is_err());
    }
}
#[test]
fn unavailable_audio_is_returned_as_an_error() {
    const TEST: &str = "audio::callback::tests::unavailable_audio_is_returned_as_an_error";
    if child(TEST, "reforge-invalid-driver") {
        return;
    }
    assert!(
        Output::new(
            Spec {
                sample_rate: 48000,
                channels: 1
            },
            Fault {
                panic: false,
                calls: Arc::default()
            }
        )
        .is_err()
    );
}
