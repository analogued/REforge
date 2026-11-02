// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Development frontend with optional diagnostic execution limits.

#[path = "core/host.rs"]
mod host;
#[path = "core/terminal.rs"]
mod terminal;

use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
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
        return Err("Usage: btz-core --original <ADISK.OBJ> [--interpreter <GO.EXE>] [--side-map <SIDE.TXT>] [--snapshot | --input <DOS-key-file>] [--save-dir <directory>] [--bookmark-path <file>] [--transcript-path <file>] [--screen-dump <file>] [--trace] [--unlimited | VM-step-limit]".into());
    }
    let path = host::resolve_original(std::path::Path::new(
        &args.next().ok_or("missing original data path")?,
    ))?;
    let mut interpreter = None;
    let mut screen_dump = None;
    let mut input_file = None;
    let mut snapshot = false;
    let mut trace = false;
    let mut limit = None;
    let mut unlimited = false;
    let mut files = host::Files::new(&path)?;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--interpreter" => interpreter = Some(args.next().ok_or("missing interpreter path")?),
            "--screen-dump" => screen_dump = Some(args.next().ok_or("missing screen dump path")?),
            "--input" => input_file = Some(args.next().ok_or("missing key replay path")?),
            "--snapshot" => snapshot = true,
            "--trace" => trace = true,
            "--unlimited" => unlimited = true,
            "--save-dir" => {
                files.save_directory =
                    std::path::absolute(args.next().ok_or("missing save directory")?)?;
            }
            "--bookmark-path" => {
                files.bookmark_path = Some(args.next().ok_or("missing bookmark path")?.into())
            }
            "--transcript-path" => {
                files.transcript_path = Some(args.next().ok_or("missing transcript path")?.into())
            }
            "--side-map" => {
                files.side_map_path = Some(host::resolve_original(std::path::Path::new(
                    &args.next().ok_or("missing side map path")?,
                ))?)
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
    files.originals.push(path.clone());
    let bytes = std::fs::read(path)?;
    let image = if interpreter.is_some() {
        btz::player::prepare_image(bytes)?
    } else {
        StoryImage::from_dos_obj(bytes)?
    };
    let Some(interpreter) = interpreter else {
        if input_file.is_some() || screen_dump.is_some() || snapshot || unlimited {
            return Err("text/input options require --interpreter".into());
        }
        return trace_core(image, limit.unwrap_or(100_000));
    };
    let interpreter = host::resolve_original(std::path::Path::new(&interpreter))?;
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
    let (definition, resources) = btz::player::identify_dos_interpreter(&exe).unwrap_or((
        &btz::interpreter::MINDWHEEL_DOS,
        btz::interpreter::MINDWHEEL_RESOURCES,
    ));
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
