// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! SDL host for the same character screen, editor and native services as terminal play.

use crate::{
    display,
    input::Key,
    player::{Loaded, bios_ticks},
    services::{HostReply, HostRequest},
    session::SessionStep,
};
use reforge_sdl::{
    DisplayGeometry, FrameFormat, FrameTexture, HostPause, HostShortcut, Window, egui, game_title,
    gamepad::{Gamepads, PadEvent},
    host_shortcut,
    sdl3::{
        event::{Event, WindowEvent},
        keyboard::{Keycode, Mod},
    },
    settings::{Gui, SettingsMenu, VideoCapabilities},
};
use reforge_settings::RuntimeSettings;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

pub const GEOMETRY: DisplayGeometry = DisplayGeometry::new((640, 200), (640, 480));

/// The queue retains text after Enter for the next original keyboard poll.
#[derive(Default)]
struct Keyboard {
    keys: VecDeque<Key>,
}
impl Keyboard {
    fn event(&mut self, event: &Event) {
        match event {
            Event::TextInput { text, .. } => {
                self.keys
                    .extend(text.chars().filter_map(|character| match character {
                        '\r' | '\n' => Some(Key::Enter),
                        ' '..='~' => Some(Key::Character(character as u8)),
                        _ => None,
                    }));
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                ..
            } => {
                if let Some(key) = special_key(*key, *keymod) {
                    self.keys.push_back(key);
                }
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => self.keys.clear(),
            _ => {}
        }
    }
    fn batch(&mut self) -> Vec<Key> {
        let mut batch = Vec::new();
        while let Some(key) = self.keys.pop_front() {
            batch.push(key);
            if key == Key::Enter {
                break;
            }
        }
        batch
    }
}

fn special_key(key: Keycode, modifiers: Mod) -> Option<Key> {
    let control = Mod::LCTRLMOD | Mod::RCTRLMOD;
    let other = Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD;
    if modifiers.intersects(other) {
        return None;
    }
    if modifiers.intersects(control) {
        return match key {
            Keycode::S => Some(Key::Pause),
            Keycode::G => Some(Key::Character(7)),
            _ => None,
        };
    }
    match key {
        Keycode::Return | Keycode::KpEnter | Keycode::Return2 => Some(Key::Enter),
        Keycode::Backspace | Keycode::Left => Some(Key::Backspace),
        Keycode::Tab => Some(Key::Tab),
        Keycode::Home | Keycode::F5 => Some(Key::EraseLine),
        Keycode::End | Keycode::F3 => Some(Key::RecallLine),
        Keycode::Right | Keycode::F1 => Some(Key::RecallCharacter),
        Keycode::Escape => Some(Key::Pause),
        _ => None,
    }
}

struct FilePrompt {
    request: HostRequest,
    label: String,
    path: String,
    focus: bool,
}

#[derive(Default)]
struct Overlay {
    prompt: Option<FilePrompt>,
    error: Option<String>,
    retry: Option<HostRequest>,
    back: bool,
}

enum OverlayAction {
    Submit(Option<PathBuf>),
    DismissError,
    Retry,
    Exit,
}

impl Overlay {
    fn open(&self) -> bool {
        self.prompt.is_some() || self.error.is_some()
    }

    fn file_error(&mut self, request: &HostRequest, label: Option<String>, error: String) {
        self.prompt = label.map(|label| FilePrompt {
            request: request.clone(),
            label,
            path: String::new(),
            focus: true,
        });
        self.retry = self.prompt.is_none().then(|| request.clone());
        self.error = Some(error);
    }

