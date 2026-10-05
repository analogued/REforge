// Copyright (c) 2026 analogued. Project license has not been selected.
//! No-input HUD updates through the bounded first-wave opening preview.

use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

use super::Random;

#[derive(Clone, Copy, Debug)]
pub struct HudSnapshot {
    pub fuel: u16,
    pub left_bar_width: usize,
    pub score: u32,
    pub pending_score: u16,
    pub marker_requested: u8,
    pub marker_previous: u8,
    pub marker_resolved: usize,
    pub marker_width: usize,
    pub marker_height: usize,
}

#[derive(Debug)]
pub(crate) struct Hud {
    state: HudSnapshot,
    marker_dimensions: [(usize, usize); 3],
}

impl Hud {
    pub fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        let mut marker_dimensions = [(0, 0); 3];
        for (choice, dimensions) in marker_dimensions.iter_mut().enumerate() {
            let index = 187 + choice;
            let sprite = assets
                .sprites
                .get(index)
                .ok_or_else(|| Error::InvalidData(format!("missing HUD sprite {index}")))?;
            *dimensions = (sprite.width_bytes, sprite.height);
        }
        Ok(Self {
            state: HudSnapshot {
                // The pre-loop HUD update consumes fuel once, without choosing a marker.
                fuel: 0x0dff - 5,
                left_bar_width: 13,
                score: 0,
                pending_score: 0,
                marker_requested: 0,
                marker_previous: 0,
                marker_resolved: 0,
                marker_width: 2,
                marker_height: 2,
            },
            marker_dimensions,
        })
    }

    /// Called once per completed body, before the event parser and terrain recycling.
    pub fn update(&mut self, loop_index: u16, random: &mut Random) {
        let resolve_marker = loop_index % 4 == 1;
        if resolve_marker {
            // Source resolution precedes the choice update, so the newly requested
            // marker can be drawn with the previous source and dimensions.
            self.state.marker_resolved = usize::from(self.state.marker_requested);
            if self.state.marker_requested >= 187 {
                let dimensions =
                    self.marker_dimensions[usize::from(self.state.marker_requested - 187)];
                self.state.marker_width = dimensions.0;
                self.state.marker_height = dimensions.1;
            }
        }
        if loop_index % 2 == 0 {
            self.add_points(1);
            let applied = self.state.pending_score.min(500);
            self.state.pending_score -= applied;
            self.state.score += u32::from(applied);
        }
        if loop_index % 2 == 0 || resolve_marker {
            while self.state.marker_requested == self.state.marker_previous {
                let choice = random.next_word() & 3;
                if choice != 3 {
                    self.state.marker_requested = 187 + choice as u8;
                }
            }
            // The bar uses the fuel value before this call consumes its five units.
            self.state.left_bar_width = usize::from(self.state.fuel >> 8);
            self.state.fuel -= 5;
        }
        if resolve_marker {
            // Incremental drawing remembers the request, even when its source was
            // resolved before that request changed.
            self.state.marker_previous = self.state.marker_requested;
        }
    }

    pub fn snapshot(&self) -> HudSnapshot {
        self.state
    }

    pub fn add_points(&mut self, points: u16) {
        self.state.pending_score = self.state.pending_score.saturating_add(points).min(32_000);
    }

    pub fn reset_fuel(&mut self) {
        self.state.fuel = 3583;
    }

    /// All HUD layers needed when composing a fresh background at a loop boundary.
    pub fn draws(&self) -> Vec<Draw> {
        // The source stride follows the current object width, as in the original
        // blitter. The left bar first shrinks to twelve bytes at body 69.
        let mut draws = vec![
            draw(24, 8, 38, self.state.left_bar_width, 3),
            draw(24, 58, 38, 14, 3),
        ];
        let mut divisor = 100_000;
        for digit in 0..6 {
            let value = (self.state.score / divisor) % 10;
            draws.push(draw(70 + value as usize, 34 + digit * 2, 33, 2, 7));
            divisor /= 10;
        }
        // Both indicators are first drawn with their initial sources. Later
        // requests resolve to a transparent sprite and preserve this artwork.
        draws.push(draw(85, 28, 13, 6, 7));
        draws.push(draw(86, 47, 13, 6, 7));
        for x in [63, 2, 68, 7, 73, 12] {
            draws.push(draw(28, x, 180, 6, 7));
        }
        draws.push(draw(
            self.state.marker_resolved,
            38,
            4,
            self.state.marker_width,
            self.state.marker_height,
        ));
        draws
    }
}

