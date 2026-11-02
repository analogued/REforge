// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Game timer counters, foreground waits, and ordered device/IRQ execution.
//!
//! Handlers remain engine routines: advancing this clock returns at interrupt
//! entry, before any subsequent device deadline. There is no host clock or PCM
//! renderer here. Whole-clock writes and atomic, zero-duration handlers are an
//! explicit execution approximation, not instruction-cycle emulation.

mod clock;
mod sample;

pub use clock::{Clock, ClockEvent, Interrupt, Progress, Stamp};
pub use sample::{SampleProgress, SampleStart, SampleTask};

use crate::sound::{SampleBus, SoundAction, read, write};

/// Separate from the hardware count register, including during sample playback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counters {
    /// CS:7b27, written by 7b19; zero adds zero, not the PIT's encoded 65,536.
    pub divisor: u16,
    /// CS:7b29, retained across handler changes and foreground wait returns.
    pub phase: u16,
    /// DS:17bf (CS:ad5f), wrapping carries accumulated by interrupt helper 7b2b.
    pub units: u16,
}

impl Counters {
    /// Execute helper 7b19. Channel 0 must already have its control word.
    pub fn program_divisor(&mut self, divisor: u16, bus: &mut impl SampleBus) {
        self.divisor = divisor;
        write(bus, 0x40, divisor as u8);
        write(bus, 0x40, (divisor >> 8) as u8);
    }

    /// Execute helper 7b2b, including its return AX and sound-disable branch.
    /// Every caller advances counters even when native sound is disabled.
    pub fn interrupt(&mut self, sound_disable_word: u16, bus: &mut impl SampleBus) -> u16 {
        for _ in 0..2 {
            let (phase, carry) = self.phase.overflowing_add(self.divisor);
            self.phase = phase;
            if carry {
                self.units = self.units.wrapping_add(1);
            }
        }
        let result = sound_disable_word ^ 0xffff;
        if result == 0 {
            let control = read(bus, 0x61);
            write(bus, 0x61, control & 0xfc);
        }
        result
    }
}

/// Suspended foreground call to 7437. It consumes no IRQs itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Wait {
    target: u16,
}

impl Wait {
    /// Entry loads only AL from the stack argument, preserving incoming AH.
    pub const fn from_call(incoming_ax: u16, argument: u16) -> Self {
        Self {
            target: (incoming_ax & 0xff00) | (argument & 0xff),
        }
    }

    pub const fn target(self) -> u16 {
        self.target
    }

    /// Unsigned comparison; return clears the entire counter, including excess.
    /// The software phase and hardware phase are retained.
    pub fn poll(self, counters: &mut Counters) -> bool {
        if counters.units < self.target {
            return false;
        }
        counters.units = 0;
        true
    }
}

/// Finish an ordinary IRQ after its game-specific body has executed.
pub fn return_interrupt(bus: &mut impl SampleBus) {
    write(bus, 0x20, 0x20);
    bus.emit(SoundAction::InterruptReturn);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Bus(Vec<SoundAction>);
    impl SampleBus for Bus {
        fn read_port(&mut self, port: u16) -> u8 {
            assert_eq!(port, 0x61);
            0x63
        }
        fn emit(&mut self, action: SoundAction) {
            self.0.push(action);
        }
    }

    #[test]
    fn helper_preserves_two_separate_carries_and_wraps_counter() {
        let mut counters = Counters {
            divisor: 0xffff,
            phase: 1,
            units: 0xffff,
        };
        let mut bus = Bus::default();
        assert_eq!(counters.interrupt(0, &mut bus), 0xffff);
        assert_eq!(
            counters,
            Counters {
                divisor: 0xffff,
                phase: 0xffff,
                units: 0
            }
        );
        counters.interrupt(0, &mut bus);
        assert_eq!((counters.phase, counters.units), (0xfffd, 2));
        assert!(bus.0.is_empty());
    }

    #[test]
    fn disable_branch_still_accumulates_and_returns_the_actual_ax() {
        let mut counters = Counters {
            divisor: 40000,
            phase: 0,
            units: 0,
        };
        let mut bus = Bus::default();
        assert_eq!(counters.interrupt(0xffff, &mut bus), 0);
        assert_eq!((counters.phase, counters.units), (14464, 1));
        assert_eq!(
            bus.0,
            vec![
                SoundAction::Read {
                    port: 0x61,
                    value: 0x63
                },
                SoundAction::Write {
                    port: 0x61,
                    value: 0x60,
                    source: None
                },
            ]
        );
        bus.0.clear();
        assert_eq!(counters.interrupt(0x1234, &mut bus), 0xedcb);
        assert!(bus.0.is_empty());
    }

    #[test]
    fn software_zero_is_not_a_hardware_full_period() {
        let mut counters = Counters {
            phase: 0xffff,
            units: 7,
            ..Counters::default()
        };
        counters.interrupt(0, &mut Bus::default());
        assert_eq!((counters.phase, counters.units), (0xffff, 7));
    }

    #[test]
    fn wait_preserves_ah_discards_excess_and_leaves_phase() {
        let mut counters = Counters {
            divisor: 1850,
            phase: 123,
            units: 5,
        };
        let wait = Wait::from_call(0x1201, 0xab02);
        assert_eq!(wait.target(), 0x1202);
        assert!(!wait.poll(&mut counters));
        assert!(Wait::from_call(1, 1).poll(&mut counters));
        assert_eq!(
            counters,
            Counters {
                divisor: 1850,
                phase: 123,
                units: 0
            }
        );
        assert!(Wait::from_call(0, 0).poll(&mut counters));
    }
}
