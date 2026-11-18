// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn grid_height(grid: &GalleryGrid) -> f32 {
    gallery_height_for(grid.card_width, std::iter::repeat_n(FOUR_THREE, grid.rows))
}

#[test]
fn gallery_votes_use_exact_aspects_and_include_partial_rows() {
    let wide = (16, 9);
    let ultra = (21, 9);
    let entries = [wide, wide, FOUR_THREE, ultra, ultra, FOUR_THREE, ultra];
    for (policy, expected) in [
        (GalleryPreviewAspect::FourThree, [FOUR_THREE; 3]),
        (GalleryPreviewAspect::PerRow, [wide, ultra, ultra]),
        (GalleryPreviewAspect::WholeGallery, [ultra; 3]),
    ] {
        assert_eq!(row_aspects(entries.into_iter(), 3, policy), expected);
    }
    assert_eq!(
        row_aspects(
            [wide, FOUR_THREE].into_iter(),
            2,
            GalleryPreviewAspect::PerRow
        ),
        [FOUR_THREE]
    );
    assert_eq!(
        row_aspects(
            [wide, ultra].into_iter(),
            2,
            GalleryPreviewAspect::WholeGallery
        ),
        [FOUR_THREE]
    );
}

#[test]
fn rounded_height_does_not_discard_a_larger_four_column_layout() {
    let available = Vec2::new(1126.6666, 610.8);
    let grid = GalleryGrid::fit_entire_library(11, available, 7).unwrap();
    assert_eq!((grid.columns, grid.rows, grid.card_width), (4, 3, 191.0));
    assert!(grid_height(&grid) <= available.y);
    // The next pixel is too tall until the exact height boundary. The
    // search then finds both widths on the rounded height plateau.
    assert!(3.0 * card_height(192.0) + 2.0 * GALLERY_GAP > available.y);
    // Searching actual heights also finds the last width on a rounding plateau.
    assert_eq!(
        GalleryGrid::fit_entire_library(11, Vec2::new(1126.6666, 611.0), 7)
            .unwrap()
            .card_width,
        193.0
    );
}

#[test]
fn roomier_grid_matches_mockup_at_reference_viewport_sizes() {
    // The mockup allows 36 points for side margins and 119 for vertical chrome.
    for (width, height, count, expected) in [
        (630.0, 520.0, 6, (3, 2, 187.0)),
        (657.0, 560.0, 12, (3, 4, 196.0)),
        (831.0, 560.0, 12, (4, 3, 186.0)),
        (1300.0, 560.0, 12, (5, 3, 218.0)),
        (1318.0, 560.0, 12, (5, 3, 218.0)),
        (1319.0, 560.0, 12, (6, 2, 200.0)),
        (1300.0, 720.0, 12, (4, 3, 292.0)),
        (1707.0, 901.0, 12, (4, 3, 269.0)),
    ] {
        let grid = GalleryGrid::fit(count, Vec2::new(width - 36.0, height - 119.0));
        assert_eq!(
            (grid.columns, grid.rows, grid.card_width),
            expected,
            "{width} × {height} with {count} games"
        );
    }
}

#[test]
fn very_short_or_narrow_viewports_keep_cards_within_width() {
    let grid = GalleryGrid::fit(11, Vec2::new(1450.0, 100.0));
    assert_eq!((grid.columns, grid.rows, grid.card_width), (9, 2, 144.0));
    let grid = GalleryGrid::fit(11, Vec2::new(400.0, 100.0));
    assert_eq!((grid.columns, grid.rows, grid.card_width), (2, 6, 144.0));
    for width in [1.0, 100.0, 143.5] {
        let grid = GalleryGrid::fit(11, Vec2::new(width, 0.0));
        assert_eq!((grid.columns, grid.rows, grid.card_width), (1, 11, width));
    }
}

#[test]
fn full_library_fit_matches_an_exhaustive_search_of_actual_card_sizes() {
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
                let max_columns =
                    (((width + GALLERY_GAP) / (MIN_CARD_WIDTH + GALLERY_GAP)) as usize).min(count);
                let actual = GalleryGrid::fit_entire_library(count, available, max_columns);
                assert_eq!(
                    actual.map(|grid| (grid.columns, grid.card_width)),
                    expected.map(|(_, columns, card_width)| (columns, card_width as f32)),
                    "count {count}, available {available:?}"
                );
            }
        }
    }
}
