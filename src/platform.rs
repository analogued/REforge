// Copyright (c) 2026 analogued
// The project license has not yet been selected.

//! SDL3 presentation and held-key input. Call from the program's main thread.
//!
//! The 320x200 framebuffer uses a 5:6 pixel aspect ratio on a 4:3 display.
//! Nearest-neighbor sampling keeps pixels sharp; resizing preserves that ratio.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::marker::PhantomData;
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::alley::Machine;
use crate::menu::MenuKey;

/// Host input sampled after draining SDL's event queue.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub alt: bool,
    pub quit: bool,
    pub pause_pressed: bool,
    pub restart_pressed: bool,
    pub continue_pressed: bool,
    pub menu_key: Option<MenuKey>,
    pub menu_pressed: bool,
    pub new_game_pressed: bool,
}

/// Host pause bookkeeping shared by the normal and diagnostic runners.
pub struct PauseClock {
    previous: Instant,
    paused_at: Option<Instant>,
}

pub struct PauseSample {
    pub paused: bool,
    pub changed: bool,
    pub elapsed: Duration,
    pub resumed_after: Option<Duration>,
}

impl PauseClock {
    pub fn new(now: Instant) -> Self {
        Self {
            previous: now,
            paused_at: None,
        }
    }

    pub fn sample(&mut self, now: Instant, input: Input) -> PauseSample {
        let mut resumed_after = None;
        let changed = if let Some(start) = self.paused_at {
            if input.pause_pressed {
                self.paused_at = None;
                resumed_after = Some(now.duration_since(start));
                true
            } else {
                false
            }
        } else if input.pause_pressed {
            self.paused_at = Some(now);
            true
        } else {
            false
        };
        let paused = self.paused_at.is_some();
        let elapsed = if paused || changed {
            Duration::ZERO
        } else {
            now.duration_since(self.previous)
        };
        self.previous = now;
        PauseSample {
            paused,
            changed,
            elapsed,
            resumed_after,
        }
    }
}

/// Original title and host resume hint, rendered with SDL's built-in font until
/// BIOS font support is available. This never modifies the game's framebuffer.
pub struct PauseOverlay {
    lines: [CString; 2],
    color: [u8; 4],
}

impl PauseOverlay {
    pub fn new(title: &str, machine: Machine) -> Result<Self, String> {
        Ok(Self {
            lines: [
                CString::new(title).map_err(|_| "pause text contains a NUL byte")?,
                c" Esc / P / Pause to continue...  ".to_owned(),
            ],
            color: machine.colors()[2],
        })
    }
}

static SDL_IN_USE: AtomicBool = AtomicBool::new(false);

struct Sdl {
    // SDL video and renderer operations, including destruction, stay on the
    // initializing thread. Rc makes the owning Window neither Send nor Sync.
    _main_thread: PhantomData<Rc<()>>,
}

impl Sdl {
    fn new() -> Result<Self, String> {
        SDL_IN_USE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "an SDL window is already active".to_owned())?;
        let sdl = Self {
            _main_thread: PhantomData,
        };
        // SAFETY: This module exclusively owns SDL's lifecycle. The caller is
        // on the program's main thread; no Window can migrate between threads.
        // SDL_SetMainReady supports Rust's ordinary entry point without SDL_main.
        let initialized = unsafe {
            ffi::SDL_SetMainReady();
            ffi::SDL_Init(ffi::INIT_VIDEO)
        };
        if !initialized {
            return Err(error("initialize SDL video"));
        }
        Ok(sdl)
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // SAFETY: All owned SDL objects are destroyed before this lifecycle
        // guard. SDL_Quit is also valid after a failed initialization.
        unsafe { ffi::SDL_Quit() };
        SDL_IN_USE.store(false, Ordering::Release);
    }
}

