// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent player launcher, independent of reconstructed gameplay loops.
mod layouts;
mod model;
mod preview;
mod settings;
mod switchers;
mod symbolic;
mod widgets;
use crate::{
    detection::{self, Detected},
    launch_handoff,
};
use model::{LayoutVariant, State, VerificationUpdate};
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
    io::IsTerminal,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use widgets::UiAction;

struct LaunchedGame {
    process: Child,
    // Remove an unconsumed ticket after startup failure or early child exit.
    _ticket: Option<launch_handoff::Ticket>,
}

fn global_dead_zone(config: &Path) -> u8 {
    reforge_settings::Session::load(reforge_settings::Launch {
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(config.to_owned()),
        cli: Default::default(),
    })
    .saved
    .global_values()
    .stick_dead_zone
}

fn text_input(canvas: &WindowCanvas, enabled: bool) {
    let keyboard = canvas.window().subsystem().text_input();
    if enabled != keyboard.is_active(canvas.window()) {
        if enabled {
            keyboard.start(canvas.window());
        } else {
            keyboard.stop(canvas.window());
        }
    }
}

fn check_terminal_launch(
    renderer: reforge_settings::Renderer,
    terminal_available: bool,
) -> Result<(), String> {
    if renderer == reforge_settings::Renderer::Terminal && !terminal_available {
        return Err(
            "Terminal rendering requires an attached terminal. Launch REforge from a terminal, or select a window renderer."
                .into(),
        );
    }
    Ok(())
}

