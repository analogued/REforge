// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "dev-tools")]

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "btz-frontend-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let mut story = vec![0; 7 * 512];
        put(&mut story, 0, 512);
        for (offset, value) in [
            (0xe8, 0x40),
            (0x112, 0xffff),
            (0xdc, 0xffff),
            (0xde, 0xffff),
        ] {
            put(&mut story, offset, value);
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
            put(&mut story, 512 + offset, value);
        }
        // An authored counter loop executes 150,002 VM instructions before
        // requesting native exit, crossing the former whole-session cap.
        let program = [
            0x002c, 0, 0x0030, 0x120, 0x012c, 0x120, 0x0040, 1, 0x0030, 0x120, 0x0028, 30_000,
            0x0010, 0x48, 0x0064, 4,
        ];
        for (index, word) in program.into_iter().enumerate() {
            put(&mut story, 512 + 0x40 + index * 2, word);
        }
        let mut exe = vec![0; 32 + 0x6cb];
        exe[..2].copy_from_slice(b"MZ");
        put(&mut exe, 8, 2);
        fs::write(path.join("story.obj"), story).unwrap();
        fs::write(path.join("interpreter.exe"), exe).unwrap();
        fs::write(path.join("keys.bin"), []).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_btz-core"))
            .arg("--original")
            .arg(self.0.join("story.obj"))
            .arg("--interpreter")
            .arg(self.0.join("interpreter.exe"))
            .arg("--input")
            .arg(self.0.join("keys.bin"))
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn unlimited_replay_reaches_exit_beyond_default_diagnostic_limit() {
    let fixture = Fixture::new();
    let limited = fixture.run(&[]);
    assert!(limited.status.success());
    assert!(String::from_utf8_lossy(&limited.stderr).contains("instruction budget exhausted"));
    assert!(String::from_utf8_lossy(&limited.stdout).contains('╔'));

    let unlimited = fixture.run(&["--unlimited"]);
    assert!(
        unlimited.status.success(),
        "{}",
        String::from_utf8_lossy(&unlimited.stderr)
    );
    assert!(!String::from_utf8_lossy(&unlimited.stderr).contains("instruction budget exhausted"));
    // Native exit clears the screen; the bounded run never reaches this state.
    assert!(unlimited.stdout.iter().all(u8::is_ascii_whitespace));

    let explicit = fixture.run(&["7"]);
    assert!(explicit.status.success());
    assert!(String::from_utf8_lossy(&explicit.stderr).contains("instruction budget exhausted"));
}

