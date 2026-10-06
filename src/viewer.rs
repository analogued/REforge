// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and live gameplay.

use std::collections::VecDeque;

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    pixels::{Color, PixelFormat},
    render::{Canvas, FRect, RenderTarget, ScaleMode, Texture},
};

use crate::{
    assets::GraphicsAssets,
    frame::{Frame, HEIGHT, WIDTH},
    opening::{
        BODY_PERIOD, DisplayMode, Ending, Opening, PauseEvent, PauseState, ScriptRules,
        SessionState, Steering, TransitionFrame,
    },
    startup::{Presentation, Screen, StartupFrames},
};

// CGA mode 4 stores 320x200 pixels but fills a 4:3 display (pixel aspect 5:6).
const DISPLAY_HEIGHT: u32 = 240;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playback {
    #[default]
    Animated,
    Step,
    Static,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub scale: u32,
    pub playback: Playback,
    pub controls: bool,
    pub body_period: std::time::Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: 3,
            playback: Playback::Animated,
            controls: false,
            body_period: BODY_PERIOD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Space,
    Close,
}

fn title(screen: Screen) -> &'static str {
    match screen {
        Screen::Splash => "Mach 3 — Splash (Space to advance)",
        Screen::Intro => "Mach 3 — Intro (Space to advance)",
        Screen::InitialScene => "Mach 3 — Initial scene (Esc to close)",
    }
}

fn event_action(presentation: &mut Presentation, event: Event) -> Action {
    match event {
        Event::Quit { .. }
        | Event::Window {
            win_event: WindowEvent::CloseRequested,
            ..
        }
        | Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return Action::Close,
        Event::KeyDown {
            keycode: Some(Keycode::Space),
            repeat,
            ..
        } => {
            if presentation.space_down(repeat) {
                return Action::Space;
            }
        }
        Event::KeyUp {
            keycode: Some(Keycode::Space),
            ..
        } => presentation.space_up(),
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => presentation.focus_lost(),
        _ => {}
    }
    Action::Continue
}

#[cfg(test)]
fn handle_event(presentation: &mut Presentation, event: Event) -> bool {
    event_action(presentation, event) != Action::Close
}

fn destination(width: u32, height: u32) -> Option<FRect> {
    let scale = (width / WIDTH as u32).min(height / DISPLAY_HEIGHT);
    if scale == 0 {
        return None;
    }
    let scaled_width = WIDTH as u32 * scale;
    let scaled_height = DISPLAY_HEIGHT * scale;
    Some(FRect::new(
        ((width - scaled_width) / 2) as f32,
        ((height - scaled_height) / 2) as f32,
        scaled_width as f32,
        scaled_height as f32,
    ))
}

