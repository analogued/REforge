// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native terminal adapter. The engine itself consumes typed key events.

use std::collections::VecDeque;
use std::io;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::input::Key;

#[derive(Debug, Eq, PartialEq)]
pub enum Input {
    Idle,
    Key(Key),
    Quit,
}

/// Read a cooked filename prompt without a buffered reader taking ownership of
/// later game keys. The terminal adapter must be dropped before calling this.
pub fn read_prompt_line() -> io::Result<String> {
    #[cfg(unix)]
    {
        let mut line = Vec::new();
        loop {
            let mut byte = 0;
            // SAFETY: byte is writable for exactly one requested byte. Reading
            // directly from fd0 leaves all bytes after this line in the kernel
            // queue for the subsequently reopened raw terminal adapter.
            let count = unsafe { libc::read(libc::STDIN_FILENO, (&mut byte as *mut u8).cast(), 1) };
            if count < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if count == 0 {
                break;
            }
            line.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        String::from_utf8(line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
    #[cfg(windows)]
    {
        windows::read_prompt_line()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line)
    }
}

pub struct Terminal {
    decoder: KeyboardDecoder,
    #[cfg(unix)]
    saved: String,
    #[cfg(windows)]
    console: windows::Console,
}

impl Terminal {
    pub fn open() -> io::Result<Self> {
        #[cfg(unix)]
        {
            let result = Command::new("stty")
                .arg("-g")
                .stdin(Stdio::inherit())
                .output()?;
            if !result.status.success() {
                return Err(io::Error::other("cannot read terminal mode"));
            }
            let saved = String::from_utf8(result.stdout)
                .map_err(io::Error::other)?
                .trim()
                .to_owned();
            let mut terminal = Self {
                decoder: KeyboardDecoder::default(),
                saved,
            };
            let result = Command::new("stty")
                .args([
                    "-icanon", "-echo", "-isig", "-ixon", "-icrnl", "min", "0", "time", "0",
                ])
                .stdin(Stdio::inherit())
                .status()?;
            if !result.success() {
                terminal.restore();
                return Err(io::Error::other("cannot configure terminal input"));
            }
            Ok(terminal)
        }
        #[cfg(windows)]
        {
            Ok(Self {
                decoder: KeyboardDecoder::default(),
                console: windows::Console::open()?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native terminal input is not supported on this platform",
            ))
        }
    }

    fn collect_available(&mut self) -> io::Result<bool> {
        // Read only as far as the next decoded key. Bytes after CR must remain
        // in the OS queue, including for a subsequent cooked filename prompt.
        // VMIN=0/VTIME=0 makes this read nonblocking.
        #[cfg(unix)]
        {
            let mut byte = 0u8;
            // SAFETY: byte is writable for the one byte requested. Read the OS
            // descriptor directly so stdio read-ahead cannot hide queued keys
            // from the readiness wait. The terminal has VMIN=0 and VTIME=0.
            let count = unsafe { libc::read(libc::STDIN_FILENO, (&mut byte as *mut u8).cast(), 1) };
            if count < 0 {
                let error = io::Error::last_os_error();
                return if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    Ok(false)
                } else {
                    Err(error)
                };
            }
            if count == 0 {
                return Ok(false);
            }
            self.decoder.pending.push_back(byte);
            Ok(true)
        }
        #[cfg(windows)]
        {
            let Some(byte) = self.console.read_byte()? else {
                return Ok(false);
            };
            self.decoder.pending.push_back(byte);
            // Console KEY_EVENT_RECORDs distinguish Escape from function keys;
            // there is no ANSI prefix to wait for on this input path.
            if byte == 27 {
                self.decoder.escape_since = Instant::now().checked_sub(ESCAPE_DELAY);
            }
            Ok(true)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native terminal input is not supported on this platform",
            ))
        }
    }

    /// A keyboard poll never waits, including for the rest of an ANSI sequence.
    pub fn read(&mut self) -> io::Result<Input> {
        loop {
            let input = self.decoder.next(Instant::now());
            if input != Input::Idle || !self.collect_available()? {
                return Ok(input);
            }
        }
    }

    pub fn poll_batch(&mut self) -> io::Result<Vec<Key>> {
        let mut keys = Vec::new();
        while let Input::Key(key) = self.read()? {
            keys.push(key);
            if key == Key::Enter {
                break;
            }
        }
        Ok(keys)
    }

    pub fn quit_requested(&self) -> bool {
        self.decoder.quit
    }

    /// Wait for activity without accepting a key into the game. The caller must
    /// refresh its clock and check the native deadline before polling again.
    pub fn wait(&mut self, duration: Duration) -> io::Result<()> {
        #[cfg(unix)]
        {
            let now = Instant::now();
            if self.decoder.ready(now) {
                return Ok(());
            }
            let duration = self
                .decoder
                .escape_since
                .map(|since| ESCAPE_DELAY.saturating_sub(now.duration_since(since)))
                .map_or(duration, |remaining| duration.min(remaining));
            let milliseconds = duration.as_millis()
                + u128::from(!duration.subsec_nanos().is_multiple_of(1_000_000));
            let timeout = milliseconds.min(i32::MAX as u128) as i32;
            let mut descriptor = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor is initialized and remains writable for the
            // one-element array throughout poll; this does not read any keys.
            if unsafe { libc::poll(&mut descriptor, 1, timeout) } < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
            if descriptor.revents & libc::POLLNVAL != 0 {
                return Err(io::Error::other("terminal input descriptor is invalid"));
            }
            if descriptor.revents & libc::POLLHUP != 0 {
                self.decoder.quit = true;
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            if self.decoder.ready(Instant::now()) {
                return Ok(());
            }
            self.console.wait(duration)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = duration;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native terminal input is not supported on this platform",
            ))
        }
    }

    #[cfg(unix)]
    fn restore(&mut self) {
        if !self.saved.is_empty() {
            let _ = Command::new("stty")
                .arg(&self.saved)
                .stdin(Stdio::inherit())
                .status();
            self.saved.clear();
        }
    }
}

