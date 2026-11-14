// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Flagship hulls, projectiles, ramming and marines. Resource-defined collision
//! circles remain separate from the rendered sprites, as in the DOS overlay.
use crate::assets::{Assets, Result};
use crate::naval_motion::{Clock, move_axis, put, signed};
use crate::rng::OriginalRng;

pub struct Geometry {
    collision: Vec<u8>,
    pub bounds: [(i16, i16, i16, i16); 2],
    hulls: [[(i16, i16, i16, i16); 16]; 3],
}
fn square(n: i16) -> u16 {
    (n as u16).wrapping_mul(n as u16)
}
fn touches(a: (i16, i16, i16), b: (i16, i16, i16)) -> bool {
    square(a.0.wrapping_sub(b.0)).wrapping_add(square(a.1.wrapping_sub(b.1)))
        <= square(a.2.wrapping_add(b.2))
}
impl Geometry {
    pub fn new(assets: &Assets) -> Result<Self> {
        let collision = assets.resource("MNAVAL", "COLLIDE.LZW")?;
        if collision.len() != 816 {
            return Err("Invalid naval collision tables".into());
        }
        let mut hulls = [[(0, 0, 0, 0); 16]; 3];
        for (kind, stem) in ["SML", "MED", "LRG"].into_iter().enumerate() {
            let sprites = assets.sprites("MNAVAL", stem)?;
            if sprites.len() != 16 {
                return Err("Invalid naval hull directions".into());
            }
            for (direction, sprite) in sprites.into_iter().enumerate() {
                hulls[kind][direction] = (
                    sprite.origin_x,
                    sprite.origin_y,
                    sprite.width as i16,
                    sprite.height as i16,
                );
            }
        }
        Ok(Self {
            collision,
            hulls,
            bounds: [(0, 0, 0, 0); 2],
        })
    }
    pub fn update(&mut self, ds: &mut [u8]) -> Result<()> {
        for side in 0..2 {
            let direction = usize::from(ds[0xc388 + side]);
            let hull = self.hulls[usize::from(ds[0xc37a + side])][direction];
            let x = signed(ds, 0xc372 + side * 4).wrapping_sub(hull.0);
            let y = signed(ds, 0xc374 + side * 4).wrapping_sub(hull.1);
            self.bounds[side] = (
                x,
                y,
                x.wrapping_add(hull.2).wrapping_sub(1),
                y.wrapping_add(hull.3).wrapping_sub(1),
            );
            for (axis, v) in [x, y, self.bounds[side].2, self.bounds[side].3]
                .into_iter()
                .enumerate()
            {
                put(ds, 0xc3f0 + side * 8 + axis * 2, v as u16);
            }
        }
        let order = usize::from(ds[0xc384]);
        if !self.point(ds, order ^ 1, self.tip(ds, order))
            && self.point(ds, order, self.tip(ds, order ^ 1))
        {
            put(ds, 0xc384, (order ^ 1) as u16);
        }
        Ok(())
    }
    fn tip(&self, ds: &[u8], side: usize) -> (i16, i16) {
        let n = usize::from(ds[0xc37a + side]) * 16 + usize::from(ds[0xc388 + side]);
        (
            signed(ds, 0xc372 + side * 4).wrapping_add(i16::from(self.collision[720 + n] as i8)),
            signed(ds, 0xc374 + side * 4).wrapping_add(i16::from(self.collision[768 + n] as i8)),
        )
    }
    fn circle(&self, ds: &[u8], side: usize, n: usize) -> (i16, i16, i16) {
        let i = usize::from(ds[0x98e2 + usize::from(ds[0xc388 + side])])
            + usize::from(ds[0x98f2 + usize::from(ds[0xc37a + side])])
            + n;
        (
            signed(ds, 0xc372 + side * 4).wrapping_add(i16::from(self.collision[i] as i8)),
            signed(ds, 0xc374 + side * 4).wrapping_add(i16::from(self.collision[240 + i] as i8)),
            i16::from(self.collision[480 + i] as i8),
        )
    }
    pub fn point(&self, ds: &[u8], side: usize, p: (i16, i16)) -> bool {
        let b = self.bounds[side];
        p.0 >= b.0
            && p.0 <= b.2
            && p.1 >= b.1
            && p.1 <= b.3
            && (0..5).any(|n| touches(self.circle(ds, side, n), (p.0, p.1, 0)))
    }
    fn overlaps(&self, ds: &[u8]) -> bool {
        let (a, b) = (self.bounds[0], self.bounds[1]);
        a.0 <= b.2
            && b.0 <= a.2
            && a.1 <= b.3
            && b.1 <= a.3
            && (0..5).any(|i| (0..5).any(|j| touches(self.circle(ds, 0, i), self.circle(ds, 1, j))))
    }
    pub fn ram(&self, ds: &mut [u8], rng: &mut OriginalRng) -> u16 {
        if !self.overlaps(ds) {
            return 0;
        }
        let side = usize::from(ds[0xc384]);
        let other = side ^ 1;
        let delta =
            (i16::from(ds[0xc388 + side]) - i16::from(ds[0xc388 + other])).unsigned_abs() as u8;
        let kind = usize::from(ds[0xc37a + side])
            + if side == 1 {
                usize::from(ds[0x491]) * 3
            } else {
                6
            };
        let mut damage = (rng.bounded(16) as u8)
            .wrapping_add(ds[0x99bc + kind].wrapping_mul(ds[0xc38c + side]))
            .wrapping_sub(8);
        if delta <= 2 || delta >= 14 {
            damage >>= 1;
        }
        ds[0xc38e + other] = ds[0xc38e + other].wrapping_sub(damage);
        let loss = rng.bounded(u16::from(damage));
        put(
            ds,
            0xc37e + other * 2,
            subtract(signed(ds, 0xc37e + other * 2) as u16, loss),
        );
        let mut flags = u16::from(ds[0x95d2 + side]);
        if (6..=10).contains(&delta) {
            ds[0xc38e + side] = ds[0xc38e + side].wrapping_sub(damage);
            let loss = rng.bounded(u16::from(damage));
            put(
                ds,
                0xc37e + side * 2,
                subtract(signed(ds, 0xc37e + side * 2) as u16, loss),
            );
            flags |= u16::from(ds[0x95d2 + other]);
        }
        flags
    }
    pub fn arrow(&self, ds: &mut [u8], clock: &mut Clock, side: usize, rng: &mut OriginalRng) {
        let timer = side * 8 + 1;
        if ds[0xc392 + side] == 0 || clock.elapsed(timer) <= 6 {
            return;
        }
        if ds[0xc392 + side] == 1 {
            ds[0x98be + side] = ds[0x98be + side].wrapping_add(1);
            if ds[0x98be + side] as i8 > 57 {
                ds[0xc392 + side] = 0;
            }
        } else {
            ds[0x98be + side] ^= 16;
            let left = signed(ds, 0xc3c8 + side * 2).wrapping_sub(15);
            put(ds, 0xc3c8 + side * 2, left as u16);
            if left > 0 {
                projectile(ds, side, 15, false);
                if !in_sea(ds, 0xc3d0 + side * 4) {
                    ds[0xc392 + side] = 0;
                }
            } else {
                let other = side ^ 1;
                let mut hit = false;
                for _ in 0..left.wrapping_add(15) as u16 {
                    projectile(ds, side, 1, false);
                    if self.point(
                        ds,
                        other,
                        (signed(ds, 0xc3d0 + side * 4), signed(ds, 0xc3d2 + side * 4)),
                    ) {
                        hit = true;
                        break;
                    }
                }
                if hit {
                    let damage =
                        ds[if other == 0 { 0x98c0 } else { 0x98c4 } + usize::from(ds[0x491])];
                    ds[0xc38e + other] = ds[0xc38e + other].wrapping_sub(damage);
                    let loss = rng.bounded(u16::from(damage) * 2);
                    put(
                        ds,
                        0xc37e + other * 2,
                        subtract(signed(ds, 0xc37e + other * 2) as u16, loss),
                    );
                    ds[0xc3a0 + other] = (rng.bounded(50) as u8) + 10;
                    ds[0xc392 + side] = 0;
                } else {
                    ds[0xc392 + side] = 1;
                    ds[0x98be + side] = 51;
                }
            }
        }
        clock.reset(timer);
    }
    pub fn fire(&self, ds: &mut [u8], clock: &mut Clock, side: usize, rng: &mut OriginalRng) {
        let timer = side * 8 + 1;
        if ds[0xc3ba + side] == 0 || clock.elapsed(timer) <= 8 {
            return;
        }
        if signed(ds, 0xc3c4 + side * 2) <= 0 {
            ds[0xc3ba + side] = 0;
        } else {
            put(
                ds,
                0xc3c4 + side * 2,
                signed(ds, 0xc3c4 + side * 2).wrapping_sub(6) as u16,
            );
            projectile(ds, side, 6, true);
            let other = side ^ 1;
            if signed(ds, 0xc3c4 + side * 2) <= 0 && self.fire_hit(ds, other) {
                let chance = ds[if other == 0 { 0x98c8 } else { 0x98cc } + usize::from(ds[0x491])];
                let loss = (0..10)
                    .filter(|_| rng.bounded(100) < u16::from(chance))
                    .count() as u16;
                put(
                    ds,
                    0xc37e + other * 2,
                    subtract(signed(ds, 0xc37e + other * 2) as u16, loss),
                );
            }
            if !in_sea(ds, 0xc3d8 + side * 4) {
                ds[0xc3ba + side] = 0;
            }
        }
        clock.reset(timer);
    }
    fn fire_hit(&self, ds: &[u8], other: usize) -> bool {
        let side = other ^ 1;
        let dir = usize::from(ds[0xc3ce + side]);
        let circle = (
            signed(ds, 0xc3d8 + side * 4).wrapping_add(i16::from(ds[0x98f6 + dir] as i8)),
            signed(ds, 0xc3da + side * 4).wrapping_add(i16::from(ds[0x9906 + dir] as i8)),
            i16::from(ds[0x9916 + dir]),
        );
        (0..5).any(|n| touches(circle, self.circle(ds, other, n)))
    }
    pub fn board(&self, ds: &mut [u8], side: usize) -> u16 {
        if ds[0xc3aa + side] == 0 {
            return 0;
        }
        ds[0xc3aa + side] -= 1;
        let dir = usize::from(ds[0xc388 + side]);
        let delta = ds[0xc388 + side].wrapping_sub(ds[0xc370 + side]) & 15;
        let base = usize::from(ds[0xc37a + side]) * 16 + dir;
        let (table, xs, ys) = if delta > 0 && delta < 8 {
            (0x95d4, 0x95f4, 0x9624)
        } else if delta > 8 {
            (0x95e4, 0x9654, 0x9684)
        } else {
            ds[0xc3aa + side] = 0;
            return 0;
        };
        let n = usize::from(ds[table + dir]);
        let x = signed(ds, 0xc372 + side * 4).wrapping_add(i16::from(ds[xs + base] as i8));
        let y = signed(ds, 0xc374 + side * 4).wrapping_add(i16::from(ds[ys + base] as i8));
        if self.point(
            ds,
            side ^ 1,
            (
                x.wrapping_add(i16::from(ds[0x96b4 + n] as i8)),
                y.wrapping_add(i16::from(ds[0x96bc + n] as i8)),
            ),
        ) {
            u16::from(ds[0x95d2 + side])
        } else {
            0
        }
    }
}
pub fn subtract(a: u16, b: u16) -> u16 {
    (a.wrapping_sub(b) as i16).max(0) as u16
}
fn in_sea(ds: &[u8], at: usize) -> bool {
    signed(ds, at) > 0 && signed(ds, at) < 319 && signed(ds, at + 2) > 0 && signed(ds, at + 2) < 185
}
fn projectile(ds: &mut [u8], side: usize, speed: i16, fire: bool) {
    let (direction, pos, frac) = if fire {
        (0xc3ce, 0xc3d8, 0xc3e8)
    } else {
        (0xc3cc, 0xc3d0, 0xc3e0)
    };
    let dir = usize::from(ds[direction + side]);
    for (axis, table) in [(0, 0x987a), (2, 0x989a)] {
        let v = signed(ds, table + dir * 2).wrapping_mul(speed);
        move_axis(ds, pos + side * 4 + axis, frac + side * 4 + axis, v);
    }
}
pub fn shoot(ds: &mut [u8], clock: &mut Clock, side: usize) {
    let timer = side * 8 + 5;
    if ds[0xc392 + side] != 0 || ds[0xc3ba + side] != 0 || clock.elapsed(timer) <= 144 {
        return;
    }
    clock.reset(timer);
    ds[0xc392 + side] = 255;
    ds[0xc3cc + side] = ds[0xc388 + side];
    ds[0x98be + side] = 0;
    for axis in [0, 2] {
        put(
            ds,
            0xc3d0 + side * 4 + axis,
            signed(ds, 0xc372 + side * 4 + axis) as u16,
        );
        put(ds, 0xc3e0 + side * 4 + axis, 0);
    }
    let delta =
        (i16::from(ds[0xc388 + side]) - i16::from(ds[0xc388 + (side ^ 1)])).unsigned_abs() as usize;
    let divisor = i16::from(ds[0x98d0 + delta] as i8);
    let distance = signed(ds, 0xc382);
    let mut adjust = if divisor != 0 { distance / divisor } else { 0 };
    if (ds[0xc38a + (side ^ 1)] as i8) < 4 {
        adjust /= 2;
    }
    put(
        ds,
        0xc3c8 + side * 2,
        distance.wrapping_sub(adjust).max(90) as u16,
    );
}
pub fn start_fire(ds: &mut [u8], side: usize) {
    if ds[0xc3ba + side] != 0
        || ds[0xc392 + side] != 0
        || signed(ds, 0xc382) as u16 >= 90
        || signed(ds, 0xc37e + side * 2) == 0
    {
        return;
    }
    ds[0xc3ba + side] = 255;
    ds[0xc3ce + side] = ds[0xc370 + side];
    for axis in [0, 2] {
        put(
            ds,
            0xc3d8 + side * 4 + axis,
            signed(ds, 0xc372 + side * 4 + axis) as u16,
        );
        put(ds, 0xc3e8 + side * 4 + axis, 0);
    }
    put(
        ds,
        0xc3c4 + side * 2,
        subtract(signed(ds, 0xc382) as u16, 10).max(7),
    );
}

pub fn marine_hits(ds: &[u8], side: usize, morale: u8, rng: &mut OriginalRng) -> Result<u16> {
    let mut hits = 0;
    for _ in 0..crate::assets::word(ds, 0xc37e + side * 2)? {
        if rng.bounded(100) < u16::from(morale) && rng.bounded(100) < 25 {
            hits += 1;
        }
    }
    Ok(hits)
}
