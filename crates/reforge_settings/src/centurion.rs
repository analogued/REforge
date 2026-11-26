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
pub enum MapHighlight {
    /// Retain the original map borders without pointer feedback.
    Off,
    /// Highlight the province beneath the map pointer.
    #[default]
    On,
}
impl MapHighlight {
    pub const ALL: [Self; 2] = [Self::Off, Self::On];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
        }
    }
}
impl std::str::FromStr for MapHighlight {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "off" => Ok(Self::Off),
            "on" => Ok(Self::On),
            _ => Err("map highlight must be off or on".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MapHoverInfo {
    /// Keep map information available through explicit inspection only.
    #[default]
    Off,
    /// Show information about the map subject beneath the pointer.
    On,
}
impl MapHoverInfo {
    pub const ALL: [Self; 2] = [Self::Off, Self::On];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
        }
    }
}
impl std::str::FromStr for MapHoverInfo {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "off" => Ok(Self::Off),
            "on" => Ok(Self::On),
            _ => Err("map hover info must be off or on".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppliedValueIndicator {
    /// No added tribute mark; retain the white applied difficulty label.
    Original,
    /// A subdued background band distinguishes the applied value from focus.
    #[default]
    SubduedBand,
    /// A compact mark beside the applied row.
    SideMarker,
    /// Underline the applied row's label.
    Underline,
}
impl AppliedValueIndicator {
    pub const ALL: [Self; 4] = [
        Self::Original,
        Self::SubduedBand,
        Self::SideMarker,
        Self::Underline,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::SubduedBand => "subdued-band",
            Self::SideMarker => "side-marker",
            Self::Underline => "underline",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::SubduedBand => "Subdued band",
            Self::SideMarker => "Side marker",
            Self::Underline => "Underline",
        }
    }
}
impl std::str::FromStr for AppliedValueIndicator {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "original" => Ok(Self::Original),
            "subdued-band" => Ok(Self::SubduedBand),
            "side-marker" => Ok(Self::SideMarker),
            "underline" => Ok(Self::Underline),
            _ => Err(
                "applied value indicator must be original, subdued-band, side-marker or underline"
                    .into(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CenturionOptions {
    pub fleet_information: FleetInformation,
    pub popup_cursor: PopupCursor,
    pub menu_decorations: MenuDecorations,
    pub map_highlight: MapHighlight,
    pub map_hover_info: MapHoverInfo,
    pub applied_value_indicator: AppliedValueIndicator,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CENTURION_APPLIED_VALUE_INDICATOR as INDICATOR, CENTURION_FLEET_INFORMATION as KEY,
        CENTURION_MAP_HIGHLIGHT as MAP, CENTURION_MAP_HOVER_INFO as HOVER,
        CENTURION_MENU_DECORATIONS as DECORATIONS, CENTURION_POPUP_CURSOR as CURSOR,
        ENHANCEMENTS_SECTION, Launch, Patch, RuntimeSettings, Session, SoundRenderer,
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
                directory: None,
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
    fn centurion_audio_uses_entry_storage_and_reset() {
        let temp = Temp::new();
        fs::write(temp.path(), "schema_version = 2\n[global.audio]\naudio_renderer = 'speaker'\n[entries.copy.audio]\nmaster_volume = 62\n").unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session.effective(&session.saved).audio.renderer,
            SoundRenderer::GameDefault
        );
        assert!(!session.saved.game.contains_key("audio_renderer"));
        let initial = fs::read(temp.path()).unwrap();
        session.commit(session.saved.clone()).unwrap();
        assert_eq!(fs::read(temp.path()).unwrap(), initial);
        let mut draft = session.saved.clone();
        draft.edit_game("audio_renderer", "adlib".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        let document = fs::read_to_string(temp.path())
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            document["entries"]["copy"]["audio"]["audio_renderer"].as_str(),
            Some("adlib")
        );
        assert!(document["entries"]["copy"].get("launcher").is_none());
        assert!(session.saved.has_custom_settings(2));
        let mut draft = session.saved.clone();
        draft.reset_section(ENHANCEMENTS_SECTION);
        assert_eq!(draft.game_values().audio.renderer, SoundRenderer::Adlib);
        draft.reset_section(2);
        assert_eq!(
            draft.game_values().audio.renderer,
            SoundRenderer::GameDefault
        );
        session.commit(draft).unwrap();
        let mut draft = session.saved.clone();
        draft.edit("audio_renderer", "default".into());
        session.commit(draft).unwrap();
        assert_eq!(
            session.effective(&session.saved).audio.renderer,
            SoundRenderer::GameDefault
        );
        assert_eq!(
            crate::AudioCapabilities {
                renderers: &[SoundRenderer::Adlib, SoundRenderer::Speaker],
                default: SoundRenderer::Adlib,
                categories: &[],
                apply: crate::ApplyTiming::NextLaunch,
            }
            .resolve(session.effective(&session.saved).audio.renderer),
            SoundRenderer::Adlib
        );
    }

    #[test]
    fn shared_audio_cli_is_temporary_and_unrelated_saves_preserve_concurrent_edits() {
        let temp = Temp::new();
        fs::write(
            temp.path(),
            "[entries.copy.audio]\naudio_renderer = 'adlib' # keep\n",
        )
        .unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        session
            .cli
            .insert("audio_renderer".into(), "speaker".into());
        assert_eq!(
            session.effective(&session.saved).audio.renderer,
            SoundRenderer::Speaker
        );
        assert_eq!(
            session.saved.game_values().audio.renderer,
            SoundRenderer::Adlib
        );
        let mut draft = session.saved.clone();
        draft.edit("master_volume", "70".into());
        session.commit(draft).unwrap();
        let doc = fs::read_to_string(temp.path())
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            doc["entries"]["copy"]["audio"]["audio_renderer"].as_str(),
            Some("adlib")
        );
        let mut latest = temp.session("centurion-defender-of-rome");
        let mut draft = latest.saved.clone();
        draft.edit("audio_renderer", "speaker".into());
        latest.commit(draft).unwrap();
        let mut unrelated = session.saved.clone();
        unrelated.edit("vsync", "false".into());
        session.commit(unrelated).unwrap();
        assert_eq!(
            temp.session("centurion-defender-of-rome")
                .saved
                .game_values()
                .audio
                .renderer,
            SoundRenderer::Speaker
        );
    }

    #[test]
    fn map_hover_info_validates_and_defaults_to_off() {
        let mut values = RuntimeSettings::default();
        assert_eq!(values.centurion.map_hover_info, MapHoverInfo::Off);
        for choice in MapHoverInfo::ALL {
            values.set(HOVER, choice.as_str()).unwrap();
            assert_eq!(values.centurion.map_hover_info, choice);
            assert_eq!(values.get(HOVER), choice.as_str());
        }
        for invalid in ["", "original", "reforged", "true", "false", "0"] {
            assert!(values.set(HOVER, invalid).is_err());
            assert_eq!(values.centurion.map_hover_info, MapHoverInfo::On);
        }
        assert_eq!(crate::section_index(HOVER), ENHANCEMENTS_SECTION);
    }

    #[test]
    fn map_hover_info_is_per_entry_preserves_explicit_values_and_resets_off() {
        let temp = Temp::new();
        let missing = temp.session("centurion-defender-of-rome");
        assert_eq!(
            missing.effective(&missing.saved).centurion.map_hover_info,
            MapHoverInfo::Off
        );
        for choice in MapHoverInfo::ALL {
            fs::write(temp.path(), format!("[global.enhancements]\nmap_hover_info = 'on'\n[entries.copy.enhancements]\nmap_hover_info = '{}' # retain choice\nunknown = 'retained'\n[entries.copy.display]\nwindow_mode = 'fullscreen'\n", choice.as_str())).unwrap();
            let mut session = temp.session("centurion-defender-of-rome");

            assert_eq!(
                session.effective(&session.saved).centurion.map_hover_info,
                choice
            );
            let mut draft = session.saved.clone();
            draft.edit_game(CURSOR, "hidden".into(), Some(&session.saved));
            session.commit(draft).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            assert!(text.contains(&format!(
                "map_hover_info = '{}' # retain choice",
                choice.as_str()
            )));
            assert!(!text.contains(HOVER));
            assert!(!text.contains("preset"));
            let mut reset = session.saved.clone();
            assert!(reset.has_custom_settings(ENHANCEMENTS_SECTION));
            reset.reset_section(ENHANCEMENTS_SECTION);
            assert!(!reset.has_custom_settings(ENHANCEMENTS_SECTION));
            assert_eq!(reset.game_values().centurion, CenturionOptions::default());
            assert!(reset.game_values().fullscreen);
            session.commit(reset).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            let document = text.parse::<toml_edit::DocumentMut>().unwrap();
            assert!(
                document["entries"]["copy"]["enhancements"]
                    .get("map_hover_info")
                    .is_none()
            );
            assert!(text.contains("unknown = 'retained'"));
        }
        let mut other = temp.session("mach-3");
        other.cli.insert(HOVER.into(), "on".into());
        assert_eq!(
            other.effective(&other.saved).centurion.map_hover_info,
            MapHoverInfo::Off
        );
        fs::write(temp.path(), "[entries.copy.enhancements]\nmap_hover_info = 'invalid'\nfleet_information = 'original'\n").unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert!(!session.saved.game.contains_key(HOVER));
        assert_eq!(
            session.effective(&session.saved).centurion.map_hover_info,
            MapHoverInfo::Off
        );
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Original
        );
    }

