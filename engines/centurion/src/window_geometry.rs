// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Follows REforge's shared SDL host: probe maximum client area, restore, then
// request an aspect-correct normal window in density-adjusted coordinates.
use crate::assets::Result;

pub fn window_units(scale: u32, density: f32) -> Result<(i32, i32)> {
    if scale == 0 || !density.is_finite() || density <= 0.0 {
        return Err("Invalid window scale/density".into());
    }
    let width = (320.0 * scale as f64 / density as f64).ceil();
    let height = (240.0 * scale as f64 / density as f64).ceil();
    if width > i32::MAX as f64 || height > i32::MAX as f64 {
        return Err("Window dimensions overflow".into());
    }
    Ok((width as i32, height as i32))
}

pub fn fitted_scale(pixels: (i32, i32), units: (i32, i32), density: f32) -> Result<u32> {
    let mut scale = (pixels.0.max(0) / 320).min(pixels.1.max(0) / 240).max(1) as u32;
    while scale > 1 {
        let requested = window_units(scale, density)?;
        if requested.0 <= units.0 && requested.1 <= units.1 {
            break;
        }
        scale -= 1;
    }
    Ok(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wider_client_area_becomes_a_compact_four_three_window() {
        assert_eq!(fitted_scale((1852, 1304), (1852, 1304), 1.0).unwrap(), 5);
        assert_eq!(window_units(5, 1.0).unwrap(), (1600, 1200));
    }

    #[test]
    fn fractional_density_and_rounding_fit_both_constraints() {
        assert_eq!(window_units(4, 1.5).unwrap(), (854, 640));
        assert_eq!(fitted_scale((1280, 960), (853, 640), 1.5).unwrap(), 3);
        assert!(window_units(1, f32::NAN).is_err());
    }
}
