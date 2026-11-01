// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Temporary menu space without changing the player's display preferences.

use crate::settings::{RuntimeSettings, SettingsMenu, UiMetrics};
use crate::{Window, sync_window, window_size_for_pixels};
use sdl3::rect::Rect;
use sdl3::video::{FullscreenType, WindowPos};

const MINIMUM: (u32, u32) = (569, 420);
const COMFORTABLE: (u32, u32) = (630, 470);

#[derive(Clone, Copy)]
struct Geometry {
    size: (u32, u32),
    position: (i32, i32),
}

#[derive(Default)]
pub(super) struct State {
    restore: Option<Geometry>,
    last_view: Option<(UiMetrics, bool, bool)>,
}

impl Window {
    /// Prepare and apply the shared menu, temporarily growing undersized normal
    /// windows before layout. Closing restores geometry unless a new display
    /// preference supersedes it. Both engines use this host-side entry point.
    pub fn prepare_settings(&mut self, menu: &mut SettingsMenu<'_>) {
        let opening = menu.is_open() && menu.navigation.focus_first;
        let initial = self.ui_metrics();
        self.update_settings_window(menu);
        let before = (self.ui_metrics(), self.creator.generation());
        menu.set_status(self.renderer_status());
        if let Some(requested) = menu.prepare(before.0, self.settings()) {
            match self.apply_settings(requested) {
                Ok(()) => menu.applied(),
                Err(error) => {
                    eprintln!("REforge: could not apply settings: {error}");
                    menu.set_error(error);
                }
            }
        }
        self.update_settings_window(menu);
        let after = (self.ui_metrics(), self.creator.generation());
        if menu.is_open() && (opening || initial != before.0 || before != after) {
            // Paint only the layout for the accepted geometry/renderer. Do not
            // show one frame in the old position before settling in the centre.
            menu.set_status(self.renderer_status());
            // Wrapping and the anchored outer size use egui's previous layout.
            // Settle them before the first visible frame, rather than snapping
            // into place on subsequent presentations.
            for _ in 0..2 {
                let _ = menu.prepare(after.0, self.settings());
            }
        }
    }

    fn update_settings_window(&mut self, menu: &mut SettingsMenu<'_>) {
        let mut state = std::mem::take(&mut self.settings_window);
        if let Err(error) = state.sync(self, menu.is_open()) {
            eprintln!("REforge: cannot resize settings window: {error}");
            menu.set_error(error);
        }
        self.settings_window = state;
    }
}

impl State {
    pub(super) fn invalidate(&mut self) {
        self.last_view = None;
    }

    pub(super) fn applied(&mut self, window: &mut Window, before: &RuntimeSettings) {
        let after = window.settings();
        if after.scale != before.scale || after.fullscreen != before.fullscreen {
            if let Some(original) = self.restore.take() {
                if after.fullscreen && !before.fullscreen {
                    // Fullscreen must remember the actual game window, rather
                    // than the temporary menu size. A changed scale already
                    // sets windowed_resize_pending and takes precedence later.
                    window.windowed_geometry = Some(original.size);
                }
            }
        }
    }

    fn sync(&mut self, display: &mut Window, open: bool) -> Result<(), String> {
        let view = (
            display.ui_metrics(),
            display.canvas.window().fullscreen_state() != FullscreenType::Off,
            display.canvas.window().is_maximized(),
        );
        if !open {
            self.last_view = None;
            if let Some(original) = self.restore.take() {
                if !view.1 && !view.2 {
                    let window = display.canvas.window_mut();
                    window
                        .set_size(original.size.0, original.size.1)
                        .map_err(|e| e.to_string())?;
                    // Best effort: Wayland leaves placement to the compositor.
                    window.set_position(
                        WindowPos::Positioned(original.position.0),
                        WindowPos::Positioned(original.position.1),
                    );
                    sync_window(window);
                }
            }
            return Ok(());
        }
        if self.last_view == Some(view) {
            return Ok(());
        }
        self.last_view = Some(view);
        if view.1 || view.2 {
            return Ok(());
        }
        let window = display.canvas.window();
        let Some(target) = required_size(view.0, window.pixel_density())? else {
            return Ok(());
        };
        let original = Geometry {
            size: window.size(),
            position: window.position(),
        };
        let bounds = window
            .get_display()
            .and_then(|d| d.get_usable_bounds())
            .ok();
        let (top, left, bottom, right) = window.border_size().unwrap_or_default();
        let available = bounds.map(|r| {
            (
                r.width()
                    .saturating_sub(u32::from(left) + u32::from(right))
                    .max(1),
                r.height()
                    .saturating_sub(u32::from(top) + u32::from(bottom))
                    .max(1),
            )
        });
        let size = bounded_size(original.size, target, available);
        if size == original.size {
            return Ok(());
        }
        // A second growth (for example after moving to a denser monitor) must
        // retain the geometry from before this settings session's first growth.
        self.restore.get_or_insert(original);
        let window = display.canvas.window_mut();
        window.set_size(size.0, size.1).map_err(|e| e.to_string())?;
        let position = centred_position(original, size, bounds);
        window.set_position(
            WindowPos::Positioned(position.0),
            WindowPos::Positioned(position.1),
        );
        sync_window(window);
        // A compositor can constrain or refuse sizing. Do not fight it every
        // frame; reconsider when the measured size, density or mode changes.
        self.last_view = Some((display.ui_metrics(), false, false));
        Ok(())
    }
}

