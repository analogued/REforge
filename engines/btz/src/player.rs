// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player loading and host adapters shared by windowed and terminal play.

#[cfg(feature = "sdl")]
pub(crate) mod file_dialog;
pub mod files;
mod patches;
pub mod presentation;
// Raw host input is isolated here; the VM, parser and story services remain safe Rust.
#[allow(unsafe_code)]
pub mod terminal;

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
    use sha2::{Digest, Sha256};
    match <[u8; 32]>::from(Sha256::digest(exe)) {
        [
            0x26,
            0x8e,
            0xde,
            0xa9,
            0xa2,
            0xed,
            0x7d,
            0xf7,
            0x2c,
            0xd4,
            0x1f,
            0x2d,
            0x3c,
            0x76,
            0x7a,
            0x57,
            0xe6,
            0xeb,
            0xbe,
            0x01,
            0x90,
            0x37,
            0xb4,
            0x66,
            0x38,
            0xc0,
            0x6b,
            0xe5,
            0xc8,
            0x51,
            0x2b,
            0xcb,
        ] => Some((&MINDWHEEL_DOS, MINDWHEEL_RESOURCES)),
        [
            0xca,
            0xf6,
            0xb0,
            0x84,
            0x90,
            0xf4,
            0xd3,
            0x43,
            0xab,
            0x9c,
            0x45,
            0x07,
            0xba,
            0x59,
            0x1f,
            0x66,
            0xb5,
            0xfc,
            0x47,
            0xd1,
            0x80,
            0x7a,
            0xac,
            0x6d,
            0xfc,
            0xd2,
            0x32,
            0xec,
            0x11,
            0x40,
            0x03,
            0xbd,
        ] => Some((&ESSEX_DOS, ESSEX_RESOURCES)),
        [
            0x33,
            0xe3,
            0x98,
            0x86,
            0x6b,
            0xc7,
            0x36,
            0xf3,
            0x47,
            0x5c,
            0x36,
            0x00,
            0x25,
            0x91,
            0xc2,
            0x18,
            0xdf,
            0xdc,
            0x2c,
            0xd5,
            0x4d,
            0x33,
            0xb8,
            0x7e,
            0xd7,
            0xc0,
            0xb1,
            0xd3,
            0xe9,
            0xab,
            0x46,
            0xb7,
        ] => Some((&BRIMSTONE_DOS, BRIMSTONE_RESOURCES)),
        [
            0x4a,
            0x66,
            0x7b,
            0xcb,
            0x84,
            0x0a,
            0x72,
            0x35,
            0x76,
            0x13,
            0xa0,
            0x4b,
            0xa2,
            0xb4,
            0x3c,
            0x34,
            0x1b,
            0x58,
            0xc9,
            0xa9,
            0xea,
            0x08,
            0x51,
            0x34,
            0x33,
            0x75,
            0x09,
            0x30,
            0xf2,
            0xbd,
            0x64,
            0xbb,
        ] => Some((&BREAKERS_DOS, BREAKERS_RESOURCES)),
        _ => None,
    }
}

pub fn load(data: &Path, interpreter: &Path, side_map: &Path) -> Result<Loaded, String> {
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
            let title = files::resolve_original(&data.parent().unwrap().join("TITLE.EXE"))?;
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

pub fn run_terminal(
    mut loaded: Loaded,
    audio: reforge_settings::AudioSettings,
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
                print!("\x1b[?25h\x1b[?1049l");
                let _ = io::stdout().flush();
            }
        }
        print!("\x1b[?1049h\x1b[?25l");
        let _screen = Screen;
        let origin = Instant::now();
        let mut last = Vec::new();
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
                .and_then(|sequence| sequence.frame(origin.elapsed()))
            {
                title_keys.extend(terminal.as_mut().unwrap().poll_batch()?);
                if last != screen.dos_cells() {
                    print!("\x1b[H{}", screen.unicode());
                    io::stdout().flush()?;
                    last = screen.dos_cells().to_vec();
                }
                terminal.as_mut().unwrap().wait(Duration::from_millis(20))?;
                continue;
            }
            loaded.session.set_bios_ticks(bios_ticks(origin.elapsed()));
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
            if last != loaded.session.screen().dos_cells() {
                print!("\x1b[H{}", loaded.session.screen().unicode());
                io::stdout().flush()?;
                last = loaded.session.screen().dos_cells().to_vec();
            }
            write_bells(&mut io::stdout(), loaded.session.take_bells(), &audio)?;
            match event {
                SessionStep::ReadLine { .. } => {
                    if !loaded.session.input().line_ready() {
                        terminal
                            .as_mut()
                            .unwrap()
                            .wait(until_next_bios_tick(origin.elapsed()))?;
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
                    let prompting = loaded.files.needs_input(&request);
                    if prompting {
                        print!("\x1b[25;1H\x1b[K");
                        io::stdout().flush()?;
                        drop(terminal.take());
                    }
                    let result = loaded.files.handle(&request, true);
                    if prompting {
                        terminal = Some(terminal::Terminal::open()?);
                        last.clear();
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
