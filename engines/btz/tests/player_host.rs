// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "frontend")]

use btz::{
    player::files::Files,
    services::{HostReply, HostRequest},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "btz-player-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn files(&self) -> Files {
        Files::new(&self.0.join("ADISK.OBJ")).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
mod terminal_bell {
    use super::*;
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::{AsRawFd, FromRawFd},
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };

    fn loaded(directory: &Directory) -> btz::player::Loaded {
        loaded_with_program(directory, &[0x002c, 0xffff, 0x0064, 1, 0x0064, 4])
    }

    fn loaded_with_program(directory: &Directory, program: &[u16]) -> btz::player::Loaded {
        let put = |bytes: &mut [u8], offset: usize, value: u16| {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        };
        let mut bytes = vec![0; 7 * 512];
        put(&mut bytes, 0, 512);
        for (offset, value) in [
            (0xe8, 0x40),
            (0x112, 0xffff),
            (0xdc, 0xffff),
            (0xde, 0xffff),
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
        // An unlimited input read; no submitted line or original data required.
        for (index, word) in program.iter().copied().enumerate() {
            put(&mut bytes, 512 + 0x40 + index * 2, word);
        }
        let mut exe = vec![0; 32 + 0x6cb];
        exe[..2].copy_from_slice(b"MZ");
        put(&mut exe, 8, 2);
        fs::write(directory.0.join("ADISK.OBJ"), bytes).unwrap();
        fs::write(directory.0.join("GO.EXE"), exe).unwrap();
        fs::write(directory.0.join("SIDE.TXT"), "0\na\n".repeat(10) + "0\n").unwrap();
        btz::player::load(
            &directory.0.join("ADISK.OBJ"),
            &directory.0.join("GO.EXE"),
            &directory.0.join("SIDE.TXT"),
        )
        .unwrap()
    }

    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn receive(
        master: &mut File,
        until: Instant,
        bytes: &mut Vec<u8>,
        done: impl Fn(&[u8]) -> bool,
    ) -> bool {
        while Instant::now() < until {
            if done(bytes) {
                return true;
            }
            let mut descriptor = libc::pollfd {
                fd: master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor is a live one-element array; poll only reports
            // readiness on the owned master descriptor.
            if unsafe { libc::poll(&mut descriptor, 1, 10) } <= 0 {
                continue;
            }
            let mut block = [0; 8192];
            match master.read(&mut block) {
                Ok(0) => break,
                Ok(count) => bytes.extend_from_slice(&block[..count]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                Err(error) => panic!("PTY read: {error}"),
            }
        }
        done(bytes)
    }

    #[test]
    fn ctrl_g_flushes_bel_during_input_and_respects_mute() {
        const MARKER: &str = "REFORGE_BTZ_TERMINAL_BELL_TEST";
        const TEST: &str = "terminal_bell::ctrl_g_flushes_bel_during_input_and_respects_mute";
        if let Ok(mode) = std::env::var(MARKER) {
            let directory = Directory::new();
            let mut settings = reforge_settings::AudioSettings::default();
            if mode == "disabled" {
                settings.enabled = false;
            }
            if mode == "master-zero" {
                settings.volume = 0;
            }
            if mode == "effects-zero" {
                settings.categories.0[reforge_settings::SoundCategory::Effects.index()] = 0;
            }
            let game = loaded(&directory);
            if mode == "invalid-preferences" {
                let path = directory.0.join("config.toml");
                let original = b"invalid = [";
                fs::write(&path, original).unwrap();
                let _guard = reforge_settings::Launch {
                    game_id: "mindwheel".into(),
                    entry_id: None,
                    path: Some(path.clone()),
                    cli: Default::default(),
                }
                .install();
                btz::cli::run(
                    [
                        "--original".into(),
                        directory.0.as_os_str().into(),
                        "--renderer".into(),
                        "terminal".into(),
                    ]
                    .into_iter(),
                )
                .unwrap();
                assert_eq!(fs::read(path).unwrap(), original);
            } else {
                btz::player::run_terminal(game, settings).unwrap();
            }
            return;
        }
        for mode in [
            "enabled",
            "disabled",
            "master-zero",
            "effects-zero",
            "invalid-preferences",
        ] {
            let (mut master, mut slave) = (-1, -1);
            // SAFETY: output pointers are valid writable integers. Optional
            // name, termios and size pointers are null for system defaults.
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
                0
            );
            // SAFETY: openpty returned two distinct, newly owned descriptors.
            let (mut master, slave) =
                unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                .env(MARKER, mode)
                .stdin(slave.try_clone().unwrap())
                .stdout(slave.try_clone().unwrap())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            drop(slave);
            let mut child = Process(child);
            let mut bytes = Vec::new();
            assert!(
                receive(
                    &mut master,
                    Instant::now() + Duration::from_secs(3),
                    &mut bytes,
                    |b| b.windows(6).any(|w| w == b"\x1b[?25h")
                ),
                "terminal did not draw its initial frame: {}",
                String::from_utf8_lossy(&bytes)
            );
            bytes.clear();
            master.write_all(&[7]).unwrap();
            let bell = receive(
                &mut master,
                Instant::now() + Duration::from_millis(400),
                &mut bytes,
                |b| b.contains(&7),
            );
            assert_eq!(
                bell,
                matches!(mode, "enabled" | "invalid-preferences"),
                "{mode}: {}",
                String::from_utf8_lossy(&bytes)
            );
            master.write_all(&[17]).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(Instant::now() < deadline, "terminal did not exit");
                receive(
                    &mut master,
                    Instant::now() + Duration::from_millis(10),
                    &mut bytes,
                    |_| false,
                );
            }
            let mut stderr = String::new();
            child
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut stderr)
                .unwrap();
            assert_eq!(
                stderr.matches("REforge:").count(),
                usize::from(mode == "invalid-preferences"),
                "{mode}: {stderr}"
            );
        }
    }

    #[test]
    fn terminal_file_browser_retries_confirms_restores_and_keeps_closing_keys_out_of_game() {
        const MARKER: &str = "REFORGE_BTZ_TERMINAL_BROWSER";
        const TEST: &str = "terminal_bell::terminal_file_browser_retries_confirms_restores_and_keeps_closing_keys_out_of_game";
        if let Some(path) = std::env::var_os(MARKER) {
            let directory = std::mem::ManuallyDrop::new(Directory(path.into()));
            let mut game = loaded_with_program(
                &directory,
                &[0x64, 5, 0x64, 6, 0x2c, 0xffff, 0x64, 1, 0x64, 4],
            );
            let data = loop {
                if let btz::session::SessionStep::Host(HostRequest::SaveBookmark { data }) =
                    game.session.advance().unwrap()
                {
                    break data;
                }
            };
            fs::write(directory.0.join("EXISTING"), &data).unwrap();
            fs::write(directory.0.join("expected.bin"), &data).unwrap();
            btz::player::run_terminal_with_settings(
                game,
                reforge_settings::RuntimeSettings {
                    btz: reforge_settings::BtzSettings {
                        file_cursor: reforge_settings::FileCursorStyle::SteadyBlock,
                    },
                    ..Default::default()
                },
                "Mindwheel",
            )
            .unwrap();
            return;
        }
        let directory = Directory::new();
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: valid descriptor outputs; null optional termios/size defaults.
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
            0
        );
        // SAFETY: openpty transferred ownership of two distinct descriptors.
        let (mut master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(MARKER, &directory.0)
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(slave);
        let mut child = Process(child);
        let mut bytes = Vec::new();
        let expect = |master: &mut File, bytes: &mut Vec<u8>, text: &str| {
            assert!(
                receive(
                    master,
                    Instant::now() + Duration::from_secs(3),
                    bytes,
                    |b| b.windows(text.len()).any(|w| w == text.as_bytes())
                ),
                "missing {text:?}: {}",
                String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(4000)..])
            );
        };
        expect(&mut master, &mut bytes, "Name this saved novel:");
        assert!(String::from_utf8_lossy(&bytes).contains("\x1b[?2004h"));
        bytes.clear();
        master.write_all(b"EXISTING\r").unwrap();
        expect(&mut master, &mut bytes, "Replace existing version?");
        bytes.clear();
        master.write_all(b"\r").unwrap(); // No is the default.
        expect(&mut master, &mut bytes, "Filename:");
        let expected = fs::read(directory.0.join("expected.bin")).unwrap();
        assert_eq!(fs::read(directory.0.join("EXISTING")).unwrap(), expected);
        bytes.clear();
        master.write_all(b"\x01missing/file\r").unwrap();
        expect(
            &mut master,
            &mut bytes,
            "The destination directory does not exist.",
        );
        bytes.clear();
        master.write_all(b"\x01SAVEX\x1b[D\x1b[3~1\r").unwrap();
        expect(
            &mut master,
            &mut bytes,
            "Which version do you want to resume?",
        );
        assert_eq!(fs::read(directory.0.join("SAVE1")).unwrap(), expected);
        bytes.clear();
        master.write_all(b"\t\x01SAVE1\r\n").unwrap();
        expect(&mut master, &mut bytes, "\x1b[?2004l");
        receive(
            &mut master,
            Instant::now() + Duration::from_millis(150),
            &mut bytes,
            |_| false,
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "dialog Enter leaked into the game read"
        );
        assert!(String::from_utf8_lossy(&bytes).contains("\x1b[2 q"));
        master.write_all(b"\x11").unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until, "terminal browser did not exit");
            receive(
                &mut master,
                Instant::now() + Duration::from_millis(10),
                &mut bytes,
                |_| false,
            );
        }
        assert!(String::from_utf8_lossy(&bytes).contains("\x1b[0 q"));
        assert_eq!(fs::read(directory.0.join("EXISTING")).unwrap(), expected);
    }
}

