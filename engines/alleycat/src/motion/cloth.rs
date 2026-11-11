// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded history of completed native garment-band scrolls.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClothBand {
    pub offset: u16,
    pub direction: f64,
    pub position: f64,
    pub serial: u64,
    pub epoch: u64,
    pub period: f64,
    pub stride: usize,
    pub current: usize,
    pub pixels: Vec<u8>,
}

fn unpack(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|&b| (0..4).map(move |i| (b >> (6 - i * 2)) & 3))
        .collect()
}
impl ClothBand {
    pub fn advance(
        previous: Option<Self>,
        before: &[u8],
        after: &[u8],
        offset: u16,
        left: bool,
        serial: u64,
        period: f64,
    ) -> Self {
        let old = unpack(before);
        let new = unpack(after);
        let direction = if left { -1.0 } else { 1.0 };
        let previous = previous
            .filter(|p| p.offset == offset && p.direction == direction && p.matches(before));
        let epoch = previous.as_ref().map_or(serial, |p| p.epoch);
        let position = previous.as_ref().map_or(0.0, |p| p.position) + direction * 4.0;
        let old_stride = previous.as_ref().map_or(WIDTH, |p| p.stride);
        let old_pixels = previous
            .as_ref()
            .map_or(old.as_slice(), |p| p.pixels.as_slice());
        let stride = (old_stride + 4).min(WIDTH + 16);
        let mut pixels = Vec::with_capacity(stride * 16);
        for y in 0..16 {
            let row = &old_pixels[y * old_stride..(y + 1) * old_stride];
            if left {
                let discard = old_stride + 4 - stride;
                pixels.extend_from_slice(&row[discard..]);
                pixels.extend_from_slice(&new[y * WIDTH + WIDTH - 4..(y + 1) * WIDTH]);
            } else {
                pixels.extend_from_slice(&new[y * WIDTH..y * WIDTH + 4]);
                pixels.extend_from_slice(&row[..stride - 4]);
            }
        }
        Self {
            offset,
            direction,
            position,
            serial,
            epoch,
            period,
            stride,
            current: if left { stride - WIDTH } else { 0 },
            pixels,
        }
    }
    pub fn matches(&self, packed: &[u8]) -> bool {
        let pixels = unpack(packed);
        (0..16).all(|y| {
            self.pixels[y * self.stride + self.current..y * self.stride + self.current + WIDTH]
                == pixels[y * WIDTH..(y + 1) * WIDTH]
        })
    }
    pub fn layer(&self, row: usize) -> Layer {
        let mut layer = Layer::new(
            8 + row as u8,
            self.offset,
            Dimensions(0x1028),
            Vec::new(),
            Blend::Opaque,
        );
        layer.position[0] = self.position;
        layer.smooth_velocity = true;
        layer.cadenced = true;
        layer.stride = [self.direction * 4.0, 0.0];
        layer.cadence_hint = Some(self.period);
        layer.observation = self.serial;
        layer.epoch = self.epoch;
        let history = (self.stride - WIDTH) as f64;
        let end = self.position + self.direction * 4.0;
        let other = end - self.direction * history;
        layer.bounds = Some((
            [end.min(other), layer.position[1]],
            [end.max(other), layer.position[1]],
        ));
        layer.cloth = Some(self.clone());
        layer
    }
    pub fn paint(&self, dst: &mut [u8], top: usize, position: f64) {
        let source = (self.current as f64 - (position - self.position))
            .round()
            .clamp(0.0, (self.stride - WIDTH) as f64) as usize;
        for y in 0..16 {
            if top + y < HEIGHT {
                dst[(top + y) * WIDTH..(top + y + 1) * WIDTH].copy_from_slice(
                    &self.pixels[y * self.stride + source..y * self.stride + source + WIDTH],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::pixel_offset;
    fn scroll(old: &[u8], left: bool, edge: u8) -> Vec<u8> {
        let mut next = old.to_vec();
        for row in next.as_chunks_mut::<80>().0 {
            if left {
                row.copy_within(1.., 0);
                row[79] = edge;
            } else {
                row.copy_within(..79, 1);
                row[0] = edge;
            }
        }
        next
    }
    #[test]
    fn strip_history_reconstructs_both_native_endpoints_and_intermediate_pixels() {
        for left in [false, true] {
            let old: Vec<_> = (0..1280).map(|i| (i as u8).wrapping_mul(37)).collect();
            let next = scroll(&old, left, 0x1b);
            let band = ClothBand::advance(None, &old, &next, pixel_offset(0, 40), left, 1, 0.11);
            assert!(band.matches(&next));
            for phase in 0..=4 {
                let mut target = vec![0; WIDTH * HEIGHT];
                band.paint(
                    &mut target,
                    40,
                    band.position - band.direction * f64::from(4 - phase),
                );
                let decoded = unpack(if phase == 0 { &old } else { &next });
                if phase == 0 || phase == 4 {
                    assert_eq!(&target[40 * WIDTH..56 * WIDTH], decoded);
                }
                if phase == 2 {
                    let before = unpack(&old);
                    for y in 0..16 {
                        let actual = &target[(40 + y) * WIDTH..(41 + y) * WIDTH];
                        if left {
                            assert_eq!(&actual[..318], &before[y * WIDTH + 2..(y + 1) * WIDTH]);
                        } else {
                            assert_eq!(&actual[2..], &before[y * WIDTH..(y + 1) * WIDTH - 2]);
                        }
                    }
                }
            }
            let mut state = band;
            let mut current = next;
            for serial in 2..40 {
                let next = scroll(&current, left, serial as u8);
                state = ClothBand::advance(
                    Some(state),
                    &current,
                    &next,
                    pixel_offset(0, 40),
                    left,
                    serial,
                    0.11,
                );
                assert!(state.matches(&next));
                assert!(state.stride <= WIDTH + 16);
                current = next;
            }
            let epoch = state.epoch;
            current[80] ^= 0xff; // Another native helper changed this band.
            let next = scroll(&current, left, 0xaa);
            let reset = ClothBand::advance(
                Some(state),
                &current,
                &next,
                pixel_offset(0, 40),
                left,
                100,
                0.11,
            );
            assert_ne!(reset.epoch, epoch);
            assert_eq!(reset.stride, WIDTH + 4);
        }
    }
}
