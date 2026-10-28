// Copyright (c) 2026 analogued. Project license has not yet been selected.
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
                    release.game.id(),
                    release.id,
                    release.metadata.title,
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
            game: game.into(),
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
            assert_eq!(game.title, detection::RELEASES[0].metadata.title);
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
fn both_engines_quick_options_fit_the_minimum_logical_window() {
    let fixture = Fixture::new(2);
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            for index in 0..2 {
                let mut h = Harness::new(&fixture, theme, layout, true);
                h.size = Vec2::new(630.0, 470.0);
                h.state.selected = Some(h.state.games[index].id);
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
            assert_eq!(h.state.selected, Some(2));
            assert!(
                h.actions.is_empty(),
                "game selection launched: {theme:?} {layout:?}"
            );
            h.click("Play");
            assert_eq!(h.actions, [UiAction::Play(2)]);
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
            assert_eq!(h.state.pending_remove, Some(1));
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
            assert_eq!(library.entries[0].id, 2);
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
fn native_painter_capture_matrix_and_modal_navigation() {
    use reforge_sdl::settings::UiMetrics;
    let configuration = Fixture::new(0);
    let _guard = Launch {
        game: "standalone".into(),
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
                    state.request_remove(state.games[0].id);
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
