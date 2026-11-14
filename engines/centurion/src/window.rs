// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Minimal calls to the installed SDL3 C API; no SDL implementation is copied.
use std::ffi::{CStr, c_char, c_int, c_void};
use std::thread;
use std::time::{Duration, Instant};

use crate::assets::{Assets, Result};
use crate::graphics::{HEIGHT, WIDTH};
use crate::session::{Input, Phase, Session};

#[repr(C, align(8))]
struct Event {
    bytes: [u8; 128],
}

#[repr(C)]
#[derive(Default)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[link(name = "SDL3")]
unsafe extern "C" {
    fn SDL_GetError() -> *const c_char;
    fn SDL_CreateWindow(
        title: *const c_char,
        width: c_int,
        height: c_int,
        flags: u64,
    ) -> *mut c_void;
    fn SDL_DestroyWindow(window: *mut c_void);
    fn SDL_MaximizeWindow(window: *mut c_void) -> bool;
    fn SDL_RestoreWindow(window: *mut c_void) -> bool;
    fn SDL_SyncWindow(window: *mut c_void) -> bool;
    fn SDL_GetWindowFlags(window: *mut c_void) -> u64;
    fn SDL_GetWindowPixelDensity(window: *mut c_void) -> f32;
    fn SDL_GetWindowSize(window: *mut c_void, width: *mut c_int, height: *mut c_int) -> bool;
    fn SDL_GetWindowSizeInPixels(
        window: *mut c_void,
        width: *mut c_int,
        height: *mut c_int,
    ) -> bool;
    fn SDL_SetWindowSize(window: *mut c_void, width: c_int, height: c_int) -> bool;
    fn SDL_SetWindowMinimumSize(window: *mut c_void, width: c_int, height: c_int) -> bool;
    fn SDL_SetWindowPosition(window: *mut c_void, x: c_int, y: c_int) -> bool;
    fn SDL_SetWindowAspectRatio(window: *mut c_void, minimum: f32, maximum: f32) -> bool;
    fn SDL_CreateRenderer(window: *mut c_void, name: *const c_char) -> *mut c_void;
    fn SDL_DestroyRenderer(renderer: *mut c_void);
    fn SDL_SetRenderVSync(renderer: *mut c_void, vsync: c_int) -> bool;
    fn SDL_SetRenderLogicalPresentation(
        renderer: *mut c_void,
        width: c_int,
        height: c_int,
        mode: c_int,
    ) -> bool;
    fn SDL_CreateTexture(
        renderer: *mut c_void,
        format: u32,
        access: c_int,
        width: c_int,
        height: c_int,
    ) -> *mut c_void;
    fn SDL_DestroyTexture(texture: *mut c_void);
    fn SDL_SetTextureScaleMode(texture: *mut c_void, mode: c_int) -> bool;
    fn SDL_UpdateTexture(
        texture: *mut c_void,
        rect: *const c_void,
        pixels: *const c_void,
        pitch: c_int,
    ) -> bool;
    fn SDL_RenderClear(renderer: *mut c_void) -> bool;
    fn SDL_RenderTexture(
        renderer: *mut c_void,
        texture: *mut c_void,
        source: *const c_void,
        dest: *const c_void,
    ) -> bool;
    fn SDL_RenderPresent(renderer: *mut c_void) -> bool;
    fn SDL_GetRenderLogicalPresentationRect(renderer: *mut c_void, rect: *mut Rect) -> bool;
    fn SDL_PollEvent(event: *mut Event) -> bool;
    fn SDL_StartTextInput(window: *mut c_void) -> bool;
    fn SDL_StopTextInput(window: *mut c_void) -> bool;
    fn SDL_RenderCoordinatesFromWindow(
        renderer: *mut c_void,
        wx: f32,
        wy: f32,
        x: *mut f32,
        y: *mut f32,
    ) -> bool;
    fn SDL_HideCursor() -> bool;
    fn SDL_ShowCursor() -> bool;
}

struct Context {
    window: *mut c_void,
    renderer: *mut c_void,
    texture: *mut c_void,
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: pointers originate from SDL, are only owned here, and SDL
        // documents null destruction as valid. Run cleanup on the main thread.
        unsafe {
            SDL_ShowCursor();
            SDL_DestroyTexture(self.texture);
            SDL_DestroyRenderer(self.renderer);
            SDL_DestroyWindow(self.window);
        }
    }
}

