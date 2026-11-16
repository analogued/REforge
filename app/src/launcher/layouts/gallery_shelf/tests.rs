// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn rounded_height_does_not_discard_a_larger_four_column_layout() {
    let available = Vec2::new(1126.6666, 610.8);
    let grid = GalleryGrid::fit(11, available);
    assert_eq!((grid.columns, grid.rows, grid.card_width), (4, 3, 218.0));
    assert!(grid.size().y <= available.y);
    // The old inverse estimated 219 points, rejected it after rounding, then
    // selected five columns of 212-point cards. One point smaller fits.
    assert!(3.0 * card_height(219.0) + 2.0 * GALLERY_GAP > available.y);
    // Searching actual heights also finds the last width on a rounding plateau.
    assert_eq!(
        GalleryGrid::fit(11, Vec2::new(1126.6666, 611.0)).card_width,
        220.0
    );
}

#[test]
fn one_pixel_less_height_starts_scrolling_without_enlarging_or_reflowing_cards() {
    for density in [1.0, 1.5, 2.0, 2.25] {
        let available = Vec2::new(588.4444, 624.0);
        let before = GalleryGrid::fit(11, available);
        let after = GalleryGrid::fit(11, available - Vec2::new(0.0, 1.0 / density));
        for grid in [&before, &after] {
            assert_eq!((grid.columns, grid.rows, grid.card_width), (3, 4, 144.0));
        }
        assert!(before.size().y <= available.y);
        assert!(after.size().y > available.y - 1.0 / density);
    }
}

#[test]
fn scrolling_uses_readable_cards_and_avoids_empty_grid_positions() {
    let grid = GalleryGrid::fit(11, Vec2::new(1450.0, 100.0));
    assert_eq!((grid.columns, grid.rows, grid.card_width), (6, 2, 144.0));
    let grid = GalleryGrid::fit(11, Vec2::new(400.0, 100.0));
    assert_eq!((grid.columns, grid.rows, grid.card_width), (2, 6, 144.0));
    for width in [1.0, 100.0, 143.5] {
        let grid = GalleryGrid::fit(11, Vec2::new(width, 0.0));
        assert_eq!((grid.columns, grid.rows, grid.card_width), (1, 11, width));
    }
}

#[test]
fn fitting_matches_an_exhaustive_search_of_actual_card_sizes() {
    // An independent, exhaustive oracle checks the binary search and rounding
    // boundaries against every integer card width and possible column count.
    for count in [1usize, 6, 11, 20] {
        for width in [588.4444, 1126.6666, 1600.0] {
            for height in [303.9, 610.8, 611.0, 623.5555, 624.0, 720.0] {
                let available = Vec2::new(width, height);
                let mut expected = None;
                for columns in 1..=count {
                    let rows = count.div_ceil(columns);
                    for card_width in MIN_CARD_WIDTH as usize..=MAX_CARD_WIDTH as usize {
                        let size = Vec2::new(
                            columns as f32 * card_width as f32 + (columns - 1) as f32 * GALLERY_GAP,
                            rows as f32 * card_height(card_width as f32)
                                + (rows - 1) as f32 * GALLERY_GAP,
                        );
                        if size.x > width || size.y > height {
                            continue;
                        }
                        // Larger cards, then fewer empty positions, then fewer rows.
                        let score = (
                            MAX_CARD_WIDTH as usize - card_width,
                            columns * rows - count,
                            rows,
                        );
                        if expected.as_ref().is_none_or(|(old, _, _)| score < *old) {
                            expected = Some((score, columns, card_width));
                        }
                    }
                }
                let actual = GalleryGrid::fit(count, available);
                if let Some((_, columns, card_width)) = expected {
                    assert_eq!(
                        (actual.columns, actual.card_width),
                        (columns, card_width as f32),
                        "count {count}, available {available:?}"
                    );
                } else {
                    assert_eq!(actual.card_width, MIN_CARD_WIDTH);
                    assert!(actual.size().x <= width);
                }
            }
        }
    }
}