    fn show(&mut self, context: &egui::Context) -> Option<OverlayAction> {
        let mut action = None;
        let back = std::mem::take(&mut self.back);
        if let Some(prompt) = &mut self.prompt {
            egui::Window::new("Game files")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .default_width(440.0)
                .show(context, |ui| {
                    ui.label(&prompt.label);
                    let field = Gui::text_edit(
                        ui.add(
                            egui::TextEdit::singleline(&mut prompt.path)
                                .hint_text("Filename or path")
                                .desired_width(f32::INFINITY),
                        ),
                    );
                    if prompt.focus {
                        field.request_focus();
                        prompt.focus = false;
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    let escape = back || ui.input(|input| input.key_pressed(egui::Key::Escape));
                    ui.horizontal(|ui| {
                        if Gui::control(ui.button("Continue")).clicked() || enter {
                            action = Some(OverlayAction::Submit(
                                (!prompt.path.is_empty()).then(|| PathBuf::from(&prompt.path)),
                            ));
                        }
                        if Gui::control(ui.button("Cancel")).clicked() || escape {
                            action = Some(OverlayAction::Submit(None));
                        }
                    });
                });
        } else if let Some(error) = &self.error {
            egui::Window::new("Game file error")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .default_width(440.0)
                .show(context, |ui| {
                    ui.label(error);
                    if self.retry.is_some() {
                        ui.horizontal(|ui| {
                            if Gui::control(ui.button("Retry")).clicked()
                                || ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                action = Some(OverlayAction::Retry);
                            }
                            if Gui::control(ui.button("Close game")).clicked() {
                                action = Some(OverlayAction::Exit);
                            }
                        });
                    } else if Gui::control(ui.button("Continue")).clicked()
                        || back
                        || ui.input(|input| {
                            input.key_pressed(egui::Key::Enter)
                                || input.key_pressed(egui::Key::Escape)
                        })
                    {
                        action = Some(OverlayAction::DismissError);
                    }
                });
        }
        action
    }
}

/// Controller actions belong to host menus, never to the story's text editor.
fn route_gamepad(
    pad: PadEvent,
    menu: &mut SettingsMenu<'_>,
    gui: &mut Gui<'_>,
    overlay: &mut Overlay,
) -> bool {
    if overlay.open() {
        overlay.back |= pad.cancel;
        gui.handle_gamepad(pad);
        true
    } else {
        menu.handle_gamepad(pad)
    }
}

fn complete_file_request(
    loaded: &mut Loaded,
    request: &HostRequest,
    path: Option<PathBuf>,
    overlay: &mut Overlay,
) -> Result<(), String> {
    match loaded.files.handle_with_path(request, path) {
        Ok(Some(reply)) => {
            overlay.retry = None;
            if let Err(error) = loaded.session.complete_host(reply) {
                if matches!(request, HostRequest::LoadBookmark) {
                    loaded
                        .session
                        .complete_host(HostReply::Cancelled)
                        .map_err(|e| e.to_string())?;
                    overlay.error = Some(format!("Cannot restore bookmark: {error}"));
                } else {
                    return Err(error.to_string());
                }
            }
            if let Some(error) = loaded.files.take_error() {
                overlay.error = Some(error);
            }
        }
        Ok(None) => {
            // Missing media cannot be acknowledged as a successful transition.
            // Leave the request pending and offer another path or window close.
            overlay.file_error(
                request,
                loaded.files.prompt(request),
                "This game file is required to continue. Choose its location, or close the game window.".into(),
            );
        }
        Err(error) => {
            overlay.file_error(request, loaded.files.prompt(request), error.to_string());
        }
    }
    Ok(())
}

pub fn run(mut loaded: Loaded, preferences: RuntimeSettings) -> Result<(), String> {
    let mut window = Window::new_with_geometry(
        &game_title("Mindwheel", None),
        GEOMETRY,
        preferences.scale,
        preferences.presentation,
    )?;
    window.apply_settings(preferences)?;
    window.start_text_input();
    let creator = window.texture_creator();
    let mut audio = crate::sound::Player::new(creator.audio_playback())?;
    let mut frame = FrameTexture::new(
        &creator,
        display::WIDTH as u32,
        display::HEIGHT as u32,
        FrameFormat::Rgb,
    )?;
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
        .with_btz_options(false)
        .with_audio_renderers(&[reforge_settings::SoundRenderer::Speaker])
        .with_audio_categories(&[reforge_settings::SoundCategory::Effects]);
    let mut gui = Gui::new(&creator);
    let mut overlay = Overlay::default();
    let mut keyboard = Keyboard::default();
    let mut gamepads = Gamepads::new(window.settings().stick_dead_zone);
    let epoch = Instant::now();
    let mut pause = HostPause::default();
    let mut manual_pause = false;
    let mut previous_cells = Vec::new();
    let mut previous_phase = None;
    let mut previous_status = "";
    loop {
        let was_menu = menu.is_open();
        gamepads.set_dead_zone(window.settings().stick_dead_zone);
        for event in window.poll_events() {
            let pad = gamepads.handle(&event);
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) || host_shortcut(&event) == Some(HostShortcut::Quit)
            {
                return Ok(());
            }
            let pad_consumed =
                pad.is_some_and(|pad| route_gamepad(pad, &mut menu, &mut gui, &mut overlay));
            if overlay.open() {
                gui.handle_event(&event);
            } else if menu.handle_event(&event) || pad_consumed {
                keyboard.keys.clear();
            } else if host_shortcut(&event) == Some(HostShortcut::TogglePause) {
                manual_pause = !manual_pause;
                keyboard.keys.clear();
            } else if !manual_pause {
                keyboard.event(&event);
            }
        }
        if was_menu && !menu.is_open() {
            window.start_text_input();
        }
        if was_menu != menu.is_open() {
            gamepads.reset();
        }
        let paused = manual_pause || menu.is_open() || overlay.open();
        if paused != pause.is_paused() {
            pause.toggle(Instant::now());
        }
        if let Some(requested) = menu.prepare(window.ui_metrics(), window.settings()) {
            match window.apply_settings(requested) {
                Ok(()) => {
                    menu.applied();
                    window.start_text_input();
                }
                Err(error) => menu.set_error(error),
            }
        }
        if overlay.open() {
            let mut action = None;
            gui.prepare(window.ui_metrics(), |context| {
                action = overlay.show(context)
            });
            if let Some(action) = action {
                match action {
                    OverlayAction::DismissError => overlay.error = None,
                    OverlayAction::Exit => return Ok(()),
                    OverlayAction::Retry => {
                        let request = overlay.retry.take().expect("retryable file request");
                        overlay.error = None;
                        complete_file_request(&mut loaded, &request, None, &mut overlay)?;
                    }
                    OverlayAction::Submit(path) => {
                        let prompt = overlay.prompt.take().expect("submitted file prompt");
                        overlay.error = None;
                        complete_file_request(&mut loaded, &prompt.request, path, &mut overlay)?;
                    }
                }
                gui.reset_input();
                keyboard.keys.clear();
                gamepads.reset();
            }
        } else if !paused {
            // A bounded slice only yields to SDL; it is never a lifetime limit.
            let slice = Instant::now();
            for _ in 0..4096 {
                let elapsed = pause.now(Instant::now()).saturating_duration_since(epoch);
                loaded.session.set_bios_ticks(bios_ticks(elapsed));
                let mut submitted = loaded.session.input().line_ready();
                let step = loaded
                    .session
                    .advance_with_keyboard(&mut || {
                        if submitted {
                            return Ok(Vec::new());
                        }
                        let keys = keyboard.batch();
                        submitted |= keys.last() == Some(&Key::Enter);
                        Ok(keys)
                    })
                    .map_err(|error| error.to_string())?;
                match step {
                    SessionStep::WaitKey => {
                        if let Some(key) = keyboard.keys.pop_front() {
                            loaded.session.key(key).map_err(|error| error.to_string())?;
                        } else {
                            break;
                        }
                    }
                    SessionStep::ReadLine { .. } if !loaded.session.input().line_ready() => break,
                    SessionStep::Host(request) => {
                        if let Some(label) = loaded.files.prompt(&request) {
                            overlay.prompt = Some(FilePrompt {
                                request,
                                label,
                                path: String::new(),
                                focus: true,
                            });
                            keyboard.keys.clear();
                            gui.reset_input();
                            gamepads.reset();
                        } else {
                            complete_file_request(&mut loaded, &request, None, &mut overlay)?;
                        }
                        if overlay.open() {
                            break;
                        }
                    }
                    SessionStep::Exited { .. } => return Ok(()),
                    SessionStep::Unsupported(request) => {
                        return Err(format!("Unimplemented game request: {request:?}"));
                    }
                    _ => {}
                }
                if slice.elapsed() >= Duration::from_millis(2) {
                    break;
                }
            }
        }
        // Begin the modal pause before presentation can block on VSync.
        if overlay.open() && !pause.is_paused() {
            pause.toggle(Instant::now());
        }
        audio.service(
            loaded.session.take_bells(),
            pause.now(Instant::now()).saturating_duration_since(epoch),
            paused || overlay.open(),
        );
        let blink_on = (epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let cells = loaded.session.screen().dos_cells();
        if previous_cells != cells || previous_phase != Some(blink_on) {
            let pixels = display::render(cells, blink_on)?;
            frame.update(&pixels)?;
            previous_cells.clear();
            previous_cells.extend_from_slice(cells);
            previous_phase = Some(blink_on);
        }
        let status = if overlay.open() {
            "Game files"
        } else if menu.is_open() {
            "Settings"
        } else if manual_pause {
            "Paused · Pause to resume"
        } else {
            "F10 settings · Pause pauses · Ctrl+Q exits"
        };
        if previous_status != status {
            window.set_title(&game_title("Mindwheel", Some(status)))?;
            previous_status = status;
        }
        if window.presentation_due() {
            window.present_with(&frame, |canvas| {
                if overlay.open() {
                    gui.paint(canvas)
                } else {
                    menu.paint(canvas)
                }
            })?;
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Keycode, modifiers: Mod) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            raw: 0,
            scancode: None,
            keycode: Some(key),
            keymod: modifiers,
            repeat: false,
        }
    }