fn error() -> String {
    // SAFETY: SDL_GetError returns a valid thread-local NUL-terminated string.
    unsafe {
        CStr::from_ptr(SDL_GetError())
            .to_string_lossy()
            .into_owned()
    }
}

fn fit_window(context: &Context, scale: Option<u32>) -> Result<()> {
    // SAFETY: this is the live main-thread SDL window and renderer. Queries
    // write only to initialized locals; size arithmetic is checked separately.
    unsafe {
        let multiplier = if let Some(number) = scale {
            number
        } else {
            SDL_MaximizeWindow(context.window);
            SDL_SyncWindow(context.window);
            SDL_RenderPresent(context.renderer);
            SDL_SyncWindow(context.window);
            if SDL_GetWindowFlags(context.window) & 0x80 != 0 {
                let (mut pw, mut ph, mut ww, mut wh) = (0, 0, 0, 0);
                if !SDL_GetWindowSizeInPixels(context.window, &mut pw, &mut ph)
                    || !SDL_GetWindowSize(context.window, &mut ww, &mut wh)
                {
                    return Err(error());
                }
                crate::window_geometry::fitted_scale(
                    (pw, ph),
                    (ww, wh),
                    SDL_GetWindowPixelDensity(context.window),
                )?
            } else {
                4
            }
        };
        if SDL_GetWindowFlags(context.window) & 0x80 != 0 {
            if !SDL_RestoreWindow(context.window) {
                return Err(error());
            }
            SDL_SyncWindow(context.window);
        }
        let density = SDL_GetWindowPixelDensity(context.window);
        let (width, height) = crate::window_geometry::window_units(multiplier, density)?;
        if !SDL_SetWindowSize(context.window, width, height) {
            return Err(error());
        }
        SDL_SyncWindow(context.window);
        let minimum = crate::window_geometry::window_units(1, density)?;
        SDL_SetWindowMinimumSize(context.window, minimum.0, minimum.1);
        SDL_SetWindowPosition(context.window, 0x2fff0000, 0x2fff0000);
        // Also constrain subsequent manual resizing, as requested. Retain SDL's
        // aspect-preserving presentation as a fallback if the compositor denies it.
        SDL_SetWindowAspectRatio(context.window, 4.0 / 3.0, 4.0 / 3.0);
        SDL_SyncWindow(context.window);
    }
    Ok(())
}

fn key_input(key: u32, phase: Phase) -> Option<Input> {
    match key {
        13 => Some(Input::Enter),
        43 if matches!(phase, Phase::Naval | Phase::Games) => Some(Input::Shortcut(b'+')),
        45 if phase == Phase::Games => Some(Input::Shortcut(b'-')),
        32 | 0x40000049 if phase == Phase::Naval => Some(Input::Space),
        32 if phase == Phase::Map => Some(Input::Space),
        32 if phase != Phase::SaveDescription => Some(Input::Enter),
        8 => Some(Input::Backspace),
        27 | 0x4000003a => Some(Input::Escape),
        0x40000052 => Some(Input::Up),
        0x40000051 => Some(Input::Down),
        0x40000050 => Some(Input::Left),
        0x4000004f => Some(Input::Right),
        97..=122 => Some(Input::Shortcut(key as u8)),
        49..=53 if phase == Phase::BattlePreparation => Some(Input::Shortcut(key as u8)),
        49..=51 if phase == Phase::LegionSelect => Some(Input::Shortcut(key as u8)),
        // The original BIOS input accepts nonzero characters and Insert.
        // These were previously omitted from the frontend's game-key mapping.
        9 | 32..=126 | 0x40000049
            if phase.is_startup()
                || matches!(
                    phase,
                    Phase::ProvinceList
                        | Phase::LegionList
                        | Phase::LegionMessage
                        | Phase::Diplomacy
                ) =>
        {
            Some(Input::Enter)
        }
        _ => None,
    }
}

#[derive(Default)]
pub struct Smoke {
    pub frames: Option<u32>,
    pub resize: Option<(i32, i32)>,
    pub maximize: bool,
    pub skip: bool,
}

fn mouse_delta(remainder: &mut (f32, f32), dx: f32, dy: f32) -> (i32, i32) {
    remainder.0 += dx;
    remainder.1 += dy;
    let whole = (remainder.0.trunc() as i32, remainder.1.trunc() as i32);
    remainder.0 -= whole.0 as f32;
    remainder.1 -= whole.1 as f32;
    whole
}

