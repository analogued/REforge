// Copyright (c) 2026 analogued. Project license has not been selected.
//! Timestamped host playback. Positions are mono input-sample indices on one
//! engine-defined timeline, never wall-clock readings. No synthesis runs here.
use super::{DEFAULT_PREFILL, Playback};
use std::{collections::VecDeque, time::Duration};

/// Conservative host defaults; engines can override for a demonstrated need.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub prefill: Duration,
    pub target: Duration,
    /// Allowed extra lateness beyond normal prefill at startup/reset, or beyond
    /// the playback position established at resume while running.
    pub max_lateness: Duration,
    /// Reject excess pending data without silently truncating future sound.
    pub max_pending: Duration,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            prefill: DEFAULT_PREFILL,
            target: Duration::from_millis(100),
            max_lateness: Duration::from_millis(100),
            max_pending: Duration::from_secs(10),
        }
    }
}
impl Policy {
    /// Honor an endpoint's explicit engine override and accommodate its prefill.
    pub fn for_output(output: &Playback) -> Self {
        let prefill = output.prefill_duration();
        Self {
            prefill,
            target: Self::default().target.max(prefill),
            ..Self::default()
        }
    }
}

/// Small device interface so clocks, remaining cushion and failures can be
/// controlled in tests. Queued input is not an exact audible-device cursor.
pub trait Sink {
    fn queued(&self) -> Result<usize, String>;
    fn submit(&mut self, samples: &[i16]) -> Result<(), String>;
    fn pause(&mut self, paused: bool) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}
