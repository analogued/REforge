// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Standalone Compact host UI using the settings painter and input adapter.

use super::*;

/// A host surface that survives renderer replacement. It shares the settings
/// menu's painter, texture ownership, DPI conversion and navigation without
/// opening preferences.
pub struct Gui<'a> {
    surface: SettingsMenu<'a>,
    focus_overlay: bool,
    text_input: bool,
}
impl<'a> Gui<'a> {
    pub fn new(creator: &'a FrameCreator) -> Self {
        let mut surface = SettingsMenu::new(creator, VideoCapabilities::default());
        surface.open = true;
        Self {
            surface,
            focus_overlay: true,
            text_input: false,
        }
    }
    /// Disable the additional rectangular focus outline when the screen's
    /// widgets already paint their own themed keyboard/controller focus.
    /// Focus registration, navigation and activation remain enabled.
    pub fn set_focus_overlay(&mut self, enabled: bool) {
        self.focus_overlay = enabled;
    }
    /// Select an appearance programmatically; palette changes preserve input
    /// state, while shared primitives read this theme's component composition.
    pub fn set_theme(&mut self, theme: crate::ui::Theme) {
        self.surface.set_theme(theme);
    }
    pub fn theme(&self) -> &crate::ui::Theme {
        self.surface.theme()
    }
    pub fn metrics(canvas: &WindowCanvas) -> Result<UiMetrics, String> {
        Ok(UiMetrics {
            drawable: canvas.output_size().map_err(|e| e.to_string())?,
            window: canvas.window().size(),
            pixels_per_point: canvas.window().display_scale(),
        })
    }
    pub fn handle_event(&mut self, event: &Event) {
        let mode = match event {
            Event::MouseButtonDown { .. } | Event::MouseWheel { .. } => Some(InputMode::Mouse),
            Event::KeyDown { .. } => Some(InputMode::Keyboard),
            _ => None,
        };
        if let Some(mode) = mode {
            self.surface
                .navigation
                .set_mode(&self.surface.context, mode);
        }
        match event {
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.surface.focused = false;
                self.surface.modifiers = Default::default();
                let mode = self.surface.navigation.mode;
                self.surface.navigation = Default::default();
                self.surface
                    .navigation
                    .set_mode(&self.surface.context, mode);
            }
            Event::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => self.surface.focused = true,
            _ => {}
        }
        self.surface.events.push(MenuEvent::Sdl(event.clone()));
    }
    pub fn handle_gamepad(&mut self, pad: crate::gamepad::PadEvent) {
        let s = &mut self.surface;
        if s.navigation
            .controller(pad, s.started.elapsed().as_secs_f64())
        {
            s.navigation.set_mode(&s.context, InputMode::Controller);
        }
    }
    /// Returning from a native dialog/game must not replay hidden-window input.
    pub fn reset_input(&mut self) {
        self.text_input = false;
        let s = &mut self.surface;
        s.events.clear();
        s.modifiers = Default::default();
        let mode = s.navigation.mode;
        s.navigation = Default::default();
        s.navigation.set_mode(&s.context, mode);
        s.events
            .push(MenuEvent::Ui(egui::Event::WindowFocused(false)));
        s.events
            .push(MenuEvent::Ui(egui::Event::WindowFocused(true)));
    }
    /// Register a launcher button/list row for the same navigation as settings.
    pub fn control(response: egui::Response) -> egui::Response {
        crate::settings_navigation::control(response)
    }
    /// Register a text field while retaining Left/Right for caret movement and
    /// selection. Tab, vertical arrows and controller shoulders navigate away.
    pub fn text_edit(response: egui::Response) -> egui::Response {
        crate::settings_navigation::value_control(
            response,
            crate::settings_navigation::Kind::TextEdit,
        )
    }
    /// Whether a focused text editor needs native composed text events.
    /// Hosts must suspend text input while a child window or dialog owns input.
    pub fn wants_text_input(&self) -> bool {
        self.text_input
    }
    pub fn prepare(&mut self, metrics: UiMetrics, mut draw: impl FnMut(&egui::Context)) {
        let s = &mut self.surface;
        s.recover_renderer();
        s.density = metrics.density();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(
                    metrics.drawable.0 as f32 / s.density,
                    metrics.drawable.1 as f32 / s.density,
                ),
            )),
            time: Some(s.started.elapsed().as_secs_f64()),
            focused: s.focused,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(s.density);
        for event in s.events.drain(..) {
            match event {
                MenuEvent::Sdl(event) => {
                    translate_event(&event, metrics, &mut s.modifiers, &mut input.events)
                }
                MenuEvent::Ui(event) => input.events.push(event),
            }
        }
        s.navigation
            .drain(s.started.elapsed().as_secs_f64(), &mut input.events);
        s.navigation.navigate(&s.context, &mut input.events);
        input.modifiers = s.modifiers;
        let output = s.context.run(input, |context| {
            crate::settings_navigation::begin_frame(context);
            draw(context);
            if self.focus_overlay {
                crate::settings_navigation::paint_focus(context);
            }
        });
        self.text_input = s.focused && output.platform_output.ime.is_some();
        s.pending_textures.append(output.textures_delta);
        s.density = output.pixels_per_point;
        s.jobs = s.context.tessellate(output.shapes, output.pixels_per_point);
    }
    pub fn paint(&mut self, canvas: &mut WindowCanvas) -> Result<(), String> {
        self.surface.paint(canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_recovery_retains_text_and_artwork() {
        const NAME: &str = "settings::gui::tests::renderer_recovery_retains_text_and_artwork";
        if std::env::var("REFORGE_GUI_RECOVERY_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_GUI_RECOVERY_TEST", NAME)
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
        use crate::{FrameFormat, FrameTexture, Window};
        use reforge_settings::{Backend, Renderer};
        let _guard = reforge_settings::Launch {
            game_id: "standalone".into(),
            entry_id: None,
            path: None,
            cli: reforge_settings::Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ]),
        }
        .install();
        // Include a surface that has never been prepared or painted before
        // replacement, as well as one with an atlas, artwork and queued updates.
        for painted in [false, true] {
            let mut display = Window::new("GUI recovery", 320, 240, Some(3)).unwrap();
            display.start_text_input();
            let creator = display.texture_creator();
            let mut frame = FrameTexture::new(&creator, 320, 240, FrameFormat::Rgb).unwrap();
            frame.update(&vec![0; 320 * 240 * 3]).unwrap();
            let mut gui = Gui::new(&creator);
            let mut artwork = gui.surface.context.load_texture(
                "retained artwork",
                egui::ColorImage::new([8, 8], Color32::GREEN),
                egui::TextureOptions::NEAREST,
            );
            let mut filename = String::from("BOOKMARK.SAV");
            let field = egui::Id::new("retained filename");
            let artwork_id = artwork.id();
            let draw = |ctx: &egui::Context, filename: &mut String| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(100, 0, 0)))
                    .show(ctx, |ui| {
                        ui.label(egui::RichText::new("Save bookmark").color(Color32::WHITE));
                        Gui::text_edit(ui.add(egui::TextEdit::singleline(filename).id(field)));
                        ui.image((artwork_id, egui::vec2(64.0, 64.0)));
                    });
            };
            if painted {
                for _ in 0..3 {
                    gui.prepare(display.ui_metrics(), |ctx| draw(ctx, &mut filename));
                    display
                        .present_with(&frame, |canvas| gui.paint(canvas))
                        .unwrap();
                }
                gui.surface
                    .context
                    .memory_mut(|memory| memory.request_focus(field));
            }
            artwork.set_partial(
                [2, 2],
                egui::ColorImage::new([2, 2], Color32::BLUE),
                egui::TextureOptions::NEAREST,
            );
            if painted {
                // An update prepared but not uploaded must survive too.
                gui.prepare(display.ui_metrics(), |ctx| draw(ctx, &mut filename));
            }
            for renderer in [Renderer::Sdl, Renderer::Auto, Renderer::Gpu] {
                let before = display.settings();
                let generation = creator.generation();
                let request = RuntimeSettings {
                    renderer,
                    backend: if renderer == Renderer::Gpu {
                        Backend::OpenGl
                    } else {
                        Backend::Auto
                    },
                    ..before.clone()
                };
                let result = display.apply_settings(request);
                if renderer == Renderer::Gpu {
                    assert!(
                        result.is_err(),
                        "dummy driver must reject GPU and roll back"
                    );
                    assert_eq!(display.settings(), before);
                } else {
                    result.unwrap();
                }
                assert!(creator.generation() > generation);
                assert!(
                    display
                        ._video
                        .text_input()
                        .is_active(display.canvas.window())
                );
                gui.prepare(display.ui_metrics(), |ctx| draw(ctx, &mut filename));
                display
                    .present_with(&frame, |canvas| {
                        gui.paint(canvas)?;
                        let image = canvas
                            .read_pixels(None)
                            .map_err(|e| e.to_string())?
                            .convert_format(PixelFormat::RGB24)
                            .map_err(|e| e.to_string())?;
                        image.with_lock(|bytes| {
                            for color in [[100, 0, 0], [0, 255, 0], [0, 0, 255]] {
                                assert!(
                                    bytes
                                        .chunks_exact(3)
                                        .filter(|pixel| *pixel == color)
                                        .count()
                                        > 20,
                                    "missing panel/artwork/text color {color:?} after {renderer:?}"
                                );
                            }
                            assert!(
                                bytes
                                    .chunks_exact(3)
                                    .filter(|pixel| pixel.iter().all(|value| *value > 160))
                                    .count()
                                    > 20,
                                "missing text after {renderer:?}"
                            );
                        });
                        Ok(())
                    })
                    .unwrap();
                assert!(
                    gui.surface
                        .textures
                        .values()
                        .all(|texture| texture.belongs_to(&creator))
                );
                assert_eq!(filename, "BOOKMARK.SAV");
                if painted {
                    assert_eq!(
                        gui.surface.context.memory(|memory| memory.focused()),
                        Some(field)
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "owns SDL video; run serially with dummy/software"]
    fn widget_focus_can_replace_overlay_without_losing_navigation() {
        let _guard = reforge_settings::Launch {
            game_id: "standalone".into(),
            entry_id: None,
            path: None,
            cli: reforge_settings::Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ]),
        }
        .install();
        let display = crate::Window::new("widget focus", 320, 240, Some(3)).unwrap();
        let creator = display.texture_creator();
        let mut gui = Gui::new(&creator);
        let metrics = display.ui_metrics();
        let mut cancel = egui::Id::NULL;
        let mut confirm = egui::Id::NULL;
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = Gui::control(ui.button("Cancel"));
                    cancel = response.id;
                    confirm = Gui::control(ui.button("Remove")).id;
                });
            });
        };
        for _ in 0..3 {
            gui.prepare(metrics, &mut draw);
        }
        gui.surface
            .context
            .memory_mut(|memory| memory.request_focus(cancel));
        let draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    Gui::control(ui.button("Cancel"));
                    Gui::control(ui.button("Remove"));
                });
            });
        };
        let vertices = |gui: &Gui<'_>| {
            gui.surface
                .jobs
                .iter()
                .map(|job| match &job.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum::<usize>()
        };
        gui.prepare(metrics, draw);
        let with_overlay = vertices(&gui);
        gui.set_focus_overlay(false);
        gui.prepare(metrics, draw);
        assert!(
            vertices(&gui) < with_overlay,
            "the additional focus outline must disappear"
        );
        assert_eq!(
            gui.surface.context.memory(|memory| memory.focused()),
            Some(cancel)
        );
        gui.handle_event(&Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(sdl3::keyboard::Keycode::Right),
            scancode: Some(sdl3::keyboard::Scancode::Right),
            keymod: Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        });
        gui.prepare(metrics, draw);
        assert_eq!(
            gui.surface.context.memory(|memory| memory.focused()),
            Some(confirm)
        );
    }
}