impl State {
    fn draw(&mut self, context: &egui::Context, busy: bool) -> Option<UiAction> {
        self.busy = busy;
        layouts::draw(context, self);
        self.pending_action.take()
    }
    fn entry(&self, id: &str) -> Result<Entry, String> {
        self.library
            .entries
            .iter()
            .find(|e| e.entry_id == id)
            .cloned()
            .ok_or_else(|| "Game is no longer in the library".into())
    }
    fn add(&mut self, path: &Path) -> Result<(), String> {
        self.add_with(path, detection::detect, verify_entry)
    }
    fn add_with(
        &mut self,
        path: &Path,
        detect: impl FnOnce(&Path) -> Result<Detected, String>,
        verify: impl FnMut(&Entry) -> Result<Detected, String>,
    ) -> Result<(), String> {
        let detected = detect(path)?;
        let original = library_original(path, &detected)?;
        let id = self.library.add_at(
            detected.game.id(),
            detected.release.id,
            detected.release.edition.title(),
            &original,
            &detected.root,
        )?;
        let title = detected.release.edition.title();
        let update = VerificationUpdate::success(id.clone(), original, detected)?;
        self.selected = Some(id);
        self.reconcile_with(Some(update), verify);
        self.reload_preferences();
        self.notify(format!("{title} added"), false);
        Ok(())
    }
    fn relocate(&mut self, id: String, path: &Path) -> Result<(), String> {
        self.relocate_with(id, path, detection::detect, verify_entry)
    }
    fn relocate_with(
        &mut self,
        id: String,
        path: &Path,
        detect: impl FnOnce(&Path) -> Result<Detected, String>,
        verify: impl FnMut(&Entry) -> Result<Detected, String>,
    ) -> Result<(), String> {
        let entry = self.entry(&id)?;
        let detected = detect_entry_with(&entry, path, detect)?;
        let original = library_original(path, &detected)?;
        self.library
            .relocate_at(&id, detected.release.id, &original, &detected.root)?;
        let update = VerificationUpdate::success(id.clone(), original, detected)?;
        self.selected = Some(id);
        self.reconcile_with(Some(update), verify);
        self.reload_preferences();
        self.notify("Game location updated".into(), false);
        Ok(())
    }
    fn prepare_play(&mut self, id: String) -> Result<(PathBuf, Detected), String> {
        self.prepare_play_with(id, verify_entry)
    }
    fn prepare_play_with(
        &mut self,
        id: String,
        verify: impl FnOnce(&Entry) -> Result<Detected, String>,
    ) -> Result<(PathBuf, Detected), String> {
        let entry = self.entry(&id)?;
        let result = verify(&entry);
        self.apply_verification(&entry, &result);
        result.map(|detected| (entry.original, detected))
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
fn library_original(input: &Path, detected: &Detected) -> Result<PathBuf, String> {
    // Keep an explicit file as the anchor for later verification and launches.
    // Normalize the root only: file and subdirectory symlinks must retain the
    // logical layout beside the game's other catalog directories.
    let selected = if input.is_file() {
        input
    } else {
        &detected.original
    };
    let root = std::path::absolute(&detected.root).map_err(|error| error.to_string())?;
    let selected = std::path::absolute(selected).map_err(|error| error.to_string())?;
    let relative = selected
        .strip_prefix(&root)
        .map_err(|_| "Selected original is outside the detected game root")?;
    Ok(std::fs::canonicalize(&root)
        .map_err(|error| error.to_string())?
        .join(relative))
}

fn detect_entry_with(
    entry: &Entry,
    path: &Path,
    detect: impl FnOnce(&Path) -> Result<Detected, String>,
) -> Result<Detected, String> {
    let detected = detect(path)?;
    let previous = detection::RELEASES
        .iter()
        .find(|r| r.id == entry.release_id && r.game().id() == entry.game_id);
    if !previous.is_some_and(|r| r.compatible_with(detected.release)) {
        return Err(
            "These files belong to an incompatible game or release. Use Add for a separate entry."
                .into(),
        );
    }
    Ok(detected)
}
fn verify_entry(entry: &Entry) -> Result<Detected, String> {
    verify_entry_with(entry, detection::detect)
}
fn verify_entry_with(
    entry: &Entry,
    detect: impl FnOnce(&Path) -> Result<Detected, String>,
) -> Result<Detected, String> {
    let detected = detect_entry_with(entry, &entry.original, detect)?;
    if detected.release.id != entry.release_id {
        return Err("Game files changed release; use Relocate to verify the replacement".into());
    }
    if reforge_settings::library::directory(&detected.root)?
        != reforge_settings::library::directory(&entry.directory)?
    {
        return Err("Game root changed; use Relocate to verify the replacement".into());
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
const MIN_LOGICAL_SIZE: (u32, u32) = (630, 470);

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
            ((MIN_LOGICAL_SIZE.0 as f32 * ratio) as u32).min(size.0),
            ((MIN_LOGICAL_SIZE.1 as f32 * ratio) as u32).min(size.1),
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
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(config),
        cli: Default::default(),
    }
    .install();
    let creator = FrameCreator::for_ui(&canvas);
    drop(guard);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    let mut pads = Gamepads::new(global_dead_zone(&state.config));
    let mut picker: Option<(UiAction, mpsc::Receiver<FolderResult>)> = None;
    let mut child: Option<LaunchedGame> = None;
    let mut visible = false;
    let mut previews = preview::sources::Manager::new(preview::sources::default_cache_path());
    loop {
        for event in events.poll_iter() {
            if matches!(event, Event::RenderDeviceReset { .. }) {
                // Cached previews and the painter's font atlas belong to this
                // renderer. A new context recreates both on the next frame.
                gui = Gui::new(&creator);
                gui.set_focus_overlay(false);
            }
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) {
                return state.save_on_close();
            }
            let pad = pads.handle(&event);
            if child.is_none() && picker.is_none() {
                if let Event::KeyDown {
                    keycode,
                    keymod,
                    repeat,
                    ..
                } = event
                    && state.shortcut(keycode, keymod, repeat)
                {
                    continue;
                }
                if let Some(pad) = pad {
                    gui.handle_gamepad(pad);
                }
                gui.handle_event(&event);
            }
        }
        if let Some(game) = child.as_mut() {
            if let Some(status) = game.process.try_wait().map_err(|e| e.to_string())? {
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
                    let request = request.clone();
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
        previews.poll(&mut state.games);
        gui.set_theme(catalog::ResolvedAppearance::resolve(state.effective_theme()).theme);
        let mut action = None;
        gui.prepare(Gui::metrics(&canvas)?, |ctx| {
            action = state.draw(ctx, picker.is_some());
        });
        text_input(&canvas, picker.is_none() && gui.wants_text_input());
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
        if state.settings_open {
            text_input(&canvas, false);
            if settings::run(&mut canvas, &mut gui, &mut events, &mut pads, &mut state)? {
                return state.save_on_close();
            }
        }
        if let Some(action) = action {
            if state.preview_open {
                state.commit_preview();
            }
            let result = match action {
                UiAction::Add | UiAction::Locate(_) => {
                    text_input(&canvas, false);
                    let initial = if let UiAction::Locate(ref id) = action {
                        state
                            .games
                            .iter()
                            .find(|g| &g.entry_id == id)
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
                    let (original, detected) = state.prepare_play(id.clone())?;
                    let entry = state
                        .games
                        .iter()
                        .find(|g| g.entry_id == id)
                        .ok_or("Game is no longer in the library")?;
                    let preferences = reforge_settings::Session::load(reforge_settings::Launch {
                        game_id: entry.game_id.clone(),
                        entry_id: Some(entry.entry_id.clone()),
                        path: Some(state.config.clone()),
                        cli: Default::default(),
                    });
                    check_terminal_launch(
                        preferences.effective(&preferences.saved).renderer,
                        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
                    )?;
                    let args = state
                        .games
                        .iter()
                        .find(|g| g.entry_id == id)
                        .map(|g| g.native_arguments())
                        .unwrap_or_default();
                    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                    let ticket = match launch_handoff::Ticket::new(&original, &detected) {
                        Ok(ticket) => Some(ticket),
                        Err(error) => {
                            eprintln!(
                                "REforge: launch handoff unavailable ({error}); child will verify original files"
                            );
                            None
                        }
                    };
                    let mut command = Command::new(exe);
                    command
                        .arg("--original")
                        .arg(original)
                        .arg("--config")
                        .arg(&state.config)
                        .args(args)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                    launch_handoff::clear(&mut command);
                    if let Some(ticket) = &ticket {
                        ticket.attach(&mut command);
                    }
                    let process = command
                        .spawn()
                        .map_err(|error| format!("Cannot start game: {error}"))?;
                    text_input(&canvas, false);
                    canvas.window_mut().hide();
                    gui.reset_input();
                    child = Some(LaunchedGame {
                        process,
                        _ticket: ticket,
                    });
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
