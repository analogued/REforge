// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Main-thread SDL3 presentation, independent of engine clocks and controls.
//! Source texture dimensions are independent of the aspect-corrected display
//! dimensions. Resizing uses integer display multiples and nearest sampling.

pub use sdl3;
pub mod audio;
mod hyllian;
mod input;
pub use input::{EventClockSample, HostPause, HostShortcut, event_clock_sample, host_shortcut};

/// Shared display treatment; this never changes an engine's pixels or palette.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Presentation {
    #[default]
    Pixels,
    Hyllian,
}

impl std::str::FromStr for Presentation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pixels" => Ok(Self::Pixels),
            "hyllian" => Ok(Self::Hyllian),
            _ => Err("presentation must be pixels or hyllian".into()),
        }
    }
}

/// Used only when automatic startup sizing cannot query a usable desktop area.
const AUTO_FALLBACK_SCALE: u32 = 4;

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
    size: (u32, u32),
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
            size: (width, height),
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
    // GPU resources must be released before their owning renderer/device.
    effect: Option<hyllian::Hyllian>,
    canvas: WindowCanvas,
    events: sdl3::EventPump,
    minimum_pixels: (u32, u32),
    gpu_presented_pixels: Option<(u32, u32)>,
    _video: sdl3::VideoSubsystem,
    _sdl: sdl3::Sdl,
}

impl Window {
    /// None requests the display's usable desktop area, ignoring decorations.
    /// Some(scale) requests display dimensions times scale in drawable pixels.
    /// Desktop constraints can override sizing; later user resizes are retained.
    pub fn new(
        title: &str,
        display_width: u32,
        display_height: u32,
        scale: Option<u32>,
    ) -> Result<Self, String> {
        Self::new_with_presentation(
            title,
            display_width,
            display_height,
            scale,
            Presentation::Pixels,
        )
    }

