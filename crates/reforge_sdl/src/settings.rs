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
pub use reforge_settings::{AudioSettings, ControlMode, RuntimeSettings, SoundRenderer};
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
    pub fn permits(&self, settings: RuntimeSettings) -> bool {
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
    events: Vec<MenuEvent>,
    pub(super) navigation: crate::settings_navigation::Navigation,
    pub(super) textures: HashMap<TextureId, crate::resources::TextureLease>,
    pub(super) generation: usize,
    pub(super) draft: Option<Draft>,
    pub(super) pending: Option<Draft>,
    pub(super) scope: usize,
    #[cfg(test)]
    pub(super) cancel_rect: Option<egui::Rect>,
    #[cfg(test)]
    pub(super) apply_rect: Option<egui::Rect>,
    pub(super) audio_devices: Vec<String>,
    pub(super) audio_devices_loaded: bool,
    pub(super) audio_renderers: Vec<reforge_settings::SoundRenderer>,
    pub(super) audio_categories: Vec<reforge_settings::SoundCategory>,
    pub(super) mach3_options: bool,
    pub(super) alleycat_options: bool,
    pub(super) game_editing: [bool; 4],
    pub(super) page: usize,
    pub(super) game_touched: [bool; 4],
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

enum MenuEvent {
    Sdl(Event),
    Ui(egui::Event),
}

impl<'a> SettingsMenu<'a> {
    /// Expose Alley Cat's game-only input policy beside shared controller settings.
    pub fn with_alleycat_options(mut self) -> Self {
        let mut session = self.creator.session.borrow_mut();
        if session.game == "standalone" {
            session.game = "alley-cat".into();
        }
        self.alleycat_options = session.game == "alley-cat";
        drop(session);
        self
    }
    /// Declare available engine renderers; absence leaves game audio controls
    /// disabled while shared defaults can still be edited.
    pub fn with_audio_renderers(mut self, renderers: &[reforge_settings::SoundRenderer]) -> Self {
        self.audio_renderers = renderers.to_vec();
        self
    }
    /// Declare semantic groups implemented by the engine's audio adapter.
    pub fn with_audio_categories(mut self, categories: &[reforge_settings::SoundCategory]) -> Self {
        self.audio_categories = categories.to_vec();
        self
    }
    /// Expose Mach 3's native startup switch, independently of host audio.
    /// Standalone previews retain in-memory settings; app launches use their
    /// configured per-game storage. An explicit CLI value is temporary/locked.
    pub fn with_mach3_options(mut self, samples: Option<bool>) -> Self {
        {
            let mut session = self.creator.session.borrow_mut();
            if session.game == "standalone" {
                session.game = "mach-3".into();
            }
            self.mach3_options = session.game == "mach-3";
            if self.mach3_options {
                if let Some(value) = samples {
                    session.cli.insert(
                        reforge_settings::DIGITIZED_SAMPLES.into(),
                        value.to_string(),
                    );
                }
            }
        }
        self
    }
    /// Resolve once before starting simulation. Saving a later draft does not
    /// change the running game's native sample flag.
    pub fn mach3_startup_settings(&self) -> reforge_settings::Mach3Settings {
        let session = self.creator.session.borrow();
        session.effective(&session.saved).mach3
    }
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
            navigation: Default::default(),
            textures: HashMap::new(),
            generation: creator.generation(),
            draft: None,
            pending: None,
            scope: 0,
            #[cfg(test)]
            cancel_rect: None,
            #[cfg(test)]
            apply_rect: None,
            audio_devices: Vec::new(),
            audio_devices_loaded: false,
            audio_renderers: Vec::new(),
            audio_categories: Vec::new(),
            mach3_options: false,
            alleycat_options: false,
            game_editing: [false; 4],
            page: 0,
            game_touched: [false; 4],
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

    /// Toggle from a host control (for example a gamepad's View/Back button).
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            // SAFETY: settings run on SDL's main thread. The focused window is
            // borrowed, checked for null, and is not retained or destroyed here.
            unsafe {
                let window = sdl3::sys::keyboard::SDL_GetKeyboardFocus();
                if !window.is_null() {
                    sdl3::sys::keyboard::SDL_StartTextInput(window);
                }
            }
        }
        self.draft = None;
        self.pending = None;
        self.game_touched = [false; 4];
        self.events.clear();
        self.navigation = Default::default();
        self.navigation.focus_first = self.open;
        self.context.memory_mut(|memory| {
            memory.close_popup();
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        self.context.memory_mut(|memory| memory.reset_areas());
    }

    /// Route only events accepted by the shared active-gamepad owner. Open menu
    /// input is consumed here, including pause, and never reaches either game.
    pub fn handle_gamepad(&mut self, pad: crate::gamepad::PadEvent) -> bool {
        if pad.settings {
            self.toggle();
            return true;
        }
        if !self.open {
            return false;
        }
        if pad.cancel {
            self.back();
        } else {
            self.navigation
                .controller(pad, self.started.elapsed().as_secs_f64());
            let mut events = Vec::new();
            self.navigation.take_events(&mut events);
            self.events.extend(events.into_iter().map(MenuEvent::Ui));
        }
        true
    }

    fn back(&mut self) {
        if self.context.memory(|memory| memory.any_popup_open()) {
            crate::settings_navigation::close_popup(&self.context);
        } else {
            self.toggle();
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.pending = None;
        self.error = Some(error);
    }

    /// Route host quit/window events first. The menu consumes gameplay input
    /// while open, and consumes both edges of its own F10/Escape shortcuts.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        if matches!(
            event,
            Event::AudioDeviceAdded { .. } | Event::AudioDeviceRemoved { .. }
        ) {
            self.audio_devices_loaded = false;
        }

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
                self.toggle();
                if self.open {
                    if let Event::KeyDown { window_id, .. } = event {
                        start_text_input(*window_id);
                    }
                }
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
            if !self.escape_held {
                self.back();
            }
            self.escape_held = true;
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
                self.navigation = Default::default();
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
            self.events.push(MenuEvent::Sdl(event.clone()));
        }
        if self.open && is_menu_input(event) {
            self.events.push(MenuEvent::Sdl(event.clone()));
            true
        } else {
            false
        }
    }

    /// Build UI before painting. A change is a request: callers apply it to
    /// SDL and provide the actual current values on the following frame.
    pub fn prepare(
        &mut self,
        metrics: UiMetrics,
        current: RuntimeSettings,
    ) -> Option<RuntimeSettings> {
        if !self.open {
            self.jobs.clear();
            self.events.clear();
            return None;
        }
        if self.generation != self.creator.generation() {
            self.textures.clear();
            self.pending_textures = egui::TexturesDelta::default();
            // Rebuild font textures for the new renderer without discarding
            // measured window sizes, scroll offsets or expanded sections.
            // A fresh layout would otherwise visibly settle over several frames.
            let memory = self.context.memory(Clone::clone);
            self.context = egui::Context::default();
            self.context.memory_mut(|new| *new = memory);
            self.context.set_visuals(egui::Visuals::dark());
            crate::settings_ui::style(&self.context);
            self.generation = self.creator.generation();
        }
        if self.draft.is_none() {
            self.audio_devices_loaded = false;
            let saved = self.creator.session.borrow().saved.clone();
            self.game_editing = [0, 1, 2, 3].map(|page| saved.has_game_overrides(page));
            self.draft = Some(saved);
        }
        if self.page == 2 && !self.audio_devices_loaded {
            self.audio_devices = crate::audio::playback_devices().unwrap_or_default();
            self.audio_devices_loaded = true;
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
            match event {
                MenuEvent::Sdl(event) => {
                    translate_event(&event, metrics, &mut self.modifiers, &mut input.events)
                }
                MenuEvent::Ui(event) => input.events.push(event),
            }
        }
        self.navigation
            .drain(self.started.elapsed().as_secs_f64(), &mut input.events);
        crate::settings_navigation::filter_popup_input(&self.context, &mut input.events);
        self.navigation.navigate(&self.context, &mut input.events);
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
            crate::settings_navigation::paint_focus(ctx);
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
        if request
            .as_ref()
            .is_some_and(|value| !self.capabilities.permits(value.clone()))
        {
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
                    self.game_touched = [false; 4];
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
        Keycode::Backspace => Key::Backspace,
        Keycode::Delete => Key::Delete,
        Keycode::A => Key::A,
        Keycode::C => Key::C,
        Keycode::V => Key::V,
        Keycode::X => Key::X,
        Keycode::Y => Key::Y,
        Keycode::Z => Key::Z,
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
    #[ignore = "owns SDL video; run serially with dummy/software"]
    fn alleycat_controls_scope_draft_and_live_apply() {
        use reforge_settings::{CONTROL_MODE, ControlMode, Launch, Patch, Session};
        fn draw(menu: &mut SettingsMenu<'_>, events: Vec<egui::Event>) -> egui::FullOutput {
            let context = menu.context.clone();
            let screen = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0));
            context.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(menu.started.elapsed().as_secs_f64()),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    crate::settings_ui::draw(menu, ctx, screen);
                },
            )
        }
        fn label(output: &egui::FullOutput, name: &str) -> Option<egui::Rect> {
            output.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == name => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
        }
        fn click(menu: &mut SettingsMenu<'_>, name: &str) {
            draw(menu, vec![]);
            let output = draw(menu, vec![]);
            let pos = label(&output, name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .center();
            for pressed in [true, false] {
                draw(
                    menu,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        let path =
            std::env::temp_dir().join(format!("reforge-controls-ui-{}.toml", std::process::id()));
        assert!(!path.exists());
        let launch = Launch {
            game: "alley-cat".into(),
            path: Some(path.clone()),
            cli: Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ]),
        };
        let _guard = launch.clone().install();
        let mut display = crate::Window::new("controls transaction", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        let mut menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_alleycat_options();
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        menu.prepare(display.ui_metrics(), display.settings());
        assert!(label(&draw(&mut menu, vec![]), "Controls").is_some());
        click(&mut menu, "Alley Cat overrides");
        click(&mut menu, "Controls");
        assert_eq!(menu.page, 3);
        assert!(label(&draw(&mut menu, vec![]), "Use defaults for this section").is_some());
        click(&mut menu, "Use defaults for this section");
        click(&mut menu, "Responsive");
        click(&mut menu, "Original");
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .alleycat
                .control_mode,
            ControlMode::Original
        );
        assert_eq!(
            display.settings().alleycat.control_mode,
            ControlMode::Responsive
        );
        assert!(!path.exists(), "draft changes must not save");
        click(&mut menu, "Apply");
        let request = creator
            .session
            .borrow()
            .effective(menu.pending.as_ref().expect("clicked Apply"));
        display.apply_settings(request).unwrap();
        menu.applied();
        assert_eq!(
            display.settings().alleycat.control_mode,
            ControlMode::Original
        );
        assert_eq!(
            Session::load(launch.clone())
                .saved
                .game_values()
                .get(CONTROL_MODE),
            "original"
        );
        click(&mut menu, "Restore defaults");
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .alleycat
                .control_mode,
            ControlMode::Responsive
        );
        menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, false));
        assert_eq!(
            display.settings().alleycat.control_mode,
            ControlMode::Original
        );
        menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, false, false));
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        menu.prepare(display.ui_metrics(), display.settings());
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .alleycat
                .control_mode,
            ControlMode::Original
        );
        click(&mut menu, "Restore defaults");
        click(&mut menu, "Apply");
        let request = creator
            .session
            .borrow()
            .effective(menu.pending.as_ref().unwrap());
        display.apply_settings(request).unwrap();
        menu.applied();
        assert_eq!(
            display.settings().alleycat.control_mode,
            ControlMode::Responsive
        );
        assert!(!Session::load(launch).saved.has_game_overrides(3));
        click(&mut menu, "Global defaults");
        assert_eq!(menu.page, 3, "Controls is now shared across scopes");
        assert!(label(&draw(&mut menu, vec![]), "Stick dead zone").is_some());
        assert!(label(&draw(&mut menu, vec![]), "Input response").is_none());
        creator.session.borrow_mut().game = "mach-3".into();
        let mut mach_menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_mach3_options(None);
        mach_menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        mach_menu.prepare(display.ui_metrics(), display.settings());
        mach_menu.scope = 1;
        mach_menu.page = 3;
        assert!(label(&draw(&mut mach_menu, vec![]), "Stick dead zone").is_some());
        assert!(label(&draw(&mut mach_menu, vec![]), "Input response").is_none());
        std::fs::remove_file(path).unwrap();
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
            menu.prepare(display.ui_metrics(), display.settings());
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
        menu.prepare(metrics, display.settings());
        menu.handle_event(&Event::MouseButtonUp {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x,
            y,
        });
        menu.prepare(metrics, display.settings());
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
            menu.prepare(tiny, display.settings());
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
        menu.prepare(display.ui_metrics(), display.settings());
        let unchanged = display.settings();
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "vsync", "false".into());
        assert!(
            menu.prepare(display.ui_metrics(), display.settings())
                .is_none()
        );
        assert_eq!(display.settings(), unchanged, "editing must not apply");
        menu.handle_event(&key(Scancode::Escape, Mod::NOMOD, true, false));
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        menu.prepare(display.ui_metrics(), display.settings());
        assert_eq!(
            menu.draft.as_ref().unwrap().global_values().vsync,
            unchanged.vsync
        );
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "vsync", "false".into());
        // Apply must reconcile treatment and renderer together, including the
        // saved draft, rather than leave a remembered but inactive shader.
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "presentation", "hyllian".into());
        menu.draft
            .as_mut()
            .unwrap()
            .edit(false, "renderer", "software".into());
        // Applying from a different scope must still normalize pending global edits.
        menu.scope = 1;
        for _ in 0..3 {
            menu.prepare(display.ui_metrics(), display.settings());
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
            if let Some(request) = menu.prepare(metrics, display.settings()) {
                display.apply_settings(request.clone()).unwrap();
                menu.applied();
            }
        }
        assert!(menu.is_open(), "Apply keeps settings open");
        assert!(!display.settings().vsync);
        assert_eq!(display.presentation(), crate::Presentation::Pixels);
        assert_eq!(display.settings().presentation, crate::Presentation::Pixels);
        assert_eq!(
            creator.session.borrow().saved.global_values().presentation,
            crate::Presentation::Pixels
        );
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
            let mut request = display.settings();
            request.renderer = renderer;
            request.backend = Backend::Auto;
            request.vsync = false;
            eprintln!("switch {renderer:?}: begin");
            display.apply_settings(request.clone()).unwrap();
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
            menu.prepare(display.ui_metrics(), display.settings());
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
            let first_cancel = menu.cancel_rect.unwrap();
            for _ in 0..3 {
                menu.prepare(display.ui_metrics(), display.settings());
                assert_eq!(
                    menu.cancel_rect.unwrap(),
                    first_cancel,
                    "menu layout must be stable from the first frame after switching to {renderer:?}"
                );
            }
            eprintln!("switch {renderer:?}: painted");
        }
        if display._video.current_video_driver() != "dummy" {
            let mut request = display.settings();
            request.renderer = Renderer::Gpu;
            request.backend = Backend::Auto;
            eprintln!("explicit GPU: begin");
            display.apply_settings(request.clone()).unwrap();
            eprintln!("explicit GPU: applied");
            display.present(&frame).unwrap();
            eprintln!("explicit GPU: painted");
            if cfg!(target_os = "linux") {
                request.backend = Backend::Vulkan;
                eprintln!("explicit Vulkan: begin");
                display.apply_settings(request.clone()).unwrap();
                eprintln!("explicit Vulkan: applied");
                display.present(&frame).unwrap();
                assert!(display.renderer_status().contains("vulkan"));
            }
            menu.prepare(display.ui_metrics(), display.settings());
            display
                .present_with(&frame, |canvas| menu.paint(canvas))
                .unwrap();
        }
        let before = display.settings();
        let generation = creator.generation();
        let mut bad = before.clone();
        bad.renderer = Renderer::Gpu;
        bad.backend = Backend::OpenGl;
        assert!(display.apply_settings(bad).is_err());
        assert_eq!(display.settings(), before);
        assert!(creator.generation() > generation);
        display
            .present_with(&frame, |canvas| menu.paint(canvas))
            .unwrap();
    }

    #[test]
    #[ignore = "owns native SDL video; run serially on a Linux GPU desktop"]
    fn renderer_round_trips_at_auto_scale() {
        use crate::{FrameFormat, FrameTexture, Window};
        use reforge_settings::{Backend, Renderer};
        if !cfg!(target_os = "linux") {
            return;
        }
        let _launch = reforge_settings::Launch {
            game: "mach-3".into(),
            path: None,
            cli: reforge_settings::Patch::from([
                ("renderer".into(), "sdl".into()),
                ("backend".into(), "opengl".into()),
            ]),
        }
        .install();
        let mut display = Window::new("renderer round trips", 320, 240, None).unwrap();
        let creator = display.texture_creator();
        let mut frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgb).unwrap();
        frame.update(&[80, 120, 160].repeat(320 * 200)).unwrap();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
        menu.handle_event(&key(Scancode::F10, Mod::NOMOD, true, false));
        let mut gpu_address = None;
        for round in 0..12 {
            for (renderer, backend) in [
                (Renderer::Sdl, Backend::OpenGl),
                (Renderer::Auto, Backend::OpenGl),
                (Renderer::Auto, Backend::Auto),
                (Renderer::Sdl, Backend::OpenGlEs),
                (Renderer::Sdl, Backend::Auto),
                (Renderer::Auto, Backend::Auto),
                (Renderer::Gpu, Backend::Vulkan),
                (Renderer::Sdl, Backend::Vulkan),
                (Renderer::Software, Backend::Auto),
                (Renderer::Auto, Backend::Auto),
            ] {
                eprintln!("round {round}: {renderer:?}/{backend:?}: begin");
                let mut request = display.settings();
                request.renderer = renderer;
                request.backend = backend;
                let generation = creator.generation();
                let previous = display.settings();
                let result = display.apply_settings(request.clone());
                if renderer == Renderer::Sdl && backend == Backend::Vulkan {
                    assert!(result.unwrap_err().contains("choose GPU / Vulkan"));
                    assert_eq!(display.settings(), previous);
                    assert_eq!(creator.generation(), generation);
                } else {
                    result.unwrap();
                }
                eprintln!("round {round}: {}", display.renderer_status());
                if let Some(device) = &display.gpu {
                    let address = std::rc::Rc::as_ptr(device);
                    if let Some(previous) = gpu_address {
                        assert_eq!(address, previous, "compatible GPU device must be retained");
                    }
                    gpu_address = Some(address);
                } else {
                    gpu_address = None;
                }
                for _ in 0..5 {
                    display.poll_events().for_each(drop);
                    menu.prepare(display.ui_metrics(), display.settings());
                    display
                        .present_with(&frame, |canvas| menu.paint(canvas))
                        .unwrap();
                }
            }
        }
        drop(menu);
        drop(display);
        // External textures can outlive Window through their creator. The
        // borrowed GPU device must still be alive until that renderer drops.
        frame.update(&[80, 120, 160].repeat(320 * 200)).unwrap();
    }

    #[test]
    fn restricted_engine_choices_and_fractional_dpi_input() {
        let capabilities = VideoCapabilities {
            min_scale: 2,
            max_scale: 8,
            allow_auto: false,
            allow_fullscreen: false,
        };
        assert!(!capabilities.permits(RuntimeSettings::default()));
        assert!(capabilities.permits(RuntimeSettings {
            scale: Some(8),
            fullscreen: false,
            ..RuntimeSettings::default()
        }));
        assert!(!capabilities.permits(RuntimeSettings {
            scale: Some(9),
            fullscreen: false,
            ..RuntimeSettings::default()
        }));
        assert!(!capabilities.permits(RuntimeSettings {
            scale: Some(4),
            fullscreen: true,
            ..RuntimeSettings::default()
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
        let creator = FrameCreator::new(
            &canvas,
            reforge_settings::Session::for_window(None),
            4,
            None,
        );
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
        assert_eq!(menu.prepare(metrics, RuntimeSettings::default()), None);
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
    #[ignore = "requires desktop and SDL_AUDIO_DRIVER=dummy; run serially"]
    fn audio_endpoint_survives_renderer_changes_and_failed_device_apply() {
        assert_eq!(std::env::var("SDL_AUDIO_DRIVER").as_deref(), Ok("dummy"));
        let mut window =
            crate::Window::new("Audio settings transaction check", 320, 240, Some(4)).unwrap();
        let creator = window.texture_creator();
        let host = creator.audio_playback();
        host.start(48_000).unwrap();
        let mut frame =
            crate::FrameTexture::new(&creator, 320, 200, crate::FrameFormat::Rgba).unwrap();
        frame
            .update(&[80, 120, 160, 255].repeat(320 * 200))
            .unwrap();
        let mut request = window.settings();
        request.audio.volume = 35;
        request.audio.renderer = SoundRenderer::EnhancedPcm;
        request.audio.device = Some(crate::audio::playback_devices().unwrap().remove(0));
        request.renderer = reforge_settings::Renderer::Software;
        window.apply_settings(request.clone()).unwrap();
        assert_eq!(window.settings(), request);
        assert_eq!(host.settings(), request.audio);
        let audio_generation = host.generation();
        window.present(&frame).unwrap();
        request.renderer = reforge_settings::Renderer::Gpu;
        window.apply_settings(request.clone()).unwrap();
        window.present(&frame).unwrap();
        assert_eq!(host.generation(), audio_generation);
        assert_eq!(creator.audio_playback().settings(), request.audio);
        let generation = creator.generation();
        let mut invalid = request.clone();
        invalid.audio.device = Some("reforge-nonexistent-output".into());
        invalid.audio.volume = 80;
        assert!(window.apply_settings(invalid).is_err());
        assert_eq!(window.settings(), request);
        assert_eq!(host.settings(), request.audio);
        assert_eq!(creator.generation(), generation);
        assert!(host.available());
        let mut invalid = request.clone();
        invalid.scale = Some(0);
        invalid.audio.volume = 70;
        assert!(window.apply_settings(invalid).is_err());
        assert_eq!(host.settings(), request.audio);
        host.stop();
        request.vsync = false;
        window.apply_settings(request).unwrap();
        assert!(
            !host.available(),
            "video edits must not reopen a failed audio endpoint"
        );
    }

    #[test]
    #[ignore = "owns native SDL GPU window; run alone"]
    fn gpu_menu_after_crt_and_texture_updates() {
        let mut display = crate::Window::new_with_presentation(
            "Settings rendering test",
            320,
            240,
            Some(8),
            crate::Presentation::Hyllian,
        )
        .unwrap();
        assert_eq!(display.canvas.renderer_name, "gpu");
        assert_eq!(display.presentation(), crate::Presentation::Hyllian);
        let creator = display.texture_creator();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
            .with_audio_renderers(&[SoundRenderer::Speaker, SoundRenderer::EnhancedPcm])
            .with_audio_categories(&reforge_settings::SoundCategory::ALL)
            .with_mach3_options(None);
        // Exercise section notices and outside-control locks in the real painter.
        creator.session.borrow_mut().cli = reforge_settings::Patch::from([
            ("scale".into(), "8".into()),
            ("vsync".into(), "false".into()),
            ("presentation".into(), "hyllian".into()),
            ("audio_renderer".into(), "enhanced-pcm".into()),
            (reforge_settings::DIGITIZED_SAMPLES.into(), "false".into()),
        ]);
        let saved = creator.session.borrow().saved.clone();
        menu.open = true;
        let mut frame =
            crate::FrameTexture::new(&creator, 320, 200, crate::FrameFormat::Rgba).unwrap();
        frame.update(&[170, 0, 170, 255].repeat(320 * 200)).unwrap();
        for page in [0, 1, 2] {
            menu.page = page;
            if page == 2 {
                menu.scope = 1;
                menu.game_editing[2] = true;
            }
            for _ in 0..3 {
                menu.prepare(display.ui_metrics(), display.settings());
                assert!(
                    menu.draft.as_ref() == Some(&saved),
                    "CLI display values must not edit the draft"
                );
                display
                    .present_with(&frame, |canvas| {
                        menu.paint(canvas)?;
                        if let Some(path) = std::env::var_os("REFORGE_SETTINGS_TEST_IMAGE") {
                            let logical = canvas.logical_size();
                            let viewport = canvas.viewport();
                            let clip = canvas.clip_rect();
                            let scale = canvas.scale();
                            canvas
                                .set_logical_size(
                                    0,
                                    0,
                                    sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED,
                                )
                                .map_err(|error| error.to_string())?;
                            canvas.set_viewport(None);
                            canvas.set_clip_rect(None);
                            canvas.set_scale(1.0, 1.0).map_err(|e| e.to_string())?;
                            canvas
                                .read_pixels(None)
                                .map_err(|error| error.to_string())?
                                .save_bmp(path)
                                .map_err(|error| error.to_string())?;
                            canvas
                                .set_logical_size(logical.0, logical.1, logical.2)
                                .map_err(|error| error.to_string())?;
                            canvas.set_viewport(viewport);
                            canvas.set_clip_rect(clip);
                            canvas
                                .set_scale(scale.0, scale.1)
                                .map_err(|e| e.to_string())?;
                        }
                        Ok(())
                    })
                    .unwrap();
            }
        }
    }
}
