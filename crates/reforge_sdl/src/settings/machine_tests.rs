// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::launcher::{Choice, OptionDefinition, Storage};
use reforge_settings::{ALLEYCAT_MACHINE, Launch, Patch};

const CGA: Choice = Choice {
    value: "cga",
    label: "IBM CGA",
    short_label: "CGA",
    badge: None,
    palette: None,
};
const PCJR: Choice = Choice {
    value: "pcjr",
    label: "IBM PCjr",
    short_label: "PCjr",
    badge: None,
    palette: None,
};
const TANDY: Choice = Choice {
    value: "tandy",
    label: "Tandy",
    short_label: "Tandy",
    badge: None,
    palette: None,
};
const LEGACY: OptionDefinition = OptionDefinition {
    label: "Machine",
    choices: &[CGA, PCJR],
    default: "cga",
    storage: Storage::Native {
        key: "machine",
        argument: "--machine",
    },
};
const RESTORED: OptionDefinition = OptionDefinition {
    choices: &[CGA, PCJR, TANDY],
    default: "tandy",
    ..LEGACY
};

// Keep ordinary workspace tests independent of a desktop and of SDL state in
// other test threads, matching the shared presentation regression convention.
fn dummy_video(test: &str) -> bool {
    const MARKER: &str = "REFORGE_MACHINE_SETTINGS_TEST";
    if std::env::var(MARKER).as_deref() == Ok(test) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--test-threads=1"])
        .env(MARKER, test)
        .env("SDL_VIDEO_DRIVER", "dummy")
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_RENDER_DRIVER", "software")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn draw(menu: &mut SettingsMenu<'_>, events: Vec<egui::Event>) -> egui::FullOutput {
    let context = menu.context.clone();
    let now = menu.started.elapsed().as_secs_f64();
    context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(1100.0, 1000.0),
            )),
            events,
            time: Some(now),
            ..Default::default()
        },
        |context| {
            crate::settings_ui::draw(menu, context, context.screen_rect());
        },
    )
}
fn label(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::epaint::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::epaint::Shape::Text(shape) if shape.galley.text() == text => {
                Some(shape.visual_bounding_rect())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, text))
}
fn click(menu: &mut SettingsMenu<'_>, text: &str) {
    draw(menu, vec![]);
    let pos = label(&draw(menu, vec![]), text)
        .unwrap_or_else(|| panic!("missing {text}"))
        .center();
    for pressed in [true, false] {
        draw(
            menu,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn machine_choices_share_launcher_storage_and_apply_cancel_reset() {
    if !dummy_video(
        "settings::machine_tests::machine_choices_share_launcher_storage_and_apply_cancel_reset",
    ) {
        return;
    }
    for machines in [&LEGACY, &RESTORED] {
        let path = std::env::temp_dir().join(format!(
            "reforge-machine-ui-{}-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            machines.default
        ));
        assert!(!path.exists());
        let _guard = Launch {
            game_id: "alley-cat".into(),
            entry_id: Some("copy".into()),
            path: Some(path.clone()),
            cli: Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ]),
        }
        .install();
        let mut display = crate::Window::new("machine transaction", 320, 240, Some(3)).unwrap();
        let creator = display.texture_creator();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
            .with_alleycat_options()
            .with_alleycat_machines(machines);
        menu.toggle();
        menu.prepare(display.ui_metrics(), display.settings());
        assert!(
            label(&draw(&mut menu, vec![]), "Machine").is_none(),
            "global defaults must not select machines"
        );
        menu.scope = 1;
        menu.game_editing[0] = true;
        let selected = if machines.default == "tandy" {
            "Tandy"
        } else {
            "IBM CGA"
        };
        click(&mut menu, selected);
        assert_eq!(
            label(&draw(&mut menu, vec![]), "Tandy").is_some(),
            machines.default == "tandy"
        );
        click(&mut menu, "IBM PCjr");
        assert_eq!(menu.draft.as_ref().unwrap().game[ALLEYCAT_MACHINE], "pcjr");
        assert!(!path.exists(), "draft must not save before Apply");
        menu.toggle(); // Cancel/close discards this draft.
        menu.toggle();
        menu.prepare(display.ui_metrics(), display.settings());
        menu.scope = 1;
        menu.game_editing[0] = true;
        assert!(
            !menu
                .draft
                .as_ref()
                .unwrap()
                .game
                .contains_key(ALLEYCAT_MACHINE)
        );
        click(&mut menu, selected);
        click(&mut menu, "IBM PCjr");
        click(&mut menu, "Apply");
        let request = creator
            .session
            .borrow()
            .effective(menu.pending.as_ref().unwrap());
        display.apply_settings(request).unwrap();
        menu.applied();
        assert_eq!(
            reforge_settings::launcher::native_options(&path, "copy").unwrap()[ALLEYCAT_MACHINE],
            "pcjr"
        );
        click(&mut menu, "Restore defaults");
        click(&mut menu, "Apply");
        let request = creator
            .session
            .borrow()
            .effective(menu.pending.as_ref().unwrap());
        display.apply_settings(request).unwrap();
        menu.applied();
        assert!(
            !reforge_settings::launcher::native_options(&path, "copy")
                .unwrap()
                .contains_key(ALLEYCAT_MACHINE)
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn machine_round_trip_restores_saved_override_presence() {
    if !dummy_video("settings::machine_tests::machine_round_trip_restores_saved_override_presence")
    {
        return;
    }
    for machines in [&LEGACY, &RESTORED] {
        for pin in [None, Some(machines.default), Some("pcjr")] {
            let path = std::env::temp_dir().join(format!(
                "reforge-machine-round-trip-{}-{}.toml",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            ));
            if let Some(pin) = pin {
                reforge_settings::launcher::save_native_option(
                    &path,
                    "copy",
                    ALLEYCAT_MACHINE,
                    Some(pin),
                )
                .unwrap();
            }
            let before = std::fs::read(&path).ok();
            let _guard = Launch {
                game_id: "alley-cat".into(),
                entry_id: Some("copy".into()),
                path: Some(path.clone()),
                cli: Patch::from([
                    ("renderer".into(), "software".into()),
                    ("vsync".into(), "false".into()),
                ]),
            }
            .install();
            let display = crate::Window::new("machine round trip", 320, 240, Some(3)).unwrap();
            let creator = display.texture_creator();
            let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
                .with_alleycat_options()
                .with_alleycat_machines(machines);
            menu.toggle();
            menu.prepare(display.ui_metrics(), display.settings());
            menu.scope = 1;
            menu.game_editing[0] = true;
            let initial = pin.unwrap_or(machines.default);
            let away = if initial == "pcjr" {
                machines.default
            } else {
                "pcjr"
            };
            let title = |value| {
                machines
                    .choices
                    .iter()
                    .find(|choice| choice.value == value)
                    .unwrap()
                    .label
            };
            click(&mut menu, title(initial));
            click(&mut menu, title(away));
            assert_ne!(
                menu.draft.as_ref().unwrap(),
                &creator.session.borrow().saved
            );
            assert!(label(&draw(&mut menu, vec![]), "Unapplied changes").is_some());
            click(&mut menu, title(away));
            click(&mut menu, title(initial));
            assert_eq!(
                menu.draft.as_ref().unwrap(),
                &creator.session.borrow().saved,
                "round trip must restore saved presence: default={}, pin={pin:?}",
                machines.default
            );
            assert!(label(&draw(&mut menu, vec![]), "No pending changes").is_some());
            assert_eq!(std::fs::read(&path).ok(), before);
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }
    }
}

#[test]
fn machine_selector_locks_cli_overrides_and_disables_unsavable_runs() {
    if !dummy_video(
        "settings::machine_tests::machine_selector_locks_cli_overrides_and_disables_unsavable_runs",
    ) {
        return;
    }
    for (registered, explicit) in [(true, true), (false, false)] {
        let path = std::env::temp_dir().join(format!(
            "reforge-machine-locked-{}-{}-{registered}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!path.exists());
        let mut cli = Patch::from([
            ("renderer".into(), "software".into()),
            ("vsync".into(), "false".into()),
        ]);
        if explicit {
            cli.insert(ALLEYCAT_MACHINE.into(), "pcjr".into());
        }
        let _guard = Launch {
            game_id: "alley-cat".into(),
            entry_id: registered.then(|| "copy".into()),
            path: Some(path.clone()),
            cli,
        }
        .install();
        let display = crate::Window::new("machine availability", 320, 240, Some(3)).unwrap();
        let creator = display.texture_creator();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
            .with_alleycat_options()
            .with_alleycat_machines(&RESTORED);
        menu.toggle();
        menu.prepare(display.ui_metrics(), display.settings());
        menu.scope = 1;
        menu.game_editing[0] = true;
        click(&mut menu, if explicit { "IBM PCjr" } else { "Tandy" });
        let output = draw(&mut menu, vec![]);
        assert!(
            label(&output, "IBM CGA").is_none(),
            "locked selector must not open"
        );
        assert!(menu.draft.as_ref().unwrap().game.is_empty());
        assert_eq!(
            label(
                &output,
                "Machine requires saved game settings; it applies on next launch."
            )
            .is_some(),
            !registered
        );
        assert!(!path.exists());
    }
}