    #[test]
    fn text_input_is_not_duplicated_by_printable_keydown() {
        let mut keyboard = Keyboard::default();
        keyboard.event(&press(Keycode::A, Mod::NOMOD));
        keyboard.event(&Event::TextInput {
            timestamp: 0,
            window_id: 1,
            text: "aB".into(),
        });
        assert_eq!(
            keyboard.batch(),
            vec![Key::Character(b'a'), Key::Character(b'B')]
        );
    }

    #[test]
    fn native_editor_aliases_and_game_pause_do_not_become_window_shortcuts() {
        for key in [Keycode::Left, Keycode::Backspace] {
            assert_eq!(special_key(key, Mod::NOMOD), Some(Key::Backspace));
        }
        for key in [Keycode::F1, Keycode::Right] {
            assert_eq!(special_key(key, Mod::NOMOD), Some(Key::RecallCharacter));
        }
        assert_eq!(special_key(Keycode::Escape, Mod::NOMOD), Some(Key::Pause));
        assert_eq!(special_key(Keycode::S, Mod::LCTRLMOD), Some(Key::Pause));
        for control in [Mod::LCTRLMOD, Mod::RCTRLMOD] {
            assert_eq!(special_key(Keycode::G, control), Some(Key::Character(7)));
        }
        assert_eq!(special_key(Keycode::G, Mod::LALTMOD), None);
        assert_eq!(special_key(Keycode::G, Mod::LCTRLMOD | Mod::RALTMOD), None);
        assert_eq!(special_key(Keycode::S, Mod::LALTMOD), None);
    }

