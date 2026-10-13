// Copyright (c) 2026 analogued. Project license has not been selected.
//! Live presentation of the reconstructed device/foreground timeline.

use super::*;
#[cfg(test)]
mod tests;
use crate::{
    live::GameplayRuntime,
    opening::execution::{GameplayEvent, GameplayProgress, GameplayResources, GameplayState},
    sound::{
        DosSampleMemory, Effect, FarPointer, IntroProgram, IntroProgress, IntroTask,
        ProceduralProgram, SampleBus, SoundAction, SoundAssets,
    },
    timer::{Clock, ClockEvent, Progress, Stamp, Wait},
};
use reforge_audio::{
    PIT_HZ,
    render::{BlipRenderer, Config, SignalRenderer},
    speaker::PcSpeaker,
};
use reforge_sdl::audio::AudioOutput;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

const RATE: u32 = 48_000;
const CHUNK: u64 = 11_931;
const PREFILL: usize = RATE as usize * 30 / 1000;
const CS: u16 = 0x1234;
type DeviceClock = Clock<Box<dyn FnMut(Stamp, ClockEvent)>>;

pub(super) struct Resources {
    pub samples: SoundAssets,
    pub intro: IntroProgram,
    pub procedural: ProceduralProgram,
}

struct EndingResources<'a> {
    frames: &'a StartupFrames,
    sound: &'a Resources,
}

