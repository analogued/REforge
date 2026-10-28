// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Local game library. Game execution remains the ordinary command-line path.

use crate::detection::{self, Detected};
use reforge_sdl::{
    FrameCreator, egui, gamepad::Gamepads, sdl3, settings::Gui, ui::ButtonActivation,
};
use reforge_settings::library::{Entry, Library};
use sdl3::{
    event::{Event, WindowEvent},
    render::WindowCanvas,
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::mpsc,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Add,
    Play(i64),
    Remove(i64),
    Locate(i64),
}
struct State {
    library: Library,
    selected: Option<i64>,
    error: Option<String>,
    locate: Option<i64>,
    focus_first: bool,
    #[cfg(test)]
    controls: Vec<(String, egui::Rect)>,
}
impl State {
    fn new(path: PathBuf) -> Self {
        let mut library = Library::new(path);
        let error = library.reload().err();
        let selected = library.entries.first().map(|entry| entry.id);
        Self {
            library,
            selected,
            error,
            locate: None,
            focus_first: true,
            #[cfg(test)]
            controls: Vec::new(),
        }
    }
    fn entry(&self, id: i64) -> Result<Entry, String> {
        self.library
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| "Game is no longer in the library".into())
    }
    fn add(&mut self, path: &Path) -> Result<(), String> {
        let detected = detection::detect(path)?;
        let original = std::fs::canonicalize(&detected.original).map_err(|e| e.to_string())?;
        self.selected = Some(self.library.add(
            detected.game.id(),
            detected.release.id,
            detected.game.title(),
            &original,
        )?);
        self.locate = None;
        self.error = None;
        Ok(())
    }
    fn relocate(&mut self, id: i64, path: &Path) -> Result<(), String> {
        let entry = self.entry(id)?;
        let detected = detect_entry(&entry, path)?;
        let original = std::fs::canonicalize(detected.original).map_err(|e| e.to_string())?;
        self.library.relocate(id, &original)?;
        self.selected = Some(id);
        self.locate = None;
        self.error = None;
        Ok(())
    }
    fn remove(&mut self, id: i64) -> Result<(), String> {
        self.library.remove(id)?;
        self.selected = self.library.entries.first().map(|entry| entry.id);
        self.error = None;
        self.locate = None;
        self.focus_first = true;
        Ok(())
    }
    fn play_path(&mut self, id: i64) -> Result<PathBuf, String> {
        let entry = self.entry(id)?;
        if let Err(error) = detect_entry(&entry, &entry.original) {
            self.locate = Some(id);
            return Err(error);
        }
        Ok(entry.original)
    }
    fn refresh(&mut self) {
        if let Err(error) = self.library.reload() {
            self.error = Some(error);
        }
        if !self
            .library
            .entries
            .iter()
            .any(|entry| Some(entry.id) == self.selected)
        {
            self.selected = self.library.entries.first().map(|entry| entry.id);
        }
    }
    fn draw(&mut self, context: &egui::Context, busy: bool) -> Option<Action> {
        let mut action = None;
        let theme = reforge_sdl::ui::current_theme(context);
        #[cfg(test)]
        self.controls.clear();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.canvas)
                    .inner_margin(20.0),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("REforge");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let add = Gui::control(ui.add_enabled(
                            !busy,
                            reforge_sdl::ui::primitives::Button::new(&theme, "Add"),
                        ));
                        #[cfg(test)]
                        self.controls.push(("Add".into(), add.rect));
                        if self.focus_first && self.library.entries.is_empty() {
                            add.request_focus();
                        }
                        if add.activated() {
                            action = Some(Action::Add);
                        }
                    });
                });
                if self.library.entries.is_empty() {
                    if let Some(error) = &self.error {
                        ui.add_space(12.0);
                        ui.colored_label(theme.palette.error, error);
                    }
                    self.focus_first = false;
                    return;
                }
                ui.add_space(12.0);
                if let Some(error) = &self.error {
                    ui.colored_label(theme.palette.error, error);
                    if let Some(id) = self.locate {
                        if Gui::control(ui.add_enabled(
                            !busy,
                            reforge_sdl::ui::primitives::Button::new(&theme, "Locate files…"),
                        ))
                        .activated()
                        {
                            action = Some(Action::Locate(id));
                        }
                    }
                    ui.add_space(8.0);
                }
                let height = (ui.available_height() - 106.0).max(60.0);
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in &self.library.entries {
                            let response = Gui::control(
                                ui.add_enabled(
                                    !busy,
                                    reforge_sdl::ui::primitives::Button::new(&theme, &entry.title)
                                        .selection(if self.selected == Some(entry.id) {
                                            reforge_sdl::ui::SelectionState::Explicit
                                        } else {
                                            reforge_sdl::ui::SelectionState::Unselected
                                        })
                                        .min_size(egui::vec2(ui.available_width(), 38.0)),
                                ),
                            );
                            #[cfg(test)]
                            self.controls
                                .push((format!("game:{}", entry.id), response.rect));
                            if self.focus_first && self.selected == Some(entry.id) {
                                response.request_focus();
                            }
                            if response.activated() || response.has_focus() {
                                self.selected = Some(entry.id);
                            }
                            if response.double_clicked() {
                                action = Some(Action::Play(entry.id));
                            }
                        }
                    });
                self.focus_first = false;
                if let Some(entry) = self
                    .library
                    .entries
                    .iter()
                    .find(|entry| Some(entry.id) == self.selected)
                {
                    ui.add_space(10.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(entry.original.display().to_string())
                                .small()
                                .weak(),
                        )
                        .truncate(),
                    )
                    .on_hover_text(entry.original.display().to_string());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let play = Gui::control(
                            ui.add_enabled(
                                !busy,
                                reforge_sdl::ui::primitives::Button::new(&theme, "Play")
                                    .min_size(egui::vec2(90.0, 28.0)),
                            ),
                        );
                        #[cfg(test)]
                        self.controls.push(("Play".into(), play.rect));
                        if play.activated() {
                            action = Some(Action::Play(entry.id));
                        }
                        if Gui::control(
                            ui.add_enabled(
                                !busy,
                                reforge_sdl::ui::primitives::Button::new(&theme, "Remove")
                                    .min_size(egui::vec2(90.0, 28.0)),
                            ),
                        )
                        .on_hover_text("Remove from the library; keep all game files")
                        .activated()
                        {
                            action = Some(Action::Remove(entry.id));
                        }
                    });
                }
            });
        action
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
fn show_folder(canvas: &WindowCanvas) -> mpsc::Receiver<FolderResult> {
    let (sender, receiver) = mpsc::channel();
    let data = Box::into_raw(Box::new(sender));
    // SAFETY: invoked on SDL's main thread with a live parent window. SDL copies
    // the dialog arguments; the callback owns/frees data and never borrows UI.
    unsafe {
        sdl3::sys::dialog::SDL_ShowOpenFolderDialog(
            Some(folder_result),
            data.cast(),
            canvas.window().raw(),
            std::ptr::null(),
            false,
        );
    }
    receiver
}
fn canvas(video: &sdl3::VideoSubsystem) -> Result<WindowCanvas, String> {
    let mut window = video
        .window("REforge", 720, 480)
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
        (720.0 * ratio).round() as u32,
        (480.0 * ratio).round() as u32,
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
            ((400.0 * ratio) as u32).min(size.0),
            ((280.0 * ratio) as u32).min(size.1),
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
    let mut state = State::new(path);
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let mut canvas = canvas(&video)?;
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    let mut pads = Gamepads::new(35);
    let mut picker: Option<(Action, mpsc::Receiver<FolderResult>)> = None;
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
                return Ok(());
            }
            let pad = pads.handle(&event);
            if child.is_none() {
                if let Some(pad) = pad {
                    gui.handle_gamepad(pad);
                }
                gui.handle_event(&event);
            }
        }
        if let Some(game) = child.as_mut() {
            if let Some(status) = game.try_wait().map_err(|e| e.to_string())? {
                child = None;
                state.error = if status.success() {
                    None
                } else {
                    Some(format!(
                        "Game exited with {status}. See the terminal for details."
                    ))
                };
                state.refresh();
                pads.reset();
                gui.reset_input();
                state.focus_first = true;
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
                    let result = result.and_then(|path| match path {
                        None => Ok(()),
                        Some(path) => match request {
                            Action::Add => state.add(&path),
                            Action::Locate(id) => state.relocate(id, &path),
                            _ => unreachable!(),
                        },
                    });
                    if let Err(error) = result {
                        state.error = Some(error);
                    }
                    pads.reset();
                    gui.reset_input();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    picker = None;
                    state.error = Some("Folder selection could not complete".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut action = None;
        gui.prepare(Gui::metrics(&canvas)?, |context| {
            action = state.draw(context, picker.is_some());
        });
        let background = gui.theme().palette.canvas;
        canvas.set_draw_color(sdl3::pixels::Color::RGB(
            background.r(),
            background.g(),
            background.b(),
        ));
        canvas.clear();
        gui.paint(&mut canvas)?;
        canvas.present();
        if !visible {
            canvas.window_mut().show();
            visible = true;
        }
        if let Some(action) = action {
            let result = match action {
                Action::Add | Action::Locate(_) => {
                    picker = Some((action, show_folder(&canvas)));
                    Ok(())
                }
                Action::Remove(id) => state.remove(id),
                Action::Play(id) => (|| {
                    let original = state.play_path(id)?;
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    let process = Command::new(executable)
                        .arg("--original")
                        .arg(&original)
                        .spawn()
                        .map_err(|e| format!("Cannot start game: {e}"))?;
                    canvas.window_mut().hide();
                    gui.reset_input();
                    child = Some(process);
                    Ok(())
                })(),
            };
            if let Err(error) = result {
                state.error = Some(error);
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_callback_cancellation_and_owned_result() {
        for selected in [None, Some("/synthetic/game")] {
            let (tx, rx) = mpsc::channel::<FolderResult>();
            let text = selected.map(|path| std::ffi::CString::new(path).unwrap());
            let files = [
                text.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                std::ptr::null(),
            ];
            // SAFETY: emulate SDL's single callback with live null-terminated data.
            unsafe {
                folder_result(Box::into_raw(Box::new(tx)).cast(), files.as_ptr(), -1);
            }
            assert_eq!(rx.recv().unwrap().unwrap(), selected.map(PathBuf::from));
        }
    }
    #[test]
    #[ignore = "owns SDL video; run alone with SDL_VIDEODRIVER=dummy"]
    fn launcher_dpi_navigation_and_missing_file_recovery() {
        use reforge_sdl::{
            gamepad::{Actions, PadEvent},
            settings::UiMetrics,
        };
        use sdl3::{
            keyboard::{Keycode, Mod, Scancode},
            mouse::MouseButton,
        };
        let path =
            std::env::temp_dir().join(format!("reforge-launcher-check-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        for density in [1.0, 1.5, 2.25] {
            let size = ((720.0 * density) as u32, (480.0 * density) as u32);
            let window = video
                .window("launcher check", size.0, size.1)
                .hidden()
                .build()
                .unwrap();
            let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
            let creator = FrameCreator::for_ui(&canvas);
            let mut gui = Gui::new(&creator);
            let file = path.join(format!("library-{}.toml", (density * 100.0) as u32));
            let mut state = State::new(file.clone());
            let frame = |state: &mut State, gui: &mut Gui<'_>, canvas: &mut WindowCanvas| {
                let mut action = None;
                gui.prepare(
                    UiMetrics {
                        drawable: size,
                        window: size,
                        pixels_per_point: density,
                    },
                    |ctx| action = state.draw(ctx, false),
                );
                canvas.set_draw_color(sdl3::pixels::Color::RGB(27, 27, 27));
                canvas.clear();
                gui.paint(canvas).unwrap();
                action
            };
            let capture = |canvas: &WindowCanvas, name: &str| {
                if let Some(directory) = std::env::var_os("REFORGE_LAUNCHER_CAPTURE") {
                    let directory = PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    canvas
                        .read_pixels(None)
                        .unwrap()
                        .save_bmp(
                            directory.join(format!("{name}-{}.bmp", (density * 100.0) as u32)),
                        )
                        .unwrap();
                }
            };
            for _ in 0..3 {
                frame(&mut state, &mut gui, &mut canvas);
            }
            assert_eq!(state.controls.len(), 1);
            capture(&canvas, "empty");
            let click = |gui: &mut Gui<'_>, rect: egui::Rect| {
                for down in [true, false] {
                    let p = rect.center() * density;
                    let event = if down {
                        Event::MouseButtonDown {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: p.x,
                            y: p.y,
                        }
                    } else {
                        Event::MouseButtonUp {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: p.x,
                            y: p.y,
                        }
                    };
                    gui.handle_event(&event);
                }
            };
            click(&mut gui, state.controls[0].1);
            assert_eq!(frame(&mut state, &mut gui, &mut canvas), Some(Action::Add));
            // Moving within a held button keeps capture and activates on
            // release through the actual SDL event adapter at each density.
            let start = state.controls[0].1.center() * density;
            gui.handle_event(&Event::MouseButtonDown {
                timestamp: 0,
                window_id: 0,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: start.x,
                y: start.y,
            });
            assert_eq!(frame(&mut state, &mut gui, &mut canvas), None);
            let moved = start + egui::vec2(8.0, 1.0) * density;
            gui.handle_event(&Event::MouseMotion {
                timestamp: 0,
                window_id: 0,
                which: 0,
                mousestate: sdl3::mouse::MouseState::from_sdl_state(1),
                x: moved.x,
                y: moved.y,
                xrel: moved.x - start.x,
                yrel: moved.y - start.y,
            });
            assert_eq!(frame(&mut state, &mut gui, &mut canvas), None);
            gui.handle_event(&Event::MouseButtonUp {
                timestamp: 0,
                window_id: 0,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: moved.x,
                y: moved.y,
            });
            assert_eq!(frame(&mut state, &mut gui, &mut canvas), Some(Action::Add));
            // Input queued before returning from a hidden game/dialog is discarded.
            click(&mut gui, state.controls[0].1);
            gui.reset_input();
            assert_eq!(frame(&mut state, &mut gui, &mut canvas), None);
            let one = state
                .library
                .add("alley-cat", "one", "Alley Cat", &path.join("Alley Cat"))
                .unwrap();
            let two = state
                .library
                .add("mach-3", "two", "Mach 3", &path.join("Mach 3"))
                .unwrap();
            state.selected = Some(one);
            state.focus_first = true;
            for _ in 0..3 {
                frame(&mut state, &mut gui, &mut canvas);
            }
            capture(&canvas, "library");
            gui.handle_event(&Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::Down),
                scancode: Some(Scancode::Down),
                keymod: Mod::NOMOD,
                repeat: false,
                which: 0,
                raw: 0,
            });
            frame(&mut state, &mut gui, &mut canvas);
            assert_eq!(state.selected, Some(two));
            gui.handle_gamepad(PadEvent {
                actions: Actions {
                    up: true,
                    ..Default::default()
                },
                pressed: Actions {
                    up: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            frame(&mut state, &mut gui, &mut canvas);
            assert_eq!(state.selected, Some(one));
            gui.handle_gamepad(PadEvent::default());
            let play = state
                .controls
                .iter()
                .find(|(name, _)| name == "Play")
                .unwrap()
                .1;
            click(&mut gui, play);
            assert_eq!(
                frame(&mut state, &mut gui, &mut canvas),
                Some(Action::Play(one))
            );
            let entries = state.library.entries.clone();
            assert!(state.play_path(one).is_err());
            assert_eq!(state.locate, Some(one));
            assert_eq!(state.library.entries, entries);
            assert!(state.relocate(one, &path.join("missing")).is_err());
            assert_eq!(state.library.entries, entries);
            state.remove(one).unwrap();
            let restored = State::new(file);
            assert_eq!(restored.library.entries.len(), 1);
            assert_eq!(restored.selected, Some(two));
        }
        std::fs::remove_dir_all(path).unwrap();
    }
}
