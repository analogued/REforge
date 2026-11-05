// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const ALIASES: &[ReleaseAlias] = &[ReleaseAlias {
    game_id: "one",
    old_release_id: "r1",
    new_release_id: "r2",
}];

fn setup(temp: &Temp) -> Library {
    let library = temp.library().with_release_aliases(ALIASES).with_storage(
        temp.0.join("config/config.toml"),
        Some(temp.0.join("saves")),
    );
    fs::create_dir_all(library.path.parent().unwrap()).unwrap();
    fs::write(&library.path, format!("# library note\nschema_version = 2\n[[entries]]\nentry_id = 'one-aaaaaa'\ngame_id = 'one'\nrelease_id = 'r1' # release note\ntitle = 'My title'\noriginal = '{}'\ndirectory = '{}'\ncustom = 'keep'\n",temp.0.join("missing").display(),temp.0.join("missing").display())).unwrap();
    fs::write(&library.config, "schema_version = 2\n[launcher]\nselected_entry = 'one-aaaaaa'\n[entries.one-aaaaaa.audio]\nmaster_volume = 40\n").unwrap();
    library
}

#[test]
fn release_aliases_migrate_without_originals_and_preserve_profiles_and_comments() {
    let temp = Temp::new();
    let mut library = setup(&temp);
    let preferences = fs::read(&library.config).unwrap();
    let saves = library.saves.as_ref().unwrap().join("one-aaaaaa");
    fs::create_dir_all(&saves).unwrap();
    fs::write(saves.join("slot"), b"saved bytes").unwrap();
    library.reload().unwrap();
    assert_eq!(library.entries[0].entry_id, "one-aaaaaa");
    assert_eq!(library.entries[0].release_id, "r2");
    assert_eq!(library.entries[0].title, "My title");
    assert!(!library.entries[0].original.exists());
    let migrated = fs::read_to_string(&library.path).unwrap();
    for retained in [
        "# library note",
        "# release note",
        "custom = 'keep'",
        "title = 'My title'",
    ] {
        assert!(migrated.contains(retained), "{retained}");
    }
    assert_eq!(fs::read(&library.config).unwrap(), preferences);
    assert_eq!(fs::read(saves.join("slot")).unwrap(), b"saved bytes");
    library.reload().unwrap();
    assert_eq!(fs::read_to_string(&library.path).unwrap(), migrated);
}

#[test]
fn busy_migration_uses_aliases_in_memory_then_retries_without_losing_edits() {
    let temp = Temp::new();
    let mut library = setup(&temp);
    let before = fs::read(&library.path).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(library.path.with_extension("lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    library.reload().unwrap();
    assert_eq!(library.entries[0].release_id, "r2");
    assert!(
        library
            .warning
            .as_ref()
            .unwrap()
            .contains("Cannot save updated release IDs")
    );
    assert_eq!(fs::read(&library.path).unwrap(), before);
    drop(lock);
    let edited = String::from_utf8(before)
        .unwrap()
        .replace("My title", "External title");
    fs::write(&library.path, edited).unwrap();
    library.reload().unwrap();
    assert_eq!(library.entries[0].title, "External title");
    assert_eq!(library.entries[0].release_id, "r2");
    assert!(library.warning.is_none());
}

#[test]
fn aliases_do_not_guess_unknown_releases_or_cross_game_boundaries() {
    for (game, release) in [("other", "r1"), ("one", "unknown")] {
        let temp = Temp::new();
        let mut library = setup(&temp);
        let original = fs::read_to_string(&library.path)
            .unwrap()
            .replace("game_id = 'one'", &format!("game_id = '{game}'"))
            .replace("release_id = 'r1'", &format!("release_id = '{release}'"));
        fs::write(&library.path, &original).unwrap();
        library.reload().unwrap();
        assert_eq!(library.entries[0].release_id, release);
        assert_eq!(fs::read_to_string(&library.path).unwrap(), original);
    }
}

#[test]
fn mutations_translate_latest_records_and_duplicate_add_keeps_the_entry_id() {
    let temp = Temp::new();
    let mut library = setup(&temp);
    let original = temp.0.join("missing");
    assert_eq!(
        library
            .add_at("one", "r2", "One", &original, &original)
            .unwrap(),
        "one-aaaaaa"
    );
    let second = library
        .add("one", "r2", "Two", &temp.0.join("second"))
        .unwrap();
    let text = fs::read_to_string(&library.path)
        .unwrap()
        .replace("release_id = \"r2\"", "release_id = 'r1'");
    fs::write(&library.path, text).unwrap();
    library.remove("one-aaaaaa").unwrap();
    assert_eq!(library.entries[0].entry_id, second);
    assert_eq!(library.entries[0].release_id, "r2");
    let text = fs::read_to_string(&library.path)
        .unwrap()
        .replace("release_id = \"r2\"", "release_id = 'r1'");
    fs::write(&library.path, text).unwrap();
    library
        .relocate(&second, "r2", &temp.0.join("third"))
        .unwrap();
    assert_eq!(library.entries[0].release_id, "r2");
}

#[test]
fn malformed_libraries_are_preserved_before_any_alias_write() {
    let temp = Temp::new();
    let mut library = setup(&temp);
    fs::write(&library.path, "entries = [broken").unwrap();
    assert!(library.reload().is_err());
    assert_eq!(
        fs::read_to_string(&library.path).unwrap(),
        "entries = [broken"
    );
}

#[test]
fn release_aliases_compose_with_schema_one_and_preference_migration() {
    let temp = Temp::new();
    let mut library = setup(&temp);
    fs::write(&library.path,format!("schema_version = 1\n[[games]]\nid = 1\ngame_id = 'one'\nrelease_id = 'r1'\ntitle = 'Old copy'\noriginal = '{}'\n",temp.0.join("missing").display())).unwrap();
    fs::write(
        &library.config,
        "schema_version = 1\n[games.one.audio]\nmaster_volume = 40\n",
    )
    .unwrap();
    library.reload().unwrap();
    let entry = &library.entries[0];
    assert_eq!(entry.release_id, "r2");
    assert!(entry.entry_id.starts_with("one-"));
    assert!(!entry.entry_id.starts_with("legacy-"));
    let preferences = crate::Session::load(crate::Launch {
        game_id: "one".into(),
        entry_id: Some(entry.entry_id.clone()),
        path: Some(library.config.clone()),
        cli: Default::default(),
    });
    assert_eq!(preferences.effective(&preferences.saved).audio.volume, 40);
}
