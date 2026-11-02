// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Production UI regressions use authored library records, never original data.
use super::*;
use model::EntryStatus;
use reforge_sdl::egui::{Context, Pos2, RawInput, Rect, Vec2};
use reforge_sdl::ui::themes::{ResolvedAppearance, ThemeId};
use reforge_settings::{Launch, Session, library::Library};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[test]
fn terminal_launch_requires_a_real_attached_terminal() {
    use reforge_settings::Renderer;
    assert!(check_terminal_launch(Renderer::Terminal, false).is_err());
    assert!(check_terminal_launch(Renderer::Terminal, true).is_ok());
    for renderer in [
        Renderer::Auto,
        Renderer::Gpu,
        Renderer::Sdl,
        Renderer::Software,
    ] {
        assert!(check_terminal_launch(renderer, false).is_ok());
    }
}

struct Fixture {
    directory: PathBuf,
    library: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "reforge-launcher-ui-{}-{nonce}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let fixture = Self {
            library: directory.join("library.toml"),
            config: directory.join("config.toml"),
            directory,
        };
        let mut library = Library::new(fixture.library.clone());
        for index in 0..count {
            let release = &detection::RELEASES[index % detection::RELEASES.len()];
            // Deliberately nonexistent originals exercise real Missing recovery.
            library
                .add(
                    release.game().id(),
                    release.id,
                    release.edition.title(),
                    &fixture.directory.join(format!("missing-original-{index}")),
                )
                .unwrap();
        }
        fixture
    }
    fn state(&self, theme: ThemeId, layout: LayoutVariant, verified: bool) -> State {
        let mut state = State::new(self.library.clone(), self.config.clone());
        state.saved_theme = theme;
        state.preview_draft = theme;
        state.layout = layout;
        if verified {
            for game in &mut state.games {
                game.status = EntryStatus::Verified;
            }
        }
        state
    }
    fn session(&self, game: &str) -> Session {
        Session::load(Launch {
            game_id: game.into(),
            entry_id: {
                let mut library = Library::new(self.library.clone());
                library.reload().unwrap();
                library
                    .entries
                    .iter()
                    .find(|e| e.game_id == game)
                    .map(|e| e.entry_id.clone())
            },
            path: Some(self.config.clone()),
            cli: Default::default(),
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
struct Harness {
    context: Context,
    state: State,
    size: Vec2,
    density: f32,
    time: f64,
    actions: Vec<UiAction>,
}
impl Harness {
    fn new(fixture: &Fixture, theme: ThemeId, layout: LayoutVariant, verified: bool) -> Self {
        Self {
            context: Context::default(),
            state: fixture.state(theme, layout, verified),
            size: Vec2::new(1000.0, 720.0),
            density: 1.0,
            time: 0.0,
            actions: Vec::new(),
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(self.density);
        self.context.run(input, |ctx| {
            if let Some(action) = self.state.draw(ctx, false) {
                self.actions.push(action);
            }
        })
    }
    fn settle(&mut self) -> egui::FullOutput {
        for _ in 0..3 {
            self.frame(vec![]);
        }
        self.frame(vec![])
    }
    fn click_at(&mut self, point: Pos2) {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        self.frame(vec![]);
    }
    fn moving_click_at(&mut self, point: Pos2) {
        self.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let moved = point + Vec2::new(8.0, 1.0);
        self.frame(vec![egui::Event::PointerMoved(moved)]);
        self.frame(vec![egui::Event::PointerButton {
            pos: moved,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![]);
    }
    fn click(&mut self, name: &str) {
        let output = self.settle();
        self.click_at(
            text_rect(&output, name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .center(),
        );
    }
}
fn text_rect(output: &egui::FullOutput, name: &str) -> Option<Rect> {
    output
        .shapes
        .iter()
        .rev()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == name => {
                Some(Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        })
}
fn texts(output: &egui::FullOutput) -> Vec<&str> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect()
}

#[test]
fn split_taglines_keep_original_spacing_and_fit_a_second_line() {
    use detection::{Edition, GameMetadata};
    static GAMES: [GameMetadata; 4] = [
        GameMetadata {
            tagline: "",
            ..*detection::RELEASES[0].edition.game
        },
        GameMetadata {
            tagline: "A small cat.",
            ..*detection::RELEASES[0].edition.game
        },
        GameMetadata {
            tagline: "A small cat.\nA very busy neighbourhood.",
            ..*detection::RELEASES[0].edition.game
        },
        GameMetadata {
            tagline: "A small cat. A very busy neighbourhood.",
            ..*detection::RELEASES[0].edition.game
        },
    ];
    static VARIANTS: [Edition; 4] = [
        Edition {
            game: &GAMES[0],
            ..*detection::RELEASES[0].edition
        },
        Edition {
            game: &GAMES[1],
            ..*detection::RELEASES[0].edition
        },
        Edition {
            game: &GAMES[2],
            ..*detection::RELEASES[0].edition
        },
        Edition {
            game: &GAMES[3],
            ..*detection::RELEASES[0].edition
        },
    ];
    let fixture = Fixture::new(1);
    for theme in ThemeId::ALL {
        for density in [1.0, 1.5, 2.25] {
            for size in [Vec2::new(630.0, 470.0), Vec2::new(1000.0, 720.0)] {
                let mut h = Harness::new(&fixture, theme, LayoutVariant::SplitInspector, true);
                h.size = size;
                h.density = density;
                let mut baseline = None;
                for metadata in &VARIANTS {
                    h.state.games[0].edition = Some(metadata);
                    let output = h.settle();
                    let specs = text_rect(&output, &metadata.features_line()).unwrap();
                    let play = text_rect(&output, "Play").unwrap();
                    let credits = text_rect(&output, &metadata.detail_line()).unwrap();
                    // Match the original one-line Label plus the two normal
                    // inter-widget gaps and the existing explicit four points.
                    let original_label_height = h.context.fonts(|fonts| {
                        fonts
                            .layout_no_wrap(
                                "A small cat.".into(),
                                egui::FontId::proportional(11.5),
                                egui::Color32::WHITE,
                            )
                            .size()
                            .y
                    });
                    let original_gap = original_label_height
                        + 2.0 * h.context.style().spacing.item_spacing.y
                        + 4.0;
                    assert!(
                        (specs.top() - credits.bottom() - original_gap).abs() < 0.02,
                        "original spacing changed: {theme:?} {density} {size:?}: actual {} expected {original_gap}, label {original_label_height}",
                        specs.top() - credits.bottom()
                    );
                    if let Some(expected) = baseline {
                        assert_eq!((specs, play), expected, "{theme:?} {density} {size:?}");
                    } else {
                        baseline = Some((specs, play));
                    }
                    if !metadata.game.tagline.is_empty() {
                        let text = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Text(t)
                                    if t.galley.text() == metadata.game.tagline =>
                                {
                                    Some(t)
                                }
                                _ => None,
                            })
                            .unwrap();
                        assert!(text.galley.rows.len() <= 2);
                        assert!(text.pos.y + text.galley.size().y <= specs.top());
                        assert!(!text.galley.elided);
                        assert!(text.pos.y >= credits.bottom());
                    }
                }
            }
        }
    }
}

#[test]
fn catalog_taglines_fit_the_minimum_split_window() {
    let fixture = Fixture::new(detection::RELEASES.len());
    for theme in ThemeId::ALL {
        for density in [1.0, 1.5, 2.0, 2.25] {
            let mut h = Harness::new(&fixture, theme, LayoutVariant::SplitInspector, true);
            h.size = Vec2::new(MIN_LOGICAL_SIZE.0 as f32, MIN_LOGICAL_SIZE.1 as f32);
            h.density = density;
            for (index, release) in detection::RELEASES.iter().enumerate() {
                let metadata = release.edition;
                if metadata.game.tagline.is_empty() {
                    continue;
                }
                h.state.selected = Some(h.state.games[index].entry_id.clone());
                let output = h.settle();
                let tagline = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) if t.galley.text() == metadata.game.tagline => Some(t),
                        _ => None,
                    })
                    .expect("catalog tagline must be visible");
                let credits = text_rect(&output, &metadata.detail_line()).unwrap();
                let specs = text_rect(&output, &metadata.features_line()).unwrap();
                assert!(
                    tagline.galley.rows.len() <= 2
                        && !tagline.galley.elided
                        && tagline.pos.y >= credits.bottom()
                        && tagline.pos.y + tagline.galley.size().y <= specs.top(),
                    "Shorten the tagline for {}: it must fit between credits and specs in at most two lines at the minimum launcher size ({theme:?}, density {density})",
                    release.id
                );
            }
        }
    }
}

#[test]
fn unavailable_catalog_release_keeps_the_entry_title_without_fake_edition_data() {
    let fixture = Fixture::new(1);
    let state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let mut entry = state.library.entries[0].clone();
    entry.title = "My installed copy".into();
    entry.release_id = "synthetic-unavailable-release".into();
    let game = model::GameEntry::from_entry(&entry, fixture.config.clone(), true);
    assert!(game.edition.is_none());
    assert_eq!(game.title, "My installed copy");
    assert!(game.status.is_missing());
    assert_eq!(game.tagline(), "Original files unavailable");
    assert!(game.library_line().is_empty());
    assert!(game.detail_line().is_empty());
    assert!(game.features_line().is_empty());
    assert!(game.options.is_empty());
}

#[test]
fn all_themes_layouts_and_library_states_keep_actions_usable_at_supported_sizes() {
    let fixtures = [
        (Fixture::new(0), true),
        (Fixture::new(6), true),
        (Fixture::new(2), false),
    ];
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            for size in [Vec2::new(630.0, 470.0), Vec2::new(1000.0, 720.0)] {
                for density in [1.0, 1.5, 2.25] {
                    for (fixture, verified) in &fixtures {
                        let mut h = Harness::new(fixture, theme, layout, *verified);
                        h.size = size;
                        h.density = density;
                        h.state.options_open = true;
                        let output = h.settle();
                        let mut labels = vec!["Add…", "Settings…"];
                        if h.state.games.is_empty() {
                            labels.push("Add Game Folder…");
                            assert!(texts(&output).contains(&"Your library is empty"));
                            assert!(
                                !texts(&output)
                                    .iter()
                                    .any(|text| text.contains("Alley Cat")
                                        || text.contains("Mach 3"))
                            );
                        } else {
                            labels.push("Remove");
                            labels.push(if *verified { "Play" } else { "Locate Files…" });
                            assert_eq!(texts(&output).contains(&"Relocate…"), *verified);
                            assert!(!texts(&output).contains(&"Locate…"));
                        }
                        let screen = Rect::from_min_size(Pos2::ZERO, size);
                        for label in labels {
                            let rect = text_rect(&output, label).unwrap_or_else(|| {
                                panic!("missing {label}: {theme:?} {layout:?} {size:?} {density}")
                            });
                            assert!(
                                screen.expand(0.1).contains_rect(rect),
                                "{label} outside screen {rect:?}: {theme:?} {layout:?} {size:?} {density}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn symbolic_art_uses_catalog_titles_and_engine_mode_palettes() {
    let fixture = Fixture::new(2);
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let mut h = Harness::new(&fixture, theme, layout, true);
            let output = h.settle();
            assert!(texts(&output).contains(&"AC"));
            if layout != LayoutVariant::SplitInspector {
                assert!(texts(&output).contains(&"M3"));
            }
            let game = &mut h.state.games[0];
            assert_eq!(game.title, detection::RELEASES[0].edition.title());
            assert_eq!(
                game.palette_swatches(),
                alleycat::launcher::OPTIONS[0].choices[0].palette.unwrap()
            );
            game.save_option("Machine", "IBM PCjr");
            assert_eq!(
                game.palette_swatches(),
                alleycat::launcher::OPTIONS[0].choices[1].palette.unwrap()
            );
            assert_eq!(game.native_arguments(), ["--machine", "pcjr"]);
            game.save_option("Machine", "IBM PCjr");
            let game = &mut h.state.games[1];
            game.save_option("Video", "Monochrome");
            assert_eq!(game.palette_swatches(), &[[0, 0, 0], [255, 255, 255]]);
            assert_eq!(game.native_arguments(), ["--video", "mono"]);
            game.save_option("Video", "Monochrome");
        }
    }
}

#[test]
fn all_engines_quick_options_fit_the_minimum_logical_window() {
    let fixture = Fixture::new(detection::RELEASES.len());
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            for index in 0..detection::RELEASES.len() {
                let mut h = Harness::new(&fixture, theme, layout, true);
                h.size = Vec2::new(630.0, 470.0);
                h.state.selected = Some(h.state.games[index].entry_id.clone());
                h.state.options_open = true;
                let output = h.settle();
                let screen = Rect::from_min_size(Pos2::ZERO, h.size);
                for option in h.state.games[index].options {
                    for choice in option.choices {
                        let label = if layout == LayoutVariant::SplitInspector {
                            choice.label
                        } else {
                            choice.short_label
                        };
                        let rect = text_rect(&output, label).unwrap_or_else(|| {
                            panic!("missing {label}: {theme:?} {layout:?} game {index}")
                        });
                        assert!(
                            screen.contains_rect(rect),
                            "clipped {label}: {theme:?} {layout:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn mach3_quick_presentation_options_preserve_hidden_sample_preferences() {
    let fixture = Fixture::new(2);
    let mut session = fixture.session("mach-3");
    let mut draft = session.saved.clone();
    draft
        .game
        .insert("digitized_samples".into(), "false".into());
    session.commit(draft).unwrap();
    let mut h = Harness::new(
        &fixture,
        ThemeId::ClassicDark,
        LayoutVariant::SplitInspector,
        true,
    );
    h.state.selected = Some(h.state.games[1].entry_id.clone());
    let game = &mut h.state.games[1];
    game.save_option("Video", "Monochrome");
    game.save_option("Display", "Hyllian CRT");
    game.save_option("Window", "Fullscreen");
    assert_eq!(game.full_mode_label(), "Mono · CRT");
    game.save_option("Audio", "Enhanced PCM");
    assert!(game.error.is_none());
    assert_eq!(game.full_mode_label(), "Mono · CRT · PCM");
    assert_eq!(
        fixture.session("mach-3").saved.game["digitized_samples"],
        "false"
    );
    let output = h.settle();
    assert!(!texts(&output).contains(&"Samples"));
    let mut previous = f32::NEG_INFINITY;
    for label in ["Video", "Display", "Window", "Audio"] {
        let rect = text_rect(&output, label).unwrap();
        assert!(rect.top() > previous, "quick options are out of order");
        previous = rect.top();
    }
}

#[test]
fn immediate_pins_survive_reloads_and_global_changes_even_when_values_match() {
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let fixture = Fixture::new(2);
            let mut h = Harness::new(&fixture, theme, layout, true);
            h.state.options_open = true;
            let output = h.settle();
            h.moving_click_at(text_rect(&output, "Windowed").unwrap().center());
            assert_eq!(
                fixture.session("alley-cat").saved.game["window_mode"],
                "windowed"
            );
            let mut session = fixture.session("alley-cat");
            let mut draft = session.saved.clone();
            draft
                .global
                .insert("window_mode".into(), "fullscreen".into());
            session.commit(draft).unwrap();
            h.state.games[0].refresh_preferences().unwrap();
            assert_eq!(h.state.games[0].option_values()["Window"], "Windowed");
            h.click("Windowed");
            assert!(
                !fixture
                    .session("alley-cat")
                    .saved
                    .game
                    .contains_key("window_mode")
            );
            assert_eq!(h.state.games[0].option_values()["Window"], "Fullscreen");
            h.click("Fullscreen");
            assert_eq!(
                fixture.session("alley-cat").saved.game["window_mode"],
                "fullscreen"
            );
            let mut session = fixture.session("alley-cat");
            let mut draft = session.saved.clone();
            draft.global.remove("window_mode");
            draft.edit(false, "master_volume", "65".into());
            session.commit(draft).unwrap();
            let reloaded = fixture.state(theme, layout, false);
            assert_eq!(reloaded.games[0].preferences.saved["Window"], "Fullscreen");
            assert_eq!(reloaded.games[0].option_values()["Window"], "Fullscreen");
        }
    }
}

#[test]
fn btz_launch_options_preserve_graphics_and_disable_unavailable_choices() {
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let fixture = Fixture::new(detection::RELEASES.len());
            let mut h = Harness::new(&fixture, theme, layout, true);
            h.state.games.retain(|game| game.game_id == "mindwheel");
            h.size = Vec2::new(630.0, 470.0);
            h.state.selected = Some(h.state.games[0].entry_id.clone());
            h.state.options_open = true;
            let game = &mut h.state.games[0];
            game.terminal_available = true;
            assert_eq!(
                game.option_choices()
                    .iter()
                    .map(|(field, _)| *field)
                    .collect::<Vec<_>>(),
                ["Renderer", "Display", "Window"]
            );
            let mut s = fixture.session("mindwheel");
            let mut d = s.saved.clone();
            d.game.insert("renderer".into(), "gpu".into());
            s.commit(d).unwrap();
            game.refresh_preferences().unwrap();
            game.save_option("Display", "Hyllian CRT");
            game.save_option("Window", "Fullscreen");
            let graphical = fixture.session("mindwheel").saved;
            h.click("Terminal");
            assert!(h.state.games[0].terminal_selected());
            assert_eq!(h.state.games[0].full_mode_label(), "Terminal");
            assert!(!h.state.games[0].crt_preview());
            assert!(!h.state.games[0].option_enabled("Display"));
            assert!(!h.state.games[0].option_enabled("Window"));
            h.click("Sharp pixels");
            h.click("Windowed");
            assert_eq!(h.state.games[0].option_values()["Display"], "Hyllian CRT");
            assert_eq!(h.state.games[0].option_values()["Window"], "Fullscreen");
            // Reload from disk while terminal mode is active.
            h.state.games[0].refresh_preferences().unwrap();
            h.click("Graphical");
            assert_eq!(fixture.session("mindwheel").saved, graphical);
            assert!(h.state.games[0].option_enabled("Display"));
            assert!(h.state.games[0].option_enabled("Window"));
            h.state.games[0].terminal_available = false;
            h.click("Terminal");
            assert!(!h.state.games[0].terminal_selected());
            assert_eq!(fixture.session("mindwheel").saved, graphical);
        }
    }
}

#[test]
fn layout_hotkeys_and_theme_keep_revert_and_dismissal_persist_independently() {
    let fixture = Fixture::new(2);
    let mut h = Harness::new(
        &fixture,
        ThemeId::ClassicDark,
        LayoutVariant::SplitInspector,
        true,
    );
    for (key, layout) in [
        (Keycode::F4, LayoutVariant::CompactDeck),
        (Keycode::F3, LayoutVariant::GalleryShelf),
        (Keycode::F2, LayoutVariant::SplitInspector),
    ] {
        assert!(h.state.shortcut(Some(key), Mod::NOMOD, false));
        assert_eq!(
            State::new(fixture.library.clone(), fixture.config.clone()).layout,
            layout
        );
        assert!(!h.state.shortcut(Some(Keycode::F4), Mod::NOMOD, true));
        assert_eq!(h.state.layout, layout);
    }
    assert!(h.state.shortcut(Some(Keycode::T), Mod::LCTRLMOD, false));
    h.click(ThemeId::SoftTonal.label());
    h.click("Revert");
    assert_eq!(h.state.saved_theme, ThemeId::ClassicDark);
    assert!(!h.state.preview_open);
    h.state.shortcut(Some(Keycode::T), Mod::LCTRLMOD, false);
    h.click(ThemeId::SoftTonal.label());
    h.click("Keep");
    assert_eq!(
        reforge_settings::launcher::load(&fixture.config)
            .unwrap()
            .theme,
        "soft-tonal"
    );
    h.state.shortcut(Some(Keycode::T), Mod::LCTRLMOD, false);
    h.click(ThemeId::WarmAmber.label());
    h.click_at(egui::pos2(980.0, 100.0));
    assert!(!h.state.preview_open);
    assert_eq!(
        reforge_settings::launcher::load(&fixture.config)
            .unwrap()
            .theme,
        "warm-amber"
    );
    assert_eq!(
        reforge_settings::launcher::load(&fixture.config)
            .unwrap()
            .layout,
        "split"
    );
    h.state.shortcut(Some(Keycode::T), Mod::LCTRLMOD, false);
    h.state.preview_draft = ThemeId::CrispLight;
    h.state.shortcut(Some(Keycode::T), Mod::LCTRLMOD, false);
    assert_eq!(
        reforge_settings::launcher::load(&fixture.config)
            .unwrap()
            .theme,
        "crisp-light"
    );
}

#[test]
fn game_selection_and_double_clicks_never_launch_until_play_is_activated() {
    let fixture = Fixture::new(2);
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let mut h = Harness::new(&fixture, theme, layout, true);
            let output = h.settle();
            let point = text_rect(&output, "Mach 3").unwrap().center();
            h.moving_click_at(point);
            h.click_at(point);
            assert_eq!(h.state.selected, Some(h.state.games[1].entry_id.clone()));
            assert!(
                h.actions.is_empty(),
                "game selection launched: {theme:?} {layout:?}"
            );
            h.click("Play");
            assert_eq!(
                h.actions,
                [UiAction::Play(h.state.games[1].entry_id.clone())]
            );
        }
    }
}

#[test]
fn removal_requires_confirmation_and_cancel_or_escape_preserves_library() {
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let fixture = Fixture::new(2);
            let mut h = Harness::new(&fixture, theme, layout, true);
            h.click("Remove");
            assert_eq!(
                h.state.pending_remove,
                Some(h.state.games[0].entry_id.clone())
            );
            assert_eq!(h.state.library.entries.len(), 2);
            h.click("Cancel");
            assert!(h.state.pending_remove.is_none());
            assert_eq!(h.state.library.entries.len(), 2);
            h.click("Remove");
            assert!(h.state.shortcut(Some(Keycode::Escape), Mod::NOMOD, false));
            assert_eq!(h.state.library.entries.len(), 2);
            h.frame(vec![]);
            h.click("Remove");
            h.click("Remove game");
            assert!(h.state.pending_remove.is_none());
            let mut library = Library::new(fixture.library.clone());
            library.reload().unwrap();
            assert_eq!(library.entries.len(), 1);
            assert_eq!(library.entries[0].game_id, "mach-3");
        }
    }
}

