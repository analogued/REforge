// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Centurion-only presentation choices, independent of campaign state.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FleetInformation {
    /// The original fleet-name/location notice.
    Original,
    /// Ship counts, transport assignment and readiness.
    #[default]
    Detailed,
}
impl FleetInformation {
    pub const ALL: [Self; 2] = [Self::Original, Self::Detailed];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Detailed => "detailed",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Detailed => "Detailed",
        }
    }
}
impl std::str::FromStr for FleetInformation {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "original" => Ok(Self::Original),
            "detailed" => Ok(Self::Detailed),
            _ => Err("fleet information must be original or detailed".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CenturionOptions {
    pub fleet_information: FleetInformation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CENTURION_FLEET_INFORMATION as KEY, Draft, ENHANCEMENTS_SECTION, Launch, Patch,
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
    fn fleet_information_is_validated_and_defaults_to_detailed() {
        assert_eq!(
            CenturionOptions::default().fleet_information,
            FleetInformation::Detailed
        );
        let mut values = RuntimeSettings::default();
        for choice in FleetInformation::ALL {
            values.set(KEY, choice.as_str()).unwrap();
            assert_eq!(values.centurion.fleet_information, choice);
            assert_eq!(values.get(KEY), choice.as_str());
        }
        for invalid in ["", "reforged", "mixed", "true", "0"] {
            assert!(values.set(KEY, invalid).is_err());
            assert_eq!(
                values.centurion.fleet_information,
                FleetInformation::Detailed
            );
        }
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
