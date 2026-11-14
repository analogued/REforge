// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Integer motion and steering from the naval overlay. All tables come from
//! the selected release; fixed-point remainders retain its signed-byte quirk.
use crate::assets::Result;
use crate::rng::OriginalRng;

pub fn put(ds: &mut [u8], at: usize, value: u16) {
    ds[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
pub fn signed(ds: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([ds[at], ds[at + 1]])
}
#[derive(Default)]
pub struct Clock {
    pub now: u32,
    pub stamps: [u32; 16],
}
impl Clock {
    pub fn elapsed(&self, i: usize) -> u32 {
        self.now.wrapping_sub(self.stamps[i]).min(65535)
    }
    pub fn reset(&mut self, i: usize) {
        self.stamps[i] = self.now;
    }
}
pub fn nearest_root(n: u32) -> u16 {
    let lo = n.isqrt();
    let hi = lo + 1;
    if hi * hi - n < n - lo * lo {
        hi as u16
    } else {
        lo as u16
    }
}
pub fn bearings(ds: &mut [u8]) -> Result<()> {
    let dx = signed(ds, 0xc372).wrapping_sub(signed(ds, 0xc376));
    let dy = signed(ds, 0xc374).wrapping_sub(signed(ds, 0xc378));
    let distance = nearest_root((i32::from(dx).pow(2) + i32::from(dy).pow(2)) as u32);
    put(ds, 0xc382, distance);
    let radius = if distance > 100 {
        distance / 4
    } else {
        distance
    } as i16;
    let mut best = 150000i64;
    let mut heading = 0;
    for direction in 0..16 {
        let x = signed(ds, 0x987a + direction * 2).wrapping_mul(radius) / 256;
        let y = signed(ds, 0x989a + direction * 2).wrapping_mul(radius) / 256;
        let error = i64::from(dx.wrapping_sub(x)).pow(2) + i64::from(dy.wrapping_sub(y)).pow(2);
        if error < best {
            best = error;
            heading = direction as u8;
        }
    }
    ds[0xc371] = heading;
    ds[0xc370] = heading.wrapping_sub(8) & 15;
    Ok(())
}
pub fn turn(ds: &mut [u8], clock: &mut Clock, side: usize) {
    let kind = usize::from(ds[0xc37a + side]);
    let timer = side * 8 + 3;
    if clock.elapsed(timer) > u32::from(ds[0x9828 + kind]) {
        if ds[0xc38a + side] != 0 && ds[0xc396 + side] != 0 {
            ds[0xc388 + side] = ds[0xc388 + side].wrapping_add(ds[0xc396 + side]) & 15;
            clock.reset(timer);
        }
        ds[0xc396 + side] = 0;
    }
}
pub fn speed(ds: &mut [u8], clock: &mut Clock, side: usize) {
    let kind = usize::from(ds[0xc37a + side]);
    let timer = side * 8 + 4;
    if clock.elapsed(timer) <= u32::from(ds[0x982c + kind]) {
        return;
    }
    if ds[0xc394 + side] != 0 {
        let gear = (ds[0xc38c + side] as i8).wrapping_add(ds[0xc394 + side] as i8);
        ds[0xc38c + side] = gear as u8;
        if gear > 4 {
            ds[0xc38c + side] = 4;
            ds[0xc394 + side] = 0;
        } else if gear <= 0 {
            if ds[0xc390 + side] == 6 {
                ds[0xc38c + side] = 0;
                ds[0xc394 + side] = 0;
            } else {
                ds[0xc38c + side] = ds[0xc38c + side].wrapping_add(1);
            }
        } else {
            if side == 1 {
                ds[0xc37c] = 10;
            }
            clock.reset(timer);
            ds[0xc394 + side] = 0;
        }
    }
    let gear = usize::from(ds[0xc38c + side]);
    ds[0xc38a + side] = ds[0x9818 + kind * 5 + gear];
}
/// The fraction is low-eight bits of the signed sum, negated when negative.
/// This differs from ordinary signed `% 256` and is observable on diagonals.
pub fn move_axis(ds: &mut [u8], pos: usize, frac: usize, velocity: i16) {
    let sum = velocity.wrapping_add(signed(ds, frac));
    let tail = i16::from(sum as u8);
    put(ds, frac, if sum < 0 { -tail } else { tail } as u16);
    put(ds, pos, signed(ds, pos).wrapping_add(sum / 256) as u16);
}
pub fn movement(ds: &mut [u8], clock: &mut Clock, side: usize) {
    let speed = usize::from(ds[0xc38a + side]);
    let timer = side * 8 + 2;
    if clock.elapsed(timer) <= u32::from(ds[0x93fe + speed]) {
        return;
    }
    if speed != 0 {
        ds[0xc390 + side] = (ds[0xc390 + side] + 1) % 7;
    }
    let factor = i16::from(ds[0x93f6 + speed] as i8);
    let direction = usize::from(ds[0xc388 + side]);
    for (axis, table) in [(0, 0x987a), (2, 0x989a)] {
        let velocity = signed(ds, table + direction * 2).wrapping_mul(factor);
        move_axis(
            ds,
            0xc372 + side * 4 + axis,
            0xc398 + side * 4 + axis,
            velocity,
        );
    }
    clock.reset(timer);
}
/// One original AI poll. The DOS program polls between PIT ticks; callers
/// must distinguish these polls from elapsed timer ticks when comparing RNG.
pub fn ai(ds: &mut [u8], rng: &mut OriginalRng) -> bool {
    let cautious = (ds[0x99c2 + usize::from(ds[0xc37a])].wrapping_mul(4) as i8) >= ds[0xc38f] as i8;
    let enemy_strength = i16::from(ds[0xc36e] as i8).wrapping_mul(signed(ds, 0xc37e));
    let player_strength = i16::from(ds[0xc36f] as i8).wrapping_mul(signed(ds, 0xc380));
    let outmatched = player_strength > enemy_strength;
    let direction = usize::from(ds[0xc388]);
    let mut shoot = false;
    if ds[0xc396] == 0 {
        let mut delta = ds[0xc388].wrapping_sub(ds[0xc370]) & 15;
        if delta == 8 {
            delta = rng.bounded(16) as u8;
        }
        let mut command = ds[0x93a6 + usize::from(delta)];
        if delta == 0 {
            shoot = true;
        }
        if command == 0 {
            let mut y = i16::from(ds[0x93c6 + direction] as i8)
                .wrapping_mul(signed(ds, 0xc374).wrapping_sub(signed(ds, 0xc378)));
            let mut x = i16::from(ds[0x93b6 + direction] as i8)
                .wrapping_mul(signed(ds, 0xc372).wrapping_sub(signed(ds, 0xc376)));
            if x != 0 {
                y = y.wrapping_abs();
            }
            if y != 0 {
                x = x.wrapping_abs();
            }
            if y < x {
                command = ds[0x93d6 + direction];
            }
            if y > x {
                command = ds[0x93d6 + direction].wrapping_neg();
            }
        }
        if cautious || outmatched || signed(ds, 0xc382) > 95 {
            ds[0xc396] = command;
        }
        if ds[0xc393] != 0 {
            ds[0xc396] = (rng.bounded(3) as u8).wrapping_sub(1);
        }
    }
    let limit = if cautious { 3 } else { 2 };
    ds[0xc394] = if (ds[0xc38c] as i8) > limit {
        (rng.bounded(3) as u8).wrapping_sub(1)
    } else {
        rng.bounded(2) as u8
    };
    shoot
}

/// Scroll both ships' coordinate system at the edge. The crossed fraction
/// updates are intentional: they are present in D242 of the selected release.
pub fn bounds(ds: &mut [u8]) -> u16 {
    let mut flags = 0;
    for axis in [2, 0] {
        let maximum = if axis == 2 { 170 } else { 304 };
        let mut correction = [0i16; 2];
        for (side, c) in correction.iter_mut().enumerate() {
            let p = signed(ds, 0xc372 + side * 4 + axis);
            *c = if p < 15 {
                15 - p
            } else if p > maximum {
                maximum - p
            } else {
                0
            };
        }
        let [enemy, roman] = correction;
        if enemy != 0 && roman != 0 {
            if (enemy ^ roman) >= 0 {
                if enemy < roman {
                    correction[0] = 0;
                } else {
                    correction[1] = 0;
                }
            } else {
                for (side, c) in correction.iter().enumerate() {
                    let table = if *c < 0 { 0x9928 } else { 0x992a };
                    let direction = i16::from(ds[table + usize::from(axis == 2)] as i8);
                    let delta = (direction - i16::from(ds[0xc388 + side])).abs();
                    if !(4..=12).contains(&delta) {
                        flags |= u16::from(ds[0x9926 + side]);
                    }
                }
            }
        }
        for (side, c) in correction.iter().enumerate() {
            let at = 0xc372 + side * 4 + axis;
            put(ds, at, signed(ds, at).wrapping_add(*c) as u16);
            let frac = 0xc398 + (side ^ 1) * 4 + axis;
            put(
                ds,
                frac,
                signed(ds, frac).wrapping_add(c.wrapping_mul(256)) as u16,
            );
        }
        let shift = correction[0].wrapping_add(correction[1]).wrapping_mul(256);
        for at in [0xc3e0, 0xc3e4, 0xc3e8, 0xc3ec] {
            put(
                ds,
                at + axis,
                signed(ds, at + axis).wrapping_add(shift) as u16,
            );
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fraction_matches_signed_low_byte_rule() {
        let mut ds = vec![0; 4];
        move_axis(&mut ds, 0, 2, -98);
        assert_eq!((signed(&ds, 0), signed(&ds, 2)), (0, -158));
        move_axis(&mut ds, 0, 2, -98);
        assert_eq!((signed(&ds, 0), signed(&ds, 2)), (-1, 0));
    }
    #[test]
    fn integer_distance_rounds_to_nearest() {
        for n in 0..140000 {
            let r = i64::from(nearest_root(n));
            let n = i64::from(n);
            assert!((r * r - n).abs() <= ((r + 1) * (r + 1) - n).abs());
            if r != 0 {
                assert!((r * r - n).abs() <= ((r - 1) * (r - 1) - n).abs());
            }
        }
    }
}
