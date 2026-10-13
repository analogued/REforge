// Copyright (c) 2026 analogued. Project license has not been selected.
//! Budgeted headless gameplay on the sound device's single simulation timeline.
//!
//! Foreground segments and handlers are atomic and have zero instruction cost.
//! Native pause's busy keyboard loops use an explicit polling interval in PIT
//! clocks; this is a deterministic approximation, not a guest CPU speed model.
//! No host time, audio output, rendering backend, or sleep drives execution here.

#[cfg(test)]
mod basic_tests;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use super::{
    Controls, Gameplay, KeyboardState, PauseControl, PauseEvent, PauseState, Steering,
    TransitionFrame,
    pass::{PassProgress, PassTask},
};
use crate::{
    Error,
    assets::GraphicsAssets,
    sound::{
        self, FarPointer, GameplayAction, ProceduralProgram, ProceduralState, SampleBus,
        SampleMemory, SamplePlayer, SampleSoundState,
    },
    timer::{
        Clock, ClockEvent, Interrupt, Progress, SampleStart, SampleTask, Stamp, Wait,
        return_interrupt,
    },
};

/// Explicit game-side execution context at entry to the main gameplay routine.
/// The caller supplies the corresponding PIT state and vectors in `Clock`.
#[derive(Clone, Debug)]
pub struct GameplayState {
    pub controls: Controls,
    pub keyboard: KeyboardState,
    pub pause: PauseControl,
    pub procedural: ProceduralState,
    pub di: u16,
    pub next_playback: u64,
    pub joystick: u8,
    /// Frontend option affecting gameplay steering only. Ordinary function-key
    /// processing and native pause decisions still consume the keyboard state.
    pub steering_enabled: bool,
}

impl Default for GameplayState {
    fn default() -> Self {
        Self {
            controls: Controls::default(),
            keyboard: KeyboardState::default(),
            pause: PauseControl::default(),
            procedural: ProceduralState::default(),
            di: 0,
            next_playback: 0,
            joystick: 0,
            steering_enabled: true,
        }
    }
}

pub struct GameplayResources<'a, M: SampleMemory> {
    pub graphics: &'a GraphicsAssets,
    pub procedural: &'a ProceduralProgram,
    pub samples: &'a M,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameplayEvent {
    Action(GameplayAction),
    WaitReturned { site: u16 },
    InputPoll { site: u16, mask: u8 },
    Pause(PauseEvent),
    Published { body: u16, pass: u8 },
    TransitionPublished { body: u16, pass: u8, step: u8 },
    Returned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameplayProgress {
    Pending,
    /// Stops at publication before the pass tail, including the body's noise call.
    Published {
        body: u16,
        pass: u8,
    },
    /// An expanding scene region, before resuming the suspended foreground.
    TransitionPublished {
        body: u16,
        pass: u8,
        step: u8,
    },
    Returned,
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Start,
    PauseCheck,
    PausePoll,
    BeginPass,
    RunPass,
    Publish,
    Returned,
}

/// Owns the game while suspended so callers cannot mutate a pending continuation.
/// Schedule already-received input before advancing across its tick. Input at a
/// deadline is latched before IRQ dispatch/foreground polling; IRQ0 retains its
/// priority over IRQ1. Already-executed polls are never rolled back.
pub struct GameplayTask {
    game: Gameplay,
    state: GameplayState,
    game_cs: u16,
    pause_poll_period: u64,
    pause_poll_at: u64,
    stage: Stage,
    pass: Option<PassTask>,
    wait: Option<(u16, Wait)>,
    sample: Option<SampleTask>,
    sample_state: SampleSoundState,
    scans: VecDeque<(u64, u8)>,
    scan_latched: bool,
    committed_tick: u64,
    transition_period: Option<u64>,
    transition: Option<TransitionPlayback>,
    transition_frame: Option<TransitionFrame>,
}

struct TransitionPlayback {
    frames: VecDeque<TransitionFrame>,
    deadline: u64,
    step: u8,
}