const fn draw(sprite: usize, x: i32, y: i32, width: usize, height: usize) -> Draw {
    Draw {
        resolved_sprite: sprite,
        rect: Rect {
            x_bytes: x,
            y,
            width_bytes: width,
            height,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Sprite;

    fn assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![],
            alternate: vec![],
            sprites: (0..190)
                .map(|index| Sprite {
                    width_bytes: 6,
                    height: if index == 188 { 24 } else { 23 },
                    descriptor: [0; 7],
                    color: vec![],
                    mask: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn source_resolution_precedes_selection_and_draw_remembers_the_request() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        random.next_word();
        random.next_word();
        hud.update(1, &mut random);
        assert_eq!(hud.state.marker_requested, 187);
        assert_eq!(hud.state.marker_previous, 187);
        assert_eq!(hud.state.marker_resolved, 0);
        // No terrain is present in this synthetic sequence. The next even update
        // chooses a distinct marker but does not resolve its source or draw it.
        hud.update(2, &mut random);
        assert_ne!(hud.state.marker_requested, 187);
        assert_eq!(hud.state.marker_previous, 187);
        assert_eq!(hud.state.marker_resolved, 0);
        let requested = hud.state.marker_requested;
        hud.update(3, &mut random);
        hud.update(4, &mut random);
        hud.update(5, &mut random);
        assert_eq!(hud.state.marker_resolved, usize::from(requested));
        assert_eq!(hud.state.marker_previous, requested);
        assert_eq!(hud.state.marker_width, 6);
        assert_eq!(hud.state.fuel, 3558);
        assert_eq!(hud.state.score, 2);
    }

    #[test]
    fn initial_layers_preserve_indicator_sources_and_bar_stride() {
        let hud = Hud::new(&assets()).unwrap();
        let draws = hud.draws();
        assert_eq!(draws.len(), 17);
        assert_eq!(draws[0].rect.width_bytes, 13);
        assert_eq!(draws[1].rect.width_bytes, 14);
        assert_eq!(draws[8].resolved_sprite, 85);
        assert_eq!(draws[9].resolved_sprite, 86);
        assert_eq!(draws[16].resolved_sprite, 0);
        assert_eq!(draws[16].rect.height, 2);
    }

    #[test]
    fn fuel_bar_uses_the_value_before_consumption_at_the_width_boundary() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        for body in 1..=68 {
            hud.update(body, &mut random);
        }
        assert_eq!(hud.state.fuel, 3323);
        assert_eq!(hud.state.left_bar_width, 13);
        assert_eq!(hud.draws()[0].rect.width_bytes, 13);
        hud.update(69, &mut random);
        assert_eq!(hud.state.fuel, 3318);
        assert_eq!(hud.state.left_bar_width, 12);
        assert_eq!(hud.draws()[0].rect.width_bytes, 12);
    }

    #[test]
    fn missing_marker_resource_returns_an_error() {
        let mut assets = assets();
        assets.sprites.truncate(188);
        assert!(Hud::new(&assets).is_err());
    }

    #[test]
    fn collision_points_are_paid_in_bounded_even_body_updates() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        hud.add_points(1000);
        hud.update(1, &mut random);
        assert_eq!(hud.state.score, 0);
        hud.update(2, &mut random);
        assert_eq!((hud.state.score, hud.state.pending_score), (500, 501));
        hud.update(4, &mut random);
        assert_eq!((hud.state.score, hud.state.pending_score), (1000, 2));
        hud.update(6, &mut random);
        assert_eq!((hud.state.score, hud.state.pending_score), (1003, 0));
        hud.add_points(40_000);
        assert_eq!(hud.state.pending_score, 32_000);
    }
}