struct PcmBlock {
    start: u64,
    samples: Vec<i16>,
}
struct Synthesis {
    renderer: BlipRenderer,
    blocks: VecDeque<PcmBlock>,
    generated: u64,
    error: Option<String>,
}
impl Synthesis {
    fn new() -> Result<Self, String> {
        Ok(Self {
            renderer: BlipRenderer::new(
                Config {
                    clock_hz: PIT_HZ as u32,
                    sample_rate: RATE,
                    chunk_ticks: CHUNK as u32,
                },
                0,
                0,
            )
            .map_err(|e| e.to_string())?,
            blocks: VecDeque::new(),
            generated: 0,
            error: None,
        })
    }
    fn advance(&mut self, tick: u64, level: Option<i16>) {
        if self.error.is_some() {
            return;
        }
        let Self {
            renderer,
            blocks,
            generated,
            ..
        } = self;
        let mut output = |samples: &[i16]| {
            blocks.push_back(PcmBlock {
                start: *generated,
                samples: samples.to_vec(),
            });
            *generated += samples.len() as u64;
            Ok(())
        };
        let result = if let Some(level) = level {
            renderer.level_at(tick, level, &mut output)
        } else {
            renderer.finish_until(tick, &mut output)
        };
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}

struct PlaybackAudio {
    signal: Rc<RefCell<Synthesis>>,
    output: Option<AudioOutput>,
    started: bool,
    recovering: bool,
}
impl PlaybackAudio {
    fn new(signal: Rc<RefCell<Synthesis>>) -> Self {
        let output = AudioOutput::new(RATE)
            .map_err(|e| {
                eprintln!("warning: audio output unavailable; simulation continues: {e}");
            })
            .ok();
        Self {
            signal,
            output,
            started: false,
            recovering: false,
        }
    }
    fn pause(&mut self, paused: bool) {
        let result = self
            .output
            .as_mut()
            .map(|o| o.set_paused(paused || !self.started));
        if let Some(Err(error)) = result {
            self.disable(error);
        }
    }
    fn disable(&mut self, error: String) {
        eprintln!("warning: audio output stopped; simulation continues: {error}");
        self.output = None;
    }
    fn service(&mut self, committed: u64, wanted: u64) {
        self.signal.borrow_mut().advance(committed, None);
        let mut signal = self.signal.borrow_mut();
        if let Some(error) = signal.error.take() {
            // A synthesis failure is permanent. Retain the stopped renderer;
            // future device execution must still proceed independently.
            signal.error = Some(error.clone());
            signal.blocks.clear();
            drop(signal);
            if self.output.is_some() {
                self.disable(error);
            }
            return;
        }
        let cutoff = samples_at(wanted).saturating_sub(PREFILL as u64);
        let overdue = wanted.saturating_sub(committed) > PIT_HZ / 10;
        let mut error = None;
        if let Some(output) = &mut self.output {
            if overdue && !self.recovering {
                error = output.set_paused(true).and_then(|_| output.clear()).err();
                self.started = false;
                self.recovering = true;
            }
            while let Some(block) = signal.blocks.pop_front() {
                let skip = cutoff
                    .saturating_sub(block.start)
                    .min(block.samples.len() as u64) as usize;
                let samples = &block.samples[skip..];
                if samples.is_empty() || error.is_some() {
                    continue;
                }
                match output.queued_samples() {
                    Ok(queued) if queued + samples.len() > RATE as usize / 10 => {
                        // Discard stale presentation data, never device phase.
                        error = output.set_paused(true).and_then(|_| output.clear()).err();
                        self.started = false;
                    }
                    Err(e) => error = Some(e),
                    _ => {}
                }
                if error.is_none() {
                    error = output.queue(samples).err();
                }
            }
            if !overdue {
                self.recovering = false;
            }
            if !self.started && !overdue && error.is_none() {
                match output.queued_samples() {
                    Ok(queued) if queued >= PREFILL => {
                        error = output.set_paused(false).err();
                        self.started = error.is_none();
                    }
                    Err(e) => error = Some(e),
                    _ => {}
                }
            }
        } else {
            signal.blocks.clear();
        }
        drop(signal);
        if let Some(error) = error {
            self.disable(error);
        }
    }
}

fn samples_at(tick: u64) -> u64 {
    (u128::from(tick) * u128::from(RATE) / u128::from(PIT_HZ)) as u64
}
fn ticks_since(epoch: Instant, now: Instant) -> u64 {
    (now.saturating_duration_since(epoch).as_nanos() * u128::from(PIT_HZ) / 1_000_000_000)
        .min(u128::from(u64::MAX)) as u64
}
#[cfg(feature = "dev-tools")]
fn instant_at(epoch: Instant, tick: u64) -> Instant {
    epoch + Duration::from_nanos((u128::from(tick) * 1_000_000_000 / u128::from(PIT_HZ)) as u64)
}

// Native input, waits and sound routines run even with no audio device.
enum Phase {
    Splash,
    Intro(Box<IntroTask>),
    Pregame {
        task: Box<IntroTask>,
        game: Box<Opening>,
    },
    Playing(Box<GameplayRuntime>),
    Ending {
        ending: Box<Ending>,
        state: Box<GameplayState>,
        step: EndingStep,
    },
}

#[derive(Clone, Copy)]
enum EndingStep {
    Entry { initial: bool, poll_at: u64 },
    Poll,
    Wait(Wait),
    Finish,
    Intertitle { second: bool, wait: Wait },
}
struct Simulation {
    clock: DeviceClock,
    phase: Phase,
    initial: Option<GameplayState>,
    session: SessionState,
    visible: Frame,
    intro_choice: bool,
    palette_family: u8,
    scans: VecDeque<(u64, u8)>,
}
impl Simulation {
    fn new(frames: &StartupFrames, signal: Rc<RefCell<Synthesis>>) -> Result<Self, String> {
        let observe: Box<dyn FnMut(Stamp, ClockEvent)> = Box::new(move |_, event| {
            if let ClockEvent::Device(events) = event {
                if let Some(edge) = events.drive {
                    signal
                        .borrow_mut()
                        .advance(edge.tick, Some(if edge.high { 8192 } else { 0 }));
                }
            }
        });
        let mut clock = Clock::new(
            PcSpeaker::with_mode3_count_one(
                0x60,
                [true; 3],
                reforge_audio::pit::CountOnePolicy::DosBoxWrap,
            ),
            [
                FarPointer::default(),
                FarPointer {
                    segment: CS,
                    offset: 0x744a,
                },
            ],
            true,
            observe,
        );
        let mut state = GameplayState::default();
        state.procedural.request(Effect::Reset, CS, 0, &mut clock);
        clock.check().map_err(|e| e.to_string())?;
        let mut session = SessionState::default();
        let intro_choice = session.select_intro();
        Ok(Self {
            clock,
            phase: Phase::Splash,
            initial: Some(state),
            session,
            visible: frames.frame(Screen::Splash).clone(),
            intro_choice,
            palette_family: 1,
            scans: VecDeque::new(),
        })
    }
    fn start_intro(
        &mut self,
        state: GameplayState,
        frames: &StartupFrames,
        resources: &Resources,
    ) -> Result<(), String> {
        self.visible = frames.return_intro(self.intro_choice).clone();
        self.palette_family = 1;
        self.phase = Phase::Intro(Box::new(
            IntroTask::begin(&resources.intro, state, CS, &self.clock)
                .map_err(|e| e.to_string())?,
        ));
        Ok(())
    }
    fn state(&self) -> &GameplayState {
        match &self.phase {
            Phase::Splash => self.initial.as_ref().unwrap(),
            Phase::Intro(task) | Phase::Pregame { task, .. } => task.state(),
            Phase::Playing(runtime) => runtime.state(),
            Phase::Ending { state, .. } => state,
        }
    }
    fn sync_session(&mut self) {
        let state = self.state();
        let (controls, keyboard, pause) = (
            state.controls.clone(),
            state.keyboard.clone(),
            state.pause.clone(),
        );
        self.session.controls = controls;
        self.session.keyboard = keyboard;
        self.session.pause = pause;
    }
    fn clear_input(&mut self) -> Result<(), String> {
        self.scans.clear();
        let result = match &mut self.phase {
            Phase::Intro(task) | Phase::Pregame { task, .. } => task.clear_input(&self.clock),
            Phase::Playing(runtime) => runtime.clear_input(&self.clock),
            Phase::Ending { state, .. } => {
                state.keyboard.clear();
                state.joystick = 0;
                Ok(())
            }
            Phase::Splash => {
                self.initial.as_mut().unwrap().keyboard.clear();
                Ok(())
            }
        };
        result.map_err(|e| e.to_string())?;
        self.sync_session();
        Ok(())
    }
    fn receive(&mut self, tick: u64, raw: u8) {
        // Preserve receipt order for equal or uncertain/nonmonotonic host
        // timestamps. Already executed polls are never rewritten.
        let tick = tick
            .max(self.clock.tick())
            .max(self.scans.back().map_or(0, |(at, _)| *at));
        self.scans.push_back((tick, raw));
    }
    fn advance(
        &mut self,
        target: u64,
        frames: &StartupFrames,
        rules: &ScriptRules,
        resources: &Resources,
        controls: bool,
        events: &mut Vec<(u64, GameplayEvent)>,
    ) -> Result<bool, String> {
        let memory = DosSampleMemory::new(&resources.samples, *b"MACH3\0\0\0");
        let mut repaint = false;
        let display_before = (self.display_mode(), self.palette());
        let before = std::mem::discriminant(&self.phase);
        let mut assigned = Vec::new();
        let target = self.scans.front().map_or(target, |(at, _)| target.min(*at));
        // Assign only the earliest timestamp group. If the foreground returns
        // before that deadline, its future bytes remain input to the next phase.
        while self.scans.front().is_some_and(|(tick, _)| *tick <= target) {
            let (tick, raw) = self.scans.pop_front().unwrap();
            match &mut self.phase {
                Phase::Intro(task) | Phase::Pregame { task, .. } => {
                    task.schedule_scan(tick, raw, &self.clock)
                        .map_err(|e| e.to_string())?;
                }
                Phase::Playing(runtime) => {
                    runtime
                        .schedule_scan(tick, raw, &self.clock)
                        .map_err(|e| e.to_string())?;
                }
                _ => {
                    self.scans.push_front((tick, raw));
                    break;
                }
            }
            assigned.push((tick, raw));
        }
        loop {
            let phase = std::mem::replace(&mut self.phase, Phase::Splash);
            match phase {
                Phase::Splash => {
                    self.phase = Phase::Splash;
                    return Ok(false);
                }
                Phase::Intro(mut task) => {
                    let progress = task
                        .advance_until(target, &memory, &resources.procedural, &mut self.clock)
                        .map_err(|e| e.to_string())?;
                    if progress == IntroProgress::Returned {
                        let mut state = task.into_state().map_err(|e| e.to_string())?;
                        self.session.controls = state.controls.clone();
                        self.session.keyboard = state.keyboard.clone();
                        let start = self.session.begin_game();
                        let game = Opening::for_play(frames.graphics(), rules.clone(), start)
                            .map_err(|e| e.to_string())?;
                        state.steering_enabled = controls;
                        self.visible = game.frame().clone();
                        self.phase = Phase::Pregame {
                            task: Box::new(
                                IntroTask::begin_pregame(&resources.intro, state, CS, &self.clock)
                                    .map_err(|e| e.to_string())?,
                            ),
                            game: Box::new(game),
                        };
                        repaint = true;
                        break;
                    } else {
                        self.phase = Phase::Intro(task);
                        break;
                    }
                }
                Phase::Pregame { mut task, game } => {
                    let progress = task
                        .advance_until(target, &memory, &resources.procedural, &mut self.clock)
                        .map_err(|e| e.to_string())?;
                    if progress == IntroProgress::Returned {
                        let state = task.into_state().map_err(|e| e.to_string())?;
                        self.phase = Phase::Playing(Box::new(
                            GameplayRuntime::begin(*game, state, CS, 64, &self.clock)
                                .map_err(|e| e.to_string())?,
                        ));
                        repaint = true;
                        break;
                    } else {
                        self.phase = Phase::Pregame { task, game };
                        break;
                    }
                }
                Phase::Playing(mut runtime) => {
                    let progress = runtime
                        .advance_until(
                            target,
                            &GameplayResources {
                                graphics: frames.graphics(),
                                procedural: &resources.procedural,
                                samples: &memory,
                            },
                            &mut self.clock,
                            &mut |tick, event| events.push((tick, event)),
                        )
                        .map_err(|e| e.to_string())?;
                    runtime.synchronize_session(&mut self.session);
                    self.palette_family = runtime.game().palette_family();
                    match progress {
                        GameplayProgress::Pending => {
                            self.phase = Phase::Playing(runtime);
                            break;
                        }
                        GameplayProgress::Published { .. } => {
                            self.visible = runtime.visible_frame().clone();
                            repaint = true;
                            self.phase = Phase::Playing(runtime);
                        }
                        GameplayProgress::Returned => {
                            let (game, state) = runtime.into_parts().map_err(|e| e.to_string())?;
                            // GameplayTask already executes the caller's 0452
                            // reset through Opening::return_sound exactly once.
                            let ending = Ending::pending_with_best(
                                &game.snapshot(),
                                game.frame(),
                                self.session.best_score,
                            );
                            self.visible = ending.frame().clone();
                            let step = EndingStep::Entry {
                                initial: true,
                                poll_at: self.clock.tick(),
                            };
                            self.phase = Phase::Ending {
                                ending: Box::new(ending),
                                state: Box::new(state),
                                step,
                            };
                            repaint = true;
                            break;
                        }
                    }
                }
                Phase::Ending {
                    mut ending,
                    mut state,
                    step,
                } => {
                    let (next, dirty, returned) = self.advance_ending(
                        &mut ending,
                        &mut state,
                        step,
                        target,
                        &EndingResources {
                            frames,
                            sound: resources,
                        },
                        events,
                    )?;
                    repaint |= dirty;
                    if let Some(choice) = returned {
                        self.session
                            .remember_ending(&ending.snapshot(), ending.special_snapshots());
                        self.intro_choice = choice;
                        self.start_intro(*state, frames, resources)?;
                        repaint = true;
                    } else {
                        self.phase = Phase::Ending {
                            ending,
                            state,
                            step: next,
                        };
                    }
                    break;
                }
            }
            if self.clock.tick() >= target {
                break;
            }
        }
        if before != std::mem::discriminant(&self.phase) {
            for scan in assigned.into_iter().rev() {
                if scan.0 > self.clock.tick() {
                    self.scans.push_front(scan);
                }
            }
        }
        self.sync_session();
        repaint |= display_before != (self.display_mode(), self.palette());
        Ok(repaint)
    }
    fn ending_poll(
        &mut self,
        state: &mut GameplayState,
        site: u16,
        events: &mut Vec<(u64, GameplayEvent)>,
    ) -> u8 {
        let poll = state.controls.poll(state.keyboard.held_keys(0));
        if poll.display_toggled {
            self.palette_family = 1;
        }
        events.push((
            self.clock.tick(),
            GameplayEvent::InputPoll {
                site,
                mask: poll.mask,
            },
        ));
        poll.mask
    }

