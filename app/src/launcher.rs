// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Persistent player launcher, independent of reconstructed gameplay loops.
mod layouts;
mod model;
mod settings;
mod switchers;
mod symbolic;
mod widgets;
use crate::detection::{self, Detected};
use model::{LayoutVariant, State};
use reforge_sdl::{
    FrameCreator, egui, gamepad::Gamepads, sdl3, settings::Gui, ui::themes as catalog,
};
use reforge_settings::library::Entry;
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    render::WindowCanvas,
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::mpsc,
    time::Duration,
};
use widgets::UiAction;

impl State {
    fn draw(&mut self, context: &egui::Context, busy: bool) -> Option<UiAction> {
        self.busy = busy;
        layouts::draw(context, self);
        self.pending_action.take()
    }
    fn entry(&self, id: i64) -> Result<Entry, String> {
        self.library
            .entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| "Game is no longer in the library".into())
    }
    fn add(&mut self, path: &Path) -> Result<(), String> {
        let detected = detection::detect(path)?;
        let original = std::fs::canonicalize(&detected.original).map_err(|e| e.to_string())?;
        let id = self.library.add(
            detected.game.id(),
            detected.release.id,
            detected.release.metadata.title,
            &original,
        )?;
        self.selected = Some(id);
        self.refresh();
        self.notify(format!("{} added", detected.release.metadata.title), false);
        Ok(())
    }
    fn relocate(&mut self, id: i64, path: &Path) -> Result<(), String> {
        let entry = self.entry(id)?;
        let detected = detect_entry(&entry, path)?;
        let original = std::fs::canonicalize(&detected.original).map_err(|e| e.to_string())?;
        self.library.relocate(id, &original)?;
        self.selected = Some(id);
        self.refresh();
        self.notify("Game location updated".into(), false);
        Ok(())
    }
    fn play_path(&mut self, id: i64) -> Result<PathBuf, String> {
        let entry = self.entry(id)?;
        if let Err(e) = detect_entry(&entry, &entry.original) {
            self.refresh();
            return Err(e);
        }
        Ok(entry.original)
    }
    fn shortcut(&mut self, key: Option<Keycode>, mods: Mod, repeat: bool) -> bool {
        if repeat {
            return false;
        }
        if self.pending_remove.is_some() {
            if key == Some(Keycode::Escape) {
                self.pending_remove = None;
                return true;
            }
            return false;
        }
        let control = mods.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
        if control && key == Some(Keycode::T) {
            self.toggle_preview();
            return true;
        }
        if self.preview_open && key == Some(Keycode::Escape) {
            self.commit_preview();
            return true;
        }
        let layout = match key {
            Some(Keycode::F2) => Some(LayoutVariant::SplitInspector),
            Some(Keycode::F3) => Some(LayoutVariant::GalleryShelf),
            Some(Keycode::F4) => Some(LayoutVariant::CompactDeck),
            _ => None,
        };
        if let Some(layout) = layout {
            self.set_layout(layout);
            return true;
        }
        false
    }
}
fn detect_entry(entry: &Entry, path: &Path) -> Result<Detected, String> {
    let detected = detection::detect(path)?;
    if detected.game.id() != entry.game_id || detected.release.id != entry.release_id {
        return Err(
            "These files belong to a different game or release. Use Add for a separate entry."
                .into(),
        );
    }
    Ok(detected)
}
type FolderResult = Result<Option<PathBuf>, String>;
// Own the callback data until SDL completes the asynchronous dialog, including
// cancellation/error. Only send owned paths across the callback thread boundary.
unsafe extern "C" fn folder_result(
    userdata: *mut std::ffi::c_void,
    files: *const *const std::ffi::c_char,
    _filter: i32,
) {
    // SAFETY: show_folder transfers exactly one boxed Sender to SDL, which calls
    // this callback once. No references to the launcher/window are retained.
    let sender = unsafe { Box::from_raw(userdata.cast::<mpsc::Sender<FolderResult>>()) };
    let result = if files.is_null() {
        Err(sdl3::get_error().to_string())
    } else {
        // SAFETY: SDL's callback supplies a null-terminated array, valid until
        // this callback returns. Copy the first path before returning.
        let first = unsafe { *files };
        if first.is_null() {
            Ok(None)
        } else {
            // SAFETY: non-null entries are valid null-terminated UTF-8 paths.
            unsafe { std::ffi::CStr::from_ptr(first) }
                .to_str()
                .map(|text| Some(PathBuf::from(text)))
                .map_err(|e| e.to_string())
        }
    };
    let _ = sender.send(result);
}
fn show_folder(canvas: &WindowCanvas, initial: Option<&Path>) -> mpsc::Receiver<FolderResult> {
    let (sender, receiver) = mpsc::channel();
    let data = Box::into_raw(Box::new(sender));
    let initial = initial
        .and_then(|p| if p.is_dir() { Some(p) } else { p.parent() })
        .and_then(|p| p.to_str())
        .and_then(|s| std::ffi::CString::new(s).ok());
    // SAFETY: invoked on SDL's main thread with a live parent window. SDL copies
    // the dialog arguments; the callback owns/frees data and never borrows UI.
    unsafe {
        sdl3::sys::dialog::SDL_ShowOpenFolderDialog(
            Some(folder_result),
            data.cast(),
            canvas.window().raw(),
            initial.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            false,
        );
    }
    receiver
}
fn canvas(video: &sdl3::VideoSubsystem) -> Result<WindowCanvas, String> {
    let mut window = video
        .window("REforge", 880, 560)
        .high_pixel_density()
        .resizable()
        .position_centered()
        .hidden()
        .build()
        .map_err(|e| e.to_string())?;
    let ratio = (window.display_scale() / window.pixel_density()).max(0.5);
    let bounds = window
        .get_display()
        .and_then(|display| display.get_usable_bounds())
        .ok();
    let desired = (
        (880.0 * ratio).round() as u32,
        (560.0 * ratio).round() as u32,
    );
    let size = bounds.map_or(desired, |bounds| {
        (
            desired.0.min(bounds.width().saturating_sub(60).max(1)),
            desired.1.min(bounds.height().saturating_sub(80).max(1)),
        )
    });
    window.set_size(size.0, size.1).map_err(|e| e.to_string())?;
    window
        .set_minimum_size(
            ((630.0 * ratio) as u32).min(size.0),
            ((470.0 * ratio) as u32).min(size.1),
        )
        .map_err(|e| e.to_string())?;
    window.set_position(
        sdl3::video::WindowPos::Centered,
        sdl3::video::WindowPos::Centered,
    );
    // UI uses the ordinary SDL renderer and the same egui painter as settings.
    sdl3::render::create_renderer(window, None).map_err(|e| e.to_string())
}

