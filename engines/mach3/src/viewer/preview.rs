// Copyright (c) 2026 analogued. Project license has not been selected.
//! Silent nominal scheduling for step, static and custom-period previews.

use super::*;

#[allow(clippy::too_many_arguments)]
fn advance_game_pass_observed(
    gameplay: &mut Gameplay,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
    #[cfg(feature = "dev-tools")] trace: &mut Option<InputTrace>,
    #[cfg(feature = "dev-tools")] timing: &mut Option<TimingTrace>,
    #[cfg(feature = "dev-tools")] poll_time: (std::time::Instant, Option<std::time::Instant>),
) -> Result<bool, String> {
    let result = session
        .keyboard
        .poll_controls(&mut session.controls, session.joystick);
    if result.display_toggled {
        gameplay.reset_video_palette();
    }
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = trace {
        let state = gameplay.snapshot();
        trace.poll(
            PollSite {
                label: "steering",
                timeline: poll_time.0,
                deadline: poll_time.1,
                body: state.body,
                pass: state.render_pass,
                controls,
            },
            session,
            result,
        );
    }
    #[cfg(feature = "dev-tools")]
    let host_start = timing.as_mut().map(TimingTrace::now);
    gameplay
        .advance_subframe(assets, steering_from_mask(result.mask, controls))
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = timing {
        let end = trace.now();
        trace.span(
            "pass",
            poll_time.0,
            poll_time.1,
            (host_start.unwrap(), end),
            Some(&gameplay.snapshot()),
            if gameplay.pass_one_wait_required() {
                "extra_wait=true"
            } else {
                "extra_wait=false"
            },
        );
    }
    // Pause ran after the original exit gate. Enter the committed body before
    // setting the flag so complete() permits its remaining two render passes.
    if *quit_after_body_entry {
        gameplay.request_game_over();
        *quit_after_body_entry = false;
    }
    Ok(result.display_toggled)
}

#[cfg(test)]
fn advance_game_pass(
    gameplay: &mut Gameplay,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
) -> Result<bool, String> {
    advance_game_pass_observed(
        gameplay,
        assets,
        session,
        controls,
        quit_after_body_entry,
        #[cfg(feature = "dev-tools")]
        &mut None,
        #[cfg(feature = "dev-tools")]
        &mut None,
        #[cfg(feature = "dev-tools")]
        (std::time::Instant::now(), None),
    )
}

fn pause_waiting(state: PauseState) -> bool {
    matches!(
        state,
        PauseState::WaitBeforePress(_) | PauseState::WaitBeforeDecision(_)
    )
}

#[derive(Default)]
struct TransitionPlayback {
    frames: VecDeque<TransitionFrame>,
    deadline: Option<std::time::Instant>,
}

impl TransitionPlayback {
    fn append(
        &mut self,
        frames: Vec<TransitionFrame>,
        now: std::time::Instant,
        unit: std::time::Duration,
    ) {
        self.frames.extend(frames);
        if self.deadline.is_none() {
            self.deadline = self
                .frames
                .front()
                .map(|step| now + unit * u32::from(step.wait_count));
        }
    }

    fn advance(&mut self, now: std::time::Instant, unit: std::time::Duration) -> bool {
        if !self.deadline.is_some_and(|deadline| now >= deadline) {
            return false;
        }
        self.frames.pop_front();
        self.deadline = self
            .frames
            .front()
            .map(|step| now + unit * u32::from(step.wait_count));
        true
    }

    fn active(&self) -> bool {
        !self.frames.is_empty()
    }
}

/// Mach 3's scheduling state is independent of how often SDL presents a frame.
struct GamePlayback {
    session: SessionState,
    history: InputHistory,
    gameplay: Option<Gameplay>,
    ending: Option<Ending>,
    next_frame: Option<std::time::Instant>,
    // A composed pass is not necessarily visible until its separate CGA-copy
    // deadline. Keep publication independent from input/state advancement.
    visible_frame: Option<Frame>,
    pending_publication: bool,
    pause_tick: Option<std::time::Instant>,
    transitions: TransitionPlayback,
    body_polled: bool,
    quit_after_body_entry: bool,
    pending_step: bool,
    color_family: u8,
    #[cfg(feature = "dev-tools")]
    trace: Option<InputTrace>,
    #[cfg(feature = "dev-tools")]
    timing: Option<TimingTrace>,
}

