// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Granular Centurion choices and their preset aliases share one draft value.
use super::machine_tests::{click, draw, dummy_video, label};
use super::*;
use reforge_settings::{
    AppliedValueIndicator, CENTURION_APPLIED_VALUE_INDICATOR as INDICATOR,
    CENTURION_FLEET_INFORMATION as KEY, CENTURION_MAP_HIGHLIGHT as MAP,
    CENTURION_MAP_HOVER_INFO as HOVER, CENTURION_MENU_DECORATIONS as DECORATIONS,
    CENTURION_POPUP_CURSOR as CURSOR, ENHANCEMENTS_SECTION, FleetInformation, Launch, MapHighlight,
    MapHoverInfo, MenuDecorations, Patch, PopupCursor,
};

fn click_last(menu: &mut SettingsMenu<'_>, text: &str) {
    fn labels(shape: &egui::Shape, text: &str, result: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                result.push(shape.visual_bounding_rect());
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    labels(shape, text, result);
                }
            }
            _ => {}
        }
    }
    draw(menu, vec![]);
    let output = draw(menu, vec![]);
    let mut found = Vec::new();
    for shape in &output.shapes {
        labels(&shape.shape, text, &mut found);
    }
    let position = found
        .last()
        .unwrap_or_else(|| panic!("missing {text}"))
        .center();
    for pressed in [true, false] {
        draw(
            menu,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn click_value_row(menu: &mut SettingsMenu<'_>, row: &str, value: &str) {
    fn labels(shape: &egui::Shape, text: &str, result: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Text(shape) if shape.galley.text() == text => {
                result.push(shape.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    labels(shape, text, result);
                }
            }
            _ => {}
        }
    }
    draw(menu, vec![]);
    let output = draw(menu, vec![]);
    let row_y = label(&output, row)
        .unwrap_or_else(|| panic!("missing row {row}"))
        .center()
        .y;
    let mut found = Vec::new();
    for shape in &output.shapes {
        labels(&shape.shape, value, &mut found);
    }
    let position = found
        .iter()
        .min_by(|a, b| {
            (a.center().y - row_y)
                .abs()
                .total_cmp(&(b.center().y - row_y).abs())
        })
        .unwrap_or_else(|| panic!("missing value {value} for {row}"))
        .center();
    for pressed in [true, false] {
        draw(
            menu,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn centurion_presets_alias_granular_choice_and_apply_cancel_reset() {
    if !dummy_video(
        "settings::centurion_tests::centurion_presets_alias_granular_choice_and_apply_cancel_reset",
    ) {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "reforge-centurion-ui-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &path,
        "[entries.copy.audio]\nmaster_volume = 61\naudio_renderer = 'speaker'\n",
    )
    .unwrap();
    let initial = std::fs::read(&path).unwrap();
    let _guard = Launch {
        game_id: "centurion-defender-of-rome".into(),
        entry_id: Some("copy".into()),
        path: Some(path.clone()),
        cli: Patch::from([
            ("renderer".into(), "software".into()),
            ("vsync".into(), "false".into()),
        ]),
    }
    .install();
    let mut display =
        crate::Window::new("Centurion Settings transaction", 320, 240, Some(3)).unwrap();
    let creator = display.texture_creator();
    let mut menu =
        SettingsMenu::new(&creator, VideoCapabilities::default()).with_centurion_options();
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    assert!(
        label(&draw(&mut menu, vec![]), "UI preset").is_none(),
        "game option must not appear in Global defaults"
    );
    menu.scope = 1;
    assert!(
        label(&draw(&mut menu, vec![]), "UI preset").is_none(),
        "Centurion enhancements must not remain in Display"
    );
    menu.scope = 0;
    click(&mut menu, "Enhancements");
    assert_eq!(menu.page, ENHANCEMENTS_SECTION);
    assert!(label(&draw(&mut menu, vec![]), "UI preset").is_none());
    menu.scope = 1;
    let output = draw(&mut menu, vec![]);
    assert!(label(&output, "UI preset").is_some());
    assert!(label(&output, "Reforged").is_some());
    assert_eq!(
        menu.draft.as_ref().unwrap().game_values().audio.renderer,
        reforge_settings::SoundRenderer::Speaker
    );
    assert!(
        !menu.game_editing[ENHANCEMENTS_SECTION],
        "enhancements work without enabling global overrides"
    );
    assert!(
        label(
            &output,
            "Override global defaults · only differences are saved"
        )
        .is_none()
    );
    assert!(
        label(&output, "Fleet information").is_none(),
        "details start collapsed"
    );
    click(&mut menu, "Reforged");
    click(&mut menu, "Original");
    assert_eq!(menu.draft.as_ref().unwrap().game[KEY], "original");
    assert_eq!(menu.draft.as_ref().unwrap().game[CURSOR], "hidden");
    assert_eq!(menu.draft.as_ref().unwrap().game[MAP], "off");
    assert_eq!(menu.draft.as_ref().unwrap().game[INDICATOR], "original");
    assert_eq!(std::fs::read(&path).unwrap(), initial);
    click(&mut menu, "Advanced options");
    assert!(label(&draw(&mut menu, vec![]), "Fleet information").is_some());
    assert!(label(&draw(&mut menu, vec![]), "Menu decorations").is_some());
    assert!(label(&draw(&mut menu, vec![]), "Map highlight").is_some());
    assert!(label(&draw(&mut menu, vec![]), "Map hover info").is_some());
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .map_hover_info,
        MapHoverInfo::Off
    );
    click_value_row(&mut menu, "Map hover info", "Off");
    click_last(&mut menu, "On");
    assert_eq!(menu.draft.as_ref().unwrap().game[HOVER], "on");
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    click(&mut menu, "Custom");
    click_last(&mut menu, "Original");
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .map_hover_info,
        MapHoverInfo::Off
    );
    assert!(label(&draw(&mut menu, vec![]), "Applied value indicator").is_some());
    for choice in [
        AppliedValueIndicator::SubduedBand,
        AppliedValueIndicator::SideMarker,
        AppliedValueIndicator::Underline,
    ] {
        let current = menu
            .draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .applied_value_indicator;
        click_value_row(&mut menu, "Applied value indicator", current.label());
        click_last(&mut menu, choice.label());
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .centurion
                .applied_value_indicator,
            choice
        );
        assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    }
    click(&mut menu, "Custom");
    click_last(&mut menu, "Original");
    click(&mut menu, "Off");
    click(&mut menu, "On");
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(MAP));
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    click(&mut menu, "Custom");
    click_last(&mut menu, "Original");
    assert_eq!(menu.draft.as_ref().unwrap().game[MAP], "off");
    assert_eq!(menu.draft.as_ref().unwrap().game[INDICATOR], "original");
    click(&mut menu, "Mouse");
    click(&mut menu, "Keyboard");
    assert_eq!(menu.draft.as_ref().unwrap().game[DECORATIONS], "keyboard");
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    click(&mut menu, "Custom");
    click_last(&mut menu, "Original");
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(DECORATIONS));
    click_value_row(&mut menu, "Fleet information", "Original");
    click(&mut menu, "Detailed");
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    assert_eq!(menu.draft.as_ref().unwrap().game[KEY], "detailed");
    click(&mut menu, "Custom");
    click(&mut menu, "Reforged");
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(KEY));
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(CURSOR));
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(MAP));
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(INDICATOR));
    assert!(label(&draw(&mut menu, vec![]), "Subdued band").is_some());
    assert!(label(&draw(&mut menu, vec![]), "Both").is_some());
    assert!(label(&draw(&mut menu, vec![]), "No pending changes").is_some());
    click(&mut menu, "Visible");
    click(&mut menu, "Hidden");
    assert_eq!(menu.draft.as_ref().unwrap().game[CURSOR], "hidden");
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(KEY));
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    click(&mut menu, "Custom");
    click(&mut menu, "Original");
    assert_eq!(menu.draft.as_ref().unwrap().game[KEY], "original");
    assert_eq!(menu.draft.as_ref().unwrap().game[CURSOR], "hidden");
    click(&mut menu, "Hidden");
    click(&mut menu, "Visible");
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    assert!(!menu.draft.as_ref().unwrap().game.contains_key(CURSOR));
    click(&mut menu, "Custom");
    click(&mut menu, "Reforged");
    click(&mut menu, "Both");
    click_last(&mut menu, "Original");
    assert_eq!(menu.draft.as_ref().unwrap().game[KEY], "original");
    click(&mut menu, "Mouse");
    click(&mut menu, "Dynamic");
    assert_eq!(menu.draft.as_ref().unwrap().game[DECORATIONS], "dynamic");
    click_value_row(&mut menu, "Map hover info", "Off");
    click_last(&mut menu, "On");
    assert_eq!(menu.draft.as_ref().unwrap().game[HOVER], "on");
    menu.back(); // Escape/controller Back discards the same pending draft as Cancel.
    assert!(!menu.is_open());
    assert_eq!(std::fs::read(&path).unwrap(), initial);
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    menu.scope = 1;
    menu.page = ENHANCEMENTS_SECTION;
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .fleet_information,
        FleetInformation::Both
    );
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .menu_decorations,
        MenuDecorations::Mouse
    );
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .applied_value_indicator,
        AppliedValueIndicator::SubduedBand
    );
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .map_hover_info,
        MapHoverInfo::Off
    );
    click(&mut menu, "Reforged");
    click(&mut menu, "Original");
    click(&mut menu, "Apply");
    let request = creator
        .session
        .borrow()
        .effective(menu.pending.as_ref().unwrap());
    display.apply_settings(request).unwrap();
    menu.applied();
    assert_eq!(
        display.settings().centurion.fleet_information,
        FleetInformation::Original
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("fleet_information = \"original\""));
    assert!(text.contains("popup_cursor = \"hidden\""));
    assert!(text.contains("map_highlight = \"off\""));
    assert!(text.contains("applied_value_indicator = \"original\""));
    assert!(!text.contains("preset"));
    assert!(text.contains("master_volume = 61"));
    click(&mut menu, "Hidden");
    click(&mut menu, "Visible");
    click_value_row(&mut menu, "Fleet information", "Original");
    click(&mut menu, "Detailed");
    click(&mut menu, "Mouse");
    click(&mut menu, "Dynamic");
    click_value_row(&mut menu, "Applied value indicator", "Original");
    click(&mut menu, "Side marker");
    assert_eq!(menu.draft.as_ref().unwrap().game[INDICATOR], "side-marker");
    click_value_row(&mut menu, "Map hover info", "Off");
    click_last(&mut menu, "On");
    assert_eq!(menu.draft.as_ref().unwrap().game[HOVER], "on");
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    click(&mut menu, "Apply");
    let request = creator
        .session
        .borrow()
        .effective(menu.pending.as_ref().unwrap());
    display.apply_settings(request).unwrap();
    menu.applied();
    assert_eq!(
        display.settings().centurion.fleet_information,
        FleetInformation::Detailed
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("fleet_information = \"detailed\"")
    );
    assert!(label(&draw(&mut menu, vec![]), "Custom").is_some());
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("menu_decorations = \"dynamic\"")
    );
    assert_eq!(
        display.settings().centurion.applied_value_indicator,
        AppliedValueIndicator::SideMarker
    );
    assert_eq!(
        display.settings().centurion.map_hover_info,
        MapHoverInfo::On
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("map_hover_info = \"on\"")
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("applied_value_indicator = \"side-marker\"")
    );
    click(&mut menu, "Restore defaults");
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .fleet_information,
        FleetInformation::Both
    );
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .popup_cursor,
        PopupCursor::Visible
    );
    assert_eq!(
        menu.draft
            .as_ref()
            .unwrap()
            .game_values()
            .centurion
            .menu_decorations,
        MenuDecorations::Mouse
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("fleet_information"),
        "reset is a draft"
    );
    click(&mut menu, "Apply");
    let request = creator
        .session
        .borrow()
        .effective(menu.pending.as_ref().unwrap());
    display.apply_settings(request).unwrap();
    menu.applied();
    assert_eq!(
        display.settings().centurion.fleet_information,
        FleetInformation::Both
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("fleet_information")
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("popup_cursor")
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("menu_decorations")
    );
    assert_eq!(display.settings().centurion.map_highlight, MapHighlight::On);
    assert_eq!(
        display.settings().centurion.map_hover_info,
        MapHoverInfo::Off
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("map_hover_info")
    );
    assert_eq!(
        display.settings().centurion.applied_value_indicator,
        AppliedValueIndicator::SubduedBand
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("applied_value_indicator")
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("map_highlight")
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("master_volume = 61")
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("lock")).unwrap();
}