/// Owns a resizable window, renderer, and RGBA texture with 5:6 pixel aspect.
///
/// Construct, use, and drop this on the program's main thread. SDL is owned by
/// this backend; other code must not initialize or shut down SDL independently.
pub struct Window {
    window: *mut c_void,
    renderer: *mut c_void,
    texture: *mut c_void,
    pitch: c_int,
    byte_len: usize,
    quit: bool,
    _sdl: Sdl,
}

impl Window {
    pub fn new(title: &str, width: u32, height: u32, scale: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || scale == 0 {
            return Err("window dimensions and scale must be positive".to_owned());
        }
        let title = CString::new(title).map_err(|_| "window title contains a NUL byte")?;
        let source_width = c_int::try_from(width).map_err(|_| "source width is too large")?;
        let source_height = c_int::try_from(height).map_err(|_| "source height is too large")?;
        // 320x200 source pixels occupy a 320x240 square-pixel display area.
        // Correct presentation only; framebuffer coordinates remain unchanged.
        let display_height = height
            .checked_mul(6)
            .map(|value| value / 5)
            .filter(|value| *value > 0)
            .ok_or("aspect-corrected height is too large or zero")?;
        let logical_height =
            c_int::try_from(display_height).map_err(|_| "logical height is too large")?;
        let window_width = width
            .checked_mul(scale)
            .and_then(|value| c_int::try_from(value).ok())
            .ok_or("scaled window width is too large")?;
        let window_height = display_height
            .checked_mul(scale)
            .and_then(|value| c_int::try_from(value).ok())
            .ok_or("scaled window height is too large")?;
        let pitch = width
            .checked_mul(4)
            .and_then(|value| c_int::try_from(value).ok())
            .ok_or("RGBA row is too large")?;
        let byte_len = (pitch as usize)
            .checked_mul(height as usize)
            .ok_or("RGBA buffer is too large")?;

        // Partially constructed objects use the same cleanup as a live window.
        let mut result = Self {
            window: ptr::null_mut(),
            renderer: ptr::null_mut(),
            texture: ptr::null_mut(),
            pitch,
            byte_len,
            quit: false,
            _sdl: Sdl::new()?,
        };
        // SAFETY: SDL video is initialized; title is NUL-terminated and remains
        // alive for the call. Dimensions are positive C ints. Returned handles
        // are exclusively owned by result and checked before further use.
        unsafe {
            result.window = ffi::SDL_CreateWindow(
                title.as_ptr(),
                window_width,
                window_height,
                ffi::WINDOW_RESIZABLE,
            );
            if result.window.is_null() {
                return Err(error("create SDL window"));
            }
            if !ffi::SDL_SetWindowMinimumSize(result.window, source_width, logical_height) {
                return Err(error("set window minimum size"));
            }
            result.renderer = ffi::SDL_CreateRenderer(result.window, ptr::null());
            if result.renderer.is_null() {
                return Err(error("create SDL renderer"));
            }
            // Host presentation sync is independent of the virtual PIT/video
            // clocks. Unsupported renderers can still present without vsync.
            if !ffi::SDL_SetRenderVSync(result.renderer, 1) {
                eprintln!("{}; continuing without it", error("enable display vsync"));
            }
            if !ffi::SDL_SetRenderLogicalPresentation(
                result.renderer,
                source_width,
                logical_height,
                ffi::LOGICAL_PRESENTATION_LETTERBOX,
            ) {
                return Err(error("set aspect-corrected presentation"));
            }
            if !ffi::SDL_SetRenderDrawColor(result.renderer, 0, 0, 0, 255) {
                return Err(error("set letterbox color"));
            }
            result.texture = ffi::SDL_CreateTexture(
                result.renderer,
                ffi::PIXELFORMAT_RGBA32,
                ffi::TEXTUREACCESS_STREAMING,
                source_width,
                source_height,
            );
            if result.texture.is_null() {
                return Err(error("create SDL texture"));
            }
            if !ffi::SDL_SetTextureScaleMode(result.texture, ffi::SCALEMODE_NEAREST) {
                return Err(error("set nearest texture sampling"));
            }
        }
        Ok(result)
    }

