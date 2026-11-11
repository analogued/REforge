// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only publication of the existing selected artwork and native backgrounds.
use super::*;
mod bird;
mod hearts;
use crate::motion::{Blend as VisualBlend, Forecast, Layer, Publication};
impl Alley {
    pub(crate) fn motion_player_observation(&self) -> u64 {
        self.motion_player_step
    }

    pub(crate) fn remember_motion_clothes(
        &mut self,
        before: &Framebuffer,
        tick: u16,
    ) -> Result<(), String> {
        let row = usize::from(word(&self.data, 0x52f));
        let insertion = word(&self.data, 0x51d + row * 2);
        let offset = if row == 1 {
            insertion.checked_sub(79).ok_or("invalid clothes band")?
        } else {
            insertion
        };
        let dimensions = Dimensions(0x1028);
        self.motion_order = self.motion_order.wrapping_add(1);
        let ticks = self
            .motion_clothes_tick
            .map_or(3, |last| tick.wrapping_sub(last).clamp(1, 18));
        self.motion_clothes_tick = Some(tick);
        self.motion_clothes[row] = Some(crate::motion::ClothBand::advance(
            self.motion_clothes[row].take(),
            &before.read_rect(offset, dimensions)?,
            &self.screen.read_rect(offset, dimensions)?,
            offset,
            row == 1,
            self.motion_order,
            f64::from(ticks) * crate::timing::BIOS_TICK_PIT_CLOCKS as f64
                / crate::timing::PIT_HZ as f64,
        ));
        Ok(())
    }
    pub(crate) fn remember_motion_mouse(&mut self, row: usize, moving: bool) -> Result<(), String> {
        self.motion_order = self.motion_order.wrapping_add(1);
        self.motion_mice[row] = if let Some(image) = self.mice.images()[row] {
            let mut layer = Layer::new(
                4 + row as u8,
                image.offset,
                Dimensions(image.dimensions),
                sprite_bytes(&self.data, image.source, Dimensions(image.dimensions))?.to_vec(),
                match image.blend {
                    Blend::And => VisualBlend::And,
                    Blend::ZeroTransparent => VisualBlend::Zero,
                },
            );
            let x = f64::from(word(&self.data, 0x1f30 + row * 2));
            let y = f64::from(self.data[0x1f36 + row]);
            layer.position = [x, y];
            layer.observation = self.motion_order;
            layer.smooth_velocity = self.data[0x1f50 + row] == 0;
            if !layer.smooth_velocity {
                layer.position = layer.native.map(f64::from);
            }
            layer.cadenced = layer.smooth_velocity;
            layer.abrupt_turns = true;
            layer.bounds = Some(([0.0, y], [302.0, y]));
            let direction = if moving { self.data[0x1f3c + row] } else { 0 };
            let speed = f64::from(word(&self.data, 0x1f69));
            let dx = match direction {
                1 => speed,
                0xff => -speed,
                _ => 0.0,
            };
            layer.stride = [(x + dx).clamp(0.0, 302.0) - x, 0.0];
            layer.cadence_hint = Some(
                3.0 * crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64,
            );
            Some(layer)
        } else {
            None
        };
        Ok(())
    }
    pub(crate) fn motion_signature(&self, moving: bool) -> (u16, u16, u8, u64, u32, bool) {
        let visibility = (0..24).fold(0u32, |mask, i| {
            mask | (u32::from(self.data[0x348f + i] == 0 && self.data[0x34a7 + i] == 0) << i)
        });
        (
            self.scene(),
            self.player.x,
            self.player.y,
            self.motion_order,
            visibility
                | (u32::from(self.dog_image.is_some()) << 24)
                | (u32::from(self.projectile_image.is_some()) << 25),
            moving && self.motion_supported(),
        )
    }
    fn motion_supported(&self) -> bool {
        matches!(self.scene(), 0..=7)
            && !(self.scene() == 4 && self.data[0x39e1] != 0)
            && self.damage_effect.is_none()
            && self.failure.is_none()
            && self.entry_transition.is_none()
            && self.completion.is_none()
            && self.hearts_completion.is_none()
            && self.selection_work.is_none()
            && !self.game_over
    }
    pub(crate) fn remember_motion_fish(&mut self) -> Result<(), String> {
        let slot = usize::from(word(&self.data, 0x3415));
        if slot >= 24 {
            return Err("invalid aquarium observation slot".into());
        }
        let source = if slot >= 12 {
            0x3330 + (((slot as u16 * 8).wrapping_add(word(&self.data, 0x3413))) & 0x18)
        } else {
            let phase = word(&self.data, 0x3411) ^ if slot & 1 == 0 { 12 } else { 0 };
            0x3300 + phase + if self.data[0x3417 + slot] == 1 { 0 } else { 24 }
        };
        let dimensions = Dimensions(if slot < 12 { 0x0601 } else { 0x0202 });
        self.motion_order = self.motion_order.wrapping_add(1);
        let mut layer = Layer::new(
            16 + slot as u8,
            word(&self.data, 0x34bf + slot * 2),
            dimensions,
            sprite_bytes(&self.data, source, dimensions)?.to_vec(),
            VisualBlend::Opaque,
        );
        layer.position = [
            f64::from(word(&self.data, 0x3447 + slot * 2)),
            f64::from(self.data[0x3477 + slot]),
        ];
        layer.order = self.motion_order;
        layer.cadenced = true;
        layer.smooth_velocity = true;
        layer.abrupt_turns = true;
        layer.observation = self.motion_order;
        let top = f64::from(self.data[0x34f1 + slot]);
        layer.bounds = Some(([0.0, top], [302.0, top + 24.0]));
        let [x, y] = layer.position;
        let dx = if self.data[0x3417 + slot] == 1 {
            1.0
        } else {
            -1.0
        } * if slot < 12 { 4.0 } else { 2.0 };
        let dy = if self.data[0x342f + slot] == 1 {
            1.0
        } else {
            -1.0
        };
        layer.stride = [
            (x + dx).clamp(0.0, 302.0) - x,
            (y + dy).clamp(top, top + 24.0) - y,
        ];
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        layer.cadence_hint = Some(
            tick * if self.data[0x697] == 0xfd && self.player.y < 48 {
                1.0
            } else {
                2.0
            },
        );
        self.motion_fish[slot] = Some(layer);
        Ok(())
    }
    fn motion_dog_layer(&self, image: &crate::dog::VisibleSprite) -> Result<Layer, String> {
        let mut layer = Layer::new(
            1,
            image.offset,
            image.dimensions,
            image.pixels.clone(),
            if image.opaque {
                VisualBlend::Opaque
            } else {
                VisualBlend::Zero
            },
        );
        if image.opaque {
            // Pursuit pictures contain background pixels; retain their native
            // placement, including the milk room's contact/death sequence.
            layer.stepped = true;
            return Ok(layer);
        }
        let direction = self.data[0x1cd0];
        let phase = usize::from((self.data[0x1ccf] & 2) | if direction == 1 { 4 } else { 0 });
        let dimensions = Dimensions(0x0f04);
        layer.pixels =
            sprite_bytes(&self.data, word(&self.data, 0x15c8 + phase), dimensions)?.to_vec();
        layer.dimensions = dimensions;
        let crop = 4 - image.dimensions.0 as u8;
        let x = if crop > 0 && layer.native[0] == 0 {
            -f64::from(crop) * 8.0
        } else {
            f64::from(word(&self.data, 0x1cc6))
        };
        layer.native[0] = if x < 0.0 { x as i32 } else { layer.native[0] };
        layer.position = [x, f64::from(self.data[0x1cc8])];
        let entry = self.data[0x1cc0];
        let exit = self.data[0x1cc1];
        let next = if exit > 0 {
            let crop = 5 - exit;
            if direction == 1 {
                288.0 + f64::from(crop) * 8.0
            } else {
                -f64::from(crop) * 8.0
            }
        } else if entry > 1 {
            if direction == 1 {
                -f64::from(entry - 1) * 8.0
            } else {
                288.0 + f64::from(entry - 1) * 8.0
            }
        } else {
            let raw = f64::from(word(&self.data, 0x1cc6));
            if direction == 1 {
                (raw + 8.0).min(286.0)
            } else if direction == 0xff {
                (raw - 8.0).max(0.0)
            } else {
                raw
            }
        };
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        // Equal steps alternate one- and two-tick gates. Chasing the next gate
        // alone turns that timing pattern into a repeated 2:1 visual speed change.
        layer.smooth_velocity = true;
        layer.cadenced = true;
        layer.abrupt_turns = true;
        // A dog can stop at the logical edge before choosing to reverse. Its
        // zero stride must not retain an opposing coast when it runs back in.
        layer.coast_stops = true;
        layer.observation = u64::from(word(&self.data, 0x1ce1));
        layer.stride = [next - x, 0.0];
        layer.cadence_hint = Some(tick * 1.5);
        layer.bounds = Some(([-24.0, layer.position[1]], [312.0, layer.position[1]]));
        Ok(layer)
    }
    fn motion_projectile_layer(
        &self,
        image: crate::projectiles::VisibleSprite,
    ) -> Result<Layer, String> {
        let dimensions = Dimensions(word(&self.data, 0x17df));
        let mut layer = Layer::new(
            2,
            image.offset,
            dimensions,
            sprite_bytes(&self.data, image.source, dimensions)?.to_vec(),
            VisualBlend::Zero,
        );
        let x = word(&self.data, 0x1671);
        let y = self.data[0x1673];
        layer.position = [f64::from(x), f64::from(y)];
        layer.clip_bottom = i32::from(self.data[0x1676]) + dimensions.rows() as i32;
        let mut delay = word(&self.data, 0x17ea);
        if delay <= 1 {
            delay = delay.wrapping_sub(1)
        }
        let step = (delay & 0xff00) | u16::from(delay as u8 >> 3);
        let direction = if self.data[0x1677] != 0 && x & 0xfff8 == self.player.x & 0xfff8 {
            0
        } else {
            self.data[0x1674]
        };
        let next = match direction {
            1 => x.saturating_add(step).min(302),
            0 => x,
            _ => x.saturating_sub(step),
        };
        layer.forecast = Some(Forecast {
            seconds: crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64,
            delta: [
                f64::from(next) - f64::from(x),
                f64::from(self.data[0x17e9].wrapping_add(1) >> 1),
            ],
        });
        Ok(layer)
    }
    fn motion_broom_layer(&self) -> Result<Layer, String> {
        let dimensions = Dimensions(0x1e02);
        let source = word(&self.data, 0x3260 + usize::from(word(&self.data, 0x327a)));
        let mut layer = Layer::new(
            40,
            word(&self.data, 0x3282),
            dimensions,
            sprite_bytes(&self.data, source, dimensions)?.to_vec(),
            VisualBlend::Or,
        );
        layer.position = [
            f64::from(word(&self.data, 0x327d)),
            f64::from(self.data[0x327f]),
        ];
        layer.observation = self.motion_broom_step;
        layer.cadenced = true;
        layer.smooth_velocity = true;
        layer.abrupt_turns = true;
        layer.coast_stops = true;
        let signed = |step: f64, direction| match direction {
            1 => step,
            0xff => -step,
            _ => 0.0,
        };
        let [x, y] = layer.position;
        layer.bounds = Some(([0.0, 0.0], [304.0, 165.0]));
        layer.stride = [
            (x + signed(8.0, self.data[0x3280])).clamp(0.0, 304.0) - x,
            (y + signed(2.0, self.data[0x3281])).clamp(0.0, 165.0) - y,
        ];
        if !self.motion_broom_moving {
            layer.stride = [0.0; 2];
        }
        let ticks = word(&self.data, 0x32f2 + usize::from(self.scene()) * 2).max(1);
        layer.cadence_hint = Some(
            f64::from(ticks) * crate::timing::BIOS_TICK_PIT_CLOCKS as f64
                / crate::timing::PIT_HZ as f64,
        );
        Ok(layer)
    }
    pub(super) fn motion_spider_broom_stop_forecast(&self, tick: u16) -> Result<bool, String> {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::Broom) {
            return Ok(false);
        }
        // Probe the nearby broom/spider interaction on independent scratch
        // state. Include a broom call already due this pass and the following
        // gate. The latter may actually run after the next spider call, so
        // this is a conservative nearby-stop forecast, not a future simulation.
        // Current cat state and RNG can also change before the real collision.
        let mut data = self.data.clone();
        let mut screen = self.screen.clone();
        crate::room::advance_broom(&mut data, &mut screen, tick)?;
        crate::room::advance_broom(&mut data, &mut screen, tick.wrapping_add(2))?;
        let next = crate::bookcase::advance_spider_with_damage(
            &mut data,
            &mut screen,
            tick.wrapping_add(2),
            true,
        )?;
        Ok(matches!(
            next.quiet,
            Some(crate::room::QuietWork::SpiderBlocked | crate::room::QuietWork::SpiderRollback)
        ))
    }
    fn motion_spider_layer(&self) -> Result<Layer, String> {
        let dimensions = Dimensions(0x0e03);
        let mut layer = Layer::new(
            41,
            word(&self.data, 0x3968),
            dimensions,
            sprite_bytes(
                &self.data,
                word(&self.data, 0x396b).wrapping_add(0x38bc),
                dimensions,
            )?
            .to_vec(),
            VisualBlend::Zero,
        );
        let x = f64::from(word(&self.data, 0x3964));
        let y = f64::from(self.data[0x3966]);
        layer.position = [x, y];
        layer.observation = self.motion_spider_step;
        layer.cadenced = true;
        layer.smooth_velocity = true;
        layer.abrupt_turns = true;
        layer.coast_stops = true;
        layer.settle_stops = [true; 2];
        // Accepted displacement and column arrival are retained together:
        // a later cat move cannot revive the spider's preceding X forecast.
        layer.stride = self.motion_spider_stride;
        layer.cadence_hint =
            Some(2.0 * crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64);
        layer.bounds = Some(([0.0, 8.0], [296.0, 164.0]));
        Ok(layer)
    }
    pub fn motion_publication(&self) -> Result<Publication, String> {
        #[cfg(feature = "dev-tools")]
        let (broom_enabled, spider_enabled, bin_enabled, mice_enabled) = (
            !self.dev_options.actor_disabled(Actor::Broom),
            !self.dev_options.actor_disabled(Actor::Spider),
            !self.dev_options.actor_disabled(Actor::BinCat),
            !self.dev_options.actor_disabled(Actor::AlleyMice),
        );
        #[cfg(not(feature = "dev-tools"))]
        let (broom_enabled, spider_enabled, bin_enabled, mice_enabled) = (true, true, true, true);
        let mut result = Publication {
            native: self.presented.clone(),
            background: self.presented.clone(),
            colors: self.palette(),
            layers: Vec::new(),
            scene: self.scene(),
            supported: self.motion_supported(),
            coherent_water: self.scene() == 2 && self.motion_water.is_some(),
            hearts_presentation: self
                .hearts_completion
                .as_ref()
                .is_some_and(|ending| ending.falling_return()),
        };
        if let Some(ending) = &self.hearts_completion {
            if let Some(offset) = ending.falling_cat(&self.data) {
                let dimensions = Dimensions(0x0c03);
                let pixels = sprite_bytes(&self.data, 0x6e10, dimensions)?.to_vec();
                // The completed black wipe and zero erase are native contracts.
                // Exact matching prevents peeling a landing or overwritten image.
                if result.native.read_rect(offset, dimensions)? == pixels {
                    let mut layer = Layer::new(60, offset, dimensions, pixels, VisualBlend::Opaque);
                    layer.cadenced = true;
                    layer.native_by_default = true;
                    layer.observation = u64::from(offset);
                    layer.stride = [0.0, 12.0];
                    result.background.copy_rect(offset, dimensions, &[0; 72])?;
                    result.layers.push(layer);
                    result.supported = true;
                }
            }
            return Ok(result);
        }
        if !result.supported {
            return Ok(result);
        }
        let (mut background, player) = self.compositor.publication(&self.screen)?;
        if self.scene() == 0 {
            for (row, band) in self
                .motion_clothes
                .iter()
                .enumerate()
                .filter_map(|(i, b)| b.as_ref().map(|b| (i, b)))
            {
                if band.matches(&background.read_rect(band.offset, Dimensions(0x1028))?) {
                    result.layers.push(band.layer(row));
                }
            }
        }
        if matches!(self.scene(), 1 | 3..=6)
            && self.data[0x3286] == 0
            && broom_enabled
            && !(self.scene() == 5 && self.motion_bird_group_native())
        {
            let broom = self.motion_broom_layer()?;
            let offset = word(&self.data, 0x3282);
            let saved = &self.data[0x31e8..0x3260];
            let expected: Vec<_> = saved
                .iter()
                .zip(&broom.pixels)
                .map(|(old, sprite)| old | sprite)
                .collect();
            // Floor tracks can overwrite the bottom of a contact-blocked broom.
            // Its old saved background can no longer separate that image safely.
            // Keep just this broom native until a fresh draw restores coherence;
            // unrelated actors need not lose their motion tracks in the meantime.
            if background.read_rect(offset, broom.dimensions)? == expected {
                background.copy_rect(offset, broom.dimensions, saved)?;
                result.layers.push(broom);
            }
        }
        if self.scene() == 3 && self.data[0x396a] == 0 && spider_enabled {
            let spider = self.motion_spider_layer()?;
            let saved = &self.data[0x396f..0x39c3];
            let expected: Vec<_> = saved
                .iter()
                .zip(&spider.pixels)
                .map(|(old, sprite)| {
                    let mask = (sprite | (sprite >> 1)) & 0x55;
                    (old & !(mask | (mask << 1))) | sprite
                })
                .collect();
            let offset = word(&self.data, 0x3968);
            // Peel the broom first, then the spider beneath it. If later writes
            // prevent exact separation, retain just that image in native scenery.
            if background.read_rect(offset, spider.dimensions)? == expected {
                background.copy_rect(offset, spider.dimensions, saved)?;
                result.layers.insert(0, spider);
            }
        }
        if self.scene() == 5 {
            self.motion_bird_layers(&mut background, &mut result.layers)?;
        }
        if self.scene() == 7 {
            self.motion_hearts_layers(&mut background, &mut result.layers)?;
        }
        if self.scene() == 2 {
            let mut fish: Vec<_> = self
                .motion_fish
                .iter()
                .enumerate()
                .filter(|(i, l)| {
                    l.as_ref().is_some_and(|l| {
                        l.position
                            == [
                                f64::from(word(&self.data, 0x3447 + i * 2)),
                                f64::from(self.data[0x3477 + i]),
                            ]
                    }) && self.data[0x348f + i] == 0
                        && self.data[0x34a7 + i] == 0
                })
                .filter_map(|(_, l)| l.clone())
                .collect();
            fish.sort_by_key(|l| l.order);
            if let Some(water) = &self.motion_water {
                background = water.clone();
            } else {
                result.supported = false;
            }
            result.layers.extend(fish);
        }
        if let Some((offset, dimensions, pixels)) = player {
            let mut layer = Layer::new(0, offset, dimensions, pixels, VisualBlend::And);
            let logical_anchor = layer.native[0] == i32::from(self.player.x & !3)
                && layer.native[1] == i32::from(self.player.y);
            let mut ordinary = self.scene() != 2
                && self.player.entry_state == 0
                && logical_anchor
                && !(self.scene() == 6 && self.data[0x44bd] != 0);
            let mut position = [f64::from(self.player.x), f64::from(self.player.y)];
            if self.scene() == 7 && ordinary {
                let normalized = self.motion_hearts_player(&mut layer)?;
                if normalized {
                    position[1] = f64::from(self.player.vertical_coordinate) - 50.0;
                } else if self.player.vertical_coordinate < 50 || self.player.y >= 187 {
                    ordinary = false;
                }
            }
            if ordinary && dimensions == Dimensions(0x0b03) {
                layer.pose_offset = crate::motion::walking::offset(&self.data, &layer.pixels)?;
            }
            layer.indoor_cat = matches!(self.scene(), 1 | 3..=7) && ordinary;
            if layer.indoor_cat {
                let tick =
                    crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
                // Active movement has a changed-tick and countdown-expiry
                // opportunity. Idle movement additionally waits for retrace.
                // Observations can slow this prior; short gaps within a tick
                // cannot make the sustained visual cadence faster.
                layer.cadence_hint = Some(
                    tick * if self.player.horizontal_direction == 0
                        && self.player.vertical_direction == 0
                    {
                        1.0
                    } else {
                        0.5
                    },
                );
            }
            layer.stepped = matches!(self.scene(), 1 | 3..=7) && !ordinary;
            layer.cadenced = self.scene() == 2 || ordinary;
            layer.smooth_velocity = layer.cadenced;
            if layer.cadenced {
                layer.observation = self.motion_player_step;
                layer.bounds = Some(if ordinary {
                    if self.scene() == 7 {
                        if self.player.vertical_direction == 0 {
                            ([36.0, position[1]], [270.0, position[1]])
                        } else {
                            ([36.0, -50.0], [270.0, 198.0])
                        }
                    } else if self.player.vertical_direction == 0 {
                        let y = f64::from(self.player.y);
                        ([8.0, y], [290.0, y])
                    } else {
                        ([8.0, 0.0], [290.0, 180.0])
                    }
                } else {
                    ([8.0, 2.0], [290.0, 179.0])
                });
                let signed = |step: f64, direction: u8| match direction {
                    1 => step,
                    0xff => -step,
                    _ => 0.0,
                };
                let x = f64::from(self.player.x);
                let y = position[1];
                let difficulty = usize::from(word(&self.data, 8));
                let dx = if ordinary && self.player.support_state != 0 {
                    0.0
                } else if !ordinary {
                    signed(f64::from(word(&self.data, 0x572)), self.data[0x56e])
                } else {
                    signed(
                        f64::from(self.player.horizontal_step),
                        self.player.horizontal_direction,
                    )
                };
                let dy = if ordinary {
                    signed(
                        f64::from(self.player.vertical_step),
                        self.player.vertical_direction,
                    )
                } else {
                    signed(
                        f64::from((self.data[0x576] >> 4).min(self.data[0x67c + difficulty])),
                        self.data[0x571],
                    )
                };
                let (low, high) = layer.bounds.unwrap();
                layer.stride = [
                    (x + dx).clamp(low[0], high[0]) - x,
                    (y + dy).clamp(low[1], high[1]) - y,
                ];
                layer.abrupt_turns = ordinary;
                layer.coast_stops = ordinary;
                // Stops must reconcile to the logical position; retaining the
                // last visual pixel can preserve a multi-pixel coast forever.
                layer.settle_stops[0] = ordinary;
                if ordinary && self.scene() == 7 {
                    layer.settle_stops[1] = true;
                    let (next, outcome) = self.motion_hearts_next_player()?;
                    let next_y = f64::from(next.vertical_coordinate) - 50.0;
                    let (mut low, mut high) = layer.bounds.unwrap();
                    if self.player.vertical_direction != 0 {
                        if outcome.landed {
                            // Settle continuously onto the known next support,
                            // including native one-pixel upward corrections. A
                            // fresh observer must retain its starting position.
                            low[1] = next_y;
                            high[1] = next_y;
                            if next_y == y {
                                // A boundary can cancel a requested jump before
                                // it ever moves the native picture.
                                layer.stride[1] = 0.0;
                            }
                        } else {
                            layer.stride[1] = next_y - y;
                        }
                    }
                    if self.motion_hearts_player_near_rival(&next) {
                        // Near potential contact, limit independent prediction
                        // to half of the next native step. Neither participant
                        // should run a whole body width past the other.
                        for (axis, delta) in
                            [f64::from(next.x) - x, next_y - y].into_iter().enumerate()
                        {
                            let at = if axis == 0 { x } else { y };
                            let limit = (at + delta * 0.5).clamp(low[axis], high[axis]);
                            if delta > 0.0 {
                                high[axis] = high[axis].min(limit);
                            } else if delta < 0.0 {
                                low[axis] = low[axis].max(limit);
                            }
                        }
                    }
                    layer.bounds = Some((low, high));
                } else if ordinary && self.player.vertical_direction == 1 {
                    let mut next = self.player.clone();
                    next.update_with_projectiles(self.requested_controls(), &self.data, false)?;
                    if next.support_state != 0 {
                        // The next native move attaches to a support and stops
                        // horizontal travel. Do not predict past that known
                        // landing, then slide back after the real attachment.
                        let stop = f64::from(next.x);
                        let (mut low, mut high) = layer.bounds.unwrap();
                        if dx < 0.0 {
                            low[0] = stop;
                        } else if dx > 0.0 {
                            high[0] = stop;
                        }
                        layer.bounds = Some((low, high));
                    }
                }
            }
            // Retained clipped/entry images can have an anchor distinct from the player.
            if logical_anchor {
                layer.position = position;
            }
            if self.scene() == 0 && self.player.support_state != 0 {
                let row = match self.player.y & 0xf8 {
                    0x08 => Some(0),
                    0x28 => Some(1),
                    0x48 => Some(2),
                    _ => None,
                };
                if let Some(parent) =
                    row.and_then(|row| result.layers.iter().find(|l| l.id == 8 + row))
                {
                    layer.parent = Some(parent.id);
                    layer.epoch = parent.epoch;
                    layer.position[0] -= parent.position[0];
                    if let Some((low, high)) = layer.bounds.as_mut() {
                        low[0] -= parent.position[0];
                        high[0] -= parent.position[0];
                    }
                }
            }
            result.layers.push(layer);
        }
        if let Some(image) = &self.dog_image {
            result.layers.push(self.motion_dog_layer(image)?);
        }
        if let Some(image) = self.projectile_image {
            result.layers.push(self.motion_projectile_layer(image)?);
        }
        if self.scene() == 0 {
            if let Some(image) = self.bin_image.filter(|_| bin_enabled) {
                result.layers.push(Layer::new(
                    3,
                    image.offset,
                    image.dimensions,
                    sprite_bytes(&self.data, image.source, image.dimensions)?.to_vec(),
                    VisualBlend::Zero,
                ));
            }
            if mice_enabled {
                for (i, image) in self
                    .mice
                    .images()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, x)| x.map(|x| (i, x)))
                    .chain(self.mouse_effect.map(|x| (3, x)))
                {
                    let dimensions = Dimensions(image.dimensions);
                    if i < 3
                        && let Some(cached) = &self.motion_mice[i]
                        && cached.native
                            == Layer::new(
                                4 + i as u8,
                                image.offset,
                                dimensions,
                                Vec::new(),
                                VisualBlend::And,
                            )
                            .native
                    {
                        result.layers.push(cached.clone());
                        continue;
                    }
                    result.layers.push(Layer::new(
                        4 + i as u8,
                        image.offset,
                        dimensions,
                        sprite_bytes(&self.data, image.source, dimensions)?.to_vec(),
                        match image.blend {
                            Blend::And => VisualBlend::And,
                            Blend::ZeroTransparent => VisualBlend::Zero,
                        },
                    ));
                }
            }
        }
        result.background = background;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_cat_publication_counts_accepted_updates_and_uses_logical_motion() {
        let mut game = Alley::synthetic();
        game.player = Player::at_alley_floor(100);
        for address in (0xf7a..0xf92).step_by(2) {
            set_word(&mut game.data, address, 0x2000);
        }
        game.data[0x2000..0x2042].fill(0xff);
        game.advance_player_update(Controls { x: 1, y: 0 }).unwrap();
        let data = game.data.clone();
        let screen = game.screen.bytes().to_vec();
        let publication = game.motion_publication().unwrap();
        let cat = publication.layers.iter().find(|l| l.id == 0).unwrap();
        assert!(cat.cadenced && cat.smooth_velocity && cat.abrupt_turns);
        assert_eq!(cat.position, [f64::from(game.player.x), 180.0]);
        assert_eq!(cat.stride, [f64::from(game.player.horizontal_step), 0.0]);
        assert_eq!(cat.observation, game.motion_player_step);
        assert_eq!(cat.bounds, Some(([8.0, 180.0], [290.0, 180.0])));
        assert_eq!(game.data, data);
        assert_eq!(game.screen.bytes().as_slice(), screen);
        let observation = cat.observation;
        // Retained artwork still gets an acceptance serial while attached.
        game.player.support_state = 2;
        game.advance_player_update(Controls::default()).unwrap();
        let next = game.motion_publication().unwrap();
        let cat = next.layers.iter().find(|l| l.id == 0).unwrap();
        assert_eq!(cat.observation, observation + 1);
        assert_eq!(cat.stride, [0.0; 2]);
    }

    #[test]
    fn mouse_observation_keeps_native_state_and_preserves_a_caught_sprite_anchor() {
        let mut game = Alley::synthetic();
        set_word(&mut game.data, 0x1f30, 100);
        game.data[0x1f36] = 60;
        game.data[0x1f3c] = 1;
        set_word(&mut game.data, 0x1f42, pixel_offset(100, 60));
        set_word(&mut game.data, 0x1f69, 8);
        game.mice = Mice::from_data(&game.data).unwrap();
        let data = game.data.clone();
        game.remember_motion_mouse(0, true).unwrap();
        assert_eq!(game.data, data);
        let mouse = game.motion_mice[0].as_ref().unwrap();
        assert_eq!(mouse.stride, [8.0, 0.0]);
        assert!(mouse.smooth_velocity);
        game.data[0x1f50] = 1;
        set_word(&mut game.data, 0x1f30, 108);
        game.mice = Mice::from_data(&game.data).unwrap();
        game.remember_motion_mouse(0, false).unwrap();
        let caught = game.motion_mice[0].as_ref().unwrap();
        assert!(!caught.smooth_velocity);
        assert_eq!(caught.position, [100.0, 60.0]);
    }
    #[test]
    fn aquatic_velocity_publication_uses_native_steps_bands_and_ring_regime() {
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        for (machine, y, ticks) in [(0, 30, 2.0), (0xfd, 30, 1.0), (0xfd, 80, 2.0)] {
            for (slot, step) in [(0, 4.0), (12, 2.0)] {
                let mut game = Alley::synthetic();
                game.data[0x697] = machine;
                game.data[0x57b] = y;
                game.player.y = y;
                set_word(&mut game.data, 0x3415, slot as u16);
                set_word(&mut game.data, 0x3447 + slot * 2, 100);
                set_word(&mut game.data, 0x34bf + slot * 2, pixel_offset(100, 60));
                game.data[0x3477 + slot] = 60;
                game.data[0x34f1 + slot] = 50;
                game.data[0x3417 + slot] = 1;
                game.data[0x342f + slot] = 0xff;
                let data = game.data.clone();
                let screen = game.screen.bytes().to_vec();
                game.remember_motion_fish().unwrap();
                assert_eq!(game.data, data);
                assert_eq!(game.screen.bytes().as_slice(), screen);
                let layer = game.motion_fish[slot].as_ref().unwrap();
                assert!(layer.smooth_velocity && layer.abrupt_turns);
                assert_eq!(layer.stride, [step, -1.0]);
                assert_eq!(layer.bounds, Some(([0.0, 50.0], [302.0, 74.0])));
                assert_eq!(layer.cadence_hint, Some(tick * ticks));
            }
        }
    }

    #[test]
    fn fish_publication_retains_actual_selected_art_and_observation_preserves_native_state() {
        let mut game = Alley::synthetic();
        set_word(&mut game.data, 4, 2);
        set_word(&mut game.data, 0x3415, 12);
        game.data[0x3330..0x3338].fill(0x1b);
        game.data[0x3338..0x3340].fill(0xe4);
        game.motion_water = Some(Framebuffer::default());
        set_word(&mut game.data, 0x34bf + 12 * 2, pixel_offset(40, 50));
        set_word(&mut game.data, 0x3447 + 12 * 2, 40);
        game.data[0x3477 + 12] = 50;
        let before = game.data.clone();
        let screen = game.screen.bytes().to_vec();
        game.remember_motion_fish().unwrap();
        assert_eq!(game.data, before);
        assert_eq!(game.screen.bytes().as_slice(), screen);
        // Another slot can change the global phase before this eel redraws.
        set_word(&mut game.data, 0x3413, 8);
        let p = game.motion_publication().unwrap();
        let eel = p.layers.iter().find(|l| l.id == 28).unwrap();
        assert_eq!(eel.pixels, vec![0x1b; 8]);
        let before = game.data.clone();
        for _ in 0..5 {
            std::hint::black_box(game.motion_publication().unwrap());
        }
        assert_eq!(game.data, before);
        // A disabled/collected slot retires instead of leaving its image behind.
        game.data[0x34a7 + 12] = 1;
        assert!(
            game.motion_publication()
                .unwrap()
                .layers
                .iter()
                .all(|l| l.id != 28)
        );
    }
    #[test]
    fn fully_occluded_fish_retirement_is_observed_even_when_native_pixels_do_not_change() {
        let mut game = Alley::synthetic();
        set_word(&mut game.data, 4, 2);
        set_word(&mut game.data, 0x3415, 12);
        game.motion_water = Some(Framebuffer::default());
        game.remember_motion_fish().unwrap();
        let mut presenter = crate::motion::Presenter::default();
        presenter.observe(&game, 0, 0, true).unwrap();
        assert!(presenter.is_active());
        let pixels = game.framebuffer().bytes().to_vec();
        game.data[0x34a7 + 12] = 1;
        presenter
            .observe(&game, 1_000_000, 1_000_000, true)
            .unwrap();
        assert_eq!(game.framebuffer().bytes().as_slice(), pixels);
        assert!(!presenter.is_active());
        assert_eq!(presenter.observations, 2);
    }
    #[test]
    fn normalized_dog_art_matches_every_native_edge_crop_without_shape_resets() {
        for scene in [0, 1, 3, 4, 5] {
            for direction in [1, 0xff] {
                for crop in 0..4 {
                    let mut game = Alley::synthetic();
                    set_word(&mut game.data, 4, scene);
                    game.data[0x3286] = 1;
                    game.screen.fill_visible(0xaa);
                    game.data[0x1cd0] = direction;
                    game.data[0x1ccf] = 0;
                    let phase = if direction == 1 { 4 } else { 0 };
                    set_word(&mut game.data, 0x15c8 + phase, 0x2000);
                    for (i, p) in game.data[0x2000..0x2078].iter_mut().enumerate() {
                        *p = (i as u8).wrapping_mul(37);
                    }
                    set_word(&mut game.data, 0x1cbb, 0x2000);
                    set_word(&mut game.data, 0x1cc4, 0x0f04);
                    game.data[0x1cc8] = 177;
                    set_word(&mut game.data, 0x1cc6, if direction == 1 { 0 } else { 288 });
                    if crop > 0 {
                        crate::dog::prepare_edge_sprite(&mut game.data, crop, direction).unwrap();
                    }
                    let dimensions = Dimensions(word(&game.data, 0x1cc4));
                    let offset = pixel_offset(word(&game.data, 0x1cc6), 177);
                    let pixels = sprite_bytes(&game.data, word(&game.data, 0x1cbb), dimensions)
                        .unwrap()
                        .to_vec();
                    game.dog_image = Some(crate::dog::VisibleSprite {
                        dimensions,
                        offset,
                        pixels,
                        opaque: false,
                    });
                    game.compose_actors().unwrap();
                    let publication = game.motion_publication().unwrap();
                    assert!(
                        publication.native_composition_matches(),
                        "scene={scene}, direction={direction}, crop={crop}"
                    );
                    assert_eq!(publication.layers[0].dimensions, Dimensions(0x0f04));
                    let layer = &publication.layers[0];
                    assert!(layer.smooth_velocity && layer.cadenced && layer.abrupt_turns);
                    assert!(layer.coast_stops);
                    assert!(layer.forecast.is_none());
                    let tick =
                        crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
                    assert_eq!(layer.cadence_hint, Some(tick * 1.5));
                }
            }
        }
    }

    #[test]
    fn opaque_dog_pursuit_remains_native_including_milk_room() {
        for scene in [0, 1, 3, 4, 5, 6] {
            let mut game = Alley::synthetic();
            set_word(&mut game.data, 4, scene);
            game.data[0x3286] = 1;
            game.screen.fill_visible(0xaa);
            let image = crate::dog::VisibleSprite {
                dimensions: Dimensions(0x0f04),
                offset: pixel_offset(104, 177),
                pixels: vec![0x55; 120],
                opaque: true,
            };
            game.dog_image = Some(image.clone());
            game.compose_actors().unwrap();
            let data = game.data.clone();
            let publication = game.motion_publication().unwrap();
            assert!(publication.native_composition_matches());
            let dog = publication.layers.iter().find(|l| l.id == 1).unwrap();
            assert!(dog.stepped && !dog.smooth_velocity && !dog.cadenced);
            assert_eq!(dog.position, [104.0, 177.0]);
            assert_eq!(dog.dimensions, image.dimensions);
            assert_eq!(dog.pixels, image.pixels);
            assert_eq!(publication.placed_rgba(), game.rgba());
            assert_eq!(game.data, data);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn actual_projectile_flight_is_monotone_through_acceleration_and_floor_clipping() {
        let mut game = Alley::synthetic();
        let mut dev = DevOptions::default();
        dev.set_disabled_damage("all").unwrap();
        game.set_dev_options(dev).unwrap();
        game.data[0x1673] = 20;
        game.data[0x1674] = 1;
        game.data[0x1676] = 100;
        game.data[0x17e9] = 1;
        set_word(&mut game.data, 0x1671, 75);
        set_word(&mut game.data, 0x17ea, 32);
        set_word(&mut game.data, 0x17dd, 0x2000);
        set_word(&mut game.data, 0x17df, 0x0c02);
        game.data[0x2000..0x2030].fill(0xff);
        let period = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        let mut presenter = crate::motion::Presenter::default();
        let mut clipped = false;
        let mut last_y = 0.0;
        for tick in 1..40 {
            game.advance_projectiles(tick).unwrap();
            let at = (f64::from(tick) * period * 1e9) as u128;
            presenter.observe(&game, at, at, true).unwrap();
            let Some(image) = game.projectile_image else {
                break;
            };
            clipped |= image.dimensions.0 != 0x0c02;
            assert!(
                game.motion_publication()
                    .unwrap()
                    .native_composition_matches()
            );
            for frame in 0..6 {
                let now = at + (frame as f64 * period / 6.0 * 1e9) as u128;
                let sample = presenter
                    .samples(now)
                    .into_iter()
                    .find(|p| p.actor == 2)
                    .unwrap();
                assert_eq!(sample.dimensions, 0x0c02);
                assert!(sample.horizon > 0.05);
                if last_y > 0.0 {
                    assert!(
                        sample.drawn[1] - last_y < 3.0,
                        "visible jump: {last_y} -> {}",
                        sample.drawn[1]
                    );
                }
                assert!(
                    sample.drawn[1] >= last_y - 1e-8,
                    "{last_y} -> {}",
                    sample.drawn[1]
                );
                last_y = sample.drawn[1];
                assert_eq!(sample.target[0], f64::from(word(&game.data, 0x1671)));
            }
        }
        assert!(
            clipped,
            "the probe must exercise the shrinking native sprite"
        );
    }
}

#[cfg(test)]
mod indoor_tests {
    use super::*;
    fn room(scene: u16) -> Alley {
        let mut game = Alley::synthetic();
        set_word(&mut game.data, 4, scene);
        game.screen.fill_visible(0xaa);
        set_word(&mut game.data, 0x3260, 0x6000);
        game.data[0x6000..0x6078].fill(0x55);
        set_word(&mut game.data, 0x327d, 48);
        game.data[0x327f] = 80;
        game.data[0x3280] = 1;
        game.data[0x3281] = 0xff;
        set_word(&mut game.data, 0x3284, pixel_offset(48, 80));
        set_word(
            &mut game.data,
            0x32f2 + usize::from(scene) * 2,
            if scene == 3 { 2 } else { 1 },
        );
        crate::room::draw_broom(&mut game.data, &mut game.screen).unwrap();
        game.motion_broom_moving = true;
        game.player = Player::at_alley_floor(100);
        game.compositor
            .draw(
                &mut game.screen,
                pixel_offset(100, 180),
                Dimensions(0x0101),
                &[0; 2],
            )
            .unwrap();
        game.compose_actors().unwrap();
        game
    }
    #[test]
    fn indoor_publication_separates_broom_and_cat_without_changing_native_state() {
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        for scene in [1, 3, 4, 5, 6] {
            let game = room(scene);
            let data = game.data.clone();
            let pixels = game.presented.bytes().to_vec();
            let publication = game.motion_publication().unwrap();
            assert!(publication.native_composition_matches());
            let broom = publication.layers.iter().find(|l| l.id == 40).unwrap();
            assert_eq!(broom.position, [48.0, 80.0]);
            assert_eq!(broom.stride, [8.0, -2.0]);
            assert_eq!(
                broom.cadence_hint,
                Some(tick * if scene == 3 { 2.0 } else { 1.0 })
            );
            let cat = publication.layers.iter().find(|l| l.id == 0).unwrap();
            assert!(cat.smooth_velocity && cat.coast_stops && !cat.stepped);
            assert_eq!(game.data, data);
            assert_eq!(game.presented.bytes().as_slice(), pixels);
        }
    }
    #[test]
    fn floor_track_overwrite_keeps_only_the_broom_native_and_preserves_dog_motion() {
        for scene in [1, 3, 4, 5, 6] {
            let mut game = room(scene);
            crate::room::restore_broom(&game.data, &mut game.screen).unwrap();
            set_word(&mut game.data, 0x327d, 112);
            game.data[0x327f] = 165;
            set_word(&mut game.data, 0x3284, pixel_offset(112, 165));
            crate::room::draw_broom(&mut game.data, &mut game.screen).unwrap();
            set_word(&mut game.data, 0x15c8, 0x6100);
            game.data[0x6100..0x6178].fill(0x55);
            set_word(&mut game.data, 0x1cc6, 160);
            game.data[0x1cc8] = 177;
            game.data[0x1cd0] = 0xff;
            game.dog_image = Some(crate::dog::VisibleSprite {
                offset: pixel_offset(160, 177),
                dimensions: Dimensions(0x0f04),
                pixels: vec![0x55; 120],
                opaque: false,
            });
            game.compose_actors().unwrap();
            let mut presenter = crate::motion::Presenter::default();
            presenter.observe(&game, 0, 0, true).unwrap();
            let before = presenter
                .samples(20_000_000)
                .into_iter()
                .find(|s| s.actor == 1)
                .unwrap()
                .drawn;
            // The native floor-track writer replaces the broom's last three
            // rows while a contact can hold that broom image in place.
            set_word(&mut game.data, 0x579, 100);
            game.data[0x57b] = 180;
            game.data[0x56e] = 1;
            game.data[0x32c2..0x32cc].fill(0);
            assert!(crate::room::record_floor_track(&mut game.data, &mut game.screen).unwrap());
            game.compose_actors().unwrap();
            let data = game.data.clone();
            let native = game.rgba();
            let publication = game.motion_publication().unwrap();
            assert!(publication.native_composition_matches());
            assert!(!publication.layers.iter().any(|l| l.id == 40));
            assert_eq!(publication.placed_rgba(), native);
            assert_eq!(game.data, data);
            presenter
                .observe(&game, 20_000_000, 20_000_000, true)
                .unwrap();
            assert!(presenter.is_active());
            let dog = |now| {
                presenter
                    .samples(now)
                    .into_iter()
                    .find(|s| s.actor == 1)
                    .unwrap()
                    .drawn
            };
            assert_eq!(dog(20_000_000), before);
            assert!(dog(30_000_000)[0] < before[0]);
            // A normal native redraw makes the broom separable again.
            crate::room::restore_broom(&game.data, &mut game.screen).unwrap();
            crate::room::draw_broom(&mut game.data, &mut game.screen).unwrap();
            game.compose_actors().unwrap();
            let publication = game.motion_publication().unwrap();
            assert!(publication.native_composition_matches());
            assert!(publication.layers.iter().any(|l| l.id == 40));
        }
    }

    fn spider_room() -> Alley {
        let mut game = room(3);
        crate::bookcase::initialize(&mut game.data, &mut game.screen).unwrap();
        game.data[0x38bc..0x3910].fill(0x1b);
        game.data[0x3910..0x3964].fill(0xe4);
        set_word(&mut game.data, 0x39cc, 4);
        #[cfg(feature = "dev-tools")]
        {
            let mut dev = DevOptions::default();
            dev.set_disabled_damage("all").unwrap();
            game.set_dev_options(dev).unwrap();
        }
        game
    }

    #[test]
    fn spider_publication_tracks_completed_moves_phases_and_blocked_updates() {
        let mut game = spider_room();
        assert!(
            !game
                .motion_publication()
                .unwrap()
                .layers
                .iter()
                .any(|l| l.id == 41)
        );
        assert!(game.advance_bookcase_spider(2).unwrap().drew);
        let data = game.data.clone();
        let pixels = game.rgba();
        let first = game.motion_publication().unwrap();
        assert!(first.native_composition_matches());
        let spider = first.layers.iter().find(|l| l.id == 41).unwrap();
        assert_eq!(spider.position, [276.0, 8.0]);
        assert_eq!(spider.stride, [-4.0, 0.0]);
        assert_eq!(spider.observation, 1);
        assert_eq!(spider.pixels, vec![0x1b; 84]);
        assert!(
            spider.cadenced && spider.smooth_velocity && spider.abrupt_turns && spider.coast_stops
        );
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        assert_eq!(spider.cadence_hint, Some(tick * 2.0));
        assert_eq!(game.data, data);
        assert_eq!(game.rgba(), pixels);
        assert!(game.advance_bookcase_spider(4).unwrap().drew);
        let next = game.motion_publication().unwrap();
        assert!(next.native_composition_matches());
        let spider = next.layers.iter().find(|l| l.id == 41).unwrap();
        assert_eq!(spider.pixels, vec![0xe4; 84]);
        assert_eq!(spider.position, [272.0, 8.0]);
        set_word(&mut game.data, 0x327d, 272);
        game.data[0x327f] = 8;
        let step = game.advance_bookcase_spider(6).unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!(game.motion_spider_layer().unwrap().stride, [0.0; 2]);
        assert_eq!(game.motion_spider_layer().unwrap().observation, 3);
        // A proposed vertical step can change phase but still be rolled back.
        set_word(&mut game.data, 0x3964, 280);
        game.data[0x3966] = 31;
        game.data[0x3967] = 1;
        game.data[0x396e] = 3;
        game.player.x = 280;
        set_word(&mut game.data, 0x327d, 280);
        game.data[0x327f] = 46;
        let step = game.advance_bookcase_spider(8).unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!(game.data[0x396e], 4);
        assert_eq!(game.data[0x3966], 31);
        assert_eq!(game.motion_spider_layer().unwrap().stride, [0.0; 2]);
    }

    #[test]
    fn spider_stops_horizontal_prediction_on_the_native_column_arrival() {
        for move_cat in [false, true] {
            let mut game = spider_room();
            game.player.x = 178;
            for tick in (2..=48).step_by(2) {
                assert!(game.advance_bookcase_spider(tick).unwrap().drew);
            }
            assert_eq!(word(&game.data, 0x3964), 184);
            assert_eq!(game.motion_spider_layer().unwrap().stride, [-4.0, 0.0]);
            game.advance_bookcase_spider(50).unwrap();
            let spider = game.motion_spider_layer().unwrap();
            // Native alignment compares eight-pixel columns, not exact cat X.
            assert_eq!(spider.position, [180.0, 8.0]);
            assert_eq!(spider.stride, [0.0; 2]);
            assert_eq!(spider.settle_stops, [true; 2]);
            if move_cat {
                game.player.x = 200;
                let waiting = game.motion_spider_layer().unwrap();
                assert_eq!(waiting.stride, [0.0; 2]);
                assert_eq!(waiting.observation, spider.observation);
            }
            game.advance_bookcase_spider(52).unwrap();
            let next = game.motion_spider_layer().unwrap();
            if move_cat {
                assert_eq!(next.position, [184.0, 8.0]);
                assert_eq!(next.stride, [4.0, 0.0]);
            } else {
                assert_eq!(next.position, [180.0, 10.0]);
                assert_eq!(next.stride, [0.0, 2.0]);
            }
        }
    }

    #[test]
    fn spider_broom_forecast_uses_scratch_state_and_both_vertical_directions() {
        for (y, direction, broom_y) in [(45, 1, 26), (120, 0xff, 88)] {
            let mut game = spider_room();
            set_word(&mut game.data, 0x3964, 176);
            game.data[0x3966] = y;
            game.data[0x3967] = direction;
            game.data[0x396e] = 11;
            set_word(&mut game.data, 0x327d, 152);
            game.data[0x327f] = broom_y;
            game.data[0x3280] = 1;
            game.data[0x3281] = 0;
            set_word(&mut game.data, 0x39c8, 2);
            let data = game.data.clone();
            let screen = game.screen.bytes().to_vec();
            // The broom's next horizontal step reaches the spider's column.
            assert!(game.motion_spider_broom_stop_forecast(2).unwrap());
            assert_eq!(game.data, data);
            assert_eq!(game.screen.bytes().as_slice(), screen);
            // Moving away or disabling the broom removes the forecast stop.
            game.data[0x3280] = 0xff;
            assert!(!game.motion_spider_broom_stop_forecast(2).unwrap());
            #[cfg(feature = "dev-tools")]
            {
                game.data[0x3280] = 1;
                game.dev_options.set_disabled_actors("broom").unwrap();
                assert!(!game.motion_spider_broom_stop_forecast(2).unwrap());
            }
        }
    }

    #[test]
    fn spider_stop_forecast_includes_a_broom_call_already_due_this_pass() {
        let mut game = spider_room();
        set_word(&mut game.data, 0x3964, 176);
        game.data[0x3966] = 45;
        game.data[0x3967] = 1;
        game.data[0x396e] = 11;
        set_word(&mut game.data, 0x327d, 144);
        game.data[0x327f] = 26;
        game.data[0x3280] = 1;
        game.data[0x3281] = 0;
        set_word(&mut game.data, 0x39c8, 0xffff);
        set_word(&mut game.data, 0x32f8, 2);
        set_word(&mut game.data, 0x328c, 0xffff);
        assert!(!game.motion_spider_broom_stop_forecast(0xffff).unwrap());
        set_word(&mut game.data, 0x328c, 0xfffd);
        assert!(game.motion_spider_broom_stop_forecast(0xffff).unwrap());
        // Tick-wrap retains the same due/not-due distinction.
        set_word(&mut game.data, 0x39c8, 1);
        assert!(game.motion_spider_broom_stop_forecast(1).unwrap());
        set_word(&mut game.data, 0x328c, 1);
        assert!(!game.motion_spider_broom_stop_forecast(1).unwrap());
    }

    #[test]
    fn spider_and_broom_peel_in_reverse_order_and_keep_native_overwrites_local() {
        let mut game = spider_room();
        game.advance_bookcase_spider(2).unwrap();
        crate::room::restore_broom(&game.data, &mut game.screen).unwrap();
        set_word(&mut game.data, 0x327d, 272);
        game.data[0x327f] = 8;
        set_word(&mut game.data, 0x3284, pixel_offset(272, 8));
        crate::room::draw_broom(&mut game.data, &mut game.screen).unwrap();
        game.compose_actors().unwrap();
        let publication = game.motion_publication().unwrap();
        assert!(publication.native_composition_matches());
        let ids: Vec<_> = publication.layers.iter().map(|l| l.id).collect();
        assert_eq!(&ids[..2], &[41, 40]);
        // Preserve a later scenery overwrite rather than resurrecting artwork.
        game.screen
            .copy_rect(pixel_offset(276, 8), Dimensions(0x0101), &[0; 2])
            .unwrap();
        game.compose_actors().unwrap();
        let publication = game.motion_publication().unwrap();
        assert!(publication.native_composition_matches());
        assert!(!publication.layers.iter().any(|l| l.id == 41));
    }

    #[test]
    fn blocked_broom_stops_prediction_even_if_its_direction_bytes_remain_set() {
        let mut game = room(1);
        game.player.x = 48;
        game.player.y = 80;
        game.player.vertical_coordinate = 130;
        let update = game.advance_room_broom(1).unwrap().unwrap();
        assert!(update.contact && !update.drawn);
        assert_eq!(game.data[0x3280], 1);
        assert_eq!(game.motion_broom_layer().unwrap().stride, [0.0; 2]);
    }
    #[test]
    fn hole_transfers_and_drinking_do_not_predict_cat_travel() {
        let mut game = room(4);
        game.data[0x39e1] = 1;
        assert!(!game.motion_publication().unwrap().supported);
        let mut game = room(6);
        game.data[0x44bd] = 1;
        let publication = game.motion_publication().unwrap();
        let cat = publication.layers.iter().find(|l| l.id == 0).unwrap();
        assert!(cat.stepped && !cat.smooth_velocity);
    }
    #[test]
    fn falling_cat_limits_prediction_to_next_native_support_without_mutating_state() {
        for (x, direction, low_x, high_x) in [(184, 0xff, 180.0, 290.0), (176, 1, 8.0, 180.0)] {
            let mut game = room(3);
            game.player.x = x;
            game.player.y = 42;
            game.player.vertical_coordinate = 92;
            game.player.horizontal_direction = direction;
            game.player.horizontal_step = 4;
            game.player.vertical_direction = 1;
            game.player.vertical_step = 3;
            game.compositor.restore(&mut game.screen).unwrap();
            game.compositor
                .draw(
                    &mut game.screen,
                    pixel_offset(x, 42),
                    Dimensions(0x0101),
                    &[0; 2],
                )
                .unwrap();
            game.compose_actors().unwrap();
            let data = game.data.clone();
            let pixels = game.rgba();
            let player = format!("{:?}", game.player);
            let publication = game.motion_publication().unwrap();
            assert!(publication.native_composition_matches());
            let cat = publication.layers.iter().find(|l| l.id == 0).unwrap();
            assert_eq!(cat.bounds, Some(([low_x, 0.0], [high_x, 180.0])));
            assert!(cat.settle_stops[0]);
            assert_eq!(game.data, data);
            assert_eq!(game.rgba(), pixels);
            assert_eq!(format!("{:?}", game.player), player);
        }
    }

    #[test]
    fn indoor_cat_cadence_prior_distinguishes_idle_and_active_gate_paths() {
        let mut game = room(3);
        let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
        let prior = |game: &Alley| {
            game.motion_publication()
                .unwrap()
                .layers
                .iter()
                .find(|layer| layer.id == 0)
                .unwrap()
                .cadence_hint
        };
        assert_eq!(prior(&game), Some(tick));
        game.player.horizontal_direction = 1;
        assert_eq!(prior(&game), Some(tick * 0.5));
        game.player.horizontal_direction = 0;
        game.player.vertical_direction = 1;
        assert_eq!(prior(&game), Some(tick * 0.5));
    }
}
