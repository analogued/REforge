// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Launcher presentation preferences and opaque engine option overrides.
//!
//! Engines define native option names and values. This module only stores their
//! explicit selections; ordinary Settings drafts leave this namespace intact.

use std::{collections::BTreeMap, path::Path};

use toml_edit::{DocumentMut, Item, Table, value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preferences {
    pub theme: String,
    pub layout: String,
    pub selected_entry: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "classic-dark".into(),
            layout: "split".into(),
            selected_entry: None,
        }
    }
}

/// Unknown identifiers survive loading and unrelated edits. The UI resolves its
/// own fallback without silently replacing a preference from a newer build.
pub fn load(path: &Path) -> Result<Preferences, String> {
    preferences(&super::read_document(path, "settings")?)
}

pub fn save_theme(path: &Path, theme: &str) -> Result<(), String> {
    save_preference(path, "theme", theme)
}

pub fn save_layout(path: &Path, layout: &str) -> Result<(), String> {
    save_preference(path, "layout", layout)
}

/// Remember a library entry independently of its release, directory or order.
pub fn save_selected_entry(path: &Path, entry_id: Option<&str>) -> Result<(), String> {
    super::edit_document(path, "settings", |doc| {
        preferences(doc)?;
        schema(doc);
        let table = table_mut(doc, &["launcher"])?;
        if let Some(entry_id) = entry_id {
            set_string(table, "selected_entry", entry_id);
        } else {
            table
                .as_table_like_mut()
                .expect("validated table")
                .remove("selected_entry");
        }
        Ok(())
    })
}

/// Return explicit native selections, including values equal to engine defaults.
/// Engine descriptors, rather than this storage layer, validate their meaning.
pub fn native_options(path: &Path, entry_id: &str) -> Result<BTreeMap<String, String>, String> {
    nonempty(entry_id, "entry identifier")?;
    native(&super::read_document(path, "settings")?, entry_id)
}

/// Set a pin, or remove it to inherit the engine's default. Each edit merges the
/// latest document under the same lock used by Settings saves.
pub fn save_native_option(
    path: &Path,
    entry_id: &str,
    key: &str,
    selection: Option<&str>,
) -> Result<(), String> {
    nonempty(entry_id, "entry identifier")?;
    nonempty(key, "option identifier")?;
    super::edit_document(path, "settings", |doc| {
        native(doc, entry_id)?;
        schema(doc);
        let route = ["entries", entry_id, "launcher"];
        if let Some(selection) = selection {
            set_string(table_mut(doc, &route)?, key, selection);
        } else if find_table(doc, &route)?.is_some() {
            table_mut(doc, &route)?
                .as_table_like_mut()
                .expect("validated table")
                .remove(key);
        }
        Ok(())
    })
}

/// Remove only this profile's preferences; saves and original files are retained.
pub fn remove_entry(path: &Path, entry_id: &str) -> Result<(), String> {
    let doc = super::read_document(path, "settings")?;
    if find_table(&doc, &["entries", entry_id])?.is_none() {
        return Ok(());
    }
    super::edit_document(path, "settings", |doc| {
        if let Some(entries) = doc.get_mut("entries").and_then(Item::as_table_like_mut) {
            entries.remove(entry_id);
        }
        Ok(())
    })
}

fn save_preference(path: &Path, key: &str, selection: &str) -> Result<(), String> {
    super::edit_document(path, "settings", |doc| {
        preferences(doc)?;
        schema(doc);
        set_string(table_mut(doc, &["launcher"])?, key, selection);
        Ok(())
    })
}

fn preferences(doc: &DocumentMut) -> Result<Preferences, String> {
    let mut preferences = Preferences::default();
    if let Some(table) = find_table(doc, &["launcher"])? {
        for (key, target) in [
            ("theme", &mut preferences.theme),
            ("layout", &mut preferences.layout),
        ] {
            if let Some(item) = table.get(key) {
                *target = item
                    .as_str()
                    .ok_or_else(|| format!("Invalid launcher {key}; settings file preserved"))?
                    .into();
            }
        }
        if let Some(item) = table.get("selected_entry") {
            preferences.selected_entry = Some(
                item.as_str()
                    .ok_or("Invalid launcher selected_entry; settings file preserved")?
                    .into(),
            );
        }
    }
    Ok(preferences)
}