    /// Request an optional CRT effect at startup, before creating frame textures.
    /// Unsupported GPU/shader setup warns and falls back to plain presentation.
    pub fn new_with_presentation(
        title: &str,
        display_width: u32,
        display_height: u32,
        scale: Option<u32>,
        presentation: Presentation,
    ) -> Result<Self, String> {
        if display_width == 0 || display_height == 0 || scale == Some(0) {
            return Err("display dimensions and scale must be positive".into());
        }
        if title.contains('\0') {
            return Err("window title contains a NUL byte".into());
        }
        let width = display_width
            .checked_mul(scale.unwrap_or(1))
            .filter(|value| *value <= i32::MAX as u32)
            .ok_or("scaled window width is too large")?;
        let height = display_height
            .checked_mul(scale.unwrap_or(1))
            .filter(|value| *value <= i32::MAX as u32)
            .ok_or("scaled window height is too large")?;
        let sdl = sdl3::init().map_err(|error| error.to_string())?;
        let video = sdl.video().map_err(|error| error.to_string())?;
        // Create auto windows at the primary display's reported work area so
        // centering is based on the requested area, not a small bootstrap window.
        // Re-query the window's selected display below and once after mapping.
        let (width, height) = if scale.is_none() {
            video
                .get_primary_display()
                .and_then(|display| display.get_usable_bounds())
                .map_or((width, height), |bounds| (bounds.width(), bounds.height()))
        } else {
            (width, height)
        };
        let mut window = video
            .window(title, width, height)
            .position_centered()
            .resizable()
            .high_pixel_density()
            .hidden()
            .build()
            .map_err(|error| error.to_string())?;
        let minimum_pixels = (display_width, display_height);
        request_initial_size(&mut window, minimum_pixels, scale)?;
        let (mut canvas, effect) = hyllian::create_canvas(window, presentation)?;
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
        if request_initial_size(canvas.window_mut(), minimum_pixels, scale)? {
            sync_window(canvas.window());
        }
        let events = sdl.event_pump().map_err(|error| error.to_string())?;
        Ok(Self {
            effect,
            canvas,
            events,
            minimum_pixels,
            gpu_presented_pixels: None,
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
        self.prepare_frame()?;
        if let Some(effect) = self.effect.as_mut() {
            effect.paint(&mut self.canvas, &frame.texture, frame.size)?;
        } else {
            paint(&mut self.canvas, &frame.texture)?;
        }
        overlay(&mut self.canvas)?;
        if !self.canvas.present() {
            return Err(sdl3::get_error().to_string());
        }
        Ok(())
    }

    fn prepare_frame(&mut self) -> Result<(), String> {
        if self.canvas.renderer_name != "gpu" {
            return Ok(());
        }
        let pixels = self.canvas.window().size_in_pixels();
        if self.gpu_presented_pixels == Some(pixels) {
            return Ok(());
        }
        // SDL's GPU renderer replaces its backing texture during Present, after
        // drawing. Settle that transition before drawing into the new viewport,
        // rather than relying on an engine to request a second frame. This also
        // avoids out-of-bounds readback in a caller's overlay after enlargement.
        // Submit the retained image, without invoking the overlay twice or
        // clearing the window to black during an ordinary resize.
        if !self.canvas.present() {
            return Err(sdl3::get_error().to_string());
        }
        configure(
            &mut self.canvas,
            self.minimum_pixels.0,
            self.minimum_pixels.1,
        )?;
        // Cache the size sampled before presentation. If the desktop changes it
        // asynchronously during Present, the next frame detects that change.
        self.gpu_presented_pixels = Some(pixels);
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

/// Desktop bounds are window coordinates, while explicit multiples target
/// drawable pixels. Do not convert auto bounds by density a second time.
fn initial_window_size(
    display_pixels: (u32, u32),
    scale: Option<u32>,
    density: f32,
    usable: Option<(u32, u32)>,
) -> Result<(u32, u32), String> {
    if scale.is_none() {
        if let Some((width, height)) = usable.filter(|&(w, h)| w > 0 && h > 0) {
            let minimum = window_size_for_pixels(display_pixels, density)?;
            return Ok((width.max(minimum.0), height.max(minimum.1)));
        }
    }
    let multiplier = scale.unwrap_or(AUTO_FALLBACK_SCALE);
    let dimension = |pixels: u32| {
        pixels
            .checked_mul(multiplier)
            .filter(|&value| value > 0 && value <= i32::MAX as u32)
            .ok_or_else(|| "scaled window dimension is out of range".to_string())
    };
    window_size_for_pixels(
        (dimension(display_pixels.0)?, dimension(display_pixels.1)?),
        density,
    )
}

fn request_initial_size(
    window: &mut SdlWindow,
    display_pixels: (u32, u32),
    scale: Option<u32>,
) -> Result<bool, String> {
    update_minimum_size(window, display_pixels)?;
    let usable = if scale.is_none() {
        match window
            .get_display()
            .and_then(|display| display.get_usable_bounds())
        {
            Ok(bounds) => Some((bounds.width(), bounds.height())),
            Err(error) => {
                eprintln!(
                    "REforge: cannot query usable desktop area: {error}; using scale {AUTO_FALLBACK_SCALE}"
                );
                None
            }
        }
    } else {
        None
    };
    let size = initial_window_size(display_pixels, scale, window.pixel_density(), usable)?;
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
    #[ignore = "requires an SDL GPU-capable desktop; run serially"]
    fn gpu_first_redraw_after_resize_preserves_all_frame_edges() {
        let mut display = Window::new_with_presentation(
            "REforge GPU resize check",
            320,
            240,
            None,
            Presentation::Hyllian,
        )
        .unwrap();
        assert_eq!(display.canvas.renderer_name, "gpu");
        let creator = display.texture_creator();
        let mut frame = FrameTexture::new(&creator, 640, 200, FrameFormat::Rgb).unwrap();
        let mut pixels = vec![0; 640 * 200 * 3];
        for y in 0..200 {
            for x in 0..640 {
                let color = if y < 8 {
                    [255, 0, 0]
                } else if y >= 192 {
                    [0, 255, 0]
                } else if x < 8 {
                    [0, 0, 255]
                } else if x >= 632 {
                    [255; 3]
                } else {
                    [0; 3]
                };
                pixels[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3].copy_from_slice(&color);
            }
        }
        frame.update(&pixels).unwrap();
        // Cover startup, enlargement, shrinkage and maximize. Each phase makes
        // exactly one public presentation request, as a static splash does.
        for requested in [
            None,
            Some((640, 480)),
            Some((1800, 1000)),
            Some((1000, 750)),
            None,
        ] {
            if let Some(size) = requested {
                let size =
                    window_size_for_pixels(size, display.canvas.window().pixel_density()).unwrap();
                display
                    .canvas
                    .window_mut()
                    .set_size(size.0, size.1)
                    .unwrap();
                sync_window(display.canvas.window());
            } else if display.gpu_presented_pixels.is_some() {
                let before = display.canvas.window().size_in_pixels();
                assert!(display.canvas.window_mut().maximize());
                sync_window(display.canvas.window());
                // Window managers may complete maximize asynchronously. Pump
                // events without presenting intermediate frames while waiting.
                for _ in 0..100 {
                    display.poll_events().for_each(drop);
                    if display.canvas.window().size_in_pixels() != before {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(3));
                }
            }
            display.poll_events().for_each(drop);
            let mut overlay_calls = 0;
            display
                .present_with(&frame, |canvas| {
                    overlay_calls += 1;
                    let readback = canvas
                        .read_pixels(None)
                        .unwrap()
                        .convert_format(PixelFormat::RGB24)
                        .unwrap();
                    let width = readback.width() as usize;
                    let height = readback.height() as usize;
                    let pitch = readback.pitch() as usize;
                    readback.with_lock(|actual| {
                        let at =
                            |x: usize, y: usize| &actual[y * pitch + x * 3..y * pitch + x * 3 + 3];
                        let top = at(width / 2, height * 4 / 200);
                        let bottom = at(width / 2, height - height * 4 / 200);
                        let left = at(width * 4 / 640, height / 2);
                        let right = at(width - width * 4 / 640, height / 2);
                        assert!(
                            top[0] > 80 && top[1] == 0 && top[2] == 0,
                            "top clipped: {top:?}"
                        );
                        assert!(
                            bottom[1] > 80 && bottom[0] == 0 && bottom[2] == 0,
                            "bottom clipped: {bottom:?}"
                        );
                        assert!(
                            left[2] > 80 && left[0] == 0 && left[1] == 0,
                            "left clipped: {left:?}"
                        );
                        assert!(right.iter().all(|&v| v > 80), "right clipped: {right:?}");
                    });
                    eprintln!(
                        "first redraw: drawable={:?}, content={width}x{height}; all edges retained",
                        canvas.window().size_in_pixels()
                    );
                    Ok(())
                })
                .unwrap();
            assert_eq!(overlay_calls, 1);
        }
    }

    #[test]
    #[ignore = "requires SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software; run serially"]
    fn hyllian_software_fallback_keeps_plain_pixels() {
        let mut display = Window::new_with_presentation(
            "REforge software fallback check",
            320,
            240,
            Some(2),
            Presentation::Hyllian,
        )
        .unwrap();
        assert_eq!(display.canvas.renderer_name, "software");
        assert!(display.effect.is_none());
        let creator = display.texture_creator();
        let mut frame = FrameTexture::new(&creator, 320, 200, FrameFormat::Rgba).unwrap();
        let pixels = [17, 85, 170, 255].repeat(320 * 200);
        frame.update(&pixels).unwrap();
        paint(&mut display.canvas, &frame.texture).unwrap();
        let readback = display
            .canvas
            .read_pixels(None)
            .unwrap()
            .convert_format(PixelFormat::RGB24)
            .unwrap();
        readback.with_lock(|actual| {
            let pitch = readback.pitch() as usize;
            for y in 0..readback.height() as usize {
                for x in 0..readback.width() as usize {
                    assert_eq!(
                        &actual[y * pitch + x * 3..y * pitch + x * 3 + 3],
                        &[17, 85, 170]
                    );
                }
            }
        });
        display.present(&frame).unwrap();
    }

    #[test]
    #[ignore = "requires an SDL GPU-capable desktop; run serially"]
    fn hyllian_gpu_scanlines_follow_source_rows() {
        let mut display = Window::new_with_presentation(
            "REforge Hyllian check",
            320,
            240,
            Some(4),
            Presentation::Hyllian,
        )
        .unwrap();
        assert_eq!(display.canvas.renderer_name, "gpu");
        assert!(
            display.effect.is_some(),
            "Hyllian must execute for this check"
        );
        let creator = display.texture_creator();
        for (width, format, bytes) in [(640, FrameFormat::Rgb, 3), (320, FrameFormat::Rgba, 4)] {
            let mut frame = FrameTexture::new(&creator, width, 200, format).unwrap();
            frame
                .update(&vec![255; width as usize * 200 * bytes])
                .unwrap();
            for scale in [4, 2] {
                let size = window_size_for_pixels(
                    (320 * scale, 240 * scale),
                    display.canvas.window().pixel_density(),
                )
                .unwrap();
                display
                    .canvas
                    .window_mut()
                    .set_size(size.0, size.1)
                    .unwrap();
                sync_window(display.canvas.window());
                display.poll_events().for_each(drop);
                // Match the engines: upload after window events/size changes.
                frame
                    .update(&vec![255; width as usize * 200 * bytes])
                    .unwrap();
                // SDL's GPU renderer updates its backing texture at presentation,
                // not Window::sync. Settle that resize before a pixel readback;
                // otherwise an enlarged read could exceed the old backing size.
                display.present(&frame).unwrap();
                paint(&mut display.canvas, &frame.texture).unwrap();
                let plain = display
                    .canvas
                    .read_pixels(None)
                    .unwrap()
                    .convert_format(PixelFormat::RGB24)
                    .unwrap();
                assert_eq!(
                    plain.with_lock(|actual| actual
                        [3 * plain.width() as usize / 2..3 * plain.width() as usize / 2 + 3]
                        .to_vec()),
                    [255, 255, 255]
                );
                display
                    .effect
                    .as_mut()
                    .unwrap()
                    .paint(&mut display.canvas, &frame.texture, frame.size)
                    .unwrap();
                display.canvas.set_draw_color(Color::RGB(17, 85, 170));
                display
                    .canvas
                    .fill_rect(sdl3::rect::Rect::new(16, 16, 8, 8))
                    .unwrap();
                let readback = display
                    .canvas
                    .read_pixels(None)
                    .unwrap()
                    .convert_format(PixelFormat::RGB24)
                    .unwrap();
                readback.with_lock(|actual| {
                    let multiplier = readback.width() as usize / 320;
                    let offset =
                        (20 * multiplier) * readback.pitch() as usize + 20 * multiplier * 3;
                    assert_eq!(
                        &actual[offset..offset + 3],
                        &[17, 85, 170],
                        "overlay must bypass CRT shader"
                    );
                });
                let levels: Vec<u32> = readback.with_lock(|actual| {
                    let pitch = readback.pitch() as usize;
                    (0..readback.height() as usize)
                        .map(|y| {
                            let left = readback.width() as usize / 2;
                            actual[y * pitch + left * 3..y * pitch + (left + 32) * 3]
                                .iter()
                                .map(|&v| u32::from(v))
                                .sum()
                        })
                        .collect()
                });
                let peaks = levels
                    .windows(3)
                    .filter(|row| row[1] > row[0] && row[1] >= row[2])
                    .count();
                eprintln!(
                    "Hyllian: source={width}x200, drawable={:?}, readback={}x{}, peaks={peaks}",
                    display.canvas.window().size_in_pixels(),
                    readback.width(),
                    readback.height()
                );
                // 200 beam cycles, regardless of source width or output height;
                // allow edge/quantization differences, but reject a 240-row mask.
                assert!(
                    (195..=205).contains(&peaks),
                    "wrong scanline frequency: {peaks}"
                );
                assert!(levels.iter().max().unwrap() - levels.iter().min().unwrap() > 1000);
                assert!(display.canvas.present());
            }
        }
    }

    #[test]
    fn auto_uses_desktop_coordinates_and_explicit_scale_uses_drawable_pixels() {
        for density in [1.0, 1.25, 1.5, 2.0] {
            assert_eq!(
                initial_window_size((320, 240), None, density, Some((1920, 1040))).unwrap(),
                (1920, 1040)
            );
            assert_eq!(
                initial_window_size((320, 240), Some(4), density, Some((1920, 1040))).unwrap(),
                window_size_for_pixels((1280, 960), density).unwrap()
            );
            for usable in [None, Some((0, 0))] {
                assert_eq!(
                    initial_window_size((320, 240), None, density, usable).unwrap(),
                    window_size_for_pixels((1280, 960), density).unwrap()
                );
            }
        }
        assert_eq!(
            initial_window_size((320, 240), None, 2.0, Some((100, 100))).unwrap(),
            (160, 120)
        );
        assert!(initial_window_size((320, 240), Some(0), 1.0, None).is_err());
        assert!(initial_window_size((u32::MAX, 240), Some(16), 1.0, None).is_err());
    }

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
    fn auto_startup_preserves_integer_presentation_and_user_resize() {
        let mut display = Window::new("REforge auto display check", 320, 240, None).unwrap();
        assert_eq!(
            display.canvas.logical_size(),
            (
                320,
                240,
                sdl3::sys::render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE
            )
        );
        let window = display.canvas.window();
        eprintln!(
            "auto: usable={:?}, window={:?}, drawable={:?}, density={}",
            window
                .get_display()
                .and_then(|display| display.get_usable_bounds()),
            window.size(),
            window.size_in_pixels(),
            window.pixel_density()
        );
        let creator = display.texture_creator();
        for (width, format, bytes) in [(320, FrameFormat::Rgba, 4), (640, FrameFormat::Rgb, 3)] {
            let mut frame = FrameTexture::new(&creator, width, 200, format).unwrap();
            frame
                .update(&vec![255; width as usize * 200 * bytes])
                .unwrap();
            display.present(&frame).unwrap();
        }
        // Startup-only auto sizing must not undo later arbitrary window sizes.
        let resized =
            window_size_for_pixels((1000, 750), display.canvas.window().pixel_density()).unwrap();
        display
            .canvas
            .window_mut()
            .set_size(resized.0, resized.1)
            .unwrap();
        sync_window(display.canvas.window());
        let actual = display.canvas.window().size();
        display.poll_events().for_each(drop);
        assert_eq!(display.canvas.window().size(), actual);
        assert_eq!(
            display.canvas.logical_size(),
            (
                320,
                240,
                sdl3::sys::render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE
            )
        );
    }

    #[test]
    #[ignore = "requires a desktop or SDL_VIDEODRIVER=dummy; run with --test-threads=1"]
    fn high_density_window_pixel_size_and_user_resize() {
        for scale in [1, 4] {
            let mut display = Window::new("REforge display check", 320, 240, Some(scale)).unwrap();
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