    fn advance_ending(
        &mut self,
        ending: &mut Ending,
        state: &mut GameplayState,
        mut step: EndingStep,
        target: u64,
        resources: &EndingResources<'_>,
        events: &mut Vec<(u64, GameplayEvent)>,
    ) -> Result<(EndingStep, bool, Option<bool>), String> {
        use crate::opening::{EndingStage, EndingTitle};
        let mut dirty = false;
        loop {
            self.drain_background(state, &resources.sound.procedural)?;
            match step {
                EndingStep::Entry { initial, poll_at } => {
                    if self.clock.tick() < poll_at {
                        self.advance_background(
                            target.min(poll_at),
                            state,
                            &resources.sound.procedural,
                        )?;
                        if self.clock.tick() >= target {
                            break;
                        }
                        continue;
                    }
                    let site = if initial || ending.snapshot().title == EndingTitle::HighScore {
                        0x26f1
                    } else {
                        0x2864
                    };
                    if self.ending_poll(state, site, events) != 0 {
                        // Busy-loop instruction costs remain approximated, as
                        // in native pause. Device IRQs and received scans run.
                        step = EndingStep::Entry {
                            initial,
                            poll_at: self.clock.tick().saturating_add(64),
                        };
                    } else {
                        if initial {
                            ending.prepare_initial_title();
                        } else {
                            ending.prepare_following_title();
                        }
                        step = EndingStep::Poll;
                    }
                }
                EndingStep::Poll => {
                    if ending.can_return() {
                        let snapshot = ending.snapshot();
                        let site = match (snapshot.title, snapshot.stage) {
                            (EndingTitle::GameOver, EndingStage::Approach) => 0x2736,
                            (EndingTitle::GameOver, EndingStage::Hold) => 0x27b8,
                            (EndingTitle::GameOver, EndingStage::Departure) => 0x27ea,
                            (EndingTitle::HighScore, EndingStage::Approach) => 0x28fd,
                            (EndingTitle::HighScore, EndingStage::Hold) => 0x29fa,
                            (EndingTitle::HighScore, EndingStage::Departure) => 0x2a2c,
                            _ => unreachable!("only animation loops permit return"),
                        };
                        if self.ending_poll(state, site, events) != 0 {
                            return Ok((step, dirty, ending.returned_intro_choice()));
                        }
                    }
                    step = if ending.phase() == EndingStage::ScoreOverlay {
                        // The score overlay has no input poll or timer wait.
                        EndingStep::Finish
                    } else {
                        EndingStep::Wait(Wait::from_call(
                            ending.wait_units() as u16,
                            ending.wait_units() as u16,
                        ))
                    };
                }
                EndingStep::Wait(wait) => {
                    if wait.poll(&mut state.procedural.counters) {
                        step = EndingStep::Finish;
                    } else {
                        self.advance_background(target, state, &resources.sound.procedural)?;
                        if self.clock.tick() >= target {
                            break;
                        }
                        continue;
                    }
                }
                EndingStep::Finish => {
                    if ending.has_prepared_draw() {
                        ending
                            .finish_prepared_step(resources.frames.graphics())
                            .map_err(|e| e.to_string())?;
                        self.visible = ending.frame().clone();
                        dirty = true;
                    }
                    if ending.next_step_starts_title() {
                        ending.finish_mandatory_title();
                        step = EndingStep::Entry {
                            initial: false,
                            poll_at: self.clock.tick(),
                        };
                    } else {
                        ending.prepare_next_step().map_err(|e| e.to_string())?;
                        step = if ending.phase() == EndingStage::Pause {
                            // The caller executes two distinct waits, clearing
                            // excess software counter units after each.
                            EndingStep::Intertitle {
                                second: false,
                                wait: Wait::from_call(1, 1),
                            }
                        } else {
                            EndingStep::Poll
                        };
                    }
                }
                EndingStep::Intertitle { second, wait } => {
                    if wait.poll(&mut state.procedural.counters) {
                        step = if second {
                            EndingStep::Entry {
                                initial: false,
                                poll_at: self.clock.tick(),
                            }
                        } else {
                            EndingStep::Intertitle {
                                second: true,
                                wait: Wait::from_call(5, 5),
                            }
                        };
                    } else {
                        self.advance_background(target, state, &resources.sound.procedural)?;
                        if self.clock.tick() >= target {
                            break;
                        }
                        continue;
                    }
                }
            }
            if self.clock.tick() >= target {
                break;
            }
        }
        Ok((step, dirty, None))
    }

