// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Race simulation and original linked sprite animation models. Rendering is pure.
use crate::assets::{Result, word};
use crate::games::put;
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub const fn actor(id: usize) -> usize {
    0xc2c2 + id * 18
}
pub fn w(s: &Campaign, at: usize) -> u16 {
    word(&s.data, at).expect("race DS word")
}
fn signed(s: &Campaign, at: usize) -> i16 {
    w(s, at) as i16
}
fn byte(s: &Campaign, at: usize) -> i16 {
    i16::from(s.data[at] as i8)
}
fn add(s: &mut Campaign, at: usize, delta: i16) {
    put(s, at, w(s, at).wrapping_add_signed(delta));
}
fn set_byte(s: &mut Campaign, at: usize, value: i16) {
    s.data[at] = value as u8;
}
fn root(s: &Campaign, id: usize) -> usize {
    usize::from(w(s, 0x83cc + id * 2))
}
fn next(s: &Campaign, node: usize) -> usize {
    usize::from(w(s, node))
}

#[derive(Clone)]
pub struct Draw {
    pub sprite: usize,
    pub x: i32,
    pub y: i32,
}
#[derive(Default)]
pub struct Motion {
    pub draws: Vec<Draw>,
    pub med: Vec<Draw>,
    pub streaks: Vec<(i32, i32, u16)>,
    pub crash: bool,
    pub crash_presentation: bool,
    pub finish_line: bool,
    pub observed_leader: Option<u16>,
}
impl Motion {
    pub fn initialize_models(s: &mut Campaign) -> Result<()> {
        fn link(
            s: &mut Campaign,
            mut node: usize,
            table: usize,
            count: &mut usize,
            guard: &mut usize,
        ) -> Result<()> {
            while node != 0 {
                *guard += 1;
                if *guard > 128 {
                    return Err("Cyclic race sprite model".into());
                }
                // Native stores an original sprite index in the transient header slot.
                put(s, node + 4, u16::from(s.data[table + *count]));
                put(s, node + 10, w(s, 0x83a4 + *count * 2));
                *count += 1;
                let child = usize::from(w(s, node + 2));
                if child != 0 {
                    link(s, child, table, count, guard)?;
                }
                node = next(s, node);
            }
            Ok(())
        }
        for id in 0..4 {
            link(
                s,
                root(s, id),
                usize::from(w(s, 0x83bc + id * 2)),
                &mut 0,
                &mut 0,
            )?;
        }
        Ok(())
    }
    pub fn chariot(s: &mut Campaign, id: usize, kind: usize) {
        let body = next(s, root(s, id));
        let horses = next(s, body);
        put(s, body + 4, u16::from(s.data[0x83c4 + kind]));
        put(s, horses + 4, u16::from(s.data[0x83c8 + kind]));
        put(s, horses + 6, w(s, 0x83e4 + kind * 2));
        put(s, body + 10, 65535);
        put(s, horses + 10, 0);
    }
    fn select(s: &mut Campaign, id: usize, animation: u8) {
        let at = actor(id);
        let old = s.data[at + 9];
        if old == animation || old == 11 {
            return;
        }
        // BA28 intentionally leaves special/crash animations alone.
        if (4..=7).contains(&old) {
            return;
        }
        let body = usize::from(w(s, next(s, root(s, id)) + 2));
        let mut node = body;
        for _ in 0..4 {
            put(s, node + 10, 0);
            let n = next(s, node);
            if n != 0 {
                node = n;
            }
        }
        node = body;
        for _ in 0..animation {
            let n = next(s, node);
            if n != 0 {
                node = n;
            }
        }
        put(s, node + 10, 65535);
    }
    fn draw_model(
        &mut self,
        s: &mut Campaign,
        mut node: usize,
        x: i16,
        y: i16,
        guard: &mut usize,
    ) -> Result<()> {
        while node != 0 {
            *guard += 1;
            if *guard > 128 {
                return Err("Cyclic active race sprite model".into());
            }
            if w(s, node + 10) != 0 {
                let frame = usize::from(s.data[node + 13]);
                let table = usize::from(w(s, node + 6));
                let px = x
                    .wrapping_add(byte(s, node + 8))
                    .wrapping_add(byte(s, table + frame * 2));
                let py = y
                    .wrapping_add(byte(s, node + 9))
                    .wrapping_add(byte(s, table + frame * 2 + 1));
                self.draws.push(Draw {
                    sprite: usize::from(w(s, node + 4)) + frame,
                    x: i32::from(px),
                    y: i32::from(py),
                });
                s.data[node + 14] = s.data[node + 14].wrapping_add(1);
                if s.data[node + 14] <= s.data[node + 15] {
                    s.data[node + 14] = 0;
                    s.data[node + 13] = s.data[node + 13].wrapping_add(1);
                    if s.data[node + 13] >= s.data[node + 12] {
                        s.data[node + 13] = 0;
                    }
                }
                let child = usize::from(w(s, node + 2));
                if child != 0 {
                    self.draw_model(s, child, px, py, guard)?;
                }
            }
            node = next(s, node);
        }
        Ok(())
    }
    pub fn contacts(s: &mut Campaign, id: usize) {
        s.data[0xc344..0xc350].fill(0);
        let x = signed(s, 0xc334 + id * 4);
        let y = signed(s, 0xc336 + id * 4);
        let mut all = [[(0i16, 0i16, 0i16, 0i16); 2]; 4];
        for (other, areas) in all.iter_mut().enumerate() {
            let ox = signed(s, 0xc334 + other * 4);
            let oy = signed(s, 0xc336 + other * 4);
            for (part, area) in areas.iter_mut().enumerate() {
                let a = 0x8ae4 + part * 8;
                *area = (
                    ox.wrapping_add(signed(s, a)),
                    oy.wrapping_add(signed(s, a + 2)),
                    ox.wrapping_add(signed(s, a + 4)),
                    oy.wrapping_add(signed(s, a + 6)),
                );
            }
        }
        for zone in 0..6 {
            let at = 0x8ab4 + zone * 8;
            let r = (
                x.wrapping_add(signed(s, at)),
                y.wrapping_add(signed(s, at + 2)),
                x.wrapping_add(signed(s, at + 4)),
                y.wrapping_add(signed(s, at + 6)),
            );
            let mut count = 0;
            for (other, areas) in all.iter().enumerate() {
                if id == other || w(s, actor(other)) == 0 {
                    continue;
                }
                for (part, b) in areas.iter().enumerate() {
                    if r.0 <= b.2 && r.2 >= b.0 && r.1 <= b.3 && r.3 >= b.1 {
                        // The original has two slots per zone; a crowded overlap can
                        // spill into the following zone. Keep its bounded DS writes.
                        s.data[0xc344 + zone * 2 + count] =
                            other as u8 | if part == 0 { 16 } else { 32 };
                        count += 1;
                    }
                }
            }
        }
    }
    fn damage(s: &mut Campaign, id: usize, amount: u8, chassis: bool) {
        let at = actor(id) + if chassis { 11 } else { 12 };
        s.data[at] = if (s.data[at] as i8) > (amount as i8) {
            s.data[at].wrapping_sub(amount)
        } else {
            0
        };
        if id == 3 {
            put(s, if chassis { 0xc36c } else { 0xc330 }, 65535);
        }
        if chassis {
            if (s.data[at] as i8) <= 0 && s.data[actor(id) + 9] != 11 {
                s.data[actor(id) + 10] = 11;
            }
        } else {
            s.data[actor(id) + 6] = s.data[actor(id) + 6].min(s.data[actor(id) + 5]);
        }
    }
    fn collision_damage(
        s: &mut Campaign,
        attacker: usize,
        victim: usize,
        chassis: bool,
        rng: &mut OriginalRng,
    ) {
        let kind = usize::from(s.data[actor(attacker) + 4]);
        let mut hits = 0;
        for _ in 0..2 {
            if u16::from(s.data[0x87ea + kind]) > rng.bounded(100) {
                hits += 1;
            }
        }
        if hits != 0 {
            Self::damage(s, victim, if chassis { hits } else { 1 }, chassis);
        }
    }
    fn zone_damage(s: &mut Campaign, id: usize, zone: usize, rng: &mut OriginalRng) {
        if w(s, actor(id)) == 0 {
            return;
        }
        let a = s.data[0xc344 + zone * 2];
        let b = s.data[0xc345 + zone * 2];
        let victim = if b & 32 != 0 && a & 16 != 0 { b } else { a };
        Self::collision_damage(s, id, usize::from(victim & 15), victim & 32 != 0, rng);
    }
    fn strike(s: &mut Campaign, _id: usize, zone: usize, rng: &mut OriginalRng) {
        let a = s.data[0xc344 + zone * 2];
        let b = s.data[0xc345 + zone * 2];
        let victim = if b & 16 != 0 && a & 32 != 0 { b } else { a };
        if victim & 32 != 0 {
            Self::damage(s, usize::from(victim & 15), 1, true);
        } else if victim & 16 != 0 {
            // B91B uses the whole contact flag as an actor index (16..19),
            // rather than its low nibble. Preserve the bounded DOS DS write.
            let raw = usize::from(victim);
            let speed = 0xc2c8 + raw * 18;
            s.data[speed] = s.data[speed].wrapping_add(rng.bounded(10) as u8).max(20);
            put(s, 0xc324 + raw * 2, w(s, 0xc322));
            Self::damage(s, usize::from(victim & 15), 1, false);
        }
    }