    #[test]
    fn submitted_line_leaves_typeahead_for_the_next_poll() {
        let mut keyboard = Keyboard::default();
        keyboard.event(&Event::TextInput {
            timestamp: 0,
            window_id: 1,
            text: "look\rinventory".into(),
        });
        assert_eq!(
            keyboard.batch(),
            vec![
                Key::Character(b'l'),
                Key::Character(b'o'),
                Key::Character(b'o'),
                Key::Character(b'k'),
                Key::Enter
            ]
        );
        assert_eq!(keyboard.batch().len(), 9);
    }

    #[test]
    fn transcript_write_error_retries_pending_request_without_a_filename_prompt() {
        let mut files =
            crate::player::files::Files::new(std::path::Path::new("ADISK.OBJ")).unwrap();
        let request = HostRequest::WriteTranscript {
            bytes: b"text".to_vec(),
        };
        // An absent writer is a synthetic nonprompt failure. A chosen filename
        // cannot repair this operation; the same VM request must remain pending.
        let error = files.handle_with_path(&request, None).unwrap_err();
        let mut overlay = Overlay::default();
        overlay.file_error(&request, files.prompt(&request), error.to_string());
        assert!(overlay.open());
        assert!(overlay.prompt.is_none());
        assert_eq!(overlay.retry, Some(request));
        assert!(overlay.error.is_some());

        for request in [HostRequest::ReadSideMap, HostRequest::CloseTranscript] {
            overlay.file_error(&request, files.prompt(&request), "I/O failure".into());
            assert!(overlay.prompt.is_none());
            assert_eq!(overlay.retry, Some(request));
        }
    }

