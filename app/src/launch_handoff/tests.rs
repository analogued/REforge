// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::time::SystemTime;
use std::{
    io::Write,
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    directory: PathBuf,
    original: PathBuf,
    detected: Detected,
}

impl Fixture {
    fn new(release: &'static detection::Release) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "reforge-handoff-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(directory.join("DATA")).unwrap();
        let files = release
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let path = directory.join(format!("DATA/renamed-{index}.bin"));
                // Authored placeholders deliberately do NOT have catalog hashes.
                // Accepting a ticket proves that the child did not run detection.
                let output = File::create(&path).unwrap();
                output.set_len(file.size).unwrap();
                (file.name, path)
            })
            .collect::<BTreeMap<_, _>>();
        let original = files[release.files[0].name].clone();
        let detected = Detected {
            game: release.game(),
            release,
            root: directory.clone(),
            original: if release.game() == Game::AlleyCat {
                original.clone()
            } else {
                directory.clone()
            },
            files,
        };
        Self {
            directory,
            original,
            detected,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[test]
fn all_releases_keep_their_verified_paths_without_another_detector_call() {
    for release in detection::RELEASES {
        let fixture = Fixture::new(release);
        let ticket = Ticket::new(&fixture.original, &fixture.detected).unwrap();
        let found = resolve(&fixture.original, Some(&ticket.path), |_| {
            panic!("duplicate full verification")
        })
        .unwrap();
        assert_eq!(found.game, fixture.detected.game);
        assert_eq!(found.release.id, release.id);
        assert_eq!(found.root, fixture.detected.root);
        assert_eq!(found.original, fixture.detected.original);
        assert_eq!(found.files, fixture.detected.files);
        assert!(!ticket.path.exists());
    }
}

#[test]
fn ordinary_cli_launches_still_verify_original_contents() {
    let fixture = Fixture::new(&detection::RELEASES[0]);
    assert!(resolve(&fixture.original, None, detection::detect).is_err());
    let mut calls = 0;
    let error = resolve(&fixture.original, None, |path| {
        calls += 1;
        assert_eq!(path, fixture.original);
        Err("full verification".into())
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error, "full verification");
}

#[test]
fn changed_files_fall_back_to_full_verification() {
    for change in ["size", "modified", "missing", "directory"] {
        let fixture = Fixture::new(&detection::RELEASES[0]);
        let ticket = Ticket::new(&fixture.original, &fixture.detected).unwrap();
        match change {
            "size" => OpenOptions::new()
                .append(true)
                .open(&fixture.original)
                .unwrap()
                .write_all(b"changed")
                .unwrap(),
            "modified" => {
                let modified = fs::metadata(&fixture.original).unwrap().modified().unwrap()
                    + Duration::from_secs(5);
                File::options()
                    .write(true)
                    .open(&fixture.original)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .unwrap();
            }
            "missing" => fs::remove_file(&fixture.original).unwrap(),
            _ => {
                fs::remove_file(&fixture.original).unwrap();
                fs::create_dir(&fixture.original).unwrap();
            }
        }
        let mut calls = 0;
        assert!(
            resolve(&fixture.original, Some(&ticket.path), |_| {
                calls += 1;
                Err("full verification".into())
            })
            .is_err()
        );
        assert_eq!(calls, 1, "{change}");
    }
}

#[test]
fn missing_malformed_mismatched_and_consumed_tickets_cannot_skip_verification() {
    let fixture = Fixture::new(&detection::RELEASES[0]);
    let ticket = Ticket::new(&fixture.original, &fixture.detected).unwrap();
    let bytes = fs::read(&ticket.path).unwrap();
    let mut legacy = bytes.clone();
    legacy[..MAGIC.len()].copy_from_slice(b"reforge-verified-launch-v1\0");
    assert!(decode(&legacy, &fixture.original).is_err());
    for length in [0, MAGIC.len() - 1, MAGIC.len(), bytes.len() - 1] {
        assert!(decode(&bytes[..length], &fixture.original).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode(&extra, &fixture.original).is_err());
    let mut other_catalog = bytes.clone();
    let hash = fixture.detected.release.files[0].blake3.as_bytes();
    let index = other_catalog
        .windows(hash.len())
        .position(|window| window == hash)
        .unwrap();
    other_catalog[index] = b'!';
    assert!(decode(&other_catalog, &fixture.original).is_err());
    assert!(receive(&ticket.path, &fixture.directory).is_err());
    assert!(receive(&ticket.path, &fixture.original).is_ok());
    let mut calls = 0;
    assert!(
        resolve(&fixture.original, Some(&ticket.path), |_| {
            calls += 1;
            Err("full verification".into())
        })
        .is_err()
    );
    assert_eq!(calls, 1);
}

#[test]
fn failed_child_start_removes_the_unused_ticket() {
    let fixture = Fixture::new(&detection::RELEASES[0]);
    let path;
    {
        let ticket = Ticket::new(&fixture.original, &fixture.detected).unwrap();
        path = ticket.path.clone();
        let mut command = Command::new(fixture.directory.join("absent-program"));
        ticket.attach(&mut command);
        assert!(command.spawn().is_err());
        assert!(path.exists());
    }
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn native_non_utf8_paths_and_file_symlinks_survive_the_handoff() {
    use std::os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    };
    let mut fixture = Fixture::new(&detection::RELEASES[0]);
    let link = fixture
        .directory
        .join(std::ffi::OsString::from_vec(b"start-\xff".to_vec()));
    symlink(&fixture.original, &link).unwrap();
    fixture.detected.original = link.clone();
    fixture
        .detected
        .files
        .insert(fixture.detected.release.files[0].name, link.clone());
    let ticket = Ticket::new(&link, &fixture.detected).unwrap();
    assert_eq!(
        fs::metadata(&ticket.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let found = receive(&ticket.path, &link).unwrap();
    assert_eq!(found.original, link);
    assert_eq!(found.files, fixture.detected.files);
}

#[test]
fn child_process_receives_verified_paths_and_retains_stdin() {
    let fixture = Fixture::new(&detection::RELEASES[0]);
    let ticket = Ticket::new(&fixture.original, &fixture.detected).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "launch_handoff::tests::child_process_fixture",
            "--nocapture",
        ])
        .env("REFORGE_HANDOFF_TEST_ORIGINAL", &fixture.original)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    ticket.attach(&mut command);
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"input\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("handoff accepted; stdin preserved"));
    assert!(!ticket.path.exists());
}

#[test]
fn child_process_fixture() {
    let Some(original) = std::env::var_os("REFORGE_HANDOFF_TEST_ORIGINAL") else {
        return;
    };
    crate::run([
        std::ffi::OsString::from("--original"),
        original,
        std::ffi::OsString::from("--help"),
    ])
    .unwrap();
    let mut input = [0; 6];
    std::io::stdin().read_exact(&mut input).unwrap();
    assert_eq!(&input, b"input\n");
    println!("handoff accepted; stdin preserved");
}