    fn speed(s: &mut Campaign, id: usize, delta: i16) -> bool {
        if s.data[if delta > 0 { 0xc34c } else { 0xc34e }] != 0 {
            return false;
        }
        let at = actor(id) + 6;
        set_byte(s, at, (i16::from(s.data[at]) + delta).max(20));
        if id == 3 {
            put(s, 0xc32e, 65535);
        }
        true
    }
    fn steer(s: &mut Campaign, id: usize, right: bool) -> bool {
        if s.data[if right { 0xc344 } else { 0xc34a }] != 0
            || s.data[if right { 0xc346 } else { 0xc348 }] != 0
        {
            return false;
        }
        let at = 0xc334 + id * 4;
        let x = signed(s, at);
        put(
            s,
            at,
            if right {
                x.saturating_add(2).min(280)
            } else {
                x.saturating_sub(2).max(0)
            } as u16,
        );
        true
    }
    fn projection(s: &mut Campaign, id: usize) -> bool {
        let mut diff = signed(s, 0xc316).wrapping_sub(signed(s, 0xc30a + id * 4));
        if diff > 184 {
            diff = diff.wrapping_sub(368);
        } else if diff < -184 {
            diff = diff.wrapping_add(368);
        }
        let target = diff.wrapping_shl(3).wrapping_add(100);
        let at = 0xc336 + id * 4;
        let old = signed(s, at);
        let updated =
            if diff.abs() > 24 || (old > 170 && target > 170) || (old < -22 && target < -22) {
                target
            } else {
                let old = old.clamp(-22, 170);
                let distance = target.wrapping_sub(old).abs().min(4);
                if old > target {
                    old - distance
                } else if old < target {
                    old + distance
                } else {
                    old
                }
            };
        put(s, at, updated as u16);
        (-22..=170).contains(&updated)
    }
    fn crashed(&mut self, s: &mut Campaign, id: usize, visible: bool, rng: &mut OriginalRng) {
        let body = next(s, root(s, id));
        let horses = next(s, body);
        let wreck = next(s, horses);
        put(s, horses + 10, 0);
        put(s, wreck + 10, 65535);
        let at = actor(id);
        if !visible {
            s.data[at + 9] = 7;
            s.data[at + 10] = 7;
            put(s, at, 0);
        }
        s.data[at + 13] = s.data[at + 13].wrapping_add(1);
        if s.data[at + 13] < 24 {
            if s.data[0xc2fe] > 70 {
                Self::speed(s, id, -5);
            } else {
                Self::speed(s, id, 10 + rng.bounded(10) as i16);
            }
            if rng.bounded(2) == 0 {
                s.data[at + 10] = 9;
                return;
            }
        } else if id == 3 {
            if w(s, 0xaab4) == 0 {
                for other in 0..4 {
                    put(s, actor(other), 0);
                }
                put(s, wreck + 10, 0);
                self.crash = true;
                return;
            }
            if (s.data[at + 13] > 48 && s.data[at + 5] < s.data[at + 6]) || s.data[at + 6] > 70 {
                Self::speed(s, id, -5);
            }
        } else if s.data[at + 6] > s.data[at + 5] / 3 {
            Self::speed(s, id, -5);
        }
        s.data[at + 10] = 10;
    }
    fn animate(
        &mut self,
        s: &mut Campaign,
        id: usize,
        previous_y: i16,
        rng: &mut OriginalRng,
    ) -> Result<()> {
        let at = actor(id);
        let action = s.data[at + 10];
        // B460 uses the contacts measured before BAC5 updates projection.
        if [0, 9, 10].contains(&action) {
            if action == 10 && !Self::steer(s, id, true) && s.data[0xc346] != 0 {
                Self::zone_damage(s, id, 1, rng);
            }
            if action == 9 && !Self::steer(s, id, false) && s.data[0xc348] != 0 {
                Self::zone_damage(s, id, 2, rng);
            }
            let current_y = signed(s, 0xc336 + id * 4);
            if current_y > previous_y && s.data[0xc34e] != 0 {
                put(s, 0xc336 + id * 4, previous_y as u16);
                Self::zone_damage(s, id, 5, rng);
            }
            let current_y = signed(s, 0xc336 + id * 4);
            if current_y < previous_y && s.data[0xc34c] != 0 {
                put(s, 0xc336 + id * 4, previous_y as u16);
                Self::zone_damage(s, usize::from(s.data[0xc34c] & 15), 5, rng);
            }
        } else if action == 8 && !Self::speed(s, id, -5) {
            Self::zone_damage(s, id, 5, rng);
        }
        let body = next(s, root(s, id));
        let horses = next(s, body);
        if (w(s, body + 10) != 0 && s.data[body + 13] == 0)
            || (w(s, horses + 10) != 0 && s.data[horses + 13] == 0)
        {
            if s.data[at + 6] > s.data[at + 5] {
                s.data[at + 6] = s.data[at + 5];
                if id == 3 {
                    put(s, 0xc32e, 65535);
                }
            }
            let mut command = s.data[at + 10];
            match command {
                0 | 8..=10 => Self::select(s, id, 0),
                2 => {
                    if s.data[0xc34a] != 0 {
                        Self::strike(s, id, 3, rng);
                        Self::select(s, id, 2);
                    } else {
                        command = 1;
                    }
                }
                3 => {
                    if s.data[0xc344] != 0 {
                        Self::strike(s, id, 0, rng);
                        Self::select(s, id, 3);
                    } else {
                        command = 1;
                    }
                }
                11 => {
                    put(s, body + 10, 0);
                    put(s, horses + 10, 65535);
                }
                _ => {}
            }
            s.data[at + 10] = command;
            if command == 1 && s.data[0xc34c] == 0 {
                let whips = s.data[at + 12];
                let boost = rng.bounded(10) as i16 + if whips != 0 { 10 } else { 0 };
                Self::speed(s, id, boost);
                if whips != 0 {
                    s.data[at + 12] -= 1;
                    if id == 3 {
                        put(s, 0xc330, 65535);
                    }
                }
                Self::select(s, id, 1);
                put(s, 0xc324 + id * 2, w(s, 0xc322));
            }
            s.data[at + 9] = s.data[at + 10];
            s.data[at + 10] = 0;
            if s.data[at + 9] == 11 || s.data[at + 9] == 5 {
                if s.data[at + 9] == 5 {
                    put(s, horses + 10, 0);
                    put(s, next(s, horses) + 10, 65535);
                }
                s.data[at + 10] = 5;
            }
        }
        self.draw_model(
            s,
            root(s, id),
            signed(s, 0xc334 + id * 4),
            signed(s, 0xc336 + id * 4),
            &mut 0,
        )
    }
    fn advance(&mut self, s: &mut Campaign, rng: &mut OriginalRng) -> Result<()> {
        put(s, 0xaab4, 0);
        put(s, 0xc322, w(s, 0xc322).wrapping_add(1));
        self.draws.clear();
        self.med.clear();
        self.streaks.clear();
        for id in 0..4 {
            put(s, 0xc30a + id * 4, w(s, 0xc30c + id * 4));
        }
        let total = w(s, 0x83ea).wrapping_add(u16::from(s.data[0xc2fe]));
        let shift = total >> 4;
        put(s, 0x83ea, total & 15);
        for at in (0xaa78..0xaab4).step_by(6) {
            self.streaks.push((
                i32::from(signed(s, at)),
                i32::from(signed(s, at + 2)),
                w(s, at + 4),
            ));
            add(s, at + 2, shift as i16);
            if signed(s, at + 2) > 170 {
                put(s, at, rng.bounded(77) * 4 + 5);
                put(s, at + 2, 45);
                put(s, at + 4, rng.bounded(65535));
            }
        }
        for id in 0..4 {
            let at = actor(id);
            if w(s, at) == 0 {
                continue;
            }
            Self::contacts(s, id);
            let mut speed = s.data[at + 7];
            if s.data[0xc34c] != 0 {
                speed = speed.min(s.data[actor(usize::from(s.data[0xc34c] & 15)) + 7]);
            }
            if s.data[0xc34e] != 0 {
                speed = speed.max(s.data[actor(usize::from(s.data[0xc34e] & 15)) + 7]);
            }
            add(s, at - 4, i16::from(speed));
            let distance = w(s, at - 4) / 75;
            put(s, 0xc30c + id * 4, distance);
            if distance >= 368 {
                add(s, at - 4, -27600);
                add(s, 0xc30c + id * 4, -368);
                s.data[at + 2] = s.data[at + 2].wrapping_add(1);
                if (s.data[at + 2] as i8) > (s.data[0xc350] as i8) {
                    put(s, 0xc332, 65535);
                    s.data[0xc350] = s.data[0xc350].wrapping_add(1);
                    if s.data[0xc350] == 2 {
                        self.finish_line = true;
                    }
                }
            }
            if s.data[at + 2] == 3 && s.data[at + 9] != 7 {
                s.data[at + 9] = 7;
                put(s, 0xc31a + id * 2, w(s, 0xc322));
            }
            let old_y = signed(s, 0xc336 + id * 4);
            let visible = Self::projection(s, id);
            put(s, 0xaa76, if visible { 65535 } else { 0 });
            if id != 3 {
                put(s, 0xaab4, w(s, 0xaab4) | if visible { 65535 } else { 0 });
                if !visible && s.data[0xc301] == 5 {
                    s.data[at + 9] = 7;
                    s.data[at + 10] = 7;
                    put(s, at, 0);
                }
            }
            if s.data[at + 9] == 5 {
                self.crashed(s, id, visible, rng);
            }
            if w(s, at) != 0 {
                self.animate(s, id, old_y, rng)?;
            }
        }
        let active = (0..4)
            .filter(|&id| w(s, actor(id)) != 0 && s.data[actor(id) + 2] != 3)
            .count();
        if w(s, actor(3)) == 0 || s.data[0xc301] == 7 {
            put(s, 0xc32c, 65535);
        } else if active == 1 && ![5, 11].contains(&s.data[0xc301]) {
            // B445 treats the last unfinished racer as finished on the next
            // frame, even when still several laps short of the finish line.
            put(s, 0xc32c, 65535);
            put(s, 0xc320, w(s, 0xc322).wrapping_add(1));
        }
        Ok(())
    }
}

