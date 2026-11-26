// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::machine_tests::{click, draw, dummy_video, label};
use super::*;
use reforge_settings::{Launch, Patch};

fn pulse(menu: &mut SettingsMenu<'_>, metrics: UiMetrics, key: Key) {
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
fn launcher_section_is_opt_in_and_height_apply_cancel_reset_are_separate() {
    if !dummy_video(
        "settings::launcher_tests::launcher_section_is_opt_in_and_height_apply_cancel_reset_are_separate",
    ) {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "reforge-launcher-ui-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &path,
        "[launcher]\ncustom = 'keep'\n[entries.copy.display]\nrenderer = 'software'\n[entries.copy.crt]\npresentation = 'hyllian'\n[entries.copy.audio]\nmaster_volume = 47\n",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let _guard = Launch {
        directory: None,
        game_id: "centurion-defender-of-rome".into(),
        entry_id: Some("copy".into()),
        path: Some(path.clone()),
        cli: Patch::from([
            ("renderer".into(), "software".into()),
            ("vsync".into(), "false".into()),
        ]),
    }
    .install();
    let display = crate::Window::new("launcher settings transaction", 320, 240, Some(3)).unwrap();
    let creator = display.texture_creator();
    let mut game_menu = SettingsMenu::new(&creator, VideoCapabilities::default());
    game_menu.toggle();
    game_menu.prepare(display.ui_metrics(), display.settings());
    assert!(label(&draw(&mut game_menu, vec![]), "Launcher").is_none());
    assert!(game_menu.launcher_options.is_none());
    drop(game_menu);

    let mut menu =
        SettingsMenu::new(&creator, VideoCapabilities::default()).with_launcher_options();
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    click(&mut menu, "Launcher");
    let output = draw(&mut menu, vec![]);
    assert!(label(&output, "Preview height").is_some());
    assert!(label(&output, "Global defaults").is_none());
    assert!(label(&output, "Game overrides").is_none());
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 180);
    // Reopening the selected section starts at its sidebar button; Right enters
    // the slider and a second Right edits it through the ordinary navigation.
    menu.toggle();
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    menu.prepare(display.ui_metrics(), display.settings());
    pulse(&mut menu, display.ui_metrics(), Key::ArrowRight);
    pulse(&mut menu, display.ui_metrics(), Key::ArrowRight);
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 181);
    pulse(&mut menu, display.ui_metrics(), Key::ArrowLeft);
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 180);
    menu.handle_gamepad(crate::gamepad::PadEvent {
        actions: crate::gamepad::Actions {
            right: true,
            ..Default::default()
        },
        ..Default::default()
    });
    menu.prepare(display.ui_metrics(), display.settings());
    menu.handle_gamepad(Default::default());
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 181);
    pulse(&mut menu, display.ui_metrics(), Key::ArrowLeft);
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 180);
    assert!(label(&draw(&mut menu, vec![]), "No pending changes").is_some());
    pulse(&mut menu, display.ui_metrics(), Key::ArrowRight);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "slider edits wait for Apply"
    );
    menu.back(); // Escape/controller Back discards the open draft.
    assert!(!menu.is_open());
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 180);
    menu.prepare(display.ui_metrics(), display.settings());
    pulse(&mut menu, display.ui_metrics(), Key::ArrowRight);
    for _ in 0..60 {
        pulse(&mut menu, display.ui_metrics(), Key::ArrowLeft);
    }
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 144);
    let saved_game = menu.draft.as_ref().unwrap().game.clone();
    click(&mut menu, "Apply");
    menu.applied();
    assert_eq!(
        reforge_settings::launcher::load(&path)
            .unwrap()
            .split_preview_height,
        144
    );
    assert_eq!(menu.draft.as_ref().unwrap().game, saved_game);
    assert_eq!(
        menu.draft.as_ref().unwrap().game["presentation"],
        "hyllian",
        "launcher-only edits must not normalize untouched game choices"
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("custom = 'keep'")
    );
    click(&mut menu, "Reset section");
    assert_eq!(menu.launcher_options.as_ref().unwrap().height, 180);
    assert_eq!(
        reforge_settings::launcher::load(&path)
            .unwrap()
            .split_preview_height,
        144
    );
    click(&mut menu, "Apply");
    menu.applied();
    assert_eq!(
        reforge_settings::launcher::load(&path)
            .unwrap()
            .split_preview_height,
        180
    );
    assert!(label(&draw(&mut menu, vec![]), "No pending changes").is_some());
    use reforge_settings::launcher::GalleryPreviewAspect;
    assert_eq!(
        menu.launcher_options.as_ref().unwrap().gallery_aspect,
        GalleryPreviewAspect::PerRow
    );
    click(&mut menu, "Per row");
    click(&mut menu, "Whole gallery");
    assert_eq!(
        menu.launcher_options.as_ref().unwrap().gallery_aspect,
        GalleryPreviewAspect::WholeGallery
    );
    menu.back();
    assert!(!menu.is_open());
    assert_eq!(
        menu.launcher_options.as_ref().unwrap().gallery_aspect,
        GalleryPreviewAspect::PerRow
    );
    menu.toggle();
    menu.prepare(display.ui_metrics(), display.settings());
    click(&mut menu, "Per row");
    click(&mut menu, "4:3");
    click(&mut menu, "Apply");
    menu.applied();
    assert_eq!(
        reforge_settings::launcher::load(&path)
            .unwrap()
            .gallery_preview_aspect,
        GalleryPreviewAspect::FourThree
    );
    click(&mut menu, "Reset section");
    assert_eq!(
        menu.launcher_options.as_ref().unwrap().gallery_aspect,
        GalleryPreviewAspect::PerRow
    );
    click(&mut menu, "Apply");
    menu.applied();
    assert_eq!(
        reforge_settings::launcher::load(&path)
            .unwrap()
            .gallery_preview_aspect,
        GalleryPreviewAspect::PerRow
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("lock")).unwrap();
}

