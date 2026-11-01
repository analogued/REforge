// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Procedural sound setup and IRQ0 routines; tone data stays in the executable.

use super::{FarPointer, SampleBus, SoundAction, read, write};
use crate::{
    Error,
    timer::{Clock, ClockEvent, Counters, Interrupt, Stamp, return_interrupt},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    First,
    Second,
    Third,
    Fourth,
}

impl Tone {
    pub const ALL: [Self; 4] = [Self::First, Self::Second, Self::Third, Self::Fourth];
    fn index(self) -> usize {
        self as usize
    }
    pub const fn handler(self) -> u16 {
        [0x7532, 0x761b, 0x773e, 0x7824][self as usize]
    }
    fn priority(self) -> u16 {
        [1, 1, 5, 4][self.index()]
    }
    fn timer_divisor(self) -> u16 {
        [0x9000, 0xd000, 0x2600, 0x2600][self.index()]
    }
    fn last_index(self) -> u16 {
        [15, 45, 12, 40][self.index()]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ramp {
    First,
    Second,
    Third,
}

impl Ramp {
    fn parameters(self) -> (u16, u16, u16, u16, u16) {
        match self {
            Self::First => (0, 0x03a4, 0x06f8, 8, 4),
            Self::Second => (2, 0x0500, 0x0868, 0x30, 7),
            Self::Third => (3, 0x04a0, 0x0568, 4, 3),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Reset,
    Tone(Tone),
    Noise { divisor: u16 },
    Ramp(Ramp),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Handler {
    Tone(Tone),
    Noise,
    Ramp,
}

impl Handler {
    pub const fn offset(self) -> u16 {
        match self {
            Self::Tone(tone) => tone.handler(),
            Self::Noise => 0x79e7,
            Self::Ramp => 0x7a58,
        }
    }
}

/// Four externally supplied divisor tables. Entry zero exists but normal setup
/// starts at one. The terminal entry is read repeatedly, not just once.
#[derive(Clone, Debug)]
pub struct ProceduralProgram {
    tables: [Vec<u16>; 4],
}

impl ProceduralProgram {
    pub fn from_expanded(image: &[u8]) -> Result<Self, Error> {
        let layouts = [
            (0x7541, 0x7583, 17),
            (0x762a, 0x766c, 47),
            (0x774d, 0x778c, 14),
            (0x7833, 0x7872, 42),
        ];
        let mut tables = [const { Vec::new() }; 4];
        for (index, (instruction, base, count)) in layouts.into_iter().enumerate() {
            let anchor = [0xd1, 0xe0, 0x8d, 0x1e, base as u8, (base >> 8) as u8];
            if image.get(instruction..instruction + anchor.len()) != Some(anchor.as_slice()) {
                return Err(invalid("unsupported procedural tone-table addressing"));
            }
            let bytes = image
                .get(base..base + count * 2)
                .ok_or_else(|| invalid("truncated procedural tone table"))?;
            tables[index] = bytes
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
        }
        Ok(Self { tables })
    }

    pub fn divisor(&self, tone: Tone, index: u16) -> Result<u16, Error> {
        self.tables[tone.index()]
            .get(usize::from(index))
            .copied()
            .ok_or_else(|| invalid("tone index is outside the reconstructed table"))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoiseState {
    /// CS:7b0a. Setups reset only this cadence byte, not the persistent seeds.
    pub cadence: u8,
    /// CS:7b0b. Also retained when a ramp is replaced.
    pub ramp_counter: u16,
    /// Bytes CS:7b0d..7b10; word accesses at offsets 0, 1 and 2 overlap.
    pub seed_bytes: [u8; 4],
    pub current: u16,
    pub limit: u16,
    pub step: u16,
    pub group: u16,
}

impl NoiseState {
    fn word(&self, index: usize) -> u16 {
        u16::from_le_bytes([self.seed_bytes[index], self.seed_bytes[index + 1]])
    }
    fn set_word(&mut self, index: usize, value: u16) {
        self.seed_bytes[index..index + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn regenerate(&mut self, mut ax: u16, mask: bool, bus: &mut impl SampleBus) {
        ax ^= self.word(0);
        ax = ax.wrapping_add(0x2345);
        ax ^= self.word(1);
        ax ^= self.word(2);
        if mask {
            ax &= 0xdb6d;
        }
        self.set_word(0, ax);
        ax = ax.wrapping_add(0x4567);
        ax ^= self.word(1); // This read sees the just-updated byte at 7b0e.
        self.set_word(2, ax);
        self.cadence = 0;
        channel_two(bus, 0xb4, 48);
    }
    fn modulate(&mut self, bus: &mut impl SampleBus) {
        let rotated = self.word(0).rotate_left(1);
        self.set_word(0, rotated);
        let control = read(bus, 0x61);
        write(
            bus,
            0x61,
            (control & 0xfd) | (((rotated >> 8) as u8 & 2) | 1),
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProceduralState {
    pub counters: Counters,
    pub priority: u16,
    pub tone_indices: [u16; 4],
    pub noise: NoiseState,
    pub handler: Option<Handler>,
}

impl Default for ProceduralState {
    fn default() -> Self {
        Self {
            counters: Counters::default(),
            priority: 255,
            tone_indices: [0; 4],
            noise: NoiseState::default(),
            handler: None,
        }
    }
}

/// The accepted call's initial state writes precede its device/vector writes.
/// A caller can deliver an intervening IRQ to the previous handler before
/// finishing this continuation. No wall-clock delay is prescribed here.
/// Finish on the same state. Both stages group instructions; interrupt windows
/// within either stage and original setup return registers are not modeled.
#[derive(Debug)]
pub struct PreparedEffect {
    effect: Effect,
}

impl ProceduralState {
    /// Helper 74ad uses unsigned priority; equal requests and ff are accepted.
    fn accept(&mut self, requested: u16) -> bool {
        if requested != 255 && self.priority < requested {
            return false;
        }
        self.priority = requested;
        true
    }

    pub fn prepare(&mut self, effect: Effect) -> Option<PreparedEffect> {
        let priority = match effect {
            Effect::Reset => 255,
            Effect::Tone(tone) => tone.priority(),
            Effect::Noise { .. } => 6,
            Effect::Ramp(ramp) => ramp.parameters().0,
        };
        if !self.accept(priority) {
            return None;
        }
        match effect {
            Effect::Noise { .. } | Effect::Ramp(_) => {
                self.tone_indices[3] = 0;
                self.noise.cadence = 15;
                if let Effect::Ramp(ramp) = effect {
                    let (_, current, limit, step, group) = ramp.parameters();
                    self.noise.current = current;
                    self.noise.limit = limit;
                    self.noise.step = step;
                    self.noise.group = group;
                }
            }
            _ => {}
        }
        Some(PreparedEffect { effect })
    }

    /// Convenience for a caller choosing atomic foreground setup. The original
    /// routines do not bracket ordinary effect setup with CLI/STI.
    pub fn request(
        &mut self,
        effect: Effect,
        game_cs: u16,
        sound_disable_word: u16,
        bus: &mut impl SampleBus,
    ) -> bool {
        let Some(prepared) = self.prepare(effect) else {
            return false;
        };
        prepared.finish(self, game_cs, sound_disable_word, bus);
        true
    }

    fn install(&mut self, handler: Handler, game_cs: u16, bus: &mut impl SampleBus) {
        self.handler = Some(handler);
        bus.emit(SoundAction::SetHandler {
            irq: 0,
            handler: FarPointer {
                segment: game_cs,
                offset: handler.offset(),
            },
        });
    }

    /// Execute an explicitly delivered active gameplay IRQ, including EOI/IRET.
    /// Temporary sample handlers never pass through this common counter helper.
    pub fn timer_interrupt(
        &mut self,
        handler: Handler,
        program: &ProceduralProgram,
        sound_disable_word: u16,
        bus: &mut impl SampleBus,
    ) -> Result<(), Error> {
        if self.handler != Some(handler) {
            return Err(invalid("procedural IRQ does not match installed handler"));
        }
        let ax = self.counters.interrupt(sound_disable_word, bus);
        if ax != 0 {
            match handler {
                Handler::Tone(tone) => {
                    let index = self.tone_indices[tone.index()];
                    if index != 0 {
                        let divisor = program.divisor(tone, index)?;
                        channel_two(bus, 0xb6, divisor);
                        if index as i16 > tone.last_index() as i16 {
                            if matches!(tone, Tone::First | Tone::Second) {
                                speaker(bus, false);
                            }
                            self.accept(255);
                        } else {
                            self.tone_indices[tone.index()] = index.wrapping_add(1);
                        }
                    }
                }
                Handler::Noise | Handler::Ramp => {
                    self.noise.cadence = self.noise.cadence.wrapping_add(1);
                    if self.noise.cadence & 16 != 0 {
                        self.noise.regenerate(ax, handler == Handler::Noise, bus);
                    }
                    if handler == Handler::Ramp {
                        self.noise.ramp_counter = self.noise.ramp_counter.wrapping_add(1);
                        if self.noise.ramp_counter as i16 >= self.noise.group as i16 {
                            self.noise.ramp_counter = 0;
                            self.noise.current = self.noise.current.wrapping_add(self.noise.step);
                            if (self.noise.current as i16) < self.noise.limit as i16 {
                                write(bus, 0x43, 0x36);
                                self.counters.program_divisor(self.noise.current, bus);
                            } else {
                                self.accept(255);
                            }
                        }
                    }
                    self.noise.modulate(bus);
                }
            }
        }
        return_interrupt(bus);
        Ok(())
    }

    /// Dispatch through the device clock; errors stop an entered IRQ permanently.
    pub fn dispatch<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        interrupt: Interrupt,
        program: &ProceduralProgram,
        game_cs: u16,
        sound_disable_word: u16,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        clock.check()?;
        let result = if interrupt.irq == 0 && interrupt.handler.segment == game_cs {
            match self
                .handler
                .filter(|handler| handler.offset() == interrupt.handler.offset)
            {
                Some(handler) => self.timer_interrupt(handler, program, sound_disable_word, clock),
                None => Err(invalid("unknown installed procedural handler")),
            }
        } else {
            Err(invalid("interrupt is outside procedural IRQ0 execution"))
        };
        if let Err(error) = &result {
            clock.fail(error);
        }
        clock.check()?;
        result
    }
}

impl PreparedEffect {
    pub fn finish(
        self,
        state: &mut ProceduralState,
        game_cs: u16,
        sound_disable_word: u16,
        bus: &mut impl SampleBus,
    ) {
        match self.effect {
            Effect::Reset => {
                state.install(Handler::Tone(Tone::First), game_cs, bus);
                write(bus, 0x43, 0x36);
                state.counters.program_divisor(0xffff, bus);
                speaker(bus, false);
            }
            Effect::Tone(tone) => {
                write(bus, 0x43, 0x36);
                state.counters.program_divisor(tone.timer_divisor(), bus);
                state.install(Handler::Tone(tone), game_cs, bus);
                channel_two(bus, 0xb6, 0x0200);
                state.tone_indices[tone.index()] = 1;
                speaker(bus, sound_disable_word == 0);
            }
            Effect::Noise { divisor } => {
                state.install(Handler::Noise, game_cs, bus);
                write(bus, 0x43, 0x36);
                state.counters.program_divisor(divisor, bus);
                speaker(bus, sound_disable_word == 0);
            }
            Effect::Ramp(_) => {
                state.install(Handler::Ramp, game_cs, bus);
                write(bus, 0x43, 0x36);
                state.counters.program_divisor(state.noise.current, bus);
                speaker(bus, sound_disable_word == 0);
            }
        }
    }
}

fn channel_two(bus: &mut impl SampleBus, control: u8, divisor: u16) {
    write(bus, 0x43, control);
    write(bus, 0x42, divisor as u8);
    write(bus, 0x42, (divisor >> 8) as u8);
}
fn speaker(bus: &mut impl SampleBus, enabled: bool) {
    let control = read(bus, 0x61);
    write(
        bus,
        0x61,
        if enabled { control | 3 } else { control & 0xfc },
    );
}
fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}
