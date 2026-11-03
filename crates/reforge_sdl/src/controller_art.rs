// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Gidedin controller diagrams and Kenney prompts; retained CC0 masters and
//! regeneration instructions are in assets/controller/. Embedded coverage masks
//! avoid an SVG runtime and are tinted only when the diagram selection changes.

use std::{io::Cursor, sync::OnceLock};

use egui::{
    Color32, ColorImage, Id, Rect, Response, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use reforge_settings::{ControllerMovement, FaceButton};

use crate::gamepad::ControllerStyle;

struct Mask {
    size: [usize; 2],
    bytes: Vec<u8>,
}

struct Model {
    view: [f32; 4],
    // Stick, D-pad, South, East, West, North, in the original SVG viewBox.
    points: [[f32; 3]; 6],
    bytes: &'static [u8],
}

const MODELS: [Model; 3] = [
    Model {
        view: [59.0, 20.0, 1417.0, 959.0],
        points: [
            [396.0, 310.0, 112.0],
            [576.0, 530.0, 108.0],
            [1128.0, 416.0, 51.0],
            [1221.0, 327.0, 51.0],
            [1034.0, 327.0, 51.0],
            [1128.0, 233.0, 51.0],
        ],
        bytes: include_bytes!("../assets/controller/generated/xbox-series.png"),
    },
    Model {
        view: [38.0, 40.0, 1460.0, 952.0],
        points: [
            [550.0, 506.0, 116.0],
            [333.0, 307.0, 126.0],
            [1202.0, 390.0, 46.0],
            [1290.0, 306.0, 46.0],
            [1113.0, 306.0, 46.0],
            [1202.0, 220.0, 46.0],
        ],
        bytes: include_bytes!("../assets/controller/generated/dualsense.png"),
    },
    Model {
        view: [69.0, 43.0, 1397.0, 945.0],
        points: [
            [390.0, 329.0, 108.0],
            [561.0, 514.0, 105.0],
            [1132.0, 421.0, 50.0],
            [1224.0, 334.0, 50.0],
            [1037.0, 334.0, 50.0],
            [1132.0, 249.0, 50.0],
        ],
        bytes: include_bytes!("../assets/controller/generated/switch-pro.png"),
    },
];
static MASKS: [OnceLock<Mask>; 3] = [const { OnceLock::new() }; 3];
static GLYPHS: OnceLock<Mask> = OnceLock::new();
const UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
pub(super) const PREFERRED_DIAGRAM_HEIGHT: f32 = 340.0;

fn family(style: ControllerStyle) -> usize {
    match style {
        ControllerStyle::Xbox => 0,
        ControllerStyle::DualSense => 1,
        ControllerStyle::Switch => 2,
    }
}
fn face(button: FaceButton) -> usize {
    match button {
        FaceButton::South => 0,
        FaceButton::East => 1,
        FaceButton::West => 2,
        FaceButton::North => 3,
    }
}
pub(super) fn model_name(style: ControllerStyle) -> &'static str {
    match style {
        ControllerStyle::Xbox => "Xbox Series",
        ControllerStyle::DualSense => "DualSense",
        ControllerStyle::Switch => "Switch Pro",
    }
}
pub(super) fn button_name(style: ControllerStyle, button: FaceButton) -> &'static str {
    const NAMES: [[&str; 4]; 3] = [
        ["A", "B", "X", "Y"],
        ["Cross", "Circle", "Square", "Triangle"],
        ["B", "A", "Y", "X"],
    ];
    NAMES[family(style)][face(button)]
}
fn decode(bytes: &[u8], expected: png::ColorType) -> Mask {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("verified embedded controller PNG header");
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut bytes)
        .expect("verified embedded controller PNG data");
    assert_eq!(info.color_type, expected);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    bytes.truncate(info.buffer_size());
    Mask {
        size: [info.width as usize, info.height as usize],
        bytes,
    }
}