#[test]
#[ignore = "owns SDL video; run alone with dummy/software"]
fn launcher_section_native_capture_at_minimum_and_large_sizes() {
    use crate::ui::themes::{ResolvedAppearance, ThemeId};
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    for (width, height, density) in [(630, 520, 2.25_f32), (1000, 720, 1.0)] {
        let pixels = (
            (width as f32 * density).round() as u32,
            (height as f32 * density).round() as u32,
        );
        let window = video
            .window("launcher Settings capture", pixels.0, pixels.1)
            .hidden()
            .build()
            .unwrap();
        let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
        let creator = FrameCreator::new(
            &canvas,
            reforge_settings::Session::load(Launch {
                directory: None,
                game_id: "standalone".into(),
                entry_id: None,
                path: Some(std::env::temp_dir().join(format!(
                    "reforge-launcher-capture-{}.toml",
                    std::process::id()
                ))),
                cli: Patch::new(),
            }),
            1,
            crate::DisplayGeometry::new((320, 200), (320, 240)),
            None,
        );
        for theme in [ThemeId::ClassicDark, ThemeId::CrispLight] {
            let mut menu =
                SettingsMenu::new(&creator, VideoCapabilities::default()).with_launcher_options();
            menu.set_theme(ResolvedAppearance::resolve(theme).theme);
            menu.toggle();
            menu.page = crate::settings_ui::LAUNCHER_PAGE;
            let metrics = UiMetrics {
                drawable: pixels,
                window: pixels,
                pixels_per_point: density,
            };
            for _ in 0..3 {
                canvas.set_draw_color(sdl3::pixels::Color::RGB(80, 80, 80));
                canvas.clear();
                menu.prepare(metrics, RuntimeSettings::default());
                menu.paint(&mut canvas).unwrap();
            }
            assert!(menu.error.is_none());
            for rect in [menu.apply_rect.unwrap(), menu.cancel_rect.unwrap()] {
                assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0);
                assert!(rect.max.x <= width as f32 && rect.max.y <= height as f32);
            }
            // The slider is the first content navigation stop, and its bounds
            // work at native minimum dimensions and a fractional desktop scale.
            pulse(&mut menu, metrics, Key::ArrowRight);
            pulse(&mut menu, metrics, Key::ArrowRight);
            assert_eq!(menu.launcher_options.as_ref().unwrap().height, 181);
            menu.prepare(metrics, RuntimeSettings::default());
            menu.paint(&mut canvas).unwrap();
            if let Some(directory) = std::env::var_os("REFORGE_LAUNCHER_SETTINGS_CAPTURE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                canvas
                    .read_pixels(None)
                    .unwrap()
                    .save_bmp(directory.join(format!(
                        "{}-{width}x{height}-{}.bmp",
                        theme.slug(),
                        (density * 100.0).round() as u32
                    )))
                    .unwrap();
            }
        }
    }
}
