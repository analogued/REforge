// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

// A test catalog uses real detection with authored bytes, while retaining the
// game's public identity so library/profile compatibility checks are exercised.
static ORIGINALS: &[detection::Release] = &[detection::Release {
    files: &[
        detection::FileRecord {
            name: "GAME.EXE",
            size: 3,

            blake3: "f006b5ee4890b66656cf6c23998e25196a163644665dc9d4b47da1fca3037023",
        },
        detection::FileRecord {
            name: "DATA/ROOMS",
            size: 3,

            blake3: "5abc20c80d2681bed0c7564a5065f692305d47b80e23ab85b296ac8832617084",
        },
        detection::FileRecord {
            name: "AUDIO/MUSIC",
            size: 3,

            blake3: "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        },
    ],
    identity_files: &[],
    ..detection::RELEASES[1]
}];

#[derive(Default)]
struct Counts {
    detections: Vec<PathBuf>,
    hashes: Vec<PathBuf>,
    bytes: usize,
}
#[derive(Clone, Default)]
struct Trace(Rc<RefCell<Counts>>);
impl Trace {
    fn detect(&self, path: &Path) -> Result<Detected, String> {
        self.0.borrow_mut().detections.push(path.to_owned());
        detection::detect_with(
            path,
            ORIGINALS,
            |path| {
                let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
                if !metadata.is_file() {
                    return Err("not a regular file".into());
                }
                Ok(metadata.len())
            },
            |path| {
                let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
                let mut counts = self.0.borrow_mut();
                counts.hashes.push(path.to_owned());
                counts.bytes += bytes.len();
                Ok(blake3::hash(&bytes).to_hex().to_string())
            },
        )
    }
    fn verify(&self, entry: &Entry) -> Result<Detected, String> {
        verify_entry_with(entry, |path| self.detect(path))
    }
    fn reset(&self) {
        *self.0.borrow_mut() = Counts::default();
    }
    fn assert_work(&self, detections: usize, hashes: usize, bytes: usize) {
        let counts = self.0.borrow();
        assert_eq!(
            counts.detections.len(),
            detections,
            "{:?}",
            counts.detections
        );
        assert_eq!(counts.hashes.len(), hashes, "{:?}", counts.hashes);
        assert_eq!(counts.bytes, bytes);
    }
}

fn install(fixture: &Fixture, name: &str) -> PathBuf {
    let root = fixture.directory.join(name);
    std::fs::create_dir_all(root.join("DATA")).unwrap();
    std::fs::create_dir_all(root.join("AUDIO")).unwrap();
    for (name, bytes) in [
        ("program.bin", b"xyz"),
        ("DATA/resources.bin", b"dat"),
        ("AUDIO/voice.bin", b"abc"),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    root
}
fn boot(fixture: &Fixture, trace: &Trace) -> State {
    State::new_with(fixture.library.clone(), fixture.config.clone(), |entry| {
        trace.verify(entry)
    })
}
fn add(state: &mut State, original: &Path, trace: &Trace) {
    state
        .add_with(
            original,
            |path| trace.detect(path),
            |entry| trace.verify(entry),
        )
        .unwrap();
}
fn relocate(state: &mut State, id: &str, original: &Path, trace: &Trace) {
    state
        .relocate_with(
            id.into(),
            original,
            |path| trace.detect(path),
            |entry| trace.verify(entry),
        )
        .unwrap();
}
fn write_records(fixture: &Fixture, records: &[Entry]) {
    let mut text = "schema_version = 2\n".to_owned();
    for entry in records {
        text.push_str(&format!("\n[[entries]]\nentry_id = {:?}\ngame_id = {:?}\nrelease_id = {:?}\ntitle = {:?}\noriginal = {:?}\ndirectory = {:?}\n",
            entry.entry_id, entry.game_id, entry.release_id, entry.title,
            entry.original.to_str().unwrap(), entry.directory.to_str().unwrap()));
    }
    std::fs::write(&fixture.library, text).unwrap();
}

#[test]
fn translating_an_old_release_id_keeps_live_state_without_reverification() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &root, &trace);
    let receipt = state.games[0].preview_verification.clone().unwrap();
    let mut entries = state.library.entries.clone();
    let alias = crate::detection::migration::RELEASES
        .iter()
        .find(|alias| alias.game_id == "mach-3")
        .unwrap();
    entries[0].release_id = alias.old_release_id.into();
    write_records(&fixture, &entries);
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(0, 0, 0);
    assert_eq!(state.games[0].release_id, alias.new_release_id);
    assert!(Arc::ptr_eq(
        state.games[0].preview_verification.as_ref().unwrap(),
        &receipt
    ));
    assert!(
        !std::fs::read_to_string(&fixture.library)
            .unwrap()
            .contains(alias.old_release_id)
    );
}