fn required_size(metrics: UiMetrics, pixel_density: f32) -> Result<Option<(u32, u32)>, String> {
    let density = metrics.density();
    // Match the rounded logical dimensions shown in the preview: 1280 / 2.25
    // is 568.89, displayed as 569, and must not trigger an x4 window resize.
    if (metrics.drawable.0 as f32 / density).round() >= MINIMUM.0 as f32
        && (metrics.drawable.1 as f32 / density).round() >= MINIMUM.1 as f32
    {
        return Ok(None);
    }
    // Convert UI units directly into SDL window coordinates. Rounding first to
    // physical pixels and then to window units would unnecessarily round twice.
    let target = window_size_for_pixels(COMFORTABLE, pixel_density / density)?;
    Ok(Some((
        metrics.window.0.max(target.0),
        metrics.window.1.max(target.1),
    )))
}

fn bounded_size(
    current: (u32, u32),
    target: (u32, u32),
    available: Option<(u32, u32)>,
) -> (u32, u32) {
    let maximum = available.unwrap_or((u32::MAX, u32::MAX));
    (
        current.0.max(target.0.min(maximum.0)),
        current.1.max(target.1.min(maximum.1)),
    )
}

fn centred_position(original: Geometry, size: (u32, u32), bounds: Option<Rect>) -> (i32, i32) {
    let dimension = |position: i32, old: u32, new: u32, range: Option<(i32, u32)>| {
        let centre = i64::from(position) + (i64::from(old) - i64::from(new)) / 2;
        let centre = if let Some((start, extent)) = range {
            centre.clamp(
                i64::from(start),
                i64::from(start) + i64::from(extent.saturating_sub(new)),
            )
        } else {
            centre
        };
        centre.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    };
    (
        dimension(
            original.position.0,
            original.size.0,
            size.0,
            bounds.map(|r| (r.x(), r.width())),
        ),
        dimension(
            original.position.1,
            original.size.1,
            size.1,
            bounds.map(|r| (r.y(), r.height())),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(pixels: (u32, u32), density: f32) -> UiMetrics {
        UiMetrics {
            drawable: pixels,
            window: pixels,
            pixels_per_point: density,
        }
    }

    // A pixel-density-one window has window coordinates equal to physical pixels.
    fn required_pixels(metrics: UiMetrics) -> Option<(u32, u32)> {
        required_size(metrics, 1.0).unwrap()
    }

    #[test]
    fn threshold_and_comfort_target_follow_logical_space() {
        for density in [1.0, 1.5, 2.0, 2.25] {
            let at = |w: u32, h: u32| {
                metrics(
                    (
                        (w as f32 * density).ceil() as u32,
                        (h as f32 * density).ceil() as u32,
                    ),
                    density,
                )
            };
            assert_eq!(required_pixels(at(569, 420)), None);
            assert_eq!(
                required_pixels(at(568, 420)),
                Some((
                    (630.0 * density).ceil() as u32,
                    (470.0 * density).ceil() as u32
                ))
            );
            assert!(required_pixels(at(569, 419)).is_some());
            assert!(required_pixels(at(1000, 419)).is_some());
        }
        for (density, adequate) in [
            (1.0, [false, true, true, true]),
            (1.5, [false, false, true, true]),
            (2.0, [false, false, false, true]),
            (2.25, [false, false, false, true]),
        ] {
            for (index, sufficient) in adequate.into_iter().enumerate() {
                let scale = index as u32 + 1;
                assert_eq!(
                    required_pixels(metrics((320 * scale, 240 * scale), density)).is_none(),
                    sufficient
                );
            }
        }
        assert_eq!(required_pixels(metrics((1280, 960), 2.25)), None);
        assert_eq!(
            required_pixels(metrics((960, 720), 2.25)),
            Some((1418, 1058))
        );
        assert_eq!(required_pixels(metrics((960, 720), 2.0)), Some((1260, 940)));
        assert_eq!(required_pixels(metrics((960, 720), 1.5)), None);
        assert_eq!(
            required_pixels(metrics((2000, 700), 2.25)),
            Some((2000, 1058))
        );
        assert_eq!(
            required_size(
                UiMetrics {
                    drawable: (322, 241),
                    window: (143, 107),
                    pixels_per_point: 2.25,
                },
                2.25
            )
            .unwrap(),
            Some((630, 470))
        );
    }

    #[test]
    fn available_area_limits_growth_without_shrinking_existing_space() {
        assert_eq!(
            bounded_size((320, 240), (630, 470), Some((600, 450))),
            (600, 450)
        );
        assert_eq!(
            bounded_size((800, 300), (800, 470), Some((600, 450))),
            (800, 450)
        );
        let original = Geometry {
            size: (320, 240),
            position: (200, 200),
        };
        assert_eq!(centred_position(original, (630, 470), None), (45, 85));
        assert_eq!(
            centred_position(original, (630, 470), Some(Rect::new(100, 100, 800, 600))),
            (100, 100)
        );
    }

    #[test]
    fn native_menu_growth_restores_and_respects_applied_geometry() {
        const NAME: &str =
            "settings_window::tests::native_menu_growth_restores_and_respects_applied_geometry";
        if std::env::var("REFORGE_MENU_WINDOW_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_MENU_WINDOW_TEST", NAME)
                .env("SDL_VIDEODRIVER", "dummy")
                .env("SDL_AUDIODRIVER", "dummy")
                .env("SDL_RENDER_DRIVER", "software")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use crate::settings::VideoCapabilities;
        use reforge_settings::{Renderer, Session};
        use sdl3::{event::Event, mouse::MouseButton};
        fn click(display: &mut Window, menu: &mut SettingsMenu<'_>, pos: egui::Pos2) {
            let m = display.ui_metrics();
            let x = pos.x * m.pixels_per_point * m.window.0 as f32 / m.drawable.0 as f32;
            let y = pos.y * m.pixels_per_point * m.window.1 as f32 / m.drawable.1 as f32;
            for pressed in [true, false] {
                let event = if pressed {
                    Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: 0,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                } else {
                    Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: 0,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                };
                menu.handle_event(&event);
                display.prepare_settings(menu);
            }
        }
        for game in ["alley-cat", "mach-3"] {
            let mut session = Session::for_window(Some(1));
            session.game_id = game.into();
            session.saved.edit(false, "renderer", "software".into());
            let requested = session.effective(&session.saved);
            let mut display = Window::build(
                "menu geometry test",
                crate::DisplayGeometry::new((320, 200), (320, 240)),
                requested,
                crate::Presentation::Pixels,
                true,
                session,
                None,
            )
            .unwrap();
            display
                .canvas
                .window_mut()
                .set_position(WindowPos::Positioned(100), WindowPos::Positioned(120));
            let original = Geometry {
                size: display.canvas.window().size(),
                position: display.canvas.window().position(),
            };
            let creator = display.texture_creator();
            let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
            menu = if game == "alley-cat" {
                menu.with_alleycat_options()
            } else {
                menu.with_mach3_options(None)
            };
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (630, 470));
            assert_eq!(display.settings().scale, Some(1));
            let rect = menu
                .context
                .memory(|m| m.area_rect(egui::Id::new("reforge_settings")).unwrap());
            assert!(
                (rect.center().x - 315.0).abs() < 1.0,
                "centre on first prepared frame: {rect:?}"
            );
            assert!(
                (rect.center().y - 235.0).abs() < 1.0,
                "centre on first prepared frame: {rect:?}"
            );
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(630.0, 470.0));
            assert!(screen.contains_rect(menu.cancel_rect.unwrap()));
            let grown = display.canvas.window().size();
            for page in 0..4 {
                menu.page = page;
                display.prepare_settings(&mut menu);
                assert_eq!(display.canvas.window().size(), grown);
            }
            menu.page = 0;
            display.prepare_settings(&mut menu);
            // Real Apply changes a non-geometry setting; it must not retain the
            // temporary window or lose its original geometry.
            menu.draft
                .as_mut()
                .unwrap()
                .edit(false, "vsync", "false".into());
            display.prepare_settings(&mut menu);
            let apply = menu.apply_rect.unwrap().center();
            click(&mut display, &mut menu, apply);
            assert!(!display.settings().vsync);
            assert_eq!(display.canvas.window().size(), grown);
            let cancel = menu.cancel_rect.unwrap().center();
            click(&mut display, &mut menu, cancel);
            assert!(!menu.is_open());
            assert_eq!(display.canvas.window().size(), original.size);
            assert_eq!(display.canvas.window().position(), original.position);
            // A renderer replacement keeps the lease; a rejected choice does too.
            menu.toggle();
            display.prepare_settings(&mut menu);
            display
                .apply_settings(RuntimeSettings {
                    renderer: Renderer::Sdl,
                    ..display.settings()
                })
                .unwrap();
            display.prepare_settings(&mut menu);
            let invalid = RuntimeSettings {
                backend: reforge_settings::Backend::Vulkan,
                ..display.settings()
            };
            assert!(display.apply_settings(invalid).is_err());
            let unsupported = RuntimeSettings {
                renderer: Renderer::Gpu,
                backend: reforge_settings::Backend::Vulkan,
                ..display.settings()
            };
            assert!(display.apply_settings(unsupported).is_err());
            display.prepare_settings(&mut menu);
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.canvas.window().size(), original.size);
            // A newly applied adequate scale supersedes the pre-menu geometry.
            menu.toggle();
            display.prepare_settings(&mut menu);
            display
                .apply_settings(RuntimeSettings {
                    scale: Some(2),
                    ..display.settings()
                })
                .unwrap();
            display.prepare_settings(&mut menu);
            assert_eq!(display.settings().scale, Some(2));
            assert_eq!(display.ui_metrics().drawable, (640, 480));
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (640, 480));
            // A new small preference supersedes a manually sized game window.
            display.canvas.window_mut().set_size(400, 300).unwrap();
            menu.toggle();
            display.prepare_settings(&mut menu);
            display
                .apply_settings(RuntimeSettings {
                    scale: Some(1),
                    ..display.settings()
                })
                .unwrap();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (630, 470));
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (320, 240));
            // Adequate manually sized windows are not touched.
            display.canvas.window_mut().set_size(569, 420).unwrap();
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (569, 420));
            menu.toggle();
            display.prepare_settings(&mut menu);
            assert_eq!(display.ui_metrics().drawable, (569, 420));
            // Fullscreen entered from a temporarily enlarged window must retain
            // the original windowed dimensions for a later return.
            display.canvas.window_mut().set_size(320, 240).unwrap();
            menu.toggle();
            display.prepare_settings(&mut menu);
            display
                .apply_settings(RuntimeSettings {
                    fullscreen: true,
                    ..display.settings()
                })
                .unwrap();
            display.prepare_settings(&mut menu);
            assert_eq!(display.windowed_geometry, Some((320, 240)));
            menu.toggle();
            display.prepare_settings(&mut menu);
            display
                .apply_settings(RuntimeSettings {
                    fullscreen: false,
                    ..display.settings()
                })
                .unwrap();
            assert_eq!(display.ui_metrics().drawable, (320, 240));
            // Repeated opening/closing must neither accumulate growth nor save
            // the temporary size as the selected window scale.
            for _ in 0..12 {
                menu.toggle();
                display.prepare_settings(&mut menu);
                assert_eq!(display.ui_metrics().drawable, (630, 470));
                menu.toggle();
                display.prepare_settings(&mut menu);
                assert_eq!(display.ui_metrics().drawable, (320, 240));
                assert_eq!(display.settings().scale, Some(1));
            }
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires a native desktop; opens synthetic windows"]
fn desktop_menu_geometry_smoke() {
    use crate::settings::VideoCapabilities;
    use reforge_settings::Session;
    for scale in 1..=4 {
        let mut session = Session::for_window(Some(scale));
        session.game_id = "alley-cat".into();
        session.saved.edit(false, "renderer", "software".into());
        let requested = session.effective(&session.saved);
        let mut display = Window::build(
            "REforge settings sizing test",
            crate::DisplayGeometry::new((320, 200), (320, 240)),
            requested,
            crate::Presentation::Pixels,
            true,
            session,
            None,
        )
        .unwrap();
        let original = display.ui_metrics();
        let should_grow = required_size(original, display.canvas.window().pixel_density())
            .unwrap()
            .is_some();
        let creator = display.texture_creator();
        let mut menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_alleycat_options();
        menu.toggle();
        display.prepare_settings(&mut menu);
        let opened = display.ui_metrics();
        if should_grow {
            assert!((opened.drawable.0 as f32 / opened.density()).round() >= COMFORTABLE.0 as f32);
            assert!((opened.drawable.1 as f32 / opened.density()).round() >= COMFORTABLE.1 as f32);
        } else {
            assert_eq!(opened.window, original.window);
        }
        let mut frame =
            crate::FrameTexture::new(&creator, 320, 200, crate::FrameFormat::Rgba).unwrap();
        frame.update(&[65, 75, 85, 255].repeat(320 * 200)).unwrap();
        display
            .present_with(&frame, |canvas| menu.paint(canvas))
            .unwrap();
        menu.toggle();
        display.prepare_settings(&mut menu);
        let restored = display.ui_metrics();
        assert_eq!(restored.window, original.window);
        println!(
            "scale={scale} original={original:?} grew={should_grow} opened={opened:?} restored={restored:?}"
        );
    }
}