fn paint<T: RenderTarget>(canvas: &mut Canvas<T>, texture: &Texture<'_>) -> Result<(), String> {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    let (width, height) = canvas.output_size().map_err(|error| error.to_string())?;
    if let Some(rect) = destination(width, height) {
        canvas
            .copy(texture, None, rect)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Open a resizable nearest-neighbor viewer. No audio subsystem is initialized.
pub fn run(frames: &StartupFrames, scale: u32) -> Result<(), String> {
    run_with_options(
        frames,
        Options {
            scale,
            ..Options::default()
        },
    )
}

pub fn run_with_options(frames: &StartupFrames, options: Options) -> Result<(), String> {
    run_inner(frames, options, None)
}

/// Run with externally decoded encounter rules, including retained-session restarts.
pub fn run_with_rules(
    frames: &StartupFrames,
    options: Options,
    rules: ScriptRules,
) -> Result<(), String> {
    run_inner(frames, options, Some(rules))
}

fn scan_for_key(key: Keycode) -> Option<u8> {
    Some(match key {
        Keycode::S => 0x1f,
        Keycode::X => 0x2d,
        Keycode::B => 0x30,
        Keycode::N => 0x31,
        Keycode::Up => 0x48,
        Keycode::Down => 0x50,
        Keycode::Left => 0x4b,
        Keycode::Right => 0x4d,
        Keycode::Kp5 => 0x4c,
        Keycode::Space => 0x39,
        Keycode::F1 => 0x3b,
        Keycode::F2 => 0x3c,
        Keycode::F3 => 0x3d,
        _ => return None,
    })
}

fn control_event(session: &mut SessionState, event: &Event, gameplay: bool) {
    match event {
        Event::KeyDown {
            keycode: Some(key), ..
        }
        | Event::KeyUp {
            keycode: Some(key), ..
        } => {
            if *key == Keycode::Space && !gameplay && matches!(event, Event::KeyDown { .. }) {
                return;
            }
            if let Some(scan) = scan_for_key(*key) {
                let raw = scan
                    | if matches!(event, Event::KeyUp { .. }) {
                        128
                    } else {
                        0
                    };
                session.keyboard.apply_scan(raw);
            }
        }
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => session.clear_input(),
        _ => {}
    }
}

fn steering_from_mask(mask: u8, enabled: bool) -> Steering {
    if !enabled {
        return Steering::default();
    }
    Steering {
        up: mask & 1 != 0,
        down: mask & 2 != 0,
        left: mask & 4 != 0,
        right: mask & 8 != 0,
        fire: mask & 16 != 0,
    }
}

/// A fixed 640-pixel texture preserves both raw mode-6 bits and mode-4 pixels.
fn display_rgb(frame: &Frame, mode: DisplayMode, family: u8) -> Vec<u8> {
    let mut output = Vec::with_capacity(WIDTH * 2 * HEIGHT * 3);
    if mode == DisplayMode::Color {
        let palette = if family == 0 {
            [[0, 0, 0], [85, 255, 85], [255, 85, 85], [255, 255, 85]]
        } else {
            crate::frame::PALETTE
        };
        for byte in frame.packed() {
            for shift in [6, 4, 2, 0] {
                let pixel = palette[usize::from((byte >> shift) & 3)];
                output.extend_from_slice(&pixel);
                output.extend_from_slice(&pixel);
            }
        }
    } else {
        for byte in frame.packed() {
            for shift in (0..8).rev() {
                let level = if byte & (1 << shift) != 0 { 255 } else { 0 };
                output.extend_from_slice(&[level; 3]);
            }
        }
    }
    output
}

fn advance_game_pass(
    opening: &mut Opening,
    assets: &GraphicsAssets,
    session: &mut SessionState,
    controls: bool,
    quit_after_body_entry: &mut bool,
) -> Result<bool, String> {
    let result = session.controls.poll(session.keyboard.held_keys(0));
    opening
        .advance_subframe(assets, steering_from_mask(result.mask, controls))
        .map_err(|error| error.to_string())?;
    // Pause ran after the original exit gate. Enter the committed body before
    // setting the flag so complete() permits its remaining two render passes.
    if *quit_after_body_entry {
        opening.request_game_over();
        *quit_after_body_entry = false;
    }
    Ok(result.display_toggled)
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

fn run_inner(
    frames: &StartupFrames,
    options: Options,
    rules: Option<ScriptRules>,
) -> Result<(), String> {
    let scale = options.scale;
    if !(1..=16).contains(&scale) {
        return Err("window scale must be between 1 and 16".into());
    }
    if options.body_period.is_zero() || options.body_period > std::time::Duration::from_secs(5) {
        return Err("opening period must be greater than zero and at most five seconds".into());
    }
    let mut session = SessionState::default();
    session.select_intro(); // First intro consumes exactly one random word.
    let mut opening: Option<Opening> = None;
    let mut color_family = 1;
    let mut ending: Option<Ending> = None;
    let mut returned_intro = None;
    let mut next_frame = None;
    let mut pause_tick = None;
    let mut transitions = TransitionPlayback::default();
    let mut body_polled = false;
    let mut quit_after_body_entry = false;
    let mut pending_step = false;
    let mut step_key_held = false;
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let mut window = video
        .window(
            title(Screen::Splash),
            WIDTH as u32 * scale,
            DISPLAY_HEIGHT * scale,
        )
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    window
        .set_minimum_size(WIDTH as u32, DISPLAY_HEIGHT)
        .map_err(|error| error.to_string())?;
    let mut canvas = sdl3::render::create_renderer(window, Some(c"software"))
        .map_err(|error| error.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormat::RGB24, WIDTH as u32 * 2, HEIGHT as u32)
        .map_err(|error| error.to_string())?;
    texture.set_scale_mode(ScaleMode::Nearest);
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    texture
        .update(
            None,
            &display_rgb(frames.frame(shown), DisplayMode::Color, color_family),
            WIDTH * 6,
        )
        .map_err(|error| error.to_string())?;
    paint(&mut canvas, &texture)?;
    if !canvas.present() {
        return Err(sdl3::get_error().to_string());
    }
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    loop {
        let mut repaint = false;
        for event in events.poll_iter() {
            repaint |= matches!(event, Event::Window { .. });
            let was_scene = presentation.screen() == Screen::InitialScene;
            match &event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    if !step_key_held {
                        pending_step |= was_scene && options.playback == Playback::Step;
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
            control_event(&mut session, &event, was_scene);
            let action = event_action(&mut presentation, event);
            if action == Action::Close {
                return Ok(());
            }
            if action == Action::Space && was_scene {
                if let Some(state) = ending.as_mut().filter(|state| state.can_return()) {
                    returned_intro = state.returned_intro_choice();
                    session.remember_ending(&state.snapshot(), state.special_snapshots());
                    presentation.return_to_intro();
                    next_frame = None;
                }
            }
        }
        if presentation.screen() != shown {
            shown = presentation.screen();
            repaint = true;
            if shown == Screen::Intro {
                color_family = 1; // Repeat entry selects palette family1 before intro.
            }
            if shown == Screen::InitialScene && options.playback != Playback::Static {
                let start = session.begin_game();
                opening = Some(
                    if let Some(rules) = &rules {
                        Opening::for_play(frames.graphics(), rules.clone(), start)
                    } else if options.controls {
                        Opening::with_steering(frames.graphics())
                    } else {
                        Opening::new(frames.graphics())
                    }
                    .map_err(|error| error.to_string())?,
                );
                color_family = opening.as_ref().unwrap().palette_family();
                ending = None;
                transitions = TransitionPlayback::default();
                returned_intro = None;
                body_polled = false;
                quit_after_body_entry = false;
                pending_step = false;
                next_frame = Some(std::time::Instant::now() + options.body_period / 3);
            }
        }
        let now = std::time::Instant::now();
        let transition_was_active = transitions.active();
        if transitions.advance(now, options.body_period / 3) {
            repaint = true;
            if !transitions.active() {
                next_frame = Some(now + options.body_period / 3);
            }
        }
        // Pause's busy input polls are separate from its two timed waits.
        if !transition_was_active && session.pause.state != PauseState::Running {
            if pause_waiting(session.pause.state) {
                if pause_tick.is_some_and(|deadline| now >= deadline) {
                    session.pause.tick();
                    pause_tick = Some(now + options.body_period / 3);
                }
            } else {
                let result = session.controls.poll(session.keyboard.held_keys(0));
                repaint |= result.display_toggled;
                match session.pause.poll(result.mask) {
                    PauseEvent::EndGame => quit_after_body_entry = true,
                    PauseEvent::Resumed => next_frame = Some(now),
                    _ => {}
                }
                if pause_waiting(session.pause.state) {
                    pause_tick = Some(now + options.body_period / 3);
                }
            }
        }
        if shown == Screen::InitialScene
            && !transition_was_active
            && session.pause.state == PauseState::Running
        {
            let timed = options.playback == Playback::Animated
                && next_frame.is_some_and(|deadline| now >= deadline);
            if pending_step || timed {
                if let Some(opening) = &mut opening {
                    repaint = true;
                    if let Some(ending) = &mut ending {
                        ending
                            .advance(frames.graphics())
                            .map_err(|error| error.to_string())?;
                        pending_step = false;
                    } else if opening.complete() && opening.snapshot().player.game_over {
                        ending = Some(
                            Ending::new_with_best(
                                frames.graphics(),
                                &opening.snapshot(),
                                opening.frame(),
                                session.best_score,
                            )
                            .map_err(|error| error.to_string())?,
                        );
                        pending_step = false;
                    } else if !opening.complete() {
                        if opening.at_body_boundary() && !body_polled {
                            let result = session.controls.poll(session.keyboard.held_keys(0));
                            repaint |= result.display_toggled;
                            body_polled = true;
                            session.pause.poll(result.mask);
                        }
                        if session.pause.state == PauseState::Running {
                            let passes = if options.playback == Playback::Step {
                                3 - if opening.at_body_boundary() {
                                    0
                                } else {
                                    opening.snapshot().render_pass
                                }
                            } else {
                                1
                            };
                            for _ in 0..passes {
                                repaint |= advance_game_pass(
                                    opening,
                                    frames.graphics(),
                                    &mut session,
                                    options.controls,
                                    &mut quit_after_body_entry,
                                )?;
                                transitions.append(
                                    opening.take_transitions(),
                                    now,
                                    options.body_period / 3,
                                );
                                if transitions.active() {
                                    break;
                                }
                            }
                            if opening.at_body_boundary() {
                                body_polled = false;
                            }
                            pending_step =
                                options.playback == Playback::Step && !opening.at_body_boundary();
                        }
                    } else {
                        pending_step = false;
                    }
                    color_family = opening.palette_family();
                    let period = ending.as_ref().map_or(options.body_period / 3, |state| {
                        options.body_period * state.wait_units() / 3
                    });
                    next_frame = if opening.complete()
                        && !opening.snapshot().player.game_over
                        && ending.is_none()
                    {
                        None
                    } else if timed {
                        next_frame.map(|deadline| deadline + period)
                    } else {
                        Some(now + period)
                    };
                }
            }
        }
        if repaint {
            let frame = if shown == Screen::InitialScene {
                transitions
                    .frames
                    .front()
                    .map(|step| &step.frame)
                    .or_else(|| ending.as_ref().map(Ending::frame))
                    .or_else(|| opening.as_ref().map(Opening::frame))
                    .unwrap_or_else(|| frames.frame(shown))
            } else if shown == Screen::Intro {
                returned_intro
                    .map_or_else(|| frames.frame(shown), |choice| frames.return_intro(choice))
            } else {
                frames.frame(shown)
            };
            texture
                .update(
                    None,
                    &display_rgb(
                        frame,
                        session.controls.preferences.display_mode,
                        color_family,
                    ),
                    WIDTH * 6,
                )
                .map_err(|error| error.to_string())?;
            let caption = if session.pause.state != PauseState::Running {
                "Mach 3 — Paused (release keys, then F1 to end game or another key to resume)"
            } else if shown != Screen::InitialScene {
                title(shown)
            } else if ending.as_ref().is_some_and(Ending::can_return) {
                "Mach 3 — Game Over / High Score (Space to return, Esc to close)"
            } else if ending.is_some() {
                "Mach 3 — Game Over / High Score (Esc to close)"
            } else if opening.as_ref().is_some_and(Opening::complete) {
                "Mach 3 — Preview complete (Esc to close)"
            } else if options.playback == Playback::Step {
                "Mach 3 — Opening (Enter to step, F1 to pause, Esc to close)"
            } else {
                "Mach 3 — Opening (F1 to pause, Esc to close)"
            };
            canvas
                .window_mut()
                .set_title(caption)
                .map_err(|error| error.to_string())?;
            paint(&mut canvas, &texture)?;
            if !canvas.present() {
                return Err(sdl3::get_error().to_string());
            }
        }
        let poll_interval = std::time::Duration::from_millis(4);
        let deadline = transitions.deadline.or_else(|| {
            next_frame.filter(|_| {
                options.playback == Playback::Animated && session.pause.state == PauseState::Running
            })
        });
        let delay = deadline.map_or(poll_interval, |deadline| {
            deadline
                .saturating_duration_since(std::time::Instant::now())
                .min(poll_interval)
        });
        std::thread::sleep(delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl3::{keyboard::Mod, surface::Surface};

    fn space(down: bool, repeat: bool) -> Event {
        if down {
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
                which: 0,
                raw: 0,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
                which: 0,
                raw: 0,
            }
        }
    }

    #[test]
    fn sdl_events_advance_once_and_close() {
        let mut state = Presentation::default();
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::Intro);
        assert!(handle_event(&mut state, space(true, true)));
        assert_eq!(state.screen(), Screen::Intro);
        assert!(handle_event(
            &mut state,
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost
            }
        ));
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::InitialScene);
        assert!(handle_event(&mut state, space(false, false)));
        assert!(handle_event(&mut state, space(true, false)));
        assert_eq!(state.screen(), Screen::InitialScene);
        assert!(!handle_event(&mut state, Event::Quit { timestamp: 0 }));
        assert!(!handle_event(
            &mut state,
            Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::CloseRequested
            }
        ));
    }

    #[test]
    fn deferred_pause_exit_finishes_the_committed_body_and_four_input_polls() {
        let assets = GraphicsAssets {
            primary: vec![0; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|_| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: [0; 7],
                    color: vec![0; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        };
        let mut opening = Opening::with_steering(&assets).unwrap();
        let mut session = SessionState::default();
        session.keyboard.apply_scan(0x3c); // Held F2 at all four input sites.
        session.controls.poll(session.keyboard.held_keys(0)); // Body-entry poll.
        let mut quit = true; // Pause's second F1 press passed the exit gate.
        advance_game_pass(&mut opening, &assets, &mut session, true, &mut quit).unwrap();
        assert!(opening.snapshot().player.game_over);
        assert!(!opening.complete());
        assert_eq!(opening.snapshot().render_pass, 1);
        for _ in 0..2 {
            advance_game_pass(&mut opening, &assets, &mut session, true, &mut quit).unwrap();
        }
        assert_eq!(opening.snapshot().body, 1);
        assert_eq!(opening.snapshot().render_pass, 3);
        assert!(opening.complete());
        assert!(session.controls.preferences.sound_disabled);
        assert_eq!(session.controls.sound_poll_counter, 0);
    }

    #[test]
    fn startup_space_does_not_fire_and_gameplay_aliases_release_on_focus_loss() {
        let mut session = SessionState::default();
        control_event(&mut session, &space(true, false), false);
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        control_event(&mut session, &space(false, false), false);
        control_event(&mut session, &space(true, false), true);
        let poll = session.controls.poll(session.keyboard.held_keys(0));
        assert!(steering_from_mask(poll.mask, true).fire);
        assert!(!steering_from_mask(poll.mask, false).fire);
        control_event(
            &mut session,
            &Event::Window {
                timestamp: 0,
                window_id: 1,
                win_event: WindowEvent::FocusLost,
            },
            true,
        );
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        assert_eq!(scan_for_key(Keycode::B), Some(0x30));
        assert_eq!(scan_for_key(Keycode::Kp5), Some(0x4c));
        assert_eq!(scan_for_key(Keycode::Return), None);
    }

    #[test]
    fn monochrome_uses_each_raw_bit_while_color_preserves_two_bit_pixels() {
        let mut packed = vec![0; crate::frame::FRAME_BYTES];
        packed[0] = 0x81;
        packed[crate::frame::ROW_BYTES] = 0x40;
        let frame = Frame::from_packed(&packed).unwrap();
        let mono = display_rgb(&frame, DisplayMode::Monochrome, 1);
        assert_eq!(mono.len(), 640 * 200 * 3);
        for x in 0..8 {
            assert_eq!(
                &mono[x * 3..x * 3 + 3],
                &[if x == 0 || x == 7 { 255 } else { 0 }; 3]
            );
        }
        assert_eq!(&mono[(640 + 1) * 3..(640 + 2) * 3], &[255; 3]);
        let color = display_rgb(&frame, DisplayMode::Color, 1);
        let source = frame.rgb();
        for (index, pixel) in source.chunks_exact(3).enumerate() {
            assert_eq!(&color[index * 6..index * 6 + 3], pixel);
            assert_eq!(&color[index * 6 + 3..index * 6 + 6], pixel);
        }
    }

    #[test]
    fn color_palette_families_use_the_original_high_intensity_colors() {
        let mut packed = vec![0; crate::frame::FRAME_BYTES];
        packed[0] = 0x1b; // All four two-bit indices in one byte.
        let frame = Frame::from_packed(&packed).unwrap();
        let green = display_rgb(&frame, DisplayMode::Color, 0);
        let cyan = display_rgb(&frame, DisplayMode::Color, 1);
        for (index, (family0, family1)) in [
            ([0, 0, 0], [0, 0, 0]),
            ([85, 255, 85], [85, 255, 255]),
            ([255, 85, 85], [255, 85, 255]),
            ([255, 255, 85], [255, 255, 255]),
        ]
        .into_iter()
        .enumerate()
        {
            for duplicate in 0..2 {
                let offset = index * 6 + duplicate * 3;
                assert_eq!(green[offset..offset + 3], family0);
                assert_eq!(cyan[offset..offset + 3], family1);
            }
        }
        assert_eq!(
            display_rgb(&frame, DisplayMode::Monochrome, 0),
            display_rgb(&frame, DisplayMode::Monochrome, 1)
        );
    }

    #[test]
    fn transition_queue_displays_each_frame_for_its_wait_and_then_releases_simulation() {
        let now = std::time::Instant::now();
        let unit = std::time::Duration::from_millis(10);
        let frame = Frame::from_packed(&vec![0; crate::frame::FRAME_BYTES]).unwrap();
        let mut playback = TransitionPlayback::default();
        playback.append(
            [2, 5]
                .into_iter()
                .map(|wait_count| TransitionFrame {
                    frame: frame.clone(),
                    clip: crate::frame::Rect {
                        x_bytes: 0,
                        y: 0,
                        width_bytes: 1,
                        height: 1,
                    },
                    wait_count,
                })
                .collect(),
            now,
            unit,
        );
        assert!(playback.active());
        assert!(!playback.advance(now + unit, unit));
        assert!(playback.advance(now + unit * 2, unit));
        assert_eq!(playback.frames.front().unwrap().wait_count, 5);
        assert!(!playback.advance(now + unit * 6, unit));
        assert!(playback.advance(now + unit * 7, unit));
        assert!(!playback.active());
        assert_eq!(playback.deadline, None);
    }

    #[test]
    fn software_texture_corrects_cga_aspect_and_letterboxes() {
        // A software surface exercises the renderer without opening a desktop window.
        let surface = Surface::new(800, 600, PixelFormat::RGB24).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        let creator = canvas.texture_creator();
        let mut texture = creator
            .create_texture_streaming(PixelFormat::RGB24, 320, 200)
            .unwrap();
        texture.set_scale_mode(ScaleMode::Nearest);
        let mut rgb = vec![0; WIDTH * HEIGHT * 3];
        for pixel in rgb.chunks_exact_mut(3) {
            pixel.copy_from_slice(&[85, 255, 255]);
        }
        rgb[..3].copy_from_slice(&[255, 85, 255]);
        rgb[WIDTH * HEIGHT * 3 - 3..].copy_from_slice(&[255, 255, 255]);
        texture.update(None, &rgb, WIDTH * 3).unwrap();
        paint(&mut canvas, &texture).unwrap();
        let readback = canvas
            .read_pixels(None)
            .unwrap()
            .convert_format(PixelFormat::RGB24)
            .unwrap();
        let actual = readback.with_lock(|bytes| bytes.to_vec());
        let mut expected = vec![0; 800 * 600 * 3];
        // A 640x480 image fits, centered inside 800x600. Source pixels are
        // 20% taller than wide, sampled at destination pixel centers.
        for y in 0..480 {
            for x in 0..640 {
                let source_y = (2 * y + 1) * HEIGHT / (2 * 480);
                let source = (source_y * WIDTH + x / 2) * 3;
                let destination = ((y + 60) * 800 + x + 80) * 3;
                expected[destination..destination + 3].copy_from_slice(&rgb[source..source + 3]);
            }
        }
        assert_eq!(actual.len(), expected.len());
        let first_difference = actual.iter().zip(&expected).position(|(a, b)| a != b);
        assert!(
            first_difference.is_none(),
            "rendered RGB differs at byte {first_difference:?}"
        );
    }
}
