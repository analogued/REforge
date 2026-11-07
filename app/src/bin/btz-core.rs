// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Development frontend with optional diagnostic execution limits.

#[path = "core/host.rs"]
mod host;
#[path = "core/terminal.rs"]
mod terminal;

use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use btz::image::StoryImage;
use btz::input::Key;
use btz::random::DosRandom;
use btz::services::ServiceMessages;
use btz::session::{Session, SessionStep};
use btz::text::TextCodec;
use btz::vm::{Step, Vm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--original") {
        return Err("Usage: btz-core --original <ADISK.OBJ> [--interpreter <GO.EXE>] [--raw-profile <profile>] [--side-map <SIDE.TXT>] [--snapshot | --input <DOS-key-file>] [--save-dir <directory>] [--bookmark-path <file>] [--transcript-path <file>] [--screen-dump <file>] [--trace] [--unlimited | VM-step-limit]".into());
    }
    let path = PathBuf::from(args.next().ok_or("missing original data path")?);
    let mut interpreter = None;
    let mut raw_profile = None;
    let mut screen_dump = None;
    let mut input_file = None;
    let mut snapshot = false;
    let mut trace = false;
    let mut limit = None;
    let mut unlimited = false;
    let mut save_directory = None;
    let mut bookmark_path = None;
    let mut transcript_path = None;
    let mut side_map_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--interpreter" => {
                interpreter = Some(PathBuf::from(
                    args.next().ok_or("missing interpreter path")?,
                ))
            }
            "--raw-profile" => {
                raw_profile = Some(raw_release(&args.next().ok_or("missing raw profile")?)?);
            }
            "--screen-dump" => screen_dump = Some(args.next().ok_or("missing screen dump path")?),
            "--input" => input_file = Some(args.next().ok_or("missing key replay path")?),
            "--snapshot" => snapshot = true,
            "--trace" => trace = true,
            "--unlimited" => unlimited = true,
            "--save-dir" => {
                save_directory = Some(std::path::absolute(
                    args.next().ok_or("missing save directory")?,
                )?);
            }
            "--bookmark-path" => {
                bookmark_path = Some(args.next().ok_or("missing bookmark path")?.into())
            }
            "--transcript-path" => {
                transcript_path = Some(args.next().ok_or("missing transcript path")?.into())
            }
            "--side-map" => {
                side_map_path = Some(PathBuf::from(args.next().ok_or("missing side map path")?));
            }
            _ if limit.is_none() => limit = Some(arg.parse::<usize>()?),
            _ => return Err("unexpected argument".into()),
        }
    }
    if snapshot && input_file.is_some() {
        return Err("--snapshot and --input cannot be combined".into());
    }
    if unlimited && limit.is_some() {
        return Err("--unlimited cannot be combined with a VM-step limit".into());
    }
    let selected = resolve_inputs(
        &path,
        interpreter.as_deref(),
        side_map_path.as_deref(),
        raw_profile,
        reforge::detection::detect,
    )?;
    let path = selected.data;
    let interpreter = selected.interpreter;
    let release = selected.release;
    let mut files = host::Files::new(&path)?;
    if let Some(directory) = save_directory {
        files.save_directory = directory;
    }
    files.bookmark_path = bookmark_path;
    files.transcript_path = transcript_path;
    files.side_map_path = selected.side_map;
    files.originals.push(path.clone());
    let bytes = std::fs::read(path)?;
    let image = match release {
        Some(release) => btz::player::prepare_image(release, bytes)?,
        None => StoryImage::from_dos_obj(bytes)?,
    };
    let Some(interpreter) = interpreter else {
        if input_file.is_some() || screen_dump.is_some() || snapshot || unlimited {
            return Err("text/input options require --interpreter".into());
        }
        return trace_core(image, limit.unwrap_or(100_000));
    };
    files.originals.push(interpreter.clone());
    let exe = std::fs::read(interpreter)?;
    let mut replay: Option<VecDeque<u8>> =
        input_file.map(std::fs::read).transpose()?.map(Into::into);
    let mut terminal = if !snapshot && replay.is_none() && io::stdin().is_terminal() {
        Some(terminal::Terminal::open()?)
    } else {
        None
    };
    let clock_origin = Instant::now();
    let (definition, resources) = release
        .expect("interpreter inputs have a loading profile")
        .interpreter();
    let codec = TextCodec::from_dos_resources(&exe, resources)?;
    let random = DosRandom::from_dos_resources(&exe, resources)?;
    let mut session = if let Some(terminal) = &mut terminal {
        Session::with_interpreter_and_keyboard(image, codec, random, definition, &mut || {
            poll_terminal(terminal)
        })?
    } else {
        Session::with_interpreter(image, codec, random, definition)?
    };
    session.set_service_messages(ServiceMessages::from_dos_resources(&exe, resources)?);
    if let Some(path) = &files.side_map_path {
        files.originals.push(path.clone());
        session.set_side_map(btz::services::SideMap::from_bytes(&std::fs::read(path)?)?);
    }
    let live_screen = terminal.is_some() && io::stdout().is_terminal();
    // A whole-session limit is useful for captures/replays, but must not end a
    // player's session simply because enough ordinary commands have executed.
    let mut budget = limit.or_else(|| (terminal.is_none() && !unlimited).then_some(100_000));
    let mut last_screen = Vec::new();
    let mut finished = false;
    while budget.is_none_or(|remaining| remaining > 0) {
        if terminal.is_some() {
            // PIT channel0 drives the BIOS counter; BIOS wraps at its own
            // midnight threshold rather than normalizing pending deadlines.
            let ticks =
                clock_origin.elapsed().as_nanos() * 1_193_182 / (65_536 * 1_000_000_000u128);
            session.set_bios_ticks((ticks % 0x18_00b0) as u32);
        }
        let event = if let Some(terminal) = &mut terminal {
            if terminal.quit_requested() {
                finished = true;
                break;
            }
            session.advance_with_keyboard(&mut || poll_terminal(terminal))?
        } else {
            session.advance()?
        };
        let bells = session.take_bells();
        if bells != 0 {
            for _ in 0..bells {
                print!("\x07");
            }
            io::stdout().flush()?;
        }
        match event {
            SessionStep::Executed {
                instruction,
                argument,
            } => {
                consume_step(&mut budget);
                if trace {
                    eprintln!(
                        "{:04x}\top={:02x}\tform={}\targ={:04x}\tacc={:04x}\tnext={:04x}",
                        instruction.address.0,
                        instruction.opcode,
                        instruction.form,
                        argument,
                        session.vm().accumulator(),
                        session.vm().state().ip.0
                    );
                }
            }
            SessionStep::Progress => consume_step(&mut budget),
            event @ (SessionStep::WaitKey | SessionStep::ReadLine { .. }) => {
                if live_screen && last_screen != session.screen().dos_cells() {
                    print!("\x1b[2J\x1b[H{}", session.screen().unicode());
                    io::stdout().flush()?;
                    last_screen = session.screen().dos_cells().to_vec();
                }
                if matches!(event, SessionStep::ReadLine { .. })
                    && let Some(terminal) = &mut terminal
                {
                    if !session.input().line_ready() && !session.input().paused() {
                        // Waiting only wakes the host; it never submits a key.
                        // At the next iteration, refresh BIOS time and let the
                        // native deadline check precede the nonblocking poll.
                        terminal.wait(until_next_bios_tick(clock_origin.elapsed()))?;
                    }
                    continue;
                }
                let key = if let Some(replay) = &mut replay {
                    replay.pop_front().map(Key::from_dos_byte)
                } else if let Some(terminal) = &mut terminal {
                    terminal.wait(Duration::from_millis(100))?;
                    match terminal.read()? {
                        terminal::Input::Idle => {
                            continue;
                        }
                        terminal::Input::Key(key) => Some(key),
                        terminal::Input::Quit => None,
                    }
                } else {
                    None
                };
                if let Some(key) = key {
                    let mut keys = vec![key];
                    if matches!(event, SessionStep::ReadLine { .. })
                        && key != Key::Enter
                        && let Some(replay) = &mut replay
                    {
                        while let Some(byte) = replay.pop_front() {
                            let key = Key::from_dos_byte(byte);
                            keys.push(key);
                            if key == Key::Enter {
                                break;
                            }
                        }
                    }
                    session.key_batch(&keys)?;
                    for _ in 0..session.take_bells() {
                        print!("\x07");
                    }
                    io::stdout().flush()?;
                } else {
                    if trace {
                        eprintln!("suspended: {event:?}");
                    }
                    finished = true;
                    break;
                }
            }
            SessionStep::ParsedLine { line, unknown } => {
                consume_step(&mut budget);
                if trace {
                    eprintln!(
                        "parsed: unknown={unknown}, raw line={:?}",
                        String::from_utf8_lossy(&line)
                    );
                }
            }
            SessionStep::TimedOut { value } => {
                consume_step(&mut budget);
                if trace {
                    eprintln!("read timed out: {value:04x}");
                }
            }
            SessionStep::Unsupported(request) => {
                eprintln!("suspended: {request:?}");
                finished = true;
                break;
            }
            SessionStep::Host(request) => {
                let prompting = terminal.is_some() && files.needs_input(&request);
                if prompting {
                    drop(terminal.take());
                }
                let reply = files.handle(&request, live_screen)?;
                if prompting {
                    terminal = Some(terminal::Terminal::open()?);
                    last_screen.clear();
                }
                if let Some(reply) = reply {
                    if let Err(error) = session.complete_host(reply) {
                        if matches!(request, btz::services::HostRequest::LoadBookmark) {
                            eprintln!("Cannot restore bookmark: {error}");
                            session.complete_host(btz::services::HostReply::Cancelled)?;
                        } else {
                            return Err(error.into());
                        }
                    }
                } else {
                    let description = match request {
                        btz::services::HostRequest::SaveBookmark { .. } => {
                            "bookmark destination required (--bookmark-path)"
                        }
                        btz::services::HostRequest::LoadBookmark => {
                            "bookmark source required (--bookmark-path)"
                        }
                        btz::services::HostRequest::TranscriptName { .. } => {
                            "transcript destination required (--transcript-path)"
                        }
                        btz::services::HostRequest::ReadSideMap => {
                            "side mapping required (--side-map)"
                        }
                        btz::services::HostRequest::MountDisk { .. } => {
                            "additional program disk required"
                        }
                        _ => "host file operation remains pending",
                    };
                    eprintln!("suspended: {description}");
                    finished = true;
                    break;
                }
            }
            SessionStep::Exited { .. } => {
                finished = true;
                break;
            }
        }
    }
    // Restore echo/canonical input before returning to the shell.
    if live_screen {
        print!("\x1b[2J\x1b[H");
        io::stdout().flush()?;
    }
    drop(terminal);
    print!("{}", session.screen().unicode());
    if trace {
        eprintln!(
            "text: upper={:?}, input={:?}, random={:?}",
            session.screen().upper_cursor(),
            session.input().cursor(),
            session.random_state()
        );
    }
    if let Some(path) = screen_dump {
        std::fs::write(path, session.screen().dos_cells())?;
    }
    if !finished {
        eprintln!("instruction budget exhausted");
    }
    Ok(())
}