    pub fn poll_input(&mut self) -> Input {
        let mut event = ffi::Event { bytes: [0; 128] };
        let mut presses = Input::default();
        // SAFETY: Event has SDL_Event's 128-byte storage and sufficient alignment
        // on supported desktop ABIs. PollEvent fills it; keyboard fields are
        // read only after checking its type. The pump updates held-key state.
        unsafe {
            while ffi::SDL_PollEvent(&mut event) {
                let kind = u32::from_ne_bytes(event.bytes[..4].try_into().unwrap());
                if kind == ffi::EVENT_QUIT || kind == ffi::EVENT_WINDOW_CLOSE_REQUESTED {
                    self.quit = true;
                }
                if kind == ffi::EVENT_KEY_DOWN {
                    let key = &*((&event as *const ffi::Event).cast::<ffi::KeyboardEvent>());
                    presses.record_press(key.scancode as usize, key.modifiers, key.repeat);
                }
            }
            let mut count = 0;
            let keys = ffi::SDL_GetKeyboardState(&mut count);
            let held = |index: usize| {
                // SDL owns count bools until shutdown. Index bounds are checked
                // before reading, and the state is consumed before the next pump.
                !keys.is_null() && index < count.max(0) as usize && *keys.add(index)
            };
            self.quit |= presses.quit;
            Input {
                up: held(ffi::SCANCODE_UP),
                down: held(ffi::SCANCODE_DOWN),
                left: held(ffi::SCANCODE_LEFT),
                right: held(ffi::SCANCODE_RIGHT),
                alt: held(ffi::SCANCODE_LALT) || held(ffi::SCANCODE_RALT),
                quit: self.quit,
                pause_pressed: presses.pause_pressed,
                restart_pressed: presses.restart_pressed,
                continue_pressed: presses.continue_pressed,
                menu_key: presses.menu_key,
                menu_pressed: presses.menu_pressed,
                new_game_pressed: presses.new_game_pressed,
            }
        }
    }

    /// Present tightly packed RGBA bytes matching the source dimensions.
    pub fn present(&mut self, rgba: &[u8]) -> Result<(), String> {
        self.present_with_pause(rgba, None)
    }

    pub fn present_with_pause(
        &mut self,
        rgba: &[u8],
        pause: Option<&PauseOverlay>,
    ) -> Result<(), String> {
        if rgba.len() != self.byte_len {
            return Err(format!(
                "expected {} RGBA bytes, received {}",
                self.byte_len,
                rgba.len()
            ));
        }
        // SAFETY: Handles remain live and exclusively owned. The checked slice
        // contains every texture row at pitch bytes each. SDL copies it during
        // UpdateTexture; null rectangles select the full texture/logical output.
        unsafe {
            if !ffi::SDL_UpdateTexture(self.texture, ptr::null(), rgba.as_ptr().cast(), self.pitch)
            {
                return Err(error("upload RGBA frame"));
            }
            if !ffi::SDL_RenderClear(self.renderer) {
                return Err(error("clear SDL renderer"));
            }
            if !ffi::SDL_RenderTexture(self.renderer, self.texture, ptr::null(), ptr::null()) {
                return Err(error("draw SDL texture"));
            }
            if let Some(pause) = pause {
                // Use original 320x200 coordinates in the 320x240 logical area.
                let rect = ffi::FRect {
                    x: 40.0,
                    y: 88.0,
                    w: 256.0,
                    h: 16.0,
                };
                if !ffi::SDL_SetRenderScale(self.renderer, 1.0, 1.2)
                    || !ffi::SDL_RenderFillRect(self.renderer, &rect)
                    || !ffi::SDL_SetRenderDrawColor(
                        self.renderer,
                        pause.color[0],
                        pause.color[1],
                        pause.color[2],
                        255,
                    )
                    || !ffi::SDL_RenderDebugText(self.renderer, 40.0, 88.0, pause.lines[0].as_ptr())
                    || !ffi::SDL_RenderDebugText(self.renderer, 40.0, 96.0, pause.lines[1].as_ptr())
                    || !ffi::SDL_SetRenderScale(self.renderer, 1.0, 1.0)
                    || !ffi::SDL_SetRenderDrawColor(self.renderer, 0, 0, 0, 255)
                {
                    return Err(error("draw pause overlay"));
                }
            }
            if !ffi::SDL_RenderPresent(self.renderer) {
                return Err(error("present SDL frame"));
            }
        }
        Ok(())
    }
}

