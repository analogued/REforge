// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Split sizing uses authored frames with both narrow and widescreen geometry.
use super::*;
use reforge_sdl::{DisplayGeometry, preview::Frame};
use std::sync::Arc;

#[test]
fn split_height_defaults_and_reloads_without_changing_other_presentation_choices() {
    let fixture = Fixture::new(1);
    let mut state = fixture.state(ThemeId::CrispLight, LayoutVariant::GalleryShelf, true);
    assert_eq!(state.split_preview_height, 180);
    reforge_settings::launcher::save_split_preview_height(&fixture.config, 156).unwrap();
    state.reload_preferences();
    assert_eq!(state.split_preview_height, 156);
    assert_eq!(state.layout, LayoutVariant::GalleryShelf);
    assert_eq!(state.saved_theme, ThemeId::CrispLight);
    let reopened = State::new_with(fixture.library.clone(), fixture.config.clone(), |_| {
        Err("synthetic installation".into())
    });
    assert_eq!(reopened.split_preview_height, 156);
}

#[test]
fn split_heights_preserve_aspect_and_release_play_alignment_only_for_text() {
    let fixture = Fixture::new(1);
    let edition = detection::RELEASES
        .iter()
        .find(|release| release.game().id() == "centurion-defender-of-rome")
        .unwrap()
        .edition;
    let mut aligned = 0;
    let mut released = 0;
    for density in [1.0, 2.25] {
        for width in [630.0, 775.0, 1000.0, 1180.0] {
            for preferred in [144, 180, 192] {
                let mut pair = Vec::new();
                for aspect in [(4, 3), (16, 9)] {
                    let mut h = Harness::new(
                        &fixture,
                        ThemeId::CrispLight,
                        LayoutVariant::SplitInspector,
                        true,
                    );
                    h.density = density;
                    h.size = Vec2::new(width, 720.0);
                    h.state.split_preview_height = preferred;
                    let game = &mut h.state.games[0];
                    game.edition = Some(edition);
                    game.title = edition.title().into();
                    game.previews.insert(
                        game.preview_variant().into(),
                        preview::sources::Source::new(
                            "synthetic-height".into(),
                            Arc::new(
                                Frame::new(DisplayGeometry::new((2, 1), aspect), vec![255; 8])
                                    .unwrap(),
                            ),
                        ),
                    );
                    let output = h.settle();
                    let image = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if matches!(mesh.texture_id, egui::TextureId::Managed(id) if id > 0) => {
                            Some(mesh.calc_bounds().shrink(2.0 / density))
                        }
                        _ => None,
                    }).unwrap();
                    let play = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Rect(rect)
                                if (rect.rect.width() - 124.0).abs() < 0.1
                                    && (rect.rect.height() - 30.0).abs() < 0.1 =>
                            {
                                Some(rect.rect)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let specs = text_rect(&output, &edition.features_line()).unwrap();
                    assert!(
                        (image.width() / image.height() - aspect.0 as f32 / aspect.1 as f32).abs()
                            < 0.03
                    );
                    assert!(image.height() <= f32::from(preferred) + 0.6);
                    if width >= 1000.0 {
                        assert!((image.height() - f32::from(preferred)).abs() < 0.6);
                    }
                    assert!(
                        (play.top() - (image.bottom() - play.height()).max(specs.bottom() + 12.0))
                            .abs()
                            < 1.1,
                        "{density} {width} {preferred} {aspect:?}: {image:?} {play:?} {specs:?}"
                    );
                    assert!(play.top() - specs.bottom() >= 11.0);
                    if (play.bottom() - image.bottom()).abs() < 1.1 {
                        aligned += 1;
                    } else {
                        released += 1;
                    }
                    pair.push((
                        image.height(),
                        text_rect(&output, "Launch options").unwrap().top(),
                    ));
                }
                assert!(
                    (pair[0].0 - pair[1].0).abs() < 0.6,
                    "equal height: {pair:?}"
                );
                // At the default height in a spacious window, the options stay
                // fixed while the text column moves to follow each image width.
                if width >= 1000.0 && preferred >= 180 {
                    assert!(
                        (pair[0].1 - pair[1].1).abs() < 0.6,
                        "stable options: {pair:?}"
                    );
                }
            }
        }
    }
    assert!(aligned > 0 && released > 0);
}