fn poll_terminal(terminal: &mut terminal::Terminal) -> Result<Vec<Key>, btz::Error> {
    terminal
        .poll_batch()
        .map_err(|error| btz::Error(format!("terminal input: {error}")))
}

fn until_next_bios_tick(elapsed: Duration) -> Duration {
    // Wake no later than the next BIOS transition, including a short two-tick
    // deadline. poll(2) can also wake immediately when keyboard input arrives.
    let tick_numerator = 65_536 * 1_000_000_000u128;
    let phase = elapsed.as_nanos() * 1_193_182 % tick_numerator;
    Duration::from_nanos((tick_numerator - phase).div_ceil(1_193_182) as u64)
}

fn consume_step(budget: &mut Option<usize>) {
    if let Some(remaining) = budget {
        *remaining -= 1;
    }
}

fn trace_core(image: StoryImage, limit: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut vm = Vm::new(image)?;
    for _ in 0..limit {
        match vm.step()? {
            Step::Executed {
                instruction,
                argument,
            } => eprintln!(
                "{:04x}\top={:02x}\tform={}\targ={:04x}\tacc={:04x}\tnext={:04x}",
                instruction.address.0,
                instruction.opcode,
                instruction.form,
                argument,
                vm.accumulator(),
                vm.state().ip.0
            ),
            Step::Suspended(request) => {
                eprintln!("suspended: {request:?}");
                return Ok(());
            }
        }
    }
    eprintln!("instruction budget exhausted");
    Ok(())
}