impl Input {
    fn record_press(&mut self, scancode: usize, modifiers: u16, repeat: bool) {
        if scancode == ffi::SCANCODE_Q && modifiers & ffi::KMOD_CTRL != 0 {
            self.quit = true;
        }
        if !repeat && modifiers & ffi::KMOD_CTRL != 0 && modifiers & 0x0f00 == 0 {
            self.new_game_pressed |= scancode == ffi::SCANCODE_R;
            self.menu_pressed |= scancode == ffi::SCANCODE_M;
        }
        if !repeat && modifiers & ffi::KMOD_SHORTCUT == 0 {
            // Modifier keys alone and desktop shortcuts do not dismiss startup.
            self.continue_pressed |= !(224..=231).contains(&scancode);
            if !(224..=231).contains(&scancode) {
                // Keep the first fresh press so a queued second key cannot
                // replace a skill choice or skip the newly revealed controls.
                self.menu_key.get_or_insert(match scancode {
                    ffi::SCANCODE_K => MenuKey::Kitten,
                    ffi::SCANCODE_H => MenuKey::HouseCat,
                    ffi::SCANCODE_T => MenuKey::Tomcat,
                    ffi::SCANCODE_A => MenuKey::AlleyCat,
                    _ => MenuKey::Other,
                });
            }
            self.restart_pressed |= scancode == ffi::SCANCODE_R;
            self.pause_pressed |= matches!(
                scancode,
                ffi::SCANCODE_ESCAPE | ffi::SCANCODE_P | ffi::SCANCODE_PAUSE
            );
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: Each non-null handle is owned by this Window. Destroy children
        // before parents; _sdl shuts down SDL only after this method finishes.
        unsafe {
            if !self.texture.is_null() {
                ffi::SDL_DestroyTexture(self.texture);
            }
            if !self.renderer.is_null() {
                ffi::SDL_DestroyRenderer(self.renderer);
            }
            if !self.window.is_null() {
                ffi::SDL_DestroyWindow(self.window);
            }
        }
    }
}

fn error(action: &str) -> String {
    // SAFETY: SDL_GetError returns SDL-owned, NUL-terminated storage that remains
    // valid until its thread's next error change. Copy it immediately.
    let detail = unsafe { CStr::from_ptr(ffi::SDL_GetError()) }.to_string_lossy();
    format!("{action}: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_discards_wall_time_and_requires_a_fresh_press() {
        let start = Instant::now();
        let mut clock = PauseClock::new(start);
        let running = clock.sample(start + Duration::from_millis(10), Input::default());
        assert_eq!(running.elapsed, Duration::from_millis(10));
        let mut press = Input::default();
        press.record_press(ffi::SCANCODE_ESCAPE, 0, false);
        let entered = clock.sample(start + Duration::from_millis(20), press);
        assert!(entered.paused && entered.changed);
        assert_eq!(entered.elapsed, Duration::ZERO);
        let mut repeated = Input::default();
        repeated.record_press(ffi::SCANCODE_ESCAPE, 0, true);
        let held = clock.sample(start + Duration::from_secs(30), repeated);
        assert!(held.paused && !held.changed);
        assert_eq!(held.elapsed, Duration::ZERO);
        let mut resume = Input::default();
        resume.record_press(ffi::SCANCODE_P, 0, false);
        let resumed = clock.sample(start + Duration::from_secs(60), resume);
        assert!(!resumed.paused && resumed.changed);
        assert_eq!(resumed.elapsed, Duration::ZERO);
        assert_eq!(resumed.resumed_after, Some(Duration::from_millis(59_980)));
        // Diagnostic deadlines retain the unelapsed portion of a blocking wait.
        let deadline = start + Duration::from_millis(100) + resumed.resumed_after.unwrap();
        assert_eq!(
            deadline.duration_since(start + Duration::from_secs(60)),
            Duration::from_millis(80)
        );
        let running = clock.sample(start + Duration::from_millis(60_015), Input::default());
        assert_eq!(running.elapsed, Duration::from_millis(15));
    }

    #[test]
    fn all_pause_keys_can_resume_without_immediately_repausing() {
        for scancode in [ffi::SCANCODE_ESCAPE, ffi::SCANCODE_P, ffi::SCANCODE_PAUSE] {
            let now = Instant::now();
            let mut clock = PauseClock::new(now);
            let mut press = Input::default();
            press.record_press(scancode, 0, false);
            assert!(!press.quit);
            assert!(clock.sample(now, press).paused);
            assert!(!clock.sample(now + Duration::from_secs(1), press).paused);
            assert!(
                !clock
                    .sample(now + Duration::from_secs(2), Input::default())
                    .paused
            );
        }
    }

    #[test]
    fn movement_and_desktop_shortcuts_do_not_change_pause_state() {
        let mut ignored = vec![
            (ffi::SCANCODE_UP, 0),
            (ffi::SCANCODE_DOWN, 0),
            (ffi::SCANCODE_LEFT, 0),
            (ffi::SCANCODE_RIGHT, 0),
            (43, 0x0100),  // Alt+Tab
            (226, 0x0100), // Alt itself, before Tab arrives
            (40, 0),       // Enter
            (44, 0),       // Space
        ];
        for modifier in [0x0040, 0x0080, 0x0100, 0x0200, 0x0400, 0x0800] {
            for key in [ffi::SCANCODE_ESCAPE, ffi::SCANCODE_P, ffi::SCANCODE_PAUSE] {
                ignored.push((key, modifier));
            }
        }
        for (key, modifier) in ignored {
            let start = Instant::now();
            let mut clock = PauseClock::new(start);
            let mut input = Input::default();
            input.record_press(key, modifier, false);
            assert!(!clock.sample(start, input).changed);
            let mut pause = Input::default();
            pause.record_press(ffi::SCANCODE_ESCAPE, 0, false);
            assert!(clock.sample(start, pause).paused);
            let ignored = clock.sample(start + Duration::from_secs(1), input);
            assert!(ignored.paused && !ignored.changed);
            assert_eq!(ignored.elapsed, Duration::ZERO);
        }
    }

    #[test]
    fn restart_requires_fresh_unmodified_r_and_leaves_pause_unchanged() {
        for (modifiers, repeated, expected) in [
            (0, false, true),
            (0, true, false),
            (0x0100, false, false),
            (0x0040, false, false),
            (0x0400, false, false),
        ] {
            let mut input = Input::default();
            input.record_press(ffi::SCANCODE_R, modifiers, repeated);
            assert_eq!(input.restart_pressed, expected);
            assert!(!input.pause_pressed && !input.quit);
        }
    }

    #[test]
    fn quit_requires_control_and_keyboard_abi_matches_sdl3() {
        let mut plain = Input::default();
        plain.record_press(ffi::SCANCODE_Q, 0, false);
        assert!(!plain.quit);
        for control in [0x0040, 0x0080] {
            let mut input = Input::default();
            input.record_press(ffi::SCANCODE_Q, control, false);
            assert!(input.quit);
            assert!(!input.pause_pressed);
        }
        assert_eq!(std::mem::size_of::<ffi::KeyboardEvent>(), 40);
        assert_eq!(std::mem::offset_of!(ffi::KeyboardEvent, scancode), 24);
        assert_eq!(std::mem::offset_of!(ffi::KeyboardEvent, modifiers), 32);
        assert_eq!(std::mem::offset_of!(ffi::KeyboardEvent, repeat), 37);
    }

    #[test]
    fn startup_requires_a_fresh_key_without_desktop_shortcuts() {
        for (scancode, modifiers, repeat, expected) in [
            (44, 0, false, true), // Space.
            (40, 0, false, true), // Return.
            (44, 0, true, false),
            (43, 0x0100, false, false),  // Alt+Tab.
            (225, 0x0001, false, false), // Shift alone.
            (ffi::SCANCODE_Q, ffi::KMOD_CTRL, false, false),
        ] {
            let mut input = Input::default();
            input.record_press(scancode, modifiers, repeat);
            assert_eq!(input.continue_pressed, expected);
        }
    }

    #[test]
    fn menu_choices_and_game_shortcuts_use_fresh_key_events() {
        let mut queued = Input::default();
        queued.record_press(ffi::SCANCODE_K, 0, false);
        queued.record_press(40, 0, false);
        assert_eq!(queued.menu_key, Some(MenuKey::Kitten));
        for (scancode, expected) in [
            (ffi::SCANCODE_K, MenuKey::Kitten),
            (ffi::SCANCODE_H, MenuKey::HouseCat),
            (ffi::SCANCODE_T, MenuKey::Tomcat),
            (ffi::SCANCODE_A, MenuKey::AlleyCat),
            (40, MenuKey::Other),
        ] {
            let mut input = Input::default();
            input.record_press(scancode, 0, false);
            assert_eq!(input.menu_key, Some(expected));
            for (modifiers, repeat) in [(0, true), (0x0100, false), (0x0040, false)] {
                let mut ignored = Input::default();
                ignored.record_press(scancode, modifiers, repeat);
                assert_eq!(ignored.menu_key, None);
            }
        }
        for control in [0x0040, 0x0080] {
            let mut input = Input::default();
            input.record_press(ffi::SCANCODE_M, control, false);
            assert!(input.menu_pressed);
            assert!(!input.continue_pressed && !input.pause_pressed && !input.quit);
            let mut input = Input::default();
            input.record_press(ffi::SCANCODE_R, control, false);
            assert!(input.new_game_pressed);
            assert!(!input.restart_pressed && !input.pause_pressed);
        }
        for (modifiers, repeat) in [(0, false), (0x0040, true), (0x0140, false)] {
            let mut input = Input::default();
            input.record_press(ffi::SCANCODE_R, modifiers, repeat);
            input.record_press(ffi::SCANCODE_M, modifiers, repeat);
            assert!(!input.menu_pressed && !input.new_game_pressed);
        }
    }
}

// Small authored ABI declarations, checked against SDL3's public headers.
// No SDL implementation or generated binding code is included here.
#[allow(non_snake_case)]
mod ffi {
    use super::{c_char, c_int, c_void};