// Distinguish bare Escape (pause) from terminal ANSI keys without blocking a
// native keyboard poll. An incomplete sequence has at most 30 ms to complete.
const ESCAPE_DELAY: Duration = Duration::from_millis(30);

#[derive(Default)]
struct KeyboardDecoder {
    pending: VecDeque<u8>,
    escape_since: Option<Instant>,
    quit: bool,
}

impl KeyboardDecoder {
    fn ready(&self, now: Instant) -> bool {
        if self.quit {
            return true;
        }
        let Some(&first) = self.pending.front() else {
            return false;
        };
        if first != 27 || self.escape_since.is_none() {
            return true;
        }
        if self
            .escape_since
            .is_some_and(|since| now.duration_since(since) >= ESCAPE_DELAY)
        {
            return true;
        }
        match self.pending.get(1) {
            Some(b'[' | b'O') => {
                self.pending.len() >= 10
                    || self
                        .pending
                        .iter()
                        .skip(2)
                        .any(|byte| byte.is_ascii_alphabetic() || *byte == b'~')
            }
            Some(_) => true,
            None => false,
        }
    }

    fn next(&mut self, now: Instant) -> Input {
        if self.quit {
            return Input::Quit;
        }
        let Some(&first) = self.pending.front() else {
            return Input::Idle;
        };
        if first == 27 {
            let since = *self.escape_since.get_or_insert(now);
            let expired = now.duration_since(since) >= ESCAPE_DELAY;
            if self.pending.len() == 1 {
                if !expired {
                    return Input::Idle;
                }
                self.pending.pop_front();
                self.escape_since = None;
                return Input::Key(Key::Escape);
            }
            if !matches!(self.pending.get(1), Some(b'[' | b'O')) {
                self.pending.pop_front();
                self.escape_since = None;
                return Input::Key(Key::Escape);
            }
            let end = self
                .pending
                .iter()
                .enumerate()
                .skip(2)
                .take(8)
                .find(|(_, byte)| byte.is_ascii_alphabetic() || **byte == b'~')
                .map(|(index, _)| index + 1);
            let count = if let Some(end) = end {
                end
            } else if expired || self.pending.len() >= 10 {
                self.pending.len().min(10)
            } else {
                return Input::Idle;
            };
            let sequence: Vec<_> = self.pending.drain(..count).skip(2).collect();
            self.escape_since = None;
            return Input::Key(match sequence.as_slice() {
                b"D" => Key::Backspace,
                b"C" | b"P" | b"11~" => Key::RecallCharacter,
                b"H" | b"1~" | b"15~" => Key::EraseLine,
                b"F" | b"4~" | b"R" | b"13~" => Key::RecallLine,
                _ => Key::Ignored,
            });
        }
        self.pending.pop_front();
        self.escape_since = None;
        match first {
            3 | 4 | 17 => {
                self.quit = true;
                Input::Quit
            }
            10 | 13 => Input::Key(Key::Enter),
            127 => Input::Key(Key::Backspace),
            byte => Input::Key(Key::from_dos_byte(byte)),
        }
    }
}