#[test]
fn windowed_filename_submission_uses_the_same_save_directory_and_bytes() {
    let directory = Directory::new();
    let mut files = directory.files();
    let save = HostRequest::SaveBookmark {
        data: b"authored native-state bytes".to_vec(),
    };
    assert!(files.prompt(&save).is_some());
    assert!(matches!(
        files
            .handle_with_path(&save, Some("checkpoint".into()))
            .unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(
        fs::read(directory.0.join("checkpoint")).unwrap(),
        b"authored native-state bytes"
    );
    assert!(
        matches!(files.handle_with_path(&HostRequest::LoadBookmark, Some("checkpoint".into())).unwrap(), Some(HostReply::Bytes(bytes)) if bytes == b"authored native-state bytes")
    );
    assert!(files.take_error().is_none());
}

#[test]
fn cancellation_and_fixed_paths_need_no_console_input() {
    let directory = Directory::new();
    let mut files = directory.files();
    let save = HostRequest::SaveBookmark { data: vec![1, 2] };
    for request in [&save, &HostRequest::LoadBookmark] {
        assert!(matches!(
            files.handle_with_path(request, None).unwrap(),
            Some(HostReply::Cancelled)
        ));
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    files.bookmark_path = Some(directory.0.join("fixed"));
    assert!(files.prompt(&save).is_none());
    assert!(matches!(
        files.handle_with_path(&save, None).unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(fs::read(directory.0.join("fixed")).unwrap(), [1, 2]);
}

#[test]
fn file_errors_are_available_for_windowed_feedback_and_originals_are_protected() {
    let directory = Directory::new();
    let mut files = directory.files();
    let original = directory.0.join("ADISK.OBJ");
    fs::write(&original, b"original").unwrap();
    files.originals.push(original.clone());
    let save = HostRequest::SaveBookmark { data: vec![0] };
    assert!(matches!(
        files
            .handle_with_path(&save, Some(original.clone()))
            .unwrap(),
        Some(HostReply::Cancelled)
    ));
    assert_eq!(fs::read(original).unwrap(), b"original");
    assert!(files.take_error().unwrap().contains("original game input"));
    assert!(files.take_error().is_none());
    assert!(matches!(
        files
            .handle_with_path(&HostRequest::LoadBookmark, Some("missing".into()))
            .unwrap(),
        Some(HostReply::Cancelled)
    ));
    assert!(files.take_error().unwrap().contains("Cannot read bookmark"));
}

#[test]
fn loaded_transcript_paths_are_host_paths_and_preserve_game_directory_default() {
    let directory = Directory::new();
    let mut files = directory.files();
    let request = HostRequest::TranscriptName {
        prompt: Vec::new(),
        max_bytes: 2,
    };
    let reply = files
        .handle_with_path(&request, Some("session.txt".into()))
        .unwrap();
    let Some(HostReply::Name(name)) = reply else {
        panic!("expected transcript name")
    };
    files
        .handle_with_path(&HostRequest::OpenTranscript { path: name }, None)
        .unwrap();
    files
        .handle_with_path(
            &HostRequest::WriteTranscript {
                bytes: b"game and command echoes\r\n".to_vec(),
            },
            None,
        )
        .unwrap();
    files
        .handle_with_path(&HostRequest::CloseTranscript, None)
        .unwrap();
    assert_eq!(
        fs::read(directory.0.join("session.txt")).unwrap(),
        b"game and command echoes\r\n"
    );
}

#[test]
fn a_transcript_hardlink_alias_does_not_truncate_original_data() {
    let directory = Directory::new();
    let mut files = directory.files();
    let original = directory.0.join("ADISK.OBJ");
    let alias = directory.0.join("session.txt");
    fs::write(&original, b"original input").unwrap();
    fs::hard_link(&original, &alias).unwrap();
    files.originals.push(original.clone());
    files.transcript_path = Some(alias.clone());
    files
        .handle_with_path(&HostRequest::OpenTranscript { path: vec![] }, None)
        .unwrap();
    files
        .handle_with_path(
            &HostRequest::WriteTranscript {
                bytes: b"session output".to_vec(),
            },
            None,
        )
        .unwrap();
    files
        .handle_with_path(&HostRequest::CloseTranscript, None)
        .unwrap();
    assert_eq!(fs::read(original).unwrap(), b"original input");
    assert_eq!(fs::read(alias).unwrap(), b"session output");
}

#[test]
fn breakers_bookmark_replacement_requires_a_separate_explicit_answer() {
    let directory = Directory::new();
    let path = directory.0.join("BOOKMARK");
    fs::write(&path, b"old bookmark").unwrap();
    let mut files = directory.files();
    files.confirm_bookmark_replace = true;
    let request = HostRequest::SaveBookmark {
        data: b"new bookmark".to_vec(),
    };
    assert!(
        files
            .handle_with_path(&request, Some(path.clone()))
            .unwrap()
            .is_none()
    );
    assert!(files.replacement_pending());
    assert_eq!(fs::read(&path).unwrap(), b"old bookmark");
    assert!(matches!(
        files.handle_with_path(&request, None).unwrap(),
        Some(HostReply::Cancelled)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"old bookmark");
    files.bookmark_path = Some(path.clone());
    assert!(files.handle_with_path(&request, None).unwrap().is_none());
    assert!(matches!(
        files.handle_with_path(&request, Some("y".into())).unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"new bookmark");
    let unextended = directory.0.join("NEWBOOK");
    files.bookmark_path = None;
    assert!(matches!(
        files
            .handle_with_path(&request, Some(unextended.clone()))
            .unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(fs::read(unextended).unwrap(), b"new bookmark");
    assert!(!directory.0.join("NEWBOOK.SAV").exists());
}
