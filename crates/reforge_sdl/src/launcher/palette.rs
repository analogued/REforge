// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native palette metadata and presentation-only swatch reduction.

use std::collections::BTreeMap;

/// Bound palette metadata independently of the preview pixel allocation.
pub const MAX_PALETTE_COLOURS: usize = 4096;
const MOSAIC_ROWS: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaletteHint {
    /// Keep small hardware palettes in order; arrange larger palettes as mosaics.
    #[default]
    Auto,
    PreserveOrder,
    Mosaic,
}

/// Exact native RGB colours, optionally paired with source-pixel frequencies.
/// Zero weights are allowed for palette entries unused by a particular frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub colours: Vec<[u8; 3]>,
    pub weights: Option<Vec<u64>>,
    pub hint: PaletteHint,
}

impl Palette {
    pub fn validate(&self) -> Result<(), String> {
        if self.colours.len() > MAX_PALETTE_COLOURS {
            return Err(format!("Palette exceeds {MAX_PALETTE_COLOURS} colours"));
        }
        if self
            .weights
            .as_ref()
            .is_some_and(|weights| weights.len() != self.colours.len())
        {
            return Err("Palette weights must match its colours".into());
        }
        Ok(())
    }
}

/// Colours in row-major display order. Empty palettes have no rows or columns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteDisplay {
    pub colours: Vec<[u8; 3]>,
    pub rows: usize,
    pub columns: usize,
    pub mosaic: bool,
}

/// Prepare a separate display copy, retaining exact source RGB values.
///
/// Mosaics discard at most five redundant distinct colours to fill six rows.
/// Small ordered palettes retain duplicates and their supplied order. Callers
/// loading metadata should validate it first; mismatched optional weights here
/// fall back to equal weights so a swatch display remains available.
pub fn prepare_palette(
    colours: &[[u8; 3]],
    weights: Option<&[u64]>,
    hint: PaletteHint,
) -> PaletteDisplay {
    if hint == PaletteHint::PreserveOrder || (hint == PaletteHint::Auto && colours.len() <= 16) {
        return swatches(colours.to_vec());
    }
    let weights = weights.filter(|weights| weights.len() == colours.len());
    let mut histogram = BTreeMap::<[u8; 3], u64>::new();
    for (index, &rgb) in colours.iter().enumerate() {
        let weight = weights.map_or(1, |weights| weights[index]);
        let total = histogram.entry(rgb).or_default();
        *total = total.saturating_add(weight);
    }
    if histogram.len() < MOSAIC_ROWS {
        return swatches(histogram.into_keys().collect());
    }
    // RGB order makes both pair selection and frequency ties independent of
    // source palette order. Positions are computed once and never averaged.
    let mut samples: Vec<_> = histogram
        .into_iter()
        .map(|(rgb, weight)| Sample {
            rgb,
            weight,
            lab: oklab(rgb),
        })
        .collect();
    let capacity = samples.len() / MOSAIC_ROWS * MOSAIC_ROWS;
    while samples.len() > capacity {
        let mut closest = (f64::INFINITY, 0, 1);
        for (a, first) in samples.iter().enumerate() {
            for (b, second) in samples.iter().enumerate().skip(a + 1) {
                let dl = first.lab[0] - second.lab[0];
                let da = first.lab[1] - second.lab[1];
                let db = first.lab[2] - second.lab[2];
                let distance = dl * dl + da * da + db * db;
                // Equal distances keep the first lexicographic RGB pair.
                if distance < closest.0 {
                    closest = (distance, a, b);
                }
            }
        }
        let (_, a, b) = closest;
        let removed = if (samples[a].weight, samples[a].rgb) <= (samples[b].weight, samples[b].rgb)
        {
            a
        } else {
            b
        };
        samples.remove(removed);
    }
    samples.sort_by(|a, b| {
        hue(a.rgb)
            .total_cmp(&hue(b.rgb))
            .then_with(|| a.lab[0].total_cmp(&b.lab[0]))
            .then_with(|| a.rgb.cmp(&b.rgb))
    });
    PaletteDisplay {
        columns: samples.len() / MOSAIC_ROWS,
        rows: MOSAIC_ROWS,
        mosaic: true,
        colours: samples.into_iter().map(|sample| sample.rgb).collect(),
    }
}

fn swatches(colours: Vec<[u8; 3]>) -> PaletteDisplay {
    PaletteDisplay {
        rows: usize::from(!colours.is_empty()),
        columns: colours.len(),
        colours,
        mosaic: false,
    }
}

struct Sample {
    rgb: [u8; 3],
    weight: u64,
    lab: [f64; 3],
}

fn hue(rgb: [u8; 3]) -> f64 {
    let [r, g, b] = rgb.map(f64::from);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    }
}

