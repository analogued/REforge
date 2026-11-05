// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Installed copies have stable identities, independent of paths and releases.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub entry_id: String,
    pub game_id: String,
    pub release_id: String,
    pub title: String,
    pub original: PathBuf,
    /// Verified game root; an explicit original file may be in a subdirectory.
    pub directory: PathBuf,
}

/// Catalog-owned aliases map each retired release directly to its current ID.
#[derive(Clone, Copy, Debug)]
pub struct ReleaseAlias {
    pub game_id: &'static str,
    pub old_release_id: &'static str,
    pub new_release_id: &'static str,
}

pub struct Library {
    path: PathBuf,
    config: PathBuf,
    saves: Option<PathBuf>,
    release_aliases: &'static [ReleaseAlias],
    pub entries: Vec<Entry>,
    pub warning: Option<String>,
}
impl Library {
    pub fn new(path: PathBuf) -> Self {
        let config = path.with_file_name("config.toml");
        Self {
            path,
            config,
            saves: default_saves_path(),
            release_aliases: &[],
            entries: Vec::new(),
            warning: None,
        }
    }
    pub fn with_storage(mut self, config: PathBuf, saves: Option<PathBuf>) -> Self {
        self.config = config;
        self.saves = saves;
        self
    }
    pub fn with_release_aliases(mut self, aliases: &'static [ReleaseAlias]) -> Self {
        self.release_aliases = aliases;
        self
    }
    fn translate_releases(&self, entries: &mut [Entry]) -> bool {
        let mut changed = false;
        for entry in entries {
            if let Some(alias) = self.release_aliases.iter().find(|alias| {
                alias.game_id == entry.game_id
                    && alias.old_release_id == entry.release_id
                    && alias.old_release_id != alias.new_release_id
            }) {
                entry.release_id = alias.new_release_id.into();
                changed = true;
            }
        }
        changed
    }
    pub fn reload(&mut self) -> Result<(), String> {
        let doc = super::read_document(&self.path, "library")?;
        let mut updated = entries(&doc)?;
        let mut warning = None;
        if legacy(&doc) {
            updated = super::edit_document(&self.path, "library", |doc| self.migrate(doc))?;
        } else if self.translate_releases(&mut updated) {
            match super::edit_document(&self.path, "library", |doc| self.migrate(doc)) {
                Ok(latest) => updated = latest,
                // Known aliases remain usable when a read-only/busy library
                // cannot be updated. A later read retries the atomic migration.
                Err(error) => warning = Some(format!("Cannot save updated release IDs: {error}")),
            }
        }
        // Library identities are committed first. If copying old preferences fails,
        // retrying uses the same IDs and never generates a second profile.
        self.warning = match (warning, migrate_preferences(&self.config, &updated).err()) {
            (Some(a), Some(b)) => Some(format!("{a}\n{b}")),
            (a, b) => a.or(b),
        };
        self.entries = updated;
        Ok(())
    }
    fn migrate(&self, doc: &mut DocumentMut) -> Result<Vec<Entry>, String> {
        let mut latest = entries(doc)?;
        let old_schema = legacy(doc);
        if old_schema {
            for index in 0..latest.len() {
                let old = latest[index].entry_id.clone();
                let id = self.generate_id(&latest[index].game_id, &latest)?;
                latest[index].entry_id = id.clone();
                let tables = doc["games"].as_array_of_tables_mut().expect("validated");
                for table in tables.iter_mut() {
                    if table
                        .get("id")
                        .and_then(Item::as_integer)
                        .map(|n| n.to_string())
                        .as_deref()
                        == Some(old.strip_prefix("legacy-").unwrap_or(&old))
                    {
                        table.remove("id");
                        table["entry_id"] = value(&id);
                    }
                }
            }
            let games = doc
                .remove("games")
                .unwrap_or(Item::ArrayOfTables(ArrayOfTables::new()));
            doc["entries"] = games;
            doc.remove("next_id");
        }
        let translated = self.translate_releases(&mut latest);
        if old_schema {
            store(doc, &latest)?;
        } else if translated {
            for (table, entry) in doc["entries"]
                .as_array_of_tables_mut()
                .expect("validated")
                .iter_mut()
                .zip(&latest)
            {
                if table["release_id"].as_str() != Some(entry.release_id.as_str()) {
                    let decor = table["release_id"]
                        .as_value()
                        .expect("validated")
                        .decor()
                        .clone();
                    table["release_id"] = value(&entry.release_id);
                    *table["release_id"]
                        .as_value_mut()
                        .expect("validated")
                        .decor_mut() = decor;
                }
            }
        }
        Ok(latest)
    }
    fn generate_id(&self, game: &str, entries: &[Entry]) -> Result<String, String> {
        self.generate_id_with(game, entries, |bytes| {
            getrandom::fill(bytes).map_err(|e| format!("Cannot generate library entry ID: {e}"))
        })
    }
    fn generate_id_with(
        &self,
        game: &str,
        entries: &[Entry],
        random: impl FnMut(&mut [u8]) -> Result<(), String>,
    ) -> Result<String, String> {
        let config = super::read_document(&self.config, "settings")?;
        if config.get("entries").is_some_and(|v| !v.is_table_like()) {
            return Err("Invalid entries section; settings file preserved".into());
        }
        let mut saved_ids = HashSet::new();
        if let Some(root) = &self.saves {
            match std::fs::read_dir(root) {
                Ok(items) => {
                    for item in items {
                        saved_ids.insert(
                            item.map_err(|e| format!("Cannot inspect retained saves: {e}"))?
                                .file_name(),
                        );
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Cannot inspect retained saves: {e}")),
            }
        }
        generate_id(
            game,
            |id| {
                entries.iter().any(|e| e.entry_id == id)
                    || config.get("entries").and_then(|v| v.get(id)).is_some()
                    || saved_ids.contains(std::ffi::OsStr::new(id))
            },
            random,
        )
    }
    /// Read the latest file under its lock so simultaneous launcher edits merge.
    pub fn add(
        &mut self,
        game: &str,
        release: &str,
        title: &str,
        original: &Path,
    ) -> Result<String, String> {
        self.add_at(game, release, title, original, &directory(original)?)
    }
    /// Add an original with a separately verified game root. File anchors may
    /// live in a catalog subdirectory while preferences belong to the root.
    pub fn add_at(
        &mut self,
        game: &str,
        release: &str,
        title: &str,
        original: &Path,
        root: &Path,
    ) -> Result<String, String> {
        self.reload()?;
        let (id, updated) = super::edit_document(&self.path, "library", |doc| {
            let mut latest = self.migrate(doc)?;
            let location = directory(root)?;
            if let Some(existing) = latest
                .iter()
                .find(|e| normalized_directory(&e.directory) == location)
            {
                if existing.game_id != game || existing.release_id != release {
                    return Err(
                        "This directory already belongs to a different library entry".into(),
                    );
                }
                return Ok((existing.entry_id.clone(), latest));
            }
            let id = self.generate_id(game, &latest)?;
            let entry = Entry {
                entry_id: id.clone(),
                game_id: game.into(),
                release_id: release.into(),
                title: title.into(),
                directory: location,
                original: original.into(),
            };
            validate(&entry)?;
            latest.push(entry);
            store(doc, &latest)?;
            Ok((id, latest))
        })?;
        self.entries = updated;
        Ok(id)
    }
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let updated = super::edit_document(&self.path, "library", |doc| {
            let mut latest = self.migrate(doc)?;
            latest.retain(|e| e.entry_id != id);
            // Remove preferences first. Failure preserves the library entry; an
            // interrupted removal can leave an entry with defaults, never reused saves.
            super::launcher::remove_entry(&self.config, id)?;
            store(doc, &latest)?;
            Ok(latest)
        })?;
        self.entries = updated;
        Ok(())
    }
    /// Caller verifies the replacement release's compatibility before this edit.
    pub fn relocate(&mut self, id: &str, release: &str, original: &Path) -> Result<(), String> {
        self.relocate_at(id, release, original, &directory(original)?)
    }
    /// Relocate an entry using the replacement's verified root and file anchor.
    /// Caller verifies release compatibility before this edit.
    pub fn relocate_at(
        &mut self,
        id: &str,
        release: &str,
        original: &Path,
        root: &Path,
    ) -> Result<(), String> {
        let updated = super::edit_document(&self.path, "library", |doc| {
            let mut latest = self.migrate(doc)?;
            let location = directory(root)?;
            if latest
                .iter()
                .any(|e| e.entry_id != id && normalized_directory(&e.directory) == location)
            {
                return Err("That directory is already in the library".into());
            }
            let entry = latest
                .iter_mut()
                .find(|e| e.entry_id == id)
                .ok_or("Library entry was removed; add the game again")?;
            entry.original = original.into();
            entry.directory = location;
            entry.release_id = release.into();
            validate(entry)?;
            store(doc, &latest)?;
            Ok(latest)
        })?;
        self.entries = updated;
        Ok(())
    }
    pub fn find_directory(&self, original: &Path) -> Result<Option<&Entry>, String> {
        let location = directory(original)?;
        Ok(self
            .entries
            .iter()
            .find(|e| normalized_directory(&e.directory) == location))
    }
}

/// Resolve existing symlink/relative aliases. Missing locations retain their
/// absolute spelling so old entries remain relocatable without the original disk.
pub fn directory(original: &Path) -> Result<PathBuf, String> {
    let location = if original.is_file() {
        original.parent().ok_or("Original file has no directory")?
    } else {
        original
    };
    if !location.is_absolute() {
        return std::fs::canonicalize(location)
            .map_err(|e| format!("Cannot resolve game directory: {e}"));
    }
    Ok(normalized_directory(location))
}

fn normalized_directory(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        let mut result = PathBuf::new();
        for part in path.components() {
            match part {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    result.pop();
                }
                other => result.push(other.as_os_str()),
            }
        }
        result
    })
}

