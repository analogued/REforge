// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! SDL events and host replies for the engine's character-cell file browser.

use super::*;
use crate::player::file_dialog::{Action, FileDialog, Input, Operation};
use reforge_sdl::sdl3::mouse::{MouseButton, MouseWheelDirection};

pub(super) struct FileBrowser {
    request: HostRequest,
    pub(super) dialog: FileDialog,
    held: Option<Input>,
    repeat_at: Instant,
}

impl FileBrowser {
    /// Settings owns input above a suspended chooser. Consume the closing event
    /// as well, so Escape cannot also cancel the underlying file operation.
    pub(super) fn event_with_settings(
        &mut self,
        event: &Event,
        pad: Option<PadEvent>,
        menu: &mut SettingsMenu<'_>,
        window: &Window,
    ) -> Option<Action> {
        let was_open = menu.is_open();
        let pad_consumed = pad.is_some_and(|pad| menu.handle_gamepad(pad));
        if menu.handle_event(event) || pad_consumed || was_open || menu.is_open() {
            self.held = None;
            return None;
        }
        pad.and_then(|pad| self.pad(pad))
            .or_else(|| self.event(event, window))
    }

    pub(super) fn begin(loaded: &Loaded, request: &HostRequest, title: &str) -> Option<Self> {
        let (operation, fixed) = match request {
            HostRequest::SaveBookmark { .. } => {
                (Operation::Save, loaded.files.bookmark_path.as_deref())
            }
            HostRequest::LoadBookmark => (Operation::Load, loaded.files.bookmark_path.as_deref()),
            HostRequest::TranscriptName { .. } => (
                Operation::Transcript,
                loaded.files.transcript_path.as_deref(),
            ),
            _ => return None,
        };
        let extent = loaded.session.vm().state().variables.len();
        let dialog = if let Some(path) = fixed {
            // Fixed file options keep their launch-directory semantics and skip
            // selection, but existing write destinations still need a decision.
            if operation == Operation::Load || !path.exists() {
                return None;
            }
            FileDialog::fixed(operation, path, title, extent)
        } else {
            FileDialog::new(operation, &loaded.files.save_directory, title, extent)
        };
        Some(Self {
            request: request.clone(),
            dialog,
            held: None,
            repeat_at: Instant::now(),
        })
    }

