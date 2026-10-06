// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL3 presentation of startup screens and the bounded opening animation.

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    pixels::{Color, PixelFormat},
    render::{Canvas, FRect, RenderTarget, ScaleMode, Texture},
};

use crate::{
    frame::{HEIGHT, WIDTH},
    opening::{BODY_PERIOD, Ending, Opening},
    startup::{Presentation, Screen, StartupFrames},
};

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
    pub body_period: std::time::Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scale: 3,
            playback: Playback::Animated,
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
    let scale = (width / WIDTH as u32).min(height / HEIGHT as u32);
    if scale == 0 {
        return None;
    }
    let scaled_width = WIDTH as u32 * scale;
    let scaled_height = HEIGHT as u32 * scale;
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
    let scale = options.scale;
    if !(1..=6).contains(&scale) {
        return Err("window scale must be between 1 and 6".into());
    }
    if options.body_period.is_zero() || options.body_period > std::time::Duration::from_secs(5) {
        return Err("opening period must be greater than zero and at most five seconds".into());
    }
    let mut opening = if options.playback == Playback::Static {
        None
    } else {
        Some(Opening::new(frames.graphics()).map_err(|error| error.to_string())?)
    };
    let mut next_body = None;
    let mut ending: Option<Ending> = None;
    let mut returned_intro = None;
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let mut window = video
        .window(
            title(Screen::Splash),
            WIDTH as u32 * scale,
            HEIGHT as u32 * scale,
        )
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    window
        .set_minimum_size(WIDTH as u32, HEIGHT as u32)
        .map_err(|error| error.to_string())?;
    let mut canvas = sdl3::render::create_renderer(window, Some(c"software"))
        .map_err(|error| error.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormat::RGB24, WIDTH as u32, HEIGHT as u32)
        .map_err(|error| error.to_string())?;
    texture.set_scale_mode(ScaleMode::Nearest);
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    texture
        .update(None, &frames.frame(shown).rgb(), WIDTH * 3)
        .map_err(|error| error.to_string())?;
    paint(&mut canvas, &texture)?;
    if !canvas.present() {
        return Err(sdl3::get_error().to_string());
    }
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    loop {
        let mut step = false;
        for event in events.poll_iter() {
            let was_scene = presentation.screen() == Screen::InitialScene;
            let action = event_action(&mut presentation, event);
            if action == Action::Close {
                return Ok(());
            }
            if action == Action::Space {
                if returned_intro.is_some() {
                    presentation.return_to_intro();
                } else if was_scene && ending.as_ref().is_some_and(Ending::can_return) {
                    returned_intro = ending.as_mut().unwrap().returned_intro_choice();
                    presentation.return_to_intro();
                    next_body = None;
                } else {
                    step |= was_scene && options.playback == Playback::Step;
                }
            }
        }
        if presentation.screen() != shown {
            shown = presentation.screen();
            texture
                .update(
                    None,
                    &returned_intro
                        .map_or_else(|| frames.frame(shown), |choice| frames.return_intro(choice))
                        .rgb(),
                    WIDTH * 3,
                )
                .map_err(|error| error.to_string())?;
            canvas
                .window_mut()
                .set_title(title(shown))
                .map_err(|error| error.to_string())?;
            if returned_intro.is_some() {
                canvas
                    .window_mut()
                    .set_title("Mach 3 — Return intro (Esc to close)")
                    .map_err(|error| error.to_string())?;
            }
            if shown == Screen::InitialScene && options.playback != Playback::Static {
                next_body = Some(std::time::Instant::now() + options.body_period);
                canvas
                    .window_mut()
                    .set_title(if options.playback == Playback::Step {
                        "Mach 3 — Opening (Space to step, Esc to close)"
                    } else {
                        "Mach 3 — Opening preview (Esc to close)"
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        if shown == Screen::InitialScene {
            let now = std::time::Instant::now();
            let timed = options.playback == Playback::Animated
                && next_body.is_some_and(|deadline| now >= deadline);
            if step || timed {
                if let Some(opening) = &mut opening {
                    if let Some(ending) = &mut ending {
                        ending
                            .advance(frames.graphics())
                            .map_err(|error| error.to_string())?;
                        texture
                            .update(None, &ending.frame().rgb(), WIDTH * 3)
                            .map_err(|error| error.to_string())?;
                    } else if opening.complete() && opening.snapshot().player.game_over {
                        let state =
                            Ending::new(frames.graphics(), &opening.snapshot(), opening.frame())
                                .map_err(|error| error.to_string())?;
                        texture
                            .update(None, &state.frame().rgb(), WIDTH * 3)
                            .map_err(|error| error.to_string())?;
                        ending = Some(state);
                    } else if opening
                        .advance(frames.graphics())
                        .map_err(|error| error.to_string())?
                    {
                        texture
                            .update(None, &opening.frame().rgb(), WIDTH * 3)
                            .map_err(|error| error.to_string())?;
                    }
                    let period = if let Some(ending) = &ending {
                        options.body_period * ending.wait_units() / 3
                    } else {
                        options.body_period
                    };
                    next_body = if timed {
                        next_body.map(|deadline| deadline + period)
                    } else {
                        Some(now + period)
                    };
                    if let Some(ending) = &ending {
                        canvas
                            .window_mut()
                            .set_title(if ending.can_return() {
                                "Mach 3 — Game Over / High Score (Space to return, Esc to close)"
                            } else if options.playback == Playback::Step {
                                "Mach 3 — Game Over / High Score (Space to step, Esc to close)"
                            } else {
                                "Mach 3 — Game Over / High Score (Esc to close)"
                            })
                            .map_err(|error| error.to_string())?;
                    } else if opening.complete() {
                        canvas
                            .window_mut()
                            .set_title("Mach 3 — Final gameplay frame (Esc to close)")
                            .map_err(|error| error.to_string())?;
                        if !opening.snapshot().player.game_over {
                            next_body = None;
                        }
                    }
                }
            }
        }
        // Repaint also handles expose/resize events without changing screen state.
        paint(&mut canvas, &texture)?;
        if !canvas.present() {
            return Err(sdl3::get_error().to_string());
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
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
    fn software_texture_preserves_pixels_and_letterboxes_integer_scale() {
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
        // Integer scale 2 fits, centered inside 800x600 with black borders.
        for y in 0..400 {
            for x in 0..640 {
                let source = ((y / 2) * WIDTH + x / 2) * 3;
                let destination = ((y + 100) * 800 + x + 80) * 3;
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
