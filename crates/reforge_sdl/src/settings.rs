// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Host settings UI, painted through SDL geometry on every renderer.
//! Game input and clocks remain the responsibility of each engine adapter.

use std::{collections::HashMap, time::Instant};

use egui::{Color32, Key, Pos2, TextureId, Vec2, epaint::Primitive};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod, Scancode},
    mouse::{MouseButton, MouseWheelDirection},
    pixels::{FColor, PixelFormat},
    rect::Rect,
    render::{BlendMode, FPoint, ScaleMode, Vertex, WindowCanvas},
};

use crate::FrameCreator;

use reforge_settings::Draft;
pub use reforge_settings::VideoSettings;
/// An engine can restrict shared choices when its implementation requires it.
#[derive(Clone, Copy, Debug)]
pub struct VideoCapabilities {
    pub min_scale: u32,
    pub max_scale: u32,
    pub allow_auto: bool,
    pub allow_fullscreen: bool,
}

impl Default for VideoCapabilities {
    fn default() -> Self {
        Self {
            min_scale: 1,
            max_scale: 16,
            allow_auto: true,
            allow_fullscreen: true,
        }
    }
}

impl VideoCapabilities {
    pub fn permits(&self, settings: VideoSettings) -> bool {
        let scale_allowed = match settings.scale {
            None => self.allow_auto,
            Some(scale) => scale >= self.min_scale.max(1) && scale <= self.max_scale,
        };
        scale_allowed && (!settings.fullscreen || self.allow_fullscreen)
    }
}

/// Measurements from the actual SDL window, independent of game scale.
#[derive(Clone, Copy, Debug)]
pub struct UiMetrics {
    pub drawable: (u32, u32),
    pub window: (u32, u32),
    pub pixels_per_point: f32,
}

impl UiMetrics {
    fn density(self) -> f32 {
        if self.pixels_per_point.is_finite() && self.pixels_per_point > 0.0 {
            self.pixels_per_point
        } else {
            1.0
        }
    }

    fn pointer(self, x: f32, y: f32) -> Pos2 {
        Pos2::new(
            x * self.drawable.0 as f32 / self.window.0.max(1) as f32 / self.density(),
            y * self.drawable.1 as f32 / self.window.1.max(1) as f32 / self.density(),
        )
    }
}

/// Only the embedded settings window is supported; no native egui viewports,
/// clipboard integration or custom paint callbacks are needed by these controls.
pub struct SettingsMenu<'a> {
    pub(super) creator: &'a FrameCreator,
    pub(super) context: egui::Context,
    pub(super) capabilities: VideoCapabilities,
    pub(super) open: bool,
    pub(super) events: Vec<Event>,
    pub(super) textures: HashMap<TextureId, crate::resources::TextureLease>,
    pub(super) generation: usize,
    pub(super) draft: Option<Draft>,
    pub(super) pending: Option<Draft>,
    pub(super) scope: usize,
    #[cfg(test)]
    pub(super) cancel_rect: Option<egui::Rect>,
    #[cfg(test)]
    pub(super) apply_rect: Option<egui::Rect>,
    pub(super) game_editing: bool,
    pub(super) game_touched: bool,
    pub(super) status: Option<String>,
    pub(super) jobs: Vec<egui::ClippedPrimitive>,
    pub(super) pending_textures: egui::TexturesDelta,
    pub(super) density: f32,
    pub(super) started: Instant,
    pub(super) focused: bool,
    pub(super) modifiers: egui::Modifiers,
    pub(super) error: Option<String>,
    pub(super) escape_held: bool,
}

