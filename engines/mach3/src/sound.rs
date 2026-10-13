// Copyright (c) 2026 analogued. Project license has not been selected.
//! Reconstructed sample and procedural sound execution. This module does not render audio
//! or advance a PIT: the caller supplies interrupt delivery and device reads.

mod assets;
mod dos_memory;
mod intro_task;
mod playback;
mod procedural;
mod sequence;

pub use assets::{ByteOrigin, MemoryByte, Resource, SampleBytes, SampleMemory, SoundAssets};
pub use dos_memory::DosSampleMemory;
pub use intro_task::{IntroProgress, IntroTask};
pub use playback::{SampleEnvironment, SamplePlayer, SampleSoundState, StartResult};
pub use procedural::{
    Effect, Handler, NoiseState, PreparedEffect, ProceduralProgram, ProceduralState, Ramp, Tone,
};
pub use sequence::{IntroCall, IntroProgram, IntroSequence};

/// Foreground operations selected by gameplay algorithms, before device execution.
/// `site` identifies the original CALL instruction, not a host timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameplayAction {
    Request {
        site: u16,
        effect: Effect,
    },
    /// Full AX was loaded with these units before calling the timer wait.
    Wait {
        site: u16,
        units: u16,
    },
    /// Wrapper 4fd9 plays DATA8 only when the application's b666 flag is set.
    /// The executor supplies that flag and the ordinary sample-disable state.
    IntroSample {
        site: u16,
        request: SampleRequest,
    },
}

/// A synchronous observer/executor; it does not select another sound sequence.
pub type Foreground<'a> = dyn FnMut(GameplayAction) + 'a;

pub(crate) fn request(sink: &mut Foreground<'_>, site: u16, effect: Effect) {
    sink(GameplayAction::Request { site, effect });
}

/// The actual arguments passed to the synchronous sample routine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampleRequest {
    pub resource: Resource,
    pub offset: u16,
    pub count: u16,
    pub reverse: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FarPointer {
    pub segment: u16,
    pub offset: u16,
}

/// Identity belongs to one execution, not to an independently running voice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceByte {
    pub playback: u64,
    /// Repeated writes of the current source byte share this identity.
    pub visit: u32,
    pub resource: Resource,
    pub offset: u16,
    pub value: u8,
    pub origin: ByteOrigin,
}

/// Ordered effects of the reconstructed routine, before device simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundAction {
    Read {
        port: u16,
        value: u8,
    },
    Write {
        port: u16,
        value: u8,
        source: Option<SourceByte>,
    },
    InterruptsEnabled(bool),
    /// IRET restores the flags saved by the caller's interrupt dispatcher.
    InterruptReturn,
    SetHandler {
        irq: u8,
        handler: FarPointer,
    },
    SourceBegin {
        playback: u64,
        request: SampleRequest,
    },
    SourceEnd {
        playback: u64,
    },
    ClearKeyboardWords {
        offset: u16,
        count: u16,
    },
    SoundDisabled,
}

/// The caller timestamps actions in its simulation timeline and applies writes
/// and handler changes to its device/scheduler state. No renderer is involved.
pub trait SampleBus {
    fn read_port(&mut self, port: u16) -> u8;
    fn emit(&mut self, action: SoundAction);
}

pub(crate) fn write(bus: &mut impl SampleBus, port: u16, value: u8) {
    bus.emit(SoundAction::Write {
        port,
        value,
        source: None,
    });
}

pub(crate) fn read(bus: &mut impl SampleBus, port: u16) -> u8 {
    let value = bus.read_port(port);
    bus.emit(SoundAction::Read { port, value });
    value
}
