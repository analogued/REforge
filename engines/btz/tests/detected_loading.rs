// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "frontend")]

use btz::{
    VarAddr,
    interpreter::{
        BREAKERS_RESOURCES, BRIMSTONE_RESOURCES, DosResources, ESSEX_RESOURCES, MINDWHEEL_RESOURCES,
    },
    player::{DosRelease, Loaded},
    session::SessionStep,
};
use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    directory: PathBuf,
    data: PathBuf,
    interpreter: PathBuf,
    side_map: PathBuf,
    title: PathBuf,
}

impl Fixture {
    fn new(resources: DosResources, label: &[u8; 2]) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "btz-detected-loading-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let fixture = Self {
            data: directory.join("story.data"),
            // Every test uses this basename, including the first three games.
            interpreter: directory.join("RUN.EXE"),
            side_map: directory.join("media.data"),
            title: directory.join("artwork.data"),
            directory,
        };
        fs::write(&fixture.data, story()).unwrap();
        fs::write(&fixture.interpreter, executable(resources, label)).unwrap();
        fs::write(&fixture.side_map, "0\na\n".repeat(10) + "0\n").unwrap();
        fs::write(&fixture.title, title()).unwrap();
        fixture
    }

    fn load(&self, release: DosRelease) -> Result<Loaded, String> {
        btz::player::load_with_title(
            release,
            &self.data,
            &self.interpreter,
            &self.side_map,
            Some(&self.title),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn story() -> Vec<u8> {
    let mut bytes = vec![0; 7 * 512];
    put(&mut bytes, 0, 512);
    for (offset, value) in [
        (0xe8, 0x40),
        (0x112, 0x8000),
        (0x114, 0xffff),
        (0x116, 0xffff),
        (0x134, 0xffff),
        (0x136, 0xffff),
        (0xdc, 0xffff),
        (0xde, 0xffff),
    ] {
        put(&mut bytes, offset, value);
    }
    for (offset, value) in [
        (4, 1),
        (16, 3),
        (14, 3),
        (26, 4),
        (12, 4),
        (24, 5),
        (8, 5),
        (20, 6),
        (10, 6),
        (22, 7),
    ] {
        put(&mut bytes, 512 + offset, value);
    }
    // Byte load/store distinguish Mindwheel from the extended interpreters.
    // Form 6 then distinguishes Breakers' constant from the retained argument.
    for (index, word) in [
        0x2c, 0x1234, 0x3c, 0x191, 0x38, 0x191, 0x30, 0x192, 0x2c, 0x55aa, 0x082e, 0x30, 0x194,
        0x64, 3,
    ]
    .into_iter()
    .enumerate()
    {
        put(&mut bytes, 512 + 0x40 + 2 * index, word);
    }
    // The title is one compressed dictionary word supplied by the runtime.
    put(&mut bytes, 4 * 512 + 2, 1);
    put(&mut bytes, 5 * 512, 2);
    bytes[5 * 512 + 6..5 * 512 + 8].copy_from_slice(b"q\0");
    put(&mut bytes, 6 * 512, 2);
    bytes
}

fn executable(resources: DosResources, label: &[u8; 2]) -> Vec<u8> {
    // Authored table contents cannot match an original interpreter fingerprint.
    let mut exe = vec![0; 32 + 0x780];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    exe[32 + resources.pairs + usize::from(b'q')] = label[0];
    exe[32 + resources.pairs + 256 + usize::from(b'q')] = label[1];
    exe[32 + resources.pause..32 + resources.pause + 2].copy_from_slice(label);
    exe
}

fn title() -> Vec<u8> {
    let mut exe = vec![0; 32 + 0x360];
    exe[..2].copy_from_slice(b"MZ");
    put(&mut exe, 8, 2);
    exe[36..40].copy_from_slice(b"ART\0");
    exe
}

#[test]
fn supplied_game_selects_its_resources_and_instruction_rules_without_recognition() {
    for (id, resources, label, expected_byte, expected_form6) in [
        ("mindwheel", MINDWHEEL_RESOURCES, *b"MW", 0x1234, 0x55aa),
        ("essex", ESSEX_RESOURCES, *b"EX", 0x34, 0x55aa),
        ("brimstone", BRIMSTONE_RESOURCES, *b"BR", 0x34, 0x55aa),
        ("breakers", BREAKERS_RESOURCES, *b"BK", 0x34, 0),
    ] {
        let fixture = Fixture::new(resources, &label);
        let original_story = fs::read(&fixture.data).unwrap();
        let release = DosRelease::from_game_id(id).unwrap();
        let mut loaded = fixture.load(release).unwrap();
        assert_eq!(loaded.presentation.is_some(), id == "breakers");
        assert!(
            loaded
                .session
                .screen()
                .unicode()
                .contains(std::str::from_utf8(&label).unwrap()),
            "wrong title compression table for {id}"
        );
        let mut waiting = false;
        for _ in 0..1000 {
            if loaded.session.advance().unwrap() == SessionStep::WaitKey {
                waiting = true;
                break;
            }
        }
        assert!(waiting, "did not reach the authored wait in {id}");
        assert_eq!(
            loaded.session.vm().variable(VarAddr(0x192)).unwrap(),
            expected_byte,
            "byte operation definition for {id}"
        );
        assert_eq!(
            loaded.session.vm().variable(VarAddr(0x194)).unwrap(),
            expected_form6,
            "operand form definition for {id}"
        );
        assert!(
            loaded
                .session
                .screen()
                .unicode()
                .lines()
                .last()
                .unwrap()
                .contains(std::str::from_utf8(&label).unwrap()),
            "wrong key-wait prompt offset for {id}"
        );
        assert_eq!(fs::read(&fixture.data).unwrap(), original_story);
    }
}

#[test]
fn detected_game_controls_title_requirements_instead_of_executable_names() {
    let fixture = Fixture::new(BREAKERS_RESOURCES, b"BK");
    let missing = btz::player::load(
        DosRelease::Breakers,
        &fixture.data,
        &fixture.interpreter,
        &fixture.side_map,
    )
    .err()
    .expect("Breakers must require its title resource");
    assert!(missing.contains("TITLE.EXE"), "{missing}");
    assert!(
        fixture
            .load(DosRelease::Breakers)
            .unwrap()
            .presentation
            .is_some()
    );

    let fixture = Fixture::new(MINDWHEEL_RESOURCES, b"MW");
    fs::write(&fixture.title, b"unrelated file, not a title executable").unwrap();
    assert!(
        fixture
            .load(DosRelease::Mindwheel)
            .unwrap()
            .presentation
            .is_none()
    );
}

#[test]
fn unknown_reported_game_is_rejected_without_falling_back_to_mindwheel() {
    assert!(DosRelease::from_game_id("").is_none());
    let fixture = Fixture::new(MINDWHEEL_RESOURCES, b"MW");
    let error = btz::cli::run(
        [
            OsString::from("--original"),
            fixture.directory.as_os_str().to_owned(),
        ]
        .into_iter(),
        "unlisted-btz-game",
    )
    .unwrap_err();
    assert!(error.contains("unlisted-btz-game"), "{error}");
}