fn native(doc: &DocumentMut, entry_id: &str) -> Result<BTreeMap<String, String>, String> {
    let mut selections = BTreeMap::new();
    if let Some(table) = find_table(doc, &["entries", entry_id, "launcher"])? {
        for (key, item) in table.iter() {
            selections.insert(
                key.into(),
                item.as_str()
                    .ok_or_else(|| {
                        format!(
                            "Invalid launcher option {key} for {entry_id}; settings file preserved"
                        )
                    })?
                    .into(),
            );
        }
    }
    Ok(selections)
}

fn nonempty(text: &str, kind: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        Err(format!("Empty launcher {kind}; settings file preserved"))
    } else {
        Ok(())
    }
}

fn schema(doc: &mut DocumentMut) {
    doc["schema_version"] = value(2);
}

fn find_table<'a>(
    doc: &'a DocumentMut,
    route: &[&str],
) -> Result<Option<&'a dyn toml_edit::TableLike>, String> {
    let mut item = doc.as_item();
    for part in route {
        let table = item.as_table_like().ok_or_else(|| invalid_section(route))?;
        let Some(next) = table.get(part) else {
            return Ok(None);
        };
        item = next;
    }
    item.as_table_like()
        .map(Some)
        .ok_or_else(|| invalid_section(route))
}

pub(crate) fn table_mut<'a>(
    doc: &'a mut DocumentMut,
    route: &[&str],
) -> Result<&'a mut Item, String> {
    let mut item = doc.as_item_mut();
    for part in route {
        if !item.is_table_like() {
            return Err(invalid_section(route));
        }
        item = &mut item[part];
        if item.is_none() {
            *item = Item::Table(Table::new());
        }
    }
    if !item.is_table_like() {
        return Err(invalid_section(route));
    }
    Ok(item)
}

fn invalid_section(route: &[&str]) -> String {
    format!(
        "Invalid {} section; settings file preserved",
        route.join(".")
    )
}

