// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Production UI regressions use authored library records, never original data.
use super::*;

mod lifecycle;
mod palettes;
mod split_height;
use model::EntryStatus;
use reforge_sdl::egui::{Context, Pos2, RawInput, Rect, Vec2};
use reforge_sdl::ui::themes::{ResolvedAppearance, ThemeId};
use reforge_settings::{Launch, Session, library::Library};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn loaded_game_entry(entry: &Entry, config: PathBuf, verify: bool) -> model::GameEntry {
    let mut game = model::GameEntry::from_entry(entry, config);
    if verify {
        game.apply_verification(&verify_entry(entry));
    } else {
        game.status = EntryStatus::Verified;
    }
    game.reload_preferences();
    game
}

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[test]
fn launcher_settings_share_engine_declarations_and_keep_host_availability_separate() {
    let fixture = Fixture::for_releases(detection::RELEASES.iter());
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, false);
    let declaration = |options: &[reforge_sdl::launcher::OptionDefinition]| {
        options
            .iter()
            .map(|option| {
                (
                    option.label,
                    option.storage,
                    option.default,
                    option.apply,
                    option
                        .choices
                        .iter()
                        .map(|choice| (choice.value, choice.label))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    for game in &mut state.games {
        let capabilities = game.settings_capabilities(&Default::default()).unwrap();
        assert_eq!(
            declaration(game.options),
            declaration(capabilities.quick_options)
        );
        assert_eq!(capabilities.game_id, game.game_id);
        if capabilities.display.frontend.is_some() {
            game.terminal_available = false;
            assert!(!game.choice_enabled("Renderer", "Terminal"));
            let unavailable = game.settings_capabilities(&Default::default()).unwrap();
            game.terminal_available = true;
            assert!(game.choice_enabled("Renderer", "Terminal"));
            let available = game.settings_capabilities(&Default::default()).unwrap();
            assert_eq!(
                declaration(unavailable.quick_options),
                declaration(available.quick_options)
            );
            assert_eq!(
                unavailable.display.frontend.unwrap().storage,
                available.display.frontend.unwrap().storage
            );
        }
    }
    // Unrecognized library records cannot borrow another game's settings.
    let game = &mut state.games[0];
    game.release_id = "unrecognized-release".into();
    assert!(game.settings_capabilities(&Default::default()).is_none());
}

#[test]
fn library_refresh_retains_cached_previews_when_originals_become_unavailable() {
    let fixture = Fixture::new(1);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let image = reforge_sdl::preview::Frame::new(
        reforge_sdl::DisplayGeometry::new((2, 1), (4, 3)),
        vec![255; 8],
    )
    .unwrap();
    state.games[0].previews.insert(
        "cga".into(),
        preview::sources::Source::new("cached-source".into(), std::sync::Arc::new(image)),
    );
    state.games[0].preview_attempted = true;
    let entry = state.library.entries[0].clone();
    state.prepare_play(entry.entry_id).unwrap_err();
    state.refresh();
    assert!(state.games[0].status.is_missing());
    assert_eq!(state.games[0].previews["cga"].id, "cached-source");
    assert!(state.games[0].preview_attempted);
    assert!(state.games[0].preview_verification.is_none());
}

#[test]
fn play_performs_full_verification_even_when_the_library_status_is_verified() {
    let fixture = Fixture::new(1);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let entry = state.library.entries[0].clone();
    let file = detection::RELEASES[0].files[0];
    std::fs::create_dir(&entry.original).unwrap();
    // Correct filename and size, but authored bytes with an incorrect hash.
    std::fs::File::create(entry.original.join(file.name))
        .unwrap()
        .set_len(file.size)
        .unwrap();
    assert_eq!(state.games[0].status, EntryStatus::Verified);
    let error = state.prepare_play(entry.entry_id).unwrap_err();
    assert!(error.contains("BLAKE3 mismatch"), "{error}");
    assert!(state.games[0].status.is_missing());
}

#[cfg(unix)]
#[test]
fn library_file_symlinks_keep_the_directory_used_for_companion_discovery() {
    let fixture = Fixture::new(0);
    let outside = fixture.directory.join("elsewhere");
    std::fs::create_dir(&outside).unwrap();
    let target = outside.join("program");
    std::fs::write(&target, b"authored external target").unwrap();
    let link = fixture.directory.join("start");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let release = &detection::RELEASES[0];
    let detected = detection::Detected {
        game: release.game(),
        release,
        original: link.clone(),
        root: fixture.directory.clone(),
        files: Default::default(),
    };
    let selected = library_original(&link, &detected).unwrap();
    assert_eq!(
        selected,
        fixture.directory.canonicalize().unwrap().join("start")
    );
    assert_ne!(selected, target.canonicalize().unwrap());
}

#[cfg(unix)]
#[test]
fn library_subdirectory_symlinks_keep_the_verified_game_root() {
    let fixture = Fixture::new(0);
    let target = fixture.directory.join("external");
    let root = fixture.directory.join("game");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("rooms"), b"authored data").unwrap();
    std::os::unix::fs::symlink(&target, root.join("DATA")).unwrap();
    let alias = fixture.directory.join("game-alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let selected = alias.join("DATA/rooms");
    let release = &detection::RELEASES[0];
    let detected = detection::Detected {
        game: release.game(),
        release,
        original: selected.clone(),
        root: alias,
        files: Default::default(),
    };
    assert_eq!(
        library_original(&selected, &detected).unwrap(),
        root.join("DATA/rooms")
    );
}

#[test]
fn library_round_trip_retains_an_explicit_file_and_binds_its_directory_profile() {
    let fixture = Fixture::new(0);
    let selected = fixture.directory.join("DATA/start.bin");
    std::fs::create_dir(selected.parent().unwrap()).unwrap();
    std::fs::write(&selected, b"authored location fixture").unwrap();
    let release = detection::RELEASES
        .iter()
        .find(|release| release.game() == detection::Game::Mach3)
        .unwrap();
    let detected = detection::Detected {
        game: release.game(),
        release,
        original: fixture.directory.clone(),
        root: fixture.directory.clone(),
        files: Default::default(),
    };
    let original = library_original(&selected, &detected).unwrap();
    let mut library =
        Library::new(fixture.library.clone()).with_storage(fixture.config.clone(), None);
    let id = library
        .add_at("mach-3", release.id, "Mach 3", &original, &detected.root)
        .unwrap();
    library.reload().unwrap();
    assert_eq!(
        library.entries[0].original,
        selected.canonicalize().unwrap()
    );
    assert_eq!(
        library.entries[0].directory,
        fixture.directory.canonicalize().unwrap()
    );
    let mut launch = Launch {
        game_id: "mach-3".into(),
        entry_id: None,
        path: Some(fixture.config.clone()),
        cli: Default::default(),
    };
    crate::profile::bind(&mut launch, &detected, &mut Vec::new()).unwrap();
    assert_eq!(launch.entry_id.as_deref(), Some(id.as_str()));
    let replacement = fixture.directory.join("replacement");
    std::fs::create_dir_all(replacement.join("DATA")).unwrap();
    let anchor = replacement.join("DATA/moved");
    std::fs::write(&anchor, b"authored relocated file").unwrap();
    library
        .relocate_at(&id, release.id, &anchor, &replacement)
        .unwrap();
    library.reload().unwrap();
    assert_eq!(library.entries[0].original, anchor);
    assert_eq!(library.entries[0].directory, replacement);
    assert_eq!(library.entries[0].entry_id, id);
    let relocated = detection::Detected {
        original: replacement.clone(),
        root: replacement,
        ..detected
    };
    launch.entry_id = None;
    crate::profile::bind(&mut launch, &relocated, &mut Vec::new()).unwrap();
    assert_eq!(launch.entry_id.as_deref(), Some(id.as_str()));
}

#[test]
fn alleycat_settings_and_launcher_share_machine_without_turning_saved_pins_into_cli_locks() {
    for release in detection::RELEASES
        .iter()
        .filter(|release| release.game() == detection::Game::AlleyCat)
    {
        let fixture = Fixture::for_releases([release]);
        let mut library = Library::new(fixture.library.clone());
        library.reload().unwrap();
        let entry = &library.entries[0];
        let mut game = model::GameEntry::from_entry(entry, fixture.config.clone());
        game.reload_preferences();
        game.save_option("Machine", "IBM PCjr");
        let expected = if game.options[0].default == "pcjr" {
            Vec::new()
        } else {
            vec!["--machine", "pcjr"]
        };
        assert_eq!(game.native_arguments(), ["--machine", "pcjr"]);
        assert!(game.launch_arguments().is_empty());
        let mut launch = Launch {
            game_id: "alley-cat".into(),
            entry_id: None,
            path: Some(fixture.config.clone()),
            cli: Default::default(),
        };
        let detected = detection::Detected {
            game: detection::Game::AlleyCat,
            release,
            original: entry.original.clone(),
            root: entry.directory.clone(),
            files: Default::default(),
        };
        let mut forwarded = Vec::new();
        crate::profile::bind(&mut launch, &detected, &mut forwarded).unwrap();
        assert_eq!(
            forwarded,
            expected
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
        assert!(!launch.cli.contains_key(reforge_settings::ALLEYCAT_MACHINE));
        let mut session = Session::load(launch);
        let mut draft = session.saved.clone();
        draft.reset_section(0);
        session.commit(draft).unwrap();
        game.reload_preferences();
        assert_eq!(
            game.preview_variant(),
            alleycat::launcher::options(match release.engine() {
                detection::Engine::AlleyCat(format) => format,
                _ => unreachable!(),
            })[0]
                .default
        );
    }
}

#[test]
fn invalid_shared_fields_preserve_launcher_choices_and_cli_agreement() {
    for (game_id, key, choice) in [
        ("mach-3", "video", "mono"),
        ("alley-cat", "machine", "pcjr"),
    ] {
        let fixture = Fixture::new(detection::RELEASES.len());
        let mut library = Library::new(fixture.library.clone());
        library.reload().unwrap();
        let entry = library
            .entries
            .iter()
            .find(|e| e.game_id == game_id)
            .unwrap();
        std::fs::write(&fixture.config, format!(
            "schema_version = 2\n[global.controls]\nstick_dead_zone = 60\n[entries.{}.display]\nscale = 99\nwindow_mode = \"fullscreen\"\n[entries.{}.launcher]\n{key} = {choice:?}\n",
            entry.entry_id, entry.entry_id,
        )).unwrap();
        let game = loaded_game_entry(entry, fixture.config.clone(), false);
        assert!(game.error.is_some());
        assert_eq!(game.option_values()["Window"], "Fullscreen");
        let release = detection::RELEASES
            .iter()
            .find(|r| r.id == entry.release_id)
            .unwrap();
        let detected = detection::Detected {
            game: release.game(),
            release,
            original: entry.original.clone(),
            root: entry.directory.clone(),
            files: Default::default(),
        };
        let mut launch = Launch {
            game_id: game_id.into(),
            entry_id: None,
            path: Some(fixture.config.clone()),
            cli: Default::default(),
        };
        let mut args = Vec::new();
        crate::profile::bind(&mut launch, &detected, &mut args).unwrap();
        assert_eq!(
            game.native_arguments()
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
            args
        );
        assert!(args.contains(&choice.into()));
        // A complete read failure must not turn UI fallback values into CLI pins.
        std::fs::write(&fixture.config, "invalid = [").unwrap();
        let broken = loaded_game_entry(entry, fixture.config.clone(), false);
        assert!(broken.error.is_some());
        assert!(broken.native_arguments().is_empty());
        assert_eq!(
            std::fs::read_to_string(&fixture.config).unwrap(),
            "invalid = ["
        );
    }
}

#[test]
fn loading_preferences_does_not_require_a_write_lock() {
    let fixture = Fixture::new(detection::RELEASES.len());
    let mut library = Library::new(fixture.library.clone());
    library.reload().unwrap();
    let entry = library
        .entries
        .iter()
        .find(|entry| entry.game_id == "mach-3")
        .unwrap();
    let original = format!(
        "schema_version = 2\n[entries.{}.display]\nmotion = \"scene\"\nwindow_mode = \"fullscreen\"\n[entries.{}.launcher]\nvideo = \"mono\"\n",
        entry.entry_id, entry.entry_id
    );
    std::fs::write(&fixture.config, &original).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fixture.config.with_extension("lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    let game = loaded_game_entry(entry, fixture.config.clone(), false);
    assert!(game.error.is_none());
    assert_eq!(game.option_values()["Window"], "Fullscreen");
    assert_eq!(game.native_arguments(), ["--video", "mono"]);
    assert_eq!(std::fs::read_to_string(&fixture.config).unwrap(), original);
    drop(lock);
    let recovered = loaded_game_entry(entry, fixture.config.clone(), false);
    assert!(recovered.error.is_none());
    assert_eq!(recovered.native_arguments(), game.native_arguments());
}

#[test]
#[ignore = "owns SDL video; run serially with dummy/software"]
fn filter_activates_native_text_input_and_preserves_caret_navigation() {
    let fixture = Fixture::new(5);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let canvas = canvas(&video).unwrap();
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    let mut context = Context::default();
    let mut frame = |gui: &mut Gui<'_>, state: &mut State| {
        gui.prepare(Gui::metrics(&canvas).unwrap(), |ctx| {
            context = ctx.clone();
            state.draw(ctx, false);
        });
        text_input(&canvas, gui.wants_text_input());
    };
    // Native activation is checked before any synthetic TextInput is injected.
    for _ in 0..4 {
        frame(&mut gui, &mut state);
    }
    assert!(!video.text_input().is_active(canvas.window()));
    drop(frame);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new("launcher_filter")));
    let frame = |gui: &mut Gui<'_>, state: &mut State| {
        gui.prepare(Gui::metrics(&canvas).unwrap(), |ctx| {
            state.draw(ctx, false);
        });
        text_input(&canvas, gui.wants_text_input());
    };
    frame(&mut gui, &mut state);
    assert!(video.text_input().is_active(canvas.window()));
    gui.handle_event(&Event::TextInput {
        timestamp: 0,
        window_id: canvas.window().id(),
        text: "Cat".into(),
    });
    frame(&mut gui, &mut state);
    assert_eq!(state.filter_query, "Cat");
    gui.handle_event(&native_key(Keycode::Left));
    frame(&mut gui, &mut state);
    gui.handle_event(&Event::TextInput {
        timestamp: 0,
        window_id: canvas.window().id(),
        text: "é".into(),
    });
    frame(&mut gui, &mut state);
    assert_eq!(state.filter_query, "Caét");
    state.layout = LayoutVariant::GalleryShelf;
    frame(&mut gui, &mut state);
    assert!(!video.text_input().is_active(canvas.window()));
    state.layout = LayoutVariant::SplitInspector;
    frame(&mut gui, &mut state);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new("launcher_filter")));
    frame(&mut gui, &mut state);
    assert!(video.text_input().is_active(canvas.window()));
    text_input(&canvas, false); // Child/dialog handoff.
    assert!(!video.text_input().is_active(canvas.window()));
    gui.reset_input();
    frame(&mut gui, &mut state);
    context.memory_mut(|memory| memory.request_focus(egui::Id::new("launcher_filter")));
    frame(&mut gui, &mut state);
    assert!(video.text_input().is_active(canvas.window()));
    gui.handle_event(&Event::Window {
        timestamp: 0,
        window_id: canvas.window().id(),
        win_event: WindowEvent::FocusLost,
    });
    frame(&mut gui, &mut state);
    assert!(!video.text_input().is_active(canvas.window()));
}