#[test]
fn truncated_status_has_one_tooltip_with_full_path_and_missing_details() {
    let context = Context::default();
    context.style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let fixture = Fixture::new(1);
    let mut game = fixture
        .state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true)
        .games
        .remove(0);
    game.original_path =
        "/synthetic/a/very/long/game/location/that/does/not/fit/the/status/row/CAT.EXE".into();
    let appearance = ResolvedAppearance::resolve(ThemeId::ClassicDark);
    for (index, status) in [
        EntryStatus::Verified,
        EntryStatus::Missing("Required original file unavailable".into()),
    ]
    .into_iter()
    .enumerate()
    {
        game.status = status;
        let mut point = Pos2::ZERO;
        let mut output = None;
        for frame in 0..40 {
            let rendered = context.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 400.0))),
                    time: Some((index * 40 + frame) as f64 * 0.1),
                    events: if frame > 2 {
                        vec![egui::Event::PointerMoved(point)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_max_width(200.0);
                        widgets::draw_provenance_inline(ui, &appearance, &game);
                    });
                },
            );
            if frame == 2 {
                let prefix = if game.status.is_missing() {
                    "Missing"
                } else {
                    "Verified"
                };
                point = text_rect(&rendered, &format!("{prefix} · {}", game.original_path))
                    .unwrap()
                    .center();
            }
            output = Some(rendered);
        }
        let output = output.unwrap();
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.elided && text.galley.text().contains(&game.original_path)
        )), "fixture must truncate the status label");
        // One elided inline label and one full-path tooltip; egui retains the
        // original string in the inline galley as well as the tooltip galley.
        let paths: Vec<_> = texts(&output)
            .into_iter()
            .filter(|text| text.contains(&game.original_path))
            .collect();
        assert_eq!(paths.len(), 2, "{paths:?}");
        if game.status.is_missing() {
            assert!(
                texts(&output)
                    .iter()
                    .any(|text| text.contains("Required original file unavailable"))
            );
        }
    }
}

