// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Host presentation diagnostics only; never advances an engine's clock.

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod, Scancode},
    pixels::Color,
    render::{BlendMode, FRect, WindowCanvas},
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct FpsOverlay {
    visible: bool,
    started: Option<Instant>,
    intervals: u32,
    rate: Option<f64>,
}

impl FpsOverlay {
    /// Consume both F9 edges before menus or guest input. Request a normal
    /// redraw on toggling, including when an engine is paused or showing a still.
    pub(super) fn filter_event(&mut self, event: Event, id: u32) -> Option<Event> {
        let (window_id, scancode, keycode, keymod) = match &event {
            Event::KeyDown {
                window_id,
                scancode,
                keycode,
                keymod,
                ..
            }
            | Event::KeyUp {
                window_id,
                scancode,
                keycode,
                keymod,
                ..
            } => (*window_id, *scancode, *keycode, *keymod),
            _ => return Some(event),
        };
        let desktop = Mod::LCTRLMOD
            | Mod::RCTRLMOD
            | Mod::LALTMOD
            | Mod::RALTMOD
            | Mod::LGUIMOD
            | Mod::RGUIMOD;
        if window_id != id
            || (scancode != Some(Scancode::F9) && keycode != Some(Keycode::F9))
            || keymod.intersects(desktop)
        {
            return Some(event);
        }
        if matches!(event, Event::KeyDown { repeat: false, .. }) {
            *self = Self {
                visible: !self.visible,
                ..Self::default()
            };
            Some(Event::Window {
                timestamp: event.get_timestamp(),
                window_id: id,
                win_event: WindowEvent::Exposed,
            })
        } else {
            None
        }
    }

    /// Count completed public presentations, excluding GPU resize settlement.
    /// The first presentation starts the measurement; each subsequent one
    /// completes an interval. The displayed average updates twice per second.
    pub(super) fn presented(&mut self, now: Instant) {
        if !self.visible {
            return;
        }
        let Some(started) = self.started else {
            self.started = Some(now);
            return;
        };
        self.intervals += 1;
        let elapsed = now.duration_since(started);
        if elapsed >= Duration::from_millis(500) {
            self.rate = Some(f64::from(self.intervals) / elapsed.as_secs_f64());
            self.started = Some(now);
            self.intervals = 0;
        }
    }

