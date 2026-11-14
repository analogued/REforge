// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Campaign-completion text from the shared race/ending overlay, DF74.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;
pub fn statistics(s: &Campaign) -> Result<String> {
    let number = |a| -> Result<i32> { Ok(i32::from(word(&s.data, a)? as i16)) };
    let total = |a| -> Result<i32> {
        Ok(i32::from_le_bytes(
            s.data[a..a + 4]
                .try_into()
                .map_err(|_| "Invalid campaign counter")?,
        ))
    };
    let mut text = s.string(0x8cd0)?;
    let alliances = number(0xc106)?;
    if alliances != 0 {
        text += &s.string(0x8d03)?;
        text += &alliances.to_string();
        text += &s.string(0x8d1c)?;
        if alliances != 1 {
            text += &s.string(0x8d26)?;
        }
        text += &s.string(0x8d28)?;
    }
    for (at, n) in [
        (0x8d2b, number(0xc0fa)?),
        (0x8d30, number(0xc0f8)? - number(0xc0fa)?),
        (0x8d43, total(0xc086)?),
        (0x8d57, total(0xc082)?),
    ] {
        text += &s.string(at)?;
        text += &n.to_string();
    }
    text += &s.string(0x8d76)?;
    let battles = number(0xc0fc)?;
    let wins = number(0xc0fe)?;
    if battles != 0 {
        text += &s.string(0x8d94)?;
        if wins != 0 {
            text += &s.string(0x8d99)?;
            text += &wins.to_string();
            text += &s.string(0x8d9e)?;
        }
        text += &s.string(0x8db2)?;
        if wins - battles + 1 == 0 {
            text += &s.string(0x8db8)?;
        } else {
            text += &if battles == wins {
                s.string(0x8dc9)?
            } else {
                (battles - wins).to_string()
            };
            text += &s.string(0x8dcc)?;
        }
        for (at, n) in [(0x8ddb, total(0xc08e)?), (0x8de7, total(0xc08a)?)] {
            text += &s.string(at)?;
            text += &n.to_string();
        }
        text += &s.string(0x8e05)?;
    }
    if number(0xc100)? != 0 {
        text += &s.string(0x8e1f)?;
        text += &number(0xc100)?.to_string();
        text += &s.string(0x8e2e)?;
        text += &if number(0xc102)? == 0 {
            s.string(0x8e3f)?
        } else {
            number(0xc102)?.to_string()
        };
        text += &s.string(0x8e42)?;
        text += &if number(0xc104)? == 0 {
            s.string(0x8e53)?
        } else {
            number(0xc104)?.to_string()
        };
        text += &s.string(0x8e58)?;
    }
    for (at, n) in [
        (0x8e5c, total(0xc092)?),
        (0x8e6b, total(0xc096)?),
        (0x8e81, total(0xc09a)?),
    ] {
        text += &s.string(at)?;
        text += &n.to_string();
    }
    text += &s.string(0x8e98)?;
    Ok(text)
}
pub fn epilogue(s: &Campaign) -> Result<String> {
    s.string(usize::from(word(&s.data, 0x8cce)?))
}

/// B9A1 shows all eighteen contributors, followed by the copyright page. It
/// retains the final parade image behind the original 30E6 popup.
pub struct Credits {
    background: Frame,
    border: Vec<Bitmap>,
    pages: Vec<(String, String)>,
    page: usize,
    elapsed: u32,
}
impl Credits {
    pub fn new(assets: &Assets, s: &Campaign, background: Frame) -> Result<Self> {
        let mut pages = Vec::new();
        for at in (0x6a6e..=0x6ab6).step_by(4) {
            pages.push((
                s.string(usize::from(word(&s.data, at)?))?,
                s.string(usize::from(word(&s.data, at + 2)?))?,
            ));
        }
        Ok(Self {
            background,
            border: assets.sprites("CENT", "MMENU")?,
            pages,
            page: 0,
            elapsed: 0,
        })
    }
    pub fn tick(&mut self, ticks: u32) -> bool {
        self.elapsed = self.elapsed.saturating_add(ticks);
        loop {
            let duration = if self.page == self.pages.len() - 1 {
                360
            } else {
                216
            };
            if self.elapsed < duration {
                return false;
            }
            self.elapsed -= duration;
            if self.page == self.pages.len() - 1 {
                return true;
            }
            self.page += 1;
        }
    }
    pub fn skip(&mut self) -> bool {
        if self.page == self.pages.len() - 1 {
            true
        } else {
            self.page = self.pages.len() - 1;
            self.elapsed = 0;
            false
        }
    }
    pub fn render(&self, assets: &Assets) -> Result<Frame> {
        let mut f = self.background.clone();
        f.rect(88, 43, 143, 111, 26);
        for b in &self.border {
            f.blit(b, 84, 40);
        }
        let (heading, names) = &self.pages[self.page];
        f.center_lines(assets, heading, 159, 47, 1)?;
        let top = 99 - names.split('\n').count() as i32 * 11 / 2;
        f.center_lines(assets, names, 159, top, 0)?;
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credits() -> Credits {
        Credits {
            background: Frame::new(),
            border: Vec::new(),
            pages: vec![(String::new(), String::new()); 19],
            page: 0,
            elapsed: 0,
        }
    }

    #[test]
    fn closing_credits_keep_the_final_page_after_skipping_contributors() {
        let mut c = credits();
        assert!(!c.tick(216 * 9 + 100));
        assert_eq!(c.page, 9);
        assert!(!c.skip());
        assert_eq!(c.page, 18);
        assert!(!c.tick(359));
        assert!(c.tick(1));
        let mut c = credits();
        assert!(!c.skip());
        assert!(c.skip());
    }

    #[test]
    fn closing_credits_are_independent_of_tick_chunk_sizes() {
        let mut large = credits();
        let mut small = credits();
        let duration = 216 * 18 + 360;
        assert!(!large.tick(duration - 1));
        for _ in 0..duration - 1 {
            assert!(!small.tick(1));
        }
        assert_eq!((large.page, large.elapsed), (small.page, small.elapsed));
        assert!(large.tick(1));
        assert!(small.tick(1));
    }
}
