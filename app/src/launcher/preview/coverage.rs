// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! CPU evaluation of egui's preview contour, kept out of the shared painter.
use super::{FrameKey, MARGIN};
use reforge_sdl::egui::{
    self, Color32, Pos2, Shape, Stroke, StrokeKind,
    epaint::{ClippedShape, Mesh, Primitive, Tessellator},
};

#[derive(Clone, Copy, Default)]
struct Pixel {
    neutral: Color32,
    content: f32,
}
pub(super) struct FrameMask {
    key: FrameKey,
    pixels: Vec<Pixel>,
}
fn meshes(key: FrameKey, fill: Color32, neutral: bool) -> Vec<Mesh> {
    let rect = key.rect();
    let stroke = Stroke::new(
        1.0_f32,
        if key.light {
            Color32::from_black_alpha(38)
        } else {
            Color32::from_white_alpha(22)
        },
    );
    let shapes = if neutral {
        vec![
            Shape::rect_filled(rect, key.radius, fill),
            Shape::rect_stroke(rect, key.radius, stroke, StrokeKind::Inside),
        ]
    } else {
        let mut shape = egui::epaint::RectShape::filled(rect, key.radius, fill);
        shape.stroke = stroke;
        shape.stroke_kind = StrokeKind::Inside;
        vec![Shape::Rect(shape)]
    };
    Tessellator::new(key.density(), Default::default(), [1, 1], vec![])
        .tessellate_shapes(
            shapes
                .into_iter()
                .map(|shape| ClippedShape {
                    clip_rect: egui::Rect::EVERYTHING,
                    shape,
                })
                .collect(),
        )
        .into_iter()
        .filter_map(|p| {
            if let Primitive::Mesh(mut m) = p.primitive {
                for v in &mut m.vertices {
                    v.pos = v.pos * key.density();
                }
                Some(m)
            } else {
                None
            }
        })
        .collect()
}
impl FrameMask {
    pub(super) fn new(key: FrameKey) -> Self {
        let [w, h] = key.size();
        let inset = ((f32::from(key.radius) + 2.0) * key.density()).ceil() as usize;
        let centre = [
            MARGIN as usize + inset,
            MARGIN as usize + inset,
            (MARGIN as usize + key.width as usize).saturating_sub(inset),
            (MARGIN as usize + key.height as usize).saturating_sub(inset),
        ];
        let centre = if centre[0] < centre[2] && centre[1] < centre[3] {
            centre
        } else {
            [0; 4]
        };
        let mut neutral = vec![[0.0_f32; 4]; w * h];
        visit(
            &meshes(key, key.base(), true),
            [w, h],
            centre,
            |index, colour, _| {
                let dst = &mut neutral[index];
                let alpha = 1.0 - colour[3] / 255.0;
                for c in 0..4 {
                    dst[c] = colour[c] + dst[c] * alpha;
                }
            },
        );
        let mut pixels = vec![Pixel::default(); w * h];
        for y in centre[1]..centre[3] {
            pixels[y * w + centre[0]..y * w + centre[2]].fill(Pixel {
                neutral: key.base(),
                content: 1.0,
            });
        }
        for y in 0..h {
            for x in spans(y, 0, w, centre).into_iter().flatten() {
                let i = y * w + x;
                let n = neutral[i].map(|c| c.round().clamp(0.0, 255.0) as u8);
                pixels[i].neutral = Color32::from_rgba_premultiplied(n[0], n[1], n[2], n[3]);
            }
        }
        drop(neutral);
        let mut black = meshes(key, Color32::BLACK, false);
        let white = meshes(key, Color32::WHITE, false);
        for (b, w) in black.iter_mut().zip(&white) {
            debug_assert_eq!(b.indices, w.indices);
            for (b, w) in b.vertices.iter_mut().zip(&w.vertices) {
                debug_assert_eq!(b.pos, w.pos);
                b.uv.x = (f32::from(w.color.r()) - f32::from(b.color.r())) / 255.0;
            }
        }
        visit(&black, [w, h], centre, |index, _, content| {
            pixels[index].content = content.clamp(0.0, 1.0)
        });
        Self { key, pixels }
    }
    pub(super) fn byte_size(&self) -> usize {
        self.pixels.len() * std::mem::size_of::<Pixel>()
    }
    /// Content is opaque; unchanged rows (e.g. below a palette strip) reuse the
    /// neutral reference without image sampling or per-channel composition.
    pub(super) fn compose(
        &self,
        rows: usize,
        sample: impl Fn(Pos2) -> Color32,
    ) -> egui::ColorImage {
        let [w, h] = self.key.size();
        let base = self.key.base().to_array();
        let mut image = egui::ColorImage {
            size: [w, h],
            pixels: self.pixels.iter().map(|p| p.neutral).collect(),
        };
        for (i, out) in image.pixels.iter_mut().take(rows.min(h) * w).enumerate() {
            let p = self.pixels[i];
            if p.content == 0.0 {
                continue;
            }
            let uv = Pos2::new(
                ((i % w) as f32 + 0.5 - MARGIN as f32) / self.key.width as f32,
                ((i / w) as f32 + 0.5 - MARGIN as f32) / self.key.height as f32,
            );
            let colour = sample(uv);
            debug_assert_eq!(colour.a(), 255);
            if p.content == 1.0 && p.neutral.a() == 255 {
                *out = colour;
                continue;
            }
            let c = colour.to_array();
            let n = p.neutral.to_array();
            // N + weight * (C - base): preserve the neutral frame and replace
            // only its content contribution, including the shared inner fringe.
            let channel = |k| {
                (f32::from(n[k]) + (f32::from(c[k]) - f32::from(base[k])) * p.content)
                    .round()
                    .clamp(0.0, f32::from(n[3])) as u8
            };
            *out = Color32::from_rgba_premultiplied(channel(0), channel(1), channel(2), n[3]);
        }
        image
    }
}

