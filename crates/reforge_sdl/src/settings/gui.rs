// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Standalone Compact host UI using the settings painter and input adapter.

use super::*;

/// A host surface on one fixed renderer. It shares the settings menu's painter,
/// texture ownership, DPI conversion and navigation without opening preferences.
pub struct Gui<'a> {
    surface: SettingsMenu<'a>,
}
impl<'a> Gui<'a> {
    pub fn new(creator: &'a FrameCreator) -> Self {
        let mut surface = SettingsMenu::new(creator, VideoCapabilities::default());
        surface.open = true;
        Self { surface }
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
    pub fn prepare(&mut self, metrics: UiMetrics, mut draw: impl FnMut(&egui::Context)) {
        let s = &mut self.surface;
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
            crate::settings_navigation::paint_focus(context);
        });
        s.pending_textures.append(output.textures_delta);
        s.density = output.pixels_per_point;
        s.jobs = s.context.tessellate(output.shapes, output.pixels_per_point);
    }
    pub fn paint(&mut self, canvas: &mut WindowCanvas) -> Result<(), String> {
        self.surface.paint(canvas)
    }
}