impl Sink for Playback {
    fn queued(&self) -> Result<usize, String> {
        self.queued_samples()
    }
    fn submit(&mut self, samples: &[i16]) -> Result<(), String> {
        self.queue(samples)
    }
    fn pause(&mut self, paused: bool) -> Result<(), String> {
        self.set_paused(paused)
    }
    fn clear(&mut self) -> Result<(), String> {
        Playback::clear(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Priming,
    Running,
    Recovering,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    pub submitted: u64,
    pub expired: u64,
    pub recoveries: u64,
    pub resets: u64,
    pub rebases: u64,
}
struct Block {
    start: u64,
    samples: Vec<i16>,
    offset: usize,
}

pub struct Queue {
    prefill: usize,
    target: usize,
    max_lateness: u64,
    max_pending: usize,
    blocks: VecDeque<Block>,
    pending: usize,
    input_end: u64,
    discard_before: u64,
    submitted_end: Option<u64>,
    play_position: Option<u64>,
    presentation: Option<u64>,
    phase: Phase,
    paused: bool,
    stats: Stats,
}
impl Queue {
    pub fn new(rate: u32, policy: Policy) -> Result<Self, String> {
        if rate == 0
            || rate > i32::MAX as u32
            || policy.target.is_zero()
            || policy.prefill > policy.target
            || policy.target > super::MAX_QUEUE
            || policy.max_pending < policy.target
            || policy.max_pending > Duration::from_secs(60)
        {
            return Err("invalid host audio queue policy".into());
        }
        let samples = |duration: Duration| -> Result<usize, String> {
            usize::try_from((u128::from(rate) * duration.as_nanos()).div_ceil(1_000_000_000))
                .map_err(|_| "audio policy sample count overflow".into())
        };
        let target = samples(policy.target)?.min(rate as usize / 4);
        if target == 0 {
            return Err("audio queue target must hold at least one sample".into());
        }
        Ok(Self {
            prefill: samples(policy.prefill)?.min(target),
            target,
            max_lateness: samples(policy.max_lateness)? as u64,
            max_pending: samples(policy.max_pending)?,
            blocks: VecDeque::new(),
            pending: 0,
            input_end: 0,
            discard_before: 0,
            submitted_end: None,
            play_position: None,
            presentation: None,
            phase: Phase::Priming,
            paused: false,
            stats: Stats::default(),
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn pending_samples(&self) -> usize {
        self.pending
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    /// Change the presentation observation without consuming or discarding PCM.
    /// Use only at an explicit engine boundary such as discarding pacing debt
    /// on pause. This does not change the generated-sample epoch or play cursor.
    pub fn rebase(&mut self, position: u64) {
        self.presentation = Some(position);
        self.stats.rebases = self.stats.rebases.saturating_add(1);
    }

    /// Append contiguous generated PCM. Overflow/misordered input is rejected
    /// atomically; a caller must report it rather than silently lose future audio.
    pub fn push(&mut self, start: u64, samples: Vec<i16>) -> Result<(), String> {
        if start != self.input_end {
            return Err("noncontiguous audio input; reset its epoch explicitly".into());
        }
        let end = start
            .checked_add(samples.len() as u64)
            .ok_or("audio position overflow")?;
        let skip = self
            .discard_before
            .saturating_sub(start)
            .min(samples.len() as u64) as usize;
        let retained = samples.len() - skip;
        if retained > self.max_pending.saturating_sub(self.pending) {
            return Err("pending audio capacity exceeded; input was not truncated".into());
        }
        self.input_end = end;
        self.stats.expired = self.stats.expired.saturating_add(skip as u64);
        if retained != 0 {
            self.blocks.push_back(Block {
                start,
                samples,
                offset: skip,
            });
            self.pending += retained;
        }
        Ok(())
    }
    fn trim_before(&mut self, position: u64) {
        self.discard_before = self.discard_before.max(position);
        while let Some(block) = self.blocks.front_mut() {
            let count = position
                .saturating_sub(block.start + block.offset as u64)
                .min((block.samples.len() - block.offset) as u64) as usize;
            block.offset += count;
            self.pending -= count;
            self.stats.expired = self.stats.expired.saturating_add(count as u64);
            if block.offset < block.samples.len() {
                break;
            }
            self.blocks.pop_front();
        }
    }
    /// Clear only presentation audio and establish the next generated sample.
    /// Paused state is retained; the engine separately resets its own timeline.
    pub fn reset(&mut self, sink: &mut impl Sink, epoch: u64) -> Result<(), String> {
        sink.pause(true)?;
        sink.clear()?;
        self.blocks.clear();
        self.pending = 0;
        self.input_end = epoch;
        self.discard_before = epoch;
        self.submitted_end = None;
        self.play_position = None;
        self.presentation = None;
        self.phase = Phase::Priming;
        self.stats.resets = self.stats.resets.saturating_add(1);
        Ok(())
    }
    /// Retain PCM across pause. The engine must freeze its presentation position
    /// during the pause; wall time is never inspected here. A new epoch requires
    /// reset, while an explicit forward timeline skip remains observable.
    pub fn set_paused(&mut self, sink: &mut impl Sink, paused: bool) -> Result<(), String> {
        if self.paused != paused {
            sink.pause(paused || self.phase != Phase::Running)?;
            self.paused = paused;
        }
        Ok(())
    }
    fn fill(&mut self, sink: &mut impl Sink, queued: usize) -> Result<(), String> {
        let mut remaining = self.target.saturating_sub(queued);
        while remaining != 0 {
            let Some(block) = self.blocks.front_mut() else {
                break;
            };
            let count = remaining.min(block.samples.len() - block.offset);
            sink.submit(&block.samples[block.offset..block.offset + count])?;
            block.offset += count;
            self.submitted_end = Some(block.start + block.offset as u64);
            self.pending -= count;
            remaining -= count;
            self.stats.submitted = self.stats.submitted.saturating_add(count as u64);
            if block.offset == block.samples.len() {
                self.blocks.pop_front();
            }
        }
        Ok(())
    }
    /// Service using the engine's presentation position, which may trail its
    /// generated endpoint. Waiting for prefill never blocks simulation/events.
    pub fn service(&mut self, sink: &mut impl Sink, position: u64) -> Result<(), String> {
        if self.paused {
            return Ok(());
        }
        if let Some(previous) = self.presentation {
            let elapsed = position
                .checked_sub(previous)
                .ok_or("audio presentation moved backward without reset")?;
            if let Some(play) = &mut self.play_position {
                *play = play.saturating_add(elapsed);
            }
        }
        self.presentation = Some(position);
        let mut queued = sink.queued()?;
        if self.phase == Phase::Priming {
            let begin = self
                .blocks
                .front()
                .map_or(self.input_end, |block| block.start + block.offset as u64);
            if position.saturating_sub(begin)
                > (self.prefill as u64).saturating_add(self.max_lateness)
            {
                // Startup/reset has no playback anchor yet. Do not adopt a
                // stalled producer's oldest PCM as permanent baseline latency.
                // Keep normal prefill behind presentation, including when
                // catch-up generates the expired prefix in subsequent chunks.
                self.play_position = Some(position.saturating_sub(self.prefill as u64));
                self.stats.recoveries = self.stats.recoveries.saturating_add(1);
                self.phase = Phase::Recovering;
            }
        }
        if self.phase == Phase::Running {
            let oldest = self
                .submitted_end
                .expect("running audio has submitted PCM")
                .saturating_sub(queued as u64);
            let play = self
                .play_position
                .expect("running audio has a presentation anchor");
            if play.saturating_sub(oldest) > self.max_lateness {
                sink.pause(true)?;
                sink.clear()?;
                self.stats.expired = self.stats.expired.saturating_add(queued as u64);
                self.stats.recoveries = self.stats.recoveries.saturating_add(1);
                self.submitted_end = None;
                queued = 0;
                self.phase = Phase::Recovering;
            }
        }
        if self.phase == Phase::Recovering {
            self.trim_before(
                self.play_position
                    .expect("recovery retains playback anchor"),
            );
        }
        if self.phase != Phase::Running {
            if self.pending == 0 || self.pending < self.prefill {
                return Ok(());
            }
            let block = self.blocks.front().expect("pending PCM");
            let begin = block.start + block.offset as u64;
            self.fill(sink, queued)?;
            sink.pause(false)?;
            self.phase = Phase::Running;
            self.play_position = Some(begin);
        } else {
            self.fill(sink, queued)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