impl Default for GamePlayback {
    fn default() -> Self {
        Self {
            session: SessionState::default(),
            history: InputHistory::default(),
            gameplay: None,
            ending: None,
            next_frame: None,
            visible_frame: None,
            pending_publication: false,
            pause_tick: None,
            transitions: TransitionPlayback::default(),
            body_polled: false,
            quit_after_body_entry: false,
            pending_step: false,
            color_family: 1,
            #[cfg(feature = "dev-tools")]
            trace: None,
            #[cfg(feature = "dev-tools")]
            timing: None,
        }
    }
}

impl GamePlayback {
    fn begin_game(
        &mut self,
        assets: &GraphicsAssets,
        rules: Option<&ScriptRules>,
        options: &Options,
        now: std::time::Instant,
    ) -> Result<(), String> {
        #[cfg(feature = "dev-tools")]
        let timing_start = self.timing.as_mut().map(TimingTrace::now);
        // Fold intro/ending input before initialization. Keep the engine's
        // keyboard table and duplicate byte; do not restore a host snapshot.
        self.history.apply_until(&mut self.session, now);
        let start = self.session.begin_game();
        self.gameplay = Some(
            if let Some(rules) = rules {
                Gameplay::for_play(assets, rules.clone(), start)
            } else if options.controls {
                Gameplay::with_steering(assets)
            } else {
                Gameplay::new(assets)
            }
            .map_err(|error| error.to_string())?,
        );
        self.visible_frame = Some(self.gameplay.as_ref().unwrap().frame().clone());
        self.pending_publication = false;
        self.color_family = self.gameplay.as_ref().unwrap().palette_family();
        self.ending = None;
        self.transitions = TransitionPlayback::default();
        self.body_polled = false;
        self.quit_after_body_entry = false;
        self.pending_step = false;
        self.next_frame = Some(now + options.body_period / 4);
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.timing {
            let end = trace.now();
            trace.begin_game();
            trace.span(
                "game_start",
                now,
                self.next_frame,
                (timing_start.unwrap(), end),
                Some(&self.gameplay.as_ref().unwrap().snapshot()),
                &format!(
                    "controls={} playback={:?}",
                    options.controls, options.playback
                ),
            );
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.trace {
            trace.boundary("game_started", now, self.next_frame, &self.session, "");
        }
        Ok(())
    }

    fn toggle_host_pause(
        &mut self,
        pause: &mut HostPause,
        input: &mut ControlInput,
        received: std::time::Instant,
    ) {
        toggle_host_pause(pause, &mut self.session, input, received);
        self.reset_host_input(input, pause, received);
    }

    fn reset_host_input(
        &mut self,
        input: &mut ControlInput,
        pause: &HostPause,
        received: std::time::Instant,
    ) {
        input.clear(&mut self.session);
        let now = pause.now(received);
        self.history.reset(EventTime {
            host: received,
            timeline: now,
            now,
        });
        self.pending_step = false;
    }

    fn ordinary_due(&self, options: &Options, now: std::time::Instant) -> bool {
        options.playback == Playback::Animated
            && self.ending.is_none()
            && !self.transitions.active()
            && self.session.pause.state == PauseState::Running
            && self
                .gameplay
                .as_ref()
                .is_some_and(|gameplay| !gameplay.complete())
            && self.next_frame.is_some_and(|deadline| now >= deadline)
    }

    fn advance(
        &mut self,
        assets: &GraphicsAssets,
        options: &Options,
        now: std::time::Instant,
    ) -> Result<bool, String> {
        let mut repaint = false;
        let unit = options.body_period / 4;
        // These intervals have deliberate waits, not ordinary deadline debt.
        // Fold their scan history without manufacturing additional guest polls.
        if options.playback != Playback::Animated
            || self.transitions.active()
            || self.ending.is_some()
            || self.session.pause.state != PauseState::Running
            || self.gameplay.as_ref().is_none_or(Gameplay::complete)
        {
            self.history.apply_until(&mut self.session, now);
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut self.trace {
            trace.boundary(
                "batch",
                now,
                self.next_frame,
                &self.session,
                if self.transitions.active() {
                    "transition=true"
                } else {
                    "transition=false"
                },
            );
        }
        let transition_was_active = self.transitions.active();
        #[cfg(feature = "dev-tools")]
        let previous_transition_deadline = self.transitions.deadline;
        #[cfg(feature = "dev-tools")]
        let transition_start = self
            .transitions
            .deadline
            .is_some_and(|deadline| now >= deadline)
            .then(|| self.timing.as_mut().map(TimingTrace::now))
            .flatten();
        let transition_frame = self
            .transitions
            .frames
            .front()
            .map(|step| step.frame.clone());
        if self.transitions.advance(now, unit) {
            // Retain the last displayed transition after its queue is drained;
            // the prepared ordinary framebuffer need not have been published.
            self.visible_frame = transition_frame;
            repaint = true;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.timing {
                let end = trace.now();
                trace.span(
                    "transition_wait",
                    now,
                    previous_transition_deadline,
                    (transition_start.unwrap(), end),
                    self.gameplay.as_ref().map(Gameplay::snapshot).as_ref(),
                    &format!(
                        "remaining={} next_wait_count={}",
                        self.transitions.frames.len(),
                        self.transitions
                            .frames
                            .front()
                            .map_or(0, |frame| frame.wait_count)
                    ),
                );
            }
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.trace {
                trace.rebase(
                    "transition_frame_wait",
                    now,
                    previous_transition_deadline,
                    self.transitions.deadline,
                    &self.session,
                );
            }
            if !self.transitions.active() {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    trace.rebase(
                        "transition_finished",
                        now,
                        self.next_frame,
                        Some(now + unit),
                        &self.session,
                    );
                }
                self.next_frame = Some(now + unit);
            }
        }
        // Each intentional transition retains its own displayed wait. Do not
        // spend ordinary-frame catch-up debt inside that blocking sequence.
        if transition_was_active {
            return Ok(repaint);
        }
        if self.session.pause.state != PauseState::Running {
            if pause_waiting(self.session.pause.state) {
                if self.pause_tick.is_some_and(|deadline| now >= deadline) {
                    self.session.pause.tick();
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        trace.rebase(
                            "native_pause_tick",
                            now,
                            self.pause_tick,
                            Some(now + unit),
                            &self.session,
                        );
                    }
                    self.pause_tick = Some(now + unit);
                }
            } else {
                let result = self
                    .session
                    .keyboard
                    .poll_controls(&mut self.session.controls, self.session.joystick);
                if result.display_toggled {
                    if let Some(gameplay) = &mut self.gameplay {
                        gameplay.reset_video_palette();
                    }
                    // Native pause may remain active without an ordinary pass.
                    self.color_family = 1;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    let state = self.gameplay.as_ref().map(Gameplay::snapshot);
                    trace.poll(
                        PollSite {
                            label: "native_pause",
                            timeline: now,
                            deadline: None,
                            body: state.as_ref().map_or(0, |state| state.body),
                            pass: state.as_ref().map_or(0, |state| state.render_pass),
                            controls: options.controls,
                        },
                        &self.session,
                        result,
                    );
                }
                repaint |= result.display_toggled;
                match self.session.pause.poll(result.mask) {
                    PauseEvent::EndGame => self.quit_after_body_entry = true,
                    PauseEvent::Resumed => {
                        #[cfg(feature = "dev-tools")]
                        if let Some(trace) = &mut self.trace {
                            trace.rebase(
                                "native_pause_resumed",
                                now,
                                self.next_frame,
                                Some(now),
                                &self.session,
                            );
                        }
                        self.next_frame = Some(now);
                    }
                    _ => {}
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.trace {
                    trace.boundary(
                        "native_pause_phase",
                        now,
                        self.next_frame,
                        &self.session,
                        &format!("end_game_committed={}", self.quit_after_body_entry),
                    );
                }
                if pause_waiting(self.session.pause.state) {
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        trace.rebase(
                            "native_pause_wait",
                            now,
                            self.pause_tick,
                            Some(now + unit),
                            &self.session,
                        );
                    }
                    self.pause_tick = Some(now + unit);
                }
            }
        }
        // Catch up preparation/publication events before one presentation.
        // Bound each batch for responsiveness; retain remaining deadline debt.
        for _ in 0..64 {
            if self.transitions.active() || self.session.pause.state != PauseState::Running {
                break;
            }
            let timed = options.playback == Playback::Animated
                && self.next_frame.is_some_and(|deadline| now >= deadline);
            if !self.pending_step && !timed {
                break;
            }
            let Some(gameplay) = self.gameplay.as_mut() else {
                break;
            };
            let poll_at = self.next_frame.filter(|_| timed).unwrap_or(now);
            if self.pending_publication {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                self.visible_frame = Some(gameplay.pass_frame().clone());
                self.pending_publication = false;
                repaint = true;
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    trace.span(
                        "pass_publish",
                        now,
                        Some(poll_at),
                        (host_start.unwrap(), end),
                        gameplay.pass_snapshot(),
                        "delayed=true",
                    );
                }
                self.next_frame = Some(poll_at + unit);
                continue;
            }
            self.history.apply_until(&mut self.session, poll_at);
            if let Some(ending) = self.ending.as_mut() {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                #[cfg(feature = "dev-tools")]
                let previous_wait_units = ending.wait_units();
                ending.advance(assets).map_err(|error| error.to_string())?;
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    let state = ending.snapshot();
                    trace.span(
                        "ending_step",
                        now,
                        self.next_frame.filter(|_| timed),
                        (host_start.unwrap(), end),
                        Some(&gameplay.snapshot()),
                        &format!(
                            "title={:?} stage={:?} draw={} hold={} wait_units={} previous_wait_units={}",
                            state.title,
                            state.stage,
                            state.draw_index,
                            state.hold_tick,
                            ending.wait_units(), previous_wait_units
                        ),
                    );
                }
                self.pending_step = false;
                repaint = true;
            } else if gameplay.complete() && gameplay.snapshot().player.game_over {
                #[cfg(feature = "dev-tools")]
                let host_start = self.timing.as_mut().map(TimingTrace::now);
                self.ending = Some(
                    Ending::new_with_best(
                        assets,
                        &gameplay.snapshot(),
                        gameplay.frame(),
                        self.session.best_score,
                    )
                    .map_err(|error| error.to_string())?,
                );
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut self.timing {
                    let end = trace.now();
                    trace.span(
                        "ending_start",
                        now,
                        self.next_frame.filter(|_| timed),
                        (host_start.unwrap(), end),
                        Some(&gameplay.snapshot()),
                        "wait_units=2",
                    );
                }
                self.pending_step = false;
                repaint = true;
            } else if !gameplay.complete() {
                if gameplay.at_body_boundary() && !self.body_polled {
                    let result = self
                        .session
                        .keyboard
                        .poll_controls(&mut self.session.controls, self.session.joystick);
                    if result.display_toggled {
                        gameplay.reset_video_palette();
                    }
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut self.trace {
                        let state = gameplay.snapshot();
                        trace.poll(
                            PollSite {
                                label: "body_entry",
                                timeline: now,
                                deadline: self
                                    .next_frame
                                    .filter(|_| options.playback == Playback::Animated),
                                body: state.body,
                                pass: state.render_pass,
                                controls: options.controls,
                            },
                            &self.session,
                            result,
                        );
                    }
                    repaint |= result.display_toggled;
                    self.body_polled = true;
                    self.session.pause.poll(result.mask);
                    #[cfg(feature = "dev-tools")]
                    if self.session.pause.state != PauseState::Running {
                        if let Some(trace) = &mut self.trace {
                            trace.boundary(
                                "native_pause_phase",
                                now,
                                self.next_frame,
                                &self.session,
                                &format!("end_game_committed={}", self.quit_after_body_entry),
                            );
                        }
                    }
                }
                if self.session.pause.state == PauseState::Running {
                    let passes = if options.playback == Playback::Step {
                        3 - if gameplay.at_body_boundary() {
                            0
                        } else {
                            gameplay.snapshot().render_pass
                        }
                    } else {
                        1
                    };
                    for _ in 0..passes {
                        repaint |= advance_game_pass_observed(
                            gameplay,
                            assets,
                            &mut self.session,
                            options.controls,
                            &mut self.quit_after_body_entry,
                            #[cfg(feature = "dev-tools")]
                            &mut self.trace,
                            #[cfg(feature = "dev-tools")]
                            &mut self.timing,
                            #[cfg(feature = "dev-tools")]
                            (
                                now,
                                self.next_frame
                                    .filter(|_| options.playback == Playback::Animated),
                            ),
                        )?;
                        self.pending_publication = options.playback == Playback::Animated
                            && gameplay.pass_one_wait_required();
                        if !self.pending_publication {
                            #[cfg(feature = "dev-tools")]
                            let host_start = self.timing.as_mut().map(TimingTrace::now);
                            self.visible_frame = Some(gameplay.pass_frame().clone());
                            repaint = true;
                            #[cfg(feature = "dev-tools")]
                            if let Some(trace) = &mut self.timing {
                                let end = trace.now();
                                trace.span(
                                    "pass_publish",
                                    now,
                                    self.next_frame.filter(|_| timed),
                                    (host_start.unwrap(), end),
                                    gameplay.pass_snapshot(),
                                    "delayed=false",
                                );
                            }
                        }
                        self.transitions
                            .append(gameplay.take_transitions(), now, unit);
                        repaint |= self.transitions.active();
                        #[cfg(feature = "dev-tools")]
                        if self.transitions.active() {
                            if let Some(trace) = &mut self.trace {
                                trace.boundary(
                                    "transition_started",
                                    now,
                                    self.transitions.deadline,
                                    &self.session,
                                    "",
                                );
                            }
                        }
                        if self.transitions.active() {
                            break;
                        }
                    }
                    if gameplay.at_body_boundary() {
                        self.body_polled = false;
                    }
                    self.pending_step =
                        options.playback == Playback::Step && !gameplay.at_body_boundary();
                }
            } else {
                self.pending_step = false;
            }
            self.color_family = gameplay.palette_family();
            let period = self
                .ending
                .as_ref()
                .map_or(unit, |state| options.body_period * state.wait_units() / 4);
            #[cfg(feature = "dev-tools")]
            let previous_deadline = self.next_frame;
            self.next_frame = if gameplay.complete()
                && !gameplay.snapshot().player.game_over
                && self.ending.is_none()
            {
                None
            } else if self.ending.is_some() || !timed {
                // Ending frames carry deliberate visible waits, like transitions.
                Some(now + period)
            } else {
                self.next_frame.map(|deadline| deadline + period)
            };
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut self.trace {
                if self.ending.is_some() || !timed || self.next_frame.is_none() {
                    trace.rebase(
                        if self.ending.is_some() {
                            "ending_wait"
                        } else if self.next_frame.is_none() {
                            "preview_finished"
                        } else {
                            "step_wait"
                        },
                        now,
                        previous_deadline,
                        self.next_frame,
                        &self.session,
                    );
                }
            }
            if self.ending.is_some()
                || gameplay.complete()
                || options.playback != Playback::Animated
            {
                break;
            }
        }
        if self.transitions.active()
            || self.ending.is_some()
            || self.session.pause.state != PauseState::Running
            || self.next_frame.is_none()
        {
            self.history.apply_until(&mut self.session, now);
        }
        Ok(repaint)
    }
}