#[test]
fn startup_checks_each_copy_once_and_add_reuses_its_complete_detected_mapping() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    trace.assert_work(1, 3, 9);
    let id_a = state.selected.clone().unwrap();
    let receipt = state.games[0].preview_verification.clone().unwrap();
    let entry = state.library.entries[0].clone();
    let result = receipt
        .current_for(&entry.original, &entry.release_id)
        .unwrap();
    assert_eq!(result.files["DATA/ROOMS"], a.join("DATA/resources.bin"));
    trace.reset();
    add(&mut state, &b, &trace);
    trace.assert_work(1, 3, 9);
    assert!(Arc::ptr_eq(
        state
            .games
            .iter()
            .find(|game| game.entry_id == id_a)
            .unwrap()
            .preview_verification
            .as_ref()
            .unwrap(),
        &receipt
    ));
    trace.reset();
    let reopened = boot(&fixture, &trace);
    trace.assert_work(2, 6, 18);
    assert!(
        reopened
            .games
            .iter()
            .all(|game| game.status == EntryStatus::Verified)
    );
}

#[test]
fn duplicate_add_reuses_matching_anchors_but_verifies_a_different_retained_anchor() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let anchor = root.join("DATA/resources.bin");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &anchor, &trace);
    let id = state.selected.clone().unwrap();
    let mut records = state.library.entries.clone();
    records[0].title = "My custom title".into();
    write_records(&fixture, &records);
    state.refresh_with(|entry| trace.verify(entry));
    trace.reset();
    add(&mut state, &anchor, &trace);
    trace.assert_work(1, 3, 9);
    assert_eq!(state.games[0].title, "My custom title");
    std::fs::write(root.join("alternate.bin"), b"dat").unwrap();
    std::fs::write(&anchor, b"bad").unwrap();
    trace.reset();
    add(&mut state, &root.join("alternate.bin"), &trace);
    assert_eq!(
        trace.0.borrow().detections,
        [root.join("alternate.bin"), anchor.clone()]
    );
    assert_eq!(state.games.len(), 1);
    assert_eq!(state.selected.as_deref(), Some(id.as_str()));
    assert_eq!(state.library.entries[0].original, anchor);
    assert!(state.games[0].status.is_missing());
}

#[test]
fn relocate_reuses_verification_preserves_the_id_and_does_not_scan_other_copies() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let c = install(&fixture, "c");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    let id = state.selected.clone().unwrap();
    add(&mut state, &b, &trace);
    reforge_settings::launcher::save_native_option(&fixture.config, &id, "video", Some("mono"))
        .unwrap();
    trace.reset();
    let anchor = c.join("DATA/resources.bin");
    relocate(&mut state, &id, &anchor, &trace);
    trace.assert_work(1, 3, 9);
    let game = state.games.iter().find(|game| game.entry_id == id).unwrap();
    assert_eq!(game.original, anchor);
    assert_eq!(game.directory, c);
    assert_eq!(game.option_values()["Video"], "Monochrome");
    assert_eq!(
        game.preview_verification
            .as_ref()
            .unwrap()
            .current_for(&game.original, &game.release_id)
            .unwrap()
            .files["AUDIO/MUSIC"],
        game.directory.join("AUDIO/voice.bin")
    );
}

#[test]
fn remove_settings_and_game_return_do_not_reverify_unchanged_installations() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    let id_a = state.selected.clone().unwrap();
    add(&mut state, &b, &trace);
    let id_b = state.selected.clone().unwrap();
    trace.reset();
    state.request_remove(id_b);
    state.confirm_remove_with(|entry| trace.verify(entry));
    assert_eq!(state.selected.as_deref(), Some(id_a.as_str()));
    reforge_settings::launcher::save_native_option(&fixture.config, &id_a, "video", Some("mono"))
        .unwrap();
    state.refresh_with(|entry| trace.verify(entry)); // Settings close, including Cancel.
    assert_eq!(state.games[0].option_values()["Video"], "Monochrome");
    state.refresh_with(|entry| trace.verify(entry)); // Return from a game.
    trace.assert_work(0, 0, 0);
    assert!(state.games[0].matches_installation(&state.library.entries[0]));
}