    pub const INIT_VIDEO: u32 = 0x20;
    pub const WINDOW_RESIZABLE: u64 = 0x20;
    pub const TEXTUREACCESS_STREAMING: c_int = 1;
    pub const SCALEMODE_NEAREST: c_int = 0;
    pub const LOGICAL_PRESENTATION_LETTERBOX: c_int = 2;
    #[cfg(target_endian = "little")]
    pub const PIXELFORMAT_RGBA32: u32 = 0x1676_2004;
    #[cfg(target_endian = "big")]
    pub const PIXELFORMAT_RGBA32: u32 = 0x1646_2004;
    pub const EVENT_QUIT: u32 = 0x100;
    pub const EVENT_WINDOW_CLOSE_REQUESTED: u32 = 0x210;
    pub const EVENT_KEY_DOWN: u32 = 0x300;
    pub const KMOD_CTRL: u16 = 0x00c0;
    pub const KMOD_SHORTCUT: u16 = 0x0fc0; // Left/right Ctrl, Alt and GUI/Super
    pub const SCANCODE_P: usize = 19;
    pub const SCANCODE_Q: usize = 20;
    pub const SCANCODE_R: usize = 21;
    pub const SCANCODE_A: usize = 4;
    pub const SCANCODE_H: usize = 11;
    pub const SCANCODE_K: usize = 14;
    pub const SCANCODE_M: usize = 16;
    pub const SCANCODE_T: usize = 23;
    pub const SCANCODE_PAUSE: usize = 72;
    pub const SCANCODE_ESCAPE: usize = 41;
    pub const SCANCODE_RIGHT: usize = 79;
    pub const SCANCODE_LEFT: usize = 80;
    pub const SCANCODE_DOWN: usize = 81;
    pub const SCANCODE_UP: usize = 82;
    pub const SCANCODE_LALT: usize = 226;
    pub const SCANCODE_RALT: usize = 230;

