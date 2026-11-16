// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Centurion-only presentation choices, independent of campaign state.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FleetInformation {
    /// The original fleet-name/location notice.
    Original,
    /// Ship counts and embarked troops.
    Detailed,
    /// The detailed table together with the original name/location notice.
    #[default]
    Both,
}
impl FleetInformation {
    pub const ALL: [Self; 3] = [Self::Original, Self::Detailed, Self::Both];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Detailed => "detailed",
            Self::Both => "both",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Detailed => "Detailed",
            Self::Both => "Both",
        }
    }
}
impl std::str::FromStr for FleetInformation {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "original" => Ok(Self::Original),
            "detailed" => Ok(Self::Detailed),
            "both" => Ok(Self::Both),
            _ => Err("fleet information must be original, detailed or both".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PopupCursor {
    /// Hide the pointer while a popup is open, as in the original game.
    Hidden,
    /// Keep the pointer visible while a popup is open.
    #[default]
    Visible,
}
impl PopupCursor {
    pub const ALL: [Self; 2] = [Self::Hidden, Self::Visible];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Visible => "visible",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Hidden => "Hidden",
            Self::Visible => "Visible",
        }
    }
}
impl std::str::FromStr for PopupCursor {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "hidden" => Ok(Self::Hidden),
            "visible" => Ok(Self::Visible),
            _ => Err("popup cursor must be hidden or visible".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MenuDecorations {
    /// Retain the original mouse presentation for menu decorations.
    #[default]
    Mouse,
    /// Retain the original keyboard presentation for menu decorations.
    Keyboard,
    /// Follow arrow navigation and mouse movement/clicks.
    Dynamic,
}
impl MenuDecorations {
    pub const ALL: [Self; 3] = [Self::Mouse, Self::Keyboard, Self::Dynamic];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Keyboard => "keyboard",
            Self::Dynamic => "dynamic",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mouse => "Mouse",
            Self::Keyboard => "Keyboard",
            Self::Dynamic => "Dynamic",
        }
    }
}
impl std::str::FromStr for MenuDecorations {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "mouse" => Ok(Self::Mouse),
            "keyboard" => Ok(Self::Keyboard),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err("menu decorations must be mouse, keyboard or dynamic".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CenturionOptions {
    pub fleet_information: FleetInformation,
    pub popup_cursor: PopupCursor,
    pub menu_decorations: MenuDecorations,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CENTURION_FLEET_INFORMATION as KEY, CENTURION_MENU_DECORATIONS as DECORATIONS,
        CENTURION_POPUP_CURSOR as CURSOR, Draft, ENHANCEMENTS_SECTION, Launch, Patch,
        RuntimeSettings, Session,
    };
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
                "reforge-centurion-preferences-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
        fn session(&self, game: &str) -> Session {
            Session::load(Launch {
                game_id: game.into(),
                entry_id: Some("copy".into()),
                path: Some(self.path()),
                cli: Patch::new(),
            })
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn menu_decorations_validate_and_default_to_mouse() {
        let mut values = RuntimeSettings::default();
        assert_eq!(values.centurion.menu_decorations, MenuDecorations::Mouse);
        for choice in MenuDecorations::ALL {
            values.set(DECORATIONS, choice.as_str()).unwrap();
            assert_eq!(values.centurion.menu_decorations, choice);
            assert_eq!(values.get(DECORATIONS), choice.as_str());
        }
        for invalid in ["", "original", "reforged", "visible", "true", "0"] {
            assert!(values.set(DECORATIONS, invalid).is_err());
            assert_eq!(values.centurion.menu_decorations, MenuDecorations::Dynamic);
        }
        assert_eq!(crate::section_index(DECORATIONS), ENHANCEMENTS_SECTION);
    }

    #[test]
    fn menu_decorations_are_per_entry_and_explicit_choices_survive_unrelated_edits() {
        let temp = Temp::new();
        let missing = temp.session("centurion-defender-of-rome");
        assert_eq!(
            missing.effective(&missing.saved).centurion.menu_decorations,
            MenuDecorations::Mouse
        );
        for choice in MenuDecorations::ALL {
            fs::write(temp.path(), format!("[global.enhancements]\nmenu_decorations = 'keyboard'\n[entries.copy.enhancements]\nmenu_decorations = '{}' # retain\nunknown = 'retained'\n[entries.copy.display]\nwindow_mode = 'fullscreen'\n", choice.as_str())).unwrap();
            let mut session = temp.session("centurion-defender-of-rome");
            assert_eq!(
                session.saved.global_values().centurion.menu_decorations,
                MenuDecorations::Mouse
            );
            assert_eq!(
                session.effective(&session.saved).centurion.menu_decorations,
                choice
            );
            let mut draft = session.saved.clone();
            draft.edit(false, DECORATIONS, "dynamic".into());
            assert!(!draft.global.contains_key(DECORATIONS));
            draft.edit_game(CURSOR, "hidden".into(), Some(&session.saved));
            session.commit(draft).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            assert!(text.contains(&format!(
                "menu_decorations = '{}' # retain",
                choice.as_str()
            )));
            assert!(!text.contains(DECORATIONS));
            assert!(!text.contains("preset"));
        }
        let mut other = temp.session("mach-3");
        assert!(!other.saved.game.contains_key(DECORATIONS));
        other.cli.insert(DECORATIONS.into(), "keyboard".into());
        assert_eq!(
            other.effective(&other.saved).centurion.menu_decorations,
            MenuDecorations::Mouse
        );
        let mut session = temp.session("centurion-defender-of-rome");
        let mut draft = session.saved.clone();
        assert!(draft.has_game_overrides(ENHANCEMENTS_SECTION));
        draft.reset_section(true, ENHANCEMENTS_SECTION);
        assert!(!draft.has_game_overrides(ENHANCEMENTS_SECTION));
        assert!(draft.game_values().fullscreen);
        assert_eq!(
            draft.game_values().centurion.menu_decorations,
            MenuDecorations::Mouse
        );
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        assert!(
            doc["entries"]["copy"]["enhancements"]
                .get("menu_decorations")
                .is_none()
        );
        assert!(text.contains("unknown = 'retained'"));
        fs::write(temp.path(), "[entries.copy.enhancements]\nmenu_decorations = 'invalid'\nfleet_information = 'original'\n").unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert!(!session.saved.game.contains_key(DECORATIONS));
        let options = session.effective(&session.saved).centurion;
        assert_eq!(options.menu_decorations, MenuDecorations::Mouse);
        assert_eq!(options.fleet_information, FleetInformation::Original);
    }

    #[test]
    fn popup_cursor_validates_and_defaults_to_visible() {
        let mut values = RuntimeSettings::default();
        assert_eq!(values.centurion.popup_cursor, PopupCursor::Visible);
        for choice in PopupCursor::ALL {
            values.set(CURSOR, choice.as_str()).unwrap();
            assert_eq!(values.centurion.popup_cursor, choice);
            assert_eq!(values.get(CURSOR), choice.as_str());
        }
        for invalid in ["", "original", "reforged", "true", "false", "0"] {
            assert!(values.set(CURSOR, invalid).is_err());
            assert_eq!(values.centurion.popup_cursor, PopupCursor::Visible);
        }
        assert_eq!(crate::section_index(CURSOR), ENHANCEMENTS_SECTION);
    }

    #[test]
    fn popup_cursor_is_per_entry_persists_and_resets_with_enhancements() {
        let temp = Temp::new();
        fs::write(temp.path(), "[global.enhancements]\npopup_cursor = 'hidden'\n[entries.copy.enhancements]\npopup_cursor = 'hidden' # keep\nunknown = 'retained'\n[entries.copy.display]\nwindow_mode = 'fullscreen'\n").unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session.saved.global_values().centurion.popup_cursor,
            PopupCursor::Visible
        );
        assert_eq!(
            session.effective(&session.saved).centurion.popup_cursor,
            PopupCursor::Hidden
        );
        let other = temp.session("alley-cat");
        assert!(!other.saved.game.contains_key(CURSOR));
        assert_eq!(
            other.effective(&other.saved).centurion.popup_cursor,
            PopupCursor::Visible
        );
        let mut draft = session.saved.clone();
        draft.edit(false, CURSOR, "hidden".into());
        assert!(!draft.global.contains_key(CURSOR));
        draft.edit_game(KEY, "original".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("popup_cursor = 'hidden' # keep"));
        assert!(!text.contains(CURSOR));
        assert!(!text.contains("preset"));
        let mut session = temp.session("centurion-defender-of-rome");
        let mut draft = session.saved.clone();
        assert!(draft.has_game_overrides(ENHANCEMENTS_SECTION));
        draft.reset_section(true, ENHANCEMENTS_SECTION);
        assert!(!draft.has_game_overrides(ENHANCEMENTS_SECTION));
        assert!(draft.game_values().fullscreen);
        assert_eq!(draft.game_values().centurion, CenturionOptions::default());
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        assert!(
            doc["entries"]["copy"]["enhancements"]
                .get("popup_cursor")
                .is_none()
        );
        assert!(text.contains("unknown = 'retained'"));
    }

    #[test]
    fn invalid_popup_cursor_recovers_and_other_games_ignore_cli_choice() {
        let temp = Temp::new();
        fs::write(temp.path(), "[entries.copy.enhancements]\npopup_cursor = 'invalid'\nfleet_information = 'original'\n").unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert!(!session.saved.game.contains_key(CURSOR));
        assert_eq!(
            session.effective(&session.saved).centurion.popup_cursor,
            PopupCursor::Visible
        );
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Original
        );
        let mut other = temp.session("mach-3");
        other.cli.insert(CURSOR.into(), "hidden".into());
        assert_eq!(
            other.effective(&other.saved).centurion.popup_cursor,
            PopupCursor::Visible
        );
    }