#[test]
fn long_library_titles_elide_without_changing_rows_or_hiding_actions() {
    let fixture = Fixture::new(2);
    let titles = [
        "Brimstone: The Dream of Gawain".to_owned(),
        "A very long title with Unicode — Élan Ω and an exceptionally long subtitle".repeat(3),
    ];
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            for verified in [true, false] {
                for (size, density) in [
                    (Vec2::new(630.0, 470.0), 2.25),
                    (Vec2::new(1000.0, 720.0), 1.0),
                ] {
                    let mut h = Harness::new(&fixture, theme, layout, verified);
                    h.size = size;
                    h.density = density;
                    h.state.selected = Some(h.state.games[0].entry_id.clone());
                    h.state.games[0].title = "Short title".into();
                    h.state.games[1].title = "Another title".into();
                    let before = h.settle();
                    let row = text_rect(&before, "Another title").unwrap();
                    let action = if verified { "Play" } else { "Locate Files…" };
                    let action_rect = text_rect(&before, action).unwrap();
                    for (game, title) in h.state.games.iter_mut().zip(&titles) {
                        game.title.clone_from(title);
                    }
                    let output = h.settle();
                    let actual_action = text_rect(&output, action).unwrap();
                    assert!(Rect::from_min_size(Pos2::ZERO, size).contains(actual_action.center()));
                    if layout != LayoutVariant::SplitInspector {
                        assert_eq!(actual_action, action_rect, "{theme:?} {layout:?} {size:?}");
                    }
                    let second = text_rect(&output, &titles[1]).unwrap();
                    assert_eq!(row.top(), second.top(), "title changed row placement");
                    assert_eq!(row.height(), second.height(), "title changed row height");
                    let mut elided = 0;
                    let mut full_heading = false;
                    for shape in &output.shapes {
                        let egui::Shape::Text(text) = &shape.shape else {
                            continue;
                        };
                        if !titles.iter().any(|title| title == text.galley.text()) {
                            continue;
                        }
                        if text.galley.elided {
                            elided += 1;
                            assert_eq!(text.galley.rows.len(), 1);
                            assert_eq!(text.galley.rows[0].glyphs.last().unwrap().chr, '…');
                            let rect = Rect::from_min_size(text.pos, text.galley.size());
                            assert!(rect.left() >= shape.clip_rect.left() - 0.1);
                            assert!(
                                rect.right() <= shape.clip_rect.right() + 0.1,
                                "{theme:?} {layout:?} {size:?}: title={rect:?}, clip={:?}",
                                shape.clip_rect
                            );
                            assert!(rect.right() <= size.x);
                            if layout == LayoutVariant::CompactDeck {
                                for other in &output.shapes {
                                    let egui::Shape::Text(label) = &other.shape else {
                                        continue;
                                    };
                                    if h.state.games.iter().any(|game| {
                                        label.galley.text()
                                            == if verified {
                                                game.full_mode_label()
                                            } else {
                                                "Missing files".into()
                                            }
                                    }) {
                                        let label_rect =
                                            Rect::from_min_size(label.pos, label.galley.size());
                                        assert!(
                                            !rect.intersects(label_rect),
                                            "title overlaps Compact mode/status label"
                                        );
                                    }
                                }
                            }
                        } else if text.galley.text() == titles[0] {
                            full_heading = true;
                        }
                    }
                    assert!(elided > 0, "{theme:?} {layout:?} {size:?}");
                    if layout == LayoutVariant::SplitInspector {
                        assert!(full_heading, "detail heading must keep the full title");
                    }
                    assert_eq!(h.state.games[0].title, titles[0]);
                    assert_eq!(h.state.games[1].title, titles[1]);
                }
            }
        }
    }
}