#[test]
fn invalid_configuration_stays_intact_and_draws_without_panicking() {
    for invalid in ["[global.display\n", "schema_version = 999\n"] {
        let fixture = Fixture::new(2);
        std::fs::write(&fixture.config, invalid).unwrap();
        for theme in ThemeId::ALL {
            for layout in LayoutVariant::ALL {
                let mut h = Harness::new(&fixture, theme, layout, true);
                h.state.options_open = true;
                h.settle();
                h.state.games[0].save_option("Window", "Fullscreen");
                h.frame(vec![]);
                assert!(h.state.notice.as_ref().is_some_and(|(_, error)| *error));
                assert_eq!(std::fs::read_to_string(&fixture.config).unwrap(), invalid);
            }
        }
    }
}

#[test]
fn folder_callback_owns_selection_and_handles_cancellation() {
    for selected in [None, Some("/synthetic/game")] {
        let (sender, receiver) = mpsc::channel::<FolderResult>();
        let text = selected.map(|value| std::ffi::CString::new(value).unwrap());
        let files = [
            text.as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            std::ptr::null(),
        ];
        // SAFETY: emulate SDL's one callback with live null-terminated data.
        unsafe {
            folder_result(Box::into_raw(Box::new(sender)).cast(), files.as_ptr(), -1);
        }
        assert_eq!(
            receiver.recv().unwrap().unwrap(),
            selected.map(PathBuf::from)
        );
    }
}

