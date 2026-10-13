// Copyright (c) 2026 analogued. Project license has not been selected.
//! Startup 537a and pregame 4f8d as suspended foreground continuations.
//!
//! The caller retains one device clock across sample calls, ordinary timer waits
//! and gameplay entry. Foreground operations and IRQ handlers have zero modeled
//! instruction cost; unverified instruction gaps are not assigned host delays.

use std::collections::VecDeque;

use super::{
    FarPointer, IntroProgram, IntroSequence, ProceduralProgram, SampleBus, SampleMemory,
    SamplePlayer, SampleRequest, SampleSoundState, SoundAction, read, write,
};
use crate::{
    Error,
    opening::execution::GameplayState,
    timer::{
        Clock, ClockEvent, Interrupt, Progress, SampleStart, SampleTask, Stamp, Wait,
        return_interrupt,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntroProgress {
    Pending,
    Returned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    BeginCycle,
    Sequence,
    BeginWait,
    Wait,
    Pregame,
    Returned,
}

/// Owns the caller's game state until the original foreground returns.
/// Schedule received input before crossing its timestamp. This task exclusively
/// advances the supplied clock while pending; samples use its existing vectors,
/// PIT and software-counter phase. Startup chooses its display outside this task.
pub struct IntroTask {
    program: IntroProgram,
    state: GameplayState,
    game_cs: u16,
    stage: Stage,
    sequence: Option<IntroSequence>,
    sample: Option<SampleTask>,
    sample_state: SampleSoundState,
    iterations: u16,
    scans: VecDeque<(u64, u8)>,
    scan_latched: bool,
    committed_tick: u64,
}

impl IntroTask {
    /// Begin outer intro 537a with its already-installed ordinary timer/key IRQs.
    /// The startup caller installs reset handler 7532 and stored divisor ffff
    /// before entering; this constructor does not reset the caller's device.
    pub fn begin<O: FnMut(Stamp, ClockEvent)>(
        program: &IntroProgram,
        state: GameplayState,
        game_cs: u16,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        Self::new(program, state, game_cs, Stage::BeginCycle, clock)
    }

    /// Begin the separate 4f8d wrapper after the caller draws the first game
    /// image. Its single sample return value is ignored by the original caller.
    pub fn begin_pregame<O: FnMut(Stamp, ClockEvent)>(
        program: &IntroProgram,
        state: GameplayState,
        game_cs: u16,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        Self::new(program, state, game_cs, Stage::Pregame, clock)
    }

    fn new<O: FnMut(Stamp, ClockEvent)>(
        program: &IntroProgram,
        state: GameplayState,
        game_cs: u16,
        stage: Stage,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        clock.check()?;
        let expected = state.procedural.handler.map(|handler| FarPointer {
            segment: game_cs,
            offset: handler.offset(),
        });
        if expected != Some(clock.handlers()[0])
            || clock.handlers()[1] != keyboard_handler(game_cs)
            || !clock.interrupts_enabled()
        {
            return Err(invalid(
                "intro entry requires matching ordinary timer/key vectors and IF",
            ));
        }
        Ok(Self {
            program: program.clone(),
            state,
            game_cs,
            stage,
            sequence: None,
            sample: None,
            sample_state: SampleSoundState::default(),
            iterations: 0,
            scans: VecDeque::new(),
            scan_latched: false,
            committed_tick: clock.tick(),
        })
    }

    pub fn state(&self) -> &GameplayState {
        &self.state
    }

    pub fn into_state(self) -> Result<GameplayState, Error> {
        if self.stage != Stage::Returned {
            return Err(invalid("intro foreground has not returned"));
        }
        Ok(self.state)
    }

    pub fn schedule_scan<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        tick: u64,
        raw: u8,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        self.check_clock(clock)?;
        if tick < clock.tick() || self.scans.back().is_some_and(|(last, _)| tick < *last) {
            return Err(invalid(
                "keyboard deliveries must be ordered and cannot precede committed time",
            ));
        }
        if self.stage == Stage::Returned {
            return Err(invalid("intro foreground has returned"));
        }
        self.scans.push_back((tick, raw));
        Ok(())
    }

    pub fn discard_scheduled_scans<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        self.check_clock(clock)?;
        self.scans.clear();
        Ok(())
    }

    /// Reset pending/held host input without synthesizing DOS input. A sample's
    /// already-received last scan remains part of that original call's result.
    pub fn clear_input<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        if self.scan_latched {
            return Err(invalid("input reset requires a serviced keyboard latch"));
        }
        self.discard_scheduled_scans(clock)?;
        self.state.keyboard.clear();
        self.state.joystick = 0;
        Ok(())
    }

    pub fn advance_until<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        target: u64,
        memory: &impl SampleMemory,
        procedural: &ProceduralProgram,
        clock: &mut Clock<O>,
    ) -> Result<IntroProgress, Error> {
        self.check_clock(clock)?;
        if target < clock.tick() {
            return Err(invalid("intro budget precedes committed time"));
        }
        let result = self.advance_inner(target, memory, procedural, clock);
        self.committed_tick = clock.tick();
        if let Err(error) = &result {
            clock.fail(error);
        }
        result
    }

    fn check_clock<O: FnMut(Stamp, ClockEvent)>(&self, clock: &Clock<O>) -> Result<(), Error> {
        clock.check()?;
        if clock.tick() != self.committed_tick {
            return Err(invalid("intro clock was advanced outside its pending task"));
        }
        Ok(())
    }

    fn advance_inner<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        target: u64,
        memory: &impl SampleMemory,
        procedural: &ProceduralProgram,
        clock: &mut Clock<O>,
    ) -> Result<IntroProgress, Error> {
        loop {
            if self.stage == Stage::Returned {
                return Ok(IntroProgress::Returned);
            }
            self.deliver_scan(clock)?;
            if let Progress::Interrupt(irq) = clock.advance_to(clock.tick())? {
                self.dispatch(irq, memory, procedural, clock)?;
                continue;
            }
            if let Some(sample) = &mut self.sample {
                if let Some(result) = sample.poll_return(&mut self.sample_state, clock)? {
                    self.sample = None;
                    self.state.keyboard.clear_table();
                    self.state.controls.sound_poll_counter &= 0xff00;
                    self.state.controls.preferences.sound_disabled = self.sample_state.disabled;
                    self.state.di = SamplePlayer::return_di();
                    if self.stage == Stage::Pregame {
                        self.stage = Stage::Returned;
                    } else {
                        self.sequence
                            .as_mut()
                            .expect("active sequence")
                            .complete(result)?;
                    }
                    continue;
                }
            } else {
                match self.stage {
                    Stage::BeginCycle => {
                        if self.state.controls.preferences.intro_audio_enabled {
                            self.sequence = Some(IntroSequence::new(&self.program));
                            self.stage = Stage::Sequence;
                        } else {
                            self.iterations = 0;
                            self.stage = Stage::BeginWait;
                        }
                        continue;
                    }
                    Stage::Sequence => {
                        let sequence = self.sequence.as_ref().expect("active sequence");
                        if let Some(result) = sequence.result() {
                            self.sequence = None;
                            self.stage = if result != 0 {
                                Stage::Returned
                            } else {
                                self.iterations = 0;
                                Stage::BeginWait
                            };
                        } else {
                            let request = sequence.current().expect("current intro call").request;
                            self.begin_sample(request, clock)?;
                        }
                        continue;
                    }
                    Stage::BeginWait => {
                        self.stage = Stage::Wait;
                        continue;
                    }
                    Stage::Wait => {
                        if Wait::from_call(1, 1).poll(&mut self.state.procedural.counters) {
                            if self.poll_input() != 0 {
                                self.stage = Stage::Returned;
                            } else {
                                self.iterations += 1;
                                self.stage = if self.iterations == 500 {
                                    Stage::BeginCycle
                                } else {
                                    Stage::BeginWait
                                };
                            }
                            continue;
                        }
                    }
                    Stage::Pregame => {
                        if self.state.controls.preferences.intro_audio_enabled {
                            self.begin_sample(self.program.pregame(), clock)?;
                        } else {
                            self.stage = Stage::Returned;
                        }
                        continue;
                    }
                    Stage::Returned => unreachable!("return handled above"),
                }
            }
            if clock.tick() == target {
                return Ok(IntroProgress::Pending);
            }
            let deadline = self
                .scans
                .front()
                .map_or(target, |(tick, _)| target.min(*tick));
            let progress = clock.advance_to(deadline)?;
            // Received same-tick input is latched before the entered IRQ's body,
            // its IRET and foreground polling. IRQ0 still has PIC priority.
            self.deliver_scan(clock)?;
            if let Progress::Interrupt(irq) = progress {
                self.dispatch(irq, memory, procedural, clock)?;
            }
        }
    }

    fn begin_sample<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        request: SampleRequest,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        self.sample_state.disabled = self.state.controls.preferences.sound_disabled;
        match SampleTask::begin(
            self.state.next_playback,
            request,
            &mut self.sample_state,
            self.game_cs,
            self.state.di,
            clock,
        )? {
            SampleStart::Playing(task) => {
                self.state.next_playback = self
                    .state
                    .next_playback
                    .checked_add(1)
                    .ok_or_else(|| invalid("playback identity overflow"))?;
                self.sample = Some(task);
            }
            SampleStart::RegularKeyboardPoll => {
                let result = u16::from(self.poll_input());
                if self.stage == Stage::Pregame {
                    self.stage = Stage::Returned;
                } else {
                    self.sequence
                        .as_mut()
                        .expect("active sequence")
                        .complete(result)?;
                }
            }
        }
        Ok(())
    }

    fn poll_input(&mut self) -> u8 {
        self.state
            .controls
            .poll(self.state.keyboard.held_keys(self.state.joystick))
            .mask
    }

    fn deliver_scan<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        if !self.scan_latched
            && self
                .scans
                .front()
                .is_some_and(|(tick, _)| *tick == clock.tick())
        {
            let (_, raw) = self.scans.pop_front().expect("due scan");
            clock.keyboard_input(raw)?;
            self.scan_latched = true;
        }
        Ok(())
    }

    fn dispatch<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        irq: Interrupt,
        memory: &impl SampleMemory,
        procedural: &ProceduralProgram,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        if irq.irq == 1 {
            self.scan_latched = false;
        }
        if let Some(sample) = &mut self.sample {
            return sample.dispatch(irq, memory, &mut self.sample_state, clock);
        }
        if irq.irq == 0 {
            self.state.procedural.dispatch(
                irq,
                procedural,
                self.game_cs,
                if self.state.controls.preferences.sound_disabled {
                    0xffff
                } else {
                    0
                },
                clock,
            )
        } else if irq.irq == 1 && irq.handler == keyboard_handler(self.game_cs) {
            clock.emit(SoundAction::InterruptsEnabled(false));
            let raw = read(clock, 0x60);
            let control = read(clock, 0x61);
            write(clock, 0x61, control | 0x80);
            write(clock, 0x61, control);
            self.state.keyboard.apply_scan(raw);
            return_interrupt(clock);
            clock.check()
        } else {
            Err(invalid("unknown installed intro keyboard handler"))
        }
    }
}

