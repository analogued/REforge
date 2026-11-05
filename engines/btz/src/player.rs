// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player loading and host adapters shared by windowed and terminal play.

pub(crate) mod cursor;
pub(crate) mod file_dialog;
pub mod files;
mod patches;
pub mod presentation;
// Raw host input is isolated here; the VM, parser and story services remain safe Rust.
#[allow(unsafe_code)]
pub mod terminal;
mod terminal_screen;

use crate::{
    Error,
    image::StoryImage,
    random::DosRandom,
    services::{ServiceMessages, SideMap},
    session::Session,
    text::TextCodec,
};
use std::{path::Path, time::Duration};

pub struct Loaded {
    pub session: Session,
    pub files: files::Files,
    pub presentation: Option<presentation::TitleSequence>,
}

pub fn prepare_image(mut bytes: Vec<u8>) -> Result<StoryImage, Error> {
    patches::prepare_for_play(&mut bytes);
    StoryImage::from_dos_obj(bytes)
}

/// Identification is a loading concern, outside the VM. Unknown diagnostic
/// fixtures can still use the explicit definition/resource constructors.
pub fn identify_dos_interpreter(
    exe: &[u8],
) -> Option<(
    &'static crate::interpreter::Interpreter,
    crate::interpreter::DosResources,
)> {
    use crate::interpreter::*;
    match *blake3::hash(exe).as_bytes() {
        [
            0xab,
            0xc0,
            0x67,
            0x8c,
            0x92,
            0xdc,
            0x91,
            0xf3,
            0x9a,
            0x02,
            0x03,
            0xc1,
            0x9e,
            0x80,
            0xaf,
            0xed,
            0x95,
            0x64,
            0xc3,
            0x22,
            0x50,
            0xdb,
            0x21,
            0x21,
            0x6c,
            0x90,
            0x44,
            0x27,
            0xb5,
            0xcd,
            0xd7,
            0x56,
        ] => Some((&MINDWHEEL_DOS, MINDWHEEL_RESOURCES)),
        [
            0x1d,
            0x3d,
            0x08,
            0x2d,
            0x6e,
            0xbc,
            0x8f,
            0x7c,
            0x5a,
            0x1c,
            0x49,
            0xca,
            0x7c,
            0x77,
            0x66,
            0x53,
            0xd0,
            0xb3,
            0x75,
            0x50,
            0x68,
            0xac,
            0x2f,
            0xb9,
            0x72,
            0xf0,
            0x2b,
            0x6b,
            0x13,
            0xfe,
            0xea,
            0x98,
        ] => Some((&ESSEX_DOS, ESSEX_RESOURCES)),
        [
            0x75,
            0x8b,
            0x53,
            0xef,
            0x59,
            0xd0,
            0x1a,
            0x19,
            0x12,
            0x70,
            0xb6,
            0x97,
            0x1f,
            0xea,
            0x44,
            0xb1,
            0xe0,
            0xe1,
            0xdf,
            0xcd,
            0x47,
            0x20,
            0x67,
            0x4a,
            0xfd,
            0x6e,
            0x37,
            0x6d,
            0x1d,
            0x0a,
            0xe8,
            0xe5,
        ] => Some((&BRIMSTONE_DOS, BRIMSTONE_RESOURCES)),
        [
            0x95,
            0xaa,
            0x40,
            0x27,
            0xbe,
            0xf4,
            0x1d,
            0xa2,
            0x1a,
            0x3f,
            0xa6,
            0x36,
            0xbe,
            0x57,
            0x1a,
            0xb5,
            0x03,
            0xa0,
            0xfc,
            0x3d,
            0xa1,
            0x35,
            0x31,
            0x50,
            0xf7,
            0x50,
            0x71,
            0x4c,
            0xd5,
            0xac,
            0xb6,
            0xac,
        ] => Some((&BREAKERS_DOS, BREAKERS_RESOURCES)),
        _ => None,
    }
}

pub fn load(data: &Path, interpreter: &Path, side_map: &Path) -> Result<Loaded, String> {
    load_with_title(data, interpreter, side_map, None)
}