fn sized_draw(
    menu: &mut SettingsMenu<'_>,
    metrics: UiMetrics,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let context = menu.context.clone();
    let size =
        Vec2::new(metrics.drawable.0 as f32, metrics.drawable.1 as f32) / metrics.pixels_per_point;
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(menu.started.elapsed().as_secs_f64()),
        events,
        ..Default::default()
    };
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(metrics.pixels_per_point);
    context.run(input, |context| {
        crate::settings_ui::draw(menu, context, context.screen_rect());
    })
}
fn pulse(menu: &mut SettingsMenu<'_>, metrics: UiMetrics, key: Key) {
    menu.navigation
        .set_mode(&menu.context, crate::settings_selector::InputMode::Keyboard);
    for pressed in [true, false] {
        menu.events.push(MenuEvent::Ui(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }));
    }
    menu.prepare(metrics, RuntimeSettings::default());
}

#[test]
#[ignore = "owns SDL video; run alone with dummy/software"]
fn centurion_native_settings_capture_and_detail_navigation() {
    use crate::ui::themes::{ResolvedAppearance, ThemeId};
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    for (width, height, density) in [(630, 470, 2.25_f32), (1000, 720, 1.0)] {
        let pixels = (
            (width as f32 * density).round() as u32,
            (height as f32 * density).round() as u32,
        );
        let window = video
            .window("Centurion Settings capture", pixels.0, pixels.1)
            .hidden()
            .build()
            .unwrap();
        let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
        let creator = FrameCreator::new(
            &canvas,
            reforge_settings::Session::load(Launch {
                game_id: "centurion-defender-of-rome".into(),
                entry_id: Some("copy".into()),
                path: Some(std::env::temp_dir().join(format!(
                    "reforge-centurion-capture-{}.toml",
                    std::process::id()
                ))),
                cli: Patch::new(),
            }),
            1,
            crate::DisplayGeometry::new((320, 200), (320, 240)),
            None,
        );
        let metrics = UiMetrics {
            drawable: pixels,
            window: pixels,
            pixels_per_point: density,
        };
        for theme in [ThemeId::ClassicDark, ThemeId::CrispLight] {
            let mut menu =
                SettingsMenu::new(&creator, VideoCapabilities::default()).with_centurion_options();
            menu.set_theme(ResolvedAppearance::resolve(theme).theme);
            menu.toggle();
            menu.scope = 1;
            menu.page = ENHANCEMENTS_SECTION;
            for _ in 0..3 {
                menu.prepare(metrics, RuntimeSettings::default());
            }
            let output = sized_draw(&mut menu, metrics, vec![]);
            let pos = label(&output, "Advanced options").unwrap().center();
            for pressed in [true, false] {
                sized_draw(
                    &mut menu,
                    metrics,
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
            menu.prepare(metrics, RuntimeSettings::default());
            let mut found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                let detailed = label(&output, "Both").unwrap();
                if menu
                    .context
                    .memory(|memory| memory.focused())
                    .and_then(|id| menu.context.read_response(id))
                    .is_some_and(|response| response.rect.contains(detailed.center()))
                {
                    found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                found,
                "advanced fleet selector must be reachable: {width} {density}"
            );
            pulse(&mut menu, metrics, Key::ArrowLeft);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .fleet_information,
                FleetInformation::Detailed
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Custom").is_some());
            pulse(&mut menu, metrics, Key::ArrowLeft);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .fleet_information,
                FleetInformation::Original
            );
            for choice in FleetInformation::ALL {
                if choice != FleetInformation::Original {
                    menu.handle_gamepad(crate::gamepad::PadEvent {
                        actions: crate::gamepad::Actions {
                            right: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                    menu.prepare(metrics, RuntimeSettings::default());
                    menu.handle_gamepad(Default::default());
                    assert_eq!(
                        menu.draft
                            .as_ref()
                            .unwrap()
                            .game_values()
                            .centurion
                            .fleet_information,
                        choice
                    );
                }
                for _ in 0..3 {
                    canvas.set_draw_color(sdl3::pixels::Color::RGB(80, 80, 80));
                    canvas.clear();
                    menu.prepare(metrics, RuntimeSettings::default());
                    menu.paint(&mut canvas).unwrap();
                }
                for rect in [menu.apply_rect.unwrap(), menu.cancel_rect.unwrap()] {
                    assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0);
                    assert!(rect.max.x <= width as f32 && rect.max.y <= height as f32);
                }
                if let Some(directory) = std::env::var_os("REFORGE_CENTURION_SETTINGS_CAPTURE") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    canvas
                        .read_pixels(None)
                        .unwrap()
                        .save_bmp(directory.join(format!(
                            "{}-{}-{width}x{height}-{}.bmp",
                            theme.slug(),
                            choice.as_str(),
                            (density * 100.0).round() as u32
                        )))
                        .unwrap();
                }
            }
            let mut cursor_found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                let visible = label(&output, "Visible").unwrap();
                if menu
                    .context
                    .memory(|memory| memory.focused())
                    .and_then(|id| menu.context.read_response(id))
                    .is_some_and(|response| response.rect.contains(visible.center()))
                {
                    cursor_found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                cursor_found,
                "popup cursor selector must be reachable: {width} {density}"
            );
            for cursor in PopupCursor::ALL {
                pulse(&mut menu, metrics, Key::ArrowRight);
                let options = menu.draft.as_ref().unwrap().game_values().centurion;
                assert_eq!(options.fleet_information, FleetInformation::Both);
                assert_eq!(options.popup_cursor, cursor);
                let output = sized_draw(&mut menu, metrics, vec![]);
                assert!(
                    label(
                        &output,
                        if cursor == PopupCursor::Hidden {
                            "Custom"
                        } else {
                            "Reforged"
                        }
                    )
                    .is_some()
                );
                for _ in 0..3 {
                    canvas.set_draw_color(sdl3::pixels::Color::RGB(80, 80, 80));
                    canvas.clear();
                    menu.prepare(metrics, RuntimeSettings::default());
                    menu.paint(&mut canvas).unwrap();
                }
                if let Some(directory) = std::env::var_os("REFORGE_CENTURION_SETTINGS_CAPTURE") {
                    canvas
                        .read_pixels(None)
                        .unwrap()
                        .save_bmp(std::path::PathBuf::from(directory).join(format!(
                            "{}-popup-{}-{width}x{height}-{}.bmp",
                            theme.slug(),
                            cursor.as_str(),
                            (density * 100.0).round() as u32
                        )))
                        .unwrap();
                }
            }
            let mut decorations_found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                let mouse = label(&output, "Mouse").unwrap();
                if menu
                    .context
                    .memory(|memory| memory.focused())
                    .and_then(|id| menu.context.read_response(id))
                    .is_some_and(|response| response.rect.contains(mouse.center()))
                {
                    decorations_found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                decorations_found,
                "menu decorations selector must be reachable: {width} {density}"
            );
            for decorations in [
                MenuDecorations::Keyboard,
                MenuDecorations::Dynamic,
                MenuDecorations::Mouse,
            ] {
                pulse(&mut menu, metrics, Key::ArrowRight);
                let options = menu.draft.as_ref().unwrap().game_values().centurion;
                assert_eq!(options.fleet_information, FleetInformation::Both);
                assert_eq!(options.popup_cursor, PopupCursor::Visible);
                assert_eq!(options.menu_decorations, decorations);
                let output = sized_draw(&mut menu, metrics, vec![]);
                assert!(
                    label(
                        &output,
                        if decorations == MenuDecorations::Mouse {
                            "Reforged"
                        } else {
                            "Custom"
                        }
                    )
                    .is_some()
                );
                for _ in 0..3 {
                    canvas.set_draw_color(sdl3::pixels::Color::RGB(80, 80, 80));
                    canvas.clear();
                    menu.prepare(metrics, RuntimeSettings::default());
                    menu.paint(&mut canvas).unwrap();
                }
                if let Some(directory) = std::env::var_os("REFORGE_CENTURION_SETTINGS_CAPTURE") {
                    canvas
                        .read_pixels(None)
                        .unwrap()
                        .save_bmp(std::path::PathBuf::from(directory).join(format!(
                            "{}-menu-{}-{width}x{height}-{}.bmp",
                            theme.slug(),
                            decorations.as_str(),
                            (density * 100.0).round() as u32
                        )))
                        .unwrap();
                }
            }
            let mut map_found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                let on = label(&output, "On").unwrap();
                if menu
                    .context
                    .memory(|memory| memory.focused())
                    .and_then(|id| menu.context.read_response(id))
                    .is_some_and(|response| response.rect.contains(on.center()))
                {
                    map_found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                map_found,
                "map highlight selector must be reachable: {width} {density}"
            );
            pulse(&mut menu, metrics, Key::ArrowLeft);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .map_highlight,
                MapHighlight::Off
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Custom").is_some());
            menu.handle_gamepad(crate::gamepad::PadEvent {
                actions: crate::gamepad::Actions {
                    right: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            menu.prepare(metrics, RuntimeSettings::default());
            menu.handle_gamepad(Default::default());
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .map_highlight,
                MapHighlight::On
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Reforged").is_some());
            let mut indicator_found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                if let Some(value) = label(&output, "Subdued band")
                    && menu
                        .context
                        .memory(|memory| memory.focused())
                        .and_then(|id| menu.context.read_response(id))
                        .is_some_and(|response| response.rect.contains(value.center()))
                {
                    indicator_found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                indicator_found,
                "applied value selector must be reachable: {width} {density}"
            );
            pulse(&mut menu, metrics, Key::ArrowRight);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .applied_value_indicator,
                AppliedValueIndicator::SideMarker
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Custom").is_some());
            menu.handle_gamepad(crate::gamepad::PadEvent {
                actions: crate::gamepad::Actions {
                    right: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            menu.prepare(metrics, RuntimeSettings::default());
            menu.handle_gamepad(Default::default());
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .applied_value_indicator,
                AppliedValueIndicator::Underline
            );
            pulse(&mut menu, metrics, Key::ArrowLeft);
            pulse(&mut menu, metrics, Key::ArrowLeft);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .applied_value_indicator,
                AppliedValueIndicator::SubduedBand
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Reforged").is_some());
            pulse(&mut menu, metrics, Key::ArrowLeft);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .applied_value_indicator,
                AppliedValueIndicator::Original
            );
            pulse(&mut menu, metrics, Key::ArrowRight);
            let mut hover_found = false;
            for _ in 0..80 {
                let output = sized_draw(&mut menu, metrics, vec![]);
                if let Some(value) = label(&output, "Off")
                    && menu
                        .context
                        .memory(|memory| memory.focused())
                        .and_then(|id| menu.context.read_response(id))
                        .is_some_and(|response| response.rect.contains(value.center()))
                {
                    hover_found = true;
                    break;
                }
                pulse(&mut menu, metrics, Key::Tab);
            }
            assert!(
                hover_found,
                "map hover info selector must be reachable: {width} {density}"
            );
            pulse(&mut menu, metrics, Key::ArrowRight);
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .map_hover_info,
                MapHoverInfo::On
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Custom").is_some());
            menu.handle_gamepad(crate::gamepad::PadEvent {
                actions: crate::gamepad::Actions {
                    left: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            menu.prepare(metrics, RuntimeSettings::default());
            menu.handle_gamepad(Default::default());
            assert_eq!(
                menu.draft
                    .as_ref()
                    .unwrap()
                    .game_values()
                    .centurion
                    .map_hover_info,
                MapHoverInfo::Off
            );
            assert!(label(&sized_draw(&mut menu, metrics, vec![]), "Reforged").is_some());
            if let Some(directory) = std::env::var_os("REFORGE_CENTURION_SETTINGS_CAPTURE") {
                for _ in 0..3 {
                    canvas.clear();
                    menu.prepare(metrics, RuntimeSettings::default());
                    menu.paint(&mut canvas).unwrap();
                }
                canvas
                    .read_pixels(None)
                    .unwrap()
                    .save_bmp(std::path::PathBuf::from(directory).join(format!(
                        "{}-map-highlight-{width}x{height}-{}.bmp",
                        theme.slug(),
                        (density * 100.0).round() as u32
                    )))
                    .unwrap();
            }
        }
    }
}

#[test]
fn sound_device_shares_launcher_pin_and_audio_apply_cancel_reset() {
    use reforge_settings::{SoundCategory, SoundRenderer};
    const SOUND: &str = "audio_renderer";
    if !dummy_video(
        "settings::centurion_tests::sound_device_shares_launcher_pin_and_audio_apply_cancel_reset",
    ) {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "reforge-centurion-sound-ui-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert!(!path.exists());
    let _guard = Launch {
        game_id: "centurion-defender-of-rome".into(),
        entry_id: Some("copy".into()),
        path: Some(path.clone()),
        cli: Patch::from([
            ("renderer".into(), "software".into()),
            ("vsync".into(), "false".into()),
        ]),
    }
    .install();
    let mut display = crate::Window::new("Centurion sound device", 320, 240, Some(3)).unwrap();
    let creator = display.texture_creator();
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
        .with_centurion_options()
        .with_startup_audio_renderers(&[SoundRenderer::Adlib, SoundRenderer::Speaker])
        .with_audio_categories(&[SoundCategory::Music, SoundCategory::Effects]);
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    menu.page = 2;
    assert!(label(&draw(&mut menu, vec![]), "Sound device").is_none());
    menu.scope = 1;
    menu.game_editing[2] = true;
    let output = draw(&mut menu, vec![]);
    assert!(label(&output, "Sound device").is_some());
    assert!(label(&output, "Sound renderer").is_none());
    assert!(label(&output, "Applies on next launch.").is_some());
    click(&mut menu, "AdLib");
    click(&mut menu, "PC speaker");
    assert_eq!(menu.draft.as_ref().unwrap().game[SOUND], "speaker");
    assert!(!path.exists());
    menu.back();
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    menu.scope = 1;
    menu.page = 2;
    menu.game_editing[2] = true;
    assert_eq!(
        menu.draft.as_ref().unwrap().game_values().audio.renderer,
        SoundRenderer::GameDefault
    );
    click(&mut menu, "AdLib");
    click(&mut menu, "PC speaker");
    click(&mut menu, "Apply");
    let request = creator
        .session
        .borrow()
        .effective(menu.pending.as_ref().unwrap());
    display.apply_settings(request).unwrap();
    menu.applied();
    assert_eq!(creator.session.borrow().saved.game[SOUND], "speaker");
    // Unrelated presentation presets must not alter the chosen sound device.
    menu.page = ENHANCEMENTS_SECTION;
    click(&mut menu, "Reforged");
    click(&mut menu, "Original");
    assert_eq!(
        menu.draft.as_ref().unwrap().game_values().audio.renderer,
        SoundRenderer::Speaker
    );
    click(&mut menu, "Original");
    click(&mut menu, "Reforged");
    assert_eq!(menu.draft.as_ref().unwrap().game[SOUND], "speaker");
    menu.page = 2;
    click(&mut menu, "Restore defaults");
    click(&mut menu, "Apply");
    let request = creator
        .session
        .borrow()
        .effective(menu.pending.as_ref().unwrap());
    display.apply_settings(request).unwrap();
    menu.applied();
    assert!(!creator.session.borrow().saved.game.contains_key(SOUND));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn sound_device_cli_lock_and_unregistered_runs_cannot_change_next_launch() {
    use reforge_settings::SoundRenderer;
    const SOUND: &str = "audio_renderer";
    if !dummy_video(
        "settings::centurion_tests::sound_device_cli_lock_and_unregistered_runs_cannot_change_next_launch",
    ) {
        return;
    }
    for (registered, locked) in [(false, false), (false, true), (true, true)] {
        let path = std::env::temp_dir().join(format!(
            "reforge-centurion-sound-locked-{}-{registered}-{locked}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!path.exists());
        let _guard = Launch {
            game_id: "centurion-defender-of-rome".into(),
            entry_id: registered.then(|| "copy".into()),
            path: Some(path.clone()),
            cli: Patch::from([
                ("renderer".into(), "software".into()),
                ("vsync".into(), "false".into()),
            ])
            .into_iter()
            .chain(locked.then(|| (SOUND.into(), "speaker".into())))
            .collect(),
        }
        .install();
        let display = crate::Window::new("Centurion sound lock", 320, 240, Some(3)).unwrap();
        let creator = display.texture_creator();
        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
            .with_centurion_options()
            .with_startup_audio_renderers(&[SoundRenderer::Adlib, SoundRenderer::Speaker]);
        menu.toggle();
        menu.prepare(display.ui_metrics(), display.settings());
        menu.scope = 1;
        menu.page = 2;
        menu.game_editing[2] = true;
        let output = draw(&mut menu, vec![]);
        let selected = if locked { "PC speaker" } else { "AdLib" };
        let alternative = if locked { "AdLib" } else { "PC speaker" };
        assert!(label(&output, selected).is_some());
        if !registered {
            assert!(
                label(
                    &output,
                    "Sound device requires saved game settings; it applies on next launch."
                )
                .is_some()
            );
        }
        click(&mut menu, selected);
        assert!(label(&draw(&mut menu, vec![]), alternative).is_none());
        assert!(!menu.draft.as_ref().unwrap().game.contains_key(SOUND));
        assert!(!path.exists());
    }
}