fn set_string(table: &mut Item, key: &str, text: &str) {
    let mut replacement = toml_edit::Value::from(text);
    if let Some(previous) = table.get(key).and_then(Item::as_value) {
        *replacement.decor_mut() = previous.decor().clone();
    }
    table[key] = Item::Value(replacement);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Launch, Patch, Session};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "reforge-launcher-preferences-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_and_independent_edits_preserve_unknown_preferences_and_data() {
        let temp = Temp::new();
        let path = temp.path();
        assert_eq!(load(&path).unwrap(), Preferences::default());
        assert!(!path.exists());
        fs::write(
            &path,
            "schema_version = 1\n[launcher]\ntheme = 'future-theme' # retained\ncustom = [1, 2]\n[unrelated]\nvalue = 42\n",
        )
        .unwrap();
        save_layout(&path, "gallery").unwrap();
        assert_eq!(load(&path).unwrap().theme, "future-theme");
        save_theme(&path, "soft-tonal").unwrap();
        assert_eq!(load(&path).unwrap().layout, "gallery");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# retained"));
        assert!(text.contains("custom = [1, 2]"));
        assert!(text.contains("value = 42"));
    }

    #[test]
    fn native_pins_and_ordinary_settings_survive_each_others_saves() {
        let temp = Temp::new();
        let path = temp.path();
        save_native_option(&path, "alley-cat", "machine", Some("cga")).unwrap();
        save_native_option(&path, "mach-3", "video", Some("mono")).unwrap();
        save_theme(&path, "warm-amber").unwrap();
        let mut session = Session::load(Launch {
            game_id: "alley-cat".into(),
            entry_id: Some("alley-cat".into()),
            path: Some(path.clone()),
            cli: Patch::new(),
        });
        let mut draft = session.saved.clone();
        draft
            .global
            .insert("window_mode".into(), "fullscreen".into());
        draft.game.insert("control_mode".into(), "original".into());
        session.commit(draft).unwrap();
        save_layout(&path, "compact").unwrap();
        assert_eq!(
            native_options(&path, "alley-cat").unwrap()["machine"],
            "cga"
        );
        assert_eq!(native_options(&path, "mach-3").unwrap()["video"], "mono");
        assert_eq!(load(&path).unwrap().theme, "warm-amber");
        assert_eq!(load(&path).unwrap().layout, "compact");
        save_native_option(&path, "alley-cat", "machine", None).unwrap();
        assert!(native_options(&path, "alley-cat").unwrap().is_empty());
        assert_eq!(native_options(&path, "mach-3").unwrap()["video"], "mono");
        let session = Session::load(Launch {
            game_id: "alley-cat".into(),
            entry_id: Some("alley-cat".into()),
            path: Some(path),
            cli: Patch::new(),
        });
        assert_eq!(session.saved.game["control_mode"], "original");
        assert!(session.saved.global_values().fullscreen);
    }

    #[test]
    fn selected_entry_survives_unrelated_edits_and_can_be_cleared() {
        let temp = Temp::new();
        let path = temp.path();
        fs::write(&path, "[launcher]\ncustom = 'keep' # note\n").unwrap();
        save_selected_entry(&path, Some("alley-cat-abc123")).unwrap();
        save_theme(&path, "crisp-light").unwrap();
        save_layout(&path, "compact").unwrap();
        save_native_option(&path, "alley-cat-abc123", "machine", Some("pcjr")).unwrap();
        assert_eq!(
            load(&path).unwrap().selected_entry.as_deref(),
            Some("alley-cat-abc123")
        );
        save_selected_entry(&path, None).unwrap();
        let preferences = load(&path).unwrap();
        assert_eq!(preferences.selected_entry, None);
        assert_eq!(preferences.theme, "crisp-light");
        assert_eq!(preferences.layout, "compact");
        assert_eq!(
            native_options(&path, "alley-cat-abc123").unwrap()["machine"],
            "pcjr"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("custom = 'keep' # note"));
        assert!(!text.contains("selected_entry"));
    }

    #[test]
    fn malformed_presentation_sections_and_documents_are_never_rewritten() {
        let temp = Temp::new();
        let path = temp.path();
        for malformed in [
            "not toml",
            "schema_version = 99",
            "launcher = 'broken'",
            "[launcher]\ntheme = 7",
            "[launcher]\nlayout = false",
            "[launcher]\nselected_entry = 42",
        ] {
            fs::write(&path, malformed).unwrap();
            assert!(load(&path).is_err(), "{malformed}");
            assert!(save_theme(&path, "classic-dark").is_err());
            assert!(save_layout(&path, "split").is_err());
            assert!(save_selected_entry(&path, Some("alley-cat-abc123")).is_err());
            assert!(save_selected_entry(&path, None).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), malformed);
        }
    }

    #[test]
    fn inline_tables_and_literal_entry_keys_remain_valid_after_edits() {
        let temp = Temp::new();
        let path = temp.path();
        fs::write(&path, "launcher = { theme = 'future', custom = 'keep' }\nentries = { 'custom.game' = { launcher = { mode = 'one' } } }\n").unwrap();
        save_layout(&path, "gallery").unwrap();
        save_native_option(&path, "custom.game", "mode", Some("two")).unwrap();
        save_native_option(&path, "custom.game", "other", Some("default")).unwrap();
        assert_eq!(load(&path).unwrap().theme, "future");
        assert_eq!(load(&path).unwrap().layout, "gallery");
        assert_eq!(native_options(&path, "custom.game").unwrap()["mode"], "two");
        assert_eq!(
            native_options(&path, "custom.game").unwrap()["other"],
            "default"
        );
        save_native_option(&path, "custom.game", "mode", None).unwrap();
        assert!(
            !native_options(&path, "custom.game")
                .unwrap()
                .contains_key("mode")
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("custom = 'keep'")
        );
    }

    #[test]
    fn malformed_native_sections_and_values_are_never_rewritten() {
        let temp = Temp::new();
        let path = temp.path();
        for malformed in [
            "entries = 'broken'",
            "[entries]\nalley-cat = false",
            "[entries.alley-cat]\nlauncher = 1",
            "[entries.alley-cat.launcher]\nmachine = true",
            "[entries.alley-cat.launcher]\nmachine = 'cga'\nunknown = 7",
        ] {
            fs::write(&path, malformed).unwrap();
            assert!(native_options(&path, "alley-cat").is_err());
            for value in [Some("pcjr"), None] {
                assert!(save_native_option(&path, "alley-cat", "machine", value).is_err());
                assert_eq!(fs::read_to_string(&path).unwrap(), malformed);
            }
        }
    }

    #[test]
    fn lock_contention_and_invalid_identifiers_do_not_change_preferences() {
        let temp = Temp::new();
        let path = temp.path();
        save_theme(&path, "classic-dark").unwrap();
        let before = fs::read(&path).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        assert!(save_layout(&path, "gallery").is_err());
        assert!(save_native_option(&path, "alley-cat", "machine", Some("pcjr")).is_err());
        assert!(save_native_option(&path, "", "machine", Some("pcjr")).is_err());
        assert!(save_native_option(&path, "alley-cat", " ", Some("pcjr")).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
