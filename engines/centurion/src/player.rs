// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Maintained shared SDL host; simulation and original input rules remain native.
use crate::{
    assets::{Assets, Result},
    cli::Options,
    session::{Input, Phase, Session},
};
use reforge_sdl::settings::{SettingsMenu, VideoCapabilities};
use reforge_sdl::{
    FrameFormat, FrameTexture, HostPause, HostShortcut, Presentation, Window, host_shortcut,
};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
};
use std::time::{Duration, Instant};

struct Cursor(sdl3::mouse::MouseUtil);
impl Drop for Cursor {
    fn drop(&mut self) {
        self.0.show_cursor(true);
    }
}

pub(crate) fn run(assets: &Assets, session: &mut Session, options: Options) -> Result<()> {
    let mut window = Window::new_with_geometry(
        &reforge_sdl::game_title("Centurion: Defender of Rome", None),
        crate::launcher::geometry(&Default::default()),
        options.scale,
        if options.hyllian {
            Presentation::Hyllian
        } else {
            Presentation::Pixels
        },
    )?;
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let cursor = Cursor(sdl.mouse());
    let creator = window.texture_creator();
    session.fleet_information = window.settings().centurion.fleet_information;
    session.popup_cursor = window.settings().centurion.popup_cursor;
    session.map_highlight = window.settings().centurion.map_highlight;
    session.set_map_hover_info(window.settings().centurion.map_hover_info)?;
    session.applied_value_indicator = window.settings().centurion.applied_value_indicator;
    session.set_menu_decorations(window.settings().centurion.menu_decorations);
    let mut texture = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgba)?;
    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
        .with_centurion_options()
        .with_startup_audio_renderers(crate::launcher::AUDIO_RENDERERS)
        .with_audio_categories(&[
            reforge_settings::SoundCategory::Music,
            reforge_settings::SoundCategory::Effects,
        ]);
    // Keep synthesis running when muted: the callback remains the shared game
    // clock, so changing volume never changes scene pacing or original cues.
    let sound = crate::audio::SoundDevice::from_renderer(window.settings().audio.renderer);
    session.sound_device = sound;
    let mut audio = match crate::audio_output::Output::new(sound) {
        Ok(output) => Some(output),
        Err(error) => {
            eprintln!("REforge: audio unavailable: {error}; continuing silently");
            None
        }
    };
    let mut gamepads = reforge_sdl::gamepad::Gamepads::new(window.settings().stick_dead_zone);
    menu.set_controller_style(gamepads.style());
    let mut selected_device = None;
    let mut focused = true;
    let mut manual_pause = false;
    let mut pause = HostPause::default();
    let started = Instant::now();
    let mut previous_tick = 0;
    let mut text_active = false;
    let mut remainder = (0.0, 0.0);
    let mut pointer_sync = crate::window::PointerSync::default();
    #[cfg(feature = "dev-tools")]
    let mut frames = 0;
    #[cfg(feature = "dev-tools")]
    let mut frozen = None;
    while !session.quit {
        let mut events: Vec<_> = window.poll_events().collect();
        #[cfg(feature = "dev-tools")]
        if options.smoke_frames.is_some() {
            // Exercise the real host handlers without injecting desktop input.
            let key = match frames {
                2 | 8 => Some(Keycode::F10),
                10 | 16 => Some(Keycode::Pause),
                _ => None,
            };
            if let Some(key) = key {
                use sdl3::keyboard::{Mod, Scancode};
                events.push(Event::KeyDown {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    raw: 0,
                    keycode: Some(key),
                    scancode: Some(if key == Keycode::F10 {
                        Scancode::F10
                    } else {
                        Scancode::Pause
                    }),
                    keymod: Mod::NOMOD,
                    repeat: false,
                });
            }
        }
        #[cfg(not(feature = "dev-tools"))]
        let _ = &mut events;
        let mut modal_batch = menu.is_open();
        let mut keyboard_pointer = false;
        for event in events {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) || host_shortcut(&event) == Some(HostShortcut::Quit)
            {
                session.input(assets, Input::Quit)?;
                continue;
            }
            if let Event::Window { win_event, .. } = &event {
                match win_event {
                    WindowEvent::FocusLost => {
                        focused = false;
                        session.set_map_pointer_active(false);
                        pointer_sync.clear();
                        release_controls(assets, session)?;
                    }
                    WindowEvent::FocusGained => focused = true,
                    WindowEvent::MouseLeave => session.set_map_pointer_active(false),
                    WindowEvent::MouseEnter => session.set_map_pointer_active(true),
                    _ => {}
                }
            }
            let pad = gamepads.handle(&event);
            if pad.is_some_and(|pad| menu.handle_gamepad(pad)) {
                modal_batch = true;
                continue;
            }
            if pad.is_some_and(|pad| pad.pause) {
                manual_pause = !manual_pause;
                continue;
            }
            // This shared variant keeps Escape for the game's original menus.
            if menu.handle_dialog_event(&event) {
                modal_batch = true;
                pointer_sync.clear();
                continue;
            }
            if host_shortcut(&event) == Some(HostShortcut::TogglePause) {
                manual_pause = !manual_pause;
                continue;
            }
            if modal_batch || manual_pause {
                continue;
            }
            match event {
                Event::KeyDown {
                    keycode: Some(key), ..
                } => {
                    if let Some(input) = crate::window::key_input(key as u32, session.phase) {
                        let previous = session.cursor;
                        session.input(assets, input)?;
                        keyboard_pointer |= session.cursor != previous;
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } if matches!(session.phase, Phase::Naval | Phase::Games) => {
                    let bits = match key {
                        Keycode::Up => 8,
                        Keycode::Down => 4,
                        Keycode::Left => 2,
                        Keycode::Right => 1,
                        _ => 0,
                    };
                    if bits != 0 {
                        session.input(assets, Input::ReleaseKey(bits))?;
                    }
                }
                Event::TextInput { text, .. } if text_active => {
                    session.input(assets, Input::Text(text))?
                }
                Event::MouseMotion {
                    x, y, xrel, yrel, ..
                } => {
                    if pointer_sync.synthetic_motion((x, y)) {
                        continue;
                    }
                    session.set_map_pointer_active(window.frame_point(x, y, (320, 200)).is_some());
                    let pending = pointer_sync.pending();
                    pointer_sync.real_motion();
                    if session.relative_steering() {
                        if let Some((dx, dy)) = relative_motion((x, y), (xrel, yrel), |x, y| {
                            window.frame_point(x, y, (320, 200))
                        }) {
                            let (dx, dy) = crate::window::mouse_delta(&mut remainder, dx, dy);
                            session.input(assets, Input::MouseDelta(dx, dy))?;
                        }
                    } else if keyboard_pointer || pending {
                        keyboard_pointer = true;
                        // These events were collected before the keyboard
                        // movement. Continue from the new game cursor, then
                        // align the host pointer after this batch is drained.
                        if let Some((dx, dy)) = relative_motion((x, y), (xrel, yrel), |x, y| {
                            window.frame_point(x, y, (320, 200))
                        }) {
                            let (dx, dy) = crate::window::mouse_delta(&mut remainder, dx, dy);
                            session.input(
                                assets,
                                Input::Mouse(session.cursor.0 + dx, session.cursor.1 + dy),
                            )?;
                        }
                    } else if let Some((x, y)) = window.frame_point(x, y, (320, 200)) {
                        session.input(assets, Input::Mouse(x.floor() as i32, y.floor() as i32))?;
                    }
                }
                Event::MouseButtonDown {
                    x, y, mouse_btn, ..
                } => {
                    let point = if (keyboard_pointer || pointer_sync.pending())
                        && !session.relative_steering()
                    {
                        Some((session.cursor.0 as f32, session.cursor.1 as f32))
                    } else {
                        window.frame_point(x, y, (320, 200))
                    };
                    if let Some((x, y)) = point {
                        let input = match mouse_btn {
                            MouseButton::Left => {
                                Some(Input::Click(x.floor() as i32, y.floor() as i32))
                            }
                            MouseButton::Right => {
                                Some(Input::RightClick(x.floor() as i32, y.floor() as i32))
                            }
                            _ => None,
                        };
                        if let Some(input) = input {
                            session.input(assets, input)?;
                        }
                    }
                }
                Event::MouseButtonUp {
                    x, y, mouse_btn, ..
                } => {
                    if session.relative_steering() {
                        let mask = match mouse_btn {
                            MouseButton::Left => 1,
                            MouseButton::Right => 2,
                            _ => 0,
                        };
                        if mask != 0 {
                            session.input(assets, Input::ReleaseButton(mask))?;
                        }
                    } else if mouse_btn == MouseButton::Left {
                        // A release outside the image still ends the existing
                        // drag, without moving its last valid destination.
                        session.input(
                            assets,
                            left_release(
                                (!(keyboard_pointer || pointer_sync.pending()))
                                    .then(|| window.frame_point(x, y, (320, 200)))
                                    .flatten(),
                                session.cursor,
                            ),
                        )?;
                    }
                }
                _ => {}
            }
        }
        let paused = manual_pause || menu.is_open();
        if paused != pause.is_paused() {
            pointer_sync.clear();
            pause.toggle(Instant::now());
            release_controls(assets, session)?;
            remainder = (0.0, 0.0);
        }
        if keyboard_pointer
            && focused
            && !paused
            && !session.relative_steering()
            && let Some(point) = window.warp_mouse_to_frame(session.cursor, (320, 200))
        {
            pointer_sync.warped(point);
        }
        if let Some(output) = &mut audio {
            let settings = window.settings().audio;
            output.set_host_audio(&settings, focused, options.mute)?;
            if settings.device != selected_device {
                match output.switch_device(settings.device.as_deref()) {
                    Ok(()) => selected_device = settings.device,
                    Err(error) => {
                        menu.set_error(error.clone());
                        eprintln!("REforge: {error}");
                        selected_device = settings.device;
                    }
                }
            }
            if paused {
                output.pump(assets, vec![], false, 0)?;
            }
        }
        // Freeze callback consumption before preparing a modal menu or changing
        // renderer/window geometry. Never drain original sound commands paused.
        #[cfg(feature = "dev-tools")]
        if options.smoke_frames.is_some() {
            let snapshot = (
                session.ticks,
                audio.as_mut().map(|o| o.clock_frames()).transpose()?,
            );
            if paused {
                if frozen.is_some_and(|previous| previous != snapshot) {
                    return Err("Host pause advanced simulation or audio consumption".into());
                }
                frozen = Some(snapshot);
            } else {
                frozen = None;
            }
        }
        window.prepare_settings(&mut menu);
        session.fleet_information = window.settings().centurion.fleet_information;
        session.popup_cursor = window.settings().centurion.popup_cursor;
        session.map_highlight = window.settings().centurion.map_highlight;
        session.set_map_hover_info(window.settings().centurion.map_hover_info)?;
        session.applied_value_indicator = window.settings().centurion.applied_value_indicator;
        if menu.is_open() {
            session.set_map_pointer_active(false);
        }
        session.set_menu_decorations(window.settings().centurion.menu_decorations);
        gamepads.set_dead_zone(window.settings().stick_dead_zone);
        if !paused {
            let frame = audio.as_mut().map(|o| o.clock_frames()).transpose()?;
            let now = frame.map_or_else(
                || {
                    crate::intro::clock_ticks(
                        pause.now(Instant::now()).duration_since(started).as_nanos(),
                    )
                },
                crate::audio_timeline::clock_ticks,
            );
            if now > previous_tick {
                session.input(assets, Input::Tick((now - previous_tick) as u32))?;
                previous_tick = now;
            }
            if let Some(output) = &mut audio {
                let consumed = frame.unwrap();
                let _ = output.take_completions(consumed)?;
                output.pump(
                    assets,
                    session.campaign.sound.drain(),
                    !session.quit,
                    consumed,
                )?;
            } else {
                session.campaign.sound.drain();
            }
        }
        if !session.relative_steering() && !pointer_sync.pending() {
            remainder = (0.0, 0.0);
        }
        let wants_text = !paused && session.phase == Phase::SaveDescription;
        if wants_text != text_active {
            if wants_text {
                window.start_text_input();
            } else {
                window.stop_text_input();
            }
            text_active = wants_text;
        }
        let frame = session.render(assets, true)?;
        texture.update(&crate::launcher::rgba(&frame, assets))?;
        if window.presentation_due() {
            window.present_with(&texture, |canvas| menu.paint(canvas))?;
        }
        cursor.0.show_cursor(menu.is_open() || !focused);
        #[cfg(feature = "dev-tools")]
        {
            frames += 1;
            if options.smoke_frames.is_some_and(|limit| frames >= limit) {
                println!(
                    "centurion-host-smoke: frames={frames} ticks={} settings={} paused={} audio={}",
                    session.ticks,
                    menu.is_open(),
                    pause.is_paused(),
                    audio.is_some()
                );
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    if text_active {
        window.stop_text_input();
    }
    Ok(())
}

fn relative_motion(
    current: (f32, f32),
    delta: (f32, f32),
    point: impl Fn(f32, f32) -> Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    let end = point(current.0, current.1)?;
    let start = point(current.0 - delta.0, current.1 - delta.1)?;
    Some((end.0 - start.0, end.1 - start.1))
}

fn left_release(point: Option<(f32, f32)>, last: (i32, i32)) -> Input {
    let (x, y) = point.map_or(last, |(x, y)| (x.floor() as i32, y.floor() as i32));
    Input::Release(x, y)
}

fn release_inputs(phase: Phase, cursor: (i32, i32)) -> Vec<Input> {
    match phase {
        Phase::Naval | Phase::Games => vec![Input::ReleaseKey(15), Input::ReleaseButton(3)],
        Phase::BattlePreparation => vec![left_release(None, cursor)],
        _ => vec![],
    }
}

fn release_controls(assets: &Assets, session: &mut Session) -> Result<()> {
    for input in release_inputs(session.phase, session.cursor) {
        session.input(assets, input)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_motion_uses_the_real_previous_point_at_image_edges() {
        let point = |x: f32, y: f32| {
            ((20.0..660.0).contains(&x) && (10.0..410.0).contains(&y))
                .then_some(((x - 20.0) / 2.0, (y - 10.0) / 2.0))
        };
        // Current and prior positions are inside. Extrapolating another delta
        // beyond current would incorrectly reject both of these edge events.
        assert_eq!(
            relative_motion((658.0, 408.0), (12.0, 8.0), point),
            Some((6.0, 4.0))
        );
        assert_eq!(
            relative_motion((22.0, 12.0), (-12.0, -8.0), point),
            Some((-6.0, -4.0))
        );
        assert_eq!(relative_motion((658.0, 408.0), (-12.0, -8.0), point), None);
    }

    #[test]
    fn out_of_image_release_and_focus_loss_finish_at_last_valid_battle_point() {
        assert!(matches!(
            left_release(None, (108, 58)),
            Input::Release(108, 58)
        ));
        assert!(matches!(
            left_release(Some((109.9, 59.2)), (108, 58)),
            Input::Release(109, 59)
        ));
        let inputs = release_inputs(Phase::BattlePreparation, (108, 58));
        assert!(matches!(inputs.as_slice(), [Input::Release(108, 58)]));
        assert!(release_inputs(Phase::Map, (108, 58)).is_empty());
    }
}