// Explicit format selection is only for raw, unregistered parser experiments.
// Ordinary diagnostic sessions use the application detector above.
fn raw_release(value: &str) -> Result<btz::player::DosRelease, String> {
    use btz::player::DosRelease;
    match value {
        "mindwheel" => Ok(DosRelease::Mindwheel),
        "essex" => Ok(DosRelease::Essex),
        "brimstone" => Ok(DosRelease::Brimstone),
        "breakers" => Ok(DosRelease::Breakers),
        _ => Err(format!("Unknown raw interpreter profile: {value}")),
    }
}

struct InputContext {
    data: PathBuf,
    interpreter: Option<PathBuf>,
    side_map: Option<PathBuf>,
    release: Option<btz::player::DosRelease>,
}

/// Normal diagnostic sessions share application identification. Only explicit
/// raw probes resolve their own input filenames without release verification.
fn resolve_inputs(
    original: &Path,
    interpreter: Option<&Path>,
    side_map: Option<&Path>,
    raw_profile: Option<btz::player::DosRelease>,
    detect: impl FnOnce(&Path) -> Result<reforge::detection::Detected, String>,
) -> Result<InputContext, Box<dyn std::error::Error>> {
    if raw_profile.is_some() && interpreter.is_none() {
        return Err("--raw-profile requires --interpreter for an explicit parser probe".into());
    }
    if raw_profile.is_some() || interpreter.is_none() {
        return Ok(InputContext {
            data: host::resolve_original(original)?,
            interpreter: interpreter.map(host::resolve_original).transpose()?,
            side_map: side_map.map(host::resolve_original).transpose()?,
            release: raw_profile,
        });
    }
    let detected = detect(original)?;
    let reforge::detection::Engine::Btz(release) = detected.game.engine() else {
        return Err("btz-core requires a supported BTZ installation".into());
    };
    let inputs = detected.inputs();
    let interpreter_name = if release == btz::player::DosRelease::Breakers {
        "RUN.EXE"
    } else {
        "GO.EXE"
    };
    let selected_interpreter = inputs.path(interpreter_name)?;
    if !matches_selected(interpreter.unwrap(), selected_interpreter)? {
        return Err(format!("Diagnostic input does not match detected {interpreter_name}").into());
    }
    Ok(InputContext {
        data: inputs.path("ADISK.OBJ")?.to_path_buf(),
        interpreter: Some(selected_interpreter.to_path_buf()),
        side_map: Some(match side_map {
            // An explicitly supplied diagnostic override keeps its existing
            // semantics; only the default comes from the verified file map.
            Some(path) => host::resolve_original(path)?,
            None => inputs.path("SIDE.TXT")?.to_path_buf(),
        }),
        release: Some(release),
    })
}

