// Copyright (c) 2026 analogued. Project license has not been selected.
//! Host synthesis and timestamped audio output; never selects gameplay work.

use crate::{
    sound::render::{AudioRenderer, RenderingPolicy},
    timer::{ClockEvent, Stamp},
};
use reforge_audio::{
    PIT_HZ,
    render::{CategorizedRenderer, Config},
};
use reforge_sdl::{
    audio::{
        Playback,
        queue::{Policy, Queue},
    },
    settings::SoundRenderer,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

pub(super) const RATE: u32 = 48_000;
const SYNTHESIS_CHUNK_TICKS: u64 = 11_931;

pub(super) struct PcmBlock {
    pub(super) start: u64,
    pub(super) samples: Vec<i16>,
}
pub(super) struct Synthesis {
    pub(super) renderer: CategorizedRenderer,
    pub(super) policy: RenderingPolicy,
    pub(super) blocks: VecDeque<PcmBlock>,
    pub(super) generated: u64,
    pub(super) error: Option<String>,
}
impl Synthesis {
    #[cfg(test)]
    pub(super) fn new() -> Result<Self, String> {
        Self::with_mode(AudioRenderer::Speaker)
    }
    pub(super) fn with_mode(mode: AudioRenderer) -> Result<Self, String> {
        Ok(Self {
            renderer: CategorizedRenderer::new(
                Config {
                    clock_hz: PIT_HZ as u32,
                    sample_rate: RATE,
                    chunk_ticks: SYNTHESIS_CHUNK_TICKS as u32,
                },
                0,
            )
            .map_err(|e| e.to_string())?,
            policy: RenderingPolicy::new(mode, false),
            blocks: VecDeque::new(),
            generated: 0,
            error: None,
        })
    }
    pub(super) fn set_mode(&mut self, mode: AudioRenderer, tick: u64) {
        if let Some(level) = self.policy.set_mode(mode) {
            self.advance(tick, Some(level));
            self.blocks.clear();
        }
    }
    pub(super) fn observe(&mut self, stamp: Stamp, event: ClockEvent) {
        let old_category = self.policy.category();
        let level = match event {
            ClockEvent::Action(action) => self.policy.action(stamp.tick, action),
            ClockEvent::Device(events) => events
                .drive
                .and_then(|edge| self.policy.drive(edge.tick, edge.high)),
            ClockEvent::Interrupt(_) => None,
        };
        if let Some(level) = level
            .or_else(|| (old_category != self.policy.category()).then(|| self.renderer.level()))
        {
            self.advance(stamp.tick, Some(level));
        }
    }
    pub(super) fn advance(&mut self, tick: u64, level: Option<i16>) {
        if self.error.is_some() {
            return;
        }
        let Self {
            renderer,
            blocks,
            generated,
            ..
        } = self;
        let mut output = |samples: &[i16]| {
            blocks.push_back(PcmBlock {
                start: *generated,
                samples: samples.to_vec(),
            });
            *generated += samples.len() as u64;
            Ok(())
        };
        let result = if let Some(level) = level {
            renderer.level_at(tick, self.policy.category(), level, &mut output)
        } else {
            renderer.finish_until(tick, &mut output)
        };
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}

pub(super) fn resolve_renderer(renderer: SoundRenderer, default: AudioRenderer) -> AudioRenderer {
    match renderer {
        SoundRenderer::GameDefault => default,
        SoundRenderer::Speaker => AudioRenderer::Speaker,
        SoundRenderer::EnhancedPcm => AudioRenderer::EnhancedPcm,
    }
}

pub(super) struct AudioOutput {
    pub(super) signal: Rc<RefCell<Synthesis>>,
    pub(super) host: Playback,
    pub(super) generation: u64,
    pub(super) queue: Queue,
}
impl AudioOutput {
    pub(super) fn new(signal: Rc<RefCell<Synthesis>>, host: Playback) -> Self {
        if let Err(error) = host.start(RATE) {
            eprintln!("warning: audio output unavailable; simulation continues: {error}");
        }
        Self {
            signal,
            queue: Queue::new(RATE, Policy::for_output(&host))
                .expect("valid fixed-rate playback policy"),
            generation: host.generation(),
            host,
        }
    }
    pub(super) fn sync_settings(&mut self, tick: u64, default_renderer: AudioRenderer) -> bool {
        if self.generation == self.host.generation() {
            return false;
        }
        let mode = resolve_renderer(self.host.settings().renderer, default_renderer);
        let volumes = self.host.settings().categories;
        let mut signal = self.signal.borrow_mut();
        signal.advance(tick, None);
        if let Err(error) = signal.renderer.set_volumes(volumes) {
            eprintln!("warning: invalid category levels: {error}");
        }
        signal.set_mode(mode, tick);
        signal.blocks.clear();
        let epoch = signal.generated;
        drop(signal);
        if let Err(error) = self.queue.reset(&mut self.host, epoch) {
            self.disable(error);
        }
        self.generation = self.host.generation();
        true
    }
    pub(super) fn pause(&mut self, paused: bool) {
        if let Err(error) = self.queue.set_paused(&mut self.host, paused) {
            self.disable(error);
        }
    }
    pub(super) fn disable(&mut self, error: String) {
        eprintln!("warning: audio output stopped; simulation continues: {error}");
        self.host.stop();
    }
    pub(super) fn service(&mut self, committed: u64, wanted: u64) {
        self.signal.borrow_mut().advance(committed, None);
        let mut signal = self.signal.borrow_mut();
        if let Some(error) = &signal.error {
            let error = error.clone();
            signal.blocks.clear();
            drop(signal);
            if self.host.available() {
                self.disable(error);
            }
            return;
        }
        if !self.host.available() {
            signal.blocks.clear();
            return;
        }
        let mut error = None;
        for block in signal.blocks.drain(..) {
            if let Err(e) = self.queue.push(block.start, block.samples) {
                error = Some(e);
                break;
            }
        }
        drop(signal);
        if error.is_none() {
            // Both the generated positions and this target use the native PIT
            // timeline. Paused host time never enters this presentation position.
            error = self.queue.service(&mut self.host, samples_at(wanted)).err();
        }
        if let Some(error) = error {
            self.disable(error);
        }
    }
}

pub(super) fn samples_at(tick: u64) -> u64 {
    (u128::from(tick) * u128::from(RATE) / u128::from(PIT_HZ)) as u64
}
