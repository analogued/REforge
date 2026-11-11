// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! SDL session lifecycle, host pacing and title/menu presentation.

#[cfg(feature = "dev-tools")]
use super::diagnostics::{timing_row, trace_row};
use super::*;
#[cfg(feature = "dev-tools")]
use std::{fs::File, io::BufWriter};

#[cfg(feature = "sdl")]
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SplashExit {
    Quit,
    Keyboard,
    Controller,
}

#[cfg(feature = "sdl")]
pub(super) fn present_splash(
    window: &mut alleycat::platform::Window,
    splash: &mut Splash,
    ticks: Option<usize>,
) -> Result<SplashExit, String> {
    use std::time::{Duration, Instant};
    window.audio.reset()?;
    splash.sound.record(true);
    let mut clock = VirtualClock::new(RetraceProfile::for_machine(splash.machine()));
    let start = Instant::now();
    let mut pause_clock = alleycat::platform::PauseClock::new(start);
    let mut target_nanos = 0;
    let mut advance_source = None;
    window.present(&splash.rgba())?;
    while !splash.finished()
        && ticks.is_none_or(|limit| usize::from(clock.bios_sample().tick_low) < limit)
    {
        let input = window.poll_input();
        if input.quit {
            return Ok(SplashExit::Quit);
        }
        if input.sound_pressed {
            splash.toggle_sound();
        }
        let now = Instant::now();
        // The settings panel pauses title animation without changing native state.
        // Ordinary title keys remain available when the panel is closed.
        let pause = pause_clock.sample(
            now,
            alleycat::platform::Input {
                pause_pressed: false,
                ..input
            },
        );
        target_nanos += pause.elapsed.min(Duration::from_millis(100)).as_nanos();
        window.audio.pause(pause.paused);
        let mut key_pressed = input.continue_pressed;
        while !pause.paused
            && !splash.finished()
            && (clock.elapsed_nanos() < target_nanos || key_pressed)
        {
            if key_pressed {
                // Splash latches its first advance request until it can finish.
                // Do not let later input change the source of that request.
                advance_source.get_or_insert(
                    if input.controller_confirm && input.menu_key.is_none() {
                        SplashExit::Controller
                    } else {
                        SplashExit::Keyboard
                    },
                );
            }
            advance_splash(splash, &mut clock, key_pressed)?;
            key_pressed = false;
        }
        window.audio.service(
            &mut splash.sound,
            clock.elapsed_pit_ticks() as u64,
            target_nanos,
        );
        if pause.changed || window.presentation_due() {
            window.present(&splash.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.audio.pause(true);
    window.present(&splash.rgba())?;
    Ok(advance_source.unwrap_or(SplashExit::Keyboard))
}

#[cfg(feature = "sdl")]
pub(super) fn present_menu(
    window: &mut alleycat::platform::Window,
    alley: &mut Alley,
    difficulty: Option<u16>,
    initial: bool,
    controller_title: bool,
) -> Result<bool, String> {
    use std::time::Duration;
    window.audio.pause(true);
    let mut menu = alley.menu(difficulty)?;
    if controller_title {
        menu.reveal_cursor()?;
    }
    window.present(&menu.rgba())?;
    loop {
        let input = window.poll_input();
        if input.quit {
            return Ok(false);
        }
        if input.sound_pressed {
            alley.toggle_sound();
        }
        let changed = input.menu_key.is_some()
            || input.controller_confirm
            || input.controller_menu_delta != 0;
        let finished = if let Some(key) = input.menu_key {
            menu.press(key)?
        } else {
            menu.controller(input.controller_menu_delta, input.controller_confirm)?
        };
        if finished {
            let difficulty = menu.difficulty().ok_or("Menu has no skill selection")?;
            if initial {
                alley.start_initial_game(difficulty, 0)?;
            } else {
                alley.start_new_game(difficulty, 0)?;
            }
            window.reset_controller();
            return Ok(true);
        }
        // The native question screen is static, but the host menu needs frames
        // for pointer hover, live settings and window resize updates.
        if window.presentation_due() || changed || input.settings_changed {
            window.present(&menu.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(feature = "sdl")]
pub(super) fn present_game_over(
    window: &mut alleycat::platform::Window,
    alley: &mut Alley,
) -> Result<bool, String> {
    let mut splash = alley.game_over_splash()?;
    if present_splash(window, &mut splash, None)? == SplashExit::Quit {
        return Ok(false);
    }
    alley.start_after_game_over(&splash)?;
    // The title's South confirmation may also be the selected Jump button.
    // Require a fresh controller gesture before the new gameplay epoch starts.
    window.reset_controller();
    Ok(true)
}

#[cfg(feature = "sdl")]
pub(super) const CATCH_UP_MAX_PASSES: usize = 256;
#[cfg(feature = "sdl")]
const CATCH_UP_TIME_SLICE: std::time::Duration = std::time::Duration::from_millis(4);

/// Yield between complete guest passes; never fast-forward or discard work.
/// A time slice also bounds slow-host batches, except for their final pass.
#[cfg(feature = "sdl")]
pub(super) fn catch_up_batch(
    mut elapsed: impl FnMut() -> std::time::Duration,
    mut advance: impl FnMut() -> Result<bool, String>,
) -> Result<usize, String> {
    let mut passes = 0;
    while passes < CATCH_UP_MAX_PASSES {
        // Always permit one pass so an expensive pass cannot prevent progress.
        if passes != 0 && elapsed() >= CATCH_UP_TIME_SLICE {
            break;
        }
        if !advance()? {
            break;
        }
        passes += 1;
    }
    Ok(passes)
}

/// The persisted scene choice is independent of input polling. Comparison
/// overrides are development-only and never write the player's preference.
struct ScenePresentation {
    selected: SceneMode,
    history: crate::motion::history::HistoryPresenter,
    failed: bool,
    #[cfg(feature = "dev-tools")]
    comparison: Option<crate::motion::Mode>,
    #[cfg(feature = "dev-tools")]
    predicted: crate::motion::Presenter,
}

impl ScenePresentation {
    fn new(selected: SceneMode) -> Self {
        Self {
            selected,
            history: Default::default(),
            failed: false,
            #[cfg(feature = "dev-tools")]
            comparison: None,
            #[cfg(feature = "dev-tools")]
            predicted: Default::default(),
        }
    }

    fn reset(&mut self) {
        self.history.reset();
        self.failed = false;
        #[cfg(feature = "dev-tools")]
        self.predicted.reset();
    }

    fn select(&mut self, selected: SceneMode) {
        if self.selected != selected {
            self.selected = selected;
            #[cfg(feature = "dev-tools")]
            {
                self.comparison = None;
            }
            self.reset();
        }
    }

    #[cfg(feature = "dev-tools")]
    fn cycle(&mut self) {
        self.comparison = Some(
            self.comparison
                .unwrap_or(match self.selected {
                    SceneMode::Original => crate::motion::Mode::Original,
                    SceneMode::Reforged => crate::motion::Mode::HeadHistory60,
                })
                .next(),
        );
        self.reset();
    }

    #[cfg(feature = "dev-tools")]
    fn label(&self) -> Option<&'static str> {
        self.comparison.map(|mode| {
            mode.label(if mode.delay_ns().is_some() {
                self.history.is_active()
            } else {
                self.predicted.is_active()
            })
        })
    }

    fn observe(&mut self, alley: &Alley, event: &crate::runtime::PassOutcome, now_ns: u128) {
        if self.failed {
            return;
        }
        let moving = !(event.fence_wait || event.mouse_wait || event.room_wait);
        #[cfg(feature = "dev-tools")]
        let result = if let Some(mode) = self.comparison {
            if mode.delay_ns().is_some() {
                self.history.observe(alley, event.end_nanos, now_ns, moving)
            } else if mode != crate::motion::Mode::Original {
                self.predicted
                    .observe(alley, event.end_nanos, now_ns, moving)
            } else {
                Ok(())
            }
        } else if self.selected == SceneMode::Reforged {
            self.history.observe(alley, event.end_nanos, now_ns, moving)
        } else {
            Ok(())
        };
        #[cfg(not(feature = "dev-tools"))]
        let result = if self.selected == SceneMode::Reforged {
            self.history.observe(alley, event.end_nanos, now_ns, moving)
        } else {
            Ok(())
        };
        if let Err(error) = result {
            eprintln!("Alley Cat: scene smoothing unavailable: {error}; using Original movement");
            self.reset();
            self.failed = true;
        }
    }

    fn rgba(&self, alley: &Alley, now_ns: u128) -> Vec<u8> {
        if self.failed {
            return alley.rgba();
        }
        #[cfg(feature = "dev-tools")]
        if let Some(mode) = self.comparison {
            return match mode {
                crate::motion::Mode::Original => alley.rgba(),
                crate::motion::Mode::Aligned => self
                    .predicted
                    .stepped_rgba()
                    .unwrap_or_else(|| alley.rgba()),
                crate::motion::Mode::Smooth => {
                    self.predicted.rgba(now_ns).unwrap_or_else(|| alley.rgba())
                }
                crate::motion::Mode::HistoryRivals25 | crate::motion::Mode::HistoryRivals40 => self
                    .history
                    .rgba_with_rival_delay(now_ns, 25_000_000, mode.rival_delay_ns().unwrap())
                    .unwrap_or_else(|| alley.rgba()),
                crate::motion::Mode::HeadHistory60 | crate::motion::Mode::HeadNative60 => self
                    .history
                    .rgba_with_hearts_drop(now_ns, mode == crate::motion::Mode::HeadHistory60)
                    .unwrap_or_else(|| alley.rgba()),
                crate::motion::Mode::HistorySwim25 => self
                    .history
                    .rgba_with_swimming(now_ns, 25_000_000, 40_000_000)
                    .unwrap_or_else(|| alley.rgba()),
                crate::motion::Mode::BirdVelocity | crate::motion::Mode::BirdHistory25 => self
                    .history
                    .rgba_with_bird(
                        now_ns,
                        25_000_000,
                        40_000_000,
                        if mode == crate::motion::Mode::BirdVelocity {
                            crate::motion::history::BirdPolicy::Velocity
                        } else {
                            crate::motion::history::BirdPolicy::History
                        },
                    )
                    .unwrap_or_else(|| alley.rgba()),
                crate::motion::Mode::History25
                | crate::motion::Mode::History35
                | crate::motion::Mode::History40
                | crate::motion::Mode::History60 => self
                    .history
                    .rgba(now_ns, mode.delay_ns().unwrap())
                    .unwrap_or_else(|| alley.rgba()),
            };
        }
        match self.selected {
            SceneMode::Original => alley.rgba(),
            SceneMode::Reforged => self
                .history
                .rgba_with_hearts_drop(now_ns, true)
                .unwrap_or_else(|| alley.rgba()),
        }
    }
}

#[cfg(feature = "sdl")]
pub(super) fn interactive(
    alley: &mut Alley,
    scale: Option<u32>,
    presentation: reforge_sdl::Presentation,
    startup: Option<Startup>,
    motion_choice: Option<SceneMode>,
    #[cfg(feature = "dev-tools")] steps: Option<usize>,
    #[cfg(feature = "dev-tools")] (trace, timing_trace): (
        &mut Option<BufWriter<File>>,
        &mut Option<BufWriter<File>>,
    ),
) -> Result<(), String> {
    use crate::control_history::ControlHistory;
    use alleycat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};

    let display = alleycat::platform::Display::new_with_geometry(
        &reforge_sdl::game_title(GAME_NAME, None),
        alleycat::platform::DISPLAY_GEOMETRY,
        scale,
        presentation,
    )?;
    let creator = display.texture_creator();
    let mut window = Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?
        .with_input_format(alley.input_format())
        .with_scene_motion(motion_choice)?;
    if let Some(mut startup) = startup {
        let splash_exit = present_splash(&mut window, &mut startup.splash, None)?;
        if splash_exit == SplashExit::Quit {
            return Ok(());
        }
        alley.inherit_initial_splash(&startup.splash)?;
        if !present_menu(
            &mut window,
            alley,
            startup.difficulty_override,
            true,
            splash_exit == SplashExit::Controller,
        )? {
            return Ok(());
        }
    }
    window.audio.reset()?;
    window.audio.pause(false);
    alley.sound.reset();
    alley.sound.record(true);
    let mut scene_presentation = ScenePresentation::new(window.scene_motion());
    let mut runtime = Runtime::new(alley.machine());
    runtime.responsive_controls = window.responsive_controls();
    let mut control_history = ControlHistory::new(window.control_keys());
    let start = Instant::now();
    let mut pause_clock = PauseClock::new(start);
    let pause_overlay =
        PauseOverlay::with_text_color(alley.pause_text()?[0], alley.machine(), alley.text_color())?;
    let mut target_nanos = 0;
    #[cfg(feature = "dev-tools")]
    let mut index = 0;
    #[cfg(feature = "dev-tools")]
    let mut pass = 0;
    window.present(&alley.rgba())?;
    loop {
        #[cfg(feature = "dev-tools")]
        if steps.is_some_and(|limit| index >= limit) {
            break;
        }
        if alley.game_over() {
            if !present_game_over(&mut window, alley)? {
                return Ok(());
            }
            window.audio.reset()?;
            window.audio.pause(false);
            alley.sound.reset();
            alley.sound.record(true);
            scene_presentation.reset();
            runtime = Runtime::new(alley.machine());
            runtime.responsive_controls = window.responsive_controls();
            control_history = ControlHistory::new(window.control_keys());
            target_nanos = 0;
            let now = Instant::now();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        runtime.responsive_controls = window.responsive_controls();
        let input = window.poll_game_input(alley.scene() == 2);
        if input.quit {
            break;
        }
        scene_presentation.select(window.scene_motion());
        #[cfg(feature = "dev-tools")]
        if input.motion_pressed {
            scene_presentation.cycle();
            if let Some(mode) = scene_presentation.comparison {
                eprintln!("Motion comparison: {}", mode.name());
            }
        }
        if input.sound_pressed {
            alley.toggle_sound();
        }
        let restarting = input.new_game_pressed;
        if input.menu_pressed {
            if !present_menu(&mut window, alley, None, false, false)? {
                break;
            }
        } else if restarting {
            alley.restart(0)?;
            window.reset_controller();
        }
        if input.menu_pressed || restarting {
            window.audio.reset()?;
            window.audio.pause(false);
            alley.sound.reset();
            alley.sound.record(true);
            scene_presentation.reset();
            runtime = Runtime::new(alley.machine());
            runtime.responsive_controls = window.responsive_controls();
            control_history = ControlHistory::new(window.control_keys());
            target_nanos = 0;
            let now = Instant::now();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let (host_now_ns, changes) = window.take_control_changes();
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        window.audio.pause(pause.paused);
        if pause.changed {
            scene_presentation.reset();
            window.reset_controller();
            // Preserve pending guest work across the settings panel; retain the
            // existing manual-pause policy of discarding partial pacing budget.
            if !input.settings_changed {
                target_nanos = runtime.clock.elapsed_nanos();
                control_history = ControlHistory::new(window.control_keys());
            } else {
                control_history.boundary(target_nanos, window.control_keys());
            }
            // Manual pause can discard catch-up debt and move this target
            // backward, or forward over committed blocking work. Retain audio
            // across that explicit mapping boundary instead of aging/replaying it.
            window.audio.rebase_presentation(target_nanos);
        }
        if pause.paused {
            if pause.changed || window.presentation_due() {
                window.present_with_pause(
                    &alley.rgba(),
                    (!input.settings_open).then_some(&pause_overlay),
                )?;
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        // Discard excess host stalls without resetting guest timer phases.
        let elapsed = pause.elapsed.min(Duration::from_millis(100));
        if !pause.changed {
            control_history.extend(host_now_ns, elapsed, target_nanos, changes);
        }
        target_nanos += elapsed.as_nanos();
        let batch_start = Instant::now();
        catch_up_batch(
            || batch_start.elapsed(),
            || {
                if runtime.clock.elapsed_nanos() >= target_nanos || alley.game_over() {
                    return Ok(false);
                }
                #[cfg(feature = "dev-tools")]
                if steps.is_some_and(|limit| index >= limit) {
                    return Ok(false);
                }
                let event = runtime.advance_pass_with_scene_input(alley, |at, scene| {
                    control_history.sample(at, scene == 2)
                })?;
                scene_presentation.observe(alley, &event, target_nanos);
                #[cfg(feature = "dev-tools")]
                {
                    if let Some(output) = timing_trace.as_mut() {
                        timing_row(output, pass, &event, alley.selection_trace_state())?;
                    }
                    pass += 1;
                    if event.acceptance.is_some() {
                        if let Some(output) = trace.as_mut() {
                            trace_row(output, index, alley)?;
                        }
                        index += 1;
                    }
                }
                Ok(true)
            },
        )?;
        window.audio.service(
            &mut alley.sound,
            runtime.clock.elapsed_pit_ticks() as u64,
            target_nanos,
        );
        if runtime.clock.elapsed_nanos() < target_nanos && !alley.game_over() {
            // Retain debt and repump events immediately. Intermediate rendering
            // and sleep must not make a slow catch-up batch fall further behind.
            continue;
        }
        if pause.changed || window.presentation_due() {
            #[cfg(feature = "dev-tools")]
            window.set_motion_label(scene_presentation.label());
            window.present(&scene_presentation.rgba(alley, target_nanos))?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.audio.pause(true);
    window.present(&alley.rgba())?;
    Ok(())
}

#[cfg(test)]
mod motion_tests {
    use super::*;

    #[test]
    fn live_scene_changes_discard_history_without_changing_native_execution() {
        for responsive in [false, true] {
            let replay = |switching: bool| {
                let mut alley = Alley::synthetic();
                let mut runtime = Runtime::new(alley.machine());
                runtime.responsive_controls = responsive;
                let mut view = ScenePresentation::new(if switching {
                    SceneMode::Reforged
                } else {
                    SceneMode::Original
                });
                let mut native = Vec::new();
                for pass in 0..80 {
                    if switching && matches!(pass, 20 | 40 | 60) {
                        view.select(if pass == 40 {
                            SceneMode::Reforged
                        } else {
                            SceneMode::Original
                        });
                        assert!(view.history.rgba(0, 0).is_none());
                        assert_eq!(
                            view.rgba(&alley, runtime.clock.elapsed_nanos()),
                            alley.rgba()
                        );
                    }
                    let event = runtime
                        .advance_pass(&mut alley, Controls { x: 1, y: 0 })
                        .unwrap();
                    view.observe(&alley, &event, event.end_nanos);
                    let _ = view.rgba(&alley, event.end_nanos);
                    if view.selected == SceneMode::Original {
                        assert!(view.history.rgba(event.end_nanos, 0).is_none());
                        assert_eq!(view.rgba(&alley, event.end_nanos), alley.rgba());
                    } else {
                        assert!(view.history.rgba(event.end_nanos, 0).is_some());
                    }
                    native.push((
                        format!("{event:?}{:?}", alley.player),
                        alley.framebuffer().bytes().to_vec(),
                    ));
                }
                native
            };
            assert_eq!(replay(false), replay(true));
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn saved_scene_selection_supersedes_development_comparison_without_persisting_it() {
        let mut view = ScenePresentation::new(SceneMode::Reforged);
        view.cycle();
        assert_eq!(view.comparison, Some(crate::motion::Mode::HeadNative60));
        assert_eq!(view.selected, SceneMode::Reforged);
        view.select(SceneMode::Original);
        assert!(view.comparison.is_none());
        assert_eq!(view.selected, SceneMode::Original);
        view.cycle();
        assert_eq!(view.comparison, Some(crate::motion::Mode::Aligned));
        view.select(SceneMode::Reforged);
        assert!(view.comparison.is_none());
    }
}
