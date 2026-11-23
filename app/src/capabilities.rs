// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bind detected release profiles to engine-owned settings declarations.

use crate::detection::{Engine, Release};
use reforge_sdl::settings::SettingsCapabilities;
use std::collections::BTreeMap;

pub(crate) fn settings(
    release: &Release,
    native: &BTreeMap<String, String>,
) -> SettingsCapabilities {
    let mut capabilities = match release.engine() {
        Engine::AlleyCat(format) => alleycat::launcher::settings_capabilities(format, native),
        Engine::Mach3 => mach3::launcher::settings_capabilities(native),
        Engine::Centurion(profile) => centurion::launcher::settings_capabilities(profile, native),
        Engine::Btz(_) => btz::launcher::settings_capabilities(),
    };
    // A shared engine can provide several games. The catalog supplies the
    // settings scope; engine support remains independent of that identity.
    capabilities.game_id = release.game().id();
    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{Game, RELEASES};

    #[test]
    fn catalog_profiles_declare_consistent_release_capabilities() {
        for release in RELEASES {
            let capabilities = settings(release, &BTreeMap::new());
            assert_eq!(capabilities.validate(), Ok(()), "{}", release.id);
            assert_eq!(capabilities.game_id, release.game().id());
            match release.engine() {
                Engine::AlleyCat(format) => {
                    let machine = capabilities.display.machine.unwrap();
                    assert_eq!(
                        machine.default,
                        match format {
                            alleycat::assets::InputFormat::Tandy
                            | alleycat::assets::InputFormat::CatWithPatch => "pcjr",
                            _ => "cga",
                        },
                        "{}",
                        release.id
                    );
                }
                Engine::Centurion(profile) => {
                    assert_eq!(
                        capabilities.display.mode.is_some(),
                        profile.ega,
                        "{}",
                        release.id
                    );
                }
                Engine::Btz(_) => {
                    assert!(capabilities.display.frontend.is_some());
                }
                Engine::Mach3 => {}
            }
        }
    }

    #[test]
    fn selected_native_mode_controls_the_geometry_used_by_settings() {
        let release = RELEASES
            .iter()
            .find(|release| release.game() == Game::Mach3)
            .unwrap();
        let cga = settings(release, &BTreeMap::new());
        let mono = settings(release, &BTreeMap::from([("video".into(), "mono".into())]));
        assert_eq!(
            cga.display.geometry,
            reforge_sdl::DisplayGeometry::new((320, 200), (320, 240))
        );
        assert_eq!(
            mono.display.geometry,
            reforge_sdl::DisplayGeometry::new((640, 200), (320, 240))
        );
    }
}