    pub(super) fn paint(&self, canvas: &mut WindowCanvas) -> Result<(), String> {
        if !self.visible {
            return Ok(());
        }
        let logical = canvas.logical_size();
        // Retain automatic viewport sizing across live aspect/renderer changes.
        // SAFETY: canvas owns a live renderer on its main thread.
        let viewport = unsafe { sdl3::sys::render::SDL_RenderViewportSet(canvas.raw()) }
            .then(|| canvas.viewport());
        let clip = canvas.clip_rect();
        let scale = canvas.scale();
        let color = canvas.draw_color();
        let blend = canvas.blend_mode();
        let result = (|| {
            canvas
                .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
                .map_err(|e| e.to_string())?;
            canvas.set_viewport(None);
            canvas.set_clip_rect(None);
            // SDL's tiny built-in debug font avoids any egui tessellation. Scale
            // with desktop density, independently of the game's pixel scaling.
            let density = canvas.window().display_scale();
            let density = if density.is_finite() && density > 0.0 {
                density
            } else {
                1.0
            };
            canvas
                .set_scale(density, density)
                .map_err(|e| e.to_string())?;
            let text = self
                .rate
                .map_or_else(|| "FPS: --".into(), |fps| format!("FPS: {fps:.1}"));
            let width = text.len() as f32 * 8.0 + 8.0;
            let x =
                (canvas.output_size().map_err(|e| e.to_string())?.0 as f32 / density - width - 4.0)
                    .max(0.0);
            canvas.set_blend_mode(BlendMode::Blend);
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 210));
            canvas
                .fill_rect(FRect::new(x, 4.0, width, 16.0))
                .map_err(|e| e.to_string())?;
            canvas.set_draw_color(Color::RGB(255, 255, 255));
            canvas
                .draw_debug_text(&text, (x + 4.0, 8.0))
                .map_err(|e| e.to_string())
        })();
        let restore = canvas
            .set_logical_size(logical.0, logical.1, logical.2)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                canvas
                    .set_scale(scale.0, scale.1)
                    .map_err(|e| e.to_string())
            });
        canvas.set_viewport(viewport);
        canvas.set_clip_rect(clip);
        canvas.set_draw_color(color);
        canvas.set_blend_mode(blend);
        result.and(restore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires SDL video (dummy works); run serially"]
    fn software_overlay_preserves_coordinates_and_handles_window_events() {
        check_window(reforge_settings::Renderer::Software);
    }

    #[test]
    #[ignore = "requires an SDL GPU-capable desktop; run serially"]
    fn gpu_overlay_preserves_coordinates_and_handles_window_events() {
        check_window(reforge_settings::Renderer::Gpu);
    }

    fn check_window(renderer: reforge_settings::Renderer) {
        use crate::{DisplayGeometry, FrameFormat, FrameTexture, Presentation, Session, Window};
        use sdl3::{pixels::PixelFormat, rect::Rect};
        let mut window = Window::build(
            "REforge FPS overlay check",
            DisplayGeometry::new((320, 200), (320, 240)),
            crate::settings::RuntimeSettings {
                renderer,
                scale: Some(2),
                vsync: false,
                ..Default::default()
            },
            Presentation::Pixels,
            true,
            Session::for_window(Some(2)),
            None,
        )
        .unwrap();
        let creator = window.texture_creator();
        let mut frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgba).unwrap();
        frame.update(&[0, 0, 0, 255].repeat(320 * 200)).unwrap();
        window.present(&frame).unwrap();
        assert!(window.fps.started.is_none());
        let mut event = press();
        if let Event::KeyDown { window_id, .. } = &mut event {
            *window_id = window.canvas.window().id();
        }
        let events = window._sdl.event().unwrap();
        events.push_event(event.clone()).unwrap();
        let received: Vec<_> = window.poll_events().collect();
        assert!(!received.iter().any(|e| matches!(e, Event::KeyDown { .. })));
        assert!(received.iter().any(|e| matches!(
            e,
            Event::Window {
                win_event: WindowEvent::Exposed,
                ..
            }
        )));
        assert!(window.fps.visible);

        // The overlay must neither pin an automatic viewport nor corrupt an
        // explicit engine overlay's drawing state.
        for explicit in [false, true] {
            let canvas = &mut window.canvas;
            canvas.set_viewport(explicit.then(|| Rect::new(2, 3, 310, 230)));
            canvas.set_clip_rect(Rect::new(5, 7, 300, 210));
            canvas.set_scale(1.5, 1.25).unwrap();
            canvas.set_draw_color(Color::RGBA(12, 34, 56, 78));
            canvas.set_blend_mode(BlendMode::Add);
            let before = (
                canvas.logical_size(),
                canvas.viewport(),
                canvas.clip_rect(),
                canvas.scale(),
                canvas.draw_color(),
                canvas.blend_mode(),
            );
            window.fps.paint(canvas).unwrap();
            assert_eq!(
                before,
                (
                    canvas.logical_size(),
                    canvas.viewport(),
                    canvas.clip_rect(),
                    canvas.scale(),
                    canvas.draw_color(),
                    canvas.blend_mode()
                )
            );
            // SAFETY: canvas owns this live renderer on its main thread.
            let viewport_set = unsafe { sdl3::sys::render::SDL_RenderViewportSet(canvas.raw()) };
            assert_eq!(viewport_set, explicit);
        }
        let canvas = &mut window.canvas;
        canvas
            .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
            .unwrap();
        canvas.set_scale(1.0, 1.0).unwrap();
        canvas.set_viewport(None);
        canvas.set_clip_rect(None);
        let image = canvas
            .read_pixels(None)
            .unwrap()
            .convert_format(PixelFormat::RGB24)
            .unwrap();
        image.with_lock(|pixels| {
            assert!(
                pixels.chunks_exact(3).any(|rgb| rgb == [255; 3]),
                "FPS text must be visible"
            )
        });
        if let Some(path) = std::env::var_os("REFORGE_FPS_TEST_IMAGE") {
            image.save_bmp(path).unwrap();
        }
        crate::configure(canvas, 320, 240, crate::ScalingPolicy::Integer).unwrap();
        for _ in 0..5 {
            window.present(&frame).unwrap();
        }
        assert!(window.fps.started.is_some());
        let mut menu = crate::settings::SettingsMenu::new(
            &creator,
            crate::settings::VideoCapabilities::default(),
        );
        let mut settings_key = press();
        if let Event::KeyDown {
            scancode, keycode, ..
        } = &mut settings_key
        {
            *scancode = Some(Scancode::F10);
            *keycode = Some(Keycode::F10);
        }
        assert!(menu.handle_event(&settings_key));
        menu.prepare(window.ui_metrics(), window.settings());
        window
            .present_with(&frame, |canvas| menu.paint(canvas))
            .unwrap();
        // Force renderer replacement even on dummy by changing the request.
        // Software -> Auto selects software there; GPU -> Software works live.
        let mut changed = window.settings();
        changed.renderer = if renderer == reforge_settings::Renderer::Gpu {
            reforge_settings::Renderer::Software
        } else {
            reforge_settings::Renderer::Auto
        };
        window.apply_settings(changed).unwrap();
        assert!(window.fps.visible);
        window.present(&frame).unwrap();
        // Renderer replacement changes the native window ID.
        if let Event::KeyDown { window_id, .. } = &mut event {
            *window_id = window.canvas.window().id();
        }
        events.push_event(event).unwrap();
        window.poll_events().for_each(drop);
        assert!(!window.fps.visible);
        window.present(&frame).unwrap();
    }

    fn press() -> Event {
        Event::KeyDown {
            timestamp: 123,
            window_id: 1,
            which: 0,
            raw: 0,
            keycode: Some(Keycode::F9),
            scancode: Some(Scancode::F9),
            keymod: Mod::NOMOD,
            repeat: false,
        }
    }

    #[test]
    fn shortcut_consumes_edges_and_repeats_without_guest_input() {
        let mut fps = FpsOverlay::default();
        assert!(!fps.visible);
        assert!(matches!(
            fps.filter_event(press(), 1),
            Some(Event::Window {
                timestamp: 123,
                window_id: 1,
                win_event: WindowEvent::Exposed,
            })
        ));
        let mut repeat = press();
        if let Event::KeyDown { repeat, .. } = &mut repeat {
            *repeat = true;
        }
        assert!(fps.filter_event(repeat, 1).is_none());
        let release = Event::KeyUp {
            timestamp: 124,
            window_id: 1,
            which: 0,
            raw: 0,
            keycode: Some(Keycode::F9),
            scancode: Some(Scancode::F9),
            keymod: Mod::NOMOD,
            repeat: false,
        };
        assert!(fps.filter_event(release, 1).is_none());
        assert!(fps.visible);
        for modifier in [
            Mod::LCTRLMOD,
            Mod::RCTRLMOD,
            Mod::LALTMOD,
            Mod::RALTMOD,
            Mod::LGUIMOD,
            Mod::RGUIMOD,
        ] {
            let mut event = press();
            if let Event::KeyDown { keymod, .. } = &mut event {
                *keymod = modifier;
            }
            assert!(matches!(
                fps.filter_event(event, 1),
                Some(Event::KeyDown { .. })
            ));
            assert!(fps.visible);
        }
        assert!(matches!(
            fps.filter_event(press(), 2),
            Some(Event::KeyDown { .. })
        ));
        assert!(matches!(
            fps.filter_event(Event::Quit { timestamp: 0 }, 1),
            Some(Event::Quit { .. })
        ));
        fps.filter_event(press(), 1);
        assert!(!fps.visible);
    }

    #[test]
    fn rate_measures_completed_intervals_and_includes_stalls() {
        let mut fps = FpsOverlay::default();
        let start = Instant::now();
        fps.presented(start);
        assert!(fps.started.is_none());
        fps.filter_event(press(), 1);
        fps.presented(start);
        for n in 1..=30 {
            fps.presented(start + Duration::from_secs_f64(f64::from(n) / 60.0));
        }
        assert_eq!(fps.rate, Some(60.0));
        fps.presented(start + Duration::from_secs(1));
        assert_eq!(fps.rate, Some(2.0));
        fps.filter_event(press(), 1);
        fps.filter_event(press(), 1);
        assert!(fps.rate.is_none());
        assert!(fps.started.is_none());
    }
}
