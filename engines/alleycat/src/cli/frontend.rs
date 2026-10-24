// Copyright (c) 2026 analogued. Project license has not yet been selected.

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

#[cfg(feature = "sdl")]
pub(super) fn interactive(
    alley: &mut Alley,
    scale: Option<u32>,
    presentation: reforge_sdl::Presentation,
    startup: Option<Startup>,
    #[cfg(feature = "dev-tools")] steps: Option<usize>,
    #[cfg(feature = "dev-tools")] trace: &mut Option<BufWriter<File>>,
    #[cfg(feature = "dev-tools")] timing_trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use crate::control_history::ControlHistory;
    use alleycat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};

    let display = alleycat::platform::Display::new_with_presentation(
        &reforge_sdl::game_title(GAME_NAME, None),
        320,
        240,
        scale,
        presentation,
    )?;
    let creator = display.texture_creator();
    let mut window = Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?;
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
    let mut runtime = Runtime::new(alley.machine());
    runtime.responsive_controls = window.responsive_controls();
    let mut control_history = ControlHistory::new(window.control_keys());
    let start = Instant::now();
    let mut pause_clock = PauseClock::new(start);
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
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
        let input = window.poll_input();
        if input.quit {
            break;
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
        }
        if input.menu_pressed || restarting {
            window.audio.reset()?;
            window.audio.pause(false);
            alley.sound.reset();
            alley.sound.record(true);
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
                #[cfg(not(feature = "dev-tools"))]
                runtime.advance_pass_with_input(alley, |at| control_history.sample(at))?;
                #[cfg(feature = "dev-tools")]
                {
                    let event =
                        runtime.advance_pass_with_input(alley, |at| control_history.sample(at))?;
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
            window.present(&alley.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.audio.pause(true);
    window.present(&alley.rgba())?;
    Ok(())
}