    #[test]
    fn applied_value_indicator_validates_and_defaults_to_subdued_band() {
        let mut values = RuntimeSettings::default();
        assert_eq!(
            values.centurion.applied_value_indicator,
            AppliedValueIndicator::SubduedBand
        );
        for choice in AppliedValueIndicator::ALL {
            values.set(INDICATOR, choice.as_str()).unwrap();
            assert_eq!(values.centurion.applied_value_indicator, choice);
            assert_eq!(values.get(INDICATOR), choice.as_str());
        }
        for invalid in ["", "reforged", "on", "off", "subdued_band", "true"] {
            assert!(values.set(INDICATOR, invalid).is_err());
            assert_eq!(
                values.centurion.applied_value_indicator,
                AppliedValueIndicator::Underline
            );
        }
        assert_eq!(crate::section_index(INDICATOR), ENHANCEMENTS_SECTION);
    }

    #[test]
    fn applied_value_indicator_is_per_entry_preserves_choices_and_resets() {
        let temp = Temp::new();
        let missing = temp.session("centurion-defender-of-rome");
        assert_eq!(
            missing
                .effective(&missing.saved)
                .centurion
                .applied_value_indicator,
            AppliedValueIndicator::SubduedBand
        );
        for choice in AppliedValueIndicator::ALL {
            fs::write(temp.path(), format!("[global.enhancements]\napplied_value_indicator = 'original'\n[entries.copy.enhancements]\napplied_value_indicator = '{}' # retain choice\nmenu_decorations = 'dynamic'\nunknown = 'retained'\n[entries.copy.display]\nwindow_mode = 'fullscreen'\n", choice.as_str())).unwrap();
            let mut session = temp.session("centurion-defender-of-rome");

            assert_eq!(
                session
                    .effective(&session.saved)
                    .centurion
                    .applied_value_indicator,
                choice
            );
            let mut draft = session.saved.clone();
            draft.edit_game(CURSOR, "hidden".into(), Some(&session.saved));
            session.commit(draft).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            assert!(text.contains(&format!(
                "applied_value_indicator = '{}' # retain choice",
                choice.as_str()
            )));
            assert!(!text.contains(INDICATOR));
            assert!(!text.contains("preset"));
            let mut reset = session.saved.clone();
            assert!(reset.has_custom_settings(ENHANCEMENTS_SECTION));
            reset.reset_section(ENHANCEMENTS_SECTION);
            assert!(!reset.has_custom_settings(ENHANCEMENTS_SECTION));
            assert_eq!(reset.game_values().centurion, CenturionOptions::default());
            assert!(reset.game_values().fullscreen);
            session.commit(reset).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            let document = text.parse::<toml_edit::DocumentMut>().unwrap();
            assert!(
                document["entries"]["copy"]["enhancements"]
                    .get("applied_value_indicator")
                    .is_none()
            );
            assert!(text.contains("unknown = 'retained'"));
        }
        let mut other = temp.session("mach-3");
        other.cli.insert(INDICATOR.into(), "original".into());
        assert_eq!(
            other
                .effective(&other.saved)
                .centurion
                .applied_value_indicator,
            AppliedValueIndicator::SubduedBand
        );
        fs::write(temp.path(), "[entries.copy.enhancements]\napplied_value_indicator = 'invalid'\nfleet_information = 'original'\n").unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert!(!session.saved.game.contains_key(INDICATOR));
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .applied_value_indicator,
            AppliedValueIndicator::SubduedBand
        );
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Original
        );
    }

    #[test]
    fn map_highlight_validates_defaults_and_remains_per_entry() {
        let mut values = RuntimeSettings::default();
        assert_eq!(values.centurion.map_highlight, MapHighlight::On);
        for choice in MapHighlight::ALL {
            values.set(MAP, choice.as_str()).unwrap();
            assert_eq!(values.centurion.map_highlight, choice);
            assert_eq!(values.get(MAP), choice.as_str());
        }
        for invalid in ["", "original", "reforged", "true", "false", "0"] {
            assert!(values.set(MAP, invalid).is_err());
            assert_eq!(values.centurion.map_highlight, MapHighlight::On);
        }
        assert_eq!(crate::section_index(MAP), ENHANCEMENTS_SECTION);

        let temp = Temp::new();
        fs::write(temp.path(), "[global.enhancements]\nmap_highlight = 'off'\n[entries.copy.enhancements]\nmap_highlight = 'off' # retain choice\nunknown = 'retained'\n[entries.copy.audio]\nmaster_volume = 43\n").unwrap();
        let mut session = temp.session("centurion-defender-of-rome");

        assert_eq!(
            session.effective(&session.saved).centurion.map_highlight,
            MapHighlight::Off
        );
        let mut draft = session.saved.clone();
        draft.edit_game(CURSOR, "hidden".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        assert!(
            fs::read_to_string(temp.path())
                .unwrap()
                .contains("map_highlight = 'off' # retain choice")
        );
        assert!(session.saved.has_custom_settings(ENHANCEMENTS_SECTION));
        let mut draft = session.saved.clone();
        draft.reset_section(ENHANCEMENTS_SECTION);
        assert_eq!(
            draft.game_values().centurion.map_highlight,
            MapHighlight::On
        );
        assert!(!draft.has_custom_settings(ENHANCEMENTS_SECTION));
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        assert!(
            doc["entries"]["copy"]["enhancements"]
                .get("map_highlight")
                .is_none()
        );
        assert!(text.contains("unknown = 'retained'"));
        assert!(text.contains("master_volume = 43"));
        let mut other = temp.session("mach-3");
        other.cli.insert(MAP.into(), "off".into());
        assert_eq!(
            other.effective(&other.saved).centurion.map_highlight,
            MapHighlight::On
        );
        fs::write(temp.path(), "[entries.copy.enhancements]\nmap_highlight = 'invalid'\nfleet_information = 'original'\n").unwrap();
        let session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        assert_eq!(
            session.effective(&session.saved).centurion.map_highlight,
            MapHighlight::On
        );
        assert_eq!(
            session
                .effective(&session.saved)
                .centurion
                .fleet_information,
            FleetInformation::Original
        );
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
                session.effective(&session.saved).centurion.menu_decorations,
                choice
            );
            let mut draft = session.saved.clone();
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
        assert!(draft.has_custom_settings(ENHANCEMENTS_SECTION));
        draft.reset_section(ENHANCEMENTS_SECTION);
        assert!(!draft.has_custom_settings(ENHANCEMENTS_SECTION));
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
        draft.edit_game(KEY, "original".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("popup_cursor = 'hidden' # keep"));
        assert!(!text.contains(CURSOR));
        assert!(!text.contains("preset"));
        let mut session = temp.session("centurion-defender-of-rome");
        let mut draft = session.saved.clone();
        assert!(draft.has_custom_settings(ENHANCEMENTS_SECTION));
        draft.reset_section(ENHANCEMENTS_SECTION);
        assert!(!draft.has_custom_settings(ENHANCEMENTS_SECTION));
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
            "[entries.copy.enhancements]\nfleet_information = 'detailed' # retain choice\n",
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
        fs::write(temp.path(), "[global.display]\nfleet_information = 'original'\n[entries.copy.enhancements]\nfleet_information = 'original' # keep\nunknown = 'retained'\n[entries.copy.audio]\nmaster_volume = 43\n").unwrap();
        let mut session = temp.session("centurion-defender-of-rome");

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
        assert!(document["entries"]["copy"].get("display").is_none());
        let other = temp.session("alley-cat");
        assert_eq!(
            other.effective(&other.saved).centurion,
            CenturionOptions::default()
        );
        assert!(!other.saved.game.contains_key(KEY));

        let mut draft = session.saved.clone();
        draft.edit("window_mode", "fullscreen".into());
        session.commit(draft).unwrap();
        let text = fs::read_to_string(temp.path()).unwrap();
        assert!(text.contains("fleet_information = 'original' # keep"));
        assert!(text.contains("unknown = 'retained'"));
        assert!(text.contains("master_volume = 43"));
        assert!(!text.contains("centurion_fleet_information"));
        assert!(!text.contains("preset"));
        let mut loaded = temp.session("centurion-defender-of-rome");
        let mut reset = loaded.saved.clone();
        reset.edit("window_mode", "fullscreen".into());
        let mut enhancements_only = reset.clone();
        enhancements_only.reset_section(ENHANCEMENTS_SECTION);
        assert!(
            enhancements_only.game_values().fullscreen,
            "Enhancements reset must preserve Display"
        );
        assert_eq!(crate::section_index(KEY), ENHANCEMENTS_SECTION);
        assert!(reset.has_custom_settings(ENHANCEMENTS_SECTION));
        reset.reset_section(0);
        assert!(reset.has_custom_settings(ENHANCEMENTS_SECTION));
        reset.reset_section(ENHANCEMENTS_SECTION);
        assert!(!reset.has_custom_settings(ENHANCEMENTS_SECTION));
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
