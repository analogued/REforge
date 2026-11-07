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

    fn inputs(&self, release: DosRelease) -> reforge_inputs::ResolvedFiles {
        reforge_inputs::ResolvedFiles::new([
            ("ADISK.OBJ", self.data.clone()),
            (
                if release == DosRelease::Breakers {
                    "RUN.EXE"
                } else {
                    "GO.EXE"
                },
                self.interpreter.clone(),
            ),
            ("SIDE.TXT", self.side_map.clone()),
            ("TITLE.EXE", self.title.clone()),
        ])
    }

    fn load(&self, release: DosRelease) -> Result<Loaded, String> {
        btz::player::load(release, &self.inputs(release))
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
    for (release, resources, label, expected_byte, expected_form6) in [
        (
            DosRelease::Mindwheel,
            MINDWHEEL_RESOURCES,
            *b"MW",
            0x1234,
            0x55aa,
        ),
        (DosRelease::Essex, ESSEX_RESOURCES, *b"EX", 0x34, 0x55aa),
        (
            DosRelease::Brimstone,
            BRIMSTONE_RESOURCES,
            *b"BR",
            0x34,
            0x55aa,
        ),
        (DosRelease::Breakers, BREAKERS_RESOURCES, *b"BK", 0x34, 0),
    ] {
        let fixture = Fixture::new(resources, &label);
        let original_story = fs::read(&fixture.data).unwrap();
        let mut loaded = fixture.load(release).unwrap();
        assert_eq!(
            loaded.presentation.is_some(),
            release == DosRelease::Breakers
        );
        assert!(
            loaded
                .session
                .screen()
                .unicode()
                .contains(std::str::from_utf8(&label).unwrap()),
            "wrong title compression table for {release:?}"
        );
        let mut waiting = false;
        for _ in 0..1000 {
            if loaded.session.advance().unwrap() == SessionStep::WaitKey {
                waiting = true;
                break;
            }
        }
        assert!(waiting, "did not reach the authored wait in {release:?}");
        assert_eq!(
            loaded.session.vm().variable(VarAddr(0x192)).unwrap(),
            expected_byte,
            "byte operation definition for {release:?}"
        );
        assert_eq!(
            loaded.session.vm().variable(VarAddr(0x194)).unwrap(),
            expected_form6,
            "operand form definition for {release:?}"
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
            "wrong key-wait prompt offset for {release:?}"
        );
        assert_eq!(fs::read(&fixture.data).unwrap(), original_story);
    }
}

#[test]
fn detected_game_controls_title_requirements_instead_of_executable_names() {
    let fixture = Fixture::new(BREAKERS_RESOURCES, b"BK");
    // A title file exists, but the app did not select it. Do not discover it.
    fs::copy(&fixture.title, fixture.directory.join("TITLE.EXE")).unwrap();
    let incomplete = reforge_inputs::ResolvedFiles::new([
        ("ADISK.OBJ", fixture.data.clone()),
        ("RUN.EXE", fixture.interpreter.clone()),
        ("SIDE.TXT", fixture.side_map.clone()),
    ]);
    let missing = btz::player::load(DosRelease::Breakers, &incomplete)
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
fn player_does_not_discover_resources_missing_from_the_resolved_input_set() {
    let fixture = Fixture::new(MINDWHEEL_RESOURCES, b"MW");
    // Conventional resources beside the supplied launch path are not authority.
    fs::copy(&fixture.data, fixture.directory.join("ADISK.OBJ")).unwrap();
    let error = btz::cli::run(
        [
            OsString::from("--original"),
            fixture.directory.as_os_str().to_owned(),
        ]
        .into_iter(),
        &reforge_inputs::ResolvedFiles::default(),
        DosRelease::Mindwheel,
        "Mindwheel",
    )
    .unwrap_err();
    assert!(error.contains("ADISK.OBJ"), "{error}");
}

#[test]
fn explicit_side_map_override_preserves_other_selected_resources() {
    let fixture = Fixture::new(MINDWHEEL_RESOURCES, b"MW");
    let override_path = fixture.directory.join("alternate map");
    fs::copy(&fixture.side_map, &override_path).unwrap();
    fs::write(&fixture.side_map, b"invalid map").unwrap();
    assert!(fixture.load(DosRelease::Mindwheel).is_err());
    let loaded = btz::player::load_with_side_map(
        DosRelease::Mindwheel,
        &fixture.inputs(DosRelease::Mindwheel),
        Some(&override_path),
    )
    .unwrap();
    assert_eq!(loaded.files.side_map_path, Some(override_path));
    assert!(loaded.files.originals.contains(&fixture.data));
    assert!(loaded.files.originals.contains(&fixture.interpreter));
}
