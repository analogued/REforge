// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Software-only large flat fills, using ordinary top-left triangle coverage.
//! Keep SDL's rectangle recognition and textured/gradient rasterization intact.

use sdl3::{
    pixels::Color,
    rect::Rect,
    render::{BlendMode, Canvas, FRect, RenderTarget, Texture, Vertex},
};

// Small shapes gain little and can be recognized as specialized primitives by
// SDL. Restrict the optimization to the expensive large bounding boxes.
const MIN_AREA: i64 = 4096;
const MAX_COORDINATE: f32 = 1_000_000.0;

pub(super) fn paint<T: RenderTarget>(
    canvas: &mut Canvas<T>,
    texture: &Texture<'_>,
    vertices: &[Vertex],
    indices: &[u32],
    font_atlas: bool,
    clip: Rect,
    spans: &mut Vec<FRect>,
) -> Result<(), String> {
    if !font_atlas {
        return geometry(canvas, texture, vertices, indices);
    }
    let mut start = 0;
    let mut cursor = 0;
    while cursor + 2 < indices.len() {
        // SDL has its own quad optimization with different endpoint rounding.
        // Preserve whole rectangle pairs, including their original indices.
        if rectangle_pair(vertices, &indices[cursor..]) {
            cursor += 6;
            continue;
        }
        let triangle: [_; 3] = std::array::from_fn(|i| &vertices[indices[cursor + i] as usize]);
        spans.clear();
        if flat_white(&triangle) && triangle_spans(&triangle, clip, spans) {
            geometry(canvas, texture, vertices, &indices[start..cursor])?;
            let color = triangle[0].color;
            let byte = |channel: f32| (channel * 255.0).round() as u8;
            canvas.set_draw_color(Color::RGBA(
                byte(color.r),
                byte(color.g),
                byte(color.b),
                byte(color.a),
            ));
            canvas.set_blend_mode(if color.a == 1.0 {
                BlendMode::None
            } else {
                BlendMode::Blend
            });
            if !spans.is_empty() {
                canvas
                    .fill_rects(spans)
                    .map_err(|error| error.to_string())?;
            }
            cursor += 3;
            start = cursor;
        } else {
            cursor += 3;
        }
    }
    geometry(canvas, texture, vertices, &indices[start..])
}

fn geometry<T: RenderTarget>(
    canvas: &mut Canvas<T>,
    texture: &Texture<'_>,
    vertices: &[Vertex],
    indices: &[u32],
) -> Result<(), String> {
    if indices.is_empty() {
        return Ok(());
    }
    canvas
        .render_geometry(vertices, Some(texture), indices)
        .map_err(|error| error.to_string())
}

fn flat_white(vertices: &[&Vertex]) -> bool {
    vertices.iter().all(|vertex| {
        vertex.tex_coord.x == 0.0 && vertex.tex_coord.y == 0.0 && vertex.color == vertices[0].color
    })
}