// Translate Windows console key records independently of host calls so the
// editor mappings can also be checked with synthetic fixtures on other hosts.
#[cfg(any(windows, test))]
fn windows_key_byte(virtual_key: u16, unicode: u16, modifiers: u32) -> Option<u8> {
    const CTRL: u32 = 0x000c;
    const ALT: u32 = 0x0003;
    if modifiers & CTRL != 0 && modifiers & ALT == 0 {
        match virtual_key {
            0x43 => return Some(3),
            0x44 => return Some(4),
            0x51 => return Some(17),
            0x53 => return Some(19),
            _ => {}
        }
    }
    match virtual_key {
        0x08 => Some(8),
        0x09 => Some(9),
        0x0d => Some(13),
        0x1b => Some(27),
        0x25 => Some(8),                     // Left
        0x27 | 0x70 => Some(0x1c),           // Right / F1
        0x24 | 0x74 => Some(0xc8),           // Home / F5
        0x23 | 0x72 => Some(0xc9),           // End / F3
        0x10..=0x12 if unicode == 0 => None, // Bare modifier
        _ if (1..=127).contains(&unicode) => Some(unicode as u8),
        _ => Some(0x1e), // Unmapped keys and non-ASCII input are ignored.
    }
}

#[cfg(windows)]
mod windows {
    use std::{ffi::c_void, io, time::Duration};

    type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct KeyEvent {
        down: i32,
        repeat: u16,
        virtual_key: u16,
        scan_code: u16,
        unicode: u16,
        modifiers: u32,
    }

    // INPUT_RECORD has a WORD tag and an aligned 16-byte event union. All
    // event payload bits fit these integer fields; inspect them only for KEY_EVENT.
    #[repr(C)]
    #[derive(Default)]
    struct InputRecord {
        event_type: u16,
        padding: u16,
        key: KeyEvent,
    }