    fn drain_background(
        &mut self,
        state: &mut GameplayState,
        program: &ProceduralProgram,
    ) -> Result<(), String> {
        let mut scan_latched = false;
        loop {
            if !scan_latched
                && self
                    .scans
                    .front()
                    .is_some_and(|(tick, _)| *tick <= self.clock.tick())
            {
                self.clock
                    .keyboard_input(self.scans.pop_front().unwrap().1)
                    .map_err(|e| e.to_string())?;
                scan_latched = true;
            }
            match self
                .clock
                .advance_to(self.clock.tick())
                .map_err(|e| e.to_string())?
            {
                Progress::Reached => return Ok(()),
                Progress::Interrupt(irq) => {
                    if irq.irq == 1 {
                        scan_latched = false;
                    }
                    self.dispatch_background(irq, state, program)?;
                }
            }
        }
    }
    fn advance_background(
        &mut self,
        target: u64,
        state: &mut GameplayState,
        program: &ProceduralProgram,
    ) -> Result<(), String> {
        let next_scan = self
            .scans
            .front()
            .map_or(target, |(tick, _)| (*tick).min(target));
        match self
            .clock
            .advance_to(next_scan)
            .map_err(|e| e.to_string())?
        {
            Progress::Reached => {}
            Progress::Interrupt(irq) => self.dispatch_background(irq, state, program)?,
        }
        Ok(())
    }
    fn dispatch_background(
        &mut self,
        irq: crate::timer::Interrupt,
        state: &mut GameplayState,
        program: &ProceduralProgram,
    ) -> Result<(), String> {
        if irq.irq == 0 {
            state
                .procedural
                .dispatch(
                    irq,
                    program,
                    CS,
                    if state.controls.preferences.sound_disabled {
                        0xffff
                    } else {
                        0
                    },
                    &mut self.clock,
                )
                .map_err(|e| e.to_string())?;
        } else if irq.irq == 1
            && irq.handler
                == (FarPointer {
                    segment: CS,
                    offset: 0x744a,
                })
        {
            self.clock.emit(SoundAction::InterruptsEnabled(false));
            let raw = crate::sound::read(&mut self.clock, 0x60);
            let control = crate::sound::read(&mut self.clock, 0x61);
            crate::sound::write(&mut self.clock, 0x61, control | 128);
            crate::sound::write(&mut self.clock, 0x61, control);
            state.keyboard.apply_scan(raw);
            crate::timer::return_interrupt(&mut self.clock);
            self.clock.check().map_err(|e| e.to_string())?;
        } else {
            return Err("unexpected background interrupt".into());
        }
        Ok(())
    }
    fn display_mode(&self) -> DisplayMode {
        self.state().controls.preferences.display_mode
    }
    fn palette(&self) -> u8 {
        self.palette_family
    }
    fn caption(&self) -> &'static str {
        match &self.phase {
            Phase::Splash => "REforge — Mach 3 — Splash (Space to advance)",
            Phase::Intro(_) => "REforge — Mach 3 — Intro (Space to start)",
            Phase::Pregame { .. } => "REforge — Mach 3 — Get ready",
            Phase::Playing(runtime) if runtime.state().pause.state != PauseState::Running => {
                "REforge — Mach 3 — Paused (release keys, then F1 to end game or another key to resume)"
            }
            Phase::Playing(_) => "REforge — Mach 3 — Playing (F1 to pause, Esc to close)",
            Phase::Ending { ending, .. } if ending.can_return() => {
                "REforge — Mach 3 — Game Over / High Score (Space to return, Esc to close)"
            }
            Phase::Ending { .. } => "REforge — Mach 3 — Game Over / High Score",
        }
    }
}