/// Run alone so SDL owns video on one thread. The normal suite above is entirely
/// headless; this opt-in check also exercises the production input and painter.
#[test]
#[ignore = "owns SDL video; run with SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software --test-threads=1"]
fn btz_presentation_controls_native_painter() {
    use reforge_sdl::settings::UiMetrics;
    let fixture = Fixture::new(detection::RELEASES.len());
    let _guard = Launch {
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(fixture.config.clone()),
        cli: reforge_settings::Patch::from([("renderer".into(), "software".into())]),
    }
    .install();
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("BTZ presentation", 630, 470)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas();
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    let metrics = UiMetrics {
        drawable: (630, 470),
        window: (630, 470),
        pixels_per_point: 1.0,
    };
    let capture = std::env::var_os("REFORGE_LAUNCHER_CAPTURE").map(PathBuf::from);
    if let Some(directory) = &capture {
        std::fs::create_dir_all(directory).unwrap();
    }
    for theme in ThemeId::ALL {
        gui.set_theme(ResolvedAppearance::resolve(theme).theme);
        for layout in LayoutVariant::ALL {
            for terminal in [false, true] {
                let mut state = fixture.state(theme, layout, true);
                state.games.retain(|game| game.game_id == "mindwheel");
                state.selected = Some(state.games[0].entry_id.clone());
                state.options_open = true;
                let game = &mut state.games[0];
                game.preferences
                    .saved
                    .insert("Renderer", if terminal { "Terminal" } else { "Graphical" });
                game.preferences.saved.insert("Display", "Hyllian CRT");
                game.preferences.saved.insert("Window", "Fullscreen");
                game.terminal_available = true;
                for _ in 0..4 {
                    native_frame(&mut gui, &mut canvas, &mut state, metrics);
                }
                if let Some(directory) = &capture {
                    canvas
                        .read_pixels(None)
                        .unwrap()
                        .save_bmp(directory.join(format!(
                            "btz-{}-{}-{terminal}.bmp",
                            theme.slug(),
                            layout.slug()
                        )))
                        .unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "owns SDL video; run with SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software --test-threads=1"]
fn native_painter_capture_matrix_and_modal_navigation() {
    use reforge_sdl::settings::UiMetrics;
    let configuration = Fixture::new(0);
    let _guard = Launch {
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(configuration.config.clone()),
        cli: reforge_settings::Patch::from([
            ("renderer".into(), "software".into()),
            ("vsync".into(), "false".into()),
        ]),
    }
    .install();
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let fixtures = [
        ("empty", Fixture::new(0), true),
        ("six", Fixture::new(6), true),
        ("missing", Fixture::new(2), false),
    ];
    let capture = std::env::var_os("REFORGE_LAUNCHER_CAPTURE").map(PathBuf::from);
    if let Some(directory) = &capture {
        std::fs::create_dir_all(directory).unwrap();
    }
    for logical in [(630_u32, 470_u32), (1000_u32, 720_u32)] {
        for density in [1.0_f32, 1.5, 2.25] {
            let pixels = (
                (logical.0 as f32 * density).round() as u32,
                (logical.1 as f32 * density).round() as u32,
            );
            let window = video
                .window("Launcher regression", pixels.0, pixels.1)
                .hidden()
                .build()
                .unwrap();
            let mut canvas = window.into_canvas();
            let creator = FrameCreator::for_ui(&canvas);
            let mut gui = Gui::new(&creator);
            gui.set_focus_overlay(false);
            let metrics = UiMetrics {
                drawable: pixels,
                window: pixels,
                pixels_per_point: density,
            };
            for theme in ThemeId::ALL {
                gui.set_theme(ResolvedAppearance::resolve(theme).theme);
                for layout in LayoutVariant::ALL {
                    for (name, fixture, verified) in &fixtures {
                        let mut state = fixture.state(theme, layout, *verified);
                        state.options_open = true;
                        for _ in 0..4 {
                            native_frame(&mut gui, &mut canvas, &mut state, metrics);
                        }
                        // Keep captures bounded: one compact/high-DPI and one
                        // spacious/native-DPI set, while painting all 216 cases.
                        if let Some(directory) = capture.as_ref().filter(|_| {
                            (logical.0 == 630 && density == 2.25)
                                || (logical.0 == 1000 && density == 1.0)
                        }) {
                            let file = directory.join(format!(
                                "{}-{}-{name}-{}x{}-{}.bmp",
                                theme.slug(),
                                layout.slug(),
                                logical.0,
                                logical.1,
                                (density * 100.0) as u32
                            ));
                            canvas.read_pixels(None).unwrap().save_bmp(file).unwrap();
                        }
                    }
                    let mut state = fixtures[1].1.state(theme, layout, true);
                    state.toggle_preview();
                    let mut initial = None;
                    for _ in 0..4 {
                        initial = native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    }
                    assert!(
                        initial.is_some(),
                        "theme chooser must focus its selected row"
                    );
                    let direction = if theme == ThemeId::CrispLight {
                        Keycode::Up
                    } else {
                        Keycode::Down
                    };
                    gui.handle_event(&native_key(direction));
                    let next = native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    assert_ne!(initial, next, "theme rows must be keyboard reachable");
                    gui.handle_event(&native_key(Keycode::Return));
                    native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    assert_ne!(state.preview_draft, theme);
                    state.revert_preview();
                    state.request_remove(state.games[0].entry_id.clone());
                    let mut focused = None;
                    for _ in 0..4 {
                        focused = native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    }
                    let cancel = focused.expect("confirmation must focus Cancel");
                    gui.handle_event(&native_key(Keycode::Right));
                    let confirm = native_frame(&mut gui, &mut canvas, &mut state, metrics)
                        .expect("Remove must remain keyboard reachable inside the modal");
                    assert_ne!(cancel, confirm, "{theme:?} {layout:?} {density}");
                    gui.handle_event(&native_key(Keycode::Left));
                    assert_eq!(
                        native_frame(&mut gui, &mut canvas, &mut state, metrics),
                        Some(cancel),
                        "{theme:?} {layout:?} {density}"
                    );
                    gui.handle_event(&native_key(Keycode::Return));
                    native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    assert!(state.pending_remove.is_none());
                    assert_eq!(state.games.len(), 6, "Cancel must preserve the library");
                }
            }
        }
    }
}

fn native_frame(
    gui: &mut Gui<'_>,
    canvas: &mut WindowCanvas,
    state: &mut State,
    metrics: reforge_sdl::settings::UiMetrics,
) -> Option<egui::Id> {
    let mut focused = None;
    gui.prepare(metrics, |context| {
        let action = state.draw(context, false);
        assert!(
            action.is_none(),
            "capture/navigation unexpectedly launched {action:?}"
        );
        focused = context.memory(|memory| memory.focused());
    });
    gui.paint(canvas).unwrap();
    canvas.present();
    focused
}

fn native_key(key: Keycode) -> Event {
    Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode: Some(key),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
        which: 0,
        raw: 0,
    }
}

#[test]
fn duplicate_game_copies_keep_independent_quick_options_and_display_titles() {
    let duplicate_index = detection::RELEASES.len();
    let fixture = Fixture::new(duplicate_index + 1);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    assert_eq!(state.games[0].game_id, state.games[duplicate_index].game_id);
    state.games[0].save_option("Machine", "IBM PCjr");
    state.games[0].save_option("Window", "Fullscreen");
    state.refresh();
    assert_eq!(state.games[0].option_values()["Machine"], "IBM PCjr");
    assert_eq!(
        state.games[duplicate_index].option_values()["Machine"],
        "IBM CGA"
    );
    assert_eq!(state.games[0].option_values()["Window"], "Fullscreen");
    assert_eq!(
        state.games[duplicate_index].option_values()["Window"],
        "Windowed"
    );
    let mut renamed = state.library.entries[0].clone();
    renamed.title = "Alley Cat — PCjr copy".into();
    let entry = model::GameEntry::from_entry(&renamed, fixture.config.clone(), false);
    assert_eq!(entry.title, "Alley Cat — PCjr copy");
    assert_eq!(entry.entry_id, state.games[0].entry_id);
    assert_eq!(entry.option_values()["Machine"], "IBM PCjr");
}

#[test]
fn notifications_wrap_long_paths_keep_dismiss_visible_and_do_not_shift_the_launcher() {
    let fixture = Fixture::new(2);
    let messages = [
        ("Game added".to_owned(), false),
        (
            format!(
                "Unknown or mismatched original data at /home/player/Games/Mach3/{}: no supported original file names",
                "recordings-with-long-directory-names/".repeat(6)
            ),
            true,
        ),
        (
            format!(
                "Cannot verify /Games/{}\n{}",
                "verylongunbrokenfilename".repeat(20),
                "Diagnostic detail\n".repeat(80)
            ),
            true,
        ),
    ];
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            for size in [Vec2::new(630.0, 470.0), Vec2::new(1000.0, 720.0)] {
                for density in [1.0, 2.25] {
                    let mut h = Harness::new(&fixture, theme, layout, true);
                    h.size = size;
                    h.density = density;
                    let original_play = text_rect(&h.settle(), "Play").unwrap();
                    let screen = Rect::from_min_size(Pos2::ZERO, size);
                    for (message, error) in &messages {
                        h.state.notify(message.clone(), *error);
                        let output = h.settle();
                        let card = h
                            .context
                            .memory(|m| m.area_rect(egui::Id::new("launcher_notice")))
                            .unwrap();
                        assert!(
                            screen.shrink(15.0).contains_rect(card),
                            "notice outside screen: {card:?} {theme:?} {layout:?} {size:?} {density}"
                        );
                        let dismiss = text_rect(&output, "Dismiss").unwrap();
                        assert!(
                            card.contains_rect(dismiss),
                            "dismiss clipped: {dismiss:?} {card:?}"
                        );
                        let text = text_rect(&output, message)
                            .expect("complete diagnostic remains available");
                        assert!(
                            text.left() >= card.left() && text.right() <= dismiss.left(),
                            "message overflows its column: {text:?} {card:?} {dismiss:?}"
                        );
                        assert_eq!(text_rect(&output, "Play").unwrap(), original_play);
                        if message.contains("Diagnostic detail") {
                            let point = Pos2::new(text.center().x, card.center().y);
                            h.frame(vec![egui::Event::PointerMoved(point)]);
                            h.frame(vec![egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: Vec2::new(0.0, -120.0),
                                modifiers: egui::Modifiers::NONE,
                            }]);
                            let scrolled = h.settle();
                            assert!(
                                text_rect(&scrolled, message).unwrap().top() < text.top(),
                                "long diagnostics must scroll"
                            );
                            assert_eq!(text_rect(&scrolled, "Dismiss").unwrap(), dismiss);
                            h.frame(vec![egui::Event::PointerMoved(Pos2::ZERO)]);
                            h.state.notify(message.clone(), *error);
                            let restarted = h.settle();
                            assert_eq!(
                                text_rect(&restarted, message).unwrap().top(),
                                text.top(),
                                "each new notice starts at the top"
                            );
                        }
                        h.click("Dismiss");
                        assert!(h.state.notice.is_none());
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "owns SDL video; run with dummy/software --test-threads=1"]
fn native_notification_previews() {
    use reforge_sdl::settings::UiMetrics;
    let fixture = Fixture::new(2);
    let _guard = Launch {
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(fixture.config.clone()),
        cli: reforge_settings::Patch::from([("renderer".into(), "software".into())]),
    }
    .install();
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let capture = std::env::var_os("REFORGE_LAUNCHER_CAPTURE").map(PathBuf::from);
    if let Some(path) = &capture {
        std::fs::create_dir_all(path).unwrap();
    }
    let density = 2.25;
    let pixels = (1920, 1260);
    let window = video
        .window("Notifications", pixels.0, pixels.1)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas();
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    let metrics = UiMetrics {
        drawable: pixels,
        window: pixels,
        pixels_per_point: density,
    };
    for theme in ThemeId::ALL {
        gui.set_theme(ResolvedAppearance::resolve(theme).theme);
        for layout in LayoutVariant::ALL {
            let mut state = fixture.state(theme, layout, true);
            state.notify("Unknown or mismatched original data at /home/player/Games/Mach3/private/play/recordings: no supported original file names".into(), true);
            for _ in 0..4 {
                native_frame(&mut gui, &mut canvas, &mut state, metrics);
            }
            if let Some(path) = &capture {
                canvas
                    .read_pixels(None)
                    .unwrap()
                    .save_bmp(path.join(format!("{}-{}.bmp", theme.slug(), layout.slug())))
                    .unwrap();
            }
        }
    }
}

#[test]
#[ignore = "owns SDL video; optional REFORGE_PREVIEW_ORIGINALS supplies two originals; run serially"]
fn native_engine_previews() {
    use reforge_sdl::{DisplayGeometry, preview::Frame, settings::UiMetrics};
    use std::sync::Arc;
    let fixture = Fixture::new(4);
    let originals = std::env::var_os("REFORGE_PREVIEW_ORIGINALS")
        .map(|v| std::env::split_paths(&v).collect::<Vec<_>>());
    if let Some(paths) = &originals {
        assert_eq!(
            paths.len(),
            2,
            "Alley Cat executable and Mach 3 directory required"
        );
        let mut library = Library::new(fixture.library.clone());
        // Real inputs remain outside the repository and untouched. Multiple
        // entries exercise variant selection, not separate game execution.
        for path in paths {
            let d = detection::detect(path).unwrap();
            library
                .add(
                    d.game.id(),
                    d.release.id,
                    d.release.edition.title(),
                    &d.original,
                )
                .unwrap();
        }
    }
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::GalleryShelf, true);
    if originals.is_some() {
        state.games.retain(|g| {
            super::verify_entry(
                &state
                    .library
                    .entries
                    .iter()
                    .find(|e| e.entry_id == g.entry_id)
                    .unwrap()
                    .clone(),
            )
            .is_ok()
        });
        assert_eq!(state.games.len(), 2);
        let mut manager = preview::sources::Manager::new(Some(fixture.directory.join("previews")));
        let start = std::time::Instant::now();
        while state.games.iter().any(|g| g.previews.len() != 2) {
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "engine preview generation timed out"
            );
            manager.poll(&mut state.games);
            std::thread::sleep(Duration::from_millis(10));
        }
        // A recipe is stable across independent generation, including random
        // intro motion and both machine-specific timing paths.
        for game in &state.games {
            let detected = detection::detect(&game.original).unwrap();
            for (variant, source) in &game.previews {
                let again = match detected.game {
                    detection::Game::AlleyCat => {
                        alleycat::launcher::preview(&detected.original, variant)
                    }
                    detection::Game::Mach3 => mach3::launcher::preview(&detected.files, variant),
                    detection::Game::Mindwheel => unreachable!("Mindwheel has no preview recipe"),
                }
                .unwrap();
                assert_eq!(source.frame.rgba(), again.rgba());
                assert_eq!(source.frame.geometry(), again.geometry());
                assert_eq!(source.frame.is_monochrome(), again.is_monochrome());
            }
        }
        let mono = &state
            .games
            .iter()
            .find(|g| g.game_id == "mach-3")
            .unwrap()
            .previews["mono"]
            .frame;
        assert!(mono.is_monochrome());
        for size in [[56, 42], [126, 95], [480, 360], [800, 600]] {
            assert!(
                reforge_sdl::preview::render(mono, size, 2.25, true)
                    .pixels
                    .iter()
                    .all(|p| p.r() == p.g() && p.g() == p.b())
            );
        }
        let mut additional = state.games.clone();
        for game in &mut additional {
            game.entry_id.push_str("-variant");
            let option = if game.game_id == "alley-cat" {
                "Machine"
            } else {
                "Video"
            };
            let value = if option == "Machine" {
                "IBM PCjr"
            } else {
                "Monochrome"
            };
            game.preferences.saved.insert(option, value);
        }
        state.games.extend(additional);
    } else {
        for (i, game) in state.games.iter_mut().enumerate() {
            let pixels = (0..320 * 200)
                .flat_map(|n| {
                    if ((n % 320) / 8 + (n / 320) / 8) % 2 == 0 {
                        [75, 150, 220, 255]
                    } else {
                        [220, 180, 90, 255]
                    }
                })
                .collect();
            let frame = Frame::new(DisplayGeometry::new((320, 200), (320, 240)), pixels).unwrap();
            game.previews.insert(
                game.preview_variant().into(),
                preview::sources::Source {
                    id: format!("synthetic-{i}"),
                    frame: Arc::new(frame),
                },
            );
        }
    }
    state.selected = state.games.first().map(|game| game.entry_id.clone());
    let capture = std::env::var_os("REFORGE_LAUNCHER_CAPTURE").map(PathBuf::from);
    if let Some(path) = &capture {
        std::fs::create_dir_all(path).unwrap();
    }
    let _guard = Launch {
        game_id: "standalone".into(),
        entry_id: None,
        path: Some(fixture.config.clone()),
        cli: Default::default(),
    }
    .install();
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("Engine previews", 1920, 1260)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = sdl3::render::create_renderer(window, None).unwrap();
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    for density in [1.0, 2.25] {
        let metrics = UiMetrics {
            drawable: (1920, 1260),
            window: (1920, 1260),
            pixels_per_point: density,
        };
        for theme in ThemeId::ALL {
            gui.set_theme(ResolvedAppearance::resolve(theme).theme);
            state.saved_theme = theme;
            for layout in LayoutVariant::ALL {
                state.layout = layout;
                for crt in [false, true] {
                    for game in &mut state.games {
                        game.preferences
                            .saved
                            .insert("Display", if crt { "Hyllian CRT" } else { "Sharp pixels" });
                    }
                    for _ in 0..4 {
                        native_frame(&mut gui, &mut canvas, &mut state, metrics);
                    }
                    if let Some(path) = &capture {
                        canvas
                            .read_pixels(None)
                            .unwrap()
                            .save_bmp(path.join(format!(
                                "{}-{}-{}-{}.bmp",
                                theme.slug(),
                                layout.slug(),
                                if crt { "crt" } else { "sharp" },
                                (density * 100.0) as u32
                            )))
                            .unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn gallery_options_toggle_preserves_preview_positions_and_sizes() {
    let preview_rects = |output: &egui::FullOutput| {
        output.shapes.iter().filter_map(|shape|match &shape.shape {
            egui::Shape::Mesh(mesh) if matches!(mesh.texture_id,egui::TextureId::Managed(id) if id>0)=>Some(mesh.calc_bounds()),
            _=>None,
        }).collect::<Vec<_>>()
    };
    for count in [2, 6, 12] {
        let fixture = Fixture::new(count);
        for theme in ThemeId::ALL {
            for size in [
                Vec2::new(630.0, 470.0),
                Vec2::new(1000.0, 720.0),
                Vec2::new(1600.0, 1000.0),
            ] {
                for density in [1.0, 2.25] {
                    let mut h = Harness::new(&fixture, theme, LayoutVariant::GalleryShelf, true);
                    h.size = size;
                    h.density = density;
                    let closed = preview_rects(&h.settle());
                    assert!(!closed.is_empty());
                    for open in [true, false, true, false] {
                        h.state.options_open = open;
                        // Check the transition frame as well as the settled result.
                        for _ in 0..4 {
                            let output = h.frame(vec![]);
                            let current = preview_rects(&output);
                            assert!(!current.is_empty());
                            for (before, after) in closed.iter().zip(&current) {
                                assert!(
                                    (before.min - after.min).length() < 0.01
                                        && (before.size() - after.size()).length() < 0.01,
                                    "Gallery moved: {count} {theme:?} {size:?} density {density}, open {open}: {before:?} -> {after:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn split_preview_resizes_smoothly_and_keeps_fitting_badges_visible() {
    use reforge_sdl::{DisplayGeometry, preview::Frame};
    use std::sync::Arc;

    let fixture = Fixture::new(2);
    for theme in ThemeId::ALL {
        for density in [1.0, 1.5, 2.25] {
            for selected in 0..2 {
                let mut h = Harness::new(&fixture, theme, LayoutVariant::SplitInspector, true);
                h.density = density;
                let game = &mut h.state.games[selected];
                game.preferences.saved.insert("Display", "Sharp pixels");
                let label = game.full_mode_label();
                assert!(!label.is_empty());
                game.previews.insert(
                    game.preview_variant().into(),
                    preview::sources::Source {
                        id: format!("synthetic-{selected}"),
                        frame: Arc::new(
                            Frame::new(
                                DisplayGeometry::new((8, 6), (4, 3)),
                                [75, 150, 220, 255].repeat(8 * 6),
                            )
                            .unwrap(),
                        ),
                    },
                );
                h.state.selected = Some(game.entry_id.clone());
                h.size = Vec2::new(650.0, 470.0);
                h.settle();
                let mut previous: Option<Rect> = None;
                // Sweep one logical pixel at a time across both former breakpoints.
                for width in 650..=675 {
                    h.size = Vec2::new(width as f32, 470.0);
                    let output = h.frame(vec![]);
                    let rect = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh)
                                if matches!(mesh.texture_id, egui::TextureId::Managed(id) if id > 0) =>
                            {
                                Some(mesh.calc_bounds())
                            }
                            _ => None,
                        })
                        .expect("Split preview remains visible");
                    assert!(
                        text_rect(&output, &label).is_some(),
                        "fitting badge disappeared: {theme:?} density {density}, width {width}, {label}"
                    );
                    if let Some(before) = previous {
                        assert!(
                            (rect.width() - before.width()).abs() <= 1.01
                                && (rect.height() - before.height()).abs() <= 1.01,
                            "Split preview jumped: {theme:?} density {density}, width {width}: {before:?} -> {rect:?}"
                        );
                    }
                    previous = Some(rect);
                }
            }
        }
    }
}