    const _: () = assert!(std::mem::size_of::<KeyEvent>() == 16);
    const _: () = assert!(std::mem::size_of::<InputRecord>() == 20);
    const _: () = assert!(std::mem::align_of::<InputRecord>() == 4);

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetStdHandle"]
        fn get_std_handle(kind: u32) -> Handle;
        #[link_name = "GetConsoleMode"]
        fn get_console_mode(handle: Handle, mode: *mut u32) -> i32;
        #[link_name = "SetConsoleMode"]
        fn set_console_mode(handle: Handle, mode: u32) -> i32;
        #[link_name = "GetConsoleOutputCP"]
        fn get_console_output_cp() -> u32;
        #[link_name = "SetConsoleOutputCP"]
        fn set_console_output_cp(codepage: u32) -> i32;
        #[link_name = "GetNumberOfConsoleInputEvents"]
        fn get_input_event_count(handle: Handle, count: *mut u32) -> i32;
        #[link_name = "ReadConsoleInputW"]
        fn read_console_input(
            handle: Handle,
            records: *mut InputRecord,
            count: u32,
            read: *mut u32,
        ) -> i32;
        #[link_name = "ReadConsoleW"]
        fn read_console(
            handle: Handle,
            buffer: *mut u16,
            count: u32,
            read: *mut u32,
            control: *const c_void,
        ) -> i32;
        #[link_name = "WaitForSingleObject"]
        fn wait_for_single_object(handle: Handle, milliseconds: u32) -> u32;
    }

    fn check(result: i32) -> io::Result<()> {
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn read_prompt_line() -> io::Result<String> {
        // SAFETY: the standard input selector only borrows a process handle.
        let input = unsafe { get_std_handle((-10i32) as u32) };
        if input.is_null() || input as isize == -1 {
            return Err(io::Error::other(
                "filename input requires an attached Windows console",
            ));
        }
        let mut line = Vec::new();
        loop {
            let mut unit = 0;
            let mut read = 0;
            // SAFETY: one UTF-16 unit and a count output are writable. The raw
            // adapter has already restored cooked mode. Requesting one unit
            // avoids consuming any command following the filename's newline.
            check(unsafe { read_console(input, &mut unit, 1, &mut read, std::ptr::null()) })?;
            if read == 0 {
                break;
            }
            line.push(unit);
            if unit == u16::from(b'\n') {
                break;
            }
        }
        String::from_utf16(&line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub(super) struct Console {
        input: Handle,
        output: Handle,
        input_mode: u32,
        output_mode: u32,
        output_cp: u32,
        repeated: Option<(u8, u16)>,
    }

    impl Console {
        pub(super) fn open() -> io::Result<Self> {
            // SAFETY: these well-known standard-handle selectors borrow the
            // process's console handles. They are validated and never closed here.
            let (input, output) = unsafe {
                (
                    get_std_handle((-10i32) as u32),
                    get_std_handle((-11i32) as u32),
                )
            };
            if [input, output]
                .into_iter()
                .any(|handle| handle.is_null() || handle as isize == -1)
            {
                return Err(io::Error::other(
                    "terminal play requires an attached Windows console",
                ));
            }
            let (mut input_mode, mut output_mode) = (0, 0);
            // SAFETY: handles are borrowed standard handles and mode outputs are
            // valid writable u32 pointers. Failure leaves the console unchanged.
            unsafe {
                check(get_console_mode(input, &mut input_mode))?;
                check(get_console_mode(output, &mut output_mode))?;
            }
            // SAFETY: this reads the attached console's output code page.
            let output_cp = unsafe { get_console_output_cp() };
            if output_cp == 0 {
                return Err(io::Error::last_os_error());
            }
            let console = Self {
                input,
                output,
                input_mode,
                output_mode,
                output_cp,
                repeated: None,
            };
            // Clear processed/line/echo, mouse/window, Quick Edit and VT input;
            // ReadConsoleInputW supplies native key records without ANSI decoding.
            let raw_input = (input_mode
                & !(0x0001 | 0x0002 | 0x0004 | 0x0008 | 0x0010 | 0x0040 | 0x0200))
                | 0x0080;
            // Process VT output and defer last-column wrap until the next byte.
            let vt_output = output_mode | 0x0001 | 0x0004 | 0x0008;
            // SAFETY: both handles were verified as console handles. The RAII
            // owner above restores every original setting on any partial failure.
            unsafe {
                check(set_console_mode(input, raw_input))?;
                check(set_console_mode(output, vt_output))?;
                check(set_console_output_cp(65001))?;
            }
            Ok(console)
        }

        pub(super) fn read_byte(&mut self) -> io::Result<Option<u8>> {
            if let Some((byte, remaining)) = self.repeated.take() {
                if remaining > 1 {
                    self.repeated = Some((byte, remaining - 1));
                }
                return Ok(Some(byte));
            }
            loop {
                let mut available = 0;
                // SAFETY: input is the owned adapter's borrowed console handle;
                // available is writable. This query does not consume input.
                check(unsafe { get_input_event_count(self.input, &mut available) })?;
                if available == 0 {
                    return Ok(None);
                }
                let mut record = InputRecord::default();
                let mut read = 0;
                // SAFETY: one complete INPUT_RECORD-sized writable object is
                // provided after observing queued events; this adapter is the
                // sole reader while raw input is active. Read no later records
                // so keys after a submission remain available to cooked prompts.
                check(unsafe { read_console_input(self.input, &mut record, 1, &mut read) })?;
                if read == 0 {
                    return Ok(None);
                }
                if record.event_type != 1 || record.key.down == 0 {
                    continue;
                }
                let Some(byte) = super::windows_key_byte(
                    record.key.virtual_key,
                    record.key.unicode,
                    record.key.modifiers,
                ) else {
                    continue;
                };
                if record.key.repeat > 1 {
                    self.repeated = Some((byte, record.key.repeat - 1));
                }
                return Ok(Some(byte));
            }
        }

        pub(super) fn wait(&self, duration: Duration) -> io::Result<()> {
            if self.repeated.is_some() {
                return Ok(());
            }
            let milliseconds = duration.as_millis()
                + u128::from(!duration.subsec_nanos().is_multiple_of(1_000_000));
            // MAX is reserved for infinite waits. This only observes readiness;
            // the caller refreshes its deadline before actually reading a key.
            let milliseconds = milliseconds.min(u128::from(u32::MAX - 1)) as u32;
            // SAFETY: input is a valid borrowed console input handle and waiting
            // neither transfers ownership nor reads any events.
            if unsafe { wait_for_single_object(self.input, milliseconds) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    impl Drop for Console {
        fn drop(&mut self) {
            // SAFETY: restore only the settings captured from these borrowed
            // console handles. Restoration must also run after partial setup.
            unsafe {
                let _ = set_console_mode(self.input, self.input_mode);
                let _ = set_console_mode(self.output, self.output_mode);
                let _ = set_console_output_cp(self.output_cp);
            }
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        #[cfg(unix)]
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_ansi_sequence_survives_empty_polls() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.push_back(27);
        assert_eq!(decoder.next(now), Input::Idle);
        assert!(!decoder.ready(now));
        decoder.pending.push_back(b'[');
        assert_eq!(decoder.next(now + Duration::from_millis(10)), Input::Idle);
        decoder.pending.extend(b"13~");
        assert!(decoder.ready(now + Duration::from_millis(20)));
        assert_eq!(
            decoder.next(now + Duration::from_millis(20)),
            Input::Key(Key::RecallLine)
        );
        assert_eq!(decoder.next(now + ESCAPE_DELAY), Input::Idle);
    }

    #[test]
    fn bare_escape_pauses_only_after_disambiguation_timeout() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.push_back(27);
        assert_eq!(decoder.next(now), Input::Idle);
        assert_eq!(decoder.next(now + Duration::from_millis(29)), Input::Idle);
        assert!(decoder.ready(now + ESCAPE_DELAY));
        assert_eq!(decoder.next(now + ESCAPE_DELAY), Input::Key(Key::Escape));
    }

    #[test]
    fn escape_before_ordinary_key_preserves_that_key() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.extend([27, b'a']);
        assert_eq!(decoder.next(now), Input::Key(Key::Escape));
        assert_eq!(decoder.next(now), Input::Key(Key::Character(b'a')));
    }

    #[test]
    fn incomplete_ansi_expires_without_leaking_prefix_bytes() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.extend(b"\x1b[1");
        assert_eq!(decoder.next(now), Input::Idle);
        assert_eq!(decoder.next(now + ESCAPE_DELAY), Input::Key(Key::Ignored));
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn submission_leaves_later_keys_queued_and_quit_is_sticky() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.extend(b"x\ry\x03z");
        assert_eq!(decoder.next(now), Input::Key(Key::Character(b'x')));
        assert_eq!(decoder.next(now), Input::Key(Key::Enter));
        assert_eq!(decoder.pending.front(), Some(&b'y'));
        assert_eq!(decoder.next(now), Input::Key(Key::Character(b'y')));
        assert_eq!(decoder.next(now), Input::Quit);
        assert!(decoder.quit);
        assert_eq!(decoder.next(now), Input::Quit);
        assert_eq!(decoder.pending.front(), Some(&b'z'));
    }

    #[test]
    fn control_q_quits_without_consuming_later_typeahead() {
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.extend([17, b'x']);
        assert_eq!(decoder.next(Instant::now()), Input::Quit);
        assert_eq!(decoder.pending.front(), Some(&b'x'));
        assert_eq!(decoder.next(Instant::now()), Input::Quit);
    }

    #[test]
    fn windows_console_keys_match_the_existing_editor_and_host_shortcuts() {
        for (virtual_key, expected) in [
            (0x08, Key::Backspace),
            (0x09, Key::Tab),
            (0x0d, Key::Enter),
            (0x1b, Key::Escape),
            (0x25, Key::Backspace),
            (0x27, Key::RecallCharacter),
            (0x70, Key::RecallCharacter),
            (0x24, Key::EraseLine),
            (0x74, Key::EraseLine),
            (0x23, Key::RecallLine),
            (0x72, Key::RecallLine),
            (0x26, Key::Ignored),
            (0x28, Key::Ignored),
        ] {
            let byte = windows_key_byte(virtual_key, 0, 0).unwrap();
            assert_eq!(Key::from_dos_byte(byte), expected);
        }
        assert_eq!(windows_key_byte(0x41, u16::from(b'A'), 0), Some(b'A'));
        assert_eq!(windows_key_byte(0x41, u16::from(b'a'), 0), Some(b'a'));
        assert_eq!(windows_key_byte(0x10, 0, 0), None);
        assert_eq!(windows_key_byte(0x41, 0x00e9, 0), Some(0x1e));
        for modifier in [4, 8] {
            assert_eq!(windows_key_byte(0x43, 0, modifier), Some(3));
            assert_eq!(windows_key_byte(0x44, 0, modifier), Some(4));
            assert_eq!(windows_key_byte(0x51, 0, modifier), Some(17));
            assert_eq!(windows_key_byte(0x53, 0, modifier), Some(19));
        }
        // AltGr text must not become a Ctrl-Q host shortcut.
        assert_eq!(windows_key_byte(0x51, u16::from(b'@'), 4 | 2), Some(b'@'));
    }
}
