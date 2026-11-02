// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-provided pixel geometry; texture oversampling is not native resolution.

use reforge_settings::{AspectRatio, RuntimeSettings, ScalingPolicy};

/// Native pixel grid and aspect-corrected base display size, supplied by an engine.
/// The latter also defines the units used by the window-scale preference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayGeometry {
    source: (u32, u32),
    original: (u32, u32),
}

impl DisplayGeometry {
    pub const fn new(source: (u32, u32), original: (u32, u32)) -> Self {
        assert!(source.0 > 0 && source.1 > 0 && original.0 > 0 && original.1 > 0);
        Self { source, original }
    }

    pub fn display_size(self, preference: AspectRatio) -> (u32, u32) {
        match preference {
            AspectRatio::Original => self.original,
            AspectRatio::Square => self.source,
        }
    }

    pub(crate) fn presentation_size(self, settings: &RuntimeSettings) -> (u32, u32) {
        self.display_size(if settings.scaling_policy == ScalingPolicy::Stretch {
            AspectRatio::Original
        } else {
            settings.aspect_ratio
        })
    }

    pub fn ratio(self, preference: AspectRatio) -> (u32, u32) {
        let (width, height) = self.display_size(preference);
        let (mut a, mut b) = (width, height);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        (width / a, height / a)
    }

    pub(crate) fn label(self, preference: AspectRatio, game_scope: bool) -> String {
        let name = match preference {
            AspectRatio::Original => "Original display",
            AspectRatio::Square => "Square pixels",
        };
        if game_scope {
            let (width, height) = self.ratio(preference);
            format!("{name} ({width}:{height})")
        } else {
            name.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_come_from_engine_geometry_including_non_four_three_displays() {
        let cga = DisplayGeometry::new((320, 200), (320, 240));
        assert_eq!(cga.label(AspectRatio::Original, false), "Original display");
        assert_eq!(cga.label(AspectRatio::Square, false), "Square pixels");
        assert_eq!(
            cga.label(AspectRatio::Original, true),
            "Original display (4:3)"
        );
        assert_eq!(cga.label(AspectRatio::Square, true), "Square pixels (8:5)");
        let wide = DisplayGeometry::new((320, 200), (320, 180));
        assert_eq!(wide.ratio(AspectRatio::Original), (16, 9));
        assert_eq!(wide.display_size(AspectRatio::Original), (320, 180));
        assert_eq!(wide.display_size(AspectRatio::Square), (320, 200));
        let mono = DisplayGeometry::new((640, 200), (320, 240));
        assert_eq!(mono.ratio(AspectRatio::Square), (16, 5));
        assert_eq!(mono.display_size(AspectRatio::Original), (320, 240));
    }

    #[test]
    fn stretch_ignores_but_retains_the_square_preference_across_video_modes() {
        let mut settings = RuntimeSettings {
            aspect_ratio: AspectRatio::Square,
            scaling_policy: ScalingPolicy::Stretch,
            ..Default::default()
        };
        for source in [(320, 200), (640, 200)] {
            let geometry = DisplayGeometry::new(source, (320, 240));
            assert_eq!(geometry.presentation_size(&settings), (320, 240));
            settings.scaling_policy = ScalingPolicy::Fit;
            assert_eq!(geometry.presentation_size(&settings), source);
            settings.scaling_policy = ScalingPolicy::Stretch;
        }
        assert_eq!(settings.aspect_ratio, AspectRatio::Square);
    }
}
