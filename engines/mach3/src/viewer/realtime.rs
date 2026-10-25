// Copyright (c) 2026 analogued. Project license has not been selected.
//! Live presentation of the reconstructed device/foreground timeline.

use super::input_history::InputEpoch;
use super::*;
#[cfg(test)]
mod tests;
use super::audio_output::{AudioOutput, Synthesis, resolve_renderer};
#[cfg(test)]
use super::audio_output::{RATE, samples_at};
use crate::{
    gameplay::execution::{GameplayEvent, GameplayProgress, GameplayResources, GameplayState},
    live::GameplayRuntime,
    sound::{
        DosSampleMemory, Effect, FarPointer, IntroProgram, IntroProgress, IntroTask,
        ProceduralProgram, Resource, SampleBus, SamplePlayer, SampleRequest, SampleSoundState,
        SoundAction, SoundAssets,
    },
    timer::{Clock, ClockEvent, Progress, SampleStart, SampleTask, Stamp, Wait},
};
use reforge_audio::{PIT_HZ, speaker::PcSpeaker};
use reforge_sdl::settings::SoundRenderer;
#[cfg(feature = "dev-tools")]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc, time::Instant};

const CHUNK: u64 = 11_931;
const CS: u16 = 0x1234;
// An original Fire alias not emitted by the host keyboard mapping. The sample
// intro observes IRQ1 scan bytes, not joystick bits. Keep this source separate
// from physical Space, including its release after the intro has returned.
const CONTROLLER_INTRO_SCAN: u8 = 0x77;
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
        game: Box<Gameplay>,
    },
    Playing(Box<GameplayRuntime>),
    Ending {
        ending: Box<Ending>,
        state: Box<GameplayState>,
        step: EndingStep,
    },
}

enum EndingStep {
    Entry {
        initial: bool,
        poll_at: u64,
    },
    Poll,
    Wait(Wait),
    Finish,
    Sample {
        task: SampleTask,
        state: Box<SampleSoundState>,
    },
    Intertitle {
        second: bool,
        wait: Wait,
    },
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
    joysticks: VecDeque<(u64, u8)>,
    controller_intro_held: bool,
    splash_wait: crate::startup::SplashWait,
}
impl Simulation {
    /// A host wake-up hint, never permission to execute ahead of host time.
    fn wake_tick(&self) -> Option<u64> {
        // Sample IRQs occur around 20 kHz and suspend ordinary foreground work.
        // Keep processing every IRQ in simulation, but batch their host servicing.
        let sample = matches!(self.clock.handlers()[0].offset, 0x57a4 | 0x57b8);
        let interrupt = (!sample)
            .then(|| {
                self.clock
                    .speaker()
                    .pit()
                    .next_rising_edge(0)
                    .expect("channel zero exists")
            })
            .flatten();
        let transition = match &self.phase {
            Phase::Playing(runtime) => runtime.next_transition_deadline(),
            _ => None,
        };
        interrupt
            .into_iter()
            .chain(transition)
            .chain(self.scans.front().map(|(tick, _)| *tick))
            .chain(self.joysticks.front().map(|(tick, _)| *tick))
            .min()
    }