/// Accept the selected file or a missing case spelling of that same role.
/// Never discover another filename or substitute a different existing file.
fn matches_selected(given: &Path, selected: &Path) -> io::Result<bool> {
    let selected_canonical = std::fs::canonicalize(selected)?;
    match std::fs::canonicalize(given) {
        Ok(path) => Ok(path == selected_canonical),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling symlink is an existing, different explicit input.
            if std::fs::symlink_metadata(given).is_ok() {
                return Ok(false);
            }
            let Some(given_name) = given.file_name() else {
                return Ok(false);
            };
            let Some(selected_name) = selected.file_name() else {
                return Ok(false);
            };
            if !given_name
                .as_encoded_bytes()
                .eq_ignore_ascii_case(selected_name.as_encoded_bytes())
            {
                return Ok(false);
            }
            let parent = |path: &Path| {
                path.parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            };
            Ok(std::fs::canonicalize(parent(given))? == std::fs::canonicalize(parent(selected))?)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use btz::player::DosRelease;
    use reforge::detection::{Detected, Game, RELEASES};
    use std::{
        cell::Cell,
        collections::BTreeMap,
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "btz-input-context-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            fs::create_dir(directory.join("logical")).unwrap();
            for name in ["ADISK.OBJ", "GO.EXE", "SIDE.TXT"] {
                fs::write(directory.join(name), name).unwrap();
            }
            Self(directory)
        }

        fn selected(&self, name: &str) -> PathBuf {
            self.0.join("logical").join("..").join(name)
        }

        fn detected(&self) -> Detected {
            Detected {
                game: Game::Mindwheel,
                release: RELEASES
                    .iter()
                    .find(|release| release.game() == Game::Mindwheel)
                    .unwrap(),
                root: self.0.clone(),
                original: self.0.clone(),
                files: BTreeMap::from([
                    ("ADISK.OBJ", self.selected("ADISK.OBJ")),
                    ("GO.EXE", self.selected("GO.EXE")),
                    ("SIDE.TXT", self.selected("SIDE.TXT")),
                ]),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn ordinary_diagnostics_keep_selected_paths_despite_filename_collisions() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("adisk.obj"), b"unselected story").unwrap();
        fs::write(fixture.0.join("go.exe"), b"unselected interpreter").unwrap();
        if fs::read(fixture.0.join("ADISK.OBJ")).unwrap() != b"ADISK.OBJ" {
            // A case-insensitive filesystem cannot hold distinct candidates.
            return;
        }
        let called = Cell::new(false);
        let context = resolve_inputs(
            &fixture.0,
            Some(&fixture.0.join("GO.EXE")),
            None,
            None,
            |anchor| {
                assert_eq!(anchor, fixture.0);
                called.set(true);
                Ok(fixture.detected())
            },
        )
        .unwrap();
        assert!(called.get());
        assert_eq!(context.data, fixture.selected("ADISK.OBJ"));
        assert_eq!(context.interpreter, Some(fixture.selected("GO.EXE")));
        assert_eq!(context.side_map, Some(fixture.selected("SIDE.TXT")));
        assert_eq!(context.release, Some(DosRelease::Mindwheel));
        assert_eq!(fs::read(&context.data).unwrap(), b"ADISK.OBJ");
        assert_eq!(
            host::Files::new(&context.data).unwrap().save_directory,
            std::path::absolute(context.data.parent().unwrap()).unwrap()
        );
    }

    #[test]
    fn ordinary_diagnostics_accept_only_selected_interpreter_or_its_missing_case_spelling() {
        let fixture = Fixture::new();
        let case_spelling = fixture.0.join("gO.eXe");
        let context = resolve_inputs(
            &fixture.0.join("ADISK.OBJ"),
            Some(&case_spelling),
            None,
            None,
            |_| Ok(fixture.detected()),
        )
        .unwrap();
        assert_eq!(context.interpreter, Some(fixture.selected("GO.EXE")));
        let other_directory = fixture.0.join("elsewhere");
        fs::create_dir(&other_directory).unwrap();
        for path in [fixture.0.join("other.exe"), other_directory.join("GO.EXE")] {
            fs::write(&path, b"unselected").unwrap();
            assert!(
                resolve_inputs(&fixture.0, Some(&path), None, None, |_| Ok(
                    fixture.detected()
                ))
                .is_err()
            );
        }
        fs::write(&case_spelling, b"unselected").unwrap();
        // A case-insensitive filesystem may have overwritten the selected file.
        if fs::read(fixture.0.join("GO.EXE")).unwrap() != b"unselected" {
            assert!(
                resolve_inputs(&fixture.0, Some(&case_spelling), None, None, |_| Ok(
                    fixture.detected()
                ))
                .is_err()
            );
        }
        assert!(
            !matches_selected(&other_directory.join("go.exe"), &fixture.selected("GO.EXE"))
                .unwrap()
        );
    }

    #[test]
    fn ordinary_diagnostic_rejection_happens_before_resource_parsing() {
        let fixture = Fixture::new();
        let error = resolve_inputs(
            &fixture.0,
            Some(&fixture.0.join("missing.exe")),
            None,
            None,
            |_| Err("detector rejected inputs".into()),
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "detector rejected inputs");
    }

    #[test]
    fn explicit_research_profile_and_raw_vm_probe_do_not_call_detection() {
        let fixture = Fixture::new();
        let data = fixture.0.join("adisk.obj");
        let interpreter = fixture.0.join("go.exe");
        let context = resolve_inputs(
            &data,
            Some(&interpreter),
            None,
            Some(DosRelease::Essex),
            |_| panic!("raw profile must not detect releases"),
        )
        .unwrap();
        assert_eq!(context.release, Some(DosRelease::Essex));
        assert_eq!(context.data, fixture.0.join("ADISK.OBJ"));
        let context = resolve_inputs(&data, None, None, None, |_| {
            panic!("raw VM probe must not detect releases")
        })
        .unwrap();
        assert!(context.release.is_none());
        assert!(context.interpreter.is_none());
        assert!(
            resolve_inputs(&data, None, None, Some(DosRelease::Essex), |_| panic!(
                "invalid raw options must not detect releases"
            ))
            .is_err()
        );
    }

    #[test]
    fn explicit_diagnostic_media_override_keeps_its_existing_meaning() {
        let fixture = Fixture::new();
        let custom = fixture.0.join("custom.map");
        fs::write(&custom, b"authored media map").unwrap();
        let context = resolve_inputs(
            &fixture.0,
            Some(&fixture.0.join("GO.EXE")),
            Some(&custom),
            None,
            |_| Ok(fixture.detected()),
        )
        .unwrap();
        assert_eq!(context.side_map, Some(custom));
        assert_eq!(context.data, fixture.selected("ADISK.OBJ"));
    }
}
