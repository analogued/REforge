// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Main-thread SDL3 presentation, independent of engine clocks and controls.
//! Source texture dimensions are independent of the aspect-corrected display
//! dimensions. Resizing uses integer display multiples and nearest sampling.

pub use sdl3;
mod input;
pub use input::{HostPause, HostShortcut, host_shortcut};

/// Initial multiplier of the aspect-corrected display area for every engine.
pub const DEFAULT_SCALE: u32 = 4;

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::KeyboardState,
    pixels::{Color, PixelFormat},
    render::{Canvas, RenderTarget, ScaleMode, Texture, TextureCreator, WindowCanvas},
    video::{Window as SdlWindow, WindowContext},
};

pub type FrameCreator = TextureCreator<WindowContext>;

#[derive(Clone, Copy)]
pub enum FrameFormat {
    Rgb,
    Rgba,
}

/// A reusable streaming texture. Keep its creator alive until it is dropped;
/// creators and textures must belong to the window used for presentation.
pub struct FrameTexture<'a> {
    texture: Texture<'a>,
    pitch: usize,
    byte_len: usize,
}

impl<'a> FrameTexture<'a> {
    pub fn new<T>(
        creator: &'a TextureCreator<T>,
        width: u32,
        height: u32,
        format: FrameFormat,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("source dimensions must be positive".into());
        }
        let (format, bytes) = match format {
            FrameFormat::Rgb => (PixelFormat::RGB24, 3),
            FrameFormat::Rgba => (PixelFormat::RGBA32, 4),
        };
        let pitch = (width as usize)
            .checked_mul(bytes)
            .ok_or("texture row is too large")?;
        let byte_len = pitch
            .checked_mul(height as usize)
            .ok_or("texture buffer is too large")?;
        let mut texture = creator
            .create_texture_streaming(format, width, height)
            .map_err(|error| error.to_string())?;
        texture.set_scale_mode(ScaleMode::Nearest);
        Ok(Self {
            texture,
            pitch,
            byte_len,
        })
    }

    pub fn update(&mut self, pixels: &[u8]) -> Result<(), String> {
        if pixels.len() != self.byte_len {
            return Err(format!(
                "expected {} pixel bytes, received {}",
                self.byte_len,
                pixels.len()
            ));
        }
        self.texture
            .update(None, pixels, self.pitch)
            .map_err(|error| error.to_string())
    }
}

/// Owns one window and SDL event pump. Construct, use and drop on the main thread.
/// The SDL wrapper manages native object ownership and subsystem lifetimes.
pub struct Window {
    canvas: WindowCanvas,
    events: sdl3::EventPump,
    minimum_pixels: (u32, u32),
    _video: sdl3::VideoSubsystem,
    _sdl: sdl3::Sdl,
}

