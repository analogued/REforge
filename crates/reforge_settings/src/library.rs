// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Persistent local game locations, separate from preferences and game saves.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub game_id: String,
    pub release_id: String,
    pub title: String,
    pub original: PathBuf,
}

pub struct Library {
    path: PathBuf,
    pub entries: Vec<Entry>,
}
impl Library {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
        }
    }
    pub fn reload(&mut self) -> Result<(), String> {
        self.entries = entries(&super::read_document(&self.path, "library")?)?;
        Ok(())
    }
    /// Read the latest file under its lock so simultaneous launcher edits merge.
    pub fn add(
        &mut self,
        game: &str,
        release: &str,
        title: &str,
        original: &Path,
    ) -> Result<i64, String> {
        let (id, updated) = super::edit_document(&self.path, "library", |doc| {
            let mut latest = entries(doc)?;
            if let Some(existing) = latest.iter().find(|entry| entry.original == original) {
                if existing.game_id != game || existing.release_id != release {
                    return Err("This location already belongs to a different library entry".into());
                }
                return Ok((existing.id, latest));
            }
            let id = next_id(doc, &latest)?;
            let entry = Entry {
                id,
                game_id: game.into(),
                release_id: release.into(),
                title: title.into(),
                original: original.into(),
            };
            validate(&entry)?;
            latest.push(entry);
            doc["next_id"] = value(id.checked_add(1).ok_or("Library entry IDs exhausted")?);
            store(doc, &latest)?;
            Ok((id, latest))
        })?;
        self.entries = updated;
        Ok(id)
    }
    pub fn remove(&mut self, id: i64) -> Result<(), String> {
        let updated = super::edit_document(&self.path, "library", |doc| {
            let mut latest = entries(doc)?;
            // Reserve IDs before removing the highest entry, even in hand-written files.
            doc["next_id"] = value(next_id(doc, &latest)?);
            latest.retain(|entry| entry.id != id);
            store(doc, &latest)?;
            Ok(latest)
        })?;
        self.entries = updated;
        Ok(())
    }
    pub fn relocate(&mut self, id: i64, original: &Path) -> Result<(), String> {
        let updated = super::edit_document(&self.path, "library", |doc| {
            let mut latest = entries(doc)?;
            if latest
                .iter()
                .any(|entry| entry.id != id && entry.original == original)
            {
                return Err("That location is already in the library".into());
            }
            let entry = latest
                .iter_mut()
                .find(|entry| entry.id == id)
                .ok_or("Library entry was removed; add the game again")?;
            entry.original = original.into();
            validate(entry)?;
            store(doc, &latest)?;
            Ok(latest)
        })?;
        self.entries = updated;
        Ok(())
    }
}

pub fn default_path() -> Option<PathBuf> {
    super::default_path().map(|path| path.with_file_name("library.toml"))
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
    if entry.id < 1
        || entry.game_id.trim().is_empty()
        || entry.release_id.trim().is_empty()
        || entry.title.trim().is_empty()
    {
        return Err("Invalid library entry; file preserved".into());
    }
    if !entry.original.is_absolute() || entry.original.to_str().is_none() {
        return Err("Library locations must be absolute UTF-8 paths; file preserved".into());
    }
    Ok(())
}
fn entries(doc: &DocumentMut) -> Result<Vec<Entry>, String> {
    let mut result = Vec::new();
    if let Some(item) = doc.get("games") {
        let tables = item
            .as_array_of_tables()
            .ok_or("Library games must be an array of tables; file preserved")?;
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for table in tables {
            let entry = Entry {
                id: table
                    .get("id")
                    .and_then(Item::as_integer)
                    .ok_or("Invalid library entry ID; file preserved")?,
                game_id: string(table, "game_id")?,
                release_id: string(table, "release_id")?,
                title: string(table, "title")?,
                original: string(table, "original")?.into(),
            };
            validate(&entry)?;
            if !ids.insert(entry.id) || !paths.insert(entry.original.clone()) {
                return Err("Duplicate library entry ID or location; file preserved".into());
            }
            result.push(entry);
        }
    }
    next_id(doc, &result)?;
    Ok(result)
}
fn next_id(doc: &DocumentMut, entries: &[Entry]) -> Result<i64, String> {
    let minimum = entries
        .iter()
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Library entry IDs exhausted")?;
    match doc.get("next_id") {
        None => Ok(minimum),
        Some(item) => item
            .as_integer()
            .filter(|&id| id >= minimum)
            .ok_or_else(|| "Invalid library next_id; file preserved".into()),
    }
}
fn store(doc: &mut DocumentMut, entries: &[Entry]) -> Result<(), String> {
    doc["schema_version"] = value(1);
    let mut previous: HashMap<_, _> = doc
        .get("games")
        .and_then(Item::as_array_of_tables)
        .into_iter()
        .flatten()
        .filter_map(|table| {
            table
                .get("id")
                .and_then(Item::as_integer)
                .map(|id| (id, table.clone()))
        })
        .collect();
    let mut games = ArrayOfTables::new();
    for entry in entries {
        validate(entry)?;
        let mut table = previous.remove(&entry.id).unwrap_or_default();
        table["id"] = value(entry.id);
        table["game_id"] = value(&entry.game_id);
        table["release_id"] = value(&entry.release_id);
        table["title"] = value(&entry.title);
        table["original"] = value(
            entry
                .original
                .to_str()
                .ok_or("Library location is not UTF-8")?,
        );
        games.push(table);
    }
    if games.is_empty() {
        doc.remove("games");
    } else {
        doc["games"] = Item::ArrayOfTables(games);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        first.relocate(id, &t.0.join("moved")).unwrap();
        second.reload().unwrap();
        assert_eq!(second.entries[0].original, t.0.join("moved"));
        second.remove(other).unwrap();
        first.remove(id).unwrap();
        assert_eq!(fs::read(original.join("keep.txt")).unwrap(), b"original");
        first.reload().unwrap();
        assert!(first.entries.is_empty());
        assert!(first.add("one", "r1", "One", &original).unwrap() > other);
    }
    #[test]
    fn invalid_or_future_library_is_preserved_and_failed_edits_do_not_change_memory() {
        let t = Temp::new();
        let mut l = t.library();
        let id = l.add("one", "r1", "One", &t.0.join("one")).unwrap();
        let memory = l.entries.clone();
        for bad in ["not toml", "schema_version = 99", "games = 'bad'"] {
            fs::write(&l.path, bad).unwrap();
            assert!(l.reload().is_err());
            assert_eq!(l.entries, memory);
            assert!(l.remove(id).is_err());
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
        doc["games"]
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
        assert!(l.remove(one).is_err());
        assert_eq!(l.entries, memory);
        assert_eq!(fs::read(&l.path).unwrap(), before);
        drop(lock);
        l.relocate(one, &t.0.join("moved")).unwrap();
        l.add("two", "r2", "Two", &t.0.join("two")).unwrap();
        let doc = super::super::read_document(&l.path, "library").unwrap();
        assert_eq!(doc["custom"].as_str(), Some("keep root"));
        assert_eq!(
            doc["games"].as_array_of_tables().unwrap().get(0).unwrap()["custom"].as_str(),
            Some("keep entry")
        );
    }
}