#[test]
#[ignore = "owns SDL video; run alone with SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software"]
fn long_titles_native_painter() {
    use reforge_sdl::settings::UiMetrics;
    let fixture = Fixture::new(2);
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let capture = std::env::var_os("REFORGE_LAUNCHER_CAPTURE").map(PathBuf::from);
    if let Some(directory) = &capture {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (size, density) in [((630, 470), 2.25_f32), ((1000, 720), 1.0_f32)] {
        let pixels = (
            (size.0 as f32 * density).round() as u32,
            (size.1 as f32 * density).round() as u32,
        );
        let window = video
            .window("Long library titles", pixels.0, pixels.1)
            .hidden()
            .build()
            .unwrap();
        let mut canvas = window.into_canvas();
        let creator = FrameCreator::for_ui(&canvas);
        let mut gui = Gui::new(&creator);
        gui.set_focus_overlay(false);
        gui.set_theme(ResolvedAppearance::resolve(ThemeId::CrispLight).theme);
        let metrics = UiMetrics {
            drawable: pixels,
            window: pixels,
            pixels_per_point: density,
        };
        for layout in LayoutVariant::ALL {
            let mut state = fixture.state(ThemeId::CrispLight, layout, true);
            state.games[0].title = "Brimstone: The Dream of Gawain".into();
            state.games[1].title =
                "A very long title with Unicode — Élan Ω and an exceptionally long subtitle"
                    .repeat(3);
            state.selected = Some(state.games[0].entry_id.clone());
            for _ in 0..4 {
                native_frame(&mut gui, &mut canvas, &mut state, metrics);
            }
            if let Some(directory) = &capture {
                canvas
                    .read_pixels(None)
                    .unwrap()
                    .save_bmp(directory.join(format!(
                        "{}-{}x{}.bmp",
                        layout.slug(),
                        size.0,
                        size.1
                    )))
                    .unwrap();
            }
        }
    }
}

#[test]
fn close_restores_selected_entry_without_changing_library_order() {
    for layout in LayoutVariant::ALL {
        // Two copies of Alley Cat distinguish entry identity from game identity.
        let fixture = Fixture::new(4);
        let mut state = fixture.state(ThemeId::ClassicDark, layout, false);
        let order: Vec<_> = state.games.iter().map(|g| g.entry_id.clone()).collect();
        state.selected = Some(order[3].clone());
        state.save_on_close().unwrap();
        let mut restored = State::new(fixture.library.clone(), fixture.config.clone());
        assert_eq!(restored.selected.as_ref(), Some(&order[3]));
        assert_eq!(
            restored
                .games
                .iter()
                .map(|g| &g.entry_id)
                .collect::<Vec<_>>(),
            order.iter().collect::<Vec<_>>()
        );
        restored.library.remove(&order[3]).unwrap();
        restored = State::new(fixture.library.clone(), fixture.config.clone());
        assert_eq!(restored.selected.as_ref(), Some(&order[0]));
        restored.save_on_close().unwrap();
        assert_eq!(
            reforge_settings::launcher::load(&fixture.config)
                .unwrap()
                .selected_entry,
            Some(order[0].clone())
        );
        for id in &order[..3] {
            restored.library.remove(id).unwrap();
        }
        restored.refresh();
        restored.save_on_close().unwrap();
        let empty = State::new(fixture.library.clone(), fixture.config.clone());
        assert_eq!(empty.selected, None);
        assert_eq!(
            reforge_settings::launcher::load(&fixture.config)
                .unwrap()
                .selected_entry,
            None
        );
    }
}

#[test]
fn unreadable_library_does_not_erase_saved_selection_on_close() {
    let fixture = Fixture::new(2);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, false);
    state.selected = Some(state.games[1].entry_id.clone());
    state.save_on_close().unwrap();
    std::fs::write(&fixture.library, "invalid = [").unwrap();
    let mut restored = State::new(fixture.library.clone(), fixture.config.clone());
    restored.save_on_close().unwrap();
    assert_eq!(restored.selected, state.selected);
    assert_eq!(
        reforge_settings::launcher::load(&fixture.config)
            .unwrap()
            .selected_entry,
        state.selected
    );
}

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
        Self::for_releases(
            (0..count).map(|index| &detection::RELEASES[index % detection::RELEASES.len()]),
        )
    }
    fn for_releases(releases: impl IntoIterator<Item = &'static detection::Release>) -> Self {
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
        // Match the library's root spelling, including Windows long/verbatim
        // paths when temp_dir() returns a short-name alias such as RUNNER~1.
        let directory = directory.canonicalize().unwrap();
        let fixture = Self {
            library: directory.join("library.toml"),
            config: directory.join("config.toml"),
            directory,
        };
        let mut library = Library::new(fixture.library.clone());
        for (index, release) in releases.into_iter().enumerate() {
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
    let game = loaded_game_entry(&entry, fixture.config.clone(), true);
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
                            labels.push("Relocate…");
                            labels.push(if *verified { "Play" } else { "Locate Files…" });
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
    let fixture = Fixture::new(detection::RELEASES.len());
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let mut h = Harness::new(&fixture, theme, layout, true);
            let output = h.settle();
            assert!(texts(&output).contains(&"AC"));
            if layout != LayoutVariant::SplitInspector {
                assert!(texts(&output).contains(&"M3"));
                assert!(texts(&output).contains(&"MI"));
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
            assert_eq!(game.gallery_palette_display(), game.palette_display());
            assert_eq!(game.native_arguments(), ["--machine", "pcjr"]);
            game.save_option("Machine", "IBM PC (CGA)");
            let game = &mut h.state.games[1];
            game.save_option("Video", "Monochrome");
            assert_eq!(game.palette_swatches(), &[[0, 0, 0], [255, 255, 255]]);
            assert_eq!(game.gallery_palette_display(), game.palette_display());
            assert_eq!(game.native_arguments(), ["--video", "mono"]);
            game.save_option("Video", "CGA");
            let game = h
                .state
                .games
                .iter_mut()
                .find(|game| game.game_id == "mindwheel")
                .unwrap();
            assert!(game.previews.is_empty());
            assert_eq!(game.palette_swatches(), &btz::display::TEXT_PALETTE);
            game.terminal_available = true;
            game.save_option("Renderer", "Terminal");
            assert_eq!(game.palette_swatches(), &btz::display::TEXT_PALETTE);
        }
    }
}