#[test]
fn unlimited_and_explicit_limit_are_mutually_exclusive() {
    let output = Fixture::new().run(&["--unlimited", "7"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--unlimited cannot be combined"));
}

#[test]
fn original_inputs_accept_upper_lower_and_mixed_case_filenames() {
    for (data_name, exe_name, map_name) in [
        ("ADISK.OBJ", "GO.EXE", "SIDE.TXT"),
        ("adisk.obj", "go.exe", "side.txt"),
        ("AdIsK.ObJ", "Go.ExE", "SiDe.TxT"),
    ] {
        let fixture = Fixture::new();
        fs::rename(fixture.0.join("story.obj"), fixture.0.join(data_name)).unwrap();
        fs::rename(fixture.0.join("interpreter.exe"), fixture.0.join(exe_name)).unwrap();
        fs::write(fixture.0.join(map_name), "0\na\n".repeat(10) + "0\n").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_btz-core"))
            .current_dir(&fixture.0)
            .args([
                "--original",
                "adisk.obj",
                "--interpreter",
                "Go.exe",
                "--side-map",
                "SIDE.TXT",
                "--input",
                "keys.bin",
                "--unlimited",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{exe_name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.iter().all(u8::is_ascii_whitespace));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("instruction budget exhausted"));
    }
}

#[test]
fn original_inputs_reject_ambiguous_case_even_when_an_exact_match_exists() {
    for (original, duplicate) in [
        ("story.obj", "STORY.OBJ"),
        ("interpreter.exe", "INTERPRETER.EXE"),
        ("side.txt", "SIDE.TXT"),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("side.txt"), "0\na\n".repeat(10) + "0\n").unwrap();
        fs::write(
            fixture.0.join(duplicate),
            fs::read(fixture.0.join(original)).unwrap(),
        )
        .unwrap();
        // Filesystems that fold case cannot hold two distinct candidates.
        if fs::read_dir(&fixture.0).unwrap().count() != 5 {
            continue;
        }
        let map = fixture.0.join("side.txt");
        let output = fixture.run(&["--side-map", map.to_str().unwrap()]);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("ambiguous original game filename"),
            "{error}"
        );
    }
}

#[cfg(unix)]
mod terminal_input {
    use super::*;
    use std::{
        fs::File,
        io::{self, Read, Write},
        os::fd::FromRawFd,
        process::{Child, Stdio},
        time::{Duration, Instant},
    };

    struct PtyProcess {
        child: Child,
        master: File,
        captured: Vec<u8>,
    }

    impl PtyProcess {
        fn spawn(fixture: &Fixture, queued: &[u8]) -> Self {
            Self::spawn_with(fixture, queued, |_| {})
        }

        fn spawn_with(
            fixture: &Fixture,
            queued: &[u8],
            configure: impl FnOnce(&mut Command),
        ) -> Self {
            let mut master = -1;
            let mut slave = -1;
            // SAFETY: both descriptor outputs point to writable integers. Null
            // optional arguments request the platform's default terminal mode.
            assert_eq!(
                unsafe {
                    libc::openpty(
                        &mut master,
                        &mut slave,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                        std::ptr::null(),
                    )
                },
                0,
                "{}",
                io::Error::last_os_error()
            );
            // SAFETY: openpty returned two distinct owned descriptors, each
            // transferred to exactly one File and subsequently closed by Drop.
            let mut master = unsafe { File::from_raw_fd(master) };
            let slave = unsafe { File::from_raw_fd(slave) };
            master.write_all(queued).unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_btz-core"));
            command
                .arg("--original")
                .arg(fixture.0.join("story.obj"))
                .arg("--interpreter")
                .arg(fixture.0.join("interpreter.exe"))
                .arg("--trace");
            configure(&mut command);
            let child = command
                .stdin(Stdio::from(slave.try_clone().unwrap()))
                .stdout(Stdio::from(slave.try_clone().unwrap()))
                .stderr(Stdio::from(slave))
                .spawn()
                .unwrap();
            Self {
                child,
                master,
                captured: Vec::new(),
            }
        }

        fn read_until(&mut self, deadline: Instant, condition: impl Fn(&[u8]) -> bool) -> bool {
            use std::os::fd::AsRawFd;
            while !condition(&self.captured) {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return false;
                }
                let mut descriptor = libc::pollfd {
                    fd: self.master.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: descriptor is a live one-element pollfd array; poll
                // only reports availability and leaves the PTY bytes untouched.
                let ready = unsafe {
                    libc::poll(
                        &mut descriptor,
                        1,
                        remaining.as_millis().clamp(1, 20) as i32,
                    )
                };
                if ready < 0 {
                    let error = io::Error::last_os_error();
                    assert_eq!(error.kind(), io::ErrorKind::Interrupted, "{error}");
                    continue;
                }
                if ready == 0 {
                    continue;
                }
                let mut bytes = [0; 8192];
                match self.master.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => self.captured.extend_from_slice(&bytes[..count]),
                    // A closed PTY slave reports EIO on Linux and EOF elsewhere.
                    Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => panic!("PTY read failed: {error}"),
                }
                assert!(
                    self.captured.len() <= 1_048_576,
                    "unbounded frontend output"
                );
            }
            condition(&self.captured)
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.captured).into_owned()
        }

        fn expect_successful_exit(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(status.success(), "{}", self.text());
                    break;
                }
                assert!(Instant::now() < deadline, "{}", self.text());
                self.read_until(
                    deadline.min(Instant::now() + Duration::from_millis(10)),
                    |_| false,
                );
            }
        }
    }

    impl Drop for PtyProcess {
        fn drop(&mut self) {
            // Cleanup also runs if any assertion fails, avoiding orphaned games.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn input_fixture(program: &[u16]) -> Fixture {
        let fixture = Fixture::new();
        let path = fixture.0.join("story.obj");
        let mut story = fs::read(&path).unwrap();
        for (offset, value) in [(0xe0, 0x200), (0xee, 0x80), (0x10c, 0x120), (0x10e, 0x140)] {
            put(&mut story, offset, value);
        }
        story[512 + 0x40..512 + 0x80].fill(0);
        for (index, &word) in program.iter().enumerate() {
            put(&mut story, 512 + 0x40 + index * 2, word);
        }
        fs::write(path, story).unwrap();
        fixture
    }

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    fn has_read_result(bytes: &[u8]) -> bool {
        contains(bytes, b"parsed:") || contains(bytes, b"read timed out:")
    }

    #[test]
    fn queued_enter_is_serviced_by_form_one_before_an_expired_read() {
        // The original form1 keyboard service submits this queued Enter before
        // read0 checks its expired deadline. With a swallowed poll it times out.
        let fixture = input_fixture(&[0x0035, 0xffff, 0xffff, 0x002c, 0, 0x0064, 1, 0x0064, 4]);
        let mut process = PtyProcess::spawn(&fixture, b"\r");
        assert!(
            process.read_until(Instant::now() + Duration::from_secs(3), has_read_result),
            "{}",
            process.text()
        );
        assert!(process.text().contains("parsed:"), "{}", process.text());
        assert!(
            !process.text().contains("read timed out:"),
            "{}",
            process.text()
        );
        process.expect_successful_exit();
    }

    #[test]
    fn terminal_enter_respects_the_two_tick_input_deadline() {
        // Keep the child alive at a separate key wait after its one read, so a
        // late CR can be delivered without racing a closed PTY. Compare logical
        // events, rather than asserting precise scheduler/wall-clock durations.
        let fixture = input_fixture(&[0x002c, 2, 0x0064, 1, 0x0064, 3, 0x0064, 4]);
        for (delay, expected) in [
            (Duration::from_millis(25), "parsed:"),
            (Duration::from_millis(150), "read timed out:"),
        ] {
            let mut process = PtyProcess::spawn(&fixture, &[]);
            assert!(
                process.read_until(Instant::now() + Duration::from_secs(3), |bytes| {
                    contains(bytes, b"\x1b[2J\x1b[H")
                }),
                "{}",
                process.text()
            );
            let send_at = Instant::now() + delay;
            process.read_until(send_at, |_| false);
            process.master.write_all(b"\r").unwrap();
            assert!(
                process.read_until(Instant::now() + Duration::from_secs(3), has_read_result),
                "{}",
                process.text()
            );
            let text = process.text();
            let first_result = text
                .lines()
                .find(|line| line.contains("parsed:") || line.contains("read timed out:"))
                .unwrap();
            assert!(first_result.contains(expected), "delay={delay:?}: {text}");
        }
    }

    #[test]
    fn terminal_callback_bell_does_not_wait_for_command_submission() {
        let fixture = input_fixture(&[0x002c, 0xffff, 0x0064, 1, 0x0064, 4]);
        let mut process = PtyProcess::spawn(&fixture, &[]);
        assert!(
            process.read_until(Instant::now() + Duration::from_secs(3), |bytes| {
                contains(bytes, b"\x1b[2J\x1b[H")
            })
        );
        process.master.write_all(&[7]).unwrap();
        assert!(
            process.read_until(Instant::now() + Duration::from_secs(1), |bytes| bytes
                .contains(&7)),
            "{}",
            process.text()
        );
    }

    #[test]
    fn prompted_bookmarks_and_transcripts_use_the_game_directory_or_save_dir_override() {
        for (use_override, absolute_prompt) in [(false, false), (true, false), (true, true)] {
            let fixture = input_fixture(&[0x0064, 5, 0x0064, 6, 0x0064, 8, 0x0064, 9, 0x0064, 4]);
            let side_map = fixture.0.join("side.txt");
            fs::write(&side_map, "0\na\n".repeat(10) + "0\n").unwrap();
            let cwd = fixture.0.join("unrelated-working-directory");
            let override_dir = fixture.0.join("chosen-save-directory");
            let absolute_dir = fixture.0.join("absolute-prompt-directory");
            fs::create_dir(&cwd).unwrap();
            fs::create_dir(&override_dir).unwrap();
            fs::create_dir(&absolute_dir).unwrap();
            let expected_dir = if absolute_prompt {
                &absolute_dir
            } else if use_override {
                &override_dir
            } else {
                &fixture.0
            };
            let bookmark = expected_dir.join("checkpoint.bkm");
            let transcript = expected_dir.join("session.txt");
            let mut process = PtyProcess::spawn_with(&fixture, &[], |command| {
                command.current_dir(&cwd).arg("--side-map").arg(&side_map);
                if use_override {
                    command.arg("--save-dir").arg(&override_dir);
                }
            });
            for (prompt, filename) in [
                (
                    b"Bookmark file to write (blank cancels): ".as_slice(),
                    &bookmark,
                ),
                (
                    b"Bookmark file to read (blank cancels): ".as_slice(),
                    &bookmark,
                ),
                (b"Transcript file (blank cancels): ".as_slice(), &transcript),
            ] {
                assert!(
                    process.read_until(Instant::now() + Duration::from_secs(3), |bytes| {
                        contains(bytes, prompt)
                    }),
                    "override={use_override}, absolute={absolute_prompt}: {}",
                    process.text()
                );
                let name = if absolute_prompt {
                    filename.as_os_str()
                } else {
                    filename.file_name().unwrap()
                };
                process
                    .master
                    .write_all(name.to_str().unwrap().as_bytes())
                    .unwrap();
                process.master.write_all(b"\n").unwrap();
            }
            process.expect_successful_exit();
            assert!(!process.text().contains("Cannot "), "{}", process.text());
            assert_eq!(fs::metadata(&bookmark).unwrap().len(), 512 + 722);
            assert!(transcript.is_file());
            assert!(!cwd.join("checkpoint.bkm").exists());
            assert!(!cwd.join("session.txt").exists());
            if use_override {
                assert!(!fixture.0.join("checkpoint.bkm").exists());
                assert!(!fixture.0.join("session.txt").exists());
            }
            if absolute_prompt {
                assert!(!override_dir.join("checkpoint.bkm").exists());
                assert!(!override_dir.join("session.txt").exists());
            }
        }
    }

    #[test]
    fn mounted_media_resolves_original_filenames_without_case_sensitivity() {
        // A same-medium drive change requests a folder, then loads its primary
        // text stores. Both required files intentionally differ from DOS casing.
        let fixture = input_fixture(&[0x002c, 1, 0x0064, 14, 0x0064, 4]);
        let mapping = "0\na\n".to_owned() + &"0\nb\n".repeat(9) + "0\n";
        let side_map = fixture.0.join("side.txt");
        fs::write(&side_map, &mapping).unwrap();
        let media = fixture.0.join("Additional Media");
        fs::create_dir(&media).unwrap();
        fs::copy(fixture.0.join("story.obj"), media.join("aDiSk.ObJ")).unwrap();
        fs::write(media.join("sIdE.tXt"), mapping).unwrap();
        let mut process = PtyProcess::spawn_with(&fixture, &[], |command| {
            command.arg("--side-map").arg(&side_map);
        });
        assert!(
            process.read_until(Instant::now() + Duration::from_secs(3), |bytes| {
                contains(bytes, b"Folder containing program disk 1 (blank cancels): ")
            }),
            "{}",
            process.text()
        );
        process
            .master
            .write_all(media.to_str().unwrap().as_bytes())
            .unwrap();
        process.master.write_all(b"\n").unwrap();
        process.expect_successful_exit();
    }
}
