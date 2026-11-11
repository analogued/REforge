// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Guarded Hearts layers; native draw receipts own rival visibility.
use super::*;
use crate::hearts::{HeartStep, Work};

impl Alley {
    pub(in crate::alley) fn remember_motion_hearts(
        &mut self,
        step: HeartStep,
    ) -> Result<(), String> {
        if step.accepted || step.drew {
            self.motion_order = self.motion_order.wrapping_add(1);
        }
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::HeartsRivals) {
            self.motion_hearts_cats[..6].fill(None);
        }
        match step.work {
            Some(Work::CatDraw {
                consumed,
                respawned,
                moving,
                ..
            }) => {
                let index = usize::from(word(&self.data, 0x45b6));
                if index < 7 {
                    self.motion_hearts_cats[index] = if consumed {
                        None
                    } else {
                        Some(self.motion_hearts_cat_layer(index, respawned, moving)?)
                    };
                }
            }
            Some(Work::CatArrow { .. }) => {
                let index = usize::from(word(&self.data, 0x45b6));
                if let Some(Some(layer)) = self.motion_hearts_cats.get_mut(index) {
                    layer.observation = self.motion_order;
                    layer.stride = [0.0; 2];
                }
            }
            Some(Work::CatContact { .. } | Work::CatHidden) => {
                let index = usize::from(word(&self.data, 0x45b6));
                if index < 7 {
                    self.motion_hearts_cats[index] = None;
                }
            }
            Some(Work::Place { consumed: true, .. }) => {
                // Placement can erase several eaters, including the goal cat
                // whose hide timer remains zero. Keep their native pixels until
                // individual fresh draw receipts make them separable again.
                self.motion_hearts_cats.fill(None);
            }
            Some(Work::ArrowDraw { spawned, .. }) => {
                self.motion_hearts_arrow_step = self.motion_order;
                if spawned {
                    self.motion_hearts_arrow_epoch = self.motion_order;
                }
            }
            Some(Work::ArrowContact { .. } | Work::ArrowRetire { .. }) => {
                self.motion_hearts_arrow_step = self.motion_order;
            }
            _ => {}
        }
        Ok(())
    }

    fn motion_hearts_cat_layer(
        &self,
        index: usize,
        respawned: bool,
        moving: bool,
    ) -> Result<Layer, String> {
        let address = usize::from(word(&self.data, 0x45a8 + index * 2));
        let record = self
            .data
            .get(address..address + 12)
            .ok_or("Hearts rival record is truncated")?;
        let direction = record[2];
        let source = if direction == 0 {
            0x4500
        } else {
            let phase = if record[11] != 0 {
                (word(record, 9) & 2) + 12
            } else {
                word(record, 9)
            };
            word(
                &self.data,
                0x4a60 + usize::from(phase + if direction == 0xff { 16 } else { 0 }),
            )
        };
        let mut layer = Layer::new(
            48 + index as u8,
            word(record, 4),
            Dimensions(0x0c03),
            sprite_bytes(&self.data, source, Dimensions(0x0c03))?.to_vec(),
            VisualBlend::Or,
        );
        let x = f64::from(word(record, 0));
        let y = f64::from(record[3]);
        layer.position = [x, y];
        // Native logical endpoints 37/266 rasterize to sprite origins 36/264.
        // Logical subpixel placement and walking registration must not extend
        // a picture beyond those endpoints, including its inward-facing turn.
        layer.paint_x_bounds = Some([36.0, 264.0]);
        if direction != 0 && record[11] == 0 {
            layer.pose_offset = crate::motion::hearts_walk::offset(&self.data, &layer.pixels)?;
        }
        layer.observation = self.motion_order;
        layer.epoch = if respawned {
            self.motion_order
        } else {
            self.motion_hearts_cats[index]
                .as_ref()
                .map_or(self.motion_order, |l| l.epoch)
        };
        layer.smooth_velocity = true;
        layer.cadenced = true;
        layer.abrupt_turns = true;
        layer.turn_direction = if record[11] == 0 {
            match direction {
                1 => 1,
                0xff => -1,
                _ => 0,
            }
        } else {
            0
        };
        layer.coast_stops = true;
        layer.settle_stops[0] = true;
        let step = if record[11] != 0 { 4.0 } else { 8.0 };
        layer.stride = [
            match direction {
                1 => (x + step).min(266.0) - x,
                0xff => (x - step).max(37.0) - x,
                _ => 0.0,
            },
            0.0,
        ];
        if !moving {
            layer.stride = [0.0; 2];
        }
        if moving {
            let (next, _) = self.motion_hearts_next_player()?;
            if self.motion_hearts_rival_near_player(layer.position, &next) {
                // A nearby player's next update can reach this retained image
                // before the rival's next turn. Avoid forecasting the rival
                // away from that potential contact; actual AI still decides it.
                layer.stride[0] = 0.0;
            }
        }
        if matches!(x, 37.0 | 266.0)
            && !respawned
            && self.motion_hearts_cats[index]
                .as_ref()
                .is_some_and(|previous| previous.position == layer.position)
        {
            // Boundary clamping reverses the stored facing. Repeated pursuit
            // can clamp here again instead of taking that apparent inward step.
            // An unchanged accepted position is stronger evidence than facing.
            layer.stride[0] = 0.0;
        }
        // Keep uncertain walking prediction within one native step of its
        // latest anchor. A wrong forecast must not accumulate across AI turns.
        layer.bounds = Some(([(x - step).max(36.0), y], [(x + step).min(266.0), y]));
        // An unpredicted random stop/turn may require a correction in either
        // direction. Ease it instead of rushing a sitting sprite to its anchor.
        layer.stop_response = if direction == 0 { 0.075 } else { 0.025 };
        layer.cadence_hint =
            Some(3.0 * crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64);
        Ok(layer)
    }

    pub(super) fn motion_hearts_layers(
        &self,
        background: &mut Framebuffer,
        layers: &mut Vec<Layer>,
    ) -> Result<(), String> {
        let mut retained_arrow = None;
        let arrow = if self.data[0x70f2] != 0 && self.data[0x70f7] == 0 {
            let dimensions = Dimensions(0x0802);
            let mut source = (word(&self.data, 0x70f0) & 0x01e0).wrapping_add(0x6f30);
            if self.data[0x70f6] != 0xff {
                source += 0xc0;
            }
            let mut layer = Layer::new(
                42,
                word(&self.data, 0x70f8),
                dimensions,
                sprite_bytes(&self.data, source, dimensions)?.to_vec(),
                VisualBlend::Zero,
            );
            let saved = &self.data[0x70cc..0x70ec];
            let expected: Vec<_> = saved
                .iter()
                .zip(&layer.pixels)
                .map(|(old, sprite)| {
                    let mask = (sprite | (sprite >> 1)) & 0x55;
                    (old & !(mask | (mask << 1))) | sprite
                })
                .collect();
            if background.read_rect(word(&self.data, 0x70f8), dimensions)? == expected {
                background.copy_rect(word(&self.data, 0x70f8), dimensions, saved)?;
                let x = f64::from(word(&self.data, 0x70f3));
                let y = f64::from(self.data[0x70f5]);
                layer.position = [x, y];
                layer.observation = self.motion_hearts_arrow_step;
                layer.epoch = self.motion_hearts_arrow_epoch;
                layer.smooth_velocity = true;
                layer.cadenced = true;
                layer.abrupt_turns = true;
                layer.coast_stops = true;
                layer.stride = [
                    (x + if self.data[0x70f6] == 1 { 5.0 } else { -5.0 }).clamp(0.0, 299.0) - x,
                    (y + 2.0).min(191.0) - y,
                ];
                layer.bounds = Some(([0.0, 0.0], [299.0, 191.0]));
                layer.cadence_hint =
                    Some(crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64);
                Some(layer)
            } else {
                retained_arrow = Some(layer.native);
                None
            }
        } else {
            None
        };
        for layer in self.motion_hearts_cats.iter().flatten() {
            // If an arrow cannot be separated, preserve its overlap group too.
            // Coincident arrow/cat pixels can satisfy the cat-only guard; erasing
            // that rectangle would otherwise punch a hole in the baked arrow.
            if retained_arrow.is_some_and(|at| {
                crate::collision::overlaps(
                    (layer.native[0] as u16, layer.native[1] as u8, 24, 12),
                    (at[0] as u16, at[1] as u8, 16, 8),
                )
            }) {
                continue;
            }
            let offset = pixel_offset(layer.native[0] as u16, layer.native[1] as u8);
            let expected: Vec<_> = layer.pixels.iter().map(|p| p | 0x55).collect();
            if background.read_rect(offset, layer.dimensions)? == expected {
                background.copy_rect(offset, layer.dimensions, &[0x55; 72])?;
                layers.push(layer.clone());
            }
        }
        if let Some(arrow) = arrow {
            layers.push(arrow);
        }
        Ok(())
    }

    pub(super) fn motion_hearts_next_player(&self) -> Result<(Player, UpdateOutcome), String> {
        let mut next = self.player.clone();
        let outcome = next.update_with_projectiles(self.requested_controls(), &self.data, false)?;
        Ok((next, outcome))
    }

    fn motion_hearts_rival_near_player(&self, position: [f64; 2], next: &Player) -> bool {
        let rival = (position[0] as u16, position[1] as u8, 24, 12);
        [&self.player, next]
            .iter()
            .any(|player| crate::collision::overlaps((player.x, player.y, 24, 14), rival))
    }

    pub(super) fn motion_hearts_player_near_rival(&self, next: &Player) -> bool {
        self.motion_hearts_cats
            .iter()
            .flatten()
            .any(|rival| self.motion_hearts_rival_near_player(rival.position, next))
    }

    pub(super) fn motion_hearts_player(&self, layer: &mut Layer) -> Result<bool, String> {
        let sprite = self.player.motion_airborne_sprite(&self.data);
        let dimensions = Dimensions(sprite.dimensions);
        if dimensions.width_bytes() != layer.dimensions.width_bytes() {
            return Ok(false);
        }
        let top = i32::from(self.player.vertical_coordinate) - 50;
        let hidden = (-top).max(0) as usize;
        let bottom = usize::from(self.player.y.saturating_sub(187));
        if hidden + bottom >= dimensions.rows()
            || dimensions.rows() - hidden - bottom != layer.dimensions.rows()
        {
            return Ok(false);
        }
        let full = sprite_bytes(&self.data, sprite.source, dimensions)?;
        let start = hidden * dimensions.width_bytes();
        if full.get(start..start + layer.pixels.len()) != Some(layer.pixels.as_slice()) {
            return Ok(false);
        }
        layer.native[1] = top;
        layer.dimensions = dimensions;
        layer.pixels = full.to_vec();
        layer.clip_bottom = 187 + dimensions.rows() as i32;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