impl Motion {
    fn corner_flags(position: u16) -> (bool, bool) {
        (
            position > 338 || (142 < position && position < 172),
            (0 < position && position < 42) || (172 < position && position < 213),
        )
    }
    fn corners(s: &mut Campaign, rng: &mut OriginalRng) {
        for id in 0..4 {
            let at = actor(id);
            if w(s, at) == 0 || s.data[at + 9] == 11 || s.data[at + 10] == 11 {
                continue;
            }
            let (approach, turn) = Self::corner_flags(w(s, 0xc30c + id * 4));
            let body = next(s, root(s, id));
            if !approach && !turn {
                set_byte(s, at + 8, (byte(s, at + 8) - 3).max(0));
                s.data[body + 8] = s.data[at + 8];
                s.data[at + 7] = s.data[at + 6];
                continue;
            }
            let angle = signed(s, 0xc334 + id * 4).abs() / 16;
            let limit = ((angle + 15) * 4) as u8;
            if turn {
                let product =
                    i16::from(s.data[0x8aa0 + angle as usize]) * i16::from(s.data[at + 6]);
                let delta = product / ((-15 - angle) * 4);
                s.data[at + 7] = s.data[at + 6].wrapping_add(delta as u8);
            }
            let difference = limit.wrapping_sub(s.data[at + 6]) as i8;
            let target = if difference > 0 {
                15 - (i16::from(difference) / 4).min(15)
            } else {
                15
            };
            let old = byte(s, at + 8);
            let moved = if old < target {
                (old + 3).min(target)
            } else {
                (old - 3).max(target)
            };
            set_byte(s, at + 8, moved.max(0));
            let severity = if difference < 0 {
                i16::from(difference).abs()
            } else {
                target
            };
            if difference < 0 && turn && (rng.bounded(15) as i8) < severity as i8 {
                s.data[at + 10] = 11;
            }
            if turn && severity > 8 {
                Self::contacts(s, id);
                let a = s.data[0xc346];
                let b = s.data[0xc347];
                if s.data[at + 9] != 5 {
                    let victim = if a & 32 != 0 {
                        Some(a)
                    } else if b & 32 != 0 {
                        Some(b)
                    } else {
                        None
                    };
                    if let Some(victim) = victim {
                        let other = usize::from(victim & 15);
                        Self::collision_damage(s, id, other, true, rng);
                        Self::collision_damage(s, other, id, true, rng);
                    }
                }
                if a == 0 && b == 0 && angle < 14 {
                    add(s, 0xc334 + id * 4, severity / 8 + 1);
                }
            }
            s.data[body + 8] = s.data[at + 8].min(15);
        }
    }
    fn avoid_player(s: &mut Campaign, id: usize) -> bool {
        let command = if s.data[0xc346] & 15 == 3 {
            Some((10, 10))
        } else if s.data[0xc344] & 15 == 3 {
            Some((3, 0))
        } else if s.data[0xc348] & 15 == 3 {
            Some((9, 9))
        } else if s.data[0xc34a] & 15 == 3 {
            Some((2, 0))
        } else {
            None
        };
        if let Some((action, cache)) = command {
            s.data[actor(id) + 10] = action;
            s.data[0x8ba8 + id] = cache;
            s.data[0x8bac + id] = 25;
            true
        } else {
            false
        }
    }
    fn ai(&mut self, s: &mut Campaign, rng: &mut OriginalRng) {
        // DABA leaves its leader local uninitialized. On the initial call the
        // observed slot is zero; normal dashboard drawing then leaves code/stack
        // addresses here (D499 or a saved BP), above any possible race progress.
        // After a wreck skips the gauges the slot is undefined again. Retain the
        // last normal sentinel rather than inventing a native machine-stack read;
        // that post-wreck limit is recorded in the private fidelity register.
        // Read-only trace replay can supply the measured undefined DOS local.
        // Normal gameplay never reads host stack memory.
        let floor = self
            .observed_leader
            .take()
            .unwrap_or_else(|| if w(s, 0xc322) == 0 { 0 } else { 0xd499 });
        let progress = |id| {
            ((byte(s, actor(id) + 2) + 1) as u16)
                .wrapping_mul(368)
                .wrapping_add(w(s, 0xc30a + id * 4))
        };
        let leader = (0..4).map(progress).fold(floor, u16::max);
        for id in 0..3 {
            let at = actor(id);
            if w(s, at) == 0 || [11, 5].contains(&s.data[at + 9]) {
                continue;
            }
            let (approach, turn) = Self::corner_flags(w(s, 0xc30c + id * 4));
            let behind = ((byte(s, at + 2) + 1) as u16)
                .wrapping_mul(368)
                .wrapping_add(w(s, 0xc30a + id * 4))
                < leader;
            if behind && s.data[0xc301] == 5 {
                s.data[at + 10] = 8;
                continue;
            }
            let angle = (signed(s, 0xc334 + id * 4).abs() / 16).min(15);
            let limit = ((angle + 14) * 4) as u8;
            Self::contacts(s, id);
            if !approach && !turn {
                if s.data[0x8bac + id] != 0 {
                    s.data[0x8bac + id] -= 1;
                    if s.data[0x8bac + id] == 0 {
                        s.data[0x8ba8 + id] = 0;
                    }
                    s.data[at + 10] = s.data[0x8ba8 + id];
                    continue;
                }
                if Self::avoid_player(s, id) {
                    continue;
                }
                if s.data[0xc34c] != 0 {
                    let action = if angle < 7 { 10 } else { 9 };
                    s.data[at + 10] = action;
                    s.data[0x8ba8 + id] = action;
                    s.data[0x8bac + id] = 25;
                    continue;
                }
                if !behind && s.data[at + 5].wrapping_add(10) <= s.data[at + 6] {
                    let roll = rng.bounded(10);
                    let action = if roll > 6 {
                        10
                    } else if roll > 2 {
                        9
                    } else {
                        0
                    };
                    s.data[at + 10] = action;
                    s.data[0x8ba8 + id] = action;
                    s.data[0x8bac + id] = 25;
                    continue;
                }
                if angle > 10 && rng.bounded(2) == 0 {
                    s.data[at + 10] = 9;
                    s.data[0x8ba8 + id] = 9;
                    s.data[0x8bac + id] = 25;
                    continue;
                }
                s.data[at + 10] = 1;
                s.data[0x8ba8 + id] = 0;
            } else {
                let difference = (rng.bounded(100) as i16) - i16::from(s.data[0xaac2 + id]);
                let distance = difference.abs();
                let difficulty = s.data[0x48f];
                let style = s.data[0xc368 + id];
                let mut target = limit;
                if difficulty <= 1 {
                    if difference < 0 && style <= 2 {
                        let roll = rng.bounded(distance as u16) as i16;
                        let adjusted = match style {
                            0 => distance / (if difficulty == 0 { 4 } else { 8 }),
                            1 => distance / (if difficulty == 0 { 2 } else { 4 }),
                            _ => distance * 3 / 4,
                        };
                        target = limit.wrapping_add(roll as u8).wrapping_sub(adjusted as u8);
                    }
                    s.data[0xaabe + id] = target;
                    if difficulty == 1 && rng.bounded(3) == 0 {
                        s.data[at + 10] = 9;
                    }
                } else if difficulty <= 3 {
                    s.data[0xaabe + id] = target;
                    if rng.bounded(if difficulty == 2 { 8 } else { 4 }) == 0 {
                        s.data[at + 10] = 9;
                    }
                }
                s.data[at + 6] = s.data[at + 6].min(s.data[0xaabe + id]).max(20);
            }
        }
    }
    pub fn step(
        &mut self,
        s: &mut Campaign,
        rng: &mut OriginalRng,
        held: u8,
        mouse_command: u8,
    ) -> Result<bool> {
        self.ai(s, rng);
        let player = actor(3);
        if w(s, player) != 0 && s.data[player + 9] != 11 && s.data[player + 10] != 11 {
            let mut directions = match mouse_command {
                9 => 2,
                10 => 1,
                8 => 8,
                _ => 0,
            };
            let mut attack = if (1..=3).contains(&mouse_command) {
                mouse_command
            } else {
                0
            };
            if held & 8 != 0 {
                if s.data[player + 9] == 0 {
                    attack = if held & 1 != 0 {
                        3
                    } else if held & 2 != 0 {
                        2
                    } else {
                        1
                    };
                }
            } else {
                if held & 4 != 0 {
                    directions |= 8;
                }
                if held & 2 != 0 {
                    directions |= 2;
                } else if held & 1 != 0 {
                    directions |= 1;
                }
            }
            let command = if attack != 0 {
                attack
            } else if directions & 2 != 0 {
                9
            } else if directions & 1 != 0 {
                10
            } else if directions & 8 != 0 {
                8
            } else {
                0
            };
            if command != 0 {
                s.data[player + 10] = command;
            }
        }
        Self::corners(s, rng);
        self.advance(s, rng)?;
        for id in 0..4 {
            if w(s, 0xc324 + id * 2).wrapping_add(150) <= w(s, 0xc322) {
                let kind = usize::from(s.data[actor(id) + 4]);
                let at = actor(id) + 12;
                set_byte(
                    s,
                    at,
                    (byte(s, at) + 1).min(i16::from(s.data[0x87e6 + kind])),
                );
                put(s, 0xc324 + id * 2, w(s, 0xc322));
                if id == 3 {
                    put(s, 0xc330, 65535);
                }
            }
        }
        Ok(w(s, 0xc32c) != 0 || self.crash)
    }
}
impl Motion {
    pub fn crash_clear(&mut self, s: &Campaign) {
        self.draws.clear();
        self.med.clear();
        self.streaks.clear();
        for at in (0xaa78..0xaab4).step_by(6) {
            self.streaks.push((
                i32::from(signed(s, at)),
                i32::from(signed(s, at + 2)),
                w(s, at + 4),
            ));
        }
    }
    pub fn crash_model(&mut self, s: &mut Campaign, x: i16, y: i16) -> Result<()> {
        self.draw_model(s, root(s, 3), x, y, &mut 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steering_blocks_on_both_contact_zones_and_clamps_track_edges() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        put(&mut s, 0xc340, 279);
        assert!(Motion::steer(&mut s, 3, true));
        assert_eq!(w(&s, 0xc340), 280);
        s.data[0xc344] = 16;
        assert!(!Motion::steer(&mut s, 3, true));
        s.data[0xc344] = 0;
        s.data[0xc346] = 32;
        assert!(!Motion::steer(&mut s, 3, true));
        s.data[0xc346] = 0;
        put(&mut s, 0xc340, 1);
        assert!(Motion::steer(&mut s, 3, false));
        assert_eq!(w(&s, 0xc340), 0);
    }
    #[test]
    fn horse_strike_preserves_original_raw_contact_index_bug() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        let mut expected = rng;
        let boost = expected.bounded(10) as u8;
        s.data[0xc344] = 17;
        s.data[actor(1) + 12] = 30;
        put(&mut s, 0xc322, 99);
        Motion::strike(&mut s, 3, 0, &mut rng);
        assert_eq!(s.data[actor(1) + 12], 29);
        assert_eq!(s.data[actor(1) + 6], 0);
        assert_eq!(s.data[0xc2c8 + 17 * 18], boost.max(20));
        assert_eq!(w(&s, 0xc324 + 17 * 2), 99);
        assert_eq!((rng.first, rng.second), (expected.first, expected.second));
    }
    #[test]
    fn projected_positions_wrap_course_and_smooth_only_nearby_racers() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        put(&mut s, 0xc316, 2);
        put(&mut s, 0xc30a, 366);
        put(&mut s, 0xc336, 100);
        assert!(Motion::projection(&mut s, 0));
        assert_eq!(w(&s, 0xc336), 104);
        put(&mut s, 0xc30a, 100);
        assert!(!Motion::projection(&mut s, 0));
        assert_eq!(w(&s, 0xc336) as i16, -684);
    }
    #[test]
    fn curve_thresholds_use_quarter_speed_difference_and_three_degree_steps() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        for id in 0..4 {
            put(&mut s, actor(id), 65535);
            put(&mut s, 0xc30c + id * 4, 200);
            put(&mut s, 0x83cc + id * 2, (1000 + id * 64) as u16);
            put(&mut s, 1000 + id * 64, (1016 + id * 64) as u16);
            put(&mut s, 1016 + id * 64, (1032 + id * 64) as u16);
            s.data[actor(id) + 6] = 40;
        }
        // lane0 => speed limit60; difference20 /4 => targetangle10.
        for expected in [3, 6, 9, 10] {
            Motion::corners(&mut s, &mut rng);
            assert_eq!(s.data[actor(3) + 8], expected);
        }
        assert_eq!(s.data[actor(3) + 7], 40);
    }
}