pub fn run() -> Result<(), String> {
    let path = reforge_settings::library::default_path()
        .ok_or("Cannot locate a configuration directory for the game library")?;
    let config = reforge_settings::default_path().ok_or("Cannot locate a preferences directory")?;
    let mut state = State::new(path, config.clone());
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let mut canvas = canvas(&video)?;
    let guard = reforge_settings::Launch {
        game: "standalone".into(),
        path: Some(config),
        cli: Default::default(),
    }
    .install();
    let creator = FrameCreator::for_ui(&canvas);
    drop(guard);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    let mut pads = Gamepads::new(35);
    let mut picker: Option<(UiAction, mpsc::Receiver<FolderResult>)> = None;
    let mut child: Option<Child> = None;
    let mut visible = false;
    loop {
        for event in events.poll_iter() {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) {
                if state.preview_open {
                    state.commit_preview();
                }
                return Ok(());
            }
            let pad = pads.handle(&event);
            if child.is_none() && picker.is_none() {
                if let Event::KeyDown {
                    keycode,
                    keymod,
                    repeat,
                    ..
                } = event
                {
                    if state.shortcut(keycode, keymod, repeat) {
                        continue;
                    }
                }
                if let Some(pad) = pad {
                    gui.handle_gamepad(pad);
                }
                gui.handle_event(&event);
            }
        }
        if let Some(game) = child.as_mut() {
            if let Some(status) = game.try_wait().map_err(|e| e.to_string())? {
                child = None;
                if !status.success() {
                    state.notify(
                        format!("Game exited with {status}. See the terminal for details."),
                        true,
                    );
                }
                state.refresh();
                pads.reset();
                gui.reset_input();
                visible = false;
            } else {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
        }
        if let Some((request, receiver)) = &picker {
            match receiver.try_recv() {
                Ok(result) => {
                    let request = *request;
                    picker = None;
                    let result = result.and_then(|p| match p {
                        None => Ok(()),
                        Some(path) => match request {
                            UiAction::Add => state.add(&path),
                            UiAction::Locate(id) => state.relocate(id, &path),
                            _ => unreachable!(),
                        },
                    });
                    if let Err(e) = result {
                        state.notify(e, true);
                    }
                    pads.reset();
                    gui.reset_input();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    picker = None;
                    state.notify("Folder selection could not complete".into(), true);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        gui.set_theme(catalog::ResolvedAppearance::resolve(state.effective_theme()).theme);
        let mut action = None;
        gui.prepare(Gui::metrics(&canvas)?, |ctx| {
            action = state.draw(ctx, picker.is_some());
        });
        let bg = catalog::ResolvedAppearance::resolve(state.effective_theme())
            .theme
            .palette
            .canvas;
        canvas.set_draw_color(sdl3::pixels::Color::RGB(bg.r(), bg.g(), bg.b()));
        canvas.clear();
        gui.paint(&mut canvas)?;
        canvas.present();
        if !visible {
            canvas.window_mut().show();
            visible = true;
        }
        if state.settings_open
            && settings::run(&mut canvas, &mut gui, &mut events, &mut pads, &mut state)?
        {
            return Ok(());
        }
        if let Some(action) = action {
            if state.preview_open {
                state.commit_preview();
            }
            let result = match action {
                UiAction::Add | UiAction::Locate(_) => {
                    let initial = if let UiAction::Locate(id) = action {
                        state
                            .games
                            .iter()
                            .find(|g| g.id == id)
                            .map(|g| g.original.as_path())
                    } else {
                        None
                    };
                    picker = Some((action, show_folder(&canvas, initial)));
                    Ok(())
                }
                UiAction::Remove(id) => {
                    state.request_remove(id);
                    Ok(())
                }
                UiAction::OpenSettings => {
                    state.open_settings();
                    Ok(())
                }
                UiAction::Play(id) => (|| {
                    let original = state.play_path(id)?;
                    let args = state
                        .games
                        .iter()
                        .find(|g| g.id == id)
                        .map(|g| g.native_arguments())
                        .unwrap_or_default();
                    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                    let process = Command::new(exe)
                        .arg("--original")
                        .arg(original)
                        .arg("--config")
                        .arg(&state.config)
                        .args(args)
                        .spawn()
                        .map_err(|e| format!("Cannot start game: {e}"))?;
                    canvas.window_mut().hide();
                    gui.reset_input();
                    child = Some(process);
                    Ok(())
                })(),
            };
            if let Err(e) = result {
                state.notify(e, true);
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}
#[cfg(test)]
mod tests;
