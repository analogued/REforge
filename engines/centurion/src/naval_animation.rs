// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Separation, sinking and sharks after a flagship loss (D3F2, D60C).
use super::Naval;
use crate::assets::{Assets, Result, word};
use crate::graphics::Frame;
use crate::naval_motion::{put, signed};
use crate::rng::OriginalRng;
type Rect = (i16, i16, i16, i16);
fn intersects(a: Rect, b: Rect) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}
fn contains(a: Rect, x: i16, y: i16) -> bool {
    x >= a.0 && x <= a.2 && y >= a.1 && y <= a.3
}
fn sinking_rect(ds: &[u8], side: usize) -> Rect {
    let kind = usize::from(ds[0xc37a + side]);
    let at = 0x9932 + kind * 8;
    let x = signed(ds, 0xc372 + side * 4);
    let y = signed(ds, 0xc374 + side * 4);
    (
        x.wrapping_add(signed(ds, at)),
        y.wrapping_add(signed(ds, at + 2)),
        x.wrapping_add(signed(ds, at + 4)),
        y.wrapping_add(signed(ds, at + 6)).min(185),
    )
}
fn footprint(ds: &[u8], assets: &Assets, side: usize) -> Result<Rect> {
    let direction = usize::from(ds[0xc388 + side]);
    let kind = usize::from(ds[0xc37a + side]);
    let hull = assets
        .sprites("MNAVAL", ["SML", "MED", "LRG"][kind])?
        .remove(direction);
    let oar = assets
        .sprites("NAVL", if direction < 8 { "OARS1" } else { "OARS2" })?
        .remove((direction % 8) * 7 + 6);
    let x = signed(ds, 0xc372 + side * 4);
    let y = signed(ds, 0xc374 + side * 4);
    Ok((
        x - hull.origin_x.max(oar.origin_x),
        y - hull.origin_y.max(oar.origin_y),
        (x - hull.origin_x + hull.width as i16).max(x - oar.origin_x + oar.width as i16),
        (y - hull.origin_y + hull.height as i16).max(y - oar.origin_y + oar.height as i16),
    ))
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Separate,
    Sink,
}
pub(super) struct Animation {
    part: Part,
    lost: u16,
    dead: usize,
    survivor: usize,
    turn: i8,
    ticks: u32,
    frame_id: usize,
    rectangles: [Rect; 2],
    shark_x: [i16; 2],
    shark_y: [i16; 2],
    sharks: [usize; 2],
    melee: (i16, i16),
    pub frame: Frame,
}
impl Animation {
    pub fn new(scene: &mut Naval, assets: &Assets, lost: u16) -> Result<Self> {
        let mut frame = if let Some(frame) = &scene.resolution_frame {
            frame.clone()
        } else {
            scene.render(assets, None)?
        };
        frame.rect(0, 0, 320, 186, 9);
        let dead = if lost & 1 != 0 { 1 } else { 0 };
        let survivor = dead ^ 1;
        let mut a = Self {
            part: Part::Separate,
            lost,
            dead,
            survivor,
            turn: 1,
            ticks: 0,
            frame_id: 0,
            rectangles: [
                sinking_rect(&scene.view.data, 0),
                sinking_rect(&scene.view.data, 1),
            ],
            shark_x: [0; 2],
            shark_y: [0; 2],
            sharks: [0; 2],
            melee: (
                signed(&scene.view.data, 0xc372 + dead * 4),
                signed(&scene.view.data, 0xc374 + dead * 4),
            ),
            frame,
        };
        if lost == 3 {
            a.start_sink(scene, assets)?;
        } else {
            let ds = scene.view.data.as_mut_slice();
            ds[0xc390 + survivor] = 6;
            ds[0xc390 + dead] = 3;
            if ds[0xc388 + dead] > 4 && ds[0xc388 + dead] < 12 {
                a.turn = -1;
            }
            if intersects(a.rectangles[dead], footprint(ds, assets, survivor)?) {
                a.melee = (
                    ((i32::from(signed(ds, 0xc372)) + i32::from(signed(ds, 0xc376))) / 2) as i16,
                    ((i32::from(signed(ds, 0xc374)) + i32::from(signed(ds, 0xc378))) / 2) as i16,
                );
            }
        }
        Ok(a)
    }
    fn start_sink(&mut self, scene: &mut Naval, assets: &Assets) -> Result<()> {
        self.part = Part::Sink;
        self.frame_id = 0;
        self.ticks = 0;
        self.frame.rect(0, 0, 320, 186, 9);
        if self.lost != 3 {
            scene.ship(&mut self.frame, assets, self.survivor)?;
        }
        self.rectangles = [
            sinking_rect(&scene.view.data, 0),
            sinking_rect(&scene.view.data, 1),
        ];
        for side in 0..2 {
            self.shark_x[side] = self.rectangles[side].0;
            self.shark_y[side] = if self.rectangles[side].3 + 4 < 185 {
                self.rectangles[side].3 + 4
            } else {
                self.rectangles[side].1 - 2
            };
            self.sharks[side] = side ^ 1;
        }
        for side in 0..2 {
            if self.lost & if side == 0 { 2 } else { 1 } != 0 {
                let kind = usize::from(scene.view.data[0xc37a + side]);
                self.frame.blit(
                    &assets.sprites("MNAVAL", ["SSINK", "MSINK", "LSINK"][kind])?[0],
                    i32::from(signed(&scene.view.data, 0xc372 + side * 4)),
                    i32::from(signed(&scene.view.data, 0xc374 + side * 4)),
                );
            }
        }
        Ok(())
    }
    pub fn advance(
        &mut self,
        scene: &mut Naval,
        assets: &Assets,
        rng: &mut OriginalRng,
    ) -> Result<bool> {
        // The loops present one frame then wait until timer 0 reaches 14.
        self.ticks += 1;
        if self.ticks < 14 {
            return Ok(false);
        }
        self.ticks = 0;
        if self.part == Part::Separate {
            let ds = scene.view.data.as_mut_slice();
            let overlap = intersects(
                self.rectangles[self.dead],
                footprint(ds, assets, self.survivor)?,
            );
            if overlap {
                let direction = usize::from(ds[0xc388 + self.survivor]);
                for (axis, table) in [(0, 0x9958), (2, 0x9968)] {
                    let at = 0xc372 + self.survivor * 4 + axis;
                    put(
                        ds,
                        at,
                        signed(ds, at).wrapping_add(i16::from(ds[table + direction] as i8)) as u16,
                    );
                }
                let phase = ds[0xc390 + self.survivor] as i8 - 1;
                ds[0xc390 + self.survivor] = if phase < 0 {
                    6
                } else if phase == 5 {
                    3
                } else {
                    phase as u8
                };
            }
            let done = ds[0xc388 + self.dead] == 4 && !overlap;
            if ds[0xc388 + self.dead] != 4 {
                ds[0xc388 + self.dead] = ds[0xc388 + self.dead].wrapping_add(self.turn as u8) & 15;
            }
            self.frame.rect(0, 0, 320, 186, 9);
            for side in [self.survivor, self.dead] {
                scene.burn_smoke(side);
                scene.ship(&mut self.frame, assets, side)?;
            }
            let frame = usize::from(rng.bounded(3));
            self.frame.blit(
                &assets.sprites("MNAVAL", "SMOK")?[frame],
                i32::from(self.melee.0),
                i32::from(self.melee.1),
            );
            if done {
                scene.view.data[0xc390 + self.dead] = 6;
                self.start_sink(scene, assets)?;
            }
            return Ok(false);
        }
        let mut done = true;
        for side in [1, 0] {
            let flag = if side == 0 { 2 } else { 1 };
            if self.lost & flag == 0 {
                continue;
            }
            let kind = usize::from(scene.view.data[0xc37a + side]);
            if self.frame_id < usize::from(word(&scene.view.data, 0x994a + kind * 2)?) {
                let r = self.rectangles[side];
                self.frame.rect(
                    i32::from(r.0),
                    i32::from(r.1),
                    i32::from(r.2 - r.0 + 1),
                    i32::from(r.3 - r.1 + 1),
                    9,
                );
                let sprites = assets.sprites("MNAVAL", ["SSINK", "MSINK", "LSINK"][kind])?;
                self.frame.blit(
                    sprites.get(self.frame_id).ok_or("Missing sinking frame")?,
                    i32::from(signed(&scene.view.data, 0xc372 + side * 4)),
                    i32::from(signed(&scene.view.data, 0xc374 + side * 4)),
                );
                done = false;
            }
            if !contains(
                self.rectangles[side ^ 1],
                self.shark_x[side],
                self.shark_y[side],
            ) {
                self.frame.rect(
                    i32::from(self.shark_x[side]) - 5,
                    i32::from(self.shark_y[side]) - 3,
                    10,
                    6,
                    9,
                );
                self.shark_x[side] = self.shark_x[side].wrapping_add(1);
                self.frame.blit(
                    &assets.sprites("MNAVAL", "SHARK")?[self.sharks[side]],
                    i32::from(self.shark_x[side]),
                    i32::from(self.shark_y[side]),
                );
                self.sharks[side] += 1;
                if self.sharks[side] == 5 {
                    if rng.bounded(10) != 0 {
                        self.sharks[side] = 2;
                    }
                } else if self.sharks[side] == 8 {
                    self.sharks[side] = 0;
                }
            }
        }
        self.frame_id += 1;
        Ok(done)
    }
}
impl Naval {
    fn burn_smoke(&mut self, side: usize) {
        let ds = &mut self.view.data;
        self.smoke_visible[side] = ds[0xc3a0 + side] != 0;
        if self.smoke_visible[side] {
            ds[0xc3a0 + side] -= 1;
            if ds[0xc3a0 + side] & 15 == 0 {
                ds[0xc38e + side] = ds[0xc38e + side].wrapping_sub(1);
            }
            ds[0x98e0 + side] = (ds[0x98e0 + side] + 1) % 3;
        }
    }
}
