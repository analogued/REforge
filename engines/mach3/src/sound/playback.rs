// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Synchronous sample routine 55f9 and its reconstructed interrupt handlers.

use super::{
    FarPointer, SampleBus, SampleMemory, SampleRequest, SoundAction, SourceByte, read, write,
};
use crate::Error;

#[derive(Clone, Debug)]
pub struct SampleSoundState {
    pub disabled: bool,
    pub last_scan: u8,
    pub keyboard_words: [u16; 65],
}

impl Default for SampleSoundState {
    fn default() -> Self {
        Self {
            disabled: false,
            last_scan: 0,
            keyboard_words: [0; 65],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SampleEnvironment {
    pub game_code_segment: u16,
    pub timer_handler: FarPointer,
    pub keyboard_handler: FarPointer,
    /// DI is inherited by the ISR; it is not initialized by 55f9.
    pub di: u16,
}

#[derive(Debug)]
pub enum StartResult {
    Playing(SamplePlayer),
    /// The original tail-jumps to its normal keyboard processor at 73b2.
    /// The engine must execute that processor and supply its actual result.
    RegularKeyboardPoll,
}

#[derive(Debug)]
pub struct SamplePlayer {
    pub playback: u64,
    pub request: SampleRequest,
    pub source_offset: u16,
    pub completed_bytes: u16,
    pub di: u16,
    visit: u32,
    saved: SampleEnvironment,
    active: bool,
}

impl SamplePlayer {
    pub fn begin(
        playback: u64,
        request: SampleRequest,
        state: &mut SampleSoundState,
        environment: SampleEnvironment,
        bus: &mut impl SampleBus,
    ) -> StartResult {
        if state.disabled {
            return StartResult::RegularKeyboardPoll;
        }
        state.last_scan = 0;
        bus.emit(SoundAction::SourceBegin { playback, request });
        bus.emit(SoundAction::InterruptsEnabled(false));
        bus.emit(SoundAction::SetHandler {
            irq: 0,
            handler: FarPointer {
                segment: environment.game_code_segment,
                offset: if request.reverse { 0x57b8 } else { 0x57a4 },
            },
        });
        bus.emit(SoundAction::SetHandler {
            irq: 1,
            handler: FarPointer {
                segment: environment.game_code_segment,
                offset: 0x57cc,
            },
        });
        write(bus, 0x43, 0xb2);
        write(bus, 0x42, 0);
        write(bus, 0x42, 0);
        let control = read(bus, 0x61);
        write(bus, 0x61, (control & 0xfc) | 3);
        write(bus, 0x43, 0x36);
        write(bus, 0x40, 60);
        write(bus, 0x40, 0);
        write(bus, 0x43, 0x90);
        bus.emit(SoundAction::InterruptsEnabled(true));
        StartResult::Playing(Self {
            playback,
            request,
            source_offset: if request.reverse {
                request.count
            } else {
                request.offset
            },
            completed_bytes: 0,
            di: environment.di,
            visit: 0,
            saved: environment,
            active: true,
        })
    }

    /// Apply one delivered IRQ0. Equal count writes remain distinct operations.
    /// Completion is observed by a separate foreground continuation, not the ISR.
    pub fn timer_interrupt(
        &mut self,
        memory: &impl SampleMemory,
        bus: &mut impl SampleBus,
    ) -> Result<(), Error> {
        if !self.active {
            return Err(invalid("sample handler is no longer installed"));
        }
        let byte = memory.read(self.request.resource, self.source_offset)?;
        bus.emit(SoundAction::Write {
            port: 0x42,
            value: 64 - (byte.value >> 2),
            source: Some(SourceByte {
                playback: self.playback,
                visit: self.visit,
                resource: self.request.resource,
                offset: self.source_offset,
                value: byte.value,
                origin: byte.origin,
            }),
        });
        self.di = self.di.wrapping_add(1) & 1;
        if self.di == 0 {
            self.source_offset = if self.request.reverse {
                self.source_offset.wrapping_sub(1)
            } else {
                self.source_offset.wrapping_add(1)
            };
            self.completed_bytes = self.completed_bytes.wrapping_add(1);
            self.visit = self.visit.wrapping_add(1);
        }
        write(bus, 0x20, 0x20);
        bus.emit(SoundAction::InterruptReturn);
        Ok(())
    }

    /// IRQ1 saves the latest raw make/break code; it does not abort playback.
    pub fn keyboard_interrupt(
        &self,
        state: &mut SampleSoundState,
        bus: &mut impl SampleBus,
    ) -> Result<(), Error> {
        if !self.active {
            return Err(invalid("sample keyboard handler is no longer installed"));
        }
        bus.emit(SoundAction::InterruptsEnabled(false));
        state.last_scan = read(bus, 0x60);
        let control = read(bus, 0x61);
        write(bus, 0x61, control | 0x80);
        write(bus, 0x61, control);
        write(bus, 0x20, 0x20);
        // 57f0 explicitly enables interrupts before IRET restores saved flags.
        bus.emit(SoundAction::InterruptsEnabled(true));
        bus.emit(SoundAction::InterruptReturn);
        Ok(())
    }

    pub fn completed(&self) -> bool {
        self.completed_bytes >= self.request.count
    }

    /// The foreground takes this branch only when its unsigned CX comparison
    /// observes completion. Timer phase is not magically restored.
    pub fn finish(
        &mut self,
        state: &mut SampleSoundState,
        bus: &mut impl SampleBus,
    ) -> Result<u16, Error> {
        if !self.active || !self.completed() {
            return Err(invalid("sample is not ready to return"));
        }
        bus.emit(SoundAction::InterruptsEnabled(false));
        bus.emit(SoundAction::SetHandler {
            irq: 1,
            handler: self.saved.keyboard_handler,
        });
        write(bus, 0x43, 0x36);
        write(bus, 0x40, 0x68);
        write(bus, 0x40, 0x4c);
        bus.emit(SoundAction::SetHandler {
            irq: 0,
            handler: self.saved.timer_handler,
        });
        bus.emit(SoundAction::InterruptsEnabled(true));
        state.keyboard_words.fill(0);
        bus.emit(SoundAction::ClearKeyboardWords {
            offset: 0x17c6,
            count: 65,
        });
        if state.last_scan & 0x7f == 0x3c {
            state.disabled = true;
            bus.emit(SoundAction::SoundDisabled);
        }
        bus.emit(SoundAction::SourceEnd {
            playback: self.playback,
        });
        self.active = false;
        Ok(u16::from(state.last_scan))
    }

    /// REP STOSW at return leaves DI at this offset for a subsequent call.
    pub const fn return_di() -> u16 {
        0x1848
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{ByteOrigin, MemoryByte, Resource};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Bus {
        reads: VecDeque<(u16, u8)>,
        actions: Vec<SoundAction>,
    }
    impl SampleBus for Bus {
        fn read_port(&mut self, port: u16) -> u8 {
            let (expected, value) = self.reads.pop_front().expect("expected a device read");
            assert_eq!(port, expected);
            value
        }
        fn emit(&mut self, action: SoundAction) {
            self.actions.push(action);
        }
    }

    struct Memory {
        bytes: Vec<u8>,
        external: Option<u8>,
    }
    impl SampleMemory for Memory {
        fn read(&self, _: Resource, offset: u16) -> Result<MemoryByte, Error> {
            if let Some(&value) = self.bytes.get(usize::from(offset)) {
                return Ok(MemoryByte {
                    value,
                    origin: ByteOrigin::Asset,
                });
            }
            if usize::from(offset) == self.bytes.len()
                && let Some(value) = self.external
            {
                return Ok(MemoryByte {
                    value,
                    origin: ByteOrigin::ExternalMemory,
                });
            }
            Err(invalid("unknown byte"))
        }
    }

    fn start(count: u16, reverse: bool, di: u16) -> (SamplePlayer, SampleSoundState, Bus) {
        let mut state = SampleSoundState {
            last_scan: 123,
            keyboard_words: [9; 65],
            ..Default::default()
        };
        let mut bus = Bus {
            reads: [(0x61, 0x64)].into(),
            ..Default::default()
        };
        let env = SampleEnvironment {
            game_code_segment: 0x3000,
            timer_handler: FarPointer {
                segment: 0x3000,
                offset: 0x7777,
            },
            keyboard_handler: FarPointer {
                segment: 0x4000,
                offset: 0x1111,
            },
            di,
        };
        let StartResult::Playing(player) = SamplePlayer::begin(
            17,
            SampleRequest {
                resource: Resource::DataB,
                offset: 0,
                count,
                reverse,
            },
            &mut state,
            env,
            &mut bus,
        ) else {
            panic!("expected enabled playback")
        };
        assert_eq!(state.last_scan, 0);
        (player, state, bus)
    }

    fn sources(bus: &Bus) -> Vec<(u8, SourceByte)> {
        bus.actions
            .iter()
            .filter_map(|action| match action {
                SoundAction::Write {
                    port: 0x42,
                    value,
                    source: Some(source),
                } => Some((*value, *source)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn equal_bytes_are_distinct_visits_but_repeated_writes_share_provenance() {
        let (mut player, mut state, mut bus) = start(3, false, 0);
        let memory = Memory {
            bytes: vec![128, 128, 255],
            external: None,
        };
        for _ in 0..6 {
            player.timer_interrupt(&memory, &mut bus).unwrap();
        }
        assert!(player.completed());
        let emitted = sources(&bus);
        assert_eq!(
            emitted
                .iter()
                .map(|(v, s)| (*v, s.visit, s.offset, s.value))
                .collect::<Vec<_>>(),
            [
                (32, 0, 0, 128),
                (32, 0, 0, 128),
                (32, 1, 1, 128),
                (32, 1, 1, 128),
                (1, 2, 2, 255),
                (1, 2, 2, 255)
            ]
        );
        assert_eq!(player.finish(&mut state, &mut bus).unwrap(), 0);
        assert!(player.timer_interrupt(&memory, &mut bus).is_err());
    }

    #[test]
    fn reverse_uses_external_count_byte_then_asset_count_minus_one_through_one() {
        let (mut player, mut state, mut bus) = start(3, true, 0);
        let memory = Memory {
            bytes: vec![255, 64, 128],
            external: Some(0),
        };
        for _ in 0..6 {
            player.timer_interrupt(&memory, &mut bus).unwrap();
        }
        let emitted = sources(&bus);
        assert_eq!(
            emitted
                .iter()
                .map(|(v, s)| (*v, s.offset))
                .collect::<Vec<_>>(),
            [(64, 3), (64, 3), (32, 2), (32, 2), (48, 1), (48, 1)]
        );
        assert_eq!(emitted[0].1.origin, ByteOrigin::ExternalMemory);
        assert_eq!(emitted[2].1.origin, ByteOrigin::Asset);
        assert_eq!(player.source_offset, 0);
        player.finish(&mut state, &mut bus).unwrap();
    }

    #[test]
    fn unknown_allocation_byte_is_an_error_without_inventing_a_source_visit() {
        let (mut player, _, mut bus) = start(1, true, 0);
        let previous = bus.actions.len();
        assert!(
            player
                .timer_interrupt(
                    &Memory {
                        bytes: vec![1],
                        external: None
                    },
                    &mut bus
                )
                .is_err()
        );
        assert_eq!(bus.actions.len(), previous);
        assert_eq!(
            (player.source_offset, player.completed_bytes, player.visit),
            (1, 0, 0)
        );
    }

    #[test]
    fn inherited_odd_di_advances_after_the_first_write() {
        let (mut player, mut state, mut bus) = start(2, false, 0xffff);
        let memory = Memory {
            bytes: vec![0, 255],
            external: None,
        };
        for _ in 0..3 {
            player.timer_interrupt(&memory, &mut bus).unwrap();
        }
        assert_eq!(
            sources(&bus)
                .iter()
                .map(|(_, s)| (s.visit, s.offset))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 1), (1, 1)]
        );
        assert!(player.completed());
        player.finish(&mut state, &mut bus).unwrap();
        assert_eq!(SamplePlayer::return_di(), 0x1848);
    }

    #[test]
    fn keyboard_break_does_not_abort_and_return_keeps_the_raw_byte() {
        let (mut player, mut state, mut bus) = start(1, false, 0);
        bus.reads
            .extend([(0x60, 0x39), (0x61, 0x63), (0x60, 0xb9), (0x61, 0x23)]);
        player.keyboard_interrupt(&mut state, &mut bus).unwrap();
        player.keyboard_interrupt(&mut state, &mut bus).unwrap();
        assert!(!player.completed());
        assert!(player.finish(&mut state, &mut bus).is_err());
        let memory = Memory {
            bytes: vec![127],
            external: None,
        };
        player.timer_interrupt(&memory, &mut bus).unwrap();
        player.timer_interrupt(&memory, &mut bus).unwrap();
        assert_eq!(player.finish(&mut state, &mut bus).unwrap(), 0xb9);
        assert_eq!(state.last_scan, 0xb9);
        assert_eq!(state.keyboard_words, [0; 65]);
        assert!(!state.disabled);
        let writes = bus
            .actions
            .iter()
            .filter_map(|a| match a {
                SoundAction::Write {
                    port: 0x61, value, ..
                } => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(writes, [0x67, 0xe3, 0x63, 0xa3, 0x23]);
    }

    #[test]
    fn f2_make_or_break_disables_only_on_return_and_bypass_needs_normal_poll() {
        for scan in [0x3c, 0xbc] {
            let (mut player, mut state, mut bus) = start(0, false, 0);
            bus.reads.extend([(0x60, scan), (0x61, 0)]);
            player.keyboard_interrupt(&mut state, &mut bus).unwrap();
            assert!(!state.disabled);
            assert_eq!(
                player.finish(&mut state, &mut bus).unwrap(),
                u16::from(scan)
            );
            assert!(state.disabled);
            let previous = bus.actions.len();
            assert!(matches!(
                SamplePlayer::begin(18, player.request, &mut state, player.saved, &mut bus),
                StartResult::RegularKeyboardPoll
            ));
            assert_eq!(bus.actions.len(), previous);
            assert_eq!(state.last_scan, scan);
        }
    }

    #[test]
    fn setup_and_restoration_preserve_write_order_and_reset_to_fixed_pit_divisor() {
        let (mut player, mut state, mut bus) = start(0, false, 0);
        player.finish(&mut state, &mut bus).unwrap();
        let writes = bus
            .actions
            .iter()
            .filter_map(|a| match a {
                SoundAction::Write { port, value, .. } => Some((*port, *value)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            writes,
            [
                (0x43, 0xb2),
                (0x42, 0),
                (0x42, 0),
                (0x61, 0x67),
                (0x43, 0x36),
                (0x40, 60),
                (0x40, 0),
                (0x43, 0x90),
                (0x43, 0x36),
                (0x40, 0x68),
                (0x40, 0x4c)
            ]
        );
        let restored = bus
            .actions
            .iter()
            .filter_map(|a| match a {
                SoundAction::SetHandler { irq, handler } => Some((*irq, *handler)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(restored.last().unwrap(), &(0, player.saved.timer_handler));
        assert_eq!(restored[2], (1, player.saved.keyboard_handler));
    }
}
