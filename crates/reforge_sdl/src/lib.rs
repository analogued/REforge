// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Main-thread SDL3 presentation, independent of engine clocks and controls.
//! Source texture dimensions are independent of the aspect-corrected display
//! dimensions. Resizing uses integer display multiples and nearest sampling.

pub use sdl3;

use sdl3::{
    event::Event,
    keyboard::KeyboardState,
    pixels::{Color, PixelFormat},
    render::{Canvas, RenderTarget, ScaleMode, Texture, TextureCreator, WindowCanvas},
    video::WindowContext,
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
    _video: sdl3::VideoSubsystem,
    _sdl: sdl3::Sdl,
}

impl Window {
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
            .build()
            .map_err(|error| error.to_string())?;
        window
            .set_minimum_size(display_width, display_height)
            .map_err(|error| error.to_string())?;
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
        let events = sdl.event_pump().map_err(|error| error.to_string())?;
        Ok(Self {
            canvas,
            events,
            _video: video,
            _sdl: sdl,
        })
    }

    pub fn texture_creator(&self) -> FrameCreator {
        self.canvas.texture_creator()
    }

    pub fn poll_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.events.poll_iter()
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
    fn integer_aspect_correction_and_nearest_sampling_preserve_exact_pixels() {
        // Exercise the same presentation path without a desktop or original data.
        for (output_width, output_height) in
            [(320, 240), (640, 480), (800, 600), (960, 720), (1024, 768)]
        {
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
                let actual = readback.with_lock(|pixels| pixels.to_vec());
                let scale = (output_width / 320).min(output_height / 240) as usize;
                let width = 320 * scale;
                let height = 240 * scale;
                let left = (output_width as usize - width) / 2;
                let top = (output_height as usize - height) / 2;
                assert_eq!(readback.width(), output_width);
                assert_eq!(readback.height(), output_height);
                assert_eq!(
                    actual.len(),
                    output_width as usize * output_height as usize * 3
                );
                for (index, actual) in actual.chunks_exact(3).enumerate() {
                    let x = index % output_width as usize;
                    let y = index / output_width as usize;
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
