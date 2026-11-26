// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Entry settings configure subsequent launches; launcher appearance is separate.
use super::*;
use reforge_sdl::settings::{SettingsAvailability, SettingsMenu, VideoCapabilities};

pub fn run(
    canvas: &mut WindowCanvas,
    gui: &mut Gui<'_>,
    events: &mut sdl3::EventPump,
    pads: &mut Gamepads,
    state: &mut State,
) -> Result<bool, String> {
    let game = state.selected_game().map(|g| g.game_id.clone());
    let entry_id = state.selected_game().map(|g| g.entry_id.clone());
    let native_result = entry_id
        .as_ref()
        .map(|g| reforge_settings::launcher::native_options(&state.config, g))
        .transpose();
    let (native, warning) = match native_result {
        Ok(native) => (native.unwrap_or_default(), None),
        Err(error) => (Default::default(), Some(error)),
    };
    let capabilities = state
        .selected_game()
        .and_then(|game| game.settings_capabilities(&native));
    let geometry = capabilities.map_or_else(
        || reforge_sdl::DisplayGeometry::new((320, 200), (320, 240)),
        |capabilities| capabilities.display.geometry,
    );
    let guard = reforge_settings::Launch {
        directory: state.selected_game().map(|game| game.directory.clone()),
        game_id: game.clone().unwrap_or_else(|| "standalone".into()),
        entry_id,
        path: Some(state.config.clone()),
        cli: Default::default(),
    }
    .install();
    let creator = FrameCreator::for_ui_with_geometry(canvas, geometry);
    drop(guard);
    let mut menu = match capabilities {
        Some(capabilities) => {
            use std::io::IsTerminal;
            SettingsMenu::for_game(
                &creator,
                capabilities,
                SettingsAvailability {
                    terminal: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
                    ..SettingsAvailability::default()
                },
            )
            .with_launcher_options()
        }
        None => SettingsMenu::new(&creator, VideoCapabilities::default()).with_launcher_only(),
    };
    menu.set_controller_style(pads.style());
    menu.set_theme(catalog::ResolvedAppearance::resolve(state.saved_theme).theme);
    menu.set_status("Changes apply when a game starts.".into());
    menu.toggle();
    if let Some(error) = warning {
        menu.set_error(error);
    }
    while menu.is_open() {
        for event in events.poll_iter() {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) {
                return Ok(true);
            }
            if let Some(pad) = pads.handle(&event) {
                menu.handle_gamepad(pad);
            }
            menu.handle_event(&event);
        }
        let metrics = Gui::metrics(canvas)?;
        if menu.prepare(metrics, Default::default()).is_some() {
            menu.applied();
            state.reload_preferences();
        }
        let now = Instant::now();
        gui.prepare(metrics, |ctx| {
            preview::begin_frame(ctx, metrics, now);
            layouts::draw(ctx, state);
        });
        let bg = catalog::ResolvedAppearance::resolve(state.saved_theme)
            .theme
            .palette
            .canvas;
        canvas.set_draw_color(sdl3::pixels::Color::RGB(bg.r(), bg.g(), bg.b()));
        canvas.clear();
        gui.paint(canvas)?;
        if menu.is_open() {
            menu.paint(canvas)?;
        }
        canvas.present();
        std::thread::sleep(Duration::from_millis(16));
    }
    state.settings_open = false;
    state.refresh();
    text_input(canvas, false);
    gui.reset_input();
    Ok(false)
}