impl Window {
    /// Request an initial drawable size of display dimensions times scale.
    /// Desktop constraints can override sizing; later user resizes are retained.
    pub fn new(
        title: &str,
        display_width: u32,
        display_height: u32,
        scale: u32,
    ) -> Result<Self, String> {
        if display_width == 0 || display_height == 0 || scale == 0 {
            return Err("display dimensions and scale must be positive".into());
        }
        if title.contains('\0') {
            return Err("window title contains a NUL byte".into());
        }
        let width = display_width
            .checked_mul(scale)
            .filter(|value| *value <= i32::MAX as u32)
            .ok_or("scaled window width is too large")?;
        let height = display_height
            .checked_mul(scale)
            .filter(|value| *value <= i32::MAX as u32)
            .ok_or("scaled window height is too large")?;
        let sdl = sdl3::init().map_err(|error| error.to_string())?;
        let video = sdl.video().map_err(|error| error.to_string())?;
        let mut window = video
            .window(title, width, height)
            .position_centered()
            .resizable()
            .high_pixel_density()
            .hidden()
            .build()
            .map_err(|error| error.to_string())?;
        let minimum_pixels = (display_width, display_height);
        request_initial_pixel_size(&mut window, (width, height), minimum_pixels)?;
        // None lets SDL select a renderer supported by the host, including GPU backends.
        let mut canvas =
            sdl3::render::create_renderer(window, None).map_err(|error| error.to_string())?;
        configure(&mut canvas, display_width, display_height)?;
        // The wrapper does not expose this setter. Use its generated declarations,
        // rather than maintaining a second handwritten C interface.
        // SAFETY: canvas owns a live renderer, remains on the initializing main
        // thread, and the call neither takes ownership nor retains Rust memory.
        if !unsafe { sdl3::sys::render::SDL_SetRenderVSync(canvas.raw(), 1) } {
            eprintln!(
                "REforge: cannot enable display vsync: {}; continuing without it",
                sdl3::get_error()
            );
        }
        // Wayland needs a presented buffer before a window is mapped. Settle the
        // initial display/density first, then compensate once more if mapping
        // changed it. This startup-only sizing never fights later user resizes.
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.clear();
        if !canvas.window_mut().show() || !canvas.present() {
            return Err(sdl3::get_error().to_string());
        }
        sync_window(canvas.window());
        if request_initial_pixel_size(canvas.window_mut(), (width, height), minimum_pixels)? {
            sync_window(canvas.window());
        }
        let events = sdl.event_pump().map_err(|error| error.to_string())?;
        Ok(Self {
            canvas,
            events,
            minimum_pixels,
            _video: video,
            _sdl: sdl,
        })
    }

    pub fn texture_creator(&self) -> FrameCreator {
        self.canvas.texture_creator()
    }

    pub fn poll_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        let window = self.canvas.window_mut();
        let minimum_pixels = self.minimum_pixels;
        self.events.poll_iter().inspect(move |event| {
            if matches!(
                event,
                Event::Window {
                    win_event: WindowEvent::PixelSizeChanged(..) | WindowEvent::DisplayChanged(..),
                    ..
                }
            ) {
                update_minimum_size(window, minimum_pixels).unwrap_or_else(|error| {
                    eprintln!("REforge: cannot update window minimum: {error}");
                });
            }
        })
    }

    pub fn keyboard_state(&self) -> KeyboardState<'_> {
        self.events.keyboard_state()
    }

    pub fn set_title(&mut self, title: &str) -> Result<(), String> {
        if title.contains('\0') {
            return Err("window title contains a NUL byte".into());
        }
        self.canvas
            .window_mut()
            .set_title(title)
            .map_err(|error| error.to_string())
    }

    pub fn present(&mut self, frame: &FrameTexture<'_>) -> Result<(), String> {
        self.present_with(frame, |_| Ok(()))
    }

    /// Draw an optional engine overlay in logical display coordinates, after the
    /// uploaded frame and before presentation. The engine supplies its contents.
    pub fn present_with(
        &mut self,
        frame: &FrameTexture<'_>,
        overlay: impl FnOnce(&mut WindowCanvas) -> Result<(), String>,
    ) -> Result<(), String> {
        paint(&mut self.canvas, &frame.texture)?;
        overlay(&mut self.canvas)?;
        if !self.canvas.present() {
            return Err(sdl3::get_error().to_string());
        }
        Ok(())
    }
}

/// Convert drawable pixels to platform window coordinates. Round upward so a
/// fractional density cannot reduce an integer presentation step by one pixel.
fn window_size_for_pixels(pixels: (u32, u32), density: f32) -> Result<(u32, u32), String> {
    if !density.is_finite() || density <= 0.0 {
        return Err("invalid window pixel density".into());
    }
    let dimension = |pixels: u32| -> Result<u32, String> {
        let units = (f64::from(pixels) / f64::from(density)).ceil();
        if !(1.0..=f64::from(i32::MAX)).contains(&units) {
            return Err("pixel-scaled window dimension is out of range".into());
        }
        Ok(units as u32)
    };
    Ok((dimension(pixels.0)?, dimension(pixels.1)?))
}