    fn new(frames: &StartupFrames, signal: Rc<RefCell<Synthesis>>) -> Result<Self, String> {
        let observe: Box<dyn FnMut(Stamp, ClockEvent)> = Box::new(move |stamp, event| {
            signal.borrow_mut().observe(stamp, event);
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
        // 5df0 changes hardware timing before 74c0 installs the game handler.
        // Splash IRQ0 advances the BIOS clock, not game software counters.
        crate::sound::write(&mut clock, 0x43, 0x36);
        crate::sound::write(&mut clock, 0x40, crate::startup::SPLASH_DIVISOR as u8);
        crate::sound::write(
            &mut clock,
            0x40,
            (crate::startup::SPLASH_DIVISOR >> 8) as u8,
        );
        let state = GameplayState::default();
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
            joysticks: VecDeque::new(),
            controller_intro_held: false,
            splash_wait: crate::startup::SplashWait::default(),
        })
    }
    /// Both the original timer return and the modern Space skip enter here.
    fn finish_splash(
        &mut self,
        frames: &StartupFrames,
        resources: &Resources,
    ) -> Result<(), String> {
        let state = self.initial.take().ok_or("splash has already returned")?;
        self.start_intro(state, frames, resources)
    }
    fn start_intro(
        &mut self,
        mut state: GameplayState,
        frames: &StartupFrames,
        resources: &Resources,
    ) -> Result<(), String> {
        // 03c2 calls 74c0 on every intro entry, including post-game returns.
        // This reloads hardware even when the count is unchanged; software
        // counters, held input and procedural cursors remain intact.
        state
            .procedural
            .request(Effect::Reset, CS, 0, &mut self.clock);
        self.clock.check().map_err(|e| e.to_string())?;
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
        self.session.joystick = self.state().joystick;
    }
    fn clear_input(&mut self) -> Result<(), String> {
        self.scans.clear();
        self.joysticks.clear();
        self.controller_intro_held = false;
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
                self.initial.as_mut().unwrap().joystick = 0;
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
    fn receive_joystick(&mut self, tick: u64, mask: u8) {
        let tick = tick
            .max(self.clock.tick())
            .max(self.joysticks.back().map_or(0, |(at, _)| *at));
        self.joysticks.push_back((tick, mask & 31));
    }
    fn receive_gamepad(&mut self, tick: u64, pad: reforge_sdl::gamepad::PadEvent) {
        if self.controller_intro_held && !pad.actions.action {
            self.receive(tick, CONTROLLER_INTRO_SCAN | 128);
            self.controller_intro_held = false;
        }
        if matches!(self.phase, Phase::Intro(_)) && pad.pressed.action {
            self.receive(tick, CONTROLLER_INTRO_SCAN);
            self.controller_intro_held = true;
        }
        self.receive_joystick(tick, pad.actions.bits());
    }
    fn deliver_joysticks(&mut self, mask: &mut u8) {
        while self
            .joysticks
            .front()
            .is_some_and(|(at, _)| *at <= self.clock.tick())
        {
            *mask = self.joysticks.pop_front().unwrap().1;
        }
    }
    fn advance(
        &mut self,
        target: u64,
        frames: &StartupFrames,
        rules: &ScriptRules,
        resources: &Resources,
        controls: bool,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<bool, String> {
        let memory = DosSampleMemory::new(&resources.samples, *b"MACH3\0\0\0");
        let mut repaint = false;
        let display_before = (self.display_mode(), self.palette());
        let before = std::mem::discriminant(&self.phase);
        let mut assigned = Vec::new();
        let target = self.scans.front().map_or(target, |(at, _)| target.min(*at));
        let target = self
            .joysticks
            .front()
            .map_or(target, |(at, _)| target.min(*at));
        while self
            .joysticks
            .front()
            .is_some_and(|(tick, _)| *tick <= target)
        {
            let (tick, mask) = *self.joysticks.front().unwrap();
            match &mut self.phase {
                Phase::Intro(task) | Phase::Pregame { task, .. } => {
                    task.schedule_joystick(tick, mask, &self.clock)
                }
                Phase::Playing(runtime) => runtime.schedule_joystick(tick, mask, &self.clock),
                Phase::Splash => break,
                Phase::Ending { .. } => break,
            }
            .map_err(|e| e.to_string())?;
            self.joysticks.pop_front();
        }
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
                    loop {
                        match self.clock.advance_to(target).map_err(|e| e.to_string())? {
                            Progress::Reached => {
                                let mut mask = self.initial.as_ref().unwrap().joystick;
                                self.deliver_joysticks(&mut mask);
                                self.initial.as_mut().unwrap().joystick = mask;
                                break;
                            }
                            Progress::Interrupt(irq) if irq.irq == 0 => {
                                let complete = self.splash_wait.interrupt();
                                crate::timer::return_interrupt(&mut self.clock);
                                self.clock.check().map_err(|e| e.to_string())?;
                                if complete {
                                    let mut mask = self.initial.as_ref().unwrap().joystick;
                                    self.deliver_joysticks(&mut mask);
                                    self.initial.as_mut().unwrap().joystick = mask;
                                    self.finish_splash(frames, resources)?;
                                    repaint = true;
                                    break;
                                }
                            }
                            Progress::Interrupt(_) => {
                                return Err("unexpected splash interrupt".into());
                            }
                        }
                    }
                    break;
                }
                Phase::Intro(mut task) => {
                    let progress = task
                        .advance_until(target, &memory, &resources.procedural, &mut self.clock)
                        .map_err(|e| e.to_string())?;
                    if progress == IntroProgress::Returned {
                        for change in task.take_scheduled_joysticks().into_iter().rev() {
                            self.joysticks.push_front(change);
                        }
                        let mut state = task.into_state().map_err(|e| e.to_string())?;
                        let start = self.session.begin_game();
                        let mut game = Gameplay::for_play(frames.graphics(), rules.clone(), start)
                            .map_err(|e| e.to_string())?;
                        let poll = state
                            .keyboard
                            .poll_controls(&mut state.controls, state.joystick);
                        observe(
                            self.clock.tick(),
                            GameplayEvent::InputPoll {
                                site: 0x2289,
                                mask: poll.mask,
                            },
                        );
                        if poll.display_toggled {
                            game.reset_video_palette();
                        }
                        game.initialize_input(
                            frames.graphics(),
                            if controls { poll.mask } else { 0 },
                        )
                        .map_err(|e| e.to_string())?;
                        self.session.controls = state.controls.clone();
                        self.session.keyboard = state.keyboard.clone();
                        self.palette_family = game.palette_family();
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
                        for change in task.take_scheduled_joysticks().into_iter().rev() {
                            self.joysticks.push_front(change);
                        }
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
                            observe,
                        )
                        .map_err(|e| e.to_string())?;
                    runtime.synchronize_session(&mut self.session);
                    self.palette_family = runtime.palette_family();
                    match progress {
                        GameplayProgress::Pending => {
                            self.phase = Phase::Playing(runtime);
                            break;
                        }
                        GameplayProgress::Published { .. }
                        | GameplayProgress::TransitionPublished { .. } => {
                            self.visible = runtime.visible_frame().clone();
                            repaint = true;
                            self.phase = Phase::Playing(runtime);
                        }
                        GameplayProgress::Returned => {
                            for change in runtime.take_scheduled_joysticks().into_iter().rev() {
                                self.joysticks.push_front(change);
                            }
                            let (game, state) = runtime.into_parts().map_err(|e| e.to_string())?;
                            // GameplayTask already executes the caller's 0452
                            // reset through Gameplay::return_sound exactly once.
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
                        observe,
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
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> u8 {
        self.deliver_joysticks(&mut state.joystick);
        let poll = state
            .keyboard
            .poll_controls(&mut state.controls, state.joystick);
        if poll.display_toggled {
            self.palette_family = 1;
        }
        observe(
            self.clock.tick(),
            GameplayEvent::InputPoll {
                site,
                mask: poll.mask,
            },
        );
        poll.mask
    }

    fn advance_ending(
        &mut self,
        ending: &mut Ending,
        state: &mut GameplayState,
        mut step: EndingStep,
        target: u64,
        resources: &EndingResources<'_>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<(EndingStep, bool, Option<bool>), String> {
        use crate::gameplay::{EndingStage, EndingTitle};
        let mut dirty = false;
        loop {
            // A sample owns the temporary IRQ vectors and sole source cursor.
            // Drain its interrupts before the ordinary ending scheduler resumes.
            if let EndingStep::Sample {
                mut task,
                state: mut sample_state,
            } = step
            {
                let memory = DosSampleMemory::new(&resources.sound.samples, *b"MACH3\0\0\0");
                if self.advance_ending_sample(
                    target,
                    &mut task,
                    &mut sample_state,
                    &mut state.joystick,
                    &memory,
                )? {
                    state.keyboard.clear_table();
                    state.controls.sound_poll_counter &= 0xff00;
                    state.controls.preferences.sound_disabled = sample_state.disabled;
                    state.di = SamplePlayer::return_di();
                    step = EndingStep::Poll;
                } else {
                    step = EndingStep::Sample {
                        task,
                        state: sample_state,
                    };
                    break;
                }
            }
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
                    if self.ending_poll(state, site, observe) != 0 {
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
                        if self.ending_poll(state, site, observe) != 0 {
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
                        let before = ending.phase();
                        ending.prepare_next_step().map_err(|e| e.to_string())?;
                        let snapshot = ending.snapshot();
                        let sample = snapshot.cycle == 0
                            && ending.phase() == EndingStage::Hold
                            && matches!(before, EndingStage::Approach | EndingStage::ScoreOverlay)
                            && (snapshot.title == EndingTitle::GameOver || ending.new_best());
                        step = if sample && state.controls.preferences.digitized_samples_enabled {
                            let (site, resource, count) = match snapshot.title {
                                EndingTitle::GameOver => (0x27a4, Resource::Data9, 10_874),
                                EndingTitle::HighScore => (0x29e6, Resource::Data7, 11_872),
                            };
                            let request = SampleRequest {
                                resource,
                                offset: 0,
                                count,
                                reverse: false,
                            };
                            observe(
                                self.clock.tick(),
                                GameplayEvent::Action(crate::sound::GameplayAction::PlaySample {
                                    site,
                                    request,
                                }),
                            );
                            let mut sample_state = Box::new(SampleSoundState {
                                disabled: state.controls.preferences.sound_disabled,
                                ..SampleSoundState::default()
                            });
                            match SampleTask::begin(
                                state.next_playback,
                                request,
                                &mut sample_state,
                                CS,
                                state.di,
                                &mut self.clock,
                            )
                            .map_err(|e| e.to_string())?
                            {
                                SampleStart::Playing(task) => {
                                    state.next_playback = state
                                        .next_playback
                                        .checked_add(1)
                                        .ok_or("playback identity overflow")?;
                                    EndingStep::Sample {
                                        task,
                                        state: sample_state,
                                    }
                                }
                                SampleStart::RegularKeyboardPoll => {
                                    // The wrapper ignores the return mask, including F1/Fire.
                                    self.ending_poll(state, 0x73b2, observe);
                                    EndingStep::Poll
                                }
                            }
                        } else if ending.phase() == EndingStage::Pause {
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
                EndingStep::Sample { .. } => {
                    unreachable!("sample is serviced before ordinary IRQs")
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

    /// Stop at the scan deadline before allowing the sample to finish. A byte
    /// received with IRQ0 is latched before its ISR; IRQ0 still precedes IRQ1.
    fn advance_ending_sample(
        &mut self,
        target: u64,
        task: &mut SampleTask,
        state: &mut SampleSoundState,
        joystick: &mut u8,
        memory: &DosSampleMemory<'_>,
    ) -> Result<bool, String> {
        let mut scan_latched = false;
        loop {
            self.deliver_joysticks(joystick);
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
                Progress::Interrupt(irq) => {
                    if irq.irq == 1 {
                        scan_latched = false;
                    }
                    task.dispatch(irq, memory, state, &mut self.clock)
                        .map_err(|e| e.to_string())?;
                    continue;
                }
                Progress::Reached => {}
            }
            if task
                .poll_return(state, &mut self.clock)
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Ok(true);
            }
            if self.clock.tick() >= target {
                return Ok(false);
            }
            let deadline = self
                .scans
                .front()
                .map_or(target, |(tick, _)| target.min(*tick));
            let deadline = self
                .joysticks
                .front()
                .map_or(deadline, |(tick, _)| deadline.min(*tick));
            let progress = self.clock.advance_to(deadline).map_err(|e| e.to_string())?;
            // advance_to may already enter IRQ0 at the scan timestamp.
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
            if let Progress::Interrupt(irq) = progress {
                if irq.irq == 1 {
                    scan_latched = false;
                }
                task.dispatch(irq, memory, state, &mut self.clock)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    fn drain_background(
        &mut self,
        state: &mut GameplayState,
        program: &ProceduralProgram,
    ) -> Result<(), String> {
        let mut scan_latched = false;
        loop {
            self.deliver_joysticks(&mut state.joystick);
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
        let next_scan = self
            .joysticks
            .front()
            .map_or(next_scan, |(tick, _)| next_scan.min(*tick));
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
    fn status(&self) -> &'static str {
        match &self.phase {
            Phase::Splash => "Splash (Space to skip)",
            Phase::Intro(_) => "Intro (Space to start)",
            Phase::Pregame { .. } => "Get ready",
            Phase::Playing(runtime) if runtime.state().pause.state != PauseState::Running => {
                "Paused (release keys, then F1 to end game or another key to resume)"
            }
            Phase::Playing(_) => "Playing (F1 to pause, Esc to close)",
            Phase::Ending { ending, .. } if ending.can_return() => {
                "Game Over / High Score (Space to return, Esc to close)"
            }
            Phase::Ending { .. } => "Game Over / High Score",
        }
    }
}

pub(super) fn run(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
    resources: Resources,
) -> Result<(), String> {
    let signal = Rc::new(RefCell::new(Synthesis::with_mode(options.audio_renderer)?));
    let mut simulation = Simulation::new(frames, signal.clone())?;
    simulation
        .initial
        .as_mut()
        .unwrap()
        .keyboard
        .set_f3_mode(options.f3_mode);
    let mut status = simulation.status();
    let mut window = Window::new_with_geometry(
        &game_title(GAME_NAME, Some(status)),
        display_geometry(simulation.display_mode()),
        options.scale,
        options.presentation,
    )?;
    let creator = window.texture_creator();
    let mut gamepads = reforge_sdl::gamepad::Gamepads::new(window.settings().stick_dead_zone);
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
        .with_audio_renderers(&[SoundRenderer::Speaker, SoundRenderer::EnhancedPcm])
        .with_audio_categories(&reforge_audio::category::SoundCategory::ALL)
        .with_mach3_options(options.digitized_samples);
    let samples = menu.mach3_startup_settings().digitized_samples;
    simulation
        .initial
        .as_mut()
        .unwrap()
        .controls
        .preferences
        .digitized_samples_enabled = samples;
    simulation
        .session
        .controls
        .preferences
        .digitized_samples_enabled = samples;
    let mut menu_pause = MenuPause::default();
    let host = creator.audio_playback();
    signal.borrow_mut().set_mode(
        resolve_renderer(host.settings().renderer, options.audio_renderer),
        simulation.clock.tick(),
    );
    signal
        .borrow_mut()
        .renderer
        .set_volumes(host.settings().categories)
        .map_err(|e| e.to_string())?;
    let mut audio = AudioOutput::new(signal, host);
    let mut pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut history = InputEpoch::default();
    let epoch = pause.now(Instant::now());
    let mut space_held = false;
    let mut navigation_space_release = false;
    #[cfg(feature = "dev-tools")]
    let mut trace = options
        .input_trace
        .as_ref()
        .map(|p| InputTrace::create(p))
        .transpose()?;
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = &mut trace {
        trace.boundary(
            "startup",
            Instant::now(),
            None,
            &simulation.session,
            &format!(
                "f3_mode={} controls={}",
                options.f3_mode.as_str(),
                options.controls
            ),
        );
    }
    #[cfg(feature = "dev-tools")]
    let mut timing = options
        .timing_trace
        .as_ref()
        .map(|p| TimingTrace::create(p, crate::gameplay::NOMINAL_PREVIEW_BODY_PERIOD))
        .transpose()?;
    let mut repaint = true;
    loop {
        let mut catch_up = false;
        gamepads.set_dead_zone(window.settings().stick_dead_zone);
        for event in window.poll_events() {
            let pad = gamepads.handle(&event);
            let received = Instant::now();
            let time =
                EventTime::sample(&event, reforge_sdl::event_clock_sample(), &pause, received);
            let pad_consumed = pad.is_some_and(|pad| menu.handle_gamepad(pad));
            let menu_consumed = !host_quit(&event) && (menu.handle_event(&event) || pad_consumed);
            if host_quit(&event)
                || (!menu_consumed
                    && matches!(
                        event,
                        Event::KeyDown {
                            keycode: Some(Keycode::Escape),
                            ..
                        }
                    ))
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
            if menu_pause.update(menu.is_open(), &mut pause, received) {
                gamepads.reset();
                input.fresh.clear();
                space_held = false;
                navigation_space_release = false;
                simulation.clear_input()?;
                history.reset(received);
                audio.pause(pause.is_paused());
                repaint = true;
            }
            let mut disposition = "handled";
            if menu_consumed {
                disposition = "settings_menu";
            } else if host_shortcut(&event) == Some(HostShortcut::TogglePause)
                || pad.is_some_and(|pad| pad.pause)
            {
                pause.toggle(received);
                gamepads.reset();
                input.fresh.clear();
                space_held = false;
                navigation_space_release = false;
                simulation.clear_input()?;
                history.reset(received);
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
                gamepads.reset();
                input.fresh.clear();
                space_held = false;
                navigation_space_release = false;
                simulation.clear_input()?;
                history.reset(time.host);
                disposition = "focus_reset";
            } else if !history.accepts(time.host) {
                disposition = "ignored_before_reset";
            } else if let Some(pad) = pad {
                if matches!(simulation.phase, Phase::Splash) && pad.pressed.action {
                    simulation.finish_splash(frames, &resources)?;
                    gamepads.reset();
                    repaint = true;
                } else {
                    simulation.receive_gamepad(ticks_since(epoch, time.timeline), pad);
                    disposition = "queued_gamepad";
                }
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
                        navigation_space_release = true;
                        simulation.finish_splash(frames, &resources)?;
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
        prepare_settings(&mut menu, &mut window);
        if audio.sync_settings(simulation.clock.tick(), options.audio_renderer) {
            audio.pause(pause.is_paused());
        }
        let received = Instant::now();
        if menu_pause.update(menu.is_open(), &mut pause, received) {
            gamepads.reset();
            input.fresh.clear();
            space_held = false;
            navigation_space_release = false;
            simulation.clear_input()?;
            history.reset(received);
            audio.pause(pause.is_paused());
            repaint = true;
        }
        repaint |= menu.is_open();
        if !pause.is_paused() {
            let now = pause.now(Instant::now());
            let wanted = ticks_since(epoch, now).max(simulation.clock.tick());
            let budget = wanted.min(simulation.clock.tick().saturating_add(CHUNK));
            #[cfg(feature = "dev-tools")]
            let mut events = trace.as_ref().map(|_| Vec::new());
            let mut observe = |_tick, _event| {
                #[cfg(feature = "dev-tools")]
                if let Some(events) = &mut events {
                    events.push((_tick, _event));
                }
            };
            let phase_before = std::mem::discriminant(&simulation.phase);
            #[cfg(feature = "dev-tools")]
            let start = timing.as_mut().map(TimingTrace::now);
            repaint |= simulation.advance(
                budget,
                frames,
                &rules,
                &resources,
                options.controls,
                &mut observe,
            )?;
            audio.service(simulation.clock.tick(), wanted);
            #[cfg(feature = "dev-tools")]
            {
                if let Some(trace) = &mut trace {
                    for (tick, event) in events.into_iter().flatten() {
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
            catch_up = simulation.clock.tick() < wanted
                && phase_before == std::mem::discriminant(&simulation.phase);
        }
        // Native pause can change the caption without publishing a new frame.
        // Update it even during catch-up, independently of texture presentation.
        let next_status = if menu.is_open() {
            "Settings (F10 or Escape to close, Ctrl+Q to quit)"
        } else {
            simulation.status()
        };
        if next_status != status {
            status = next_status;
            window.set_title(&game_title(GAME_NAME, Some(status)))?;
        }
        if catch_up {
            continue;
        }
        if repaint {
            window.set_display_geometry(display_geometry(simulation.display_mode()))?;
            texture.update(&display_rgb(
                &simulation.visible,
                simulation.display_mode(),
                simulation.palette(),
            ))?;
            window.present_with(&texture, |canvas| menu.paint(canvas))?;
            repaint = false;
        }
        let next_tick = if pause.is_paused() {
            None
        } else {
            simulation.wake_tick()
        };
        let delay = super::wakeup::delay(epoch, pause.now(Instant::now()), next_tick);
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}
