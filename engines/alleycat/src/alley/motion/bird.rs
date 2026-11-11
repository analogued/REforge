// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Released bird visibility follows actual draw/restore receipts.
use super::*;
use crate::birdcage::{BirdGate, BirdStep};

impl Alley {
    pub(in crate::alley) fn remember_motion_bird(&mut self, step: BirdStep) -> Result<(), String> {
        match step.work.map(|work| work.gate) {
            Some(BirdGate::Motion) => {
                self.motion_order = self.motion_order.wrapping_add(1);
                // Motion can restore the old picture and then fail its broom
                // check. Logical coordinates/40b9 alone do not prove visibility.
                self.motion_bird = if step.drew {
                    Some(self.motion_bird_layer()?)
                } else {
                    None
                };
                self.motion_bird_broom_overlap = self
                    .motion_bird
                    .as_ref()
                    .is_some_and(|layer| self.motion_bird_overlaps_broom(layer));
            }
            Some(BirdGate::Broom) => {
                self.motion_order = self.motion_order.wrapping_add(1);
                if let Some(layer) = self.motion_bird.as_mut() {
                    // This pre-motion block leaves the actual picture intact.
                    layer.observation = self.motion_order;
                    layer.stride = [0.0; 2];
                }
            }
            Some(BirdGate::NotReleased) => {
                self.motion_bird = None;
                self.motion_bird_broom_overlap = false;
            }
            _ => {}
        }
        Ok(())
    }

    fn motion_bird_overlaps_broom(&self, bird: &Layer) -> bool {
        #[cfg(feature = "dev-tools")]
        if self.dev_options.actor_disabled(Actor::Broom) {
            return false;
        }
        if self.data[0x3286] != 0 {
            return false;
        }
        let offset = usize::from(word(&self.data, 0x3282));
        let bank = offset & 0x1fff;
        crate::collision::overlaps(
            (bird.native[0] as u16, bird.native[1] as u8, 8, 5),
            (
                ((bank % 80) * 4) as u16,
                ((bank / 80) * 2 + usize::from(offset & 0x2000 != 0)) as u8,
                16,
                30,
            ),
        )
    }

    pub(super) fn motion_bird_group_native(&self) -> bool {
        self.motion_bird.as_ref().is_some_and(|bird| {
            self.motion_bird_broom_overlap || self.motion_bird_overlaps_broom(bird)
        })
    }

    fn motion_bird_layer(&self) -> Result<Layer, String> {
        let phase = word(&self.data, 0x40be) & 6;
        let source = word(&self.data, 0x40c0 + usize::from(phase))
            + if self.data[0x40b7] == 0xff { 30 } else { 0 };
        let dimensions = Dimensions(0x0501);
        let mut layer = Layer::new(
            43,
            word(&self.data, 0x40ba),
            dimensions,
            sprite_bytes(&self.data, source, dimensions)?.to_vec(),
            VisualBlend::Zero,
        );
        let [x, y] = [
            f64::from(word(&self.data, 0x40b2)),
            f64::from(self.data[0x40b4]),
        ];
        layer.position = [x, y];
        layer.native_by_default = true;
        layer.observation = self.motion_order;
        layer.order = self.motion_order;
        layer.epoch = self
            .motion_bird
            .as_ref()
            .map_or(self.motion_order, |old| old.epoch);
        layer.cadenced = true;
        layer.smooth_velocity = true;
        layer.abrupt_turns = true;
        layer.coast_stops = true;
        layer.settle_stops = [true; 2];
        let direction = |byte| match byte {
            1 => 1.0,
            0xff => -1.0,
            _ => 0.0,
        };
        let dx = direction(self.data[0x40b7]) * 4.0;
        let dy = direction(self.data[0x40b8]) * 2.0;
        layer.stride = [
            (x + dx).clamp(0.0, 309.0) - x,
            (y + dy).clamp(48.0, 167.0) - y,
        ];
        layer.turn_direction = direction(self.data[0x40b7]) as i8;
        layer.cadence_hint =
            Some(crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64);
        // Intended one-step prediction window. The shared settling path can
        // lag beyond it as the window moves; retain this rejected velocity
        // candidate for comparison, not as a strictly bounded renderer.
        layer.bounds = Some((
            [(x - 4.0).max(0.0), (y - 2.0).max(48.0)],
            [(x + 4.0).min(309.0), (y + 2.0).min(167.0)],
        ));
        layer.paint_x_bounds = Some([0.0, 308.0]);
        Ok(layer)
    }

    pub(super) fn motion_bird_layers(
        &self,
        background: &mut Framebuffer,
        layers: &mut Vec<Layer>,
    ) -> Result<(), String> {
        let Some(layer) = &self.motion_bird else {
            return Ok(());
        };
        // Saved bird background may contain an older broom picture. Retain
        // both overlapping actors natively until a clean bird redraw, rather
        // than reveal that saved picture when an interpolated bird moves away.
        if self.motion_bird_group_native() {
            return Ok(());
        }
        let saved = &self.data[0x3f2c..0x3f36];
        let expected: Vec<_> = saved
            .iter()
            .zip(&layer.pixels)
            .map(|(old, sprite)| {
                let mask = (sprite | (sprite >> 1)) & 0x55;
                (old & !(mask | (mask << 1))) | sprite
            })
            .collect();
        let offset = pixel_offset(layer.native[0] as u16, layer.native[1] as u8);
        if background.read_rect(offset, layer.dimensions)? == expected {
            background.copy_rect(offset, layer.dimensions, saved)?;
            // A later broom was peeled first. If the bird was drawn above the
            // broom instead, that broom's earlier peel failed and remains native.
            layers.insert(0, layer.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
