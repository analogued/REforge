// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Pre-race presentation, keeping timer/RNG changes independent of rendering.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::games::put;
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub struct Gallery {
    background: Frame,
    sprites: Vec<Bitmap>,
    elapsed: u32,
    x: i16,
    pose: usize,
    bobbing: bool,
    draw_x: i16,
    draw_pose: usize,
    cheer: usize,
    drawn: bool,
}
impl Gallery {
    pub fn new(assets: &Assets) -> Result<Self> {
        let mut background = Frame::new();
        background.blit(&assets.bitmap("MRACE", "GALLERY1.CBM")?, 0, 0);
        // 151C expands the lower strip into the persistent C0B6 buffer using
        // palette 3, overriding the file's default palette. Restore it opaquely.
        let mut lower = assets.bitmap("MRACE", "GALLERY2.CBM")?;
        if assets.video == crate::assets::VideoMode::Vga {
            for pixel in &mut lower.pixels {
                *pixel = assets.data_segment[0x60f0 + usize::from(*pixel)];
            }
            // Expansion aliases the final packed byte, after palette translation.
            let end = lower.pixels.len();
            lower.pixels[end - 1] =
                assets.data_segment[0x60f0 + usize::from(lower.pixels[end - 2] & 15)];
        }
        lower.visible.fill(true);
        background.blit(&lower, 0, 130);
        Ok(Self {
            background,
            sprites: assets.sprites("MRACE", "RACE2")?,
            elapsed: 0,
            x: 331,
            pose: 0,
            bobbing: false,
            draw_x: 0,
            draw_pose: 0,
            cheer: 0,
            drawn: false,
        })
    }
    pub fn tick(&mut self, s: &mut Campaign, rng: &mut OriginalRng, ticks: u32) -> Result<()> {
        self.elapsed = self.elapsed.saturating_add(ticks);
        while self.elapsed >= 7 {
            self.elapsed -= 7;
            self.cheer = usize::from(word(&s.data, 0x8418)?);
            put(s, 0x8418, ((self.cheer + 1) % 5) as u16);
            // Only the lower strip is restored. Transparent torch poses leave
            // their earlier pixels in the upper image in the original.
            self.background.blit(&self.sprites[self.cheer], 292, 35);
            if !self.bobbing {
                self.x -= 6;
                self.draw_x = self.x;
                self.draw_pose = self.pose;
                self.drawn = true;
                if self.x <= 56 {
                    self.bobbing = true;
                    self.x = 62;
                    self.pose = 0;
                    // BE9E branches into the first bob before waiting/swapping
                    // the final approach image. Both torch draws still occur.
                    self.elapsed += 7;
                    continue;
                } else {
                    self.pose = (self.pose + 1) % 4;
                }
            } else {
                self.draw_x = self.x - 8;
                self.draw_pose = 4 + self.pose;
                self.drawn = true;
                if rng.bounded(25) == 0 {
                    self.pose = 1 - self.pose;
                    self.x = if self.x <= 62 { self.x + 1 } else { self.x - 1 };
                }
            }
        }
        Ok(())
    }
    pub fn render(&self) -> Frame {
        let mut f = self.background.clone();
        if self.drawn {
            f.blit(&self.sprites[5], i32::from(self.draw_x) + 50, 142);
            f.blit(
                &self.sprites[8 + self.draw_pose],
                i32::from(self.draw_x),
                130,
            );
            f.blit(
                &self.sprites[14 + (self.draw_pose & 1)],
                i32::from(self.draw_x) + 67,
                158,
            );
        }
        f
    }
}

pub struct Parade {
    gallery: Gallery,
    types: Vec<usize>,
    x: Vec<i16>,
    pose: Vec<usize>,
    active: Vec<bool>,
    champion: usize,
    saluting: bool,
    elapsed: u32,
    frames: u8,
    part: u8,
    done: bool,
    caesar: bool,
    draws: Vec<(usize, i32, i32)>,
}
impl Parade {
    pub fn new(assets: &Assets, s: &Campaign, rng: &mut OriginalRng, caesar: bool) -> Result<Self> {
        s.sound.play(if caesar { 0 } else { 2 });
        let count = if caesar { 10 } else { 4 };
        let champion = count - 1;
        let mut types = Vec::new();
        for id in 0..count {
            types.push(if caesar {
                rng.bounded(3) as usize
            } else {
                usize::from(s.data[0xc2c6 + id * 18])
            });
        }
        types[champion] = 0;
        let mut x: Vec<i16> = (0..count).map(|id| 331 + id as i16 * 90).collect();
        x[champion] += 45;
        Ok(Self {
            gallery: Gallery::new(assets)?,
            types,
            x,
            pose: vec![0; count],
            active: vec![true; count],
            champion,
            saluting: false,
            elapsed: 0,
            frames: 0,
            part: 0,
            done: false,
            caesar,
            draws: Vec::new(),
        })
    }
    fn chariot(&mut self, id: usize, kind: usize, pose: usize) {
        let x = i32::from(self.x[id]);
        self.draws.extend([
            (5 + kind, x + 50, 142),
            (8 + pose, x, 130),
            (14 + (pose & 1), x + 67, 158),
        ]);
    }
    pub fn tick(&mut self, s: &mut Campaign, rng: &mut OriginalRng, ticks: u32) -> Result<bool> {
        self.elapsed = self.elapsed.saturating_add(ticks);
        while self.elapsed >= 7 && !self.done {
            self.elapsed -= 7;
            self.draws.clear();
            let cheer = usize::from(word(&s.data, 0x8418)?);
            put(s, 0x8418, ((cheer + 1) % 5) as u16);
            self.gallery
                .background
                .blit(&self.gallery.sprites[cheer], 292, 35);
            if !self.saluting {
                for id in 0..self.types.len() {
                    if self.active[id] {
                        self.x[id] -= 6;
                        self.chariot(id, self.types[id], self.pose[id]);
                        if self.caesar && id == self.champion {
                            self.draws.push((20, i32::from(self.x[id]) + 50, 142));
                        }
                        if self.x[id] <= -90 {
                            self.active[id] = false;
                        }
                        self.pose[id] = (self.pose[id] + 1) % 4;
                    }
                }
                if self.x[self.champion] <= 56 {
                    self.saluting = true;
                    self.pose[self.champion] = 0;
                }
            } else {
                let id = self.champion;
                self.chariot(
                    id,
                    if self.part < 4 {
                        usize::from(self.part) + 11
                    } else {
                        0
                    },
                    4 + self.pose[id],
                );
                if self.caesar {
                    self.draws.push((
                        if self.part < 4 {
                            21 + usize::from(self.part)
                        } else {
                            20
                        },
                        i32::from(self.x[id]) + 50,
                        142,
                    ));
                }
                let old = self.part as usize;
                self.frames = self.frames.wrapping_add(1);
                if (self.frames as i8) > (s.data[0x8412 + old] as i8) {
                    self.part = self.part.wrapping_add(1);
                }
                if self.part <= 4 && rng.bounded(15) == 0 {
                    self.pose[id] = 1 - self.pose[id];
                    self.x[id] += if self.x[id] <= 56 { 1 } else { -1 };
                }
                if self.frames > 75 {
                    self.done = true;
                }
            }
        }
        Ok(self.done)
    }
    pub fn render(&self) -> Frame {
        let mut f = self.gallery.background.clone();
        for &(id, x, y) in &self.draws {
            f.blit(&self.gallery.sprites[id], x, y);
        }
        f
    }

    pub fn unspent_ticks(&self) -> u32 {
        if self.done { self.elapsed } else { 0 }
    }
}