pub fn run(
    assets: &Assets,
    session: &mut Session,
    scale: Option<u32>,
    smoke: Smoke,
    mute: bool,
) -> Result<()> {
    // Keep Rust SDL owners alive through raw window destruction. Shared audio
    // owns another Sdl handle and must not become the last owner while this
    // window still exists.
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let _video = sdl.video().map_err(|e| e.to_string())?;
    // SAFETY: All SDL video and event operations remain on this main thread.
    // Event has SDL_Event's documented size/alignment. Pixel storage outlives
    // each synchronous upload; all other nullable C arguments are documented.
    unsafe {
        let mut context = Context {
            window: std::ptr::null_mut(),
            renderer: std::ptr::null_mut(),
            texture: std::ptr::null_mut(),
        };
        context.window = SDL_CreateWindow(
            c"Centurion: Defender of Rome - native prototype".as_ptr(),
            320 * scale.unwrap_or(4) as i32,
            240 * scale.unwrap_or(4) as i32,
            0x20 | 0x2000,
        );
        if context.window.is_null() {
            return Err(error());
        }
        context.renderer = SDL_CreateRenderer(context.window, std::ptr::null());
        if context.renderer.is_null() {
            return Err(error());
        }
        fit_window(&context, scale)?;
        SDL_SetRenderVSync(context.renderer, 0);
        if !SDL_SetRenderLogicalPresentation(context.renderer, 320, 240, 2) {
            return Err(error());
        }
        context.texture =
            SDL_CreateTexture(context.renderer, 0x16362004, 1, WIDTH as i32, HEIGHT as i32);
        if context.texture.is_null() {
            return Err(error());
        }
        if !SDL_SetTextureScaleMode(context.texture, 0) {
            return Err(error());
        }
        SDL_HideCursor(); // No capture/relative mode; absolute movement remains direct.
        let mut pixels = vec![0u32; WIDTH * HEIGHT];
        let mut frames = 0;
        let mut text_active = false;
        let mut mouse_remainder = (0.0f32, 0.0f32);
        let mut audio = if mute {
            None
        } else {
            match crate::audio_output::Output::new() {
                Ok(output) => Some(output),
                Err(message) => {
                    eprintln!(
                        "Audio unavailable: {message}; continuing silently (use --mute to disable audio)."
                    );
                    None
                }
            }
        };
        let started = Instant::now();
        let mut previous_tick = 0;
        let mut audio_completions = 0;
        while !session.quit {
            let audio_frame = audio
                .as_mut()
                .map(|output| output.clock_frames())
                .transpose()?;
            let now = audio_frame.map_or_else(
                || crate::intro::clock_ticks(started.elapsed().as_nanos()),
                crate::audio_timeline::clock_ticks,
            );
            if now != previous_tick {
                session.input(assets, Input::Tick((now - previous_tick) as u32))?;
                previous_tick = now;
            }
            if !session.relative_steering() {
                mouse_remainder = (0.0, 0.0);
            }
            let wants_text = session.phase == Phase::SaveDescription;
            if wants_text != text_active {
                if wants_text {
                    SDL_StartTextInput(context.window);
                } else {
                    SDL_StopTextInput(context.window);
                }
                text_active = wants_text;
            }
            let mut event = Event { bytes: [0; 128] };
            while SDL_PollEvent(&mut event) {
                let kind = u32::from_ne_bytes(event.bytes[..4].try_into().unwrap());
                match kind {
                    0x100 => session.input(assets, Input::Quit)?,
                    0x20a if SDL_GetWindowFlags(context.window) & 0x80 != 0 => {
                        fit_window(&context, None)?
                    }
                    0x206 if SDL_GetWindowFlags(context.window) & 0xc0 == 0 => {
                        let (mut width, mut height) = (0, 0);
                        if SDL_GetWindowSize(context.window, &mut width, &mut height)
                            && (width * 3 - height * 4).abs() > 4
                        {
                            let units = (width / 4).min(height / 3).max(1);
                            SDL_SetWindowSize(context.window, units * 4, units * 3);
                        }
                    }
                    0x20f if matches!(session.phase, Phase::Naval | Phase::Games) => {
                        session.input(assets, Input::ReleaseKey(15))?;
                    }
                    0x300 => {
                        let key = u32::from_ne_bytes(event.bytes[28..32].try_into().unwrap());
                        let input = key_input(key, session.phase);
                        #[cfg(feature = "dev-tools")]
                        let input = if key == 0x40000045 && session.recording_active() {
                            // SDL_KeyboardEvent.repeat is byte 37 in the installed
                            // SDL3 ABI. One marker per press, without gameplay input.
                            (event.bytes[37] == 0).then(|| Input::Mark("F12".into()))
                        } else {
                            input
                        };
                        if let Some(input) = input {
                            session.input(assets, input)?;
                        }
                    }
                    0x301 if matches!(session.phase, Phase::Naval | Phase::Games) => {
                        let key = u32::from_ne_bytes(event.bytes[28..32].try_into().unwrap());
                        let bits = match key {
                            0x40000052 => 8,
                            0x40000051 => 4,
                            0x40000050 => 2,
                            0x4000004f => 1,
                            _ => 0,
                        };
                        if bits != 0 {
                            session.input(assets, Input::ReleaseKey(bits))?;
                        }
                    }
                    0x303 if text_active => {
                        let pointer = usize::from_ne_bytes(
                            event.bytes[24..24 + std::mem::size_of::<usize>()]
                                .try_into()
                                .unwrap(),
                        );
                        if pointer != 0 {
                            let text = CStr::from_ptr(pointer as *const c_char)
                                .to_string_lossy()
                                .into_owned();
                            session.input(assets, Input::Text(text))?;
                        }
                    }
                    0x400..=0x402 => {
                        let wx = f32::from_ne_bytes(event.bytes[28..32].try_into().unwrap());
                        let wy = f32::from_ne_bytes(event.bytes[32..36].try_into().unwrap());
                        let (mut x, mut y) = (0.0, 0.0);
                        if SDL_RenderCoordinatesFromWindow(context.renderer, wx, wy, &mut x, &mut y)
                        {
                            if session.relative_steering() && kind == 0x400 {
                                let dx =
                                    f32::from_ne_bytes(event.bytes[36..40].try_into().unwrap());
                                let dy =
                                    f32::from_ne_bytes(event.bytes[40..44].try_into().unwrap());
                                let (mut end_x, mut end_y) = (0.0, 0.0);
                                if SDL_RenderCoordinatesFromWindow(
                                    context.renderer,
                                    wx + dx,
                                    wy + dy,
                                    &mut end_x,
                                    &mut end_y,
                                ) {
                                    let (dx, dy) = mouse_delta(
                                        &mut mouse_remainder,
                                        end_x - x,
                                        (end_y - y) * 200.0 / 240.0,
                                    );
                                    session.input(assets, Input::MouseDelta(dx, dy))?;
                                }
                                continue;
                            }
                            if session.relative_steering() && kind == 0x402 {
                                let mask = match event.bytes[24] {
                                    1 => 1,
                                    3 => 2,
                                    _ => 0,
                                };
                                session.input(assets, Input::ReleaseButton(mask))?;
                                continue;
                            }
                            let x = x.floor() as i32;
                            let y = (y * 200.0 / 240.0).floor() as i32;
                            let input = if kind == 0x402 && event.bytes[24] == 1 {
                                Input::Release(x, y)
                            } else if kind == 0x401 && event.bytes[24] == 1 {
                                Input::Click(x, y)
                            } else if kind == 0x401 && event.bytes[24] == 3 {
                                Input::RightClick(x, y)
                            } else {
                                Input::Mouse(x, y)
                            };
                            session.input(assets, input)?;
                        }
                    }
                    _ => {}
                }
            }
            // Internal smoke inputs exercise the same session path without
            // synthesizing any desktop keyboard/mouse events.
            if smoke.skip && matches!(frames, 10 | 20) {
                session.input(assets, Input::Enter)?;
            }
            if let Some(output) = &mut audio {
                audio_completions += output.take_completions(audio_frame.unwrap_or(0))?.len();
                output.pump(
                    assets,
                    session.campaign.sound.drain(),
                    !session.quit,
                    audio_frame.unwrap_or(0),
                )?;
            }
            let frame = session.render(assets, true)?;
            let palette = frame.palette(assets);
            for (destination, index) in pixels.iter_mut().zip(&frame.pixels) {
                let rgb = palette[*index as usize];
                *destination =
                    0xff000000 | ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32;
            }
            if !SDL_UpdateTexture(
                context.texture,
                std::ptr::null(),
                pixels.as_ptr().cast(),
                (WIDTH * 4) as i32,
            ) || !SDL_RenderClear(context.renderer)
                || !SDL_RenderTexture(
                    context.renderer,
                    context.texture,
                    std::ptr::null(),
                    std::ptr::null(),
                )
                || !SDL_RenderPresent(context.renderer)
            {
                return Err(error());
            }
            frames += 1;
            if frames == 1 {
                if let Some((width, height)) = smoke.resize {
                    SDL_SetWindowSize(context.window, width, height);
                }
                if smoke.maximize {
                    SDL_MaximizeWindow(context.window);
                }
            }
            if smoke.frames.is_some_and(|limit| frames >= limit) {
                break;
            }
            thread::sleep(Duration::from_millis(4));
        }
        if smoke.skip {
            let active = audio.as_ref().is_some_and(|output| output.is_active());
            println!("audio-check: phase={:?} active={active}", session.phase);
            if session.phase != Phase::Map || active == mute {
                return Err("Intro skip did not switch to map playback".into());
            }
        }
        if smoke.frames.is_some() {
            if let Some(output) = &mut audio {
                let consumed = output.clock_frames()?;
                let progress = output.progress()?;
                println!(
                    "audio-clock: consumed={consumed} rendered={} track={:?} started={} finished={:?} applied={} late_frames={} completions={audio_completions}",
                    progress.rendered,
                    progress.track,
                    progress.track_started,
                    progress.finished_at,
                    progress.applied,
                    progress.late_frames,
                );
            }
            if !smoke.skip {
                println!(
                    "audio-check: phase={:?} active={}",
                    session.phase,
                    audio.as_ref().is_some_and(|o| o.is_active())
                );
            }
            let (mut width, mut height) = (0, 0);
            let mut rect = Rect::default();
            if !SDL_GetWindowSizeInPixels(context.window, &mut width, &mut height)
                || !SDL_GetRenderLogicalPresentationRect(context.renderer, &mut rect)
            {
                return Err(error());
            }
            println!(
                "window-check: drawable={width}x{height} image={:.2},{:.2},{:.2},{:.2} maximized={}",
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                SDL_GetWindowFlags(context.window) & 0x80 != 0
            );
            if rect.x.abs() > 1.0
                || rect.y.abs() > 1.0
                || (rect.w - width as f32).abs() > 2.0
                || (rect.h - height as f32).abs() > 2.0
            {
                return Err("Image does not fill its 4:3 window".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_motion_retains_fractional_pixels_and_reversals() {
        let mut remainder = (0.0, 0.0);
        let mut total = (0, 0);
        for _ in 0..20 {
            let step = mouse_delta(&mut remainder, 0.25, -0.5);
            total.0 += step.0;
            total.1 += step.1;
        }
        assert_eq!(total, (5, -10));
        assert_eq!(remainder, (0.0, 0.0));
        assert_eq!(mouse_delta(&mut remainder, -0.75, 0.25), (0, 0));
        assert_eq!(mouse_delta(&mut remainder, 0.75, -0.25), (0, 0));
        assert_eq!(remainder, (0.0, 0.0));
    }

    #[test]
    fn battle_tactic_digits_reach_the_same_shortcut_path_as_menu_letters() {
        for key in 49..=53 {
            assert!(
                matches!(key_input(key, Phase::BattlePreparation), Some(Input::Shortcut(value)) if u32::from(value) == key)
            );
            assert!(key_input(key, Phase::Map).is_none());
        }
    }

    #[test]
    fn naval_arrow_and_boarding_keys_stay_distinct() {
        assert!(matches!(key_input(32, Phase::Naval), Some(Input::Space)));
        assert!(matches!(
            key_input(0x40000049, Phase::Naval),
            Some(Input::Space)
        ));
        assert!(matches!(key_input(13, Phase::Naval), Some(Input::Enter)));
        assert!(matches!(
            key_input(43, Phase::Naval),
            Some(Input::Shortcut(b'+'))
        ));
    }

    #[test]
    fn intro_accepts_digits_punctuation_tab_and_insert_without_changing_map_controls() {
        for key in [49, 63, 9, 0x40000049] {
            assert!(matches!(key_input(key, Phase::Intro), Some(Input::Enter)));
            assert!(matches!(
                key_input(key, Phase::Prologue),
                Some(Input::Enter)
            ));
            assert!(key_input(key, Phase::Map).is_none());
        }
        assert!(key_input(0x400000e1, Phase::Intro).is_none()); // Shift alone.
        assert!(key_input(0x4000003b, Phase::Intro).is_none()); // F2: BIOS AL=0.
    }
}