pub(super) fn run(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
    resources: Resources,
) -> Result<(), String> {
    let signal = Rc::new(RefCell::new(Synthesis::new()?));
    let mut simulation = Simulation::new(frames, signal.clone())?;
    let mut window = Window::new(
        simulation.caption(),
        WIDTH as u32,
        DISPLAY_HEIGHT,
        options.scale,
    )?;
    let creator = window.texture_creator();
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut audio = PlaybackAudio::new(signal);
    let mut pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut history = InputHistory::default();
    let mut epoch = pause.now(Instant::now());
    let mut space_held = false;
    let mut navigation_space_release = false;
    #[cfg(feature = "dev-tools")]
    let mut trace = options
        .input_trace
        .as_ref()
        .map(|p| InputTrace::create(p))
        .transpose()?;
    #[cfg(feature = "dev-tools")]
    let mut timing = options
        .timing_trace
        .as_ref()
        .map(|p| TimingTrace::create(p, options.body_period))
        .transpose()?;
    let mut repaint = true;
    loop {
        for event in window.poll_events() {
            let received = Instant::now();
            let time =
                EventTime::sample(&event, reforge_sdl::event_clock_sample(), &pause, received);
            if host_shortcut(&event) == Some(HostShortcut::Quit)
                || matches!(
                    event,
                    Event::Quit { .. }
                        | Event::Window {
                            win_event: WindowEvent::CloseRequested,
                            ..
                        }
                        | Event::KeyDown {
                            keycode: Some(Keycode::Escape),
                            ..
                        }
                )
            {
                #[cfg(feature = "dev-tools")]
                {
                    if let Some(trace) = &mut trace {
                        trace.flush()?;
                    }
                    if let Some(timing) = &mut timing {
                        timing.flush()?;
                    }
                }
                return Ok(());
            }
            let mut disposition = "handled";
            if host_shortcut(&event) == Some(HostShortcut::TogglePause) {
                pause.toggle(received);
                input.fresh.clear();
                space_held = false;
                navigation_space_release = false;
                simulation.clear_input()?;
                history.reset(EventTime {
                    host: received,
                    timeline: pause.now(received),
                    now: pause.now(received),
                });
                audio.pause(pause.is_paused());
                repaint = true;
                disposition = "host_shortcut";
            } else if pause.is_paused() {
                disposition = "ignored_host_pause";
            } else if matches!(
                event,
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                }
            ) {
                input.fresh.clear();
                space_held = false;
                navigation_space_release = false;
                simulation.clear_input()?;
                history.reset(time);
                disposition = "focus_reset";
            } else if !history.accepts(time.host) {
                disposition = "ignored_before_reset";
            } else if !input.accept(&event) {
                disposition = "ignored_repeat";
            } else if matches!(simulation.phase, Phase::Splash) {
                if let Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                } = event
                {
                    if !space_held {
                        space_held = true;
                        epoch = pause.now(received);
                        navigation_space_release = true;
                        let state = simulation.initial.take().unwrap();
                        simulation.start_intro(state, frames, &resources)?;
                        repaint = true;
                    }
                }
            } else if navigation_space_release
                && matches!(
                    event,
                    Event::KeyDown {
                        keycode: Some(Keycode::Space),
                        ..
                    } | Event::KeyUp {
                        keycode: Some(Keycode::Space),
                        ..
                    }
                )
            {
                if matches!(event, Event::KeyUp { .. }) {
                    navigation_space_release = false;
                    space_held = false;
                }
                disposition = "navigation_space";
            } else {
                match &event {
                    Event::KeyDown {
                        keycode: Some(key), ..
                    } => {
                        if let Some(scan) = scan_for_key(*key) {
                            simulation.receive(ticks_since(epoch, time.timeline), scan);
                            disposition = "queued";
                        }
                    }
                    Event::KeyUp {
                        keycode: Some(key), ..
                    } => {
                        if let Some(scan) = scan_for_key(*key) {
                            simulation.receive(ticks_since(epoch, time.timeline), scan | 128);
                            disposition = "queued";
                        }
                        if *key == Keycode::Space {
                            space_held = false;
                        }
                    }
                    _ => {}
                }
            }
            repaint |= matches!(event, Event::Window { .. });
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut trace {
                trace.event_with_input_time(
                    &event,
                    disposition,
                    &pause,
                    &simulation.session,
                    Some(time.timeline),
                );
            }
            #[cfg(not(feature = "dev-tools"))]
            let _ = disposition;
        }
        if !pause.is_paused() && !matches!(simulation.phase, Phase::Splash) {
            let now = pause.now(Instant::now());
            let wanted = ticks_since(epoch, now).max(simulation.clock.tick());
            let budget = wanted.min(simulation.clock.tick().saturating_add(CHUNK));
            let mut events = Vec::new();
            let phase_before = std::mem::discriminant(&simulation.phase);
            #[cfg(feature = "dev-tools")]
            let start = timing.as_mut().map(TimingTrace::now);
            repaint |= simulation.advance(
                budget,
                frames,
                &rules,
                &resources,
                options.controls,
                &mut events,
            )?;
            audio.service(simulation.clock.tick(), wanted);
            #[cfg(feature = "dev-tools")]
            {
                if let Some(trace) = &mut trace {
                    for (tick, event) in events {
                        trace.boundary(
                            "pit_event",
                            instant_at(epoch, tick),
                            None,
                            &simulation.session,
                            &format!("tick={tick} event={event:?} state_at_budget=true"),
                        );
                    }
                    trace.flush()?;
                }
                if let Some(timing) = &mut timing {
                    let end = timing.now();
                    let state = match &simulation.phase {
                        Phase::Playing(runtime) => Some(runtime.game().snapshot()),
                        _ => None,
                    };
                    timing.span(
                        "simulation",
                        now,
                        Some(instant_at(epoch, budget)),
                        (start.unwrap(), end),
                        state.as_ref(),
                        &format!(
                            "committed_tick={} wanted_tick={wanted}",
                            simulation.clock.tick()
                        ),
                    );
                    timing.flush()?;
                }
            }
            // Repump SDL while recovering simulation debt; don't insert a vsync.
            if simulation.clock.tick() < wanted
                && phase_before == std::mem::discriminant(&simulation.phase)
            {
                continue;
            }
        }
        if repaint {
            texture.update(&display_rgb(
                &simulation.visible,
                simulation.display_mode(),
                simulation.palette(),
            ))?;
            window.set_title(simulation.caption())?;
            window.present(&texture)?;
            repaint = false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