#[derive(Clone, Eq, PartialEq)]
struct DiagramKey {
    family: usize,
    action: FaceButton,
    jump: Option<FaceButton>,
    neutral: Color32,
    accent: Color32,
    size: [usize; 2],
}
#[derive(Clone)]
struct DiagramCache {
    key: DiagramKey,
    texture: TextureHandle,
}
#[derive(Clone)]
struct GlyphCache {
    cell: usize,
    texture: TextureHandle,
}
fn diagram_texture(ui: &Ui, key: DiagramKey) -> TextureHandle {
    let id = Id::new("reforge_controller_diagram_texture");
    let existing = ui.ctx().data(|data| data.get_temp::<DiagramCache>(id));
    if let Some(cache) = &existing {
        if cache.key == key {
            return cache.texture.clone();
        }
    }
    let mask = MASKS[key.family]
        .get_or_init(|| decode(MODELS[key.family].bytes, png::ColorType::GrayscaleAlpha));
    let selected = |group: usize| {
        group == 1
            || group == 2
            || group == face(key.action) + 3
            || key.jump.is_some_and(|button| group == face(button) + 3)
    };
    let colors: [_; 7] = std::array::from_fn(|group| {
        if selected(group) {
            key.accent
        } else {
            key.neutral
        }
    });
    let image = reduce(mask.size, key.size, |index| {
        let group = usize::from(mask.bytes[index * 2]);
        let coverage = f32::from(mask.bytes[index * 2 + 1]) / 255.0;
        colors[group]
            .to_array()
            .map(|channel| f32::from(channel) * coverage)
    });
    let texture = if let Some(mut cache) = existing {
        cache.texture.set(image, TextureOptions::LINEAR);
        cache.texture
    } else {
        ui.ctx()
            .load_texture("controller diagram", image, TextureOptions::LINEAR)
    };
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            id,
            DiagramCache {
                key,
                texture: texture.clone(),
            },
        )
    });
    texture
}

// Average premultiplied coverage over each physical pixel's complete source
// footprint. SDL's single bilinear lookup otherwise skips the thin SVG strokes
// when a 2048-wide master is reduced to a few hundred screen pixels.
fn reduce(source: [usize; 2], target: [usize; 2], pixel: impl Fn(usize) -> [f32; 4]) -> ColorImage {
    fn weights(source: usize, target: usize) -> Vec<Vec<(usize, f32)>> {
        let scale = source as f32 / target as f32;
        (0..target)
            .map(|output| {
                let start = output as f32 * scale;
                let end = (output + 1) as f32 * scale;
                (start.floor() as usize..(end.ceil() as usize).min(source))
                    .map(|index| {
                        (
                            index,
                            (((index + 1) as f32).min(end) - start.max(index as f32)) / scale,
                        )
                    })
                    .collect()
            })
            .collect()
    }
    let horizontal = weights(source[0], target[0]);
    let vertical = weights(source[1], target[1]);
    let mut intermediate = vec![[0.0_f32; 4]; target[0] * source[1]];
    for row in 0..source[1] {
        for (column, samples) in horizontal.iter().enumerate() {
            let value = &mut intermediate[row * target[0] + column];
            for &(input, weight) in samples {
                let rgba = pixel(row * source[0] + input);
                for channel in 0..4 {
                    value[channel] += rgba[channel] * weight;
                }
            }
        }
    }
    let mut image = ColorImage::new(target, Color32::TRANSPARENT);
    for (row, samples) in vertical.iter().enumerate() {
        for column in 0..target[0] {
            let mut value = [0.0_f32; 4];
            for &(input, weight) in samples {
                let rgba = intermediate[input * target[0] + column];
                for channel in 0..4 {
                    value[channel] += rgba[channel] * weight;
                }
            }
            let alpha = value[3].round().clamp(0.0, 255.0) as u8;
            let rgba = value.map(|channel| channel.round().clamp(0.0, f32::from(alpha)) as u8);
            image.pixels[row * target[0] + column] =
                Color32::from_rgba_premultiplied(rgba[0], rgba[1], rgba[2], alpha);
        }
    }
    image
}