    #[test]
    fn fleet_information_is_validated_and_defaults_to_both() {
        assert_eq!(
            CenturionOptions::default().fleet_information,
            FleetInformation::Both
        );
        let mut values = RuntimeSettings::default();
        for choice in FleetInformation::ALL {
            values.set(KEY, choice.as_str()).unwrap();
            assert_eq!(values.centurion.fleet_information, choice);
            assert_eq!(values.get(KEY), choice.as_str());
        }
        values.set(KEY, "detailed").unwrap();
        for invalid in ["", "reforged", "mixed", "true", "0"] {
            assert!(values.set(KEY, invalid).is_err());
            assert_eq!(
                values.centurion.fleet_information,
                FleetInformation::Detailed
            );
        }
    }

    #[test]
    fn both_fleet_information_round_trips_without_a_saved_preset() {
        let temp = Temp::new();
        fs::write(
            temp.path(),
            "[entries.copy.enhancements]\nfleet_information = 'both'\n",
        )
        .unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        let saved = session.saved.clone();
        let mut draft = session.saved.clone();
        draft.edit_game(KEY, "original".into(), Some(&saved));
        draft.edit_game(KEY, "both".into(), Some(&saved));
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("fleet_information = 'both'"));
        assert!(!text.contains("preset"));
        let loaded = temp.session("centurion-defender-of-rome");
        assert!(loaded.warning.is_none());
        assert_eq!(
            loaded.effective(&loaded.saved).centurion.fleet_information,
            FleetInformation::Both
        );
    }

    #[test]
    fn missing_values_use_both_and_migrated_detailed_stays_explicit() {
        let temp = Temp::new();
        let session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Both
        );
        fs::write(
            temp.path(),
            "[entries.copy.display]\nfleet_information = 'detailed' # retain choice\n",
        )
        .unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Detailed
        );
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("[entries.copy.enhancements]"));
        assert!(text.contains("fleet_information = 'detailed' # retain choice"));
    }

    #[test]
    fn fleet_information_is_per_entry_and_ignores_global_and_other_game_values() {
        let temp = Temp::new();
        fs::write(temp.path(), "[global.display]\nfleet_information = 'original'\n[entries.copy.display]\nfleet_information = 'original' # keep\nunknown = 'retained'\n[entries.copy.audio]\nmaster_volume = 43\n").unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session.saved.global_values().centurion,
            CenturionOptions::default()
        );
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Original
        );
        let document = fs::read_to_string(temp.path())
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            document["entries"]["copy"]["enhancements"]["fleet_information"].as_str(),
            Some("original")
        );
        assert!(
            document["entries"]["copy"]["display"]
                .get("fleet_information")
                .is_none()
        );
        let other = temp.session("alley-cat");
        assert_eq!(
            other.effective(&other.saved).centurion,
            CenturionOptions::default()
        );
        assert!(!other.saved.game.contains_key(KEY));
        let mut global = Draft::default();
        global.edit(false, KEY, "original".into());
        assert!(global.global.is_empty());
        let mut draft = session.saved.clone();
        draft.edit(false, "window_mode", "fullscreen".into());
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("fleet_information = 'original' # keep"));
        assert!(text.contains("unknown = 'retained'"));
        assert!(text.contains("master_volume = 43"));
        assert!(!text.contains("centurion_fleet_information"));
        assert!(!text.contains("preset"));
        let mut loaded = temp.session("centurion-defender-of-rome");
        let mut reset = loaded.saved.clone();
        reset.edit(true, "window_mode", "fullscreen".into());
        let mut enhancements_only = reset.clone();
        enhancements_only.reset_section(true, ENHANCEMENTS_SECTION);
        assert!(
            enhancements_only.game_values().fullscreen,
            "Enhancements reset must preserve Display"
        );
        assert_eq!(crate::section_index(KEY), ENHANCEMENTS_SECTION);
        assert!(reset.has_game_overrides(ENHANCEMENTS_SECTION));
        reset.reset_section(true, 0);
        assert!(reset.has_game_overrides(ENHANCEMENTS_SECTION));
        reset.reset_section(true, ENHANCEMENTS_SECTION);
        assert!(!reset.has_game_overrides(ENHANCEMENTS_SECTION));
        assert_eq!(reset.game_values().centurion, CenturionOptions::default());
        assert_eq!(reset.game["master_volume"], "43");
        loaded.commit(reset).unwrap();
        assert!(
            !temp
                .session("centurion-defender-of-rome")
                .saved
                .game
                .contains_key(KEY)
        );
    }

    #[test]
    fn fleet_information_retains_explicit_defaults_and_recovers_from_invalid_values() {
        let temp = Temp::new();
        fs::write(
            temp.path(),
            "[entries.copy.enhancements]\nfleet_information = 'detailed'\n",
        )
        .unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        let saved = session.saved.clone();
        let mut draft = saved.clone();
        draft.edit_game(KEY, "original".into(), Some(&saved));
        draft.edit_game(KEY, "detailed".into(), Some(&saved));
        assert_eq!(draft, saved);
        session.commit(draft).unwrap();
        assert_eq!(
            temp.session("centurion-defender-of-rome").saved.game[KEY],
            "detailed"
        );
        fs::write(
            temp.path(),
            "[entries.copy.enhancements]\nfleet_information = 'invalid'\n[entries.copy.display]\nwindow_mode = 'fullscreen'\n",
        )
        .unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert!(!session.saved.game.contains_key(KEY));
        assert_eq!(
            session.effective(&session.saved).centurion,
            CenturionOptions::default()
        );
        assert!(session.saved.game_values().fullscreen);
        let mut other = temp.session("alley-cat");
        other.cli.insert(KEY.into(), "original".into());
        assert_eq!(
            other.effective(&other.saved).centurion,
            CenturionOptions::default()
        );
    }
}