    pub(super) fn event(&mut self, event: &Event, window: &Window) -> Option<Action> {
        match event {
            Event::TextInput { text, .. } => self.dialog.input(Input::Text(text.clone())),
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                ..
            } => {
                if keymod.intersects(Mod::LALTMOD | Mod::RALTMOD) {
                    return None;
                }
                let select = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD) {
                    if !self.dialog.editing_name() {
                        return None;
                    }
                    match *key {
                        Keycode::A => return self.dialog.input(Input::SelectAll),
                        Keycode::V => match window.clipboard_text() {
                            Ok(text) => return self.dialog.input(Input::Text(text)),
                            Err(error) => self.dialog.error(format!("Cannot paste: {error}")),
                        },
                        Keycode::C | Keycode::X => {
                            if let Some(text) = self.dialog.selected_text() {
                                match window.set_clipboard_text(&text) {
                                    Ok(()) if *key == Keycode::X => {
                                        return self.dialog.input(Input::Delete);
                                    }
                                    Err(error) => {
                                        self.dialog.error(format!("Cannot copy: {error}"))
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                let input = match *key {
                    Keycode::Up => Input::Up,
                    Keycode::Down => Input::Down,
                    Keycode::PageUp => Input::PageUp,
                    Keycode::PageDown => Input::PageDown,
                    Keycode::Left => Input::Left(select),
                    Keycode::Right => Input::Right(select),
                    Keycode::Home => Input::Home(select),
                    Keycode::End => Input::End(select),
                    Keycode::Backspace => Input::Backspace,
                    Keycode::Delete => Input::Delete,
                    Keycode::Tab => Input::Tab(select),
                    Keycode::Escape => Input::Escape,
                    Keycode::Return | Keycode::KpEnter | Keycode::Return2 => Input::Enter,
                    _ => return None,
                };
                self.dialog.input(input)
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                clicks,
                ..
            } => window.frame_point(*x, *y, (640, 200)).and_then(|(x, y)| {
                self.dialog
                    .click(x as usize / 8, y as usize / 8, *clicks >= 2)
            }),
            Event::MouseWheel { y, direction, .. } => {
                let lines = (*y * 3.0) as i32;
                self.dialog
                    .scroll(if *direction == MouseWheelDirection::Flipped {
                        lines
                    } else {
                        -lines
                    });
                None
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.held = None;
                None
            }
            _ => None,
        }
    }

    pub(super) fn pad(&mut self, pad: PadEvent) -> Option<Action> {
        let held = if pad.actions.up {
            Some(Input::Up)
        } else if pad.actions.down {
            Some(Input::Down)
        } else if pad.actions.left {
            Some(Input::Left(false))
        } else if pad.actions.right {
            Some(Input::Right(false))
        } else {
            None
        };
        let changed = held != self.held;
        self.held = held.clone();
        if changed {
            self.repeat_at = Instant::now() + Duration::from_millis(350);
        }
        let input = if pad.cancel {
            Input::Escape
        } else if pad.previous {
            Input::Tab(true)
        } else if pad.next {
            Input::Tab(false)
        } else if pad.pressed.action {
            Input::Enter
        } else if changed {
            held?
        } else {
            return None;
        };
        self.dialog.input(input)
    }
    pub(super) fn tick(&mut self) {
        if Instant::now() >= self.repeat_at {
            if let Some(input) = self.held.clone() {
                self.dialog.input(input);
            }
            self.repeat_at = Instant::now() + Duration::from_millis(100);
        }
    }

    /// Errors leave the original host operation outstanding, so the user can
    /// correct a filename or choose another bookmark without losing the browser.
    pub(super) fn complete(&mut self, loaded: &mut Loaded, action: Action) -> Result<bool, String> {
        match action {
            Action::Cancel => {
                loaded
                    .session
                    .complete_host(HostReply::Cancelled)
                    .map_err(|e| e.to_string())?;
                Ok(true)
            }
            Action::Choose { path, replace } => {
                match loaded
                    .files
                    .handle_browser_path(&self.request, path, replace)
                {
                    Ok(Some(reply)) => {
                        if let Some(error) = loaded.files.take_error() {
                            self.dialog.error(error);
                            return Ok(false);
                        }
                        match loaded.session.complete_host(reply) {
                            Ok(()) => Ok(true),
                            Err(error) if matches!(self.request, HostRequest::LoadBookmark) => {
                                self.dialog
                                    .error(format!("Cannot restore bookmark: {error}"));
                                Ok(false)
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    }
                    Ok(None) => {
                        self.dialog
                            .error("The file operation is still waiting for a filename.".into());
                        Ok(false)
                    }
                    Err(error) => {
                        self.dialog.error(error.to_string());
                        Ok(false)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        image::StoryImage,
        player::{file_dialog::tests::Directory, files::Files},
        random::DosRandom,
        services::SideMap,
        session::Session,
        text::TextCodec,
    };
    fn put(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn loaded(dir: &Directory) -> Loaded {
        let mut bytes = vec![0; 7 * 512];
        put(&mut bytes, 0, 512);
        for (offset, value) in [
            (0xe8, 0x40),
            (0xdc, 0xffff),
            (0xde, 0xffff),
            (0x112, 0xffff),
            (0x102, 7),
        ] {
            put(&mut bytes, offset, value);
        }
        for (offset, value) in [
            (4, 1),
            (14, 3),
            (12, 4),
            (8, 5),
            (10, 6),
            (16, 3),
            (26, 4),
            (24, 5),
            (20, 6),
            (22, 7),
        ] {
            put(&mut bytes, 512 + offset, value);
        }
        for (index, word) in [0x64, 5, 0x64, 6, 0x64, 4].into_iter().enumerate() {
            put(&mut bytes, 512 + 0x40 + index * 2, word);
        }
        let mut exe = vec![0; 32 + 0x6cb];
        exe[..2].copy_from_slice(b"MZ");
        put(&mut exe, 8, 2);
        let mut session = Session::new(
            StoryImage::from_dos_obj(bytes).unwrap(),
            TextCodec::from_dos_executable(&exe).unwrap(),
            DosRandom::from_dos_executable(&exe).unwrap(),
        )
        .unwrap();
        session.set_side_map(
            SideMap::from_bytes(&[b"0\na\n".repeat(10), b"52\n".to_vec()].concat()).unwrap(),
        );
        Loaded {
            session,
            files: Files::new(&dir.0.join("ADISK.OBJ")).unwrap(),
            presentation: None,
        }
    }
    fn host(loaded: &mut Loaded) -> HostRequest {
        for _ in 0..100 {
            if let SessionStep::Host(request) = loaded.session.advance().unwrap() {
                return request;
            }
        }
        panic!("no host operation")
    }
    fn key(key: Keycode, keymod: Mod) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            which: 0,
            raw: 0,
            scancode: None,
            keycode: Some(key),
            keymod,
            repeat: false,
        }
    }

    #[test]
    fn browser_writes_restores_retries_and_never_paints_into_the_story_screen() {
        let dir = Directory::new();
        let mut loaded = loaded(&dir);
        let request = host(&mut loaded);
        let mut browser = FileBrowser::begin(&loaded, &request, "Mindwheel").unwrap();
        let screen = loaded.session.screen().dos_cells().to_vec();
        let variables = loaded.session.vm().state().variables.clone();
        let ip = loaded.session.vm().state().ip;
        let random = loaded.session.random_state();
        browser.dialog.input(Input::Text("first-save".into()));
        browser.dialog.cells();
        browser
            .dialog
            .set_cursor_style(reforge_settings::FileCursorStyle::SteadyBlock);
        assert_eq!(loaded.session.screen().dos_cells(), screen);
        assert_eq!(loaded.session.vm().state().variables, variables);
        assert_eq!(loaded.session.vm().state().ip, ip);
        assert_eq!(loaded.session.random_state(), random);
        // An I/O failure is recoverable without returning from the pending save.
        assert!(
            !browser
                .complete(
                    &mut loaded,
                    Action::Choose {
                        path: dir.0.join("missing/file"),
                        replace: false
                    }
                )
                .unwrap()
        );
        assert_eq!(host(&mut loaded), request);
        let action = browser.dialog.input(Input::Enter).unwrap();
        assert!(browser.complete(&mut loaded, action).unwrap());
        let path = dir.0.join("first-save");
        assert!(path.is_file());
        assert_eq!(
            crate::bookmark::Bookmark::decode(&std::fs::read(&path).unwrap(), 512)
                .unwrap()
                .variables,
            variables
        );
        let request = host(&mut loaded);
        assert_eq!(request, HostRequest::LoadBookmark);
        let mut browser = FileBrowser::begin(&loaded, &request, "Mindwheel").unwrap();
        let action = browser.dialog.input(Input::Enter).unwrap();
        assert!(browser.complete(&mut loaded, action).unwrap());
        assert!(matches!(host(&mut loaded), HostRequest::Exit { code: 0 }));
    }

    #[test]
    fn confirmed_paths_do_not_double_prompt_and_original_inputs_remain_protected() {
        let dir = Directory::new();
        let mut loaded = loaded(&dir);
        loaded.files.confirm_bookmark_replace = true;
        let request = host(&mut loaded);
        let path = dir.0.join("existing");
        std::fs::write(&path, b"before").unwrap();
        assert!(
            loaded
                .files
                .handle_browser_path(&request, path.clone(), false)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"before");
        let mut browser = FileBrowser::begin(&loaded, &request, "Breakers").unwrap();
        assert!(
            browser
                .complete(
                    &mut loaded,
                    Action::Choose {
                        path: path.clone(),
                        replace: true
                    }
                )
                .unwrap()
        );
        assert!(!loaded.files.replacement_pending());
        let mut loaded = super::tests::loaded(&dir);
        let request = host(&mut loaded);
        let original = dir.0.join("ADISK.OBJ");
        std::fs::write(&original, b"original").unwrap();
        loaded.files.originals.push(original.clone());
        let mut browser = FileBrowser::begin(&loaded, &request, "Mindwheel").unwrap();
        assert!(
            !browser
                .complete(
                    &mut loaded,
                    Action::Choose {
                        path: original.clone(),
                        replace: true
                    }
                )
                .unwrap()
        );
        assert_eq!(std::fs::read(original).unwrap(), b"original");
        assert_eq!(host(&mut loaded), request);
        assert!(browser.complete(&mut loaded, Action::Cancel).unwrap());
    }

    #[test]
    fn browser_transcript_destination_keeps_the_full_path_and_dos_text() {
        let dir = Directory::new();
        let mut loaded = loaded(&dir);
        let request = HostRequest::TranscriptName {
            prompt: Vec::new(),
            max_bytes: 28,
        };
        let path = dir.0.join("transcript-with-a-long-modern-filename.txt");
        let Some(HostReply::Name(name)) = loaded
            .files
            .handle_browser_path(&request, path.clone(), false)
            .unwrap()
        else {
            panic!("missing transcript device name");
        };
        assert!(name.len() <= 28);
        assert!(matches!(
            loaded
                .files
                .handle_with_path(&HostRequest::OpenTranscript { path: name }, None)
                .unwrap(),
            Some(HostReply::Done)
        ));
        let bytes = b"> look\r\nA room.\r\n";
        loaded
            .files
            .handle_with_path(
                &HostRequest::WriteTranscript {
                    bytes: bytes.to_vec(),
                },
                None,
            )
            .unwrap();
        loaded
            .files
            .handle_with_path(&HostRequest::CloseTranscript, None)
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn real_sdl_events_edit_names_route_mouse_and_controller_and_retain_frames() {
        const NAME: &str = "windowed::file_browser::tests::real_sdl_events_edit_names_route_mouse_and_controller_and_retain_frames";
        if std::env::var("REFORGE_FILE_BROWSER_SDL").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_FILE_BROWSER_SDL", NAME)
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
        let _guard = reforge_settings::Launch {
            game_id: "mindwheel".into(),
            entry_id: None,
            path: None,
            cli: reforge_settings::Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ]),
        }
        .install();
        let mut window = Window::new_with_geometry(
            "file browser",
            GEOMETRY,
            Some(1),
            reforge_sdl::Presentation::Pixels,
        )
        .unwrap();
        window.start_text_input();
        let dir = Directory::new();
        let mut loaded = loaded(&dir);
        let request = host(&mut loaded);
        let creator = window.texture_creator();
        let mut menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_btz_options(false);
        for operation in [
            request.clone(),
            HostRequest::LoadBookmark,
            HostRequest::TranscriptName {
                prompt: Vec::new(),
                max_bytes: 28,
            },
        ] {
            let mut chooser = FileBrowser::begin(&loaded, &operation, "Mindwheel").unwrap();
            chooser.dialog.click(3, 18, false);
            chooser.dialog.input(Input::SelectAll);
            chooser
                .dialog
                .input(Input::Text("retained-filename".into()));
            let before = chooser.dialog.cells();
            let mut f10 = key(Keycode::F10, Mod::NOMOD);
            if let Event::KeyDown { scancode, .. } = &mut f10 {
                *scancode = Some(reforge_sdl::sdl3::keyboard::Scancode::F10);
            }
            assert!(
                chooser
                    .event_with_settings(&f10, None, &mut menu, &window)
                    .is_none()
            );
            assert!(menu.is_open());
            window.prepare_settings(&mut menu);
            assert!(
                chooser
                    .event_with_settings(
                        &Event::TextInput {
                            timestamp: 0,
                            window_id: 0,
                            text: "menu-only".into()
                        },
                        None,
                        &mut menu,
                        &window
                    )
                    .is_none()
            );
            let mut escape = key(Keycode::Escape, Mod::NOMOD);
            if let Event::KeyDown { scancode, .. } = &mut escape {
                *scancode = Some(reforge_sdl::sdl3::keyboard::Scancode::Escape);
            }
            assert!(
                chooser
                    .event_with_settings(&escape, None, &mut menu, &window)
                    .is_none()
            );
            assert!(!menu.is_open());
            chooser.event_with_settings(
                &Event::KeyUp {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    raw: 0,
                    scancode: Some(reforge_sdl::sdl3::keyboard::Scancode::Escape),
                    keycode: Some(Keycode::Escape),
                    keymod: Mod::NOMOD,
                    repeat: false,
                },
                None,
                &mut menu,
                &window,
            );
            assert_eq!(chooser.dialog.cells(), before);
            assert_eq!(host(&mut loaded), request);
            assert!(
                chooser
                    .event_with_settings(&f10, None, &mut menu, &window)
                    .is_none()
            );
            assert!(menu.is_open());
            assert!(
                chooser
                    .event_with_settings(&f10, None, &mut menu, &window)
                    .is_none()
            );
            assert!(!menu.is_open());
            assert_eq!(chooser.dialog.cells(), before);
            if !matches!(operation, HostRequest::LoadBookmark) {
                let path = dir.0.join("retained-filename");
                std::fs::write(&path, b"old file").unwrap();
                assert!(chooser.dialog.input(Input::Enter).is_none());
                let confirmation = chooser.dialog.cells();
                assert_ne!(confirmation, before);
                chooser.event_with_settings(&f10, None, &mut menu, &window);
                assert!(menu.is_open());
                chooser.event_with_settings(&f10, None, &mut menu, &window);
                assert!(!menu.is_open());
                assert_eq!(chooser.dialog.cells(), confirmation);
                assert_eq!(std::fs::read(&path).unwrap(), b"old file");
                std::fs::remove_file(path).unwrap();
            }
        }
        let mut browser = FileBrowser::begin(&loaded, &request, "Mindwheel").unwrap();
        let metrics = window.ui_metrics();
        let cancel = Event::MouseButtonDown {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 20.5 / 80.0 * metrics.window.0 as f32,
            y: 20.5 / 25.0 * metrics.window.1 as f32,
        };
        assert_eq!(browser.event(&cancel, &window), Some(Action::Cancel));
        browser.event(
            &Event::TextInput {
                timestamp: 0,
                window_id: 0,
                text: "save.dat".into(),
            },
            &window,
        );
        browser.event(&key(Keycode::Left, Mod::NOMOD), &window);
        browser.event(
            &Event::TextInput {
                timestamp: 0,
                window_id: 0,
                text: "X".into(),
            },
            &window,
        );
        assert!(
            String::from_utf8_lossy(
                &browser.dialog.cells()[18 * 160..19 * 160]
                    .iter()
                    .step_by(2)
                    .copied()
                    .collect::<Vec<_>>()
            )
            .contains("save.daXt")
        );
        browser.event(&key(Keycode::A, Mod::LCTRLMOD), &window);
        window.set_clipboard_text("clipboard-save").unwrap();
        browser.event(&key(Keycode::V, Mod::LCTRLMOD), &window);
        browser.event(&key(Keycode::Tab, Mod::NOMOD), &window);
        assert!(browser.dialog.cursor(Duration::ZERO).is_none());
        browser.pad(PadEvent {
            next: true,
            ..Default::default()
        });
        assert_eq!(
            browser.event(&key(Keycode::Return, Mod::NOMOD), &window),
            Some(Action::Cancel)
        );
        browser.pad(PadEvent {
            previous: true,
            ..Default::default()
        });
        let action = browser
            .pad(PadEvent {
                pressed: reforge_sdl::gamepad::Actions {
                    action: true,
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(action,Action::Choose{path,..} if path==dir.0.join("clipboard-save")));
        browser.dialog.input(Input::Tab(true));
        let cursor = browser.dialog.cursor(Duration::ZERO).unwrap();
        let mut short = display::render(&browser.dialog.cells(), true).unwrap();
        paint_file_cursor(&mut short, Some(cursor));
        let mut tall = display::render(&browser.dialog.cells(), true).unwrap();
        paint_file_cursor(&mut tall, browser.dialog.cursor(Duration::from_millis(134)));
        assert_eq!(
            short
                .as_chunks::<3>()
                .0
                .iter()
                .zip(tall.as_chunks::<3>().0)
                .filter(|(a, b)| *a != *b)
                .count(),
            8
        );
        let creator = window.texture_creator();
        let mut frame = FrameTexture::new(&creator, 640, 200, FrameFormat::Rgb).unwrap();
        frame.update(&short).unwrap();
        let mut settings = window.settings();
        settings.renderer = reforge_settings::Renderer::Sdl;
        window.apply_settings(settings).unwrap();
        window
            .present_with(&frame, |canvas| {
                creator.show_pointer();
                let image = canvas
                    .read_pixels(None)
                    .map_err(|e| e.to_string())?
                    .convert_format(reforge_sdl::sdl3::pixels::PixelFormat::RGB24)
                    .map_err(|e| e.to_string())?;
                image.with_lock(|bytes| {
                    assert!(
                        bytes
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .filter(|v| v[0] > 100)
                            .count()
                            > 1000
                    )
                });
                if let Some(path) = std::env::var_os("REFORGE_BTZ_FILE_CAPTURE") {
                    canvas.read_pixels(None).unwrap().save_bmp(path).unwrap();
                }
                Ok(())
            })
            .unwrap();
        for policy in [
            reforge_settings::ScalingPolicy::Fit,
            reforge_settings::ScalingPolicy::Integer,
            reforge_settings::ScalingPolicy::Stretch,
        ] {
            for aspect in [
                reforge_settings::AspectRatio::Original,
                reforge_settings::AspectRatio::Square,
            ] {
                let mut settings = window.settings();
                settings.scaling_policy = policy;
                settings.aspect_ratio = aspect;
                window.apply_settings(settings).unwrap();
                let size = window.ui_metrics().window;
                let (x, y) = window
                    .frame_point(size.0 as f32 / 2.0, size.1 as f32 / 2.0, (640, 200))
                    .unwrap();
                assert!(
                    (x - 320.0).abs() < 1.0 && (y - 100.0).abs() < 1.0,
                    "{policy:?}/{aspect:?}: {x},{y}"
                );
                assert!(window.frame_point(-100.0, -100.0, (640, 200)).is_none());
            }
        }
    }
}