// All coverage work is confined to the perimeter; the guaranteed opaque
// rectangular centre is initialized directly, including during large resizes.
fn spans(y: usize, left: usize, right: usize, centre: [usize; 4]) -> [std::ops::Range<usize>; 2] {
    if y >= centre[1] && y < centre[3] {
        [left..right.min(centre[0]), left.max(centre[2])..right]
    } else {
        [left..right, 0..0]
    }
}

/// The same top-left coverage for every triangle; no rectangle recognition or
/// independently rounded per-colour cells. One 1/256-pixel grid serves all DPI.
fn visit(
    meshes: &[Mesh],
    [w, h]: [usize; 2],
    centre: [usize; 4],
    mut output: impl FnMut(usize, [f32; 4], f32),
) {
    let cross = |a: (i64, i64), b: (i64, i64), p: (i64, i64)| {
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    };
    for mesh in meshes {
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let mut v: [_; 3] = std::array::from_fn(|i| mesh.vertices[triangle[i] as usize]);
            let mut points = v.map(|v| {
                (
                    (v.pos.x * 256.0).round() as i64,
                    (v.pos.y * 256.0).round() as i64,
                )
            });
            let mut area = cross(points[0], points[1], points[2]);
            if area == 0 {
                continue;
            }
            if area < 0 {
                v.swap(1, 2);
                points.swap(1, 2);
                area = -area;
            }
            let bound = |axis: usize, limit: usize| {
                let values = points.map(|p| if axis == 0 { p.0 } else { p.1 });
                (
                    values
                        .iter()
                        .min()
                        .unwrap()
                        .div_euclid(256)
                        .clamp(0, limit as i64) as usize,
                    (values.iter().max().unwrap().div_euclid(256) + 1).clamp(0, limit as i64)
                        as usize,
                )
            };
            let (left, right) = bound(0, w);
            let (top, bottom) = bound(1, h);
            for y in top..bottom {
                for x in spans(y, left, right, centre).into_iter().flatten() {
                    let p = (x as i64 * 256 + 128, y as i64 * 256 + 128);
                    let mut weight = [0.0; 3];
                    let mut inside = true;
                    for (i, weight) in weight.iter_mut().enumerate() {
                        let a = points[(i + 1) % 3];
                        let b = points[(i + 2) % 3];
                        let edge = cross(a, b, p);
                        if edge < 0 || (edge == 0 && !(b.1 < a.1 || (b.1 == a.1 && b.0 > a.0))) {
                            inside = false;
                            break;
                        }
                        *weight = edge as f32 / area as f32;
                    }
                    if inside {
                        output(
                            y * w + x,
                            std::array::from_fn(|c| {
                                (0..3).map(|i| f32::from(v[i].color[c]) * weight[i]).sum()
                            }),
                            (0..3).map(|i| v[i].uv.x * weight[i]).sum(),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_content_reproduces_frame_and_opaque_content_has_no_internal_gaps() {
        for (radius, light) in [(2, false), (6, false), (1, false), (3, true)] {
            for density in [1.0_f32, 1.25, 1.5, 2.0, 2.25, 2.5, 3.0] {
                for dimmed in [false, true] {
                    let key = FrameKey {
                        width: (112.3 * density).round() as u32,
                        height: (84.4 * density).round() as u32,
                        density: density.to_bits(),
                        radius,
                        light,
                        dimmed,
                    };
                    let mask = FrameMask::new(key);
                    let neutral = mask.compose(key.size()[1], |_| key.base());
                    let content = Color32::from_rgb(90, 235, 255);
                    let filled = mask.compose(key.size()[1], |_| content);
                    for (i, p) in mask.pixels.iter().enumerate() {
                        assert_eq!(neutral.pixels[i], p.neutral, "{key:?}");
                        assert!((0.0..=1.0).contains(&p.content));
                        if p.content == 0.0 {
                            assert_eq!(filled.pixels[i], p.neutral);
                        }
                        if p.content == 1.0 && p.neutral.a() == 255 {
                            assert_eq!(filled.pixels[i], content, "{key:?} index {i}");
                        }
                        let c = filled.pixels[i];
                        assert!(c.r() <= c.a() && c.g() <= c.a() && c.b() <= c.a());
                    }
                    let [w, h] = key.size();
                    assert!(filled.pixels[..w].iter().all(|p| p.a() == 0));
                    assert!(filled.pixels[(h - 1) * w..].iter().all(|p| p.a() == 0));
                }
            }
        }
    }
}
