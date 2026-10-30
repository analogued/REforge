// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Clean engine preview frames and a size-aware, illustrative CRT treatment.
//! Launcher cues are deliberately independent of the gameplay Hyllian shader.
use crate::{AspectRatio, DisplayGeometry};
use egui::{Color32, ColorImage};

#[derive(Clone, Debug)]
pub struct Frame {
    geometry: DisplayGeometry,
    rgba: Vec<u8>,
}
impl Frame {
    /// Previews are opaque native pixels; geometry also describes their intended display aspect.
    pub fn new(geometry: DisplayGeometry, rgba: Vec<u8>) -> Result<Self, String> {
        let (w, h) = geometry.display_size(AspectRatio::Square);
        let len = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4));
        if len != Some(rgba.len())
            || rgba.len() > 64 * 1024 * 1024
            || rgba.chunks_exact(4).any(|p| p[3] != 255)
        {
            return Err("Invalid opaque preview frame".into());
        }
        Ok(Self { geometry, rgba })
    }
    pub fn geometry(&self) -> DisplayGeometry {
        self.geometry
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Produce one final-resolution image. Keep this output at 1:1 physical pixels;
/// shrinking a prefiltered CRT mask is precisely what this treatment avoids.
/// The agreed launcher cue is fixed: scanlines 0.30, spacing 3 UI units, mask 20%.
/// Content is contained in the supplied cell, preserving the engine's aspect.
pub fn render(frame: &Frame, size: [usize; 2], density: f32, crt: bool) -> ColorImage {
    let [width, height] = size;
    let (sw, sh) = frame.geometry.display_size(AspectRatio::Square);
    let (aw, ah) = frame.geometry.display_size(AspectRatio::Original);
    let scale = (width as f64 / f64::from(aw)).min(height as f64 / f64::from(ah));
    let iw = (f64::from(aw) * scale).round().max(1.0) as usize;
    let ih = (f64::from(ah) * scale).round().max(1.0) as usize;
    let left = width.saturating_sub(iw) / 2;
    let top = height.saturating_sub(ih) / 2;
    let mut image = ColorImage::new(size, Color32::BLACK);
    let pitch = (3.0 * density).round().max(3.0) as usize;
    for y in 0..ih.min(height) {
        let sy = ((y * 2 + 1) * sh as usize / (ih * 2)).min(sh as usize - 1);
        let phase = (y % pitch) as f32 / pitch as f32;
        let beam = if crt {
            (1.0 - 0.30 * (0.5 + 0.5 * (std::f32::consts::TAU * phase).cos())) / 0.85
        } else {
            1.0
        };
        for x in 0..iw.min(width) {
            let sx = ((x * 2 + 1) * sw as usize / (iw * 2)).min(sw as usize - 1);
            let offset = (sy * sw as usize + sx) * 4;
            let mut rgb = [0; 3];
            for (channel, out) in rgb.iter_mut().enumerate() {
                let mut value = f32::from(frame.rgba[offset + channel]);
                let mut mask = 1.0;
                if crt {
                    let l = (sy * sw as usize + sx.saturating_sub(1)) * 4;
                    let r = (sy * sw as usize + (sx + 1).min(sw as usize - 1)) * 4;
                    value = value * 0.90
                        + f32::from(frame.rgba[l + channel]) * 0.05
                        + f32::from(frame.rgba[r + channel]) * 0.05;
                    mask = if x % 3 == channel {
                        1.0 + 0.20 * (2.0 / 3.0)
                    } else {
                        1.0 - 0.20 / 3.0
                    };
                }
                *out = (value * beam * mask).round().clamp(0.0, 255.0) as u8;
            }
            image.pixels[(top + y) * width + left + x] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_crt_remains_visible_without_only_dimming() {
        let frame = Frame::new(
            DisplayGeometry::new((2, 2), (4, 3)),
            [128, 128, 128, 255].repeat(4),
        )
        .unwrap();
        for d in [1.0, 1.5, 2.0, 2.25, 3.0] {
            let size = [
                (56.0_f32 * d).round() as usize,
                (42.0_f32 * d).round() as usize,
            ];
            let clean = render(&frame, size, d, false);
            assert!(clean.pixels.iter().all(|p| *p == Color32::from_gray(128)));
            let crt = render(&frame, size, d, true);
            let min = crt.pixels.iter().map(|p| p.r()).min().unwrap();
            let max = crt.pixels.iter().map(|p| p.r()).max().unwrap();
            let mean =
                crt.pixels.iter().map(|p| f32::from(p.r())).sum::<f32>() / crt.pixels.len() as f32;
            assert!(max - min >= 30);
            assert!((mean - 128.0).abs() < 4.0, "{d} {mean}");
            assert!(crt.pixels.iter().all(|p| p.a() == 255));
        }
    }
    #[test]
    fn native_validation_and_wide_aspect_containment() {
        let g = DisplayGeometry::new((2, 1), (16, 9));
        assert!(Frame::new(g, vec![0; 8]).is_err());
        assert!(Frame::new(g, vec![255; 4]).is_err());
        let frame = Frame::new(g, vec![255; 8]).unwrap();
        let image = render(&frame, [160, 120], 2.25, false);
        assert!(
            image.pixels[..15 * 160]
                .iter()
                .all(|p| *p == Color32::BLACK)
        );
        assert!(
            image.pixels[15 * 160..105 * 160]
                .iter()
                .all(|p| *p == Color32::WHITE)
        );
        assert!(
            image.pixels[105 * 160..]
                .iter()
                .all(|p| *p == Color32::BLACK)
        );
    }
}