#[test]
fn play_always_checks_and_failed_checks_are_applied_without_a_second_scan() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    let id = state.selected.clone().unwrap();
    add(&mut state, &b, &trace);
    trace.reset();
    state
        .prepare_play_with(id.clone(), |entry| trace.verify(entry))
        .unwrap();
    trace.assert_work(1, 3, 9);
    std::fs::write(a.join("program.bin"), b"bad").unwrap();
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(0, 0, 0); // Status means last verification, not immutability.
    assert!(
        state
            .games
            .iter()
            .find(|game| game.entry_id == id)
            .unwrap()
            .status
            == EntryStatus::Verified
    );
    state
        .prepare_play_with(id.clone(), |entry| trace.verify(entry))
        .unwrap_err();
    assert_eq!(trace.0.borrow().detections, [a]);
    assert!(
        state
            .games
            .iter()
            .find(|game| game.entry_id == id)
            .unwrap()
            .status
            .is_missing()
    );
    assert!(
        state
            .games
            .iter()
            .find(|game| game.entry_id != id)
            .unwrap()
            .status
            == EntryStatus::Verified
    );
}

#[test]
fn restoring_originals_and_locating_them_retries_only_incomplete_previews() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &root, &trace);
    let id = state.selected.clone().unwrap();
    let image = reforge_sdl::preview::Frame::new(
        reforge_sdl::DisplayGeometry::new((2, 1), (4, 3)),
        vec![255; 8],
    )
    .unwrap();
    state.games[0].previews.insert(
        "color".into(),
        preview::sources::Source {
            id: "retained".into(),
            frame: Arc::new(image),
        },
    );
    state.games[0].preview_attempted = true;
    std::fs::write(root.join("program.bin"), b"bad").unwrap();
    state
        .prepare_play_with(id.clone(), |entry| trace.verify(entry))
        .unwrap_err();
    std::fs::write(root.join("program.bin"), b"xyz").unwrap();
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(0, 0, 0);
    assert!(state.games[0].status.is_missing());
    relocate(&mut state, &id, &root, &trace);
    trace.assert_work(1, 3, 9);
    assert_eq!(state.games[0].status, EntryStatus::Verified);
    assert_eq!(state.games[0].previews["color"].id, "retained");
    assert!(!state.games[0].preview_attempted);
}

#[test]
fn title_order_and_preference_edits_keep_models_but_new_installations_are_verified() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let c = install(&fixture, "c");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    add(&mut state, &b, &trace);
    let receipt = state.games[0].preview_verification.clone().unwrap();
    let mut records = state.library.entries.clone();
    records.reverse();
    records[1].title = "My renamed copy".into();
    write_records(&fixture, &records);
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(0, 0, 0);
    assert_eq!(state.games[1].title, "My renamed copy");
    assert!(Arc::ptr_eq(
        state.games[1].preview_verification.as_ref().unwrap(),
        &receipt
    ));
    let mut external = Library::new(fixture.library.clone());
    external
        .add_at("mach-3", ORIGINALS[0].id, "External copy", &c, &c)
        .unwrap();
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(1, 3, 9);
    assert_eq!(state.games.len(), 3);
}

#[test]
fn every_installation_identity_change_invalidates_the_previous_verification() {
    for field in ["original", "directory", "game", "release"] {
        let fixture = Fixture::new(0);
        let trace = Trace::default();
        let root = install(&fixture, "copy");
        let other = install(&fixture, "other");
        let mut state = boot(&fixture, &trace);
        add(&mut state, &root, &trace);
        add(&mut state, &other, &trace);
        let mut records = state.library.entries.clone();
        match field {
            "original" => records[0].original = root.join("DATA/resources.bin"),
            "directory" => records[0].directory = fixture.directory.join("wrong-root"),
            "game" => records[0].game_id = "alley-cat".into(),
            _ => records[0].release_id = detection::RELEASES[0].id.into(),
        }
        write_records(&fixture, &records);
        trace.reset();
        state.refresh_with(|entry| trace.verify(entry));
        assert_eq!(trace.0.borrow().detections.len(), 1, "{field}");
        assert!(state.games[0].matches_installation(&records[0]));
        assert_eq!(
            state.games[0].status.is_missing(),
            field != "original",
            "{field}"
        );
    }
}