// The standard sRGB-to-OKLab transform, also used by the project's palette
// prototype. Distance is squared Euclidean OKLab; squaring preserves ordering.
fn oklab(rgb: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|channel| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_palette(size: usize) -> Vec<[u8; 3]> {
        (0..size)
            .map(|i| [i as u8, (i * 17) as u8, (i * 43) as u8])
            .collect()
    }

    #[test]
    fn small_hardware_palettes_keep_their_order() {
        let cga = [[0, 0, 0], [255, 85, 255], [85, 255, 255], [255, 255, 255]];
        let mono = [[0, 0, 0], [255, 255, 255]];
        for colours in [&cga[..], &mono[..]] {
            let display = prepare_palette(colours, None, PaletteHint::Auto);
            assert_eq!(display.colours, colours);
            assert_eq!(
                (display.rows, display.columns, display.mosaic),
                (1, colours.len(), false)
            );
        }
    }

    #[test]
    fn hint_overrides_size_threshold() {
        let colours = synthetic_palette(16);
        assert!(!prepare_palette(&colours, None, PaletteHint::Auto).mosaic);
        let display = prepare_palette(&colours, None, PaletteHint::Mosaic);
        assert_eq!(
            (display.rows, display.columns, display.mosaic),
            (6, 2, true)
        );
        let colours = synthetic_palette(256);
        let display = prepare_palette(&colours, None, PaletteHint::PreserveOrder);
        assert_eq!(display.colours, colours);
        assert!(!display.mosaic);
    }

    #[test]
    fn mosaics_fill_six_rows_without_inventing_colours() {
        for (size, expected) in [(256, 252), (235, 234), (18, 18)] {
            let colours = synthetic_palette(size);
            let before = colours.clone();
            let display = prepare_palette(&colours, None, PaletteHint::Auto);
            assert_eq!(display.colours.len(), expected);
            assert_eq!(display.rows, 6);
            assert_eq!(display.rows * display.columns, expected);
            assert!(display.mosaic);
            assert!(display.colours.iter().all(|rgb| colours.contains(rgb)));
            assert_eq!(colours, before);
        }
    }

    #[test]
    fn empty_and_small_mosaics_never_discard_all_colours() {
        for size in 0..=7 {
            let colours = synthetic_palette(size);
            let display = prepare_palette(&colours, None, PaletteHint::Mosaic);
            assert_eq!(display.colours.len(), if size == 7 { 6 } else { size });
            assert_eq!(display.mosaic, size >= 6);
            assert_eq!(display.rows * display.columns, display.colours.len());
        }
        let repeated = [[12, 34, 56]; 20];
        let display = prepare_palette(&repeated, None, PaletteHint::Auto);
        assert_eq!(display.colours, [[12, 34, 56]]);
        assert!(!display.mosaic);
    }

    #[test]
    fn duplicate_weights_stay_with_their_colours_and_saturate() {
        let mut colours = vec![
            [100, 100, 100],
            [101, 100, 100],
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 255, 0],
            [0, 255, 255],
            [100, 100, 100],
        ];
        let mut weights = vec![u64::MAX, 100, 1, 1, 1, 1, 1, u64::MAX];
        let display = prepare_palette(&colours, Some(&weights), PaletteHint::Mosaic);
        assert!(display.colours.contains(&[100, 100, 100]));
        assert!(!display.colours.contains(&[101, 100, 100]));
        // Neither repeated entry alone outweighs the adjacent shade.
        assert_eq!(
            display,
            prepare_palette(
                &colours,
                Some(&[60, 100, 1, 1, 1, 1, 1, 60]),
                PaletteHint::Mosaic
            )
        );
        colours.reverse();
        weights.reverse();
        assert_eq!(
            display,
            prepare_palette(&colours, Some(&weights), PaletteHint::Mosaic)
        );
        // Exact frequency ties discard the lexicographically smaller endpoint.
        let tied = prepare_palette(&colours[..7], None, PaletteHint::Mosaic);
        assert!(!tied.colours.contains(&[100, 100, 100]));
        assert!(tied.colours.contains(&[101, 100, 100]));
    }

    #[test]
    fn rare_distinctive_colours_survive_redundant_common_shades() {
        let colours = [
            [100, 100, 100],
            [101, 100, 100],
            [50, 50, 50],
            [200, 200, 200],
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
        ];
        let weights = [1_000, 999, 1_000, 1_000, 1, 1, 1];
        let display = prepare_palette(&colours, Some(&weights), PaletteHint::Mosaic);
        assert!(!display.colours.contains(&[101, 100, 100]));
        for rgb in [[255, 0, 0], [0, 255, 0], [0, 0, 255]] {
            assert!(display.colours.contains(&rgb));
        }
    }

    #[test]
    fn mosaic_order_is_hue_then_perceptual_lightness() {
        let colours = [
            [255, 0, 255],
            [0, 0, 255],
            [255, 0, 0],
            [0, 255, 0],
            [128, 0, 0],
            [255, 255, 0],
        ];
        let weights = [3, 9, 1, 7, 4, 2];
        let original_colours = colours;
        let original_weights = weights;
        let display = prepare_palette(&colours, Some(&weights), PaletteHint::Mosaic);
        assert_eq!(
            display.colours,
            [
                [128, 0, 0],
                [255, 0, 0],
                [255, 255, 0],
                [0, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
            ]
        );
        assert_eq!(colours, original_colours);
        assert_eq!(weights, original_weights);
    }

    #[test]
    fn metadata_bounds_and_weight_validation_are_explicit() {
        let mut palette = Palette {
            colours: vec![],
            weights: Some(vec![]),
            hint: PaletteHint::default(),
        };
        assert!(palette.validate().is_ok());
        palette.colours = vec![[0; 3]; MAX_PALETTE_COLOURS];
        palette.weights = None;
        assert!(palette.validate().is_ok());
        palette.colours.push([255; 3]);
        assert!(palette.validate().is_err());
        palette.colours = synthetic_palette(7);
        palette.weights = Some(vec![1]);
        assert!(palette.validate().is_err());
        assert_eq!(
            prepare_palette(
                &palette.colours,
                palette.weights.as_deref(),
                PaletteHint::Mosaic
            ),
            prepare_palette(&palette.colours, None, PaletteHint::Mosaic)
        );
    }
}
