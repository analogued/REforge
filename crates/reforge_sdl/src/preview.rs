// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Clean engine preview frames and a size-aware, illustrative CRT treatment.
//! Launcher cues are deliberately independent of the gameplay Hyllian shader.
use crate::{AspectRatio, DisplayGeometry};
use egui::{Color32, ColorImage};

#[derive(Clone, Debug)]
pub struct Frame {
    geometry: DisplayGeometry,
    rgba: Vec<u8>,
    monochrome: bool,
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
        Ok(Self {
            geometry,
            rgba,
            monochrome: false,
        })
    }
    /// A single neutral phosphor: the CRT cue must not introduce RGB subpixels.
    pub fn new_monochrome(geometry: DisplayGeometry, rgba: Vec<u8>) -> Result<Self, String> {
        let mut frame = Self::new(geometry, rgba)?;
        if frame
            .rgba
            .chunks_exact(4)
            .any(|p| p[0] != p[1] || p[1] != p[2])
        {
            return Err("Monochrome preview contains coloured source pixels".into());
        }
        frame.monochrome = true;
        Ok(frame)
    }
    pub fn is_monochrome(&self) -> bool {
        self.monochrome
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
    // Filter source footprints before adding any destination-grid CRT pattern.
    // Each axis averages when shrinking and retains nearest sampling when growing.
    let xs = crt.then(|| axis_samples(sw as usize, iw));
    let ys = crt.then(|| axis_samples(sh as usize, ih));
    let mut row = if crt {
        vec![[0.0_f32; 3]; sw as usize]
    } else {
        vec![]
    };
    let mut cached_row = None;
    for y in 0..ih.min(height) {
        let sy = ((y * 2 + 1) * sh as usize / (ih * 2)).min(sh as usize - 1);
        if let Some(ys) = &ys {
            let weights = &ys[y];
            let single = if weights.len() == 1 {
                Some(weights[0].0)
            } else {
                None
            };
            if single.is_none() || single != cached_row {
                row.fill([0.0; 3]);
                for &(source_y, weight) in weights {
                    for (x, pixel) in row.iter_mut().enumerate() {
                        let offset = (source_y * sw as usize + x) * 4;
                        for (c, out) in pixel.iter_mut().enumerate() {
                            *out += f32::from(frame.rgba[offset + c]) * weight;
                        }
                    }
                }
                cached_row = single;
            }
        }
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
                let value = if let Some(xs) = &xs {
                    xs[x]
                        .iter()
                        .map(|&(sx, weight)| {
                            (row[sx][channel] * 0.90
                                + row[sx.saturating_sub(1)][channel] * 0.05
                                + row[(sx + 1).min(sw as usize - 1)][channel] * 0.05)
                                * weight
                        })
                        .sum()
                } else {
                    f32::from(frame.rgba[offset + channel])
                };
                let mask = if crt && !frame.monochrome {
                    if x % 3 == channel {
                        1.0 + 0.20 * (2.0 / 3.0)
                    } else {
                        1.0 - 0.20 / 3.0
                    }
                } else {
                    1.0
                };
                *out = (value * beam * mask).round().clamp(0.0, 255.0) as u8;
            }
            image.pixels[(top + y) * width + left + x] = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        }
    }
    image
}

/// Exact box footprints suppress beat patterns when dense native detail shrinks.
/// Use f64 for boundary arithmetic; normalize weights per destination pixel.
fn axis_samples(source: usize, destination: usize) -> Vec<Vec<(usize, f32)>> {
    if destination >= source {
        return (0..destination)
            .map(|x| {
                vec![(
                    ((x * 2 + 1) * source / (destination * 2)).min(source - 1),
                    1.0,
                )]
            })
            .collect();
    }
    let scale = source as f64 / destination as f64;
    (0..destination)
        .map(|x| {
            let start = x as f64 * scale;
            let end = (x + 1) as f64 * scale;
            (start.floor() as usize..(end.ceil() as usize).min(source))
                .filter_map(|sx| {
                    let coverage = (end.min((sx + 1) as f64) - start.max(sx as f64)).max(0.0);
                    (coverage > 0.0).then_some((sx, (coverage / scale) as f32))
                })
                .collect()
        })
        .collect()
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
    fn monochrome_crt_preserves_gray_channels_at_every_preview_size() {
        let rgba = (0..640 * 200)
            .flat_map(|i| {
                let v = if (i / 640 + i % 640) % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect();
        let frame =
            Frame::new_monochrome(DisplayGeometry::new((640, 200), (320, 240)), rgba).unwrap();
        for size in [[56, 42], [126, 95], [241, 181], [480, 360], [800, 600]] {
            for density in [1.0, 1.5, 2.25, 3.0] {
                let image = render(&frame, size, density, true);
                assert!(
                    image
                        .pixels
                        .iter()
                        .all(|p| p.r() == p.g() && p.g() == p.b())
                );
            }
        }
        assert!(
            Frame::new_monochrome(DisplayGeometry::new((1, 1), (1, 1)), vec![10, 20, 30, 255])
                .is_err()
        );
    }
    #[test]
    fn reduced_native_stripes_average_instead_of_turning_into_broad_bands() {
        let rgba = (0..640 * 200)
            .flat_map(|i| {
                let v = if i % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect();
        let frame =
            Frame::new_monochrome(DisplayGeometry::new((640, 200), (320, 240)), rgba).unwrap();
        // Ratios just over 2:1 make point-sampled stripes form conspicuous beats.
        for width in [56, 113, 160, 241, 319, 320] {
            let height = width * 3 / 4;
            let image = render(&frame, [width, height], 2.25, true);
            let row = &image.pixels[10 * width + 4..11 * width - 4];
            let min = row.iter().map(|p| p.r()).min().unwrap();
            let max = row.iter().map(|p| p.r()).max().unwrap();
            assert!(min > 60 && max < 190, "{width}: {min}..{max}");
            if width == 319 || width == 320 {
                assert!(max - min <= 2, "{width}: {min}..{max}");
            }
        }
        for (source, destination) in [(640, 56), (640, 319), (200, 42), (320, 800)] {
            for taps in axis_samples(source, destination) {
                assert!((taps.iter().map(|(_, w)| w).sum::<f32>() - 1.0).abs() < 1e-5);
            }
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
