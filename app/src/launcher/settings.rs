// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared Settings edits persist normally and configure subsequent game launches.
use super::*;
use reforge_sdl::settings::{SettingsMenu, VideoCapabilities};

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
    let geometry = match game.as_deref() {
        Some("alley-cat") => alleycat::launcher::geometry(&native),
        Some("mach-3") => mach3::launcher::geometry(&native),
        Some("mindwheel") => btz::windowed::GEOMETRY,
        _ => reforge_sdl::DisplayGeometry::new((320, 200), (320, 240)),
    };
    let guard = reforge_settings::Launch {
        game_id: game.clone().unwrap_or_else(|| "standalone".into()),
        entry_id,
        path: Some(state.config.clone()),
        cli: Default::default(),
    }
    .install();
    let creator = FrameCreator::for_ui_with_geometry(canvas, geometry);
    drop(guard);
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
    menu.set_controller_style(pads.style());
    use reforge_settings::{SoundCategory, SoundRenderer};
    menu = match game.as_deref() {
        Some("alley-cat") => menu
            .with_alleycat_options()
            .with_audio_renderers(&[SoundRenderer::Speaker])
            .with_audio_categories(&[SoundCategory::Music, SoundCategory::Effects]),
        Some("mach-3") => menu
            .with_mach3_options(None)
            .with_audio_renderers(&[SoundRenderer::Speaker, SoundRenderer::EnhancedPcm])
            .with_audio_categories(&SoundCategory::ALL),
        Some("mindwheel") => {
            use std::io::IsTerminal;
            menu.with_btz_options(std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
                .with_audio_renderers(&[SoundRenderer::Speaker])
                .with_audio_categories(&[SoundCategory::Effects])
        }
        _ => menu.with_global_defaults_only(),
    };
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
        }
        gui.prepare(metrics, |ctx| layouts::draw(ctx, state));
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
    gui.reset_input();
    Ok(false)
}
