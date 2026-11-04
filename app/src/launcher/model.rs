// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Library-backed launcher state. Painting never opens files or starts games.
use super::{catalog::ThemeId, widgets::UiAction};
use crate::detection::{self, Game};
use reforge_sdl::launcher::{OptionDefinition, Storage};
use reforge_settings::{
    Launch, Session,
    library::{Entry, Library},
};
use std::{
    collections::BTreeMap,
    io::IsTerminal,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutVariant {
    #[default]
    SplitInspector,
    GalleryShelf,
    CompactDeck,
}
impl LayoutVariant {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::SplitInspector, Self::GalleryShelf, Self::CompactDeck];
    pub fn slug(self) -> &'static str {
        match self {
            Self::SplitInspector => "split",
            Self::GalleryShelf => "gallery",
            Self::CompactDeck => "compact",
        }
    }
    pub fn parse(value: &str) -> Self {
        match value {
            "gallery" => Self::GalleryShelf,
            "compact" => Self::CompactDeck,
            _ => Self::SplitInspector,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    Verified,
    Missing(String),
}
impl EntryStatus {
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing(_))
    }
}
#[derive(Clone, Debug, Default)]
pub struct Preferences {
    pub defaults: BTreeMap<&'static str, &'static str>,
    pub saved: BTreeMap<&'static str, &'static str>,
    pub native: Vec<&'static str>,
}
impl Preferences {
    fn from_options(options: &[OptionDefinition]) -> Self {
        let mut p = Self::default();
        for o in options {
            if let Some(c) = o.choices.iter().find(|c| c.value == o.default) {
                p.defaults.insert(o.label, c.label);
            }
            if matches!(o.storage, Storage::Native { .. }) {
                p.native.push(o.label);
            }
        }
        p
    }
    pub fn tooltip(&self, field: &str, current: &str) -> String {
        let origin = if self.native.contains(&field) {
            "Built-in"
        } else {
            "Global"
        };
        format!(
            "{origin} default: {}\nSaved: {}\nCurrent choice: {current}\nSelections save immediately, including defaults. Click the explicit selection again to follow defaults.",
            self.defaults[field],
            self.saved.get(field).copied().unwrap_or("Follows defaults")
        )
    }
}
#[derive(Clone, Debug)]
pub struct GameEntry {
    pub entry_id: String,
    pub title: String,
    pub edition: Option<&'static detection::Edition>,
    pub original_path: String,
    pub original: PathBuf,
    pub game_id: String,
    pub release_id: String,
    pub previews: BTreeMap<String, super::preview::sources::Source>,
    pub preview_attempted: bool,
    preview_option: &'static str,
    pub status: EntryStatus,
    pub preferences: Preferences,
    pub options: &'static [OptionDefinition],
    pub terminal_available: bool,
    pub error: Option<String>,
    native_preferences_loaded: bool,
    config: PathBuf,
}
impl GameEntry {
    pub fn from_entry(entry: &Entry, config: PathBuf, verify: bool) -> Self {
        let release = detection::RELEASES
            .iter()
            .find(|r| r.id == entry.release_id && r.game().id() == entry.game_id);
        let edition = release.map(|r| r.edition);
        let options = match release.map(|r| r.game()) {
            Some(Game::AlleyCat) => alleycat::launcher::OPTIONS,
            Some(Game::Mach3) => mach3::launcher::OPTIONS,
            Some(Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers) => {
                btz::launcher::OPTIONS
            }
            None => &[],
        };
        let status = if verify {
            match super::verify_entry(entry) {
                Ok(_) => EntryStatus::Verified,
                Err(error) => EntryStatus::Missing(bounded_diagnostic(&error)),
            }
        } else {
            EntryStatus::Verified
        };
        let mut result = Self {
            entry_id: entry.entry_id.clone(),
            title: entry.title.clone(),
            edition,
            original_path: entry.original.display().to_string(),
            original: entry.original.clone(),
            game_id: entry.game_id.clone(),
            release_id: entry.release_id.clone(),
            previews: BTreeMap::new(),
            preview_attempted: false,
            preview_option: match release.map(|r| r.game()) {
                Some(Game::AlleyCat) => alleycat::launcher::PREVIEW_OPTION,
                Some(Game::Mach3) => mach3::launcher::PREVIEW_OPTION,
                Some(Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers) | None => "",
            },
            status,
            preferences: Preferences::from_options(options),
            options,
            terminal_available: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
            error: None,
            native_preferences_loaded: false,
            config,
        };
        if let Err(e) = result.refresh_preferences() {
            result.error = Some(e);
        }
        result
    }
    pub fn tagline(&self) -> &'static str {
        self.edition
            .map_or("Original files unavailable", |edition| edition.game.tagline)
    }
    pub fn edition_label(&self) -> Option<&'static str> {
        self.edition.and_then(|edition| edition.label)
    }
    pub fn library_line(&self) -> String {
        self.edition
            .map_or_else(String::new, detection::Edition::library_line)
    }
    pub fn detail_line(&self) -> String {
        self.edition
            .map_or_else(String::new, detection::Edition::detail_line)
    }
    pub fn features_line(&self) -> String {
        self.edition
            .map_or_else(String::new, detection::Edition::features_line)
    }
    pub fn preview_variant(&self) -> &'static str {
        if self
            .edition
            .is_some_and(|edition| edition.game.identity.is_btz())
        {
            return btz::launcher::PREVIEW_VARIANT;
        }
        let values = self.option_values();
        self.options
            .iter()
            .find(|o| o.key() == self.preview_option)
            .and_then(|o| {
                o.choices
                    .iter()
                    .find(|c| Some(&c.label) == values.get(o.label))
                    .map(|c| c.value)
            })
            .unwrap_or("")
    }
    pub fn crt_preview(&self) -> bool {
        if self.terminal_selected() {
            return false;
        }
        let values = self.option_values();
        self.options
            .iter()
            .find(|o| o.key() == "presentation")
            .and_then(|o| {
                o.choices
                    .iter()
                    .find(|c| Some(&c.label) == values.get(o.label))
            })
            .is_some_and(|c| c.value == "hyllian")
    }
    pub fn option_choices(&self) -> Vec<(&'static str, Vec<&'static str>)> {
        self.options
            .iter()
            .map(|o| (o.label, o.choices.iter().map(|c| c.label).collect()))
            .collect()
    }
    pub fn terminal_selected(&self) -> bool {
        self.options
            .iter()
            .any(|o| matches!(o.storage, Storage::TerminalMode))
            && self.option_values().get("Renderer") == Some(&"Terminal")
    }
    pub fn option_enabled(&self, field: &str) -> bool {
        !self.terminal_selected() || !matches!(field, "Display" | "Window")
    }
    pub fn choice_enabled(&self, field: &str, choice: &str) -> bool {
        self.option_enabled(field)
            && !(field == "Renderer" && choice == "Terminal" && !self.terminal_available)
    }
    pub fn short_label(&self, field: &str, value: &'static str) -> &'static str {
        self.options
            .iter()
            .find(|option| option.label == field)
            .and_then(|option| option.choices.iter().find(|choice| choice.label == value))
            .map_or(value, |choice| choice.short_label)
    }
    pub fn option_values(&self) -> BTreeMap<&'static str, &'static str> {
        let mut v = self.preferences.defaults.clone();
        v.extend(self.preferences.saved.clone());
        v
    }
    pub fn full_mode_label(&self) -> String {
        let values = self.option_values();
        self.options
            .iter()
            .filter(|o| self.option_enabled(o.label))
            .filter_map(|o| {
                o.choices
                    .iter()
                    .find(|c| Some(&c.label) == values.get(o.label))
                    .and_then(|c| c.badge)
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
    pub fn palette_swatches(&self) -> &'static [[u8; 3]] {
        let v = self.option_values();
        self.options
            .iter()
            .find_map(|o| {
                o.choices
                    .iter()
                    .find(|c| Some(&c.label) == v.get(o.label))
                    .and_then(|c| c.palette)
            })
            .unwrap_or(&[])
    }
    pub fn native_arguments(&self) -> Vec<String> {
        if !self.native_preferences_loaded {
            return Vec::new();
        }
        let values = self.option_values();
        let mut args = Vec::new();
        for o in self.options {
            if let Storage::Native { argument, .. } = o.storage
                && let Some(choice) = o
                    .choices
                    .iter()
                    .find(|c| Some(&c.label) == values.get(o.label))
            {
                args.push(argument.into());
                args.push(choice.value.into());
            }
        }
        args
    }
    pub fn refresh_preferences(&mut self) -> Result<(), String> {
        let session = Session::load(Launch {
            game_id: self.game_id.clone(),
            entry_id: Some(self.entry_id.clone()),
            path: Some(self.config.clone()),
            cli: Default::default(),
        });
        let mut warning = session.warning;
        let native = match reforge_settings::launcher::native_options(&self.config, &self.entry_id)
        {
            Ok(native) => {
                self.native_preferences_loaded = true;
                native
            }
            Err(error) => {
                self.native_preferences_loaded = false;
                warning = Some(error);
                BTreeMap::new()
            }
        };
        let global = session.saved.global_values();
        let mut p = Preferences::default();
        for o in self.options {
            let (default, saved) = match o.storage {
                Storage::TerminalMode => (
                    "graphical".into(),
                    session.saved.game.get("renderer").map(|value| {
                        if value == "terminal" {
                            "terminal"
                        } else {
                            "graphical"
                        }
                    }),
                ),
                Storage::Shared(key) => {
                    if !reforge_settings::keys().any(|shared| shared == key) {
                        p.native.push(o.label);
                    }
                    (
                        global.get(key),
                        session.saved.game.get(key).map(String::as_str),
                    )
                }
                Storage::Native { key, .. } => {
                    p.native.push(o.label);
                    (o.default.into(), native.get(key).map(String::as_str))
                }
            };
            let choice = o
                .choices
                .iter()
                .find(|c| c.value == default)
                .or_else(|| o.choices.iter().find(|c| c.value == o.default))
                .expect("declared default");
            p.defaults.insert(o.label, choice.label);
            if let Some(choice) = saved.and_then(|s| o.choices.iter().find(|c| c.value == s)) {
                p.saved.insert(o.label, choice.label);
            }
        }
        self.preferences = p;
        warning.map_or(Ok(()), Err)
    }
    pub fn save_option(&mut self, field: &'static str, value: &'static str) {
        if !self.choice_enabled(field, value) {
            return;
        }
        let Some(option) = self.options.iter().find(|o| o.label == field) else {
            return;
        };
        let Some(choice) = option.choices.iter().find(|c| c.label == value) else {
            return;
        };
        let value = if self.preferences.saved.get(field) == Some(&value) {
            None
        } else {
            Some(choice.value)
        };
        let result = match option.storage {
            Storage::TerminalMode => {
                let mut s = Session::load(Launch {
                    game_id: self.game_id.clone(),
                    entry_id: Some(self.entry_id.clone()),
                    path: Some(self.config.clone()),
                    cli: Default::default(),
                });
                let mut d = s.saved.clone();
                if let Some(value) = value {
                    d.set_terminal(value == "terminal");
                    if value == "graphical" && !d.game.contains_key("renderer") {
                        // Like the other launch controls, clicking an inherited
                        // default pins it, even when its value is unchanged.
                        let renderer = d.global_values().renderer.as_str();
                        d.game.insert("renderer".into(), renderer.into());
                    }
                } else {
                    // Unpinning follows the global renderer. Selecting Graphical
                    // instead restores the dormant per-game graphical renderer.
                    d.game.remove("renderer");
                }
                s.commit(d)
            }
            Storage::Native { key, .. } => reforge_settings::launcher::save_native_option(
                &self.config,
                &self.entry_id,
                key,
                value,
            ),
            Storage::Shared(key) => {
                let mut s = Session::load(Launch {
                    game_id: self.game_id.clone(),
                    entry_id: Some(self.entry_id.clone()),
                    path: Some(self.config.clone()),
                    cli: Default::default(),
                });
                let mut d = s.saved.clone();
                if let Some(value) = value {
                    d.game.insert(key.into(), value.into());
                } else {
                    d.game.remove(key);
                }
                s.commit(d)
            }
        };
        if let Err(e) = result.and_then(|_| self.refresh_preferences()) {
            self.error = Some(e);
        }
    }
}
pub fn bounded_diagnostic(text: &str) -> String {
    let mut v = text.lines().take(4).collect::<Vec<_>>().join("\n");
    if v.chars().count() > 500 {
        v = v.chars().take(500).collect();
        v.push('…');
    }
    v
}

pub struct State {
    pub library: Library,
    pub config: PathBuf,
    pub layout: LayoutVariant,
    pub saved_theme: ThemeId,
    pub preview_draft: ThemeId,
    pub preview_open: bool,
    pub preview_focus_pending: bool,
    pub filter_query: String,
    pub options_open: bool,
    pub selected: Option<String>,
    pub games: Vec<GameEntry>,
    pub pending_remove: Option<String>,
    pub remove_focus_pending: bool,
    pub settings_open: bool,
    pub pending_action: Option<UiAction>,
    pub busy: bool,
    pub content_bottom: f32,
    pub notice: Option<(String, bool)>,
    pub notice_started: Instant,
}
impl State {
    pub fn new(path: PathBuf, config: PathBuf) -> Self {
        let pref = reforge_settings::launcher::load(&config);
        let (theme, layout, selected, error) = match pref {
            Ok(p) => (
                ThemeId::from_slug(&p.theme).unwrap_or_default(),
                LayoutVariant::parse(&p.layout),
                p.selected_entry,
                None,
            ),
            Err(e) => (
                ThemeId::default(),
                LayoutVariant::default(),
                None,
                Some((e, true)),
            ),
        };
        let mut s = Self {
            library: Library::new(path).with_storage(
                config.clone(),
                reforge_settings::library::default_saves_path(),
            ),
            config,
            layout,
            saved_theme: theme,
            preview_draft: theme,
            preview_open: false,
            preview_focus_pending: false,
            filter_query: String::new(),
            options_open: false,
            selected,
            games: Vec::new(),
            pending_remove: None,
            remove_focus_pending: false,
            settings_open: false,
            pending_action: None,
            busy: false,
            content_bottom: 0.0,
            notice: error,
            notice_started: Instant::now(),
        };
        s.refresh();
        s
    }
    pub fn refresh(&mut self) {
        match self.library.reload() {
            Ok(()) => {
                self.games = self
                    .library
                    .entries
                    .iter()
                    .map(|e| {
                        let mut game = GameEntry::from_entry(e, self.config.clone(), true);
                        if !game.status.is_missing()
                            && let Some(previous) = self.games.iter().find(|old| {
                                old.entry_id == game.entry_id
                                    && old.release_id == game.release_id
                                    && old.original == game.original
                                    && !old.status.is_missing()
                            })
                        {
                            game.previews = previous.previews.clone();
                            game.preview_attempted =
                                previous.preview_attempted && !previous.previews.is_empty();
                        }
                        game
                    })
                    .collect();
                if let Some(warning) = self.library.warning.clone() {
                    self.notify(warning, true);
                }
                if !self
                    .games
                    .iter()
                    .any(|g| self.selected.as_deref() == Some(g.entry_id.as_str()))
                {
                    self.selected = self.games.first().map(|g| g.entry_id.clone());
                }
            }
            Err(e) => self.notify(e, true),
        }
    }
    pub fn effective_theme(&self) -> ThemeId {
        if self.preview_open {
            self.preview_draft
        } else {
            self.saved_theme
        }
    }
    pub fn selected_index(&self) -> Option<usize> {
        self.games
            .iter()
            .position(|g| self.selected.as_deref() == Some(g.entry_id.as_str()))
    }
    pub fn selected_game(&self) -> Option<&GameEntry> {
        self.selected_index().map(|i| &self.games[i])
    }
    pub fn save_on_close(&mut self) -> Result<(), String> {
        if self.preview_open {
            self.commit_preview();
        }
        reforge_settings::launcher::save_selected_entry(&self.config, self.selected.as_deref())
    }
    pub fn toggle_preview(&mut self) {
        if self.preview_open {
            self.commit_preview();
        } else {
            self.preview_draft = self.saved_theme;
            self.preview_open = true;
            self.preview_focus_pending = true;
        }
    }
    pub fn commit_preview(&mut self) {
        if self.preview_draft != self.saved_theme {
            match reforge_settings::launcher::save_theme(&self.config, self.preview_draft.slug()) {
                Ok(()) => self.saved_theme = self.preview_draft,
                Err(e) => self.notify(e, true),
            }
        }
        self.preview_open = false;
    }
    pub fn revert_preview(&mut self) {
        self.preview_draft = self.saved_theme;
        self.preview_open = false;
    }
    pub fn set_layout(&mut self, layout: LayoutVariant) {
        if self.preview_open {
            self.commit_preview();
        }
        match reforge_settings::launcher::save_layout(&self.config, layout.slug()) {
            Ok(()) => self.layout = layout,
            Err(e) => self.notify(e, true),
        }
    }
    pub fn open_settings(&mut self) {
        if self.preview_open {
            self.commit_preview();
        }
        self.settings_open = true;
    }
    pub fn trigger_play(&mut self, id: String) {
        self.pending_action = Some(UiAction::Play(id));
    }
    pub fn trigger_add(&mut self) {
        self.pending_action = Some(UiAction::Add);
    }
    pub fn trigger_locate(&mut self, id: String) {
        self.pending_action = Some(UiAction::Locate(id));
    }
    pub fn request_remove(&mut self, id: String) {
        if self.games.iter().any(|g| g.entry_id == id) {
            if self.preview_open {
                self.commit_preview();
            }
            self.pending_remove = Some(id);
            self.remove_focus_pending = true;
        }
    }
    pub fn confirm_remove(&mut self) {
        if let Some(id) = self.pending_remove.take() {
            match self.library.remove(&id) {
                Ok(()) => {
                    self.refresh();
                    self.notify(
                        "Game removed from the library. Original files remain on disk.".into(),
                        false,
                    );
                }
                Err(e) => self.notify(e, true),
            }
        }
    }
    pub fn notify(&mut self, message: String, error: bool) {
        self.notice = Some((message, error));
        self.notice_started = Instant::now();
    }
    pub fn expire_notice(&mut self) {
        if self.notice.as_ref().is_some_and(|(_, e)| !*e)
            && self.notice_started.elapsed() >= Duration::from_secs(4)
        {
            self.notice = None;
        }
    }
}