fn update_minimum_size(window: &mut SdlWindow, pixels: (u32, u32)) -> Result<(), String> {
    let minimum = window_size_for_pixels(pixels, window.pixel_density())?;
    if window.minimum_size() != minimum {
        window
            .set_minimum_size(minimum.0, minimum.1)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn request_initial_pixel_size(
    window: &mut SdlWindow,
    pixels: (u32, u32),
    minimum_pixels: (u32, u32),
) -> Result<bool, String> {
    update_minimum_size(window, minimum_pixels)?;
    let size = window_size_for_pixels(pixels, window.pixel_density())?;
    if window.size() == size {
        return Ok(false);
    }
    window
        .set_size(size.0, size.1)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn sync_window(window: &SdlWindow) {
    if !window.sync() {
        eprintln!("REforge: window sizing is still pending; using available drawable pixels");
    }
}

fn configure<T: RenderTarget>(
    canvas: &mut Canvas<T>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    canvas
        .set_logical_size(
            width,
            height,
            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE,
        )
        .map_err(|error| error.to_string())
}

fn paint<T: RenderTarget>(canvas: &mut Canvas<T>, texture: &Texture<'_>) -> Result<(), String> {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas
        .copy(texture, None, None)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl3::surface::Surface;

    #[test]
    fn pixel_targets_compensate_density_and_round_up() {
        for (pixels, density, expected) in [
            ((1280, 960), 1.0, (1280, 960)),
            ((1280, 960), 2.0, (640, 480)),
            ((1280, 960), 1.25, (1024, 768)),
            ((1280, 960), 1.5, (854, 640)),
            ((1280, 960), 1.75, (732, 549)),
            ((320, 240), 2.0, (160, 120)),
            ((320, 240), 3.0, (107, 80)),
        ] {
            let size = window_size_for_pixels(pixels, density).unwrap();
            assert_eq!(size, expected);
            assert!(f64::from(size.0) * f64::from(density) >= f64::from(pixels.0));
            assert!(f64::from(size.1) * f64::from(density) >= f64::from(pixels.1));
        }
        for density in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MIN_POSITIVE] {
            assert!(window_size_for_pixels((320, 240), density).is_err());
        }
    }

    #[test]
    #[ignore = "requires a desktop or SDL_VIDEODRIVER=dummy; run with --test-threads=1"]
    fn high_density_window_pixel_size_and_user_resize() {
        for scale in [1, DEFAULT_SCALE] {
            let mut display = Window::new("REforge display check", 320, 240, scale).unwrap();
            let window = display.canvas.window();
            assert_ne!(
                window.window_flags().0 & sdl3::sys::video::SDL_WINDOW_HIGH_PIXEL_DENSITY.0,
                0
            );
            let actual = window.size_in_pixels();
            eprintln!(
                "scale={scale}: window={:?}, drawable={actual:?}, density={}",
                window.size(),
                window.pixel_density()
            );
            assert!(actual.0 >= 320 * scale && actual.1 >= 240 * scale);
            let creator = display.texture_creator();
            for (width, format, bytes) in [(320, FrameFormat::Rgba, 4), (640, FrameFormat::Rgb, 3)]
            {
                let mut frame = FrameTexture::new(&creator, width, 200, format).unwrap();
                frame
                    .update(&vec![255; width as usize * 200 * bytes])
                    .unwrap();
                display.present(&frame).unwrap();
            }
            let size = window_size_for_pixels((640, 480), display.canvas.window().pixel_density())
                .unwrap();
            display
                .canvas
                .window_mut()
                .set_size(size.0, size.1)
                .unwrap();
            assert!(display.canvas.window().sync());
            display.poll_events().for_each(drop);
            assert_eq!(display.canvas.window().size(), size);
            assert!(display.canvas.window().size_in_pixels().0 >= 640);
            let minimum =
                window_size_for_pixels((320, 240), display.canvas.window().pixel_density())
                    .unwrap();
            assert_eq!(display.canvas.window().minimum_size(), minimum);
        }
    }

    #[test]
    fn integer_aspect_correction_and_nearest_sampling_preserve_exact_pixels() {
        // Exercise the same presentation path without a desktop or original data.
        for (output_width, output_height) in [
            (320, 240),
            (322, 241),
            (640, 480),
            (800, 600),
            (960, 720),
            (1024, 768),
            (1280, 961),
        ] {
            for (source_width, format) in [(320, FrameFormat::Rgba), (640, FrameFormat::Rgb)] {
                let surface =
                    Surface::new(output_width, output_height, PixelFormat::RGB24).unwrap();
                let mut canvas = surface.into_canvas().unwrap();
                configure(&mut canvas, 320, 240).unwrap();
                let creator = canvas.texture_creator();
                let mut frame = FrameTexture::new(&creator, source_width, 200, format).unwrap();
                let bytes = if source_width == 320 { 4 } else { 3 };
                let mut pixels = Vec::new();
                for y in 0..200 {
                    for x in 0..source_width {
                        pixels.extend_from_slice(&[(x % 256) as u8, y as u8, (x / 256) as u8]);
                        if bytes == 4 {
                            pixels.push(255);
                        }
                    }
                }
                assert!(frame.update(&pixels[..pixels.len() - 1]).is_err());
                frame.update(&pixels).unwrap();
                paint(&mut canvas, &frame.texture).unwrap();
                // ReadPixels defaults to the logical viewport. Disable that viewport
                // after drawing so readback includes the letterbox margins too.
                canvas
                    .set_logical_size(0, 0, sdl3::sys::render::SDL_LOGICAL_PRESENTATION_DISABLED)
                    .unwrap();
                let readback = canvas
                    .read_pixels(None)
                    .unwrap()
                    .convert_format(PixelFormat::RGB24)
                    .unwrap();
                let pitch = readback.pitch() as usize;
                let actual = readback.with_lock(|pixels| pixels.to_vec());
                let scale = (output_width / 320).min(output_height / 240) as usize;
                let width = 320 * scale;
                let height = 240 * scale;
                let left = (output_width as usize - width) / 2;
                let top = (output_height as usize - height) / 2;
                assert_eq!(readback.width(), output_width);
                assert_eq!(readback.height(), output_height);
                assert_eq!(actual.len(), pitch * output_height as usize);
                // RGB rows may include alignment padding at fractional-density
                // window widths. Inspect visible pixels, not that padding.
                for y in 0..output_height as usize {
                    for x in 0..output_width as usize {
                        let offset = y * pitch + x * 3;
                        let actual = &actual[offset..offset + 3];
                        if x < left || y < top || x >= left + width || y >= top + height {
                            assert_eq!(actual, [0, 0, 0], "letterbox at ({x}, {y})");
                            continue;
                        }
                        // At exact texel boundaries, nearest sampling can select either
                        // neighbour (SDL's software scaler uses fixed-point arithmetic).
                        // All other samples must match exactly; none may blend colours.
                        let numerator_x = (2 * (x - left) + 1) * source_width as usize;
                        let numerator_y = (2 * (y - top) + 1) * 200;
                        let source_x = numerator_x / (2 * width);
                        let source_y = numerator_y / (2 * height);
                        let first_x =
                            source_x.saturating_sub(usize::from(numerator_x % (2 * width) == 0));
                        let first_y =
                            source_y.saturating_sub(usize::from(numerator_y % (2 * height) == 0));
                        let matches = (first_y..=source_y).any(|sy| {
                            (first_x..=source_x).any(|sx| {
                                let source = (sy * source_width as usize + sx) * bytes;
                                actual == &pixels[source..source + 3]
                            })
                        });
                        assert!(
                            matches,
                            "source width {source_width}, output {output_width}x{output_height} at ({x}, {y}): {actual:?}"
                        );
                    }
                }
            }
        }
    }
}