    #[test]
    fn controller_navigation_reaches_settings_and_file_dialogs() {
        const TEST: &str =
            "windowed::tests::controller_navigation_reaches_settings_and_file_dialogs";
        const MARKER: &str = "REFORGE_BTZ_CONTROLLER_TEST";
        if std::env::var(MARKER).as_deref() != Ok(TEST) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--test-threads=1"])
                .env(MARKER, TEST)
                .env("SDL_VIDEODRIVER", "dummy")
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
        let _guard = reforge_settings::Launch {
            game_id: "mindwheel".into(),
            entry_id: None,
            path: None,
            cli: Default::default(),
        }
        .install();
        let sdl = reforge_sdl::sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video
            .window("BTZ controller regression", 640, 480)
            .hidden()
            .build()
            .unwrap();
        let canvas = window.into_canvas();
        let creator = reforge_sdl::FrameCreator::for_ui(&canvas);
        let mut menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_btz_options(false);
        let mut gui = Gui::new(&creator);
        let mut overlay = Overlay::default();
        let metrics = Gui::metrics(&canvas).unwrap();

        assert!(!route_gamepad(
            PadEvent::default(),
            &mut menu,
            &mut gui,
            &mut overlay
        ));
        assert!(route_gamepad(
            PadEvent {
                settings: true,
                ..Default::default()
            },
            &mut menu,
            &mut gui,
            &mut overlay
        ));
        assert!(menu.is_open());
        for _ in 0..3 {
            menu.prepare(metrics, RuntimeSettings::default());
        }
        assert!(route_gamepad(
            PadEvent {
                next: true,
                ..Default::default()
            },
            &mut menu,
            &mut gui,
            &mut overlay
        ));
        menu.prepare(metrics, RuntimeSettings::default());
        route_gamepad(
            PadEvent {
                cancel: true,
                ..Default::default()
            },
            &mut menu,
            &mut gui,
            &mut overlay,
        );
        assert!(!menu.is_open());

        overlay.prompt = Some(FilePrompt {
            request: HostRequest::LoadBookmark,
            label: "Bookmark".into(),
            path: "save.dat".into(),
            focus: true,
        });
        let mut action = None;
        for _ in 0..3 {
            gui.prepare(metrics, |ctx| {
                action = overlay.show(ctx);
            });
        }
        assert!(action.is_none());
        // Filename editing must keep its caret keys instead of moving UI focus.
        gui.handle_event(&press(Keycode::End, Mod::NOMOD));
        gui.handle_event(&press(Keycode::Left, Mod::NOMOD));
        gui.handle_event(&Event::TextInput {
            timestamp: 0,
            window_id: 1,
            text: "X".into(),
        });
        gui.prepare(metrics, |ctx| {
            action = overlay.show(ctx);
        });
        assert!(action.is_none());
        assert_eq!(overlay.prompt.as_ref().unwrap().path, "save.daXt");

        gui.handle_event(&press(Keycode::Right, Mod::NOMOD));
        gui.handle_event(&Event::TextInput {
            timestamp: 0,
            window_id: 1,
            text: "Y".into(),
        });
        gui.prepare(metrics, |ctx| {
            action = overlay.show(ctx);
        });
        assert_eq!(overlay.prompt.as_ref().unwrap().path, "save.daXtY");
        gui.handle_event(&press(Keycode::Left, Mod::LSHIFTMOD));
        gui.handle_event(&Event::TextInput {
            timestamp: 0,
            window_id: 1,
            text: "Z".into(),
        });
        gui.prepare(metrics, |ctx| {
            action = overlay.show(ctx);
        });
        assert_eq!(overlay.prompt.as_ref().unwrap().path, "save.daXtZ");

        // The field and both buttons must participate in shared navigation.
        for _ in 0..2 {
            assert!(route_gamepad(
                PadEvent {
                    next: true,
                    ..Default::default()
                },
                &mut menu,
                &mut gui,
                &mut overlay
            ));
            gui.prepare(metrics, |ctx| {
                action = overlay.show(ctx);
            });
            assert!(action.is_none());
        }
        route_gamepad(
            PadEvent {
                pressed: reforge_sdl::gamepad::Actions {
                    action: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut menu,
            &mut gui,
            &mut overlay,
        );
        gui.prepare(metrics, |ctx| {
            action = overlay.show(ctx);
        });
        assert!(
            matches!(action, Some(OverlayAction::Submit(None))),
            "controller A did not activate Cancel"
        );
        assert!(!menu.is_open());

        gui.reset_input();
        route_gamepad(
            PadEvent {
                cancel: true,
                ..Default::default()
            },
            &mut menu,
            &mut gui,
            &mut overlay,
        );
        gui.prepare(metrics, |ctx| {
            action = overlay.show(ctx);
        });
        assert!(
            matches!(action, Some(OverlayAction::Submit(None))),
            "controller B did not cancel the filename prompt"
        );
        assert_eq!(overlay.prompt.as_ref().unwrap().path, "save.daXtZ");
    }
}