impl GameplayTask {
    pub fn begin<O: FnMut(Stamp, ClockEvent)>(
        game: Gameplay,
        state: GameplayState,
        game_cs: u16,
        pause_poll_period: u64,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        clock.check()?;
        let expected = state.procedural.handler.map(|handler| FarPointer {
            segment: game_cs,
            offset: handler.offset(),
        });
        if expected != Some(clock.handlers()[0]) || clock.handlers()[1] != keyboard_handler(game_cs)
        {
            return Err(invalid(
                "gameplay entry requires matching procedural and ordinary keyboard vectors",
            ));
        }
        if !clock.interrupts_enabled()
            || pause_poll_period == 0
            || state.pause.state != PauseState::Running
        {
            return Err(invalid(
                "gameplay entry requires IF, running foreground, and a positive pause poll interval",
            ));
        }
        Ok(Self {
            game,
            state,
            game_cs,
            pause_poll_period,
            pause_poll_at: clock.tick(),
            stage: Stage::Start,
            pass: None,
            wait: None,
            sample: None,
            sample_state: SampleSoundState::default(),
            scans: VecDeque::new(),
            scan_latched: false,
            committed_tick: clock.tick(),
            transition_period: None,
            transition: None,
            transition_frame: None,
        })
    }

    pub fn game(&self) -> &Gameplay {
        &self.game
    }
    pub fn state(&self) -> &GameplayState {
        &self.state
    }

    /// Normalize each reveal region to a fixed simulation interval, independent
    /// of CPU execution cost. Immediate/headless stepping remains the default.
    /// Configure at task entry; earlier untimed captures are historical images.
    pub fn with_transition_period(mut self, period: u64) -> Result<Self, Error> {
        if period == 0 {
            return Err(invalid("transition interval must be positive"));
        }
        if self.pass.is_some() || self.transition.is_some() {
            return Err(invalid("configure transitions before executing a pass"));
        }
        self.discard_transitions();
        self.transition_period = Some(period);
        Ok(self)
    }

    /// The last region published by timed transition execution.
    pub fn transition_frame(&self) -> Option<&TransitionFrame> {
        self.transition_frame.as_ref()
    }

    /// Discard saved scene-transition images after their compositor updates.
    /// The current framebuffer, gameplay state and pending continuation remain
    /// unchanged. A presenter that omits the intermediate steps must drain them
    /// rather than retaining every transition for the lifetime of a game.
    pub fn discard_transitions(&mut self) -> usize {
        let count = self.game.transitions.len();
        self.game.transitions.clear();
        count
    }

    /// Recover foreground state only after its typed return; a suspended pass
    /// or sample must not be dropped and then resumed as a fresh game.
    pub fn into_parts(self) -> Result<(Gameplay, GameplayState), Error> {
        if !matches!(self.stage, Stage::Returned) {
            return Err(invalid("gameplay foreground has not returned"));
        }
        Ok((self.game, self.state))
    }

