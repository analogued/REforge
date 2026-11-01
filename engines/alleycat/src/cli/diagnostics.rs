// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless exports and unpaced accepted-player previews.

#[cfg(feature = "sdl")]
use super::frontend::{SplashExit, interactive, present_game_over, present_menu, present_splash};
use super::*;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
#[cfg(feature = "sdl")]
const BIOS_TICK_HZ: f64 = 18.2065;

#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Default)]
struct ScriptInput {
    controls: Controls,
    action: bool,
}

#[cfg(feature = "dev-tools")]
fn read_script(path: &PathBuf) -> Result<Vec<ScriptInput>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut controls = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || matches!(line, "x,y" | "x,y,action") {
            continue;
        }
        let fields: Vec<_> = line.split(',').collect();
        if !(2..=3).contains(&fields.len()) {
            return Err(format!(
                "Script line {} must contain x,y[,action]",
                index + 1
            ));
        }
        let (x, y) = (fields[0], fields[1]);
        let x = x.trim().parse().map_err(|_| "Invalid script X direction")?;
        let y = y.trim().parse().map_err(|_| "Invalid script Y direction")?;
        if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
            return Err(format!(
                "Script line {} has a direction outside -1..1",
                index + 1
            ));
        }
        let action = match fields.get(2).map(|field| field.trim()) {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err(format!("Script line {} action must be 0 or 1", index + 1)),
        };
        controls.push(ScriptInput {
            controls: Controls { x, y },
            action,
        });
    }
    Ok(controls)
}

#[cfg(feature = "dev-tools")]
pub(super) fn trace_row(
    output: &mut impl Write,
    index: usize,
    alley: &Alley,
) -> Result<(), String> {
    let player = &alley.player;
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        alley.scene(),
        player.x,
        player.y,
        player.vertical_coordinate,
        player.horizontal_direction,
        player.vertical_direction,
        player.horizontal_step,
        player.vertical_step,
        player.phase,
        player.phase_decrement,
        player.support_state,
        player.entry_state,
        player.entry_delay,
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "dev-tools")]
pub(super) const TIMING_HEADER: &str = "event,start_ns,end_ns,bios_tick,pit_counter,retrace,control_polled,accepted,accepted_ax,actor_group_due,fence_wait,mouse_wait,room_wait,scene,scene_changed,last_ordinary_room,previous_ordinary_room,random_state,difficulty,pending_hearts";

