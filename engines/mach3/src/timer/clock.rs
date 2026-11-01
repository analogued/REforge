// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The unmasked IRQ0/IRQ1 subset needed by the reconstructed sound routines.

use crate::{
    Error,
    sound::{FarPointer, SampleBus, SampleEnvironment, SoundAction},
};
use reforge_audio::speaker::{Events, PcSpeaker};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub tick: u64,
    pub order: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClockEvent {
    Action(SoundAction),
    Device(Events),
    Interrupt(Interrupt),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interrupt {
    pub irq: u8,
    pub handler: FarPointer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    Reached,
    Interrupt(Interrupt),
}

/// One device timeline. Observation is synchronous and does not drive execution.
/// The supplied pins, vectors and IF are explicit initial execution context.
/// IRQ requests coalesce while blocked; keyboard input has a single byte latch.
/// PIC masks, electrical INTA timing/spurious IRQ7, other IRQs and instruction
/// latency are outside this subset.
pub struct Clock<O: FnMut(Stamp, ClockEvent)> {
    speaker: PcSpeaker,
    handlers: [FarPointer; 2],
    enabled: bool,
    pending: [bool; 2],
    in_service: [bool; 2],
    frames: Vec<bool>,
    keyboard_byte: Option<u8>,
    order: u64,
    observe: O,
    failure: Option<String>,
}

impl<O: FnMut(Stamp, ClockEvent)> Clock<O> {
    pub fn new(speaker: PcSpeaker, handlers: [FarPointer; 2], enabled: bool, observe: O) -> Self {
        Self {
            speaker,
            handlers,
            enabled,
            pending: [false; 2],
            in_service: [false; 2],
            frames: Vec::new(),
            keyboard_byte: None,
            order: 0,
            observe,
            failure: None,
        }
    }

    pub fn speaker(&self) -> &PcSpeaker {
        &self.speaker
    }
    pub fn tick(&self) -> u64 {
        self.speaker.pit().tick()
    }
    pub fn handlers(&self) -> [FarPointer; 2] {
        self.handlers
    }
    pub fn interrupts_enabled(&self) -> bool {
        self.enabled
    }

    pub fn sample_environment(&self, game_code_segment: u16, di: u16) -> SampleEnvironment {
        SampleEnvironment {
            game_code_segment,
            di,
            timer_handler: self.handlers[0],
            keyboard_handler: self.handlers[1],
        }
    }

    /// Sticky errors: a failed bus must not produce a successful continuation.
    pub fn check(&self) -> Result<(), Error> {
        self.failure
            .as_ref()
            .map_or(Ok(()), |message| Err(invalid(message.clone())))
    }

    /// An entered handler cannot be abandoned and then silently skipped.
    pub(crate) fn fail(&mut self, error: &Error) {
        if self.failure.is_none() {
            self.failure = Some(error.to_string());
        }
    }

    /// A new byte cannot overwrite an unread keyboard-controller latch.
    pub fn keyboard_input(&mut self, raw: u8) -> Result<(), Error> {
        self.check()?;
        if self.keyboard_byte.is_some() {
            return Err(invalid("keyboard input would overwrite an unread byte"));
        }
        self.keyboard_byte = Some(raw);
        self.pending[1] = true;
        Ok(())
    }

    fn record(&mut self, event: ClockEvent) {
        self.order += 1;
        let tick = self.tick();
        (self.observe)(
            Stamp {
                tick,
                order: self.order,
            },
            event,
        );
    }

    fn device_events(&mut self, events: Events) {
        if events.pit.iter().any(|edge| edge.channel == 0 && edge.high) {
            self.pending[0] = true;
        }
        // A budget stop without an edge is not a device event. Omitting it
        // keeps diagnostic ordering independent of scheduler chunk boundaries.
        if !events.pit.is_empty() || events.drive.is_some() {
            self.record(ClockEvent::Device(events));
        }
    }

    fn enter_interrupt(&mut self) -> Option<Interrupt> {
        if !self.enabled {
            return None;
        }
        let ceiling = self
            .in_service
            .iter()
            .position(|active| *active)
            .unwrap_or(2);
        let irq = (0..ceiling).find(|index| self.pending[*index])?;
        self.pending[irq] = false;
        self.in_service[irq] = true;
        self.frames.push(self.enabled);
        self.enabled = false;
        let interrupt = Interrupt {
            irq: irq as u8,
            handler: self.handlers[irq],
        };
        self.record(ClockEvent::Interrupt(interrupt));
        Some(interrupt)
    }

    /// Stop at the first deliverable interrupt, including one raised by a write
    /// at the current time. The caller executes the selected handler and IRET
    /// before calling again. Device transitions while IF is clear still occur.
    pub fn advance_to(&mut self, target: u64) -> Result<Progress, Error> {
        self.check()?;
        if target < self.tick() {
            return Err(invalid("simulation time cannot run backwards"));
        }
        loop {
            if let Some(interrupt) = self.enter_interrupt() {
                return Ok(Progress::Interrupt(interrupt));
            }
            if self.tick() == target {
                return Ok(Progress::Reached);
            }
            let events = self
                .speaker
                .advance_to(target)
                .map_err(|error| invalid(error.to_string()))?;
            self.device_events(events);
        }
    }

    fn apply(&mut self, action: SoundAction) -> Result<(), Error> {
        match action {
            SoundAction::Write {
                port: port @ (0x40..=0x43 | 0x61),
                value,
                ..
            } => {
                let events = self
                    .speaker
                    .write_port(port, value)
                    .map_err(|error| invalid(error.to_string()))?;
                self.device_events(events);
            }
            SoundAction::Write {
                port: 0x20,
                value: 0x20,
                ..
            } => {
                if let Some(irq) = self.in_service.iter().position(|active| *active) {
                    self.in_service[irq] = false;
                }
            }
            SoundAction::Write { port, .. } => {
                return Err(invalid(format!("unsupported simulation port {port:#x}")));
            }
            SoundAction::InterruptsEnabled(enabled) => self.enabled = enabled,
            SoundAction::InterruptReturn => {
                self.enabled = self
                    .frames
                    .pop()
                    .ok_or_else(|| invalid("IRET without interrupt entry"))?;
            }
            SoundAction::SetHandler { irq, handler } => {
                *self
                    .handlers
                    .get_mut(usize::from(irq))
                    .ok_or_else(|| invalid("unsupported IRQ vector"))? = handler;
            }
            _ => {}
        }
        Ok(())
    }
}

impl<O: FnMut(Stamp, ClockEvent)> SampleBus for Clock<O> {
    fn read_port(&mut self, port: u16) -> u8 {
        if self.failure.is_some() {
            return 0;
        }
        let result = if port == 0x60 {
            self.keyboard_byte
                .take()
                .ok_or_else(|| invalid("keyboard read without an input byte"))
        } else {
            self.speaker
                .read_port(port)
                .map_err(|error| invalid(error.to_string()))
        };
        match result {
            Ok(value) => value,
            Err(error) => {
                self.failure = Some(error.to_string());
                0
            }
        }
    }

    fn emit(&mut self, action: SoundAction) {
        if self.failure.is_some() {
            return;
        }
        self.record(ClockEvent::Action(action));
        if let Err(error) = self.apply(action) {
            self.failure = Some(error.to_string());
        }
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidData(message.into())
}
