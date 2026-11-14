// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Egypt's LOVE1/LOVE2 presentation, with original tick durations and DS motion.
use crate::assets::{Assets, Bitmap, Result};
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;

pub struct Cleopatra {
    first: Vec<Bitmap>,
    second: Vec<Bitmap>,
    background: Vec<u8>,
    initial: bool,
    queued: u8,
    elapsed: u32,
    blink: u32,
    breath: u32,
}
impl Cleopatra {
    pub fn new(assets: &Assets) -> Result<Self> {
        Ok(Self {
            first: assets.sprites("MNEGO", "LOVE1")?,
            second: assets.sprites("MNEGO", "LOVE2")?,
            background: Vec::new(),
            initial: false,
            queued: 0,
            elapsed: 0,
            blink: 0,
            breath: 0,
        })
    }
    fn top(body: &mut Frame, sprites: &[Bitmap], draws: &[(usize, i32, i32)]) {
        let mut top = body.clone();
        for &(id, x, y) in draws {
            top.blit(&sprites[id], x, y);
        }
        body.pixels[..64 * 320].copy_from_slice(&top.pixels[..64 * 320]);
    }
    pub fn barge(&self, body: &mut Frame, state: &Campaign) {
        Self::top(body, &self.second, &[(2, 0, 0)]);
        body.rect(8, 151, 100, 48, state.data[0x6344]);
    }
    pub fn bedroom(&self, body: &mut Frame) {
        Self::top(body, &self.second, &[(1, 0, 0)]);
        body.blit(&self.second[0], 8, 79);
    }
    pub fn start(&mut self, body: &mut Frame, state: &Campaign) {
        self.elapsed = 0;
        self.background = body.pixels[..64 * 320].to_vec();
        Self::top(
            body,
            &self.first,
            &[
                (0, 0, 0),
                (1, 224, 0),
                (2, i32::from(state.data[0x3c26]), 0),
            ],
        );
    }
    pub fn skip_lead_in(&mut self) {
        if self.elapsed < 72 {
            self.elapsed = 72;
        } else {
            self.queue_key();
        }
    }
    pub fn tick(&mut self, body: &mut Frame, state: &Campaign, count: u32) -> Option<u32> {
        let left = 464 - self.elapsed;
        let consumed = count.min(left);
        let before = self.elapsed;
        self.elapsed += consumed;
        // 4852 first shows thirteen foreground poses, then retains the last
        // pose while the DS x table continues through frame27 (14 ticks each).
        if self.elapsed >= 72 && self.elapsed > before {
            let index = ((self.elapsed - 72) / 14).min(27) as usize;
            let mut top = Frame::new();
            top.pixels[..64 * 320].copy_from_slice(&self.background);
            Self::top(
                &mut top,
                &self.first,
                &[
                    (0, 0, 0),
                    (1, 224, 0),
                    (2, i32::from(state.data[0x3c26]), 0),
                ],
            );
            // These are transparent patches over the preceding pose, rather
            // than independent full frames. Replay the bounded prefix so both
            // large tick chunks and individual frames compose identically.
            for pose in 0..=index {
                if pose < 13 {
                    top.blit(&self.first[3 + pose], 85, 3);
                }
                if state.data[0x3c26 + pose] != 0 {
                    top.blit(&self.first[1], 224, 0);
                    top.blit(&self.first[2], i32::from(state.data[0x3c26 + pose]), 0);
                }
            }
            body.pixels[..64 * 320].copy_from_slice(&top.pixels[..64 * 320]);
        }
        (self.elapsed == 464).then_some(count - consumed)
    }
    pub fn finish(&mut self, body: &mut Frame, state: &Campaign) {
        self.barge(body, state);
        body.rect(8, 79, 98, 68, state.data[0x6344]);
        self.blink = 0;
        self.initial = true;
        self.breath = 144;
    }
    pub fn queue_key(&mut self) {
        self.queued = self.queued.saturating_add(1).min(15);
    }
    pub fn take_key(&mut self) -> bool {
        if self.queued == 0 {
            false
        } else {
            self.queued -= 1;
            true
        }
    }
    pub fn pollable(&self) -> bool {
        !self.initial && self.blink == 0
    }
    pub fn idle(&mut self, rng: &mut OriginalRng, mut ticks: u32) -> bool {
        if self.initial && ticks > 0 {
            self.blink = 9;
            self.initial = false;
        }
        while ticks > 0 {
            if self.blink > 0 {
                let n = ticks.min(self.blink);
                self.blink -= n;
                self.breath += n;
                ticks -= n;
                if self.blink == 0 && self.take_key() {
                    return true;
                }
            } else if self.breath > 72 {
                self.breath = 0;
                if rng.bounded(12) < 2 {
                    self.blink = 9;
                }
            } else {
                let n = ticks.min(73 - self.breath);
                self.breath += n;
                ticks -= n;
            }
        }
        false
    }
    pub fn overlay(&self, body: &mut Frame) {
        if self.blink > 0 {
            body.blit(&self.second[3], 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sprite(color: u8) -> Bitmap {
        Bitmap {
            width: 1,
            height: 1,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![color],
            visible: vec![true],
        }
    }
    fn scene() -> Cleopatra {
        Cleopatra {
            first: (0..16).map(sprite).collect(),
            second: (20..24).map(sprite).collect(),
            background: Vec::new(),
            initial: false,
            queued: 0,
            elapsed: 0,
            blink: 0,
            breath: 0,
        }
    }
    #[test]
    fn full_animation_keeps_final_pose_and_last_nonzero_movement_with_any_tick_chunk() {
        let mut s = Campaign::test_data(vec![0; 65536]);
        s.data[0x3c26..0x3c42].fill(230);
        s.data[0x3c26 + 27] = 0;
        let mut a = scene();
        let mut b = scene();
        let mut whole = Frame::new();
        let mut split = Frame::new();
        whole.rect(0, 64, 320, 136, 99);
        split.rect(0, 64, 320, 136, 99);
        a.start(&mut whole, &s);
        b.start(&mut split, &s);
        assert_eq!(a.tick(&mut whole, &s, 500), Some(36));
        for i in 0..464 {
            assert_eq!(
                b.tick(&mut split, &s, 1),
                if i == 463 { Some(0) } else { None }
            );
        }
        assert_eq!(whole.pixels, split.pixels);
        assert_eq!(whole.pixels[3 * 320 + 85], 15);
        assert_eq!(whole.pixels[230], 2);
        assert!(whole.pixels[64 * 320..].iter().all(|x| *x == 99));
    }
    #[test]
    fn input_can_shorten_only_the_lead_in() {
        let mut c = scene();
        c.skip_lead_in();
        assert_eq!(c.elapsed, 72);
        c.elapsed = 100;
        c.skip_lead_in();
        assert_eq!(c.elapsed, 100);
    }
    #[test]
    fn idle_randomness_is_independent_of_tick_chunk_size() {
        let mut a = scene();
        let mut b = scene();
        let mut whole = OriginalRng {
            first: 15,
            second: 40,
        };
        let mut split = whole;
        a.finish(&mut Frame::new(), &Campaign::test_data(vec![0; 65536]));
        b.finish(&mut Frame::new(), &Campaign::test_data(vec![0; 65536]));
        a.idle(&mut whole, 1000);
        for _ in 0..1000 {
            b.idle(&mut split, 1);
        }
        assert_eq!((whole.first, whole.second), (split.first, split.second));
        assert_eq!((a.blink, a.breath), (b.blink, b.breath));
    }
    #[test]
    fn transparent_poses_keep_preceding_pixels_instead_of_replacing_the_entire_frame() {
        let mut c = scene();
        let mut s = Campaign::test_data(vec![0; 65536]);
        let mut f = Frame::new();
        s.data[0x3c26..0x3c42].fill(230);
        c.first[4].origin_x = -1;
        for sprite in &mut c.first[5..] {
            sprite.origin_x = -2;
        }
        c.start(&mut f, &s);
        c.tick(&mut f, &s, 464);
        assert_eq!(
            (
                f.pixels[3 * 320 + 85],
                f.pixels[3 * 320 + 86],
                f.pixels[3 * 320 + 87]
            ),
            (3, 4, 15)
        );
    }
    #[test]
    fn queued_keys_interrupt_after_animation_wait_then_the_nine_tick_blink() {
        let mut c = scene();
        c.elapsed = 100;
        c.skip_lead_in();
        c.skip_lead_in();
        assert_eq!(c.queued, 2);
        assert!(c.take_key());
        c.finish(&mut Frame::new(), &Campaign::test_data(vec![0; 65536]));
        let mut rng = OriginalRng {
            first: 15,
            second: 40,
        };
        assert!(!c.idle(&mut rng, 8));
        assert!(c.idle(&mut rng, 1));
        assert_eq!((rng.first, rng.second), (15, 40));
    }
}