#[test]
fn malformed_reload_preserves_existing_entries_selection_and_verification_errors() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &root, &trace);
    let id = state.selected.clone();
    let entry = state.library.entries[0].clone();
    state.apply_verification(&entry, &Err("a useful verification error".into()));
    std::fs::write(&fixture.library, "entries = [broken").unwrap();
    trace.reset();
    state.refresh_with(|entry| trace.verify(entry));
    trace.assert_work(0, 0, 0);
    assert_eq!(state.selected, id);
    assert_eq!(state.games.len(), 1);
    assert_eq!(
        state.games[0].status,
        EntryStatus::Missing("a useful verification error".into())
    );
    assert!(state.notice.as_ref().is_some_and(|(_, error)| *error));
    assert_eq!(
        std::fs::read_to_string(&fixture.library).unwrap(),
        "entries = [broken"
    );
}

#[test]
fn reconciling_a_committed_mutation_does_not_reopen_the_library() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let mut state = boot(&fixture, &trace);
    let detected = trace.detect(&root).unwrap();
    let id = state
        .library
        .add_at(
            "mach-3",
            detected.release.id,
            "Committed copy",
            &root,
            &root,
        )
        .unwrap();
    let update = VerificationUpdate::success(id.clone(), root, detected).unwrap();
    std::fs::write(&fixture.library, "malformed external edit after commit").unwrap();
    trace.reset();
    state.reconcile_with(Some(update), |entry| trace.verify(entry));
    trace.assert_work(0, 0, 0);
    assert_eq!(state.games[0].entry_id, id);
    assert_eq!(state.games[0].status, EntryStatus::Verified);
}

#[test]
fn failed_storage_mutations_leave_models_selection_and_records_unchanged() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let a = install(&fixture, "a");
    let b = install(&fixture, "b");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &a, &trace);
    let selected = state.selected.clone();
    let before = state.library.entries.clone();
    std::fs::write(&fixture.library, "entries = [broken").unwrap();
    assert!(
        state
            .add_with(&b, |path| trace.detect(path), |entry| trace.verify(entry))
            .is_err()
    );
    assert!(
        state
            .relocate_with(
                selected.clone().unwrap(),
                &b,
                |path| trace.detect(path),
                |entry| trace.verify(entry)
            )
            .is_err()
    );
    assert_eq!(state.selected, selected);
    assert_eq!(state.library.entries, before);
    assert_eq!(state.games.len(), 1);
    assert!(state.games[0].matches_installation(&before[0]));
    assert_eq!(state.games[0].status, EntryStatus::Verified);
    assert_eq!(
        std::fs::read_to_string(&fixture.library).unwrap(),
        "entries = [broken"
    );
}

#[cfg(unix)]
#[test]
fn committed_verification_preserves_file_and_subdirectory_symlink_layouts() {
    let fixture = Fixture::new(0);
    let trace = Trace::default();
    let root = install(&fixture, "copy");
    let outside = fixture.directory.join("external");
    std::fs::create_dir_all(outside.join("data")).unwrap();
    std::fs::rename(
        root.join("DATA/resources.bin"),
        outside.join("data/resources.bin"),
    )
    .unwrap();
    std::fs::remove_dir(root.join("DATA")).unwrap();
    std::os::unix::fs::symlink(outside.join("data"), root.join("DATA")).unwrap();
    std::fs::rename(root.join("AUDIO/voice.bin"), outside.join("voice.bin")).unwrap();
    std::os::unix::fs::symlink(outside.join("voice.bin"), root.join("AUDIO/voice.bin")).unwrap();
    let anchor = root.join("DATA/resources.bin");
    let mut state = boot(&fixture, &trace);
    add(&mut state, &anchor, &trace);
    trace.assert_work(1, 3, 9);
    assert_eq!(state.library.entries[0].original, anchor);
    assert_eq!(state.library.entries[0].directory, root);
    let verification = state.games[0]
        .preview_verification
        .as_ref()
        .unwrap()
        .current_for(&anchor, ORIGINALS[0].id)
        .unwrap();
    assert_eq!(verification.files["DATA/ROOMS"], anchor);
    assert_eq!(
        verification.files["AUDIO/MUSIC"],
        root.join("AUDIO/voice.bin")
    );
}