impl<'a> SettingsMenu<'a> {
    pub fn new(creator: &'a FrameCreator, capabilities: VideoCapabilities) -> Self {
        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());
        context.style_mut(|style| {
            style.animation_time = 0.0;
            style.spacing.item_spacing = Vec2::new(12.0, 12.0);
            style.spacing.button_padding = Vec2::new(14.0, 7.0);
            style.visuals.selection.bg_fill = Color32::from_rgb(43, 103, 131);
        });
        crate::settings_ui::style(&context);
        Self {
            creator,
            context,
            capabilities,
            open: false,
            events: Vec::new(),
            textures: HashMap::new(),
            generation: creator.generation(),
            draft: None,
            pending: None,
            scope: 0,
            #[cfg(test)]
            cancel_rect: None,
            #[cfg(test)]
            apply_rect: None,
            game_editing: false,
            game_touched: false,
            status: None,
            jobs: Vec::new(),
            pending_textures: egui::TexturesDelta::default(),
            density: 1.0,
            started: Instant::now(),
            focused: true,
            modifiers: egui::Modifiers::default(),
            error: None,
            escape_held: false,
        }
    }

    pub fn set_status(&mut self, status: String) {
        self.status = Some(status);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_error(&mut self, error: String) {
        self.pending = None;
        self.error = Some(error);
    }

    /// Route host quit/window events first. The menu consumes gameplay input
    /// while open, and consumes both edges of its own F10/Escape shortcuts.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        if let Event::KeyDown {
            scancode: Some(Scancode::F10),
            repeat,
            keymod,
            ..
        } = event
        {
            if keymod.intersects(
                Mod::LCTRLMOD
                    | Mod::RCTRLMOD
                    | Mod::LALTMOD
                    | Mod::RALTMOD
                    | Mod::LGUIMOD
                    | Mod::RGUIMOD,
            ) {
                return self.open;
            }
            if !repeat {
                self.open = !self.open;
                if self.open {
                    if let Event::KeyDown { window_id, .. } = event {
                        start_text_input(*window_id);
                    }
                }
                self.draft = None;
                self.pending = None;
                self.game_touched = false;
                self.events.clear();
                self.context.memory_mut(|memory| memory.reset_areas());
            }
            return true;
        }
        if matches!(
            event,
            Event::KeyUp {
                scancode: Some(Scancode::F10),
                ..
            }
        ) {
            return true;
        }
        if matches!(
            event,
            Event::KeyDown {
                scancode: Some(Scancode::Escape),
                ..
            }
        ) && (self.open || self.escape_held)
        {
            self.open = false;
            self.draft = None;
            self.pending = None;
            self.game_touched = false;
            self.escape_held = true;
            self.events.clear();
            return true;
        }
        if matches!(
            event,
            Event::KeyUp {
                scancode: Some(Scancode::Escape),
                ..
            }
        ) && self.escape_held
        {
            self.escape_held = false;
            return true;
        }
        match event {
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.focused = false;
                self.modifiers = egui::Modifiers::default();
                self.escape_held = false;
            }
            Event::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => self.focused = true,
            _ => {}
        }
        if self.open
            && matches!(
                event,
                Event::Window {
                    win_event: WindowEvent::FocusLost
                        | WindowEvent::FocusGained
                        | WindowEvent::MouseLeave,
                    ..
                }
            )
        {
            self.events.push(event.clone());
        }
        if self.open && is_menu_input(event) {
            self.events.push(event.clone());
            true
        } else {
            false
        }
    }

    /// Build UI before painting. A change is a request: callers apply it to
    /// SDL and provide the actual current values on the following frame.
    pub fn prepare(&mut self, metrics: UiMetrics, current: VideoSettings) -> Option<VideoSettings> {
        if !self.open {
            self.jobs.clear();
            self.events.clear();
            return None;
        }
        if self.generation != self.creator.generation() {
            self.textures.clear();
            self.pending_textures = egui::TexturesDelta::default();
            self.context = egui::Context::default();
            self.context.set_visuals(egui::Visuals::dark());
            crate::settings_ui::style(&self.context);
            self.generation = self.creator.generation();
        }
        if self.draft.is_none() {
            let saved = self.creator.session.borrow().saved.clone();
            self.game_editing = !saved.game.is_empty();
            self.draft = Some(saved);
        }
        self.density = metrics.density();
        let screen = egui::Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(
                metrics.drawable.0 as f32 / self.density,
                metrics.drawable.1 as f32 / self.density,
            ),
        );
        let mut input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(self.started.elapsed().as_secs_f64()),
            focused: self.focused,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(self.density);
        for event in self.events.drain(..) {
            translate_event(&event, metrics, &mut self.modifiers, &mut input.events);
        }
        input.modifiers = self.modifiers;
        let context = self.context.clone();
        let output = context.run(input, |ctx| {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                screen,
                0.0,
                Color32::from_black_alpha(165),
            );
            if crate::settings_ui::draw(self, ctx, screen) {
                self.open = false;
                self.draft = None;
                self.pending = None;
            }
        });
        self.pending_textures.append(output.textures_delta);
        self.density = output.pixels_per_point;
        self.jobs = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let _ = current;
        let request = self
            .pending
            .as_ref()
            .map(|draft| self.creator.session.borrow().effective(draft));
        if request.is_some_and(|value| !self.capabilities.permits(value)) {
            self.set_error("These display settings are unavailable for this game".into());
            None
        } else {
            request
        }
    }

    /// Called only after the runtime accepted Apply. A save failure keeps the
    /// draft and dirty markers available for retry without rewriting CLI values.
    pub fn applied(&mut self) {
        if let Some(draft) = self.pending.take() {
            let result = self.creator.session.borrow_mut().commit(draft.clone());
            match result {
                Ok(()) => {
                    self.draft = Some(draft);
                    self.error = None;
                    self.game_touched = false;
                }
                Err(error) => {
                    self.error = Some(format!("Settings applied, but could not save: {error}"))
                }
            }
        }
    }

    /// Paint in actual output pixels, after CRT processing. Restore all SDL
    /// coordinate state even when texture upload or geometry drawing fails.
    pub fn paint(&mut self, canvas: &mut WindowCanvas) -> Result<(), String> {
        if !self.open || self.generation != self.creator.generation() {
            return Ok(());
        }
        let logical = canvas.logical_size();
        let viewport = canvas.viewport();
        let clip = canvas.clip_rect();
        let scale = canvas.scale();
        let result = (|| {
            canvas
                .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
                .map_err(|error| error.to_string())?;
            canvas
                .set_scale(1.0, 1.0)
                .map_err(|error| error.to_string())?;
            canvas.set_viewport(None);
            canvas.set_clip_rect(None);
            self.paint_inner(canvas)
        })();
        let restore = canvas
            .set_logical_size(logical.0, logical.1, logical.2)
            .map_err(|error| error.to_string())
            .and_then(|()| {
                canvas
                    .set_scale(scale.0, scale.1)
                    .map_err(|error| error.to_string())
            });
        canvas.set_viewport(viewport);
        canvas.set_clip_rect(clip);
        result.and(restore)
    }

    fn paint_inner(&mut self, canvas: &mut WindowCanvas) -> Result<(), String> {
        // The tested SDL software triangle path accepts premultiplied blending
        // but replaces destination pixels instead of blending them. Its standard
        // alpha path works; keep one geometry adapter with this colour conversion.
        let straight_alpha = canvas.renderer_name == "software";
        // Retain deltas until successful upload; a temporary rendering failure
        // must not permanently lose egui's font atlas updates.
        for (id, delta) in &self.pending_textures.set {
            let size = delta.image.size();
            let pixels: Vec<u8> = match &delta.image {
                egui::ImageData::Color(image) => image
                    .pixels
                    .iter()
                    .flat_map(|color| rgba(*color, straight_alpha))
                    .collect(),
                egui::ImageData::Font(image) => image
                    .srgba_pixels(None)
                    .flat_map(|color| rgba(color, straight_alpha))
                    .collect(),
            };
            if delta.pos.is_none() {
                let mut texture = self
                    .creator
                    .create_texture_streaming(PixelFormat::RGBA32, size[0] as u32, size[1] as u32)
                    .map_err(|error| error.to_string())?;
                // The wrapper's BlendMode enum omits this SDL3 built-in mode.
                // Preserve egui's premultiplied interpolation on hardware
                // renderers, including transparent antialiasing fringes.
                if straight_alpha {
                    texture.set_blend_mode(BlendMode::Blend);
                } else {
                    // SAFETY: texture is owned here and live on the renderer
                    // thread; SDL copies the blend value and retains no Rust data.
                    let configured = unsafe {
                        sdl3::sys::render::SDL_SetTextureBlendMode(
                            texture.raw(),
                            sdl3::sys::blendmode::SDL_BLENDMODE_BLEND_PREMULTIPLIED,
                        )
                    };
                    if !configured {
                        return Err(sdl3::get_error().to_string());
                    }
                }
                self.textures.insert(*id, texture);
            }
            let texture = self
                .textures
                .get_mut(id)
                .ok_or("missing egui texture for partial upload")?;
            let region = delta
                .pos
                .map(|pos| Rect::new(pos[0] as i32, pos[1] as i32, size[0] as u32, size[1] as u32));
            texture
                .update(region, &pixels, size[0] * 4)
                .map_err(|error| error.to_string())?;
            texture.set_scale_mode(match delta.options.magnification {
                egui::TextureFilter::Nearest => ScaleMode::Nearest,
                egui::TextureFilter::Linear => ScaleMode::Linear,
            });
        }
        self.pending_textures.set.clear();
        let output = canvas.output_size().map_err(|error| error.to_string())?;
        for job in &self.jobs {
            let Some(clip) = physical_clip(job.clip_rect, self.density, output) else {
                continue;
            };
            canvas.set_clip_rect(clip);
            let Primitive::Mesh(mesh) = &job.primitive else {
                return Err(
                    "custom egui paint callbacks are unsupported by the SDL settings painter"
                        .into(),
                );
            };
            let texture = self
                .textures
                .get(&mesh.texture_id)
                .ok_or("missing egui texture")?;
            // Keep premultiplied vertex and texture colours throughout drawing;
            // converting before interpolation would darken antialiasing edges.
            let vertices: Vec<Vertex> = mesh
                .vertices
                .iter()
                .map(|vertex| {
                    let [r, g, b, a] = rgba(vertex.color, straight_alpha);
                    Vertex {
                        position: FPoint::new(
                            vertex.pos.x * self.density,
                            vertex.pos.y * self.density,
                        ),
                        color: FColor::RGBA(
                            r as f32 / 255.0,
                            g as f32 / 255.0,
                            b as f32 / 255.0,
                            a as f32 / 255.0,
                        ),
                        tex_coord: FPoint::new(vertex.uv.x, vertex.uv.y),
                    }
                })
                .collect();
            texture.with(|texture| {
                canvas
                    .render_geometry(&vertices, Some(texture), &mesh.indices)
                    .map_err(|error| error.to_string())
            })??;
        }
        for id in self.pending_textures.free.drain(..) {
            self.textures.remove(&id);
        }
        Ok(())
    }
}