/// Load with an optional exact title-art path selected by the release detector.
/// Standalone callers can retain ordinary TITLE.EXE lookup through `load`.
pub fn load_with_title(
    data: &Path,
    interpreter: &Path,
    side_map: &Path,
    title: Option<&Path>,
) -> Result<Loaded, String> {
    let load = || -> Result<Loaded, Box<dyn std::error::Error>> {
        let resolve = |path: &Path| {
            if path.is_file() {
                Ok(path.to_path_buf())
            } else {
                files::resolve_original(path)
            }
        };
        let data = resolve(data)?;
        let interpreter = resolve(interpreter)?;
        let side_map = resolve(side_map)?;
        let image = prepare_image(std::fs::read(&data)?)?;
        let exe = std::fs::read(&interpreter)?;
        // Production callers have already identified exact files in the game
        // catalog. Retain the original default for explicit diagnostic fixtures.
        let (definition, resources) = identify_dos_interpreter(&exe).unwrap_or((
            &crate::interpreter::MINDWHEEL_DOS,
            crate::interpreter::MINDWHEEL_RESOURCES,
        ));
        let mut session = Session::with_interpreter(
            image,
            TextCodec::from_dos_resources(&exe, resources)?,
            DosRandom::from_dos_resources(&exe, resources)?,
            definition,
        )?;
        session.set_service_messages(ServiceMessages::from_dos_resources(&exe, resources)?);
        session.set_side_map(SideMap::from_bytes(&std::fs::read(&side_map)?)?);
        let mut files = files::Files::new(&data)?;
        files.originals = vec![data.clone(), interpreter, side_map.clone()];
        files.side_map_path = Some(side_map);
        files.confirm_bookmark_replace = definition.confirm_bookmark_replace;
        let presentation = if std::ptr::eq(definition, &crate::interpreter::BREAKERS_DOS) {
            let title = match title {
                Some(path) => resolve(path)?,
                None => files::resolve_original(&data.parent().unwrap().join("TITLE.EXE"))?,
            };
            let presentation =
                presentation::TitleSequence::from_breakers_executable(&std::fs::read(&title)?)?;
            files.originals.push(title);
            Some(presentation)
        } else {
            None
        };
        Ok(Loaded {
            session,
            files,
            presentation,
        })
    };
    load().map_err(|error| error.to_string())
}

pub fn bios_ticks(elapsed: Duration) -> u32 {
    let ticks = elapsed.as_nanos() * 1_193_182 / (65_536 * 1_000_000_000u128);
    (ticks % 0x18_00b0) as u32
}

pub fn until_next_bios_tick(elapsed: Duration) -> Duration {
    let period = 65_536 * 1_000_000_000u128;
    let phase = elapsed.as_nanos() * 1_193_182 % period;
    Duration::from_nanos((period - phase).div_ceil(1_193_182) as u64)
}

pub fn terminal_available() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn run_terminal(loaded: Loaded, audio: reforge_settings::AudioSettings) -> Result<(), String> {
    run_terminal_with_settings(
        loaded,
        reforge_settings::RuntimeSettings {
            audio,
            ..Default::default()
        },
        "BTZ",
    )
}

