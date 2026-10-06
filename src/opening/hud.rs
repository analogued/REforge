// Copyright (c) 2026 analogued. Project license has not been selected.
//! HUD fuel, weapon heat, score, and delayed source resolution.

use crate::{Error, assets::GraphicsAssets, frame::Rect, scene::Draw};

use super::Random;

#[derive(Clone, Copy, Debug)]
pub struct HudSnapshot {
    pub fuel: u16,
    pub left_bar_width: usize,
    pub left_bar_draw_width: usize,
    pub left_bar_sprite: usize,
    pub left_bar_height: usize,
    pub left_bar_draw_sprite: usize,
    pub left_bar_draw_height: usize,
    pub right_bar_x: i32,
    pub right_bar_width: usize,
    pub right_bar_draw_x: i32,
    pub right_bar_draw_width: usize,
    pub marker_x: i32,
    pub marker_y: i32,
    pub score: u32,
    pub pending_score: u16,
    pub marker_requested: u8,
    pub marker_previous: u8,
    pub marker_resolved: usize,
    pub marker_width: usize,
    pub marker_height: usize,
    pub lives_requested: u8,
    pub lives_resolved: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HudEffects {
    pub weapon_counter: Option<i16>,
    pub damage: Option<u16>,
}

#[derive(Debug)]
pub(crate) struct Hud {
    state: HudSnapshot,
    dimensions: Vec<(usize, usize)>,
    marker_styles: [(u8, i32, i32); 3],
    progression: u16,
    score_multiplier: u16,
    score_digits: [i16; 6],
    marker_draw_x: i32,
    marker_draw_y: i32,
    indicator_source: usize,
    indicator_changed: bool,
}

impl Hud {
    pub fn new(assets: &GraphicsAssets) -> Result<Self, Error> {
        if assets.sprites.len() < 190 {
            return Err(Error::InvalidData("missing HUD marker resources".into()));
        }
        let dimensions = assets
            .sprites
            .iter()
            .map(|sprite| (sprite.width_bytes, sprite.height))
            .collect();
        Ok(Self {
            state: HudSnapshot {
                // The pre-loop HUD update consumes fuel once, without choosing a marker.
                fuel: 0x0dff - 5,
                left_bar_width: 13,
                left_bar_draw_width: 13,
                left_bar_sprite: 24,
                left_bar_height: 3,
                left_bar_draw_sprite: 24,
                left_bar_draw_height: 3,
                right_bar_x: 58,
                right_bar_width: 14,
                right_bar_draw_x: 58,
                right_bar_draw_width: 14,
                marker_x: 38,
                marker_y: 4,
                score: 0,
                pending_score: 0,
                marker_requested: 0,
                marker_previous: 0,
                marker_resolved: 0,
                marker_width: 2,
                marker_height: 2,
                lives_requested: 6,
                lives_resolved: 6,
            },
            dimensions,
            marker_styles: [(187, 38, 4), (188, 38, 4), (189, 38, 4)],
            progression: 1,
            score_multiplier: 1,
            score_digits: [0; 6],
            marker_draw_x: 38,
            marker_draw_y: 4,
            indicator_source: 87,
            indicator_changed: false,
        })
    }

    /// Select the indicators after the first player update in a completed body.
    pub(crate) fn set_orientation(&mut self, orientation: i16) {
        let source = if (1..=5).contains(&orientation) {
            (90 - orientation) as usize
        } else {
            87
        };
        self.indicator_changed = source != self.indicator_source;
        self.indicator_source = source;
    }

    pub(crate) fn set_progression(&mut self, progression: u16) {
        self.progression = progression;
    }

    pub(crate) fn set_score_multiplier(&mut self, multiplier: u16) {
        self.score_multiplier = multiplier;
    }

