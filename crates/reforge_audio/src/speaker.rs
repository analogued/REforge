// Copyright (c) 2026 analogued. Project license has not been selected.
//! PC speaker digital wiring, before any physical response or PCM renderer.

use crate::pit::{CountOnePolicy, Edge, Error, Pit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriveEdge {
    pub tick: u64,
    pub high: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Events {
    pub tick: u64,
    pub pit: Vec<Edge>,
    pub drive: Option<DriveEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcSpeaker {
    pit: Pit,
    control: u8,
    drive: bool,
}

impl PcSpeaker {
    /// Synthetic/externally established pins. Channel 0/1 gates are tied high;
    /// channel 2 GATE follows port 61 bit 0. Upper control bits are retained only.
    pub fn new(control: u8, outputs: [bool; 3]) -> Self {
        Self::with_mode3_count_one(control, outputs, CountOnePolicy::Reject)
    }

    /// See [`CountOnePolicy`]; ordinary construction rejects unspecified counts.
    pub fn with_mode3_count_one(
        control: u8,
        outputs: [bool; 3],
        count_one: CountOnePolicy,
    ) -> Self {
        let pit = Pit::with_mode3_count_one([true, true, control & 1 != 0], outputs, count_one);
        Self {
            pit,
            control,
            drive: control & 2 != 0 && outputs[2],
        }
    }

    pub fn pit(&self) -> &Pit {
        &self.pit
    }
    pub fn drive(&self) -> bool {
        self.drive
    }
    pub fn control(&self) -> u8 {
        self.control
    }

    fn events(&mut self, pit: Vec<Edge>) -> Events {
        let high = self.control & 2 != 0 && self.pit.channel(2).expect("channel two").output;
        let drive = (high != self.drive).then_some(DriveEdge {
            tick: self.pit.tick(),
            high,
        });
        self.drive = high;
        Events {
            tick: self.pit.tick(),
            pit,
            drive,
        }
    }

    /// This retains port 61's byte; it does not emulate PPI, refresh or AT status inputs.
    /// A machine adapter must supply those other read bits if needed.
    pub fn read_port(&mut self, port: u16) -> Result<u8, Error> {
        if port == 0x61 {
            Ok(self.control)
        } else {
            self.pit.read_port(port)
        }
    }

    pub fn write_port(&mut self, port: u16, value: u8) -> Result<Events, Error> {
        let edges = if port == 0x61 {
            let edges = self.pit.set_gate(2, value & 1 != 0)?;
            self.control = value;
            edges
        } else {
            self.pit.write_port(port, value)?
        };
        Ok(self.events(edges))
    }

    /// Stops at the next PIT deadline; callers dispatch IRQ0 edges before advancing again.
    pub fn advance_to(&mut self, target: u64) -> Result<Events, Error> {
        let step = self.pit.advance_to(target)?;
        Ok(self.events(step.edges))
    }
}