pub fn run_terminal_with_settings(
    mut loaded: Loaded,
    settings: reforge_settings::RuntimeSettings,
    title: &str,
) -> Result<(), String> {
    use crate::session::SessionStep;
    use std::io::{self, Write};
    use std::time::Instant;
    if !terminal_available() {
        return Err("Terminal rendering requires an attached input/output terminal. Run reforge --original <game> --renderer terminal from a terminal.".into());
    }
    let mut run = || -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = Some(terminal::Terminal::open()?);
        struct Screen;
        impl Drop for Screen {
            fn drop(&mut self) {
                print!("\x1b[?2004l\x1b[0 q\x1b[0m\x1b[?25h\x1b[?1049l");
                let _ = io::stdout().flush();
            }
        }
        print!("\x1b[?1049h\x1b[?25l");
        let _screen = Screen;
        let origin = Instant::now();
        let mut view = terminal_screen::Screen::default();
        let mut paused = Duration::ZERO;
        let mut title_keys = std::collections::VecDeque::new();
        loop {
            if terminal
                .as_ref()
                .is_some_and(|input| input.quit_requested())
            {
                break;
            }
            if let Some(screen) = loaded
                .presentation
                .as_ref()
                .and_then(|sequence| sequence.frame(origin.elapsed().saturating_sub(paused)))
            {
                title_keys.extend(terminal.as_mut().unwrap().poll_batch()?);
                view.draw(
                    &mut io::stdout(),
                    screen.dos_cells(),
                    None,
                    settings.btz.file_cursor,
                )?;
                terminal.as_mut().unwrap().wait(Duration::from_millis(20))?;
                continue;
            }
            loaded
                .session
                .set_bios_ticks(bios_ticks(origin.elapsed().saturating_sub(paused)));
            let mut submitted = loaded.session.input().line_ready();
            let event = loaded.session.advance_with_keyboard(&mut || {
                if submitted {
                    return Ok(Vec::new());
                }
                if !title_keys.is_empty() {
                    let mut keys = Vec::new();
                    while let Some(key) = title_keys.pop_front() {
                        keys.push(key);
                        if key == crate::input::Key::Enter {
                            break;
                        }
                    }
                    submitted |= keys.last() == Some(&crate::input::Key::Enter);
                    return Ok(keys);
                }
                let keys = terminal
                    .as_mut()
                    .unwrap()
                    .poll_batch()
                    .map_err(|e| Error(e.to_string()))?;
                submitted |= keys.last() == Some(&crate::input::Key::Enter);
                Ok(keys)
            })?;
            let (cells, marker) = cursor::game_cells(
                loaded.session.screen(),
                loaded.session.input(),
                reforge_settings::FileCursorStyle::SteadyBlock,
                Duration::ZERO,
            );
            view.draw(
                &mut io::stdout(),
                &cells,
                marker.map(|c| (c.row, c.column)),
                settings.btz.file_cursor,
            )?;
            write_bells(
                &mut io::stdout(),
                loaded.session.take_bells(),
                &settings.audio,
            )?;
            match event {
                SessionStep::ReadLine { .. } => {
                    if !loaded.session.input().line_ready() {
                        terminal.as_mut().unwrap().wait(until_next_bios_tick(
                            origin.elapsed().saturating_sub(paused),
                        ))?;
                    }
                }
                SessionStep::WaitKey => {
                    terminal.as_mut().unwrap().wait(Duration::from_millis(20))?;
                    match terminal.as_mut().unwrap().read()? {
                        terminal::Input::Key(key) => loaded.session.key(key)?,
                        terminal::Input::Quit => break,
                        terminal::Input::Idle => (),
                    }
                }
                SessionStep::Host(request) => {
                    if let Some(mut dialog) = file_dialog::begin_selection(&loaded, &request, title)
                    {
                        let started = Instant::now();
                        print!("\x1b[?2004h");
                        let mut quit = false;
                        loop {
                            view.draw(
                                &mut io::stdout(),
                                &dialog.cells(),
                                dialog.cursor_position(),
                                settings.btz.file_cursor,
                            )?;
                            terminal
                                .as_mut()
                                .unwrap()
                                .wait(Duration::from_millis(100))?;
                            match terminal.as_mut().unwrap().read_dialog()? {
                                terminal::DialogInput::Idle => (),
                                terminal::DialogInput::Quit => {
                                    quit = true;
                                    break;
                                }
                                terminal::DialogInput::Edit(input) => {
                                    if let Some(action) = dialog.input(input)
                                        && file_dialog::complete_selection(
                                            &mut loaded,
                                            &request,
                                            &mut dialog,
                                            action,
                                        )?
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                        print!("\x1b[?2004l");
                        paused += started.elapsed();
                        view.invalidate();
                        if quit {
                            break;
                        }
                        continue;
                    }
                    let prompting = loaded.files.needs_input(&request);
                    let started = Instant::now();
                    if prompting {
                        print!("\x1b[0m\x1b[0 q\x1b[?25h\x1b[25;1H\x1b[K");
                        io::stdout().flush()?;
                        drop(terminal.take());
                    }
                    let result = loaded.files.handle(&request, true);
                    if prompting {
                        terminal = Some(terminal::Terminal::open()?);
                        paused += started.elapsed();
                        view.invalidate();
                    }
                    let reply = match result {
                        Ok(reply) => reply,
                        Err(error)
                            if matches!(
                                request,
                                crate::services::HostRequest::MountDisk { .. }
                            ) =>
                        {
                            eprintln!("Cannot load game media: {error}");
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if let Some(reply) = reply
                        && let Err(error) = loaded.session.complete_host(reply)
                    {
                        if matches!(request, crate::services::HostRequest::LoadBookmark) {
                            eprintln!("Cannot restore bookmark: {error}");
                            loaded
                                .session
                                .complete_host(crate::services::HostReply::Cancelled)?;
                        } else {
                            return Err(error.into());
                        }
                    }
                }
                SessionStep::Exited { .. } => break,
                SessionStep::Unsupported(request) => {
                    return Err(format!("unsupported game request: {request:?}").into());
                }
                _ => (),
            }
        }
        Ok(())
    };
    run().map_err(|error| error.to_string())
}

// A terminal owns the bell's actual sound/visual policy and volume. REforge can
// suppress the request when its sound controls are muted, but cannot set the
// terminal's bell amplitude. Flush immediately, including during an input wait.
fn write_bells(
    output: &mut impl std::io::Write,
    count: usize,
    settings: &reforge_settings::AudioSettings,
) -> std::io::Result<()> {
    if settings.enabled
        && settings.volume != 0
        && settings
            .categories
            .get(reforge_settings::SoundCategory::Effects)
            != 0
        && count != 0
    {
        for _ in 0..count {
            output.write_all(&[7])?;
        }
        output.flush()?;
    }
    Ok(())
}
