// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Render the actual layouts to guard hardware swatches and existing spacing.
use super::*;
use reforge_sdl::{
    DisplayGeometry,
    launcher::{Palette, PaletteHint},
    preview::Frame,
};
use std::sync::Arc;

#[test]
fn sixteen_colour_swatches_preserve_order_and_line_spacing_in_every_layout() {
    let release = detection::RELEASES
        .iter()
        .find(|r| r.game().id() == "centurion-defender-of-rome")
        .unwrap();
    let fixture = Fixture::for_releases([release]);
    // Deliberately different from theme colours and the synthetic white image.
    let colours: Vec<_> = (0..16).map(|i| [11 + i * 11, 23, 37]).collect();
    for theme in ThemeId::ALL {
        for density in [1.0, 1.5, 2.25] {
            for size in [Vec2::new(630.0, 470.0), Vec2::new(1000.0, 720.0)] {
                for layout in LayoutVariant::ALL {
                    let mut h = Harness::new(&fixture, theme, layout, true);
                    h.density = density;
                    h.size = size;
                    h.state.games[0].title = "Fixture".into();
                    let mut baseline = None;
                    for count in [4, 16] {
                        let frame = Frame::new(DisplayGeometry::new((2, 1), (4, 3)), vec![255; 8])
                            .unwrap()
                            .with_palette(Palette {
                                colours: colours[..count].to_vec(),
                                weights: None,
                                hint: PaletteHint::Auto,
                            })
                            .unwrap();
                        let game = &mut h.state.games[0];
                        game.previews.insert(
                            game.preview_variant().into(),
                            preview::sources::Source::new("swatch-fixture".into(), Arc::new(frame)),
                        );
                        let display = if layout == LayoutVariant::GalleryShelf {
                            game.gallery_palette_display()
                        } else {
                            game.palette_display()
                        };
                        assert_eq!(display.colours, colours[..count]);
                        assert!(!display.mosaic);
                        let output = h.settle();
                        // Four-colour swatches retain the previous one-row UI.
                        // Compare every text baseline/height, including details,
                        // tagline, feature labels and the Play button.
                        let text_geometry: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(t) => {
                                    Some((t.galley.text().to_owned(), t.pos.y, t.galley.size().y))
                                }
                                _ => None,
                            })
                            .collect();
                        if count == 4 {
                            baseline = Some(text_geometry);
                            continue;
                        }
                        assert_eq!(
                            &text_geometry,
                            baseline.as_ref().unwrap(),
                            "line spacing changed: {layout:?} {theme:?} {density} {size:?}"
                        );
                        let cells: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| {
                                let (rect, colour) = match &shape.shape {
                                    egui::Shape::Rect(r) => (r.rect, r.fill),
                                    egui::Shape::Circle(c) => (
                                        Rect::from_center_size(
                                            c.center,
                                            Vec2::splat(c.radius * 2.0),
                                        ),
                                        c.fill,
                                    ),
                                    _ => return None,
                                };
                                let rgb = [colour.r(), colour.g(), colour.b()];
                                colours.contains(&rgb).then_some((rect, rgb))
                            })
                            .collect();
                        assert_eq!(cells.len(), 16);
                        assert_eq!(
                            cells.iter().map(|(_, rgb)| *rgb).collect::<Vec<_>>(),
                            colours
                        );
                        let bounds = cells
                            .iter()
                            .map(|(rect, _)| *rect)
                            .reduce(|a, b| a.union(b))
                            .unwrap();
                        for shape in &output.shapes {
                            if let egui::Shape::Text(text) = &shape.shape {
                                // Wrapped labels include the preceding widget's
                                // indentation in their overall galley bounds.
                                for row in &text.galley.rows {
                                    let visible = row
                                        .visuals
                                        .mesh_bounds
                                        .translate(text.pos.to_vec2())
                                        .intersect(shape.clip_rect);
                                    assert!(
                                        !visible.intersects(bounds),
                                        "swatch overlaps text: {layout:?} {theme:?} {density} {size:?}: {}",
                                        text.galley.text()
                                    );
                                }
                            }
                        }
                        let (rows, columns) = if layout == LayoutVariant::GalleryShelf {
                            (4, 4)
                        } else {
                            (2, 8)
                        };
                        for (index, (rect, _)) in cells.iter().enumerate() {
                            assert!(rect.width() >= 6.0, "dots must retain their full size");
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(*rect));
                            assert_eq!(rect.top(), cells[index / columns * columns].0.top());
                            assert_eq!(rect.left(), cells[index % columns].0.left());
                            if index >= columns {
                                assert!(rect.top() > cells[index - columns].0.bottom());
                            }
                        }
                        assert_eq!(cells.chunks(columns).count(), rows);
                        if layout == LayoutVariant::GalleryShelf {
                            let title = text_rect(&output, "Fixture").unwrap();
                            assert!(title.right() <= cells[0].0.left());
                            let metadata =
                                text_rect(&output, &h.state.games[0].library_line()).unwrap();
                            assert!(metadata.right() <= cells[0].0.left());
                        }
                    }
                }
            }
        }
    }
}