/// Paint a labelled prompt into a caller-owned rectangle. The white coverage
/// atlas lets egui apply its ordinary disabled tint exactly once.
pub(super) fn glyph(
    ui: &Ui,
    rect: Rect,
    style: ControllerStyle,
    button: FaceButton,
    color: Color32,
) {
    let cell = (rect.width() * ui.ctx().pixels_per_point())
        .round()
        .clamp(1.0, 128.0) as usize;
    let id = Id::new("reforge_controller_prompt_texture");
    let existing = ui.ctx().data(|data| data.get_temp::<GlyphCache>(id));
    let texture = match existing {
        Some(cache) if cache.cell == cell => cache.texture,
        existing => {
            let mask = GLYPHS.get_or_init(|| {
                decode(
                    include_bytes!("../assets/controller/generated/glyphs.png"),
                    png::ColorType::Grayscale,
                )
            });
            let image = reduce(mask.size, [cell * 4, cell * 3], |index| {
                [f32::from(mask.bytes[index]); 4]
            });
            let texture = if let Some(mut cache) = existing {
                cache.texture.set(image, TextureOptions::LINEAR);
                cache.texture
            } else {
                ui.ctx()
                    .load_texture("controller button prompts", image, TextureOptions::LINEAR)
            };
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    id,
                    GlyphCache {
                        cell,
                        texture: texture.clone(),
                    },
                )
            });
            texture
        }
    };
    let x = face(button) as f32 / 4.0;
    let y = family(style) as f32 / 3.0;
    let uv = Rect::from_min_max(pos2(x, y), pos2(x + 0.25, y + 1.0 / 3.0));
    ui.painter().image(texture.id(), rect, uv, color);
}

/// Allocate one stable diagram rectangle, independent of movement/button choice.
/// Unknown controller families use the Xbox physical-position reference.
/// Short viewports scroll rather than shrinking below 60% of the normal size.
pub(super) fn drawing(
    ui: &mut Ui,
    style: ControllerStyle,
    movement: ControllerMovement,
    action: FaceButton,
    jump: Option<FaceButton>,
    max_height: f32,
) -> Response {
    let family = family(style);
    let model = &MODELS[family];
    let available = ui.available_width();
    let aspect = model.view[3] / model.view[2];
    let size = diagram_size(available, max_height, aspect);
    let width = size.x;
    let height = size.y;
    let (space, response) = ui.allocate_exact_size(vec2(available, height), Sense::hover());
    let rect = Rect::from_center_size(space.center(), vec2(width, height));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let palette = crate::ui::current_palette(ui.ctx());
    // Texture pixels carry the theme colors, so applying egui's fade to its
    // white tint would darken the whole image. Gray those two colors once and
    // clear only this painter's fade; the UI's inherited opacity is retained.
    let color = |color| {
        if ui.is_enabled() {
            color
        } else {
            ui.visuals().gray_out(color)
        }
    };
    let accent = color(palette.accent);
    let neutral = color(palette.noninteractive.foreground);
    let texture = diagram_texture(
        ui,
        DiagramKey {
            family,
            action,
            jump,
            neutral,
            accent,
            size: [rect.width(), rect.height()].map(|size| {
                (size * ui.ctx().pixels_per_point())
                    .round()
                    .clamp(1.0, 2048.0) as usize
            }),
        },
    );
    let mut painter = ui.painter().clone();
    painter.set_fade_to_color(None);
    let scale = width / model.view[2];
    let point = |x, y| rect.min + vec2(x - model.view[0], y - model.view[1]) * scale;
    for button in [Some(action), jump].into_iter().flatten() {
        let [x, y, radius] = model.points[face(button) + 2];
        painter.circle_filled(point(x, y), radius * scale, accent.gamma_multiply(0.13));
    }
    painter.image(texture.id(), rect, UV, Color32::WHITE);
    for [x, y, radius] in &model.points[..2] {
        for sign in [-1.0, 1.0] {
            painter.circle_filled(point(x + radius * 1.22 * sign, *y), 8.0 * scale, accent);
            if movement == ControllerMovement::Directional {
                painter.circle_filled(point(*x, y + radius * 1.22 * sign), 8.0 * scale, accent);
            }
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Image,
            ui.is_enabled(),
            format!(
                "{}. {} for Action.{}",
                model_name(style),
                button_name(style, action),
                jump.map_or(String::new(), |button| format!(
                    " {} for Jump or Drop.",
                    button_name(style, button)
                ))
            ),
        )
    });
    response
}

