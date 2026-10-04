// Copyright (c) 2026 analogued. Project license has not been selected.
//! SDL2 presentation of the three verified startup frames.

use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    render::{Canvas, RenderTarget, ScaleMode, Texture},
};

use crate::{
    frame::{HEIGHT, WIDTH},
    startup::{Presentation, Screen, StartupFrames},
};

fn title(screen: Screen) -> &'static str {
    match screen {
        Screen::Splash => "Mach 3 — Splash (Space to advance)",
        Screen::Intro => "Mach 3 — Intro (Space to advance)",
        Screen::InitialScene => "Mach 3 — Initial scene (Esc to close)",
    }
}

/// Returns false when the application should close.
fn handle_event(presentation: &mut Presentation, event: Event) -> bool {
    match event {
        Event::Quit { .. }
        | Event::Window {
            win_event: WindowEvent::Close,
            ..
        }
        | Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return false,
        Event::KeyDown {
            keycode: Some(Keycode::Space),
            repeat,
            ..
        } => {
            presentation.space_down(repeat);
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
    true
}

fn destination(width: u32, height: u32) -> Option<sdl2::rect::Rect> {
    let scale = (width / WIDTH as u32).min(height / HEIGHT as u32);
    if scale == 0 {
        return None;
    }
    let scaled_width = WIDTH as u32 * scale;
    let scaled_height = HEIGHT as u32 * scale;
    Some(sdl2::rect::Rect::new(
        ((width - scaled_width) / 2) as i32,
        ((height - scaled_height) / 2) as i32,
        scaled_width,
        scaled_height,
    ))
}

fn paint<T: RenderTarget>(canvas: &mut Canvas<T>, texture: &Texture<'_>) -> Result<(), String> {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    let (width, height) = canvas.output_size()?;
    if let Some(rect) = destination(width, height) {
        canvas.copy(texture, None, rect)?;
    }
    Ok(())
}

/// Open a resizable nearest-neighbor viewer. No audio subsystem is initialized.
pub fn run(frames: &StartupFrames, scale: u32) -> Result<(), String> {
    if !(1..=6).contains(&scale) {
        return Err("window scale must be between 1 and 6".into());
    }
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
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
    let mut canvas = window
        .into_canvas()
        .software()
        .build()
        .map_err(|error| error.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGB24, WIDTH as u32, HEIGHT as u32)
        .map_err(|error| error.to_string())?;
    texture.set_scale_mode(ScaleMode::Nearest);
    let mut presentation = Presentation::default();
    let mut shown = presentation.screen();
    texture
        .update(None, &frames.frame(shown).rgb(), WIDTH * 3)
        .map_err(|error| error.to_string())?;
    paint(&mut canvas, &texture)?;
    canvas.present();
    let mut events = sdl.event_pump()?;
    loop {
        for event in events.poll_iter() {
            if !handle_event(&mut presentation, event) {
                return Ok(());
            }
        }
        if presentation.screen() != shown {
            shown = presentation.screen();
            texture
                .update(None, &frames.frame(shown).rgb(), WIDTH * 3)
                .map_err(|error| error.to_string())?;
            canvas
                .window_mut()
                .set_title(title(shown))
                .map_err(|error| error.to_string())?;
        }
        // Repaint also handles expose/resize events without changing screen state.
        paint(&mut canvas, &texture)?;
        canvas.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl2::{keyboard::Mod, surface::Surface};

    fn space(down: bool, repeat: bool) -> Event {
        if down {
            Event::KeyDown {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: 1,
                keycode: Some(Keycode::Space),
                scancode: None,
                keymod: Mod::NOMOD,
                repeat,
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
                win_event: WindowEvent::Close
            }
        ));
    }

    #[test]
    fn software_texture_preserves_pixels_and_letterboxes_integer_scale() {
        // A software surface exercises the renderer without opening a desktop window.
        let surface = Surface::new(800, 600, PixelFormatEnum::RGB24).unwrap();
        let mut canvas = surface.into_canvas().unwrap();
        let creator = canvas.texture_creator();
        let mut texture = creator
            .create_texture_streaming(PixelFormatEnum::RGB24, 320, 200)
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
        let actual = canvas.read_pixels(None, PixelFormatEnum::RGB24).unwrap();
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
