// Copyright (c) 2026 analogued. Project license has not been selected.
//! Bounded adapter for the pinned Rust blip_buf dependency.

use super::{Config, Error, MAX_LEVEL, Output, SignalRenderer};
use blip_buf::{BlipBuf, MAX_FRAME, MAX_RATIO};

// Backend timing precision, checked against the pinned implementation. These
// bounds guard its arithmetic; game timing does not use this conversion factor.
const TIME_UNIT: u128 = 1 << 52;

pub struct BlipRenderer {
    config: Config,
    buffer: BlipBuf,
    frame_start: u64,
    tick: u64,
    level: i16,
    samples_written: u64,
    stopped: bool,
    scratch: Vec<i16>,
}

impl BlipRenderer {
    pub fn new(config: Config, epoch: u64, initial_level: i16) -> Result<Self, Error> {
        if config.clock_hz == 0 || config.sample_rate == 0 || config.chunk_ticks == 0 {
            return Err(Error::Invalid(
                "renderer rates and chunk length must be positive",
            ));
        }
        if config.sample_rate > config.clock_hz
            || u64::from(config.clock_hz) > u64::from(config.sample_rate) * MAX_RATIO
        {
            return Err(Error::Invalid(
                "renderer requires sample_rate <= clock_hz <= sample_rate * MAX_RATIO",
            ));
        }
        check_level(initial_level)?;
        let factor = ((TIME_UNIT as f64) * f64::from(config.sample_rate)
            / f64::from(config.clock_hz))
        .ceil() as u128;
        // Offset is always less than TIME_UNIT after a drained frame. Honor
        // the public 4,000-sample contract independently of backend allocation
        // and prevent u64 multiplication/addition overflow before calling it.
        let bound = u128::from(config.chunk_ticks) * factor + TIME_UNIT - 1;
        if bound > u128::from(u64::MAX) || (bound >> 52) >= u128::from(MAX_FRAME) {
            return Err(Error::Invalid(
                "renderer chunk exceeds blip_buf's synthesis limit",
            ));
        }
        let mut buffer = BlipBuf::new(MAX_FRAME as u32);
        buffer
            .set_rates(f64::from(config.clock_hz), f64::from(config.sample_rate))
            .map_err(Error::Backend)?;
        // set_rates does not update the constructor's default fractional offset.
        // Initialize it once; normal chunk boundaries never clear synthesis.
        buffer.clear();
        if initial_level != 0 {
            buffer
                .add_delta(0, i32::from(initial_level))
                .map_err(Error::Backend)?;
        }
        Ok(Self {
            config,
            buffer,
            frame_start: epoch,
            tick: epoch,
            level: initial_level,
            samples_written: 0,
            stopped: false,
            scratch: vec![0; MAX_FRAME as usize],
        })
    }

    pub fn config(&self) -> Config {
        self.config
    }
    /// Last accepted signal event or committed finish horizon.
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn samples_written(&self) -> u64 {
        self.samples_written
    }

    fn check_time(&self, tick: u64) -> Result<(), Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        if tick < self.tick {
            return Err(Error::Invalid(
                "signal time cannot precede committed renderer time",
            ));
        }
        Ok(())
    }

    fn end_frame(&mut self, ticks: u32, output: &mut Output<'_>) -> Result<(), Error> {
        self.buffer.end_frame(ticks).map_err(Error::Backend)?;
        let available = self.buffer.samples_avail() as usize;
        if available > self.scratch.len() {
            return Err(Error::Backend("unexpected synthesis buffer occupancy"));
        }
        let count = self
            .buffer
            .read_samples(&mut self.scratch[..available], false);
        if count != available {
            return Err(Error::Backend("incomplete synthesis buffer drain"));
        }
        if count != 0 {
            output(&self.scratch[..count])?;
            self.samples_written = self
                .samples_written
                .checked_add(count as u64)
                .ok_or(Error::Invalid("rendered sample count overflow"))?;
        }
        self.frame_start += u64::from(ticks);
        Ok(())
    }

    fn split_until(&mut self, tick: u64, output: &mut Output<'_>) -> Result<(), Error> {
        while tick - self.frame_start >= u64::from(self.config.chunk_ticks) {
            self.end_frame(self.config.chunk_ticks, output)?;
        }
        Ok(())
    }

    fn stop_on_error(&mut self, result: Result<(), Error>) -> Result<(), Error> {
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
}

impl SignalRenderer for BlipRenderer {
    fn level_at(&mut self, tick: u64, level: i16, output: &mut Output<'_>) -> Result<(), Error> {
        self.check_time(tick)?;
        check_level(level)?;
        let result = (|| {
            self.split_until(tick, output)?;
            let delta = i32::from(level) - i32::from(self.level);
            if delta != 0 {
                self.buffer
                    .add_delta((tick - self.frame_start) as u32, delta)
                    .map_err(Error::Backend)?;
            }
            self.level = level;
            self.tick = tick;
            Ok(())
        })();
        self.stop_on_error(result)
    }

    fn finish_until(&mut self, tick: u64, output: &mut Output<'_>) -> Result<(), Error> {
        self.check_time(tick)?;
        let result = (|| {
            self.split_until(tick, output)?;
            let remaining = (tick - self.frame_start) as u32;
            if remaining != 0 {
                self.end_frame(remaining, output)?;
            }
            self.tick = tick;
            Ok(())
        })();
        self.stop_on_error(result)
    }
}

fn check_level(level: i16) -> Result<(), Error> {
    if !(-MAX_LEVEL..=MAX_LEVEL).contains(&level) {
        return Err(Error::Invalid("signal level exceeds synthesis headroom"));
    }
    Ok(())
}
