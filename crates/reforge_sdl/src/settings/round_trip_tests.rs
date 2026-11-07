// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::machine_tests::{click, draw, dummy_video, label};
use super::*;
use reforge_settings::{Backend, Launch, Patch};

fn with_menu(contents: &str, scope: usize, test: impl FnOnce(&mut SettingsMenu<'_>)) {
    let path = std::env::temp_dir().join(format!(
        "reforge-settings-round-trip-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, contents).unwrap();
    let before = std::fs::read(&path).unwrap();
    let _guard = Launch {
        game_id: "alley-cat".into(),
        entry_id: Some("copy".into()),
        path: Some(path.clone()),
        cli: Patch::from([
            ("renderer".into(), "software".into()),
            ("scale".into(), "4".into()),
            ("vsync".into(), "false".into()),
        ]),
    }
    .install();
    let display = crate::Window::new("settings round trip", 320, 240, Some(4)).unwrap();
    let creator = display.texture_creator();
    // Start the real window without GPU/desktop requirements. UI edits below
    // remain unlocked and can differ from the active window's scale/renderer.
    creator.session.borrow_mut().cli.remove("renderer");
    creator.session.borrow_mut().cli.remove("scale");
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    menu.scope = scope;
    test(&mut menu);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "edits must wait for Apply"
    );
    std::fs::remove_file(path).unwrap();
}

fn values(menu: &SettingsMenu<'_>) -> RuntimeSettings {
    let draft = menu.draft.as_ref().unwrap();
    if menu.scope == 0 {
        draft.global_values()
    } else {
        draft.game_values()
    }
}

#[test]
fn renderer_round_trips_restore_each_scopes_backend() {
    if !dummy_video("settings::round_trip_tests::renderer_round_trips_restore_each_scopes_backend")
    {
        return;
    }
    for scope in [0, 1] {
        let contents = if scope == 0 {
            "[global.display]\nrenderer='gpu'\nbackend='vulkan'\n"
        } else {
            "[global.display]\nrenderer='software'\n[entries.copy.display]\nrenderer='gpu'\nbackend='vulkan'\n"
        };
        with_menu(contents, scope, |menu| {
            let saved = menu.draft.clone();
            click(menu, "GPU");
            click(menu, "SDL renderer");
            assert_eq!(values(menu).backend, Backend::Auto);
            click(menu, "SDL renderer");
            click(menu, "GPU");
            assert_eq!(values(menu).backend, Backend::Vulkan);
            assert_eq!(menu.draft, saved);
            assert!(label(&draw(menu, vec![]), "No pending changes").is_some());

            click(menu, "GPU");
            click(menu, "SDL renderer");
            click(menu, "Automatic");
            click(menu, "OpenGL");
            click(menu, "SDL renderer");
            click(menu, "GPU");
            assert_eq!(values(menu).backend, Backend::Vulkan);
            click(menu, "GPU");
            click(menu, "SDL renderer");
            assert_eq!(values(menu).backend, Backend::OpenGl);

            click(
                menu,
                if scope == 0 {
                    "Reset section"
                } else {
                    "Restore global defaults"
                },
            );
            if scope == 1 {
                menu.game_editing[0] = true;
            }
            let current = if scope == 0 {
                "Automatic · GPU preferred"
            } else {
                "Software"
            };
            click(menu, current);
            click(menu, "GPU");
            assert_eq!(
                values(menu).backend,
                Backend::Auto,
                "reset must clear dormant backend"
            );
        });
    }
    // Returning to an inherited backend must remove the temporary game pin.
    with_menu(
        "[global.display]\nrenderer='gpu'\nbackend='vulkan'\n[entries.copy.display]\nrenderer='gpu'\n",
        1,
        |menu| {
            let saved = menu.draft.clone();
            click(menu, "GPU");
            click(menu, "SDL renderer");
            click(menu, "SDL renderer");
            click(menu, "GPU");
            assert_eq!(menu.draft, saved);
            assert!(!menu.draft.as_ref().unwrap().game.contains_key("backend"));
        },
    );
}

#[test]
fn auto_scale_round_trips_restore_manual_values_in_both_scopes() {
    if !dummy_video(
        "settings::round_trip_tests::auto_scale_round_trips_restore_manual_values_in_both_scopes",
    ) {
        return;
    }
    for scope in [0, 1] {
        with_menu(
            "[global.display]\nscale=2\n[entries.copy.display]\nscale=3\n",
            scope,
            |menu| {
                let saved = menu.draft.clone();
                let manual = values(menu).scale;
                assert_ne!(manual, Some(4));
                click(menu, "Auto");
                assert_eq!(values(menu).scale, None);
                // Visit the other scope while Auto is selected: its manual value
                // must not overwrite the remembered value of the edited scope.
                menu.scope = 1 - scope;
                draw(menu, vec![]);
                menu.scope = scope;
                click(menu, "Auto");
                assert_eq!(values(menu).scale, manual);
                assert_eq!(menu.draft, saved);
                assert!(label(&draw(menu, vec![]), "No pending changes").is_some());
            },
        );
    }
}

#[test]
fn explicit_global_defaults_round_trip_without_pending_changes() {
    if !dummy_video(
        "settings::round_trip_tests::explicit_global_defaults_round_trip_without_pending_changes",
    ) {
        return;
    }
    with_menu("[global.display]\nwindow_mode='windowed'\n", 0, |menu| {
        let saved = menu.draft.clone();
        click(menu, "Fullscreen");
        click(menu, "Windowed");
        assert_eq!(menu.draft, saved);
        assert!(label(&draw(menu, vec![]), "No pending changes").is_some());
        click(menu, "Reset section");
        click(menu, "Fullscreen");
        click(menu, "Windowed");
        assert!(
            !menu
                .draft
                .as_ref()
                .unwrap()
                .global
                .contains_key("window_mode")
        );
        assert!(label(&draw(menu, vec![]), "Unapplied changes").is_some());
    });
}

#[test]
fn dormant_display_choices_are_cleared_on_reset_cancel_and_apply() {
    if !dummy_video(
        "settings::round_trip_tests::dormant_display_choices_are_cleared_on_reset_cancel_and_apply",
    ) {
        return;
    }
    with_menu("", 0, |menu| {
        for finish in ["reset", "cancel", "apply"] {
            menu.draft
                .as_mut()
                .unwrap()
                .edit(false, "scale", "2".into());
            draw(menu, vec![]);
            click(menu, "Auto");
            assert_eq!(values(menu).scale, None);
            match finish {
                "reset" => click(menu, "Reset section"),
                "cancel" => {
                    menu.toggle();
                    menu.toggle();
                    menu.prepare(
                        UiMetrics {
                            drawable: (1100, 1000),
                            window: (1100, 1000),
                            pixels_per_point: 1.0,
                        },
                        RuntimeSettings::default(),
                    );
                }
                "apply" => {
                    // The host accepts this in-memory session's Apply. Saving
                    // is already covered by Settings persistence tests.
                    menu.creator.session.borrow_mut().path = None;
                    menu.pending = menu.draft.clone();
                    menu.applied();
                }
                _ => unreachable!(),
            }
            click(menu, "Auto");
            assert_eq!(
                values(menu).scale,
                Some(4),
                "{finish} must forget old manual scale"
            );
            click(menu, "Auto");
        }
    });
}