#[test]
fn centurion_video_choices_previews_and_pins_follow_the_detected_release() {
    use centurion::assets::{DosRelease, LoadingProfile};
    for version in [DosRelease::V100A, DosRelease::V100B] {
        let releases = [false, true].map(|ega| {
            detection::RELEASES
                .iter()
                .find(|r| {
                    r.engine() == detection::Engine::Centurion(LoadingProfile::new(version, ega))
                })
                .unwrap()
        });
        let fixture = Fixture::for_releases(releases);
        let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
        let a = state
            .games
            .iter()
            .position(|g| g.release_id == releases[0].id)
            .unwrap();
        let b = state
            .games
            .iter()
            .position(|g| g.release_id == releases[1].id)
            .unwrap();
        assert_eq!(state.games[a].preview_variant(), "vga");
        assert_eq!(state.games[b].preview_variant(), "vga");
        assert!(
            !state.games[a]
                .option_choices()
                .iter()
                .any(|(label, _)| *label == "Video")
        );
        assert!(
            state.games[b]
                .option_choices()
                .iter()
                .any(|(label, choices)| *label == "Video" && choices.len() == 2)
        );
        let ega = state.games[b]
            .options
            .iter()
            .find(|o| o.label == "Video")
            .unwrap()
            .choices
            .iter()
            .find(|c| c.value == "ega")
            .unwrap()
            .label;
        state.games[b].save_option("Video", ega);
        assert_eq!(state.games[b].preview_variant(), "ega");
        assert_eq!(state.games[b].native_arguments(), ["--video", "ega"]);
        assert_eq!(state.games[a].preview_variant(), "vga");
        assert!(state.games[a].native_arguments().is_empty());
        let expected_id = state.games[b].entry_id.clone();
        let restored = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
        let game = restored
            .games
            .iter()
            .find(|g| g.entry_id == expected_id)
            .unwrap();
        assert_eq!(game.preview_variant(), "ega");
        assert_eq!(game.native_arguments(), ["--video", "ega"]);
    }
}