    /// Styles are decoded from the selected difficulty's external resource words.
    pub(crate) fn set_marker_styles(
        &mut self,
        assets: &GraphicsAssets,
        styles: [(usize, i16, i16); 3],
    ) -> Result<(), Error> {
        for (destination, (source, x, y)) in self.marker_styles.iter_mut().zip(styles) {
            if source > 255 || assets.sprites.get(source).is_none() {
                return Err(Error::InvalidData(format!(
                    "missing HUD marker sprite {source}"
                )));
            }
            *destination = (source as u8, i32::from(x), i32::from(y));
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn update(&mut self, loop_index: u16, random: &mut Random) {
        self.update_with_weapon(loop_index, random, 4).unwrap();
    }

    /// Runs after the first player update; counter and damage feed back before
    /// the remaining player polls. Fuel redraws every fourth body; heat redraws
    /// on even bodies, retaining its prior rectangle during phase-one updates.
    pub(crate) fn update_with_weapon(
        &mut self,
        loop_index: u16,
        random: &mut Random,
        weapon_counter: i16,
    ) -> Result<HudEffects, Error> {
        let mut effects = HudEffects::default();
        let resolve_marker = loop_index % 4 == 1;
        if resolve_marker {
            self.state.marker_resolved = usize::from(self.state.marker_requested);
            let dimensions = self
                .dimensions
                .get(self.state.marker_resolved)
                .ok_or_else(|| Error::InvalidData("missing requested HUD marker".into()))?;
            self.state.marker_width = dimensions.0;
            self.state.marker_height = dimensions.1;
        }
        if loop_index % 2 == 0 {
            self.add_points(self.progression);
            // The original compares the score word signed and never borrows
            // between decimal digits when an overflowing award becomes negative.
            let applied = (self.state.pending_score as i16).min(500);
            self.state.pending_score = self.state.pending_score.wrapping_sub(applied as u16);
            let mut carry = applied;
            for digit in self.score_digits.iter_mut().rev() {
                if carry == 0 {
                    break;
                }
                *digit = digit.wrapping_add(carry);
                carry = 0;
                while *digit > 9 {
                    *digit -= 10;
                    carry += 1;
                }
            }
            self.state.score = self.score_digits.iter().fold(0u32, |score, &digit| {
                score.wrapping_mul(10).wrapping_add(digit as u32)
            });
        }
        if loop_index % 2 == 0 || resolve_marker {
            if self.state.marker_requested == self.state.marker_previous
                && self
                    .marker_styles
                    .iter()
                    .all(|style| style.0 == self.state.marker_previous)
            {
                return Err(Error::InvalidData(
                    "HUD marker selection has no distinct source".into(),
                ));
            }
            while self.state.marker_requested == self.state.marker_previous {
                let choice = random.next_word() & 3;
                if choice != 3 {
                    let (source, x, y) = self.marker_styles[usize::from(choice)];
                    self.state.marker_requested = source;
                    self.state.marker_x = x;
                    self.state.marker_y = y;
                }
            }
            if (self.state.fuel as i16) > 3583 {
                self.state.fuel = 3583;
            }
            self.state.left_bar_width = usize::from(((self.state.fuel as i16) >> 8) as u16);
            self.state.left_bar_height = 3;
            self.state.fuel = self.state.fuel.wrapping_sub(5);
            if (self.state.fuel as i16) < 256 {
                self.state.left_bar_sprite = 0;
                self.state.left_bar_width = 2;
                self.state.left_bar_height = 1;
                self.state.fuel = 3583;
                effects.damage = Some(2);
            } else {
                self.state.left_bar_sprite = 24;
            }
            let counter = weapon_counter.clamp(4, 223);
            effects.weapon_counter = Some(counter);
            let shift = counter >> 4;
            self.state.right_bar_x = 58 + i32::from(shift);
            self.state.right_bar_width = (14 - shift) as usize;
        }
        if loop_index % 2 == 0 {
            self.state.right_bar_draw_x = self.state.right_bar_x;
            self.state.right_bar_draw_width = self.state.right_bar_width;
        }
        if resolve_marker {
            self.state.left_bar_draw_width = self.state.left_bar_width;
            self.state.left_bar_draw_height = self.state.left_bar_height;
            self.state.left_bar_draw_sprite = self.state.left_bar_sprite;
            self.state.marker_previous = self.state.marker_requested;
            // Selection may move the requested marker on an even body, but
            // its pixels remain at the preceding position until this phase.
            self.marker_draw_x = self.state.marker_x;
            self.marker_draw_y = self.state.marker_y;
        }
        if loop_index % 8 == 3 {
            self.state.lives_resolved = self.state.lives_requested;
        }
        Ok(effects)
    }

    pub fn snapshot(&self) -> HudSnapshot {
        self.state
    }

    pub fn add_points(&mut self, points: u16) {
        self.state.pending_score = self
            .state
            .pending_score
            .wrapping_add(points.wrapping_mul(self.score_multiplier));
        if (self.state.pending_score as i16) > 32_000 {
            self.state.pending_score = 32_000;
        }
    }

    pub fn add_fuel(&mut self, fuel: u16) {
        self.state.fuel = self.state.fuel.wrapping_add(fuel);
    }

    pub fn reset_fuel(&mut self) {
        self.state.fuel = 3583;
    }

    pub fn set_lives(&mut self, lives: u8) {
        self.state.lives_requested = lives;
    }

    /// All HUD layers needed when composing a fresh background at a loop boundary.
    pub fn draws(&self) -> Vec<Draw> {
        // The source stride follows the current object width, as in the original
        // blitter. Drawing retains the previous width until the HUD render phase.
        let mut draws = vec![
            draw(
                self.state.left_bar_draw_sprite,
                8,
                38,
                self.state.left_bar_draw_width,
                self.state.left_bar_draw_height,
            ),
            draw(
                24,
                self.state.right_bar_draw_x,
                38,
                self.state.right_bar_draw_width,
                3,
            ),
        ];
        for (digit, &value) in self.score_digits.iter().enumerate() {
            let source = value.wrapping_add(70) as u8;
            draws.push(draw(usize::from(source), 34 + digit as i32 * 2, 33, 2, 7));
        }
        // Preserve the initial artwork and masked pixels between selection
        // changes; the caller retains each indicator's preceding rectangle.
        draws.push(draw(
            self.indicator_source,
            if self.indicator_changed { 28 } else { 128 },
            13,
            6,
            7,
        ));
        draws.push(draw(
            self.indicator_source,
            if self.indicator_changed { 47 } else { 128 },
            13,
            6,
            7,
        ));
        for x in [63, 2, 68, 7, 73, 12]
            .into_iter()
            .take(usize::from(self.state.lives_resolved))
        {
            draws.push(draw(28, x, 180, 6, 7));
        }
        draws.push(draw(
            self.state.marker_resolved,
            self.marker_draw_x,
            self.marker_draw_y,
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
        assert_eq!(draws[8].resolved_sprite, 87);
        assert_eq!(draws[9].resolved_sprite, 87);
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
        assert_eq!(hud.state.pending_score, 40_000);
    }

    #[test]
    fn life_loss_waits_for_the_bottom_hud_resolution_phase() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        hud.update(3, &mut random);
        // Death can occur after the resolution phase in the same body.
        hud.set_lives(5);
        hud.update(4, &mut random);
        assert_eq!(hud.snapshot().lives_requested, 5);
        assert_eq!(hud.snapshot().lives_resolved, 6);
        assert_eq!(hud.draws().len(), 17);
        hud.update(11, &mut random);
        assert_eq!(hud.snapshot().lives_resolved, 5);
        assert_eq!(hud.draws().len(), 16);
    }

    #[test]
    fn fuel_bar_waits_for_render_phase_when_width_shrinks_or_recovers() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        hud.state.fuel = 13 << 8;
        hud.update(2, &mut random);
        hud.update(4, &mut random);
        assert_eq!(hud.snapshot().left_bar_width, 12);
        assert_eq!(hud.draws()[0].rect.width_bytes, 13);
        hud.update(5, &mut random);
        assert_eq!(hud.draws()[0].rect.width_bytes, 12);
        hud.reset_fuel();
        hud.update(6, &mut random);
        assert_eq!(hud.snapshot().left_bar_width, 13);
        assert_eq!(hud.draws()[0].rect.width_bytes, 12);
        hud.update(9, &mut random);
        assert_eq!(hud.draws()[0].rect.width_bytes, 13);
    }

    #[test]
    fn collected_fuel_clamps_on_consumption_instead_of_collection() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        hud.add_fuel(512);
        hud.update(3, &mut random);
        assert_eq!(hud.snapshot().fuel, 4090);
        hud.update(4, &mut random);
        assert_eq!(hud.snapshot().fuel, 3578);
        assert_eq!(hud.snapshot().left_bar_width, 13);
    }
    #[test]
    fn weapon_counter_clamps_on_hud_updates_but_redraws_only_on_even_bodies() {
        for (counter, bounded) in [(-20, 4), (4, 4), (15, 15), (16, 16), (223, 223), (224, 223)] {
            let mut hud = Hud::new(&assets()).unwrap();
            let mut random = Random::default();
            let effects = hud.update_with_weapon(2, &mut random, counter).unwrap();
            assert_eq!(effects.weapon_counter, Some(bounded));
            assert_eq!(hud.state.right_bar_x, 58 + i32::from(bounded >> 4));
            assert_eq!(hud.state.right_bar_width, (14 - (bounded >> 4)) as usize);
            assert_eq!(hud.draws()[1].rect.x_bytes, hud.state.right_bar_x);
            assert_eq!(hud.draws()[1].rect.width_bytes, hud.state.right_bar_width);
            assert_eq!(
                hud.update_with_weapon(3, &mut random, -20)
                    .unwrap()
                    .weapon_counter,
                None
            );
            let previous = hud.draws()[1].rect;
            hud.update_with_weapon(5, &mut random, 223).unwrap();
            assert_eq!(hud.state.right_bar_x, 71);
            assert_eq!(hud.draws()[1].rect.x_bytes, previous.x_bytes);
            assert_eq!(hud.draws()[1].rect.width_bytes, previous.width_bytes);
            hud.update_with_weapon(6, &mut random, 223).unwrap();
            assert_eq!(hud.draws()[1].rect.x_bytes, 71);
            assert_eq!(hud.draws()[1].rect.width_bytes, 1);
        }
    }

    #[test]
    fn fuel_depletion_uses_signed_post_consumption_threshold_and_cached_geometry() {
        for (fuel, depleted) in [(261, false), (260, true), (0, true), (65535, true)] {
            let mut hud = Hud::new(&assets()).unwrap();
            hud.state.fuel = fuel;
            let mut random = Random::default();
            let effects = hud.update_with_weapon(2, &mut random, 4).unwrap();
            assert_eq!(effects.damage, if depleted { Some(2) } else { None });
            if depleted {
                assert_eq!(hud.state.fuel, 3583);
                assert_eq!(
                    (
                        hud.state.left_bar_sprite,
                        hud.state.left_bar_width,
                        hud.state.left_bar_height
                    ),
                    (0, 2, 1)
                );
                assert_eq!(hud.draws()[0].resolved_sprite, 24);
                // Another depleted phase-one call publishes the empty descriptor.
                hud.state.fuel = fuel;
                hud.update_with_weapon(5, &mut random, 4).unwrap();
                assert_eq!(
                    (
                        hud.draws()[0].resolved_sprite,
                        hud.draws()[0].rect.width_bytes,
                        hud.draws()[0].rect.height
                    ),
                    (0, 2, 1)
                );
            } else {
                assert_eq!(hud.state.fuel, 256);
            }
        }
    }

    #[test]
    fn progression_and_alternate_awards_use_word_arithmetic_and_decimal_carry() {
        let mut hud = Hud::new(&assets()).unwrap();
        let mut random = Random::default();
        hud.set_progression(17);
        hud.set_score_multiplier(4);
        hud.add_points(10);
        hud.set_score_multiplier(1);
        hud.update_with_weapon(2, &mut random, 4).unwrap();
        assert_eq!((hud.state.score, hud.state.pending_score), (57, 0));
        hud.score_digits = [9; 6];
        hud.add_points(500);
        hud.set_progression(0);
        hud.update_with_weapon(4, &mut random, 4).unwrap();
        assert_eq!(hud.state.score, 499);
        hud.state.pending_score = 31999;
        hud.add_points(2);
        assert_eq!(hud.state.pending_score, 32000);
        hud.add_points(1000);
        assert_eq!(hud.state.pending_score, 33000);
        hud.update_with_weapon(6, &mut random, 4).unwrap();
        assert_eq!(hud.state.pending_score, 0);
        assert_eq!(hud.score_digits[5], 9i16.wrapping_add(33000u16 as i16));
        assert_eq!(
            hud.draws()[7].resolved_sprite,
            usize::from(hud.score_digits[5].wrapping_add(70) as u8)
        );
    }

    #[test]
    fn marker_styles_move_new_request_before_old_source_resolution() {
        let resources = assets();
        let mut hud = Hud::new(&resources).unwrap();
        hud.set_marker_styles(&resources, [(175, 37, 7), (176, 36, 8), (177, 35, 9)])
            .unwrap();
        let mut random = Random::default();
        hud.update_with_weapon(1, &mut random, 4).unwrap();
        let choice = usize::from(hud.state.marker_requested - 175);
        assert_eq!(hud.state.marker_x, 37 - choice as i32);
        assert_eq!(hud.state.marker_y, 7 + choice as i32);
        assert_eq!(hud.state.marker_resolved, 0);
        assert_eq!(hud.draws().last().unwrap().rect.x_bytes, hud.state.marker_x);
        let requested = hud.state.marker_requested;
        hud.update_with_weapon(5, &mut random, 4).unwrap();
        assert_eq!(hud.state.marker_resolved, usize::from(requested));
        assert_eq!(hud.state.marker_width, 6);
        assert!(hud.set_marker_styles(&resources, [(256, 0, 0); 3]).is_err());
        hud.set_marker_styles(
            &resources,
            [(usize::from(hud.state.marker_previous), 0, 0); 3],
        )
        .unwrap();
        hud.state.marker_requested = hud.state.marker_previous;
        assert!(hud.update_with_weapon(6, &mut random, 4).is_err());
    }

    #[test]
    fn marker_position_waits_for_render_phase_after_style_change() {
        let resources = assets();
        let mut hud = Hud::new(&resources).unwrap();
        let mut random = Random::default();
        hud.update_with_weapon(1, &mut random, 4).unwrap();
        let preceding_draw = *hud.draws().last().unwrap();
        // A new difficulty's marker family changes both coordinates. Selection
        // on the even body updates the request without repainting the marker.
        hud.set_marker_styles(&resources, [(175, 35, 9); 3])
            .unwrap();
        hud.update_with_weapon(2, &mut random, 4).unwrap();
        assert_eq!(hud.state.marker_requested, 175);
        assert_eq!((hud.state.marker_x, hud.state.marker_y), (35, 9));
        for body in 2..=4 {
            if body != 2 {
                hud.update_with_weapon(body, &mut random, 4).unwrap();
            }
            let retained = *hud.draws().last().unwrap();
            assert_eq!(retained.resolved_sprite, preceding_draw.resolved_sprite);
            assert_eq!(
                (
                    retained.rect.x_bytes,
                    retained.rect.y,
                    retained.rect.width_bytes,
                    retained.rect.height,
                ),
                (
                    preceding_draw.rect.x_bytes,
                    preceding_draw.rect.y,
                    preceding_draw.rect.width_bytes,
                    preceding_draw.rect.height,
                )
            );
        }
        hud.update_with_weapon(5, &mut random, 4).unwrap();
        let resolved = *hud.draws().last().unwrap();
        assert_eq!(resolved.resolved_sprite, 175);
        assert_eq!((resolved.rect.x_bytes, resolved.rect.y), (35, 9));
        assert_eq!((resolved.rect.width_bytes, resolved.rect.height), (6, 23));
    }
}
