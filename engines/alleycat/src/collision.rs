// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Shared CS:2e29 rectangle test. Keep native byte/word widths, inclusive
//! endpoints, wrapping upper sums and borrow-clamped lower endpoints.

pub(crate) fn overlaps(first: (u16, u8, u16, u8), second: (u16, u8, u16, u8)) -> bool {
    let (x, y, width, height) = first;
    let (other_x, other_y, other_width, other_height) = second;
    x.wrapping_add(width) >= other_x
        && x.saturating_sub(other_width) <= other_x
        && y.wrapping_add(height) >= other_y
        && y.saturating_sub(other_height) <= other_y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_includes_exact_edges_and_clamps_borrow_at_the_origin() {
        let cat = (10, 10, 24, 14);
        assert!(overlaps(cat, (34, 24, 4, 4)));
        assert!(!overlaps(cat, (35, 24, 4, 4)));
        assert!(!overlaps(cat, (34, 25, 4, 4)));
        assert!(overlaps((0, 0, 4, 4), (0, 0, 24, 14)));
    }

    #[test]
    fn word_and_byte_upper_edges_wrap_at_the_native_width() {
        assert!(!overlaps((65_530, 10, 10, 10), (65_530, 10, 10, 10)));
        assert!(!overlaps((10, 250, 10, 10), (10, 250, 10, 10)));
        assert!(overlaps((10, 240, 10, 10), (10, 240, 10, 10)));
    }
}
