// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player crash presentation (D738), which suspends race physics.
use crate::assets::Result;
use crate::race_motion::{Draw, Motion, w};
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub struct Crash {
    stage: u8,
    frame: i16,
    elapsed: u32,
    variant: bool,
    landing: (i16, i16),
    horse: (i16, i16),
    velocity: i16,
    flight: i16,
    debris: (i16, i16),
    done: bool,
}
impl Crash {
    pub fn variant(&self) -> bool {
        self.variant
    }
    pub fn new(s: &Campaign, rng: &mut OriginalRng) -> Self {
        let variant = rng.bounded(2) == 0;
        let horse = (w(s, 0xc340) as i16, w(s, 0xc342) as i16);
        let landing = (horse.0.wrapping_add(9), horse.1.wrapping_add(29));
        let flight = (170 - landing.1) / 2 + 15;
        Self {
            stage: 0,
            frame: 0,
            elapsed: 0,
            variant,
            landing,
            horse,
            velocity: i16::from(s.data[0xc2fe]) * 8 / 75,
            flight,
            debris: (landing.0 - flight * 2 - 19, landing.1 + (flight + 6) * 2),
            done: false,
        }
    }
    fn blood(&self, m: &mut Motion, x: i16, y: i16) {
        m.med.push(Draw {
            sprite: 29,
            x: i32::from(x),
            y: i32::from(y),
        });
    }
    fn last_phase(&mut self) {
        self.debris.0 += 27;
        self.debris.1 -= 18;
        if self.variant {
            self.debris.0 += 9;
            self.debris.1 -= 2;
        }
        self.stage = 2;
        self.frame = 0;
    }
    pub fn clear(&mut self, s: &Campaign, m: &mut Motion) {
        self.done = true;
        m.crash_presentation = false;
        m.crash_clear(s);
        // Paired A0000 observations retain this ground stain after D738 returns.
        if !self.variant && self.stage == 2 {
            self.blood(m, self.landing.0 - 9, self.landing.1 + 7);
        }
    }
    pub fn tick(&mut self, s: &mut Campaign, m: &mut Motion, ticks: u32) -> Result<bool> {
        self.elapsed = self.elapsed.saturating_add(ticks);
        while self.elapsed >= 7 && !self.done {
            self.elapsed -= 7;
            m.crash_clear(s);
            match self.stage {
                0 => {
                    if self.frame >= self.flight {
                        self.stage = 1;
                        self.frame = 0;
                        self.elapsed += 7;
                        continue;
                    }
                    if self.horse.1 >= -22 {
                        self.horse.1 -= self.velocity;
                        m.crash_model(s, self.horse.0, self.horse.1)?;
                    }
                    self.blood(m, self.landing.0 + 14, self.landing.1 + 6);
                    m.med.push(Draw {
                        sprite: (self.frame & 1) as usize,
                        x: i32::from(self.debris.0),
                        y: i32::from(self.debris.1),
                    });
                    self.debris.0 += 2;
                    self.debris.1 -= 2;
                    self.frame += 1;
                }
                1 => {
                    let count = if self.variant { 26 } else { 28 };
                    if self.frame >= count {
                        self.last_phase();
                        self.elapsed += 7;
                        continue;
                    }
                    if self.frame <= 7 {
                        self.blood(m, self.landing.0 + 14, self.landing.1 + 6);
                    }
                    if !self.variant && self.frame > 19 {
                        self.blood(m, self.landing.0 - 9, self.landing.1 + 7);
                    }
                    let base = if self.variant { 0x8b20 } else { 0x8b04 };
                    let offsets = if self.variant { 0x8b72 } else { 0x8b3a };
                    let frame = self.frame as usize;
                    m.med.push(Draw {
                        sprite: usize::from(s.data[base + frame]),
                        x: i32::from(self.landing.0) + i32::from(s.data[offsets + frame * 2] as i8),
                        y: i32::from(self.landing.1)
                            + i32::from(s.data[offsets + frame * 2 + 1] as i8),
                    });
                    self.frame += 1;
                }
                _ => {
                    if self.debris.0 >= 319 || self.debris.1 <= 21 {
                        self.clear(s, m);
                        break;
                    }
                    self.frame += 1;
                    if !self.variant {
                        self.blood(m, self.landing.0 - 9, self.landing.1 + 7);
                    }
                    m.med.push(Draw {
                        sprite: (if self.variant { 25 } else { 0 }) + (self.frame & 1) as usize,
                        x: i32::from(self.debris.0),
                        y: i32::from(self.debris.1),
                    });
                    self.debris.0 += 2;
                    self.debris.1 -= 2;
                    // D9F2 rechecks the exit immediately after the timed frame;
                    // clearing/returning does not consume another seven ticks.
                    if self.debris.0 >= 319 || self.debris.1 <= 21 {
                        self.clear(s, m);
                    }
                }
            }
        }
        Ok(self.done)
    }
}