fn diagram_size(available_width: f32, preferred_height: f32, aspect: f32) -> egui::Vec2 {
    let limit = available_width.clamp(0.0, 700.0);
    let normal_width = (PREFERRED_DIAGRAM_HEIGHT / aspect).min(limit);
    let width = (preferred_height / aspect).clamp(normal_width * 0.60, limit);
    vec2(width, width * aspect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_viewports_keep_diagrams_readable_without_overflow_or_size_jumps() {
        for model in &MODELS {
            let aspect = model.view[3] / model.view[2];
            for available in [180.0, 360.0, 660.0, 900.0] {
                let short = diagram_size(available, 0.0, aspect);
                let normal = diagram_size(available, PREFERRED_DIAGRAM_HEIGHT, aspect);
                assert!(short.x >= normal.x * 0.60);
                assert!(short.x <= available);
                assert_eq!(short.y, short.x * aspect);
                let mut previous = short;
                for height in 1..=600 {
                    let current = diagram_size(available, height as f32, aspect);
                    assert!(current.x >= previous.x);
                    assert!(current.x - previous.x <= 1.0 / aspect + 0.001);
                    previous = current;
                }
            }
            // Keep the existing preferred size when there is enough room.
            let roomy = diagram_size(660.0, 340.0, aspect);
            assert!((roomy.y - 340.0).abs() < 0.001);
            assert!(diagram_size(660.0, 110.0, aspect).x > 290.0);
        }
    }

    #[test]
    fn embedded_masks_have_all_control_regions_and_opaque_body() {
        for model in &MODELS {
            let mask = decode(model.bytes, png::ColorType::GrayscaleAlpha);
            assert_eq!(mask.size[0], 2048);
            let mut groups = [0; 7];
            for pixel in mask.bytes.chunks_exact(2).filter(|pixel| pixel[1] == 255) {
                groups[usize::from(pixel[0])] += 1;
            }
            assert!(groups.iter().all(|&count| count > 50));
            assert_eq!(mask.bytes.len(), mask.size[0] * mask.size[1] * 2);
        }
    }

    #[test]
    fn glyph_atlas_retains_all_twelve_prompts() {
        let mask = decode(
            include_bytes!("../assets/controller/generated/glyphs.png"),
            png::ColorType::Grayscale,
        );
        assert_eq!(mask.size, [512, 384]);
        for row in 0..3 {
            for column in 0..4 {
                assert!((row * 128..(row + 1) * 128).any(|y| {
                    mask.bytes[y * 512 + column * 128..y * 512 + (column + 1) * 128].contains(&255)
                }));
            }
        }
    }

    #[test]
    fn diagram_reuses_texture_until_selection_changes() {
        let context = egui::Context::default();
        let render = |action| {
            let mut rect = Rect::NOTHING;
            let output = context.run(egui::RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    rect = drawing(
                        ui,
                        ControllerStyle::Xbox,
                        ControllerMovement::Directional,
                        action,
                        None,
                        320.0,
                    )
                    .rect;
                });
            });
            (output, rect)
        };
        let (first, first_rect) = render(FaceButton::South);
        let (same, _) = render(FaceButton::South);
        let (changed, changed_rect) = render(FaceButton::East);
        // The first frame may upload the font atlas as well as the diagram.
        let diagram = first
            .textures_delta
            .set
            .iter()
            .find(|(_, delta)| matches!(delta.image, egui::ImageData::Color(_)))
            .unwrap()
            .0;
        assert!(same.textures_delta.set.iter().all(|(id, _)| *id != diagram));
        assert!(
            changed
                .textures_delta
                .set
                .iter()
                .any(|(id, _)| *id == diagram)
        );
        assert_eq!(first_rect, changed_rect);
    }

    #[test]
    fn physical_pixel_reduction_retains_thin_line_coverage() {
        // One source column among eight must still contribute to a small
        // preview; a single bilinear lookup between columns can lose it.
        let image = reduce([8, 4], [1, 1], |index| {
            if index % 8 == 1 {
                [0.0, 200.0, 0.0, 255.0]
            } else {
                [0.0; 4]
            }
        });
        assert_eq!(
            image.pixels,
            vec![Color32::from_rgba_premultiplied(0, 25, 0, 32)]
        );
        // Fractional footprints also preserve a uniform translucent material.
        let image = reduce([7, 5], [3, 2], |_| [20.0, 40.0, 60.0, 80.0]);
        assert!(
            image
                .pixels
                .iter()
                .all(|pixel| *pixel == Color32::from_rgba_premultiplied(20, 40, 60, 80))
        );
    }
}