    /// Discard received input that has not reached the simulated controller.
    pub fn discard_scheduled_scans<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        clock.check()?;
        self.check_clock(clock)?;
        self.scans.clear();
        Ok(())
    }

    /// A frontend focus reset discards pending input history and held keys,
    /// including the IRQ duplicate filter, while retaining native pause state.
    /// No synthetic make/break bytes or pause actions are generated.
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

    /// This task is the sole driver of its clock while pending.
    /// Arrival after a consumed poll can only affect future execution. A caller
    /// may schedule at the current tick but must not backdate delivery.
    pub fn schedule_scan<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        tick: u64,
        raw: u8,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        clock.check()?;
        self.check_clock(clock)?;
        if tick < clock.tick() || self.scans.back().is_some_and(|(last, _)| tick < *last) {
            return Err(invalid(
                "keyboard deliveries must be ordered and cannot precede committed time",
            ));
        }
        if matches!(self.stage, Stage::Returned) {
            return Err(invalid("gameplay foreground has returned"));
        }
        self.scans.push_back((tick, raw));
        Ok(())
    }

    /// Stop at the budget, one publication, or main-loop return. Reuse remaining
    /// budget after publication to run the pending tail. Output cannot influence
    /// simulation: observers receive completed operations and cannot re-enter it.
    pub fn advance_until<O: FnMut(Stamp, ClockEvent), M: SampleMemory>(
        &mut self,
        target: u64,
        resources: &GameplayResources<'_, M>,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<GameplayProgress, Error> {
        clock.check()?;
        self.check_clock(clock)?;
        if target < clock.tick() {
            return Err(invalid("gameplay budget precedes committed time"));
        }
        let result = self.advance_inner(target, resources, clock, observe);
        self.committed_tick = clock.tick();
        if let Err(error) = &result {
            clock.fail(error);
        }
        result
    }

    fn check_clock<O: FnMut(Stamp, ClockEvent)>(&self, clock: &Clock<O>) -> Result<(), Error> {
        if clock.tick() != self.committed_tick {
            return Err(invalid(
                "gameplay clock was advanced outside its pending task",
            ));
        }
        Ok(())
    }

    fn advance_inner<O: FnMut(Stamp, ClockEvent), M: SampleMemory>(
        &mut self,
        target: u64,
        resources: &GameplayResources<'_, M>,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<GameplayProgress, Error> {
        loop {
            if matches!(self.stage, Stage::Returned) {
                return Ok(GameplayProgress::Returned);
            }
            self.deliver_scans(clock)?;
            if let Progress::Interrupt(irq) = clock.advance_to(clock.tick())? {
                self.dispatch(irq, resources, clock)?;
                continue;
            }
            if let Some(transition) = &mut self.transition {
                if clock.tick() >= transition.deadline {
                    if let Some(frame) = transition.frames.pop_front() {
                        transition.step += 1;
                        transition.deadline = clock
                            .tick()
                            .checked_add(self.transition_period.expect("timed transition"))
                            .ok_or_else(|| invalid("transition deadline overflow"))?;
                        let progress = GameplayProgress::TransitionPublished {
                            body: self.game.body,
                            pass: self.game.render_pass,
                            step: transition.step,
                        };
                        self.transition_frame = Some(frame);
                        observe(
                            clock.tick(),
                            GameplayEvent::TransitionPublished {
                                body: self.game.body,
                                pass: self.game.render_pass,
                                step: transition.step,
                            },
                        );
                        return Ok(progress);
                    }
                    self.transition = None;
                    continue;
                }
            } else if let Some(sample) = &mut self.sample {
                if let Some(last_scan) = sample.poll_return(&mut self.sample_state, clock)? {
                    self.sample = None;
                    self.state.keyboard.clear_table();
                    self.state.controls.sound_poll_counter &= 0xff00;
                    self.state.controls.preferences.sound_disabled = self.sample_state.disabled;
                    self.state.di = SamplePlayer::return_di();
                    // The return scan is already reflected in sample_state;
                    // it does not pass through the ordinary duplicate filter.
                    debug_assert_eq!(last_scan, u16::from(self.sample_state.last_scan));
                    continue;
                }
            } else if let Some((site, wait)) = self.wait {
                if wait.poll(&mut self.state.procedural.counters) {
                    self.wait = None;
                    observe(clock.tick(), GameplayEvent::WaitReturned { site });
                    if matches!(self.stage, Stage::PausePoll) {
                        self.state.pause.tick();
                        self.pause_poll_at = clock.tick();
                    }
                    continue;
                }
            } else {
                match self.stage {
                    Stage::Start => {
                        if self.game.complete() {
                            self.request_return(clock, observe);
                            clock.check()?;
                            self.stage = Stage::Returned;
                            observe(clock.tick(), GameplayEvent::Returned);
                            return Ok(GameplayProgress::Returned);
                        }
                        let first = self.game.at_body_boundary();
                        let site = if first {
                            0x04d0
                        } else if self.game.render_pass == 1 {
                            0x069c
                        } else {
                            0x06e0
                        };
                        self.begin_wait(site, 1, clock.tick(), observe);
                        self.stage = if first {
                            Stage::PauseCheck
                        } else {
                            Stage::BeginPass
                        };
                        continue;
                    }
                    Stage::PauseCheck => {
                        self.poll_pause(0x2691, clock, observe)?;
                        self.stage = if self.state.pause.state == PauseState::Running {
                            Stage::BeginPass
                        } else {
                            Stage::PausePoll
                        };
                        self.pause_poll_at = clock.tick();
                        continue;
                    }
                    Stage::PausePoll if clock.tick() >= self.pause_poll_at => {
                        match self.state.pause.state {
                            PauseState::WaitBeforePress(remaining) => self.begin_wait(
                                if remaining == 2 { 0x26a7 } else { 0x26b0 },
                                1,
                                clock.tick(),
                                observe,
                            ),
                            PauseState::WaitBeforeDecision(remaining) => self.begin_wait(
                                if remaining == 2 { 0x26c0 } else { 0x26c9 },
                                1,
                                clock.tick(),
                                observe,
                            ),
                            _ => {
                                let site = match self.state.pause.state {
                                    PauseState::AwaitInitialRelease => 0x269c,
                                    PauseState::AwaitPress => 0x26b5,
                                    PauseState::SampleDecision => 0x26ce,
                                    PauseState::AwaitFinalRelease => 0x26df,
                                    _ => return Err(invalid("invalid native pause continuation")),
                                };
                                self.poll_pause(site, clock, observe)?;
                                if self.state.pause.state == PauseState::Running {
                                    self.stage = Stage::BeginPass;
                                }
                                self.pause_poll_at = clock
                                    .tick()
                                    .checked_add(self.pause_poll_period)
                                    .ok_or_else(|| invalid("pause deadline overflow"))?;
                            }
                        }
                        continue;
                    }
                    Stage::BeginPass => {
                        // 2289 runs even when damage response replaces its mask.
                        let mask = self.poll_input(0x2289, clock.tick(), observe);
                        self.pass = Some(PassTask::begin(&mut self.game));
                        self.stage = Stage::RunPass;
                        let input = if self.state.steering_enabled {
                            steering(mask)
                        } else {
                            Steering::default()
                        };
                        self.resume_pass(resources.graphics, input, clock, observe)?;
                        continue;
                    }
                    Stage::RunPass => {
                        // Steering is consumed only in the first continuation.
                        self.resume_pass(resources.graphics, Steering::default(), clock, observe)?;
                        continue;
                    }
                    Stage::Publish => {
                        self.pass
                            .as_mut()
                            .expect("pending pass")
                            .publish(&mut self.game);
                        self.stage = Stage::RunPass;
                        self.transition_frame = None;
                        let body = self.game.body;
                        let pass = self.game.render_pass;
                        observe(clock.tick(), GameplayEvent::Published { body, pass });
                        return Ok(GameplayProgress::Published { body, pass });
                    }
                    _ => {}
                }
            }
            if clock.tick() == target {
                return Ok(GameplayProgress::Pending);
            }
            let mut deadline = target;
            if let Some((tick, _)) = self.scans.front() {
                deadline = deadline.min(*tick);
            }
            if let Some(transition) = &self.transition {
                deadline = deadline.min(transition.deadline);
            }
            if self.sample.is_none()
                && self.wait.is_none()
                && matches!(self.stage, Stage::PausePoll)
            {
                deadline = deadline.min(self.pause_poll_at);
            }
            let progress = clock.advance_to(deadline)?;
            // At the same tick IRQ0 may already be entered, but a received byte
            // must become pending before its IRET and foreground return tests.
            self.deliver_scans(clock)?;
            if let Progress::Interrupt(irq) = progress {
                self.dispatch(irq, resources, clock)?;
            }
        }
    }

    fn resume_pass<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        assets: &GraphicsAssets,
        input: Steering,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<(), Error> {
        let progress = {
            let state = &mut self.state;
            let cs = self.game_cs;
            let mut sound = |action| apply_request(state, cs, action, clock, observe);
            self.pass.as_mut().expect("pending pass").resume(
                &mut self.game,
                assets,
                input,
                &mut sound,
            )?
        };
        clock.check()?;
        match progress {
            PassProgress::Action(action) => {
                observe(clock.tick(), GameplayEvent::Action(action));
                match action {
                    GameplayAction::Wait { site, units } => {
                        self.wait = Some((site, Wait::from_call(units, units)))
                    }
                    GameplayAction::IntroSample { request, .. } => {
                        if self.state.controls.preferences.intro_audio_enabled {
                            self.sample_state.disabled =
                                self.state.controls.preferences.sound_disabled;
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
                                    self.poll_input(0x73b2, clock.tick(), observe);
                                }
                            }
                        }
                    }
                    GameplayAction::Request { .. } => unreachable!("requests are immediate"),
                }
            }
            PassProgress::Publish { wait_required } => {
                self.stage = Stage::Publish;
                if wait_required {
                    self.begin_wait(0x0550, 1, clock.tick(), observe);
                }
            }
            PassProgress::Returned => {
                self.pass = None;
                self.stage = Stage::Start;
            }
            PassProgress::Transition => {
                if self.transition_period.is_some() {
                    self.transition = Some(TransitionPlayback {
                        frames: self.game.take_transitions().into(),
                        deadline: clock.tick(),
                        step: 0,
                    });
                }
            }
        }
        Ok(())
    }

    fn begin_wait(
        &mut self,
        site: u16,
        units: u16,
        tick: u64,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) {
        observe(
            tick,
            GameplayEvent::Action(GameplayAction::Wait { site, units }),
        );
        self.wait = Some((site, Wait::from_call(units, units)));
    }

    fn poll_input(
        &mut self,
        site: u16,
        tick: u64,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> u8 {
        let poll = self
            .state
            .keyboard
            .poll_controls(&mut self.state.controls, self.state.joystick);
        if poll.display_toggled {
            self.game.reset_video_palette();
        }
        observe(
            tick,
            GameplayEvent::InputPoll {
                site,
                mask: poll.mask,
            },
        );
        poll.mask
    }

    fn poll_pause<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        site: u16,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<(), Error> {
        let mask = self.poll_input(site, clock.tick(), observe);
        let state = &mut self.state;
        let event = state.pause.poll_with_sound(mask, &mut |action| {
            observe(clock.tick(), GameplayEvent::Action(action));
            if let GameplayAction::Request { effect, .. } = action {
                state.procedural.request(
                    effect,
                    self.game_cs,
                    disable_word(state.controls.preferences.sound_disabled),
                    clock,
                );
            }
        });
        if event == PauseEvent::EndGame {
            self.game.request_game_over();
        }
        if event != PauseEvent::None {
            observe(clock.tick(), GameplayEvent::Pause(event));
        }
        clock.check()
    }

    fn request_return<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) {
        let state = &mut self.state;
        self.game
            .return_sound(&mut |action| apply_request(state, self.game_cs, action, clock, observe));
    }

    fn deliver_scans<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        // Same-tick bytes are serviced one at a time through the one-byte latch.
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

    fn dispatch<O: FnMut(Stamp, ClockEvent), M: SampleMemory>(
        &mut self,
        irq: Interrupt,
        resources: &GameplayResources<'_, M>,
        clock: &mut Clock<O>,
    ) -> Result<(), Error> {
        if irq.irq == 1 {
            self.scan_latched = false;
        }
        if let Some(sample) = &mut self.sample {
            return sample.dispatch(irq, resources.samples, &mut self.sample_state, clock);
        }
        if irq.irq == 0 {
            self.state.procedural.dispatch(
                irq,
                resources.procedural,
                self.game_cs,
                disable_word(self.state.controls.preferences.sound_disabled),
                clock,
            )
        } else if irq.irq == 1 && irq.handler == keyboard_handler(self.game_cs) {
            clock.emit(sound::SoundAction::InterruptsEnabled(false));
            let raw = sound::read(clock, 0x60);
            let control = sound::read(clock, 0x61);
            sound::write(clock, 0x61, control | 0x80);
            sound::write(clock, 0x61, control);
            self.state.keyboard.apply_scan(raw);
            return_interrupt(clock);
            clock.check()
        } else {
            Err(invalid("unknown installed gameplay keyboard handler"))
        }
    }
}

fn apply_request<O: FnMut(Stamp, ClockEvent)>(
    state: &mut GameplayState,
    cs: u16,
    action: GameplayAction,
    clock: &mut Clock<O>,
    observe: &mut impl FnMut(u64, GameplayEvent),
) {
    observe(clock.tick(), GameplayEvent::Action(action));
    if let GameplayAction::Request { effect, .. } = action {
        state.procedural.request(
            effect,
            cs,
            disable_word(state.controls.preferences.sound_disabled),
            clock,
        );
    } else {
        unreachable!("only requests execute synchronously");
    }
}

fn keyboard_handler(cs: u16) -> FarPointer {
    FarPointer {
        segment: cs,
        offset: 0x744a,
    }
}
fn disable_word(disabled: bool) -> u16 {
    if disabled { 0xffff } else { 0 }
}
fn steering(mask: u8) -> Steering {
    Steering {
        up: mask & 1 != 0,
        down: mask & 2 != 0,
        left: mask & 4 != 0,
        right: mask & 8 != 0,
        fire: mask & 16 != 0,
    }
}
fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}
