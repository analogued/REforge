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

/// Host input sampled after draining SDL's event queue.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub alt: bool,
    pub quit: bool,
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
        // SAFETY: Event has SDL_Event's 128-byte storage and sufficient alignment
        // on supported desktop ABIs. PollEvent fills it; only the common initial
        // Uint32 event type is read. The queue pump updates held-key state.
        unsafe {
            while ffi::SDL_PollEvent(&mut event) {
                let kind = u32::from_ne_bytes(event.bytes[..4].try_into().unwrap());
                if kind == ffi::EVENT_QUIT || kind == ffi::EVENT_WINDOW_CLOSE_REQUESTED {
                    self.quit = true;
                }
            }
            let mut count = 0;
            let keys = ffi::SDL_GetKeyboardState(&mut count);
            let held = |index: usize| {
                // SDL owns count bools until shutdown. Index bounds are checked
                // before reading, and the state is consumed before the next pump.
                !keys.is_null() && index < count.max(0) as usize && *keys.add(index)
            };
            self.quit |= held(ffi::SCANCODE_ESCAPE);
            Input {
                up: held(ffi::SCANCODE_UP),
                down: held(ffi::SCANCODE_DOWN),
                left: held(ffi::SCANCODE_LEFT),
                right: held(ffi::SCANCODE_RIGHT),
                alt: held(ffi::SCANCODE_LALT) || held(ffi::SCANCODE_RALT),
                quit: self.quit,
            }
        }
    }

    /// Present tightly packed RGBA bytes matching the source dimensions.
    pub fn present(&mut self, rgba: &[u8]) -> Result<(), String> {
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
            if !ffi::SDL_RenderPresent(self.renderer) {
                return Err(error("present SDL frame"));
            }
        }
        Ok(())
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
        pub fn SDL_PollEvent(event: *mut Event) -> bool;
        pub fn SDL_GetKeyboardState(count: *mut c_int) -> *const bool;
    }
}
