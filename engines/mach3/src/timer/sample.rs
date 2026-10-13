// Copyright (c) 2026 analogued. Project license has not been selected.
//! Synchronous sample playback as a suspended guest foreground continuation.

use super::{Clock, ClockEvent, Interrupt, Progress, Stamp};
use crate::{
    Error,
    sound::{FarPointer, SampleMemory, SamplePlayer, SampleRequest, SampleSoundState, StartResult},
};

#[derive(Debug)]
pub enum SampleStart {
    Playing(SampleTask),
    /// The caller must run the original ordinary keyboard processor at 73b2.
    RegularKeyboardPoll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleProgress {
    Pending,
    Returned(u16),
}

/// Uses the existing sample algorithm's sole source cursor. While pending, the
/// caller suspends the guest foreground, not the host thread or PIT execution.
/// Inject timestamped keyboard bytes into the clock before crossing their time.
#[derive(Debug)]
pub struct SampleTask {
    player: SamplePlayer,
    timer_handler: FarPointer,
    keyboard_handler: FarPointer,
    returned: bool,
}

impl SampleTask {
    pub fn begin<O: FnMut(Stamp, ClockEvent)>(
        playback: u64,
        request: SampleRequest,
        state: &mut SampleSoundState,
        game_code_segment: u16,
        di: u16,
        clock: &mut Clock<O>,
    ) -> Result<SampleStart, Error> {
        clock.check()?;
        let environment = clock.sample_environment(game_code_segment, di);
        let started = SamplePlayer::begin(playback, request, state, environment, clock);
        clock.check()?;
        Ok(match started {
            StartResult::RegularKeyboardPoll => SampleStart::RegularKeyboardPoll,
            StartResult::Playing(player) => SampleStart::Playing(Self {
                player,
                timer_handler: FarPointer {
                    segment: game_code_segment,
                    offset: if request.reverse { 0x57b8 } else { 0x57a4 },
                },
                keyboard_handler: FarPointer {
                    segment: game_code_segment,
                    offset: 0x57cc,
                },
                returned: false,
            }),
        })
    }

    /// Process only through the given simulation budget or foreground return,
    /// whichever comes first. At return, remaining budget belongs to the caller's
    /// next continuation; the original teardown writes have already executed.
    pub fn advance_until<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        target: u64,
        memory: &impl SampleMemory,
        state: &mut SampleSoundState,
        clock: &mut Clock<O>,
    ) -> Result<SampleProgress, Error> {
        clock.check()?;
        if self.returned {
            return Err(invalid("sample foreground has already returned"));
        }
        if target < clock.tick() {
            return Err(invalid("sample budget precedes current simulation time"));
        }
        let result = self.advance_inner(target, memory, state, clock);
        if let Err(error) = &result {
            clock.fail(error);
        }
        result
    }

    fn advance_inner<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        target: u64,
        memory: &impl SampleMemory,
        state: &mut SampleSoundState,
        clock: &mut Clock<O>,
    ) -> Result<SampleProgress, Error> {
        loop {
            // Service pending deliverable interrupts before letting guest
            // foreground observe CX and run its CLI/teardown sequence.
            let pending = clock.advance_to(clock.tick())?;
            if pending == Progress::Reached && self.player.completed() {
                let result = self.poll_return(state, clock)?.expect("completed sample");
                return Ok(SampleProgress::Returned(result));
            }
            let progress = if pending == Progress::Reached {
                clock.advance_to(target)?
            } else {
                pending
            };
            match progress {
                Progress::Reached => return Ok(SampleProgress::Pending),
                Progress::Interrupt(irq) => self.dispatch(irq, memory, state, clock)?,
            }
            clock.check()?;
        }
    }

    /// The enclosing scheduler may need to deliver input at a deadline before
    /// dispatching the clock's entered IRQ. Keep the same sole sample cursor.
    pub(crate) fn dispatch<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        irq: Interrupt,
        memory: &impl SampleMemory,
        state: &mut SampleSoundState,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        match irq.irq {
            0 if irq.handler == self.timer_handler => self.player.timer_interrupt(memory, clock),
            1 if irq.handler == self.keyboard_handler => {
                self.player.keyboard_interrupt(state, clock)
            }
            _ => Err(invalid(
                "sample continuation encountered an unexpected installed handler",
            )),
        }
    }

    /// Call only after draining deliverable interrupts at this tick.
    pub(crate) fn poll_return<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        state: &mut SampleSoundState,
        clock: &mut Clock<O>,
    ) -> Result<Option<u16>, Error> {
        if self.returned {
            return Err(invalid("sample foreground has already returned"));
        }
        if !self.player.completed() {
            return Ok(None);
        }
        let result = self.player.finish(state, clock)?;
        clock.check()?;
        self.returned = true;
        Ok(Some(result))
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}