fn generate_id(
    game: &str,
    mut reserved: impl FnMut(&str) -> bool,
    mut random: impl FnMut(&mut [u8]) -> Result<(), String>,
) -> Result<String, String> {
    if !safe_id(game) {
        return Err("Invalid game identifier".into());
    }
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for _ in 0..1024 {
        let mut suffix = String::new();
        for _ in 0..64 {
            let mut bytes = [0; 16];
            random(&mut bytes)?;
            for byte in bytes {
                // Rejection sampling avoids modulo bias (252 is divisible by 36).
                if byte < 252 {
                    suffix.push(ALPHABET[usize::from(byte) % 36] as char);
                }
                if suffix.len() == 6 {
                    break;
                }
            }
            if suffix.len() == 6 {
                break;
            }
        }
        if suffix.len() != 6 {
            return Err("Random source did not provide usable entry ID bytes".into());
        }
        let id = format!("{game}-{suffix}");
        if !reserved(&id) {
            return Ok(id);
        }
    }
    Err("Cannot allocate a unique library entry ID".into())
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn default_path() -> Option<PathBuf> {
    super::default_path().map(|p| p.with_file_name("library.toml"))
}
/// Future save storage is per entry, never per detected release or location.
pub fn default_saves_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("REforge/saves"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/REforge/saves"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("reforge/saves"))
    }
}
/// Validate manually edited/imported identifiers before using one on disk.
pub fn save_directory(root: &Path, entry_id: &str) -> Result<PathBuf, String> {
    if !safe_id(entry_id) {
        return Err("Invalid library entry ID".into());
    }
    Ok(root.join(entry_id))
}
fn migrate_preferences(path: &Path, entries: &[Entry]) -> Result<(), String> {
    let doc = super::read_document(path, "settings")?;
    if !entries
        .iter()
        .any(|e| doc.get("games").and_then(|g| g.get(&e.game_id)).is_some())
    {
        return Ok(());
    }
    super::edit_document(path, "settings", |doc| {
        let mut moved = HashSet::new();
        for entry in entries {
            if let Some(old) = doc
                .get("games")
                .and_then(|g| g.get(&entry.game_id))
                .cloned()
            {
                let target = super::launcher::table_mut(doc, &["entries"])?;
                if let Some(existing) = target
                    .as_table_like_mut()
                    .expect("validated table")
                    .get_mut(&entry.entry_id)
                {
                    merge_missing(existing, &old)?;
                } else {
                    target[&entry.entry_id] = old;
                }
                moved.insert(entry.game_id.clone());
            }
        }
        if let Some(table) = doc.get_mut("games").and_then(Item::as_table_like_mut) {
            for game in moved {
                table.remove(&game);
            }
        }
        doc["schema_version"] = value(2);
        Ok(())
    })
}
// A retry may encounter keys written after the first library migration.
// Preserve those edits while retaining every untouched legacy field.
fn merge_missing(target: &mut Item, source: &Item) -> Result<(), String> {
    let table = target
        .as_table_like_mut()
        .ok_or("Invalid entry preferences; file preserved")?;
    let source = source
        .as_table_like()
        .ok_or("Invalid legacy game preferences; file preserved")?;
    for (key, value) in source.iter() {
        if let Some(current) = table.get_mut(key) {
            if value.is_table_like() {
                merge_missing(current, value)?;
            }
        } else {
            table.insert(key, value.clone());
        }
    }
    Ok(())
}
fn string(table: &Table, key: &str) -> Result<String, String> {
    table
        .get(key)
        .and_then(Item::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .ok_or_else(|| format!("Invalid library entry {key}; file preserved"))
}
fn validate(entry: &Entry) -> Result<(), String> {
    if !safe_id(&entry.entry_id)
        || !safe_id(&entry.game_id)
        || entry.release_id.trim().is_empty()
        || entry.title.trim().is_empty()
    {
        return Err("Invalid library entry; file preserved".into());
    }
    if !entry.directory.is_absolute()
        || !entry.original.is_absolute()
        || entry.original.to_str().is_none()
    {
        return Err("Library locations must be absolute UTF-8 paths; file preserved".into());
    }
    Ok(())
}
fn legacy(doc: &DocumentMut) -> bool {
    doc.get("schema_version")
        .and_then(Item::as_integer)
        .unwrap_or(1)
        == 1
        && doc.get("games").is_some()
}
fn entries(doc: &DocumentMut) -> Result<Vec<Entry>, String> {
    let mut result = Vec::new();
    let old = legacy(doc);
    if let Some(item) = doc.get(if old { "games" } else { "entries" }) {
        let tables = item
            .as_array_of_tables()
            .ok_or("Library entries must be an array of tables; file preserved")?;
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for table in tables {
            let id = if old {
                format!(
                    "legacy-{}",
                    table
                        .get("id")
                        .and_then(Item::as_integer)
                        .filter(|n| *n > 0)
                        .ok_or("Invalid library entry ID; file preserved")?
                )
            } else {
                string(table, "entry_id")?
            };
            let original: PathBuf = string(table, "original")?.into();
            let location = if old {
                // Schema 1's single-file originals were executables; keep their
                // parent as identity even when the executable is now missing.
                if !original.exists()
                    && original
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                {
                    original.parent().ok_or("Invalid original path")?.into()
                } else {
                    directory(&original)?
                }
            } else {
                string(table, "directory")?.into()
            };
            let entry = Entry {
                directory: location,
                entry_id: id,
                game_id: string(table, "game_id")?,
                release_id: string(table, "release_id")?,
                title: string(table, "title")?,
                original,
            };
            validate(&entry)?;
            if !ids.insert(entry.entry_id.clone())
                || !paths.insert(normalized_directory(&entry.directory))
            {
                return Err("Duplicate library entry ID or directory; file preserved".into());
            }
            result.push(entry);
        }
    }
    Ok(result)
}
fn store(doc: &mut DocumentMut, entries: &[Entry]) -> Result<(), String> {
    doc["schema_version"] = value(2);
    let mut previous: HashMap<_, _> = doc
        .get("entries")
        .and_then(Item::as_array_of_tables)
        .into_iter()
        .flatten()
        .filter_map(|table| {
            table
                .get("entry_id")
                .and_then(Item::as_str)
                .map(|id| (id.to_owned(), table.clone()))
        })
        .collect();
    let mut tables = ArrayOfTables::new();
    for entry in entries {
        validate(entry)?;
        let mut table = previous.remove(&entry.entry_id).unwrap_or_default();
        table["entry_id"] = value(&entry.entry_id);
        table["game_id"] = value(&entry.game_id);
        table["release_id"] = value(&entry.release_id);
        table["title"] = value(&entry.title);
        table["directory"] = value(
            entry
                .directory
                .to_str()
                .ok_or("Library directory is not UTF-8")?,
        );
        table["original"] = value(
            entry
                .original
                .to_str()
                .ok_or("Library location is not UTF-8")?,
        );
        tables.push(table);
    }
    doc["entries"] = Item::ArrayOfTables(tables);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    mod migrations;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "reforge-library-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn library(&self) -> Library {
            Library::new(self.0.join("config/library.toml"))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn library_persists_merges_deduplicates_relocates_and_never_deletes_originals() {
        let t = Temp::new();
        let original = t.0.join("game");
        fs::create_dir(&original).unwrap();
        fs::write(original.join("keep.txt"), b"original").unwrap();
        let mut first = t.library();
        first.reload().unwrap();
        assert!(first.entries.is_empty());
        assert!(!first.path.exists());
        let mut second = t.library();
        second.reload().unwrap();
        let id = first.add("one", "r1", "One", &original).unwrap();
        assert_eq!(first.add("one", "r1", "One", &original).unwrap(), id);
        let other = second.add("two", "r2", "Two", &t.0.join("other")).unwrap();
        first.reload().unwrap();
        assert_eq!(first.entries.len(), 2);
        first.relocate(&id, "r1", &t.0.join("moved")).unwrap();
        second.reload().unwrap();
        assert_eq!(second.entries[0].original, t.0.join("moved"));
        second.remove(&other).unwrap();
        first.remove(&id).unwrap();
        assert_eq!(fs::read(original.join("keep.txt")).unwrap(), b"original");
        first.reload().unwrap();
        assert!(first.entries.is_empty());
        assert!(first.add("one", "r1", "One", &original).unwrap() != other);
    }
    #[test]
    fn invalid_or_future_library_is_preserved_and_failed_edits_do_not_change_memory() {
        let t = Temp::new();
        let mut l = t.library();
        let id = l.add("one", "r1", "One", &t.0.join("one")).unwrap();
        let memory = l.entries.clone();
        for bad in ["not toml", "schema_version = 99", "entries = 'bad'"] {
            fs::write(&l.path, bad).unwrap();
            assert!(l.reload().is_err());
            assert_eq!(l.entries, memory);
            assert!(l.remove(&id).is_err());
            assert_eq!(fs::read_to_string(&l.path).unwrap(), bad);
            assert!(l.add("two", "r2", "Two", &t.0.join("two")).is_err());
            assert_eq!(fs::read_to_string(&l.path).unwrap(), bad);
        }
    }
    #[test]
    fn contention_preserves_memory_and_unknown_metadata_survives_edits() {
        use std::fs::OpenOptions;
        let t = Temp::new();
        let mut l = t.library();
        let one = l.add("one", "r1", "One", &t.0.join("one")).unwrap();
        let mut doc = super::super::read_document(&l.path, "library").unwrap();
        doc["custom"] = value("keep root");
        doc["entries"]
            .as_array_of_tables_mut()
            .unwrap()
            .get_mut(0)
            .unwrap()["custom"] = value("keep entry");
        fs::write(&l.path, doc.to_string()).unwrap();
        let before = fs::read(&l.path).unwrap();
        let memory = l.entries.clone();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(l.path.with_extension("lock"))
            .unwrap();
        fs2::FileExt::lock_exclusive(&lock).unwrap();
        assert!(l.remove(&one).is_err());
        assert_eq!(l.entries, memory);
        assert_eq!(fs::read(&l.path).unwrap(), before);
        drop(lock);
        l.relocate(&one, "r1", &t.0.join("moved")).unwrap();
        l.add("two", "r2", "Two", &t.0.join("two")).unwrap();
        let doc = super::super::read_document(&l.path, "library").unwrap();
        assert_eq!(doc["custom"].as_str(), Some("keep root"));
        assert_eq!(
            doc["entries"].as_array_of_tables().unwrap().get(0).unwrap()["custom"].as_str(),
            Some("keep entry")
        );
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use std::fs;

    #[test]
    fn random_ids_retry_reserved_values_and_propagate_rng_failure() {
        let mut count = 0;
        let id = generate_id(
            "alley-cat",
            |id| id == "alley-cat-aaaaaa",
            |bytes| {
                bytes.fill(count);
                count += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(id, "alley-cat-bbbbbb");
        assert!(
            generate_id(
                "alley-cat",
                |_| false,
                |_| Err("entropy unavailable".into())
            )
            .unwrap_err()
            .contains("entropy unavailable")
        );
        assert!(generate_id("../bad", |_| false, |_| Ok(())).is_err());
        assert!(save_directory(Path::new("/saves"), "../bad").is_err());
    }

    #[test]
    fn orphaned_preferences_and_save_directories_reserve_ids() {
        let root =
            std::env::temp_dir().join(format!("reforge-id-reservations-{}", std::process::id()));
        fs::create_dir_all(root.join("saves/alley-cat-bbbbbb")).unwrap();
        let config = root.join("config.toml");
        fs::write(&config, "[entries.alley-cat-aaaaaa.display]\nscale = 2\n").unwrap();
        let library = Library::new(root.join("library.toml"))
            .with_storage(config.clone(), Some(root.join("saves")));
        let active = Entry {
            entry_id: "alley-cat-cccccc".into(),
            game_id: "alley-cat".into(),
            release_id: "r1".into(),
            title: "Alley Cat".into(),
            original: root.join("game"),
            directory: root.join("game"),
        };
        let mut next = 0;
        let id = library
            .generate_id_with("alley-cat", &[active], |bytes| {
                bytes.fill(next);
                next += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(id, "alley-cat-dddddd");
        assert_eq!(next, 4);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_migration_is_restartable_and_copies_pins_to_each_existing_copy() {
        let root =
            std::env::temp_dir().join(format!("reforge-entry-migration-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("library.toml");
        let config = root.join("config.toml");
        fs::write(&path, format!("# library note\nschema_version = 1\nnext_id = 3\n[[games]]\nid = 1\ngame_id = 'alley-cat'\nrelease_id = 'r1'\ntitle = 'One'\noriginal = '{}'\ncustom = 'keep'\n[[games]]\nid = 2\ngame_id = 'alley-cat'\nrelease_id = 'r1'\ntitle = 'Two'\noriginal = '{}'\n", root.join("one/CAT.EXE").display(), root.join("two/CAT.EXE").display())).unwrap();
        fs::write(&config, "# preferences note\n[global.audio]\nmaster_volume = 65\n[games.alley-cat.display]\nwindow_mode = 'windowed' # explicit equal pin\n[games.alley-cat.launcher]\nmachine = 'pcjr'\n[games.alley-cat.future]\nvalue = 42\n").unwrap();
        let mut library =
            Library::new(path.clone()).with_storage(config.clone(), Some(root.join("saves")));
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.with_extension("lock"))
            .unwrap();
        fs2::FileExt::lock_exclusive(&lock).unwrap();
        library.reload().unwrap();
        assert!(library.warning.as_ref().unwrap().contains("busy"));
        let committed_ids = library.entries.clone();
        drop(lock);
        library.reload().unwrap();
        assert_eq!(library.entries, committed_ids);
        assert!(library.warning.is_none(), "{:?}", library.warning);
        assert_eq!(library.entries.len(), 2);
        assert_ne!(library.entries[0].entry_id, library.entries[1].entry_id);
        for entry in &library.entries {
            assert!(entry.entry_id.starts_with("alley-cat-"));
            assert_eq!(entry.entry_id.len(), "alley-cat-".len() + 6);
            assert_eq!(entry.directory, entry.original.parent().unwrap());
            assert_eq!(
                super::super::launcher::native_options(&config, &entry.entry_id).unwrap()["machine"],
                "pcjr"
            );
            let session = super::super::Session::load(super::super::Launch {
                game_id: entry.game_id.clone(),
                entry_id: Some(entry.entry_id.clone()),
                path: Some(config.clone()),
                cli: Default::default(),
            });
            assert_eq!(session.saved.game["window_mode"], "windowed");
        }
        let first = fs::read_to_string(&path).unwrap();
        assert!(first.contains("# library note") && first.contains("custom = 'keep'"));
        assert!(!first.contains("next_id"));
        let preferences = fs::read_to_string(&config).unwrap();
        assert!(
            preferences.contains("# preferences note")
                && preferences.contains("# explicit equal pin")
                && preferences.contains("value = 42")
        );
        let snapshot = library.entries.clone();
        library.reload().unwrap();
        assert_eq!(snapshot, library.entries);
        assert_eq!(first, fs::read_to_string(&path).unwrap());
        assert_eq!(preferences, fs::read_to_string(&config).unwrap());
        // Simulate a crash after the library commit, before old config was copied.
        fs::write(&config, "[games.alley-cat.launcher]\nmachine = 'cga'\n").unwrap();
        library.reload().unwrap();
        assert_eq!(snapshot, library.entries);
        assert_eq!(
            super::super::launcher::native_options(&config, &snapshot[0].entry_id).unwrap()["machine"],
            "cga"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copies_are_independent_and_removal_or_relocation_retains_saves() {
        let root =
            std::env::temp_dir().join(format!("reforge-entry-copies-{}", std::process::id()));
        fs::create_dir_all(root.join("one")).unwrap();
        fs::create_dir_all(root.join("two")).unwrap();
        fs::write(root.join("one/CAT.EXE"), b"fixture").unwrap();
        fs::write(root.join("one/OTHER.COM"), b"fixture").unwrap();
        fs::write(root.join("two/CAT.EXE"), b"fixture").unwrap();
        let config = root.join("config.toml");
        let saves = root.join("saves");
        let mut library = Library::new(root.join("library.toml"))
            .with_storage(config.clone(), Some(saves.clone()));
        let first = library
            .add("alley-cat", "r1", "Alley Cat", &root.join("one/CAT.EXE"))
            .unwrap();
        assert_eq!(
            library
                .add("alley-cat", "r1", "Alley Cat", &root.join("one/OTHER.COM"))
                .unwrap(),
            first
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("one"), root.join("alias")).unwrap();
            assert_eq!(
                library
                    .add("alley-cat", "r1", "Alley Cat", &root.join("alias/CAT.EXE"))
                    .unwrap(),
                first
            );
        }
        let second = library
            .add("alley-cat", "r1", "Alley Cat", &root.join("two/CAT.EXE"))
            .unwrap();
        assert_ne!(first, second);
        super::super::launcher::save_native_option(&config, &first, "machine", Some("pcjr"))
            .unwrap();
        assert!(
            super::super::launcher::native_options(&config, &second)
                .unwrap()
                .is_empty()
        );
        let save_dir = save_directory(&saves, &first).unwrap();
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(save_dir.join("slot"), b"save fixture").unwrap();
        assert!(
            library
                .relocate(&first, "r2", &root.join("two/CAT.EXE"))
                .is_err()
        );
        library
            .relocate(&first, "r2", &root.join("replacement"))
            .unwrap();
        assert_eq!(library.entries[0].entry_id, first);
        assert_eq!(library.entries[0].release_id, "r2");
        assert_eq!(
            super::super::launcher::native_options(&config, &first).unwrap()["machine"],
            "pcjr"
        );
        library.remove(&first).unwrap();
        assert!(
            super::super::launcher::native_options(&config, &first)
                .unwrap()
                .is_empty()
        );
        assert_eq!(fs::read(save_dir.join("slot")).unwrap(), b"save fixture");
        assert!(root.join("one/CAT.EXE").is_file());
        assert_ne!(
            library
                .add("alley-cat", "r1", "Alley Cat", &root.join("one/CAT.EXE"))
                .unwrap(),
            first
        );
        fs::remove_dir_all(root).unwrap();
    }
}