#[test]
fn gallery_mosaics_free_title_space_without_changing_card_geometry() {
    use reforge_sdl::{
        DisplayGeometry,
        launcher::{Palette, PaletteHint},
        preview::Frame,
    };
    use std::sync::Arc;

    let release = detection::RELEASES
        .iter()
        .find(|r| r.game().id() == "centurion-defender-of-rome")
        .unwrap();
    let fixture = Fixture::for_releases([release]);
    for count in [235, 250, 256] {
        let colours: Vec<_> = (0..count)
            .map(|n| [n as u8, (n as u8) ^ 0xa5, 17])
            .collect();
        let frame = Frame::new(DisplayGeometry::new((2, 1), (4, 3)), vec![255; 8])
            .unwrap()
            .with_palette(Palette {
                colours: colours.clone(),
                weights: None,
                hint: PaletteHint::Auto,
            })
            .unwrap();
        for theme in ThemeId::ALL {
            for density in [1.0, 1.5, 2.25] {
                for layout in LayoutVariant::ALL {
                    let mut h = Harness::new(&fixture, theme, layout, true);
                    h.density = density;
                    let game = &mut h.state.games[0];
                    game.previews.insert(
                        game.preview_variant().into(),
                        preview::sources::Source::new(
                            "synthetic-palette".into(),
                            Arc::new(frame.clone()),
                        ),
                    );
                    let display = if layout == LayoutVariant::GalleryShelf {
                        game.gallery_palette_display().into_owned()
                    } else {
                        game.palette_display().into_owned()
                    };
                    let expected_rows = if layout == LayoutVariant::GalleryShelf {
                        13
                    } else {
                        6
                    };
                    assert_eq!(display.rows, expected_rows);
                    assert_eq!(display.colours.len(), count / expected_rows * expected_rows);
                    let output = h.settle();
                    let cells: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|s| {
                            let (rect, colour) = match &s.shape {
                                egui::Shape::Rect(r) if r.rect.size() == Vec2::splat(2.0) => {
                                    (r.rect, r.fill)
                                }
                                egui::Shape::Circle(c) if c.radius == 1.0 => {
                                    (Rect::from_center_size(c.center, Vec2::splat(2.0)), c.fill)
                                }
                                _ => return None,
                            };
                            colours
                                .contains(&[colour.r(), colour.g(), colour.b()])
                                .then_some(rect)
                        })
                        .collect();
                    assert_eq!(
                        cells.len(),
                        display.colours.len(),
                        "{count} {layout:?} {theme:?}"
                    );
                    let bounds = cells.into_iter().reduce(|a, b| a.union(b)).unwrap();
                    assert_eq!(bounds.size(), widgets::palette_size(&display, 6.0));
                    if layout == LayoutVariant::GalleryShelf {
                        assert_eq!(bounds.height(), 38.0);
                        let title = text_rect(&output, "Centurion: Defender of Rome")
                            .expect("full title should fit");
                        assert!(title.right() <= bounds.left());
                        // Preview textures include a two-physical-pixel antialias margin.
                        let screen = output.shapes.iter().find_map(|s| match &s.shape {
                            egui::Shape::Mesh(m) if matches!(m.texture_id, egui::TextureId::Managed(id) if id > 0) => Some(m.calc_bounds()),
                            _ => None,
                        }).unwrap().shrink(2.0 / density);
                        assert!(
                            (bounds.top() - screen.bottom() - 7.0).abs() <= 1.0,
                            "{count} {theme:?} {density}: palette {bounds:?}, preview {screen:?}, title {title:?}"
                        );
                        // Original card metadata allocation is 46 points, leaving
                        // eight points below the taller 38-point mosaic.
                        assert!(
                            (screen.bottom() + 7.0 + 46.0 - bounds.bottom() - 8.0).abs() <= 1.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn all_engines_quick_options_fit_the_minimum_logical_window() {
    // Isolate option geometry from the number of catalogue distributions: a
    // programmatically selected offscreen row has not been scrolled into view.
    for (index, release) in detection::RELEASES.iter().enumerate() {
        let fixture = Fixture::for_releases([release]);
        for theme in ThemeId::ALL {
            for layout in LayoutVariant::ALL {
                let mut h = Harness::new(&fixture, theme, layout, true);
                h.size = Vec2::new(630.0, 470.0);
                h.state.selected = Some(h.state.games[0].entry_id.clone());
                h.state.options_open = true;
                let output = h.settle();
                let screen = Rect::from_min_size(Pos2::ZERO, h.size);
                for option in h.state.games[0].options {
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
fn quick_options_save_entry_choices_without_toggling_off() {
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let fixture = Fixture::new(2);
            let mut h = Harness::new(&fixture, theme, layout, true);
            h.state.options_open = true;
            h.state.games[0].save_option("Window", "Fullscreen");
            assert_eq!(
                fixture.session("alley-cat").saved.game["window_mode"],
                "fullscreen"
            );
            h.settle();
            h.click("Fullscreen");
            assert_eq!(
                fixture.session("alley-cat").saved.game["window_mode"],
                "fullscreen"
            );
            let mut session = fixture.session("alley-cat");
            let mut draft = session.saved.clone();
            draft.edit("master_volume", "65".into());
            session.commit(draft).unwrap();
            let reloaded = fixture.state(theme, layout, false);
            assert_eq!(reloaded.games[0].option_values()["Window"], "Fullscreen");
            assert_eq!(reloaded.games[1].option_values()["Window"], "Windowed");
            h.state.games[0].save_option("Window", "Windowed");
            h.settle();
            h.click("Windowed");
            assert!(!fixture.session("alley-cat").saved.game_values().fullscreen);
            assert_eq!(
                fixture
                    .session("alley-cat")
                    .saved
                    .game_values()
                    .audio
                    .volume,
                65
            );
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
fn btz_title_preview_is_shared_by_graphical_and_terminal_choices() {
    use reforge_sdl::{DisplayGeometry, preview::Frame};
    use std::sync::Arc;
    let fixture = Fixture::new(detection::RELEASES.len());
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::GalleryShelf, false);
    for game in state.games.iter_mut().filter(|game| {
        matches!(
            game.game_id.as_str(),
            "mindwheel" | "essex" | "brimstone" | "breakers"
        )
    }) {
        game.terminal_available = true;
        let frame = Frame::new_monochrome(
            DisplayGeometry::new((2, 1), (4, 3)),
            [170, 170, 170, 255].repeat(2),
        )
        .unwrap();
        game.previews.insert(
            btz::launcher::PREVIEW_VARIANT.into(),
            preview::sources::Source::new("original-title".into(), Arc::new(frame)),
        );
        for renderer in ["Terminal", "Graphical"] {
            game.save_option("Renderer", renderer);
            assert_eq!(game.preview_variant(), btz::launcher::PREVIEW_VARIANT);
            assert_eq!(game.previews.len(), 1);
            assert_eq!(game.previews[game.preview_variant()].id, "original-title");
        }
    }
}

#[test]
fn btz_frontend_selections_preserve_dormant_graphics() {
    // The active-mode preview badge can also say "Terminal". Target the
    // button beside Graphical rather than that noninteractive label.
    let click_terminal = |h: &mut Harness| {
        let output = h.settle();
        let graphical = text_rect(&output, "Graphical").unwrap();
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Terminal" => {
                    let rect = Rect::from_min_size(text.pos, text.galley.size());
                    ((rect.center().y - graphical.center().y).abs() < 4.0).then_some(rect.center())
                }
                _ => None,
            })
            .expect("Terminal renderer button");
        h.click_at(point);
    };
    for theme in ThemeId::ALL {
        for layout in LayoutVariant::ALL {
            let fixture = Fixture::new(detection::RELEASES.len());
            let mut session = fixture.session("mindwheel");
            let mut draft = session.saved.clone();
            draft.game.insert("renderer".into(), "gpu".into());
            draft.game.insert("window_mode".into(), "fullscreen".into());
            draft.game.insert("presentation".into(), "hyllian".into());
            session.commit(draft).unwrap();
            let original_graphics = session.saved.game;
            let mut h = Harness::new(&fixture, theme, layout, true);
            h.state.games.retain(|game| game.game_id == "mindwheel");
            h.state.selected = Some(h.state.games[0].entry_id.clone());
            h.state.options_open = true;
            h.state.games[0].terminal_available = true;
            h.click("Graphical");
            assert_eq!(fixture.session("mindwheel").saved.game, original_graphics);
            click_terminal(&mut h);
            h.state.games[0].refresh_preferences().unwrap();
            assert_eq!(h.state.games[0].option_values()["Renderer"], "Terminal");
            click_terminal(&mut h);
            assert_eq!(h.state.games[0].option_values()["Renderer"], "Terminal");
            h.click("Graphical");
            assert_eq!(fixture.session("mindwheel").saved.game, original_graphics);
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
    let release = detection::RELEASES
        .iter()
        .find(|release| release.game() == detection::Game::AlleyCat && release.files.len() == 2)
        .unwrap();
    let directory =
        PathBuf::from("/synthetic/a/very/long/game/location/that/does/not/fit/the/status/row");
    let entry = Entry {
        entry_id: "synthetic-copy".into(),
        game_id: "alley-cat".into(),
        release_id: release.id.into(),
        title: "Alley Cat".into(),
        original: directory.join("CAT.EXE"),
        directory: directory.clone(),
    };
    let mut game = model::GameEntry::from_entry(&entry, fixture.config.clone());
    assert_eq!(game.original_path, directory.display().to_string());
    assert_eq!(
        game.original, entry.original,
        "retain the selected file anchor"
    );
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
    for logical in [super::MIN_LOGICAL_SIZE, (1000, 720)] {
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
        "IBM PC (CGA)"
    );
    assert_eq!(state.games[0].option_values()["Window"], "Fullscreen");
    assert_eq!(
        state.games[duplicate_index].option_values()["Window"],
        "Windowed"
    );
    let mut renamed = state.library.entries[0].clone();
    renamed.title = "Alley Cat — PCjr copy".into();
    let entry = loaded_game_entry(&renamed, fixture.config.clone(), false);
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
#[ignore = "owns SDL video; optional REFORGE_PREVIEW_ORIGINALS supplies originals for two to six games; run serially"]
fn native_engine_previews() {
    use reforge_sdl::{DisplayGeometry, preview::Frame, settings::UiMetrics};
    use std::sync::Arc;
    let fixture = Fixture::new(4);
    let originals = std::env::var_os("REFORGE_PREVIEW_ORIGINALS")
        .map(|v| std::env::split_paths(&v).collect::<Vec<_>>());
    if let Some(paths) = &originals {
        assert!(
            (2..=6).contains(&paths.len()),
            "Alley Cat and Mach 3 originals required; optionally append BTZ games"
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
    if let Some(paths) = &originals {
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
        assert_eq!(state.games.len(), paths.len());
        let mut manager = preview::sources::Manager::new(Some(fixture.directory.join("previews")));
        let start = std::time::Instant::now();
        while state.games.iter().any(|game| !game.previews_complete()) {
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
                let inputs = detected.inputs();
                let again = match detected.engine() {
                    detection::Engine::AlleyCat(format) => {
                        alleycat::launcher::preview(&inputs, format, variant)
                    }
                    detection::Engine::Mach3 => mach3::launcher::preview(&inputs, variant),
                    detection::Engine::Centurion(release) => {
                        centurion::launcher::preview(&inputs, variant, release)
                    }
                    detection::Engine::Btz(release) => {
                        btz::launcher::preview(&inputs, release, variant)
                    }
                }
                .unwrap();
                assert_eq!(source.frame.rgba(), again.rgba());
                assert_eq!(source.frame.geometry(), again.geometry());
                assert_eq!(source.frame.is_monochrome(), again.is_monochrome());
                assert_eq!(source.frame.palette(), again.palette());
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
        additional.retain(|game| matches!(game.game_id.as_str(), "alley-cat" | "mach-3"));
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
                preview::sources::Source::new(format!("synthetic-{i}"), Arc::new(frame)),
            );
        }
    }
    state.selected = state
        .games
        .iter()
        .find(|game| game.game_id == "centurion-defender-of-rome")
        .or_else(|| state.games.first())
        .map(|game| game.entry_id.clone());
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
fn gallery_settings_render_three_cards_per_row_at_the_minimum() {
    use reforge_sdl::{DisplayGeometry, preview::Frame};
    use reforge_settings::launcher::{Edits, GalleryPreviewAspect};

    let fixture = Fixture::new(6);
    let mut h = Harness::new(
        &fixture,
        ThemeId::CrispLight,
        LayoutVariant::GalleryShelf,
        true,
    );
    h.size = Vec2::new(MIN_LOGICAL_SIZE.0 as f32, MIN_LOGICAL_SIZE.1 as f32);
    assert_eq!(h.state.gallery_preview_aspect, GalleryPreviewAspect::PerRow);
    let mut session = Session::load(Launch {
        game_id: "standalone".into(),
        path: Some(fixture.config.clone()),
        entry_id: None,
        cli: Default::default(),
    });
    for (game, ratio) in h.state.games.iter_mut().zip([
        (320, 180),
        (1920, 1080),
        (320, 240),
        (2560, 1080),
        (5120, 2160),
        (1280, 720),
    ]) {
        let frame = Frame::new(DisplayGeometry::new((2, 1), ratio), vec![255; 8]).unwrap();
        game.previews.insert(
            game.preview_variant().into(),
            preview::sources::Source::new(game.entry_id.clone(), std::sync::Arc::new(frame)),
        );
    }
    for (policy, expected_heights) in [
        (GalleryPreviewAspect::PerRow, [95.0, 71.0]),
        (GalleryPreviewAspect::WholeGallery, [95.0, 95.0]),
        (GalleryPreviewAspect::FourThree, [127.0, 127.0]),
    ] {
        session
            .commit_with_launcher(
                session.saved.clone(),
                Edits {
                    gallery_preview_aspect: Some(policy),
                    ..Edits::default()
                },
            )
            .unwrap();
        h.state.reload_preferences();
        let output = h.settle();
        let previews: Vec<_> = output.shapes.iter().filter_map(|s| match &s.shape {
            egui::Shape::Mesh(m) if matches!(m.texture_id, egui::TextureId::Managed(id) if id > 0) => Some(m.calc_bounds().shrink(2.0)),
            _ => None,
        }).collect();
        assert_eq!(previews.len(), 6);
        for (row, expected_height) in previews.as_chunks::<3>().0.iter().zip(expected_heights) {
            assert!(row.windows(2).all(|pair| pair[0].right() < pair[1].left()));
            for rect in row {
                assert_eq!(rect.top(), row[0].top());
                assert_eq!(rect.width(), 169.0);
                assert_eq!(rect.height(), expected_height);
                assert!(rect.min.x >= 0.0 && rect.max.x <= h.size.x);
                assert!(rect.bottom() + 53.0 <= h.state.content_bottom);
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
fn gallery_renders_roomier_cards_at_narrow_wide_and_maximized_sizes() {
    let fixture = Fixture::new(12);
    let mut h = Harness::new(
        &fixture,
        ThemeId::CrispLight,
        LayoutVariant::GalleryShelf,
        true,
    );
    for density in [1.0, 2.25] {
        h.density = density;
        for (width, height, expected_columns, expected_card_width) in [
            (657.0, 560.0, 3, 196.0),
            (831.0, 560.0, 4, 186.0),
            (1300.0, 560.0, 5, 218.0),
            (1360.0, 560.0, 6, 207.0),
            (1300.0, 720.0, 4, 292.0),
            (1707.0, 901.0, 4, 269.0),
        ] {
            h.size = Vec2::new(width, height);
            let output = h.settle();
            let previews: Vec<_> = output.shapes.iter().filter_map(|s| match &s.shape {
                egui::Shape::Mesh(m) if matches!(m.texture_id, egui::TextureId::Managed(id) if id > 0) => Some(m.calc_bounds().shrink(2.0)),
                _ => None,
            }).collect();
            assert!(
                previews.len() >= 2 * expected_columns,
                "at {width} × {height}"
            );
            if height == 901.0 {
                assert_eq!(
                    previews.len(),
                    12,
                    "maximized Gallery should show all previews"
                );
            }
            let columns: std::collections::BTreeSet<_> = previews
                .iter()
                .map(|rect| (rect.left() * density).round() as i32)
                .collect();
            assert_eq!(columns.len(), expected_columns, "at {width} × {height}");
            let card_width = previews[0].width() + 18.0;
            // The native chrome and font rounding determine the exact viewport
            // height available to the fitter at each display scale.
            assert!(
                (card_width - expected_card_width).abs() <= 4.0,
                "at {width} × {height}, density {density}: width {card_width}, expected {expected_card_width}"
            );
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
                    preview::sources::Source::new(
                        format!("synthetic-{selected}"),
                        Arc::new(
                            Frame::new(
                                DisplayGeometry::new((8, 6), (4, 3)),
                                [75, 150, 220, 255].repeat(8 * 6),
                            )
                            .unwrap(),
                        ),
                    ),
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
                            (rect.width() - before.width()).abs()
                                <= (0.75 * density).ceil() / density + 0.01
                                && (rect.height() - before.height()).abs()
                                    <= (0.5625 * density).ceil() / density + 0.01,
                            "Split preview jumped: {theme:?} density {density}, width {width}: {before:?} -> {rect:?}"
                        );
                    }
                    previous = Some(rect);
                }
            }
        }
    }
}

#[test]
fn centurion_video_pin_survives_relocating_between_graphics_sets() {
    use centurion::assets::{DosRelease, LoadingProfile};
    for version in [DosRelease::V100A, DosRelease::V100B] {
        let find = |ega| {
            detection::RELEASES
                .iter()
                .find(|r| {
                    r.engine() == detection::Engine::Centurion(LoadingProfile::new(version, ega))
                })
                .unwrap()
        };
        let full = find(true);
        let vga = find(false);
        let fixture = Fixture::for_releases([full]);
        let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
        let id = state.games[0].entry_id.clone();
        state.games[0].save_option("Video", "EGA");
        let entry = state.library.entries[0].clone();
        for release in [vga, full] {
            state
                .library
                .relocate_at(&id, release.id, &entry.original, &entry.directory)
                .unwrap();
            state.refresh();
            let game = &state.games[0];
            assert_eq!(game.entry_id, id);
            assert_eq!(
                game.preview_variant(),
                if release.id == full.id { "ega" } else { "vga" }
            );
            assert_eq!(
                game.native_arguments(),
                if release.id == full.id {
                    vec!["--video", "ega"]
                } else {
                    vec![]
                }
            );
            assert_eq!(
                reforge_settings::launcher::native_options(&fixture.config, &id).unwrap()["video"],
                "ega"
            );
        }
    }
}

#[test]
fn centurion_sound_pins_are_per_entry_and_leave_video_previews_unchanged() {
    let releases = detection::RELEASES
        .iter()
        .filter(|release| release.game() == detection::Game::Centurion)
        .collect::<Vec<_>>();
    let fixture = Fixture::for_releases(releases);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let selected = state.games[0].entry_id.clone();
    let preview = state.games[0].preview_variant();
    assert_eq!(state.games[0].option_values()["Sound"], "AdLib");
    state.games[0].save_option("Sound", "PC speaker");
    assert_eq!(state.games[0].preview_variant(), preview);
    assert!(state.games[0].native_arguments().is_empty());
    let restored = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    for game in &restored.games {
        assert_eq!(
            game.option_values()["Sound"],
            if game.entry_id == selected {
                "PC speaker"
            } else {
                "AdLib"
            }
        );
        assert_eq!(
            game.option_choices()
                .iter()
                .find(|(label, _)| *label == "Sound")
                .unwrap()
                .1
                .len(),
            2
        );
    }
}

#[test]
fn centurion_sound_settings_and_launcher_share_pins_without_cli_locks() {
    for release in detection::RELEASES
        .iter()
        .filter(|release| release.game() == detection::Game::Centurion)
    {
        let fixture = Fixture::for_releases([release]);
        let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
        let entry = state.library.entries[0].clone();
        let game = &mut state.games[0];
        let ega = game.options.iter().any(|option| option.label == "Video");
        if ega {
            game.save_option("Video", "EGA");
        }
        game.save_option("Sound", "PC speaker");
        let native = game.native_arguments();
        let carried = game.launch_arguments();
        assert!(!carried.iter().any(|arg| arg == "--sound"));
        assert_eq!(carried, native);
        if ega {
            assert_eq!(carried, ["--video", "ega"]);
        }
        let (mut launch, mut forwarded) = crate::launch_settings(
            carried.into_iter().map(Into::into).collect(),
            detection::Game::Centurion,
        );
        launch.path = Some(fixture.config.clone());
        crate::profile::bind(
            &mut launch,
            &detection::Detected {
                game: detection::Game::Centurion,
                release,
                original: entry.original.clone(),
                root: entry.directory.clone(),
                files: Default::default(),
            },
            &mut forwarded,
        )
        .unwrap();
        assert_eq!(
            forwarded,
            native
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
        assert!(!launch.cli.contains_key("audio_renderer"));
        let mut session = Session::load(launch);
        assert_eq!(session.saved.game["audio_renderer"], "speaker");
        let mut draft = session.saved.clone();
        draft.edit_game("audio_renderer", "adlib".into(), Some(&session.saved));
        session.commit(draft).unwrap();
        game.reload_preferences();
        assert_eq!(game.option_values()["Sound"], "AdLib");
        if ega {
            assert_eq!(game.option_values()["Video"], "EGA");
        }
    }
}

#[test]
fn centurion_sound_resolves_entry_defaults_without_rewriting() {
    use reforge_settings::Patch;
    let release = detection::RELEASES
        .iter()
        .find(|r| r.game() == detection::Game::Centurion)
        .unwrap();
    let fixture = Fixture::for_releases([release]);
    let mut state = fixture.state(ThemeId::ClassicDark, LayoutVariant::SplitInspector, true);
    let id = state.games[0].entry_id.clone();
    let mut session = Session::load(Launch {
        game_id: detection::Game::Centurion.id().into(),
        entry_id: Some(id.clone()),
        path: Some(fixture.config.clone()),
        cli: Patch::new(),
    });
    let mut draft = session.saved.clone();
    draft.edit("audio_renderer", "speaker".into());
    session.commit(draft).unwrap();
    state.games[0].reload_preferences();
    assert_eq!(state.games[0].option_values()["Sound"], "PC speaker");
    let mut draft = session.saved.clone();
    draft.game.insert("audio_renderer".into(), "default".into());
    session.commit(draft).unwrap();
    let before = std::fs::read(&fixture.config).unwrap();
    state.games[0].reload_preferences();
    assert_eq!(state.games[0].option_values()["Sound"], "AdLib");
    assert_eq!(std::fs::read(&fixture.config).unwrap(), before);
    let mut draft = session.saved.clone();
    draft.reset_section(2);
    session.commit(draft).unwrap();
    state.games[0].reload_preferences();
    assert_eq!(state.games[0].option_values()["Sound"], "AdLib");
}

#[test]
fn options_height_measurement_does_not_paint_above_the_drawer() {
    let fixture = Fixture::new(2);
    for theme in ThemeId::ALL {
        let appearance = ResolvedAppearance::resolve(theme);
        let context = Context::default();
        appearance.theme.apply_palette(&context);
        let mut game = fixture
            .state(theme, LayoutVariant::GalleryShelf, true)
            .games
            .remove(0);
        for width in [630.0, 1000.0] {
            for _ in 0..3 {
                let mut panel_top = 0.0;
                let output = context.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 720.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        widgets::draw_options_panel(ctx, &appearance, &mut game, "test_options");
                        panel_top = ctx.available_rect().bottom();
                    },
                );
                let mut labels = 0;
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let visible = Rect::from_min_size(text.pos, text.galley.size())
                            .intersect(shape.clip_rect);
                        if visible.is_positive() {
                            labels += 1;
                            assert!(
                                visible.top() >= panel_top,
                                "{theme:?} width={width}: {:?} escaped the drawer at {visible:?}, top={panel_top}",
                                text.galley.text()
                            );
                        }
                    }
                }
                assert!(labels > 0, "the real drawer must still paint its controls");
            }
        }
    }
}