pub(super) fn run(
    frames: &StartupFrames,
    options: Options,
    rules: Option<ScriptRules>,
) -> Result<(), String> {
    let scale = options.scale;
    if scale.is_some_and(|scale| !(1..=16).contains(&scale)) {
        return Err("window scale must be auto or an integer between 1 and 16".into());
    }
    if options.body_period.is_zero() || options.body_period > std::time::Duration::from_secs(5) {
        return Err("opening period must be greater than zero and at most five seconds".into());
    }
    let mut game = GamePlayback::default();
    game.session.keyboard.set_f3_mode(options.f3_mode);
    game.session.select_intro(); // First intro consumes exactly one random word.
    let mut returned_intro = None;
    let mut step_key_held = false;
    let mut host_pause = HostPause::default();
    let mut input = ControlInput::default();
    let mut window = Window::new_with_presentation(
        &game_title(GAME_NAME, Some(screen_status(Screen::Splash))),
        WIDTH as u32,
        DISPLAY_HEIGHT,
        scale,
        options.presentation,
    )?;
    #[cfg(feature = "dev-tools")]
    if let Some(path) = &options.input_trace {
        game.trace = Some(InputTrace::create(path)?);
        game.trace.as_mut().unwrap().boundary(
            "startup",
            std::time::Instant::now(),
            None,
            &game.session,
            &format!(
                "period_ns={} controls={} playback={:?} f3_mode={}",
                options.body_period.as_nanos(),
                options.controls,
                options.playback,
                options.f3_mode.as_str()
            ),
        );
    }
    #[cfg(feature = "dev-tools")]
    if let Some(path) = &options.timing_trace {
        game.timing = Some(TimingTrace::create(path, options.body_period)?);
    }
    let creator = window.texture_creator();
    let mut gamepads = reforge_sdl::gamepad::Gamepads::new(window.settings().stick_dead_zone);
    let mut texture =
        FrameTexture::new(&creator, WIDTH as u32 * 2, HEIGHT as u32, FrameFormat::Rgb)?;
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
        .with_mach3_options(options.digitized_samples);
    game.session.controls.preferences.digitized_samples_enabled =
        menu.mach3_startup_settings().digitized_samples;
    let mut menu_pause = MenuPause::default();
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    #[cfg(feature = "dev-tools")]
    let present_start = game.timing.as_mut().map(TimingTrace::now);
    texture
        .update(&display_rgb(
            frames.frame(shown),
            DisplayMode::Color,
            game.color_family,
        ))
        .map_err(|error| error.to_string())?;
    window.present(&texture)?;
    #[cfg(feature = "dev-tools")]
    if let Some(trace) = &mut game.timing {
        let end = trace.now();
        trace.span(
            "present",
            host_pause.now(end),
            None,
            (present_start.unwrap(), end),
            None,
            &format!(
                "screen={shown:?} controls={} playback={:?}",
                options.controls, options.playback
            ),
        );
        trace.flush()?;
    }
    loop {
        let mut repaint = false;
        gamepads.set_dead_zone(window.settings().stick_dead_zone);
        for event in window.poll_events() {
            let pad = gamepads.handle(&event);
            let clock = reforge_sdl::event_clock_sample();
            let received = std::time::Instant::now();
            let time = EventTime::sample(&event, clock, &host_pause, received);
            repaint |= matches!(event, Event::Window { .. });
            let was_scene = presentation.screen() == Screen::InitialScene;
            // Quit remains global; Escape closes settings before it can quit.
            let pad_consumed = pad.is_some_and(|pad| menu.handle_gamepad(pad));
            let menu_consumed = !host_quit(&event) && (menu.handle_event(&event) || pad_consumed);
            if menu_pause.update(menu.is_open(), &mut host_pause, received) {
                gamepads.reset();
                game.reset_host_input(&mut input, &host_pause, received);
                presentation.focus_lost();
                step_key_held = false;
                repaint = true;
            }
            if menu_consumed {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "settings_menu", &host_pause, &game.session);
                }
                continue;
            }
            let action = if pad.is_some_and(|pad| pad.pause) {
                Action::ToggleHostPause
            } else if pad.is_some_and(|pad| pad.pressed.action) && !host_pause.is_paused() {
                // Navigation only: gameplay receives the separate joystick mask.
                presentation.advance_screen();
                Action::Space
            } else {
                event_action(&mut presentation, event.clone(), host_pause.is_paused())
            };
            if action == Action::Close {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "close", &host_pause, &game.session);
                    trace.flush()?;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.timing {
                    trace.flush()?;
                }
                return Ok(());
            }
            if action == Action::ToggleHostPause {
                if was_scene {
                    gamepads.reset();
                    game.toggle_host_pause(&mut host_pause, &mut input, received);
                    step_key_held = false;
                    repaint = true;
                }
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(
                        &event,
                        if was_scene {
                            "host_shortcut"
                        } else {
                            "ignored_outside_gameplay"
                        },
                        &host_pause,
                        &game.session,
                    );
                    if was_scene {
                        trace.boundary(
                            if host_pause.is_paused() {
                                "host_pause_started"
                            } else {
                                "host_pause_ended"
                            },
                            host_pause.now(std::time::Instant::now()),
                            game.next_frame,
                            &game.session,
                            "held_cleared=true",
                        );
                    }
                }
                continue;
            }
            if host_pause.is_paused() {
                #[cfg(feature = "dev-tools")]
                if let Some(trace) = &mut game.trace {
                    trace.event(&event, "ignored_host_pause", &host_pause, &game.session);
                }
                continue;
            }
            if let Some(pad) = pad {
                if game.history.accepts(time.host) {
                    game.history
                        .push_joystick(time.timeline, pad.actions.bits());
                }
            }
            match &event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    if !step_key_held {
                        game.pending_step |= was_scene && options.playback == Playback::Step;
                    }
                    step_key_held = true;
                }
                Event::KeyUp {
                    keycode: Some(Keycode::Return),
                    ..
                }
                | Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => step_key_held = false,
                _ => {}
            }
            let disposition = input.receive(
                &mut game.session,
                &mut game.history,
                &event,
                was_scene,
                time,
            );
            #[cfg(not(feature = "dev-tools"))]
            let _ = disposition;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.event_with_input_time(
                    &event,
                    disposition,
                    &host_pause,
                    &game.session,
                    (disposition == "queued")
                        .then(|| game.history.queued_at())
                        .flatten(),
                );
            }
            if action == Action::Space && was_scene {
                if let Some(state) = game.ending.as_mut().filter(|state| state.can_return()) {
                    game.history.apply_until(&mut game.session, time.now);
                    returned_intro = state.returned_intro_choice();
                    game.session
                        .remember_ending(&state.snapshot(), state.special_snapshots());
                    presentation.return_to_intro();
                    #[cfg(feature = "dev-tools")]
                    if let Some(trace) = &mut game.trace {
                        trace.rebase(
                            "return_to_intro",
                            host_pause.now(std::time::Instant::now()),
                            game.next_frame,
                            None,
                            &game.session,
                        );
                    }
                    game.next_frame = None;
                }
            }
        }
        #[cfg(feature = "dev-tools")]
        if let Some(trace) = &mut game.trace {
            trace.flush()?;
        }
        prepare_settings(&mut menu, &mut window);
        if menu_pause.update(menu.is_open(), &mut host_pause, std::time::Instant::now()) {
            gamepads.reset();
            game.reset_host_input(&mut input, &host_pause, std::time::Instant::now());
            presentation.focus_lost();
            step_key_held = false;
            repaint = true;
        }
        repaint |= menu.is_open();
        if host_pause.is_paused() {
            if repaint {
                let status = if menu.is_open() {
                    "Settings (F10 or Escape to close, Ctrl+Q to quit)"
                } else {
                    "Paused (P or Pause/Break to continue, Ctrl+Q to close)"
                };
                window.set_title(&game_title(GAME_NAME, Some(status)))?;
                window.present_with(&texture, |canvas| menu.paint(canvas))?;
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
            continue;
        }
        if presentation.screen() != shown {
            shown = presentation.screen();
            repaint = true;
            if shown == Screen::Intro {
                game.color_family = 1; // Repeat entry selects palette family1 before intro.
            }
            if shown == Screen::InitialScene && options.playback != Playback::Static {
                game.begin_game(
                    frames.graphics(),
                    rules.as_ref(),
                    &options,
                    host_pause.now(std::time::Instant::now()),
                )?;
                returned_intro = None;
            }
        }
        let now = host_pause.now(std::time::Instant::now());
        if shown == Screen::InitialScene {
            repaint |= game.advance(frames.graphics(), &options, now)?;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.trace {
                trace.flush()?;
            }
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.timing {
                trace.flush()?;
            }
            // Repump events before the next bounded catch-up batch. Presenting
            // here would add another vsync wait while ordinary work is overdue.
            if game.ordinary_due(&options, now) {
                continue;
            }
        }
        if repaint {
            #[cfg(feature = "dev-tools")]
            let present_start = game.timing.as_mut().map(TimingTrace::now);
            let frame = if shown == Screen::InitialScene {
                game.transitions
                    .frames
                    .front()
                    .map(|step| &step.frame)
                    .or_else(|| game.ending.as_ref().map(Ending::frame))
                    .or(game.visible_frame.as_ref())
                    .or_else(|| game.gameplay.as_ref().map(Gameplay::frame))
                    .unwrap_or_else(|| frames.frame(shown))
            } else if shown == Screen::Intro {
                returned_intro
                    .map_or_else(|| frames.frame(shown), |choice| frames.return_intro(choice))
            } else {
                frames.frame(shown)
            };
            texture
                .update(&display_rgb(
                    frame,
                    game.session.controls.preferences.display_mode,
                    game.color_family,
                ))
                .map_err(|error| error.to_string())?;
            let status = if game.session.pause.state != PauseState::Running {
                "Paused (release keys, then F1 to end game or another key to resume)"
            } else if shown != Screen::InitialScene {
                screen_status(shown)
            } else if game.ending.as_ref().is_some_and(Ending::can_return) {
                "Game Over / High Score (Space to return, Esc to close)"
            } else if game.ending.is_some() {
                "Game Over / High Score (Esc to close)"
            } else if game.gameplay.as_ref().is_some_and(Gameplay::complete) {
                "Preview complete (Esc to close)"
            } else if options.playback == Playback::Step {
                "Opening (Enter to step, F1 to pause, Esc to close)"
            } else {
                "Opening (F1 to pause, Esc to close)"
            };
            window.set_title(&game_title(GAME_NAME, Some(status)))?;
            window.present_with(&texture, |canvas| menu.paint(canvas))?;
            #[cfg(feature = "dev-tools")]
            if let Some(trace) = &mut game.timing {
                let end = trace.now();
                trace.span(
                    "present",
                    now,
                    None,
                    (present_start.unwrap(), end),
                    game.gameplay.as_ref().map(Gameplay::snapshot).as_ref(),
                    &format!(
                        "screen={shown:?} transition={} ending={} host_paused={} pending_publication={}",
                        game.transitions.active(),
                        game.ending.is_some(),
                        host_pause.is_paused(),
                        game.pending_publication
                    ),
                );
                trace.flush()?;
            }
        }
        let poll_interval = std::time::Duration::from_millis(4);
        let deadline = game.transitions.deadline.or_else(|| {
            game.next_frame.filter(|_| {
                options.playback == Playback::Animated
                    && game.session.pause.state == PauseState::Running
            })
        });
        let delay = deadline.map_or(poll_interval, |deadline| {
            deadline
                .saturating_duration_since(host_pause.now(std::time::Instant::now()))
                .min(poll_interval)
        });
        std::thread::sleep(delay);
    }
}

#[cfg(test)]
mod tests;