fn start_text_input(window_id: u32) {
    // SAFETY: the event identifies a window in this main-thread SDL process.
    // Lookup is borrowed, checked for null, and SDL takes no Rust ownership.
    unsafe {
        let window =
            sdl3::sys::video::SDL_GetWindowFromID(sdl3::sys::video::SDL_WindowID(window_id));
        if !window.is_null() {
            sdl3::sys::keyboard::SDL_StartTextInput(window);
        }
    }
}

fn rgba(color: Color32, straight_alpha: bool) -> [u8; 4] {
    let mut bytes = color.to_array();
    if straight_alpha && bytes[3] != 0 {
        // SDL's default sRGB renderer blends encoded channel values. Undo the
        // premultiplication in that same space, rather than egui's linear-light
        // colour conversion, so switching renderer does not dim translucent UI.
        let alpha = u32::from(bytes[3]);
        for channel in &mut bytes[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    bytes
}

fn physical_clip(rect: egui::Rect, density: f32, output: (u32, u32)) -> Option<Rect> {
    let left = (rect.min.x * density).floor().clamp(0.0, output.0 as f32) as i32;
    let top = (rect.min.y * density).floor().clamp(0.0, output.1 as f32) as i32;
    let right = (rect.max.x * density).ceil().clamp(0.0, output.0 as f32) as i32;
    let bottom = (rect.max.y * density).ceil().clamp(0.0, output.1 as f32) as i32;
    (right > left && bottom > top)
        .then(|| Rect::new(left, top, (right - left) as u32, (bottom - top) as u32))
}

fn is_menu_input(event: &Event) -> bool {
    matches!(
        event,
        Event::KeyDown { .. }
            | Event::KeyUp { .. }
            | Event::TextInput { .. }
            | Event::TextEditing { .. }
            | Event::MouseMotion { .. }
            | Event::MouseButtonDown { .. }
            | Event::MouseButtonUp { .. }
            | Event::MouseWheel { .. }
    )
}

fn modifiers(keymod: Mod) -> egui::Modifiers {
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let mac_cmd = keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD);
    egui::Modifiers {
        alt: keymod.intersects(Mod::LALTMOD | Mod::RALTMOD),
        ctrl,
        shift: keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
        mac_cmd,
        command: if cfg!(target_os = "macos") {
            mac_cmd
        } else {
            ctrl
        },
    }
}

fn ui_key(keycode: Keycode) -> Option<Key> {
    Some(match keycode {
        Keycode::Tab => Key::Tab,
        Keycode::Return | Keycode::KpEnter => Key::Enter,
        Keycode::Space => Key::Space,
        Keycode::Left => Key::ArrowLeft,
        Keycode::Right => Key::ArrowRight,
        Keycode::Up => Key::ArrowUp,
        Keycode::Down => Key::ArrowDown,
        Keycode::Home => Key::Home,
        Keycode::End => Key::End,
        Keycode::PageUp => Key::PageUp,
        Keycode::PageDown => Key::PageDown,
        _ => return None,
    })
}

fn translate_event(
    event: &Event,
    metrics: UiMetrics,
    mods: &mut egui::Modifiers,
    events: &mut Vec<egui::Event>,
) {
    match event {
        Event::KeyDown {
            keycode,
            keymod,
            repeat,
            ..
        }
        | Event::KeyUp {
            keycode,
            keymod,
            repeat,
            ..
        } => {
            *mods = modifiers(*keymod);
            if let Some(key) = keycode.and_then(ui_key) {
                events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: matches!(event, Event::KeyDown { .. }),
                    repeat: *repeat,
                    modifiers: *mods,
                });
            }
        }
        Event::TextInput { text, .. } => events.push(egui::Event::Text(text.clone())),
        Event::MouseMotion { x, y, .. } => {
            events.push(egui::Event::PointerMoved(metrics.pointer(*x, *y)))
        }
        Event::MouseButtonDown {
            mouse_btn, x, y, ..
        }
        | Event::MouseButtonUp {
            mouse_btn, x, y, ..
        } => {
            let button = match mouse_btn {
                MouseButton::Left => egui::PointerButton::Primary,
                MouseButton::Right => egui::PointerButton::Secondary,
                MouseButton::Middle => egui::PointerButton::Middle,
                _ => return,
            };
            events.push(egui::Event::PointerButton {
                pos: metrics.pointer(*x, *y),
                button,
                pressed: matches!(event, Event::MouseButtonDown { .. }),
                modifiers: *mods,
            });
        }
        Event::MouseWheel {
            x, y, direction, ..
        } => {
            let sign = if *direction == MouseWheelDirection::Flipped {
                -1.0
            } else {
                1.0
            };
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(*x * sign, *y * sign),
                modifiers: *mods,
            });
        }
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => events.push(egui::Event::WindowFocused(false)),
        Event::Window {
            win_event: WindowEvent::FocusGained,
            ..
        } => events.push(egui::Event::WindowFocused(true)),
        Event::Window {
            win_event: WindowEvent::MouseLeave,
            ..
        } => events.push(egui::Event::PointerGone),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scan: Scancode, modifiers: Mod, down: bool, repeat: bool) -> Event {
        if down {
            Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: None,
                scancode: Some(scan),
                keymod: modifiers,
                repeat,
                which: 0,
                raw: 0,
            }
        } else {
            Event::KeyUp {
                timestamp: 0,
                window_id: 0,
                keycode: None,
                scancode: Some(scan),
                keymod: modifiers,
                repeat,
                which: 0,
                raw: 0,
            }
        }
    }

    #[test]
    #[ignore = "owns SDL video; run serially on GPU or with dummy/software"]
    fn menu_draws_on_active_renderer_and_preserves_premultiplied_alpha() {
        use crate::{FrameFormat, FrameTexture, Window};
        let scale = std::env::var("REFORGE_SETTINGS_SCALE")
            .ok()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(4);
        let mut display = Window::new("REforge settings check", 320, 240, Some(scale)).unwrap();
        let creator = display.texture_creator();
        let mut frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgba).unwrap();
        frame
            .update(&[100, 100, 100, 255].repeat(320 * 200))
            .unwrap();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
        let toggle = key(Scancode::F10, Mod::NOMOD, true, false);
        for modifier in [Mod::LALTMOD, Mod::LCTRLMOD, Mod::LGUIMOD] {
            assert!(!menu.handle_event(&key(Scancode::F10, modifier, true, false)));
            assert!(!menu.is_open());
        }
        assert!(menu.handle_event(&toggle));
        assert!(menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, true)));
        assert!(menu.is_open());
        for _ in 0..3 {
            menu.prepare(display.ui_metrics(), display.video_settings());
            display
                .present_with(&frame, |canvas| menu.paint(canvas))
                .unwrap();
        }
        // Capture only this authored synthetic scene, when explicitly requested.
        if let Some(path) = std::env::var_os("REFORGE_SETTINGS_CAPTURE") {
            display
                .present_with(&frame, |canvas| {
                    menu.paint(canvas)?;
                    let old = canvas.logical_size();
                    let viewport = canvas.viewport();
                    let clip = canvas.clip_rect();
                    canvas
                        .set_logical_size(
                            0,
                            0,
                            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED,
                        )
                        .map_err(|e| e.to_string())?;
                    canvas.set_viewport(None);
                    canvas.set_clip_rect(None);
                    let surface = canvas.read_pixels(None).map_err(|e| e.to_string())?;
                    let result = surface.save_bmp(path).map_err(|e| e.to_string());
                    canvas
                        .set_logical_size(old.0, old.1, old.2)
                        .map_err(|e| e.to_string())?;
                    canvas.set_viewport(viewport);
                    canvas.set_clip_rect(clip);
                    result
                })
                .unwrap();
        }
        // A half-transparent red quad over grey checks blending through actual
        // SDL geometry, rather than only checking our colour conversion helper.
        let id = TextureId::Managed(1000);
        menu.pending_textures.set.push((
            id,
            egui::epaint::ImageDelta::full(
                egui::ColorImage::new([1, 1], Color32::WHITE),
                egui::TextureOptions::NEAREST,
            ),
        ));
        menu.density = 1.0;
        let mut mesh = egui::Mesh::with_texture(id);
        mesh.add_rect_with_uv(
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0)),
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::from_rgba_premultiplied(128, 0, 0, 128),
        );
        menu.jobs = vec![egui::ClippedPrimitive {
            clip_rect: egui::Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }];
        display
            .present_with(&frame, |canvas| {
                menu.paint(canvas)?;
                let old = canvas.logical_size();
                canvas
                    .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
                    .map_err(|e| e.to_string())?;
                let pixels = canvas
                    .read_pixels(Rect::new(20, 20, 1, 1))
                    .unwrap()
                    .convert_format(PixelFormat::RGB24)
                    .unwrap();
                pixels.with_lock(|bytes| {
                    assert!(
                        (176..=180).contains(&bytes[0]),
                        "channels: {:?}",
                        &bytes[..3]
                    );
                    assert!((48..=51).contains(&bytes[1]));
                    assert!((48..=51).contains(&bytes[2]));
                });
                canvas
                    .set_logical_size(old.0, old.1, old.2)
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        // Operate Cancel through translated SDL coordinates.
        // Its position comes from egui's measured window, not a fixed game size.
        let metrics = display.ui_metrics();
        let pos = menu.cancel_rect.expect("measured Cancel button").center();
        let x =
            pos.x * metrics.pixels_per_point * metrics.window.0 as f32 / metrics.drawable.0 as f32;
        let y =
            pos.y * metrics.pixels_per_point * metrics.window.1 as f32 / metrics.drawable.1 as f32;
        menu.handle_event(&Event::MouseButtonDown {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x,
            y,
        });
        menu.prepare(metrics, display.video_settings());
        menu.handle_event(&Event::MouseButtonUp {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x,
            y,
        });
        menu.prepare(metrics, display.video_settings());
        assert!(
            !menu.is_open(),
            "mouse release must activate Cancel at desktop-scaled coordinates"
        );
        menu.handle_event(&toggle);
        assert!(menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, false)));
        assert!(!menu.is_open());
        assert!(menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, true)));
        assert!(menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, false, false)));
        // Small-window layout is deferred; Escape must remain usable even when
        // desktop scaling leaves too little space for the Compact layout.
        menu.handle_event(&toggle);
        let tiny = UiMetrics {
            drawable: (322, 241),
            window: (143, 107),
            pixels_per_point: 2.25,
        };
        for _ in 0..3 {
            menu.prepare(tiny, display.video_settings());
        }
        menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, false));
        assert!(!menu.is_open(), "small menus remain keyboard-dismissible");
    }

    #[test]
    #[ignore = "owns SDL video; run serially on GPU or dummy/software"]
    fn apply_switches_renderer_reloads_textures_and_keeps_failed_draft() {
        use crate::{FrameFormat, FrameTexture, Window};
        use reforge_settings::{Backend, Renderer};
        let mut display = Window::new("settings renderer transaction", 320, 240, Some(8)).unwrap();
        let creator = display.texture_creator();
        let mut frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgb).unwrap();
        let pixels = [80, 120, 160].repeat(320 * 200);
        frame.update(&pixels).unwrap();
        assert!(frame.update(&pixels[..pixels.len() - 1]).is_err());
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        menu.prepare(display.ui_metrics(), display.video_settings());
        let unchanged = display.video_settings();
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "vsync", "false".into());
        assert!(
            menu.prepare(display.ui_metrics(), display.video_settings())
                .is_none()
        );
        assert_eq!(
            display.video_settings(),
            unchanged,
            "editing must not apply"
        );
        menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, false));
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        menu.prepare(display.ui_metrics(), display.video_settings());
        assert_eq!(
            menu.draft.as_ref().unwrap().global_values().vsync,
            unchanged.vsync
        );
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "vsync", "false".into());
        for _ in 0..3 {
            menu.prepare(display.ui_metrics(), display.video_settings());
        }
        let metrics = display.ui_metrics();
        let pos = menu.apply_rect.unwrap().center();
        let x =
            pos.x * metrics.pixels_per_point * metrics.window.0 as f32 / metrics.drawable.0 as f32;
        let y =
            pos.y * metrics.pixels_per_point * metrics.window.1 as f32 / metrics.drawable.1 as f32;
        for down in [true, false] {
            let event = if down {
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
            if let Some(request) = menu.prepare(metrics, display.video_settings()) {
                display.apply_video_settings(request).unwrap();
                menu.applied();
            }
        }
        assert!(menu.is_open(), "Apply keeps settings open");
        assert!(!display.video_settings().vsync);
        assert_eq!(
            menu.draft.as_ref().unwrap(),
            &creator.session.borrow().saved
        );
        let mut rgba = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgba).unwrap();
        rgba.update(&[80, 120, 160, 255].repeat(320 * 200)).unwrap();

        for renderer in [
            Renderer::Software,
            Renderer::Sdl,
            Renderer::Auto,
            Renderer::Software,
        ] {
            let mut request = display.video_settings();
            request.renderer = renderer;
            request.backend = Backend::Auto;
            request.vsync = false;
            eprintln!("switch {renderer:?}: begin");
            display.apply_video_settings(request).unwrap();
            eprintln!("switch {renderer:?}: applied");
            // Deliberately do not call frame.update: the retained image and font
            // atlas must survive changing the renderer while simulation is paused.
            for image in [&frame, &rgba] {
                display
                    .present_with(image, |canvas| {
                        let image = canvas
                            .read_pixels(None)
                            .map_err(|e| e.to_string())?
                            .convert_format(PixelFormat::RGB24)
                            .map_err(|e| e.to_string())?;
                        image.with_lock(|bytes| {
                            assert_eq!(&bytes[..3], &[80, 120, 160], "retained frame pixels")
                        });
                        Ok(())
                    })
                    .unwrap();
            }
            menu.prepare(display.ui_metrics(), display.video_settings());
            display
                .present_with(&frame, |canvas| {
                    menu.paint(canvas)?;
                    Ok(())
                })
                .unwrap();
            assert!(
                menu.textures
                    .values()
                    .all(|texture| texture.belongs_to(&creator))
            );
            assert!(frame.texture.borrow().belongs_to(&creator));
            assert!(!menu.jobs.is_empty());
            eprintln!("switch {renderer:?}: painted");
        }
        if display._video.current_video_driver() != "dummy" {
            let mut request = display.video_settings();
            request.renderer = Renderer::Gpu;
            request.backend = Backend::Auto;
            eprintln!("explicit GPU: begin");
            display.apply_video_settings(request).unwrap();
            eprintln!("explicit GPU: applied");
            display.present(&frame).unwrap();
            eprintln!("explicit GPU: painted");
            if cfg!(target_os = "linux") {
                request.backend = Backend::Vulkan;
                eprintln!("explicit Vulkan: begin");
                display.apply_video_settings(request).unwrap();
                eprintln!("explicit Vulkan: applied");
                display.present(&frame).unwrap();
                assert!(display.renderer_status().contains("vulkan"));
            }
            menu.prepare(display.ui_metrics(), display.video_settings());
            display
                .present_with(&frame, |canvas| menu.paint(canvas))
                .unwrap();
        }
        let before = display.video_settings();
        let generation = creator.generation();
        let mut bad = before;
        bad.renderer = Renderer::Gpu;
        bad.backend = Backend::OpenGl;
        assert!(display.apply_video_settings(bad).is_err());
        assert_eq!(display.video_settings(), before);
        assert!(creator.generation() > generation);
        display
            .present_with(&frame, |canvas| menu.paint(canvas))
            .unwrap();
    }

    #[test]
    fn restricted_engine_choices_and_fractional_dpi_input() {
        let capabilities = VideoCapabilities {
            min_scale: 2,
            max_scale: 8,
            allow_auto: false,
            allow_fullscreen: false,
        };
        assert!(!capabilities.permits(VideoSettings::default()));
        assert!(capabilities.permits(VideoSettings {
            scale: Some(8),
            fullscreen: false,
            ..VideoSettings::default()
        }));
        assert!(!capabilities.permits(VideoSettings {
            scale: Some(9),
            fullscreen: false,
            ..VideoSettings::default()
        }));
        assert!(!capabilities.permits(VideoSettings {
            scale: Some(4),
            fullscreen: true,
            ..VideoSettings::default()
        }));
        let metrics = UiMetrics {
            drawable: (1280, 960),
            window: (853, 640),
            pixels_per_point: 1.5,
        };
        let pointer = metrics.pointer(426.5, 320.0);
        assert!((pointer.x - 426.6667).abs() < 0.001);
        assert_eq!(pointer.y, 320.0);
        let clip = physical_clip(
            egui::Rect::from_min_max(Pos2::new(-5.0, 1.1), Pos2::new(100.1, 20.1)),
            1.5,
            (1280, 960),
        )
        .unwrap();
        assert_eq!(clip, Rect::new(0, 1, 151, 30));
    }

    #[test]
    #[ignore = "owns SDL video; run alone with SDL_VIDEODRIVER=dummy"]
    fn software_menu_geometry_input_and_state_restoration() {
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("settings synthetic test", 1280, 960)
            .hidden()
            .build()
            .unwrap();
        let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
        let creator = FrameCreator::new(&canvas, reforge_settings::Session::for_window(None));
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
        let toggle = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::F10),
            scancode: Some(Scancode::F10),
            keymod: Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        };
        assert!(menu.handle_event(&toggle));
        assert!(menu.is_open());
        assert!(menu.handle_event(&toggle));
        let mut modified = toggle.clone();
        if let Event::KeyDown { keymod, .. } = &mut modified {
            *keymod = Mod::LALTMOD;
        }
        assert!(!menu.handle_event(&modified));
        assert!(!menu.is_open());
        assert!(menu.handle_event(&toggle));
        let mut repeat = toggle.clone();
        if let Event::KeyDown { repeat, .. } = &mut repeat {
            *repeat = true;
        }
        assert!(menu.handle_event(&repeat));
        assert!(menu.is_open());
        let metrics = UiMetrics {
            drawable: (1280, 960),
            window: (1280, 960),
            pixels_per_point: 1.0,
        };
        assert_eq!(menu.prepare(metrics, VideoSettings::default()), None);
        canvas
            .set_logical_size(
                320,
                240,
                sdl3::sys::render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE,
            )
            .unwrap();
        canvas.set_scale(2.0, 2.0).unwrap();
        let viewport = Rect::new(1, 2, 300, 220);
        let clip = Rect::new(3, 4, 290, 210);
        canvas.set_viewport(viewport);
        canvas.set_clip_rect(clip);
        let original = (
            canvas.logical_size(),
            canvas.scale(),
            canvas.viewport(),
            canvas.clip_rect(),
        );
        menu.paint(&mut canvas).unwrap();
        assert_eq!(
            (
                canvas.logical_size(),
                canvas.scale(),
                canvas.viewport(),
                canvas.clip_rect()
            ),
            original
        );
        assert!(!menu.textures.is_empty());
        assert!(!menu.jobs.is_empty());
        canvas
            .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
            .unwrap();
        canvas.set_scale(1.0, 1.0).unwrap();
        canvas.set_viewport(None);
        canvas.set_clip_rect(None);
        if let Some(path) = std::env::var_os("REFORGE_SETTINGS_TEST_IMAGE") {
            canvas.read_pixels(None).unwrap().save_bmp(path).unwrap();
        }
        canvas
            .set_logical_size(original.0.0, original.0.1, original.0.2)
            .unwrap();
        canvas.set_scale(original.1.0, original.1.1).unwrap();
        canvas.set_viewport(original.2);
        canvas.set_clip_rect(original.3);
        // Force a genuine drawing failure, then verify the same restoration path.
        menu.textures.clear();
        assert!(menu.paint(&mut canvas).is_err());
        assert_eq!(
            (
                canvas.logical_size(),
                canvas.scale(),
                canvas.viewport(),
                canvas.clip_rect()
            ),
            original
        );
        assert!(menu.handle_event(&toggle));
        assert!(!menu.is_open());
        assert!(menu.handle_event(&toggle));
        let mut escape = toggle.clone();
        if let Event::KeyDown {
            keycode, scancode, ..
        } = &mut escape
        {
            *keycode = Some(Keycode::Escape);
            *scancode = Some(Scancode::Escape);
        }
        assert!(menu.handle_event(&escape));
        assert!(!menu.is_open());
        if let Event::KeyDown { repeat, .. } = &mut escape {
            *repeat = true;
        }
        assert!(menu.handle_event(&escape));
        let release = Event::KeyUp {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Escape),
            scancode: Some(Scancode::Escape),
            keymod: Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        };
        assert!(menu.handle_event(&release));
        assert!(!menu.handle_event(&escape));
    }

    #[test]
    #[ignore = "owns native SDL GPU window; run alone"]
    fn gpu_menu_after_crt_and_texture_updates() {
        let mut display = crate::Window::new_with_presentation(
            "Settings rendering test",
            320,
            240,
            Some(4),
            crate::Presentation::Hyllian,
        )
        .unwrap();
        assert_eq!(display.canvas.renderer_name, "gpu");
        assert_eq!(display.presentation(), crate::Presentation::Hyllian);
        let creator = display.texture_creator();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
        menu.open = true;
        let mut frame =
            crate::FrameTexture::new(&creator, 320, 200, crate::FrameFormat::Rgba).unwrap();
        frame.update(&[170, 0, 170, 255].repeat(320 * 200)).unwrap();
        for _ in 0..3 {
            menu.prepare(display.ui_metrics(), display.video_settings());
            display
                .present_with(&frame, |canvas| {
                    menu.paint(canvas)?;
                    if let Some(path) = std::env::var_os("REFORGE_SETTINGS_TEST_IMAGE") {
                        let logical = canvas.logical_size();
                        canvas
                            .set_logical_size(
                                0,
                                0,
                                sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED,
                            )
                            .map_err(|error| error.to_string())?;
                        canvas
                            .read_pixels(None)
                            .map_err(|error| error.to_string())?
                            .save_bmp(path)
                            .map_err(|error| error.to_string())?;
                        canvas
                            .set_logical_size(logical.0, logical.1, logical.2)
                            .map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })
                .unwrap();
        }
    }
}
