// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Centurion-only startup and presentation choices, independent of campaign state.

/// Original sound arrangement selected at launch, not the host output device.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CenturionSoundDevice {
    #[default]
    Adlib,
    PcSpeaker,
}
impl CenturionSoundDevice {
    pub const ALL: [Self; 2] = [Self::Adlib, Self::PcSpeaker];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Adlib => "adlib",
            Self::PcSpeaker => "pc-speaker",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Adlib => "AdLib",
            Self::PcSpeaker => "PC speaker",
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        value.parse()
    }
}
impl std::str::FromStr for CenturionSoundDevice {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "adlib" => Ok(Self::Adlib),
            "pc-speaker" => Ok(Self::PcSpeaker),
            _ => Err("Sound must be adlib or pc-speaker".into()),
        }
    }
}

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
    pub sound: CenturionSoundDevice,
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
        CENTURION_SOUND as SOUND, Draft, ENHANCEMENTS_SECTION, Launch, Patch, RuntimeSettings,
        Session,
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
    fn sound_device_validates_and_defaults_to_adlib() {
        let mut values = RuntimeSettings::default();
        assert_eq!(values.centurion.sound, CenturionSoundDevice::Adlib);
        for choice in CenturionSoundDevice::ALL {
            assert_eq!(
                CenturionSoundDevice::parse(choice.as_str()).unwrap(),
                choice
            );
            values.set(SOUND, choice.as_str()).unwrap();
            assert_eq!(values.centurion.sound, choice);
            assert_eq!(values.get(SOUND), choice.as_str());
        }
        for invalid in ["", "default", "speaker", "roland", "AdLib", "true"] {
            assert!(values.set(SOUND, invalid).is_err());
            assert_eq!(values.centurion.sound, CenturionSoundDevice::PcSpeaker);
        }
        assert_eq!(crate::section_index(SOUND), 2);
        assert!(!crate::AUDIO_KEYS.contains(&SOUND));
    }

    #[test]
    fn sound_device_shares_launcher_storage_and_resets_only_with_audio() {
        let temp = Temp::new();
        crate::launcher::save_native_option(&temp.path(), "copy", SOUND, Some("pc-speaker"))
            .unwrap();
        crate::launcher::save_native_option(&temp.path(), "copy", "future_option", Some("keep"))
            .unwrap();
        crate::launcher::save_native_option(&temp.path(), "other-copy", SOUND, Some("pc-speaker"))
            .unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        assert_eq!(
            session.saved.game_values().centurion.sound,
            CenturionSoundDevice::PcSpeaker
        );
        assert!(session.saved.has_game_overrides(2));
        assert!(!session.saved.has_game_overrides(ENHANCEMENTS_SECTION));
        let mut draft = session.saved.clone();
        draft.edit_game(SOUND, "adlib".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        assert_eq!(
            temp.session("centurion-defender-of-rome")
                .saved
                .game_values()
                .centurion
                .sound,
            CenturionSoundDevice::Adlib
        );
        assert!(
            !crate::launcher::native_options(&temp.path(), "copy")
                .unwrap()
                .contains_key(SOUND)
        );
        let mut draft = session.saved.clone();
        draft.edit_game(SOUND, "pc-speaker".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        assert_eq!(
            crate::launcher::native_options(&temp.path(), "copy").unwrap()[SOUND],
            "pc-speaker"
        );
        let mut draft = session.saved.clone();
        draft.reset_section(true, ENHANCEMENTS_SECTION);
        draft.reset_section(false, 2);
        assert_eq!(
            draft.game_values().centurion.sound,
            CenturionSoundDevice::PcSpeaker
        );
        draft.reset_section(true, 2);
        assert!(!draft.has_game_overrides(2));
        assert_eq!(
            draft.game_values().centurion.sound,
            CenturionSoundDevice::Adlib
        );
        session.commit(draft).unwrap();
        let native = crate::launcher::native_options(&temp.path(), "copy").unwrap();
        assert!(!native.contains_key(SOUND));
        assert_eq!(native["future_option"], "keep");
        assert_eq!(
            crate::launcher::native_options(&temp.path(), "other-copy").unwrap()[SOUND],
            "pc-speaker"
        );
    }

    #[test]
    fn sound_device_never_inherits_global_or_other_game_values() {
        let temp = Temp::new();
        fs::write(temp.path(), "[global.launcher]\nsound = 'pc-speaker'\n[entries.copy.launcher]\nsound = 'pc-speaker'\n").unwrap();
        for game in [
            "centurion-defender-of-rome",
            "mach-3",
            "alley-cat",
            "mindwheel",
        ] {
            let mut session = temp.session(game);
            assert_eq!(
                session.saved.global_values().centurion.sound,
                CenturionSoundDevice::Adlib
            );
            let mut draft = session.saved.clone();
            draft.edit(false, SOUND, "pc-speaker".into());
            assert!(!draft.global.contains_key(SOUND));
            if game != "centurion-defender-of-rome" {
                session.cli.insert(SOUND.into(), "pc-speaker".into());
                assert!(!session.saved.game.contains_key(SOUND));
                assert_eq!(
                    session.effective(&session.saved).centurion.sound,
                    CenturionSoundDevice::Adlib
                );
                draft.edit(true, SOUND, "adlib".into());
                session.commit(draft).unwrap();
            }
        }
        let document = fs::read_to_string(temp.path())
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            document["global"]["launcher"][SOUND].as_str(),
            Some("pc-speaker")
        );
        assert_eq!(
            document["entries"]["copy"]["launcher"][SOUND].as_str(),
            Some("pc-speaker")
        );
    }

    #[test]
    fn sound_cli_is_temporary_and_unrelated_saves_keep_concurrent_launcher_edits() {
        let temp = Temp::new();
        crate::launcher::save_native_option(&temp.path(), "copy", SOUND, Some("adlib")).unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        session.cli.insert(SOUND.into(), "pc-speaker".into());
        assert_eq!(
            session.effective(&session.saved).centurion.sound,
            CenturionSoundDevice::PcSpeaker
        );
        assert_eq!(
            session.saved.game_values().centurion.sound,
            CenturionSoundDevice::Adlib
        );
        let mut draft = session.saved.clone();
        draft.edit(true, "master_volume", "70".into());
        session.commit(draft).unwrap();
        assert_eq!(
            crate::launcher::native_options(&temp.path(), "copy").unwrap()[SOUND],
            "adlib"
        );
        // A pending unrelated settings edit must not replace a newer native pin.
        crate::launcher::save_native_option(&temp.path(), "copy", SOUND, Some("pc-speaker"))
            .unwrap();
        let mut draft = session.saved.clone();
        draft.edit(true, "vsync", "false".into());
        session.commit(draft).unwrap();
        assert_eq!(
            temp.session("centurion-defender-of-rome")
                .saved
                .game_values()
                .centurion
                .sound,
            CenturionSoundDevice::PcSpeaker
        );
        assert_eq!(
            crate::launcher::native_options(&temp.path(), "copy").unwrap()[SOUND],
            "pc-speaker"
        );
    }

    #[test]
    fn invalid_sound_values_and_malformed_launcher_sections_remain_intact() {
        let temp = Temp::new();
        for invalid in ["unknown", "default", "speaker"] {
            fs::write(
                temp.path(),
                format!(
                    "[entries.copy.launcher]\nsound = '{invalid}' # retain\nfuture_flag = true\n"
                ),
            )
            .unwrap();
            let mut session = temp.session("centurion-defender-of-rome");
            assert!(session.warning.is_some());
            assert!(!session.saved.game.contains_key(SOUND));
            assert_eq!(
                session.saved.game_values().centurion.sound,
                CenturionSoundDevice::Adlib
            );
            let mut draft = session.saved.clone();
            draft.edit(true, "vsync", "false".into());
            session.commit(draft).unwrap();
            let text = fs::read_to_string(temp.path()).unwrap();
            assert!(text.contains(&format!("sound = '{invalid}' # retain")));
            assert!(text.contains("future_flag = true"));
        }
        let original = "[entries.copy]\nlauncher = 'broken'\n";
        fs::write(temp.path(), original).unwrap();
        let mut session = temp.session("centurion-defender-of-rome");
        assert!(session.warning.is_some());
        let mut draft = session.saved.clone();
        draft.edit(false, "vsync", "false".into());
        draft.edit(true, SOUND, "pc-speaker".into());
        assert!(session.commit(draft).is_err());
        assert_eq!(fs::read_to_string(temp.path()).unwrap(), original);
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
                session.saved.global_values().centurion.map_hover_info,
                MapHoverInfo::Off
            );
            assert_eq!(
                session.effective(&session.saved).centurion.map_hover_info,
                choice
            );
            let mut draft = session.saved.clone();
            draft.edit(false, HOVER, "on".into());
            assert!(!draft.global.contains_key(HOVER));
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
            assert!(reset.has_game_overrides(ENHANCEMENTS_SECTION));
            reset.reset_section(true, ENHANCEMENTS_SECTION);
            assert!(!reset.has_game_overrides(ENHANCEMENTS_SECTION));
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
                    .saved
                    .global_values()
                    .centurion
                    .applied_value_indicator,
                AppliedValueIndicator::SubduedBand
            );
            assert_eq!(
                session
                    .effective(&session.saved)
                    .centurion
                    .applied_value_indicator,
                choice
            );
            let mut draft = session.saved.clone();
            draft.edit(false, INDICATOR, "original".into());
            assert!(!draft.global.contains_key(INDICATOR));
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
            assert!(reset.has_game_overrides(ENHANCEMENTS_SECTION));
            reset.reset_section(true, ENHANCEMENTS_SECTION);
            assert!(!reset.has_game_overrides(ENHANCEMENTS_SECTION));
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
            session.saved.global_values().centurion.map_highlight,
            MapHighlight::On
        );
        assert_eq!(
            session.effective(&session.saved).centurion.map_highlight,
            MapHighlight::Off
        );
        let mut draft = session.saved.clone();
        draft.edit(false, MAP, "off".into());
        assert!(!draft.global.contains_key(MAP));
        draft.edit_game(CURSOR, "hidden".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        assert!(
            fs::read_to_string(temp.path())
                .unwrap()
                .contains("map_highlight = 'off' # retain choice")
        );
        assert!(session.saved.has_game_overrides(ENHANCEMENTS_SECTION));
        let mut draft = session.saved.clone();
        draft.reset_section(true, ENHANCEMENTS_SECTION);
        assert_eq!(
            draft.game_values().centurion.map_highlight,
            MapHighlight::On
        );
        assert!(!draft.has_game_overrides(ENHANCEMENTS_SECTION));
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
