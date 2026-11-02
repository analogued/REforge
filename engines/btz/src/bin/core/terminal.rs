// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Terminal-only development adapter. The engine itself consumes typed key events.

use std::collections::VecDeque;
use std::io;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use btz::input::Key;

#[derive(Debug, Eq, PartialEq)]
pub enum Input {
    Idle,
    Key(Key),
    Quit,
}

pub struct Terminal {
    decoder: KeyboardDecoder,
    #[cfg(unix)]
    saved: String,
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
        #[cfg(not(unix))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "nonblocking terminal input is only supported on Unix",
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
        #[cfg(not(unix))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "nonblocking terminal input is only supported on Unix",
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
            let milliseconds =
                duration.as_millis() + u128::from(duration.subsec_nanos() % 1_000_000 != 0);
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
        #[cfg(not(unix))]
        {
            let _ = duration;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "nonblocking terminal input is only supported on Unix",
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
                return Input::Key(Key::Pause);
            }
            if !matches!(self.pending.get(1), Some(b'[' | b'O')) {
                self.pending.pop_front();
                self.escape_since = None;
                return Input::Key(Key::Pause);
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
            3 | 4 => {
                self.quit = true;
                Input::Quit
            }
            10 | 13 => Input::Key(Key::Enter),
            127 => Input::Key(Key::Backspace),
            byte => Input::Key(Key::from_dos_byte(byte)),
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
        assert_eq!(decoder.next(now + ESCAPE_DELAY), Input::Key(Key::Pause));
    }

    #[test]
    fn escape_before_ordinary_key_preserves_that_key() {
        let now = Instant::now();
        let mut decoder = KeyboardDecoder::default();
        decoder.pending.extend([27, b'a']);
        assert_eq!(decoder.next(now), Input::Key(Key::Pause));
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
}