#[cfg(feature = "dev-tools")]
pub(super) fn timing_row(
    output: &mut impl Write,
    index: usize,
    event: &PassOutcome,
    selection: alleycat::alley::SelectionTraceState,
) -> Result<(), String> {
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        event.start_nanos,
        event.end_nanos,
        event.tick_low,
        event.pit_counter,
        u8::from(event.retrace),
        u8::from(event.control_polled),
        u8::from(event.acceptance.is_some()),
        event.acceptance.map_or(0, |accepted| accepted.ax),
        u8::from(event.actor_group_due),
        u8::from(event.fence_wait),
        u8::from(event.mouse_wait),
        u8::from(event.room_wait),
        event.scene,
        u8::from(event.scene_changed),
        selection.last_room,
        selection.previous_room,
        selection.random_state,
        selection.difficulty,
        u8::from(selection.pending_hearts),
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "dev-tools")]
pub(super) fn run(
    alley: &mut Alley,
    scale: Option<u32>,
    #[cfg(feature = "sdl")] presentation: reforge_sdl::Presentation,
    options: DiagnosticOptions,
    startup: Option<Startup>,
) -> Result<(), String> {
    #[cfg(not(feature = "sdl"))]
    let _ = scale;
    match options.scene {
        StartScene::Splash => {
            return splash_preview(
                startup.ok_or("Splash preview was not initialized")?.splash,
                scale,
                #[cfg(feature = "sdl")]
                presentation,
                options,
            );
        }
        StartScene::Alley => {}
        StartScene::Birdcage => alley.enter_birdcage(0)?,
        StartScene::Fishbowl | StartScene::Underwater => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Fishbowl play requires default timing or --headless --timed".into());
            }
            if options.scene == StartScene::Fishbowl {
                alley.enter_fishbowl(0)?;
            } else {
                alley.enter_underwater(0)?;
            }
        }
        StartScene::Bookcase => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Bookcase play requires default timing or --headless --timed".into());
            }
            alley.enter_bookcase(0)?;
        }
        StartScene::Cheese => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Cheese play requires default timing or --headless --timed".into());
            }
            alley.enter_cheese(0)?;
        }
        StartScene::Hearts => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Hearts play requires default timing or --headless --timed".into());
            }
            alley.enter_hearts(0)?;
        }
        StartScene::Milk => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Milk play requires default timing or --headless --timed".into());
            }
            alley.enter_milk(0)?;
        }
        StartScene::Completion => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err(
                    "Completion preview requires default timing or --headless --timed".into(),
                );
            }
            alley.enter_completion(0)?;
        }
    }
    let mut trace = options
        .trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create trace: {error}"))?;
    if let Some(trace) = trace.as_mut() {
        writeln!(trace, "index,scene,x,y,vertical_coordinate,x_direction,y_direction,x_step,y_step,phase,phase_decrement,support_state,entry_state,entry_delay")
            .map_err(|error| error.to_string())?;
    }
    let mut timing_trace = options
        .timing_trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create timing trace: {error}"))?;
    if let Some(output) = timing_trace.as_mut() {
        writeln!(output, "{TIMING_HEADER}").map_err(|error| error.to_string())?;
    }
    if options.headless {
        let script = options.script.as_ref().map(read_script).transpose()?;
        let steps = options
            .steps
            .unwrap_or_else(|| script.as_ref().map_or(60, Vec::len));
        let mut runtime = Runtime::new(alley.machine());
        let mut index = 0;
        let mut pass = 0;
        while index < steps {
            let input = script
                .as_ref()
                .and_then(|script| script.get(index))
                .copied()
                .unwrap_or_default();
            if options.timed {
                let event =
                    runtime.advance_pass_with_action(alley, input.controls, input.action)?;
                if let Some(output) = timing_trace.as_mut() {
                    timing_row(output, pass, &event, alley.selection_trace_state())?;
                }
                pass += 1;
                if alley.game_over() {
                    break;
                }
                if event.acceptance.is_none() {
                    continue;
                }
            } else {
                alley.advance_player_update(input.controls)?;
            }
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, alley)?;
            }
            index += 1;
        }
    } else {
        #[cfg(feature = "sdl")]
        if let Some(rate) = options.step_hz {
            run_player_preview(
                alley,
                scale,
                presentation,
                startup,
                rate,
                options.steps,
                &mut trace,
            )?;
        } else {
            interactive(
                alley,
                scale,
                presentation,
                startup,
                options.steps,
                &mut trace,
                &mut timing_trace,
            )?;
        }
        #[cfg(not(feature = "sdl"))]
        return Err(
            "This build has no window backend; use --headless or enable the sdl feature".into(),
        );
    }
    if let Some(trace) = trace.as_mut() {
        trace.flush().map_err(|error| error.to_string())?;
    }
    if let Some(output) = timing_trace.as_mut() {
        output.flush().map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.cga {
        let mut file = File::create_new(path).map_err(|error| error.to_string())?;
        file.write_all(alley.framebuffer().bytes())
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.ppm {
        let mut file = BufWriter::new(File::create_new(path).map_err(|error| error.to_string())?);
        write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").map_err(|error| error.to_string())?;
        for rgba in alley.rgba().chunks_exact(4) {
            file.write_all(&rgba[..3])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "dev-tools")]
fn splash_preview(
    mut splash: Splash,
    scale: Option<u32>,
    #[cfg(feature = "sdl")] presentation: reforge_sdl::Presentation,
    options: DiagnosticOptions,
) -> Result<(), String> {
    if options.headless {
        let mut clock = VirtualClock::new(RetraceProfile::for_machine(splash.machine()));
        let ticks = options.steps.unwrap_or(60);
        while !splash.finished() && usize::from(clock.bios_sample().tick_low) < ticks {
            advance_splash(&mut splash, &mut clock, false)?;
        }
    } else {
        #[cfg(feature = "sdl")]
        {
            let display = alleycat::platform::Display::new_with_geometry(
                &reforge_sdl::game_title(GAME_NAME, Some("Splash")),
                alleycat::platform::DISPLAY_GEOMETRY,
                scale,
                presentation,
            )?;
            let creator = display.texture_creator();
            let mut window =
                alleycat::platform::Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?;
            present_splash(&mut window, &mut splash, options.steps)?;
        }
        #[cfg(not(feature = "sdl"))]
        {
            let _ = scale;
            return Err(
                "This build has no window backend; use --headless or enable the sdl feature".into(),
            );
        }
    }
    if let Some(path) = options.cga {
        let mut file = File::create_new(path).map_err(|error| error.to_string())?;
        file.write_all(splash.framebuffer().bytes())
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.ppm {
        let mut file = BufWriter::new(File::create_new(path).map_err(|error| error.to_string())?);
        write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").map_err(|error| error.to_string())?;
        for rgba in splash.rgba().chunks_exact(4) {
            file.write_all(&rgba[..3])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
fn run_player_preview(
    alley: &mut Alley,
    scale: Option<u32>,
    presentation: reforge_sdl::Presentation,
    startup: Option<Startup>,
    step_hz: f64,
    steps: Option<usize>,
    trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alleycat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};
    let display = alleycat::platform::Display::new_with_geometry(
        &reforge_sdl::game_title(GAME_NAME, None),
        alleycat::platform::DISPLAY_GEOMETRY,
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
    let period = Duration::from_secs_f64(1.0 / step_hz);
    // Original 1200 latches directions at intervals of at least two BIOS ticks,
    // independently of player acceptance and presentation.
    let control_period = Duration::from_secs_f64(2.0 / BIOS_TICK_HZ);
    let mut next_control_poll = Instant::now();
    let mut controls = Controls::default();
    let mut deadline = Instant::now() + period;
    let mut pause_clock = PauseClock::new(Instant::now());
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
    let mut index = 0;
    window.present(&alley.rgba())?;
    while steps.is_none_or(|steps| index < steps) {
        if alley.game_over() {
            if !present_game_over(&mut window, alley)? {
                return Ok(());
            }
            let now = Instant::now();
            deadline = now + period;
            next_control_poll = now;
            controls = Controls::default();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let input = window.poll_input();
        if input.quit {
            break;
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
            let now = Instant::now();
            deadline = now + period;
            next_control_poll = now;
            controls = Controls::default();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        if input.settings_changed {
            controls = Controls::default();
        }
        if let Some(duration) = pause.resumed_after {
            // Preserve the remaining diagnostic step/effect and input waits.
            deadline += duration;
            next_control_poll += duration;
        }
        if pause.changed || pause.paused {
            if pause.changed || window.presentation_due() {
                window.present_with_pause(
                    &alley.rgba(),
                    (pause.paused && !input.settings_open).then_some(&pause_overlay),
                )?;
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        if now >= next_control_poll {
            // Left and up take precedence, matching the original keyboard decoder.
            controls = Controls {
                x: if input.left {
                    -1
                } else {
                    i8::from(input.right)
                },
                y: if input.up { -1 } else { i8::from(input.down) },
            };
            next_control_poll += control_period;
            if now >= next_control_poll {
                next_control_poll = now + control_period;
            }
        }
        if now >= deadline {
            let outcome = alley.advance_player_update(controls)?;
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, alley)?;
            }
            index += 1;
            deadline += period;
            if outcome.edge_reset {
                // The original fence-reset speaker effect blocks game work
                // until seven BIOS ticks elapse. Preserve a diagnostic pause
                // without blocking host input; synthesis/PRNG timing is deferred.
                deadline = now + Duration::from_secs_f64(7.0 / BIOS_TICK_HZ);
                // Interrupts may update held keys during the effect, but the
                // original direction poll resumes only after it returns.
                next_control_poll = deadline;
            }
            // Keep focus/debugger pauses from producing a burst of old input updates.
            if now.saturating_duration_since(deadline) > period * 4 {
                deadline = now + period;
            }
            window.present(&alley.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
