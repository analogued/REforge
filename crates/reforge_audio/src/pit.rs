// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Binary Intel 8254 modes 0–3. This is an explicit subset, not a complete PIT.
//!
//! Integer timestamps represent completed CLK pulses. Writes at time t occur
//! after that pulse; a pending load occurs at t+1 without decrementing it.
//! `advance_to` stops at the first internal deadline, allowing a scheduler to
//! dispatch an interrupt that may reprogram the device before advancing again.

use std::{error::Error as StdError, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    TerminalCount,
    OneShot,
    RateGenerator,
    SquareWave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Low,
    High,
    LowHigh,
}

/// Mode 3 count 1 is outside Intel's specified range. Compatibility is explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountOnePolicy {
    Reject,
    /// DOSBox-X interprets raw 1 as an effective 65,537-clock period.
    /// This model rounds the high half up to 32,769 whole clocks.
    DosBoxWrap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Port(u16),
    Channel(usize),
    Unprogrammed(usize),
    UnknownCount(usize),
    IncompleteRead(usize),
    UnsupportedControl(u8),
    InvalidPeriodicCount,
    TimeBackwards,
    ClockOverflow,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Port(port) => write!(f, "unsupported PIT port {port:#x}"),
            Self::Channel(channel) => write!(f, "invalid PIT channel {channel}"),
            Self::Unprogrammed(channel) => write!(f, "PIT channel {channel} has no control word"),
            Self::UnknownCount(channel) => write!(f, "PIT channel {channel} has no loaded count"),
            Self::IncompleteRead(channel) => write!(
                f,
                "PIT channel {channel} needs its high-byte read before a new latch"
            ),
            Self::UnsupportedControl(value) => write!(f, "unsupported PIT control word {value:#x}"),
            Self::InvalidPeriodicCount => {
                write!(f, "PIT modes 2 and 3 require a count of at least 2")
            }
            Self::TimeBackwards => write!(f, "PIT time cannot run backwards"),
            Self::ClockOverflow => write!(f, "PIT time has no representable next clock"),
        }
    }
}
impl StdError for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Edge {
    pub tick: u64,
    pub channel: usize,
    pub high: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    pub tick: u64,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelState {
    pub mode: Option<Mode>,
    pub access: Access,
    pub gate: bool,
    pub output: bool,
    /// Last complete count-register write. Zero encodes 65,536.
    pub programmed_count: Option<u16>,
    /// Counting element, distinct from the programmed reload value.
    pub counter: Option<u16>,
    pub pending_high_byte: bool,
    pub pending_load: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Channel {
    mode: Option<Mode>,
    count_one: CountOnePolicy,
    access: Access,
    gate: bool,
    output: bool,
    reload: Option<u16>,
    counter: Option<u16>,
    write_low: Option<u8>,
    read_high: bool,
    latch: Option<u16>,
    latch_high: bool,
    pending_load: bool,
    counting: bool,
    /// Clocks until next output transition; None after a one-shot terminal count.
    remaining: Option<u32>,
}

fn count(raw: u16) -> u32 {
    if raw == 0 { 65_536 } else { u32::from(raw) }
}

impl Channel {
    fn new(gate: bool, output: bool, count_one: CountOnePolicy) -> Self {
        Self {
            mode: None,
            count_one,
            access: Access::LowHigh,
            gate,
            output,
            reload: None,
            counter: None,
            write_low: None,
            read_high: false,
            latch: None,
            latch_high: false,
            pending_load: false,
            counting: false,
            remaining: None,
        }
    }

    fn configure(&mut self, access: Access, mode: Mode) {
        // A control word resets control logic and establishes OUT immediately.
        *self = Self {
            mode: Some(mode),
            access,
            output: mode != Mode::TerminalCount,
            ..Self::new(self.gate, self.output, self.count_one)
        };
    }

    fn write(&mut self, value: u8) -> Result<(), Error> {
        let mode = self.mode.expect("validated programmed channel");
        let raw = match self.access {
            Access::Low => u16::from(value),
            Access::High => u16::from(value) << 8,
            Access::LowHigh => {
                if let Some(low) = self.write_low {
                    u16::from_le_bytes([low, value])
                } else {
                    self.write_low = Some(value);
                    if mode == Mode::TerminalCount {
                        self.counting = false;
                        self.pending_load = false;
                        self.remaining = None;
                        self.output = false;
                    }
                    return Ok(());
                }
            }
        };
        if raw == 1
            && (mode == Mode::RateGenerator
                || (mode == Mode::SquareWave && self.count_one == CountOnePolicy::Reject))
        {
            return Err(Error::InvalidPeriodicCount);
        }
        self.write_low = None;
        self.reload = Some(raw);
        match mode {
            Mode::TerminalCount => {
                self.output = false;
                self.counting = false;
                self.pending_load = true;
                self.remaining = None;
            }
            Mode::OneShot => {} // A write arms the NEXT gate trigger, not another pulse.
            Mode::RateGenerator | Mode::SquareWave => {
                if self.counter.is_none() {
                    self.pending_load = true;
                }
            }
        }
        Ok(())
    }

    fn active(&self) -> bool {
        self.counting && (self.mode == Some(Mode::OneShot) || self.gate)
    }

    fn deadline(&self) -> Option<u32> {
        if self.pending_load {
            Some(1)
        } else if self.active() {
            self.remaining
        } else {
            None
        }
    }

    fn elapse(&mut self, clocks: u64) {
        if !self.active() || self.pending_load || clocks == 0 {
            return;
        }
        if let Some(counter) = &mut self.counter {
            let decrement = if self.mode == Some(Mode::SquareWave) {
                (clocks as u16).wrapping_mul(2)
            } else {
                clocks as u16
            };
            *counter = counter.wrapping_sub(decrement);
        }
        if let Some(remaining) = &mut self.remaining {
            // Pit stops at the first deadline, so this subtraction cannot underflow.
            *remaining -= clocks as u32;
        }
    }

    fn load_phase(&mut self) {
        let raw = self.reload.expect("load requires a complete count");
        let n = if raw == 1 && self.mode == Some(Mode::SquareWave) {
            // Explicit compatibility behavior, not an Intel-defined count.
            // Retain raw 1 for diagnostics and reload/access sequencing.
            65_537
        } else {
            count(raw)
        };
        self.counter = Some(raw);
        self.counting = true;
        self.remaining = Some(match self.mode.expect("load requires a mode") {
            Mode::TerminalCount | Mode::OneShot => n,
            Mode::RateGenerator => n - 1,
            Mode::SquareWave => {
                // 8254 odd-count CE starts at N-1; 8253 odd reads differ.
                self.counter = Some(raw & !1);
                if self.output { n.div_ceil(2) } else { n / 2 }
            }
        });
    }

    fn clock_deadline(&mut self) {
        if self.pending_load {
            self.pending_load = false;
            match self.mode.expect("pending load has a mode") {
                Mode::OneShot => self.output = false,
                Mode::RateGenerator | Mode::SquareWave => self.output = true,
                Mode::TerminalCount => {}
            }
            self.load_phase();
        } else if self.active() && self.remaining == Some(0) {
            match self.mode.expect("active counter has a mode") {
                Mode::TerminalCount | Mode::OneShot => {
                    self.output = true;
                    self.remaining = None;
                    // CE continues wrapping after terminal; OUT stays high.
                }
                Mode::RateGenerator => {
                    if self.output {
                        self.output = false;
                        self.remaining = Some(1);
                    } else {
                        self.output = true;
                        self.load_phase();
                    }
                }
                Mode::SquareWave => {
                    self.output = !self.output;
                    self.load_phase();
                }
            }
        }
    }

    fn set_gate(&mut self, high: bool) {
        let rising = high && !self.gate;
        self.gate = high;
        match self.mode {
            Some(Mode::OneShot) if rising && self.reload.is_some() => {
                self.pending_load = true;
            }
            Some(Mode::RateGenerator | Mode::SquareWave) => {
                if !high {
                    self.output = true;
                } else if rising && self.reload.is_some() {
                    self.pending_load = true;
                }
            }
            _ => {}
        }
    }

    fn read(&mut self) -> Option<u8> {
        let value = self.latch.or(self.counter)?;
        let high = match self.access {
            Access::Low => false,
            Access::High => true,
            Access::LowHigh => {
                if self.latch.is_some() {
                    self.latch_high
                } else {
                    self.read_high
                }
            }
        };
        if self.latch.is_some() {
            if self.access != Access::LowHigh || high {
                self.latch = None;
                self.latch_high = false;
            } else {
                self.latch_high = true;
            }
        } else if self.access == Access::LowHigh {
            self.read_high = !self.read_high;
        }
        Some(if high {
            (value >> 8) as u8
        } else {
            value as u8
        })
    }

    fn state(self) -> ChannelState {
        ChannelState {
            mode: self.mode,
            access: self.access,
            gate: self.gate,
            output: self.output,
            programmed_count: self.reload,
            counter: self.counter,
            pending_high_byte: self.write_low.is_some(),
            pending_load: self.pending_load,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pit {
    tick: u64,
    channels: [Channel; 3],
}

impl Pit {
    /// Initial pin levels are explicit caller context, not a claimed power-on state.
    /// Channels are unprogrammed until a control word is supplied.
    pub fn new(gates: [bool; 3], outputs: [bool; 3]) -> Self {
        Self::with_mode3_count_one(gates, outputs, CountOnePolicy::Reject)
    }

    /// Select a policy for the otherwise unspecified mode 3 count of one.
    pub fn with_mode3_count_one(
        gates: [bool; 3],
        outputs: [bool; 3],
        count_one: CountOnePolicy,
    ) -> Self {
        Self {
            tick: 0,
            channels: std::array::from_fn(|i| Channel::new(gates[i], outputs[i], count_one)),
        }
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn channel(&self, channel: usize) -> Result<ChannelState, Error> {
        self.channels
            .get(channel)
            .copied()
            .map(Channel::state)
            .ok_or(Error::Channel(channel))
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.channels
            .iter()
            .filter_map(|c| c.deadline())
            .min()
            .and_then(|delta| self.tick.checked_add(u64::from(delta)))
    }

    /// Next rising edge of one output, assuming no intervening writes or gate
    /// changes. This read-only scheduling hint neither clocks the device nor
    /// delivers an interrupt. Other channels' transitions do not affect it.
    pub fn next_rising_edge(&self, index: usize) -> Result<Option<u64>, Error> {
        let mut channel = *self.channels.get(index).ok_or(Error::Channel(index))?;
        let mut tick = self.tick;
        // A pending load, falling edge, then rising edge cover every supported
        // mode. One-shots and stopped/gated channels can instead run out of events.
        for _ in 0..3 {
            let Some(clocks) = channel.deadline() else {
                return Ok(None);
            };
            let Some(next) = tick.checked_add(u64::from(clocks)) else {
                return Ok(None);
            };
            tick = next;
            let high = channel.output;
            channel.elapse(u64::from(clocks));
            channel.clock_deadline();
            if !high && channel.output {
                return Ok(Some(tick));
            }
        }
        Ok(None)
    }

    /// Advance at most to the first device deadline. A load may return no edges.
    /// Process returned edges/interrupts before calling again toward the target.
    /// Simultaneous channel edges are reported in channel-number order.
    pub fn advance_to(&mut self, target: u64) -> Result<Step, Error> {
        if target < self.tick {
            return Err(Error::TimeBackwards);
        }
        let stop = self
            .next_deadline()
            .map_or(target, |deadline| target.min(deadline));
        let clocks = stop - self.tick;
        if clocks == 0 {
            return Ok(Step {
                tick: stop,
                edges: Vec::new(),
            });
        }
        let before = self.channels.map(|c| c.output);
        for channel in &mut self.channels {
            channel.elapse(clocks);
        }
        self.tick = stop;
        for channel in &mut self.channels {
            // Pending loads have deadline one; they only execute at that edge.
            // A sub-deadline advance has zero whole clocks for a pending load.
            if channel.pending_load || channel.remaining == Some(0) {
                channel.clock_deadline();
            }
        }
        Ok(Step {
            tick: stop,
            edges: self.edges(before),
        })
    }

    fn edges(&self, before: [bool; 3]) -> Vec<Edge> {
        (0..3)
            .filter(|&channel| self.channels[channel].output != before[channel])
            .map(|channel| Edge {
                tick: self.tick,
                channel,
                high: self.channels[channel].output,
            })
            .collect()
    }

    pub fn write_port(&mut self, port: u16, value: u8) -> Result<Vec<Edge>, Error> {
        let before = self.channels.map(|c| c.output);
        if port == 0x43 {
            let selected = usize::from(value >> 6);
            let access = (value >> 4) & 3;
            if selected == 3 {
                return Err(Error::UnsupportedControl(value));
            }
            let channel = &mut self.channels[selected];
            if access == 0 {
                if channel.latch.is_none() {
                    if channel.read_high {
                        return Err(Error::IncompleteRead(selected));
                    }
                    channel.latch = Some(channel.counter.ok_or(Error::UnknownCount(selected))?);
                    channel.latch_high = false;
                }
            } else {
                let mode = match (value >> 1) & 7 {
                    0 => Mode::TerminalCount,
                    1 => Mode::OneShot,
                    2 | 6 => Mode::RateGenerator,
                    3 | 7 => Mode::SquareWave,
                    _ => return Err(Error::UnsupportedControl(value)),
                };
                if value & 1 != 0 {
                    return Err(Error::UnsupportedControl(value));
                }
                channel.configure(
                    match access {
                        1 => Access::Low,
                        2 => Access::High,
                        _ => Access::LowHigh,
                    },
                    mode,
                );
            }
        } else if (0x40..=0x42).contains(&port) {
            let index = usize::from(port - 0x40);
            if self.channels[index].mode.is_none() {
                return Err(Error::Unprogrammed(index));
            }
            // Invalid complete counts leave byte sequencing and old execution intact.
            let mut channel = self.channels[index];
            channel.write(value)?;
            if channel.pending_load && self.tick == u64::MAX {
                return Err(Error::ClockOverflow);
            }
            self.channels[index] = channel;
        } else {
            return Err(Error::Port(port));
        }
        Ok(self.edges(before))
    }

    pub fn read_port(&mut self, port: u16) -> Result<u8, Error> {
        if !(0x40..=0x42).contains(&port) {
            return Err(Error::Port(port));
        }
        let channel = usize::from(port - 0x40);
        self.channels[channel]
            .read()
            .ok_or(Error::UnknownCount(channel))
    }

    pub fn set_gate(&mut self, index: usize, high: bool) -> Result<Vec<Edge>, Error> {
        let before = self.channels.map(|c| c.output);
        let mut channel = *self.channels.get(index).ok_or(Error::Channel(index))?;
        channel.set_gate(high);
        if channel.pending_load && self.tick == u64::MAX {
            return Err(Error::ClockOverflow);
        }
        self.channels[index] = channel;
        Ok(self.edges(before))
    }
}