fn keyboard_handler(cs: u16) -> FarPointer {
    FarPointer {
        segment: cs,
        offset: 0x744a,
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{ByteOrigin, Effect, MemoryByte, Resource};
    use reforge_audio::speaker::PcSpeaker;
    use std::{cell::RefCell, rc::Rc};

    type Log = Rc<RefCell<Vec<(Stamp, ClockEvent)>>>;
    type TestClock = Clock<Box<dyn FnMut(Stamp, ClockEvent)>>;

    struct Memory;
    impl SampleMemory for Memory {
        fn read(&self, _: Resource, offset: u16) -> Result<MemoryByte, Error> {
            Ok(MemoryByte {
                value: offset as u8,
                origin: ByteOrigin::Asset,
            })
        }
    }

    fn programs() -> (IntroProgram, ProceduralProgram) {
        let intro = IntroProgram::from_expanded(&super::super::sequence::tests::fixture()).unwrap();
        let mut bytes = vec![0; 0x78c6];
        for (instruction, base, count) in [
            (0x7541, 0x7583, 17),
            (0x762a, 0x766c, 47),
            (0x774d, 0x778c, 14),
            (0x7833, 0x7872, 42),
        ] {
            bytes[instruction..instruction + 6].copy_from_slice(&[
                0xd1,
                0xe0,
                0x8d,
                0x1e,
                base as u8,
                (base >> 8) as u8,
            ]);
            for index in 0..count {
                bytes[base + index * 2..base + index * 2 + 2]
                    .copy_from_slice(&(1000 + index as u16).to_le_bytes());
            }
        }
        (intro, ProceduralProgram::from_expanded(&bytes).unwrap())
    }

    fn setup(log: Log) -> (GameplayState, TestClock) {
        let cs = 0x1234;
        let observe: Box<dyn FnMut(Stamp, ClockEvent)> =
            Box::new(move |stamp, event| log.borrow_mut().push((stamp, event)));
        let mut clock = Clock::new(
            PcSpeaker::new(0x60, [true; 3]),
            [FarPointer::default(), keyboard_handler(cs)],
            true,
            observe,
        );
        let mut state = GameplayState::default();
        state.procedural.request(Effect::Reset, cs, 0, &mut clock);
        state.procedural.counters.phase = 123;
        state.procedural.counters.units = 7;
        (state, clock)
    }

    fn starts(log: &Log) -> Vec<SampleRequest> {
        log.borrow()
            .iter()
            .filter_map(|(_, event)| match event {
                ClockEvent::Action(SoundAction::SourceBegin { request, .. }) => Some(*request),
                _ => None,
            })
            .collect()
    }

    fn skip_intro(chunk: u64) -> (Vec<(Stamp, ClockEvent)>, u64, String) {
        let log = Log::default();
        let (state, mut clock) = setup(log.clone());
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin(&intro, state, 0x1234, &clock).unwrap();
        task.schedule_scan(10, 0x39, &clock).unwrap();
        let returned = loop {
            let end = clock.tick() + chunk;
            assert!(end <= 2_000_000, "sample intro failed to return");
            if task
                .advance_until(end, &Memory, &procedural, &mut clock)
                .unwrap()
                == IntroProgress::Returned
            {
                break clock.tick();
            }
        };
        // Space waits for the current call, its group and the unconditional tail.
        assert_eq!(
            starts(&log)
                .iter()
                .map(|request| request.count)
                .collect::<Vec<_>>(),
            [100, 101, 125, 126, 127, 128]
        );
        let state = task.into_state().unwrap();
        assert_eq!(state.next_playback, 6);
        assert_eq!(state.di, SamplePlayer::return_di());
        assert_eq!(state.keyboard.held_keys(0).actions, 0);
        assert_eq!(state.procedural.counters.divisor, 0xffff);
        assert_eq!(clock.handlers()[0].offset, 0x7532);
        assert_eq!(clock.handlers()[1].offset, 0x744a);
        let events = log.borrow().clone();
        (events, returned, format!("{state:?}"))
    }

    #[test]
    fn space_during_sample_waits_for_group_and_tail_and_is_chunk_invariant() {
        assert_eq!(skip_intro(1_000_000), skip_intro(137));
    }

    #[test]
    fn zero_sequence_result_waits_500_times_then_repeats() {
        let log = Log::default();
        let (state, mut clock) = setup(log.clone());
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin(&intro, state, 0x1234, &clock).unwrap();
        while task.state.next_playback < 30 {
            let end = clock.tick() + 20_000;
            assert!(end < 20_000_000, "outer intro never repeated");
            assert_eq!(
                task.advance_until(end, &Memory, &procedural, &mut clock)
                    .unwrap(),
                IntroProgress::Pending
            );
        }
        assert_eq!(starts(&log).len(), 30);
        assert!(task.sample.is_some());
        // All 29 first-cycle calls completed, the wait loop ran 500 times, and
        // one fresh first call is pending on the same timer/phase context.
        assert_eq!(task.iterations, 500);
        assert_eq!(task.state.procedural.counters.divisor, 0xffff);
        assert_ne!(task.state.procedural.counters.phase, 123);
    }

    #[test]
    fn ordinary_space_is_polled_only_after_timer_wait() {
        let log = Log::default();
        let (mut state, mut clock) = setup(log);
        state.controls.preferences.intro_audio_enabled = false;
        state.procedural.counters.units = 0;
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin(&intro, state, 0x1234, &clock).unwrap();
        task.schedule_scan(10, 0x39, &clock).unwrap();
        assert_eq!(
            task.advance_until(10, &Memory, &procedural, &mut clock)
                .unwrap(),
            IntroProgress::Pending
        );
        assert_eq!(task.state.keyboard.held_keys(0).actions, 16);
        assert_eq!(
            task.advance_until(100_000, &Memory, &procedural, &mut clock)
                .unwrap(),
            IntroProgress::Returned
        );
        assert!(clock.tick() < 100_000);
        assert_eq!(task.state.next_playback, 0);
        assert_eq!(task.state.procedural.counters.units, 0);
    }

    #[test]
    fn disabled_samples_use_ordinary_keyboard_poll_and_preserve_di() {
        let log = Log::default();
        let (mut state, mut clock) = setup(log.clone());
        state.controls.preferences.sound_disabled = true;
        state.di = 17;
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin(&intro, state, 0x1234, &clock).unwrap();
        task.schedule_scan(0, 0x39, &clock).unwrap();
        assert_eq!(
            task.advance_until(0, &Memory, &procedural, &mut clock)
                .unwrap(),
            IntroProgress::Returned
        );
        assert!(starts(&log).is_empty());
        assert_eq!(task.state.di, 17);
        assert_eq!(task.state.keyboard.held_keys(0).actions, 16);
    }

    fn pregame(chunk: u64) -> (Vec<(Stamp, ClockEvent)>, u64, String) {
        let log = Log::default();
        let (state, mut clock) = setup(log.clone());
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin_pregame(&intro, state, 0x1234, &clock).unwrap();
        task.schedule_scan(10, 0x39, &clock).unwrap();
        task.schedule_scan(10, 0xb9, &clock).unwrap();
        assert_eq!(
            task.advance_until(10, &Memory, &procedural, &mut clock)
                .unwrap(),
            IntroProgress::Pending
        );
        assert_eq!(task.sample_state.last_scan, 0xb9);
        assert_eq!(task.state.keyboard.held_keys(0).actions, 0);
        loop {
            let target = clock.tick() + chunk;
            assert!(target <= 100_010, "pregame did not return");
            if task
                .advance_until(target, &Memory, &procedural, &mut clock)
                .unwrap()
                == IntroProgress::Returned
            {
                break;
            }
        }
        assert_eq!(starts(&log), [intro.pregame()]);
        let state = task.into_state().unwrap();
        assert_eq!(state.next_playback, 1);
        let events = log.borrow().clone();
        (events, clock.tick(), format!("{state:?}"))
    }

    #[test]
    fn pregame_keeps_running_after_space_break_and_is_chunk_invariant() {
        assert_eq!(pregame(100_000), pregame(137));
    }

    #[test]
    fn input_history_reset_and_ownership_checks_are_explicit() {
        let (state, mut clock) = setup(Log::default());
        let (intro, procedural) = programs();
        let mut task = IntroTask::begin(&intro, state, 0x1234, &clock).unwrap();
        task.schedule_scan(10, 0x39, &clock).unwrap();
        assert!(task.schedule_scan(9, 0xb9, &clock).is_err());
        task.clear_input(&clock).unwrap();
        assert!(task.scans.is_empty());
        task.advance_until(20, &Memory, &procedural, &mut clock)
            .unwrap();
        assert!(task.schedule_scan(10, 0xb9, &clock).is_err());
        assert!(
            task.advance_until(19, &Memory, &procedural, &mut clock)
                .is_err()
        );
        clock.advance_to(21).unwrap();
        assert!(task.clear_input(&clock).is_err());
        assert!(task.into_state().is_err());
    }
}
