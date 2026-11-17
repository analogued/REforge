// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Race field, linked actor sprites, dashboard and persistent course-map dots.
use crate::assets::{Assets, Bitmap, Result, VideoMode};
use crate::games::put;
use crate::graphics::Frame;
use crate::race_motion::{Motion, actor, w};
use crate::state::Campaign;

pub struct View {
    video: VideoMode,
    pub frame: Frame,
    header: Vec<u8>,
    sprites: Vec<Bitmap>,
    laps: Vec<Bitmap>,
    med: Vec<Bitmap>,
    track: Vec<(i32, i32)>,
}

fn narrow(f: &mut Frame, l: i32, t: i32, r: i32, b: i32) {
    f.rect(l, t, r - l + 1, b - t + 1, 26);
    f.rect(l, t, 2, b - t + 1, 8);
    f.rect(r - 1, t, 2, b - t + 1, 8);
    f.rect(l, t, r - l + 1, 1, 15);
    f.rect(l + 1, t + 1, r - l - 1, 1, 15);
    f.rect(l, b, r - l + 1, 1, 0);
    f.rect(l + 1, b - 1, r - l - 1, 1, 0);
}
impl View {
    pub fn new(assets: &Assets, state: &mut Campaign) -> Result<Self> {
        let mut frame = Frame::new();
        frame.blit(&assets.bitmap("MRACE", "CIRCUS.CBM")?, 0, 0);
        frame.rect(0, 61, 320, 139, assets.video.color(17, 7));
        frame.bevel(
            1,
            171,
            318,
            198,
            assets.video.color(0, w(state, 0x6346) as u8),
        );
        narrow(&mut frame, 11, 180, 127, 189);
        narrow(&mut frame, 196, 176, 312, 184);
        narrow(&mut frame, 196, 185, 312, 193);
        for (text, x, y) in [("S", 7, 181), ("H", 191, 177), ("C", 191, 186)] {
            frame.text_font(assets, text, x, y, 15, 0xa3b4)?;
        }
        let mut track = Vec::new();
        let mut last = (i32::from(w(state, 0x896a)) + 1, i32::from(w(state, 0x896c)));
        for at in (0x896a..0x8a8a).step_by(4) {
            let point = (
                i32::from(w(state, at) as i16),
                i32::from(w(state, at + 2) as i16),
            );
            if point.0 == -1 {
                track.extend([(-1, -1); 2]);
                last = (i32::from(w(state, at + 4)), i32::from(w(state, at + 6)));
                continue;
            }
            if (point.0 - last.0).abs() > 1 {
                let sign = if point.0 < last.0 { -1 } else { 1 };
                let mut x = last.0;
                loop {
                    track.push((x, point.1));
                    if x == point.0 {
                        break;
                    }
                    x += sign;
                }
            } else if (point.1 - last.1).abs() > 1 {
                let sign = if point.1 < last.1 { -1 } else { 1 };
                let mut y = last.1;
                loop {
                    track.push((point.0, y));
                    if y == point.1 {
                        break;
                    }
                    y += sign;
                }
            } else {
                track.push(point);
            }
            last = point;
        }
        if track.len() != 368 {
            return Err(format!("Invalid race course: {} points", track.len()));
        }
        for at in (0xc30a..0xc31a).step_by(2) {
            put(state, at, 220);
        }
        let mut view = Self {
            video: assets.video,
            header: frame.pixels[..61 * 320].to_vec(),
            frame,
            sprites: assets.sprites("MRACE", "RACE")?,
            laps: assets.sprites("MRACE", "LAPS")?,
            med: assets.sprites("MRACE", "MED")?,
            track,
        };
        view.speed(state);
        view.horses(state);
        view.chassis(state);
        view.laps(state)?;
        Ok(view)
    }
    fn laps(&mut self, state: &Campaign) -> Result<()> {
        let lap = (state.data[0xc350] as i8).max(0) as usize;
        if let Some(sprite) = self.laps.get(lap) {
            self.frame.blit(sprite, 134, 185);
        }
        // A finished opponent can resume racing and advance the original lap
        // counter past this four-entry table. In the reference allocation the
        // following headers have zero height: 817C rejects them without drawing.
        // Preserve the counter and last label; never read beyond native assets.
        Ok(())
    }
    fn speed(&mut self, s: &Campaign) {
        if s.data[0xc301] == 5 {
            return;
        }
        let length = i32::from(s.data[0xc2fe]) * 110 / 130;
        self.frame.rect(14, 183, length, 4, 2);
        self.frame.rect(14 + length, 183, 125 - 14 - length, 4, 7);
        let angle = (w(s, 0xc340) as i16).abs() / 16;
        let threshold = ((angle.min(15) + 15) * 4) as u8;
        let x = 13 + i32::from(threshold) * 110 / 130;
        self.frame.rect(x, 183, 1, 4, 4);
    }
    fn horses(&mut self, s: &Campaign) {
        if s.data[0xc301] == 5 {
            return;
        }
        let length = (i32::from(s.data[0xc304] as i8) * 110 / 60).max(0);
        self.frame.rect(199, 179, length, 3, 4);
        self.frame.rect(199 + length, 179, 310 - 199 - length, 3, 7);
    }
    fn chassis(&mut self, s: &Campaign) {
        if s.data[0xc301] == 5 {
            return;
        }
        let length = (i32::from(s.data[0xc303] as i8) * 110 / 20).max(0);
        self.frame.rect(199, 188, length, 3, 1);
        self.frame.rect(199 + length, 188, 310 - 199 - length, 3, 7);
    }
    pub fn update(&mut self, s: &mut Campaign, motion: &Motion) -> Result<()> {
        let mut field = Frame::new();
        field.rect(0, 61, 320, 110, self.video.color(17, 7));
        for &(x, y, bits) in &motion.streaks {
            // EGA's planar writer uses fixed bits 5/4 within an eight-pixel
            // byte, rather than VGA's two adjacent indexed pixels at x.
            let x = match self.video {
                VideoMode::Vga => x,
                VideoMode::Ega => (x >> 3) * 8 + 2,
            };
            for offset in 0..16 {
                if bits & (0x8000 >> offset) != 0 && y < 170 && (61..=170).contains(&(y + offset)) {
                    field.point(x, y + offset, self.video.color(82, 15));
                    field.point(x + 1, y + offset, self.video.color(86, 8));
                }
            }
        }
        for draw in &motion.draws {
            field.blit(
                self.sprites
                    .get(draw.sprite)
                    .ok_or("Invalid chariot animation sprite")?,
                draw.x,
                draw.y,
            );
        }
        for draw in &motion.med {
            field.blit(
                self.med
                    .get(draw.sprite)
                    .ok_or("Invalid chariot crash sprite")?,
                draw.x,
                draw.y,
            );
        }
        self.frame.pixels[61 * 320..171 * 320].copy_from_slice(&field.pixels[61 * 320..171 * 320]);
        // D738 suspends B3D9: retain the last visible mini-map until return.
        if !motion.crash_presentation {
            self.frame.pixels[..61 * 320].copy_from_slice(&self.header);
            if motion.finish_line {
                self.frame.rect(120, 22, 1, 7, 15);
            }
            for id in 0..4 {
                if w(s, actor(id)) != 0 && s.data[actor(id) + 2] != 3 {
                    let (x, y) = self.track[usize::from(w(s, 0xc30c + id * 4))];
                    self.frame.point(x, y, w(s, 0x87de + id * 2) as u8);
                }
            }
        }
        self.speed(s);
        if w(s, 0xc330) != 0 {
            self.horses(s);
        }
        if w(s, 0xc36c) != 0 {
            self.chassis(s);
        }
        if w(s, 0xc332) != 0 {
            self.laps(s)?;
        }
        for at in [0xc32e, 0xc330, 0xc36c, 0xc332] {
            put(s, at, 0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaks_keep_original_video_colors_alignment_and_inclusive_bottom_quirk() {
        for video in [VideoMode::Vga, VideoMode::Ega] {
            let mut state = Campaign::test_data(vec![0; 65536]);
            state.data[0xc301] = 5;
            let mut view = View {
                video,
                frame: Frame::new(),
                header: vec![0; 61 * 320],
                sprites: Vec::new(),
                laps: Vec::new(),
                med: Vec::new(),
                track: Vec::new(),
            };
            let motion = Motion {
                streaks: vec![(101, 70, 0xa000), (5, 169, 0xffff), (9, 170, 0xffff)],
                crash_presentation: true,
                ..Motion::default()
            };
            view.update(&mut state, &motion).unwrap();
            let (x, first, second, background) = match video {
                VideoMode::Vga => (101, 82, 86, 17),
                VideoMode::Ega => (98, 15, 8, 7),
            };
            for y in [70, 72] {
                assert_eq!(view.frame.pixels[y * 320 + x], first);
                assert_eq!(view.frame.pixels[y * 320 + x + 1], second);
            }
            assert_eq!(view.frame.pixels[71 * 320 + x], background);
            let bottom_x = if video == VideoMode::Vga { 5 } else { 2 };
            assert_eq!(view.frame.pixels[170 * 320 + bottom_x], first);
            // A streak beginning at the bottom boundary is omitted, while a
            // streak beginning above it may paint the inclusive final row.
            assert_eq!(view.frame.pixels[170 * 320 + 10], background);
        }
    }
}