    #[repr(C, align(8))]
    pub struct Event {
        pub bytes: [u8; 128],
    }

    #[repr(C)]
    pub struct KeyboardEvent {
        pub kind: u32,
        pub reserved: u32,
        pub timestamp: u64,
        pub window_id: u32,
        pub keyboard_id: u32,
        pub scancode: c_int,
        pub key: u32,
        pub modifiers: u16,
        pub raw: u16,
        pub down: bool,
        pub repeat: bool,
    }

    #[repr(C)]
    pub struct FRect {
        pub x: f32,
        pub y: f32,
        pub w: f32,
        pub h: f32,
    }

    #[link(name = "SDL3")]
    unsafe extern "C" {
        pub fn SDL_SetMainReady();
        pub fn SDL_Init(flags: u32) -> bool;
        pub fn SDL_Quit();
        pub fn SDL_GetError() -> *const c_char;
        pub fn SDL_CreateWindow(
            title: *const c_char,
            w: c_int,
            h: c_int,
            flags: u64,
        ) -> *mut c_void;
        pub fn SDL_SetWindowMinimumSize(window: *mut c_void, w: c_int, h: c_int) -> bool;
        pub fn SDL_DestroyWindow(window: *mut c_void);
        pub fn SDL_CreateRenderer(window: *mut c_void, name: *const c_char) -> *mut c_void;
        pub fn SDL_SetRenderVSync(renderer: *mut c_void, vsync: c_int) -> bool;
        pub fn SDL_DestroyRenderer(renderer: *mut c_void);
        pub fn SDL_SetRenderLogicalPresentation(
            renderer: *mut c_void,
            w: c_int,
            h: c_int,
            mode: c_int,
        ) -> bool;
        pub fn SDL_SetRenderDrawColor(renderer: *mut c_void, r: u8, g: u8, b: u8, a: u8) -> bool;
        pub fn SDL_CreateTexture(
            renderer: *mut c_void,
            format: u32,
            access: c_int,
            w: c_int,
            h: c_int,
        ) -> *mut c_void;
        pub fn SDL_DestroyTexture(texture: *mut c_void);
        pub fn SDL_SetTextureScaleMode(texture: *mut c_void, mode: c_int) -> bool;
        pub fn SDL_UpdateTexture(
            texture: *mut c_void,
            rect: *const c_void,
            pixels: *const c_void,
            pitch: c_int,
        ) -> bool;
        pub fn SDL_RenderClear(renderer: *mut c_void) -> bool;
        pub fn SDL_RenderTexture(
            renderer: *mut c_void,
            texture: *mut c_void,
            source: *const c_void,
            destination: *const c_void,
        ) -> bool;
        pub fn SDL_RenderPresent(renderer: *mut c_void) -> bool;
        pub fn SDL_SetRenderScale(renderer: *mut c_void, x: f32, y: f32) -> bool;
        pub fn SDL_RenderFillRect(renderer: *mut c_void, rect: *const FRect) -> bool;
        pub fn SDL_RenderDebugText(
            renderer: *mut c_void,
            x: f32,
            y: f32,
            text: *const c_char,
        ) -> bool;
        pub fn SDL_PollEvent(event: *mut Event) -> bool;
        pub fn SDL_GetKeyboardState(count: *mut c_int) -> *const bool;
    }
}