fn rectangle_pair(vertices: &[Vertex], indices: &[u32]) -> bool {
    if indices.len() < 6 {
        return false;
    }
    let points: [_; 6] = std::array::from_fn(|i| &vertices[indices[i] as usize]);
    if !flat_white(&points) {
        return false;
    }
    let min_x = points
        .iter()
        .map(|v| v.position.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|v| v.position.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points
        .iter()
        .map(|v| v.position.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|v| v.position.y)
        .fold(f32::NEG_INFINITY, f32::max);
    if !(max_x > min_x && max_y > min_y) {
        return false;
    }
    let mut corners = 0;
    for point in &points {
        let p = point.position;
        if (p.x != min_x && p.x != max_x) || (p.y != min_y && p.y != max_y) {
            return false;
        }
        corners |= 1 << (u8::from(p.x == max_x) + 2 * u8::from(p.y == max_y));
    }
    if corners != 15 {
        return false;
    }
    let mut shared = [points[0].position; 2];
    let mut count = 0;
    for a in &points[..3] {
        for b in &points[3..] {
            if a.position == b.position {
                if count == 2 {
                    return false;
                }
                shared[count] = a.position;
                count += 1;
            }
        }
    }
    count == 2 && shared[0].x != shared[1].x && shared[0].y != shared[1].y
}

fn triangle_spans(vertices: &[&Vertex; 3], clip: Rect, spans: &mut Vec<FRect>) -> bool {
    if vertices.iter().any(|v| {
        !v.position.x.is_finite()
            || !v.position.y.is_finite()
            || v.position.x.abs() > MAX_COORDINATE
            || v.position.y.abs() > MAX_COORDINATE
    }) {
        return false;
    }
    // The software geometry queue truncates physical vertex coordinates first.
    let mut points = vertices.map(|v| (v.position.x as i64, v.position.y as i64));
    let min_x = points.iter().map(|p| p.0).min().unwrap();
    let max_x = points.iter().map(|p| p.0).max().unwrap();
    let min_y = points.iter().map(|p| p.1).min().unwrap();
    let max_y = points.iter().map(|p| p.1).max().unwrap();
    if (max_x - min_x) * (max_y - min_y) < MIN_AREA {
        return false;
    }
    let area = (points[1].0 - points[0].0) * (points[2].1 - points[0].1)
        - (points[1].1 - points[0].1) * (points[2].0 - points[0].0);
    if area == 0 {
        return true;
    }
    if area < 0 {
        points.swap(1, 2);
    }
    // Doubled coordinates make pixel centres exact integers, without epsilon
    // comparisons. The three half-planes bound one contiguous span per row.
    points.iter_mut().for_each(|p| {
        p.0 *= 2;
        p.1 *= 2;
    });
    for row in min_y.max(i64::from(clip.y()))..max_y.min(i64::from(clip.bottom())) {
        let mut left = i64::from(clip.x());
        let mut right = i64::from(clip.right()) - 1;
        for i in 0..3 {
            let (ax, ay) = points[i];
            let (bx, by) = points[(i + 1) % 3];
            let (dx, dy) = (bx - ax, by - ay);
            let step = -2 * dy;
            let value = dx * (2 * row + 1 - ay) - dy * (1 - ax);
            let threshold = i64::from(!(dy < 0 || (dy == 0 && dx > 0)));
            if step > 0 {
                left = left.max(-(value - threshold).div_euclid(step));
            } else if step < 0 {
                right = right.min((value - threshold).div_euclid(-step));
            } else if value < threshold {
                right = left - 1;
                break;
            }
        }
        if left <= right {
            spans.push(FRect::new(
                left as f32,
                row as f32,
                (right - left + 1) as f32,
                1.0,
            ));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl3::{
        pixels::{FColor, PixelFormat},
        render::{FPoint, ScaleMode},
        surface::Surface,
    };

    fn vertex(x: f32, y: f32, alpha: u8) -> Vertex {
        Vertex {
            position: FPoint::new(x, y),
            color: FColor::RGBA(
                63.0 / 255.0,
                119.0 / 255.0,
                203.0 / 255.0,
                f32::from(alpha) / 255.0,
            ),
            tex_coord: FPoint::new(0.0, 0.0),
        }
    }

    fn render(
        vertices: &[Vertex],
        indices: &[u32],
        clip: Rect,
        optimized: bool,
        font: bool,
    ) -> Vec<u8> {
        let mut canvas = Surface::new(256, 160, PixelFormat::RGBA32)
            .unwrap()
            .into_canvas()
            .unwrap();
        canvas.set_draw_color(Color::RGB(31, 67, 109));
        canvas.clear();
        canvas.set_clip_rect(clip);
        let creator = canvas.texture_creator();
        let mut texture = creator
            .create_texture_streaming(PixelFormat::RGBA32, 2, 2)
            .unwrap();
        texture.update(None, &[255; 16], 8).unwrap();
        texture.set_blend_mode(BlendMode::Blend);
        texture.set_scale_mode(ScaleMode::Linear);
        if optimized {
            paint(
                &mut canvas,
                &texture,
                vertices,
                indices,
                font,
                clip,
                &mut Vec::new(),
            )
            .unwrap();
        } else {
            geometry(&mut canvas, &texture, vertices, indices).unwrap();
        }
        canvas.set_clip_rect(None);
        canvas
            .read_pixels(None)
            .unwrap()
            .convert_format(PixelFormat::RGBA32)
            .unwrap()
            .with_lock(|pixels| pixels.to_vec())
    }

    fn assert_pixels(actual: &[u8], expected: &[u8], context: &str) {
        assert_eq!(actual.len(), expected.len(), "{context}");
        let differences = actual.iter().zip(expected).filter(|(a, b)| a != b).count();
        let first = actual.iter().zip(expected).position(|(a, b)| a != b);
        assert_eq!(differences, 0, "{context}: first differing byte {first:?}");
    }

    #[test]
    fn flat_fills_match_native_sdl_for_winding_alpha_clip_and_fractional_positions() {
        for alpha in [0, 16, 64, 128, 165, 255] {
            for offset in [-20.75, -0.25, 0.0, 0.5, 12.25] {
                for clip in [Rect::new(0, 0, 256, 160), Rect::new(19, 27, 180, 90)] {
                    let vertices = [
                        vertex(4.0 + offset, 9.25, alpha),
                        vertex(249.75, 30.5 + offset, alpha),
                        vertex(87.25, 155.75, alpha),
                    ];
                    let indices = [0, 1, 2];
                    let expected = render(&vertices, &indices, clip, false, true);
                    for order in [[0, 1, 2], [0, 2, 1]] {
                        let actual = render(&vertices, &order, clip, true, true);
                        assert_pixels(
                            &actual,
                            &expected,
                            &format!("alpha={alpha} offset={offset} clip={clip:?} order={order:?}"),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rectangle_pairs_textures_and_gradients_keep_the_native_path() {
        let clip = Rect::new(0, 0, 256, 160);
        let mut vertices = vec![
            vertex(10.75, 10.25, 165),
            vertex(240.75, 10.25, 165),
            vertex(240.75, 140.5, 165),
            vertex(10.75, 140.5, 165),
        ];
        let indices = [0, 1, 2, 2, 3, 0];
        assert!(rectangle_pair(&vertices, &indices));
        let expected = render(&vertices, &indices, clip, false, true);
        assert_pixels(
            &render(&vertices, &indices, clip, true, true),
            &expected,
            "quad",
        );
        vertices[0].tex_coord = FPoint::new(0.5, 0.5);
        assert!(!rectangle_pair(&vertices, &indices));
        assert_pixels(
            &render(&vertices, &indices, clip, true, false),
            &render(&vertices, &indices, clip, false, false),
            "texture",
        );
        vertices[0].tex_coord = FPoint::new(0.0, 0.0);
        vertices[1].color.a = 0.1;
        assert_pixels(
            &render(&vertices, &[0, 1, 2], clip, true, true),
            &render(&vertices, &[0, 1, 2], clip, false, true),
            "gradient",
        );
    }

    #[test]
    fn interleaved_native_quads_and_flat_triangles_preserve_alpha_order() {
        let mut vertices = vec![
            vertex(0.5, 0.5, 128),
            vertex(250.5, 0.5, 128),
            vertex(250.5, 150.5, 128),
            vertex(0.5, 150.5, 128),
        ];
        vertices.extend([
            vertex(7.0, 9.0, 64),
            vertex(240.5, 40.5, 64),
            vertex(40.5, 159.0, 64),
        ]);
        vertices.extend([
            vertex(50.25, 20.25, 165),
            vertex(230.75, 20.25, 165),
            vertex(230.75, 130.75, 165),
            vertex(50.25, 130.75, 165),
        ]);
        let indices = [0, 1, 2, 2, 3, 0, 4, 5, 6, 7, 8, 9, 9, 10, 7];
        let clip = Rect::new(5, 5, 240, 148);
        assert_pixels(
            &render(&vertices, &indices, clip, true, true),
            &render(&vertices, &indices, clip, false, true),
            "interleaved",
        );
    }

    #[test]
    fn offscreen_degenerate_and_small_shapes_do_not_expand_their_coverage() {
        for positions in [
            [(-300.0, -300.0), (-40.0, -250.0), (-100.0, -40.0)],
            [(0.0, 0.0), (100.0, 100.0), (150.0, 150.0)],
            [(4.0, 4.0), (19.0, 4.0), (12.0, 20.0)],
        ] {
            let vertices = positions.map(|p| vertex(p.0, p.1, 165));
            let clip = Rect::new(0, 0, 256, 160);
            assert_pixels(
                &render(&vertices, &[0, 1, 2], clip, true, true),
                &render(&vertices, &[0, 1, 2], clip, false, true),
                "offscreen/degenerate/small",
            );
        }
    }
}
