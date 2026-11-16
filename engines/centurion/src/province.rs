// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Province information, controlled-province screens, tribute and plunder.
use crate::assets::{Assets, Bitmap, Result, word};
use crate::graphics::Frame;
use crate::state::Campaign;

pub const ROME_MENU: usize = 0x5a30;
pub const PROVINCE_MENU: usize = 0x5790;
pub const TRIBUTE_MENU: usize = 0x6054;
pub const POLICY_MENU: usize = 0x60ae;

impl Campaign {
    /// Resident 720B: the first roaming army, if present, precedes the place name.
    pub fn province_information(&self, province: u8) -> Result<String> {
        let name = self.province_name(province)?;
        if let Some(army) = self.roaming_army(province, 0) {
            let origin = word(&self.data, 0xc114 + army * 14)?;
            if origin >= 27 {
                return Err("Roaming army origin outside province table".into());
            }
            return Ok(format!(
                "{}{}{}{}",
                self.string(0x58e3)?,
                self.province_name(origin as u8)?,
                self.string(0x58f6)?,
                name,
            ));
        }
        Ok(name)
    }
    /// Resident 6DE5: a legion is required; even empty plunder angers the province.
    pub fn plunder(&mut self, province: u8) -> Result<u8> {
        if province == 0
            || province >= 27
            || self.ownership(province)? == 0
            || self.local_legions(province).is_empty()
        {
            return Err("Province cannot be plundered".into());
        }
        let at = usize::from(province) * 32;
        let shift = if self.ownership(province)? == 1 { 1 } else { 2 };
        self.data[0x634e] >>= shift;
        let anger = (self.data[0x527e + at] as i8).min(if shift == 1 { 50 } else { 25 });
        self.data[0x527e + at] = (anger as u8).wrapping_shl(shift);
        let amount = self.data[0x5284 + at];
        let treasury = word(&self.data, 0x634a)?.wrapping_add(u16::from(amount));
        self.data[0x634a..0x634c].copy_from_slice(&treasury.to_le_bytes());
        let total =
            u32::from(word(&self.data, 0xc09a)?) | (u32::from(word(&self.data, 0xc09c)?) << 16);
        self.data[0xc09a..0xc09e]
            .copy_from_slice(&total.wrapping_add(u32::from(amount)).to_le_bytes());
        if amount != 0 {
            self.data[0x527f + at] >>= 1;
            self.data[0x5284 + at] = 0;
        }
        Ok(amount)
    }
}

pub struct Item {
    pub label: String,
    pub shortcut: u8,
    pub disabled: bool,
    pub rect: (i32, i32, i32, i32),
}
pub struct Menu {
    pub rect: (i32, i32, i32, i32),
    pub title: String,
    pub items: Vec<Item>,
}
impl Menu {
    pub fn read(state: &Campaign, address: usize) -> Result<Self> {
        let ds = &state.data;
        let left = i32::from(word(ds, address)?);
        let top = i32::from(word(ds, address + 2)?);
        let right = i32::from(word(ds, address + 4)?);
        let bottom = i32::from(word(ds, address + 6)?);
        let entries = word(ds, address + 10)? as usize;
        let count = *ds.get(address + 14).ok_or("Truncated menu")? as usize;
        if count == 0 || count > 16 {
            return Err("Invalid province menu size".into());
        }
        let mut items = Vec::new();
        for index in 0..count {
            let row = entries + index * 12;
            items.push(Item {
                label: state.string(word(ds, row)? as usize)?,
                shortcut: ds[row + 2].to_ascii_lowercase(),
                disabled: ds[row + 3] != 0,
                rect: (
                    left + i32::from(word(ds, row + 4)?),
                    top + i32::from(word(ds, row + 6)?),
                    left + i32::from(word(ds, row + 8)?),
                    top + i32::from(word(ds, row + 10)?),
                ),
            });
        }
        Ok(Self {
            rect: (left, top, right, bottom),
            title: state.string(word(ds, address + 12)? as usize)?,
            items,
        })
    }
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        self.items.iter().position(|item| {
            let (left, top, right, bottom) = item.rect;
            !item.disabled && (left..=right).contains(&x) && (top..=bottom).contains(&y)
        })
    }
}

pub struct Panels {
    rome: Bitmap,
    border: Vec<Bitmap>,
    button: Bitmap,
    morale: Vec<Bitmap>,
}
impl Panels {
    pub fn confirm_plunder(
        &self,
        assets: &Assets,
        state: &Campaign,
        frame: &mut Frame,
        selected: Option<usize>,
    ) -> Result<()> {
        self.confirm(assets, frame, &state.string(0x57be)?, selected)
    }
    pub fn confirm(
        &self,
        assets: &Assets,
        frame: &mut Frame,
        title: &str,
        selected: Option<usize>,
    ) -> Result<()> {
        let widths = word(&assets.data_segment, 0xa05c)? as usize;
        let width = title
            .split('\n')
            .map(|line| {
                line.bytes()
                    .map(|b| i32::from(assets.data_segment[widths + usize::from(b - 32)]) + 1)
                    .sum::<i32>()
            })
            .max()
            .unwrap_or(0)
            .max(74);
        let half = (title.lines().count() as i32 + 1) * 11 / 2;
        let top = 99 - half - 7;
        let bottom = 99 + half + 7;
        frame.bevel(159 - width / 2 - 7, top, 159 + width / 2 + 7, bottom, 26);
        frame.center_lines(assets, title, 159, top + 7, 15)?;
        let y = bottom - 16;
        for (row, (label, x)) in [("Yes", 129), ("No", 165)].into_iter().enumerate() {
            frame.blit(&self.button, x, y + 1);
            frame.text(
                assets,
                label,
                x + 11,
                y,
                if selected == Some(row) { 15 } else { 0 },
            )?;
            if selected == Some(row) {
                frame.rect(x + 2, y + 3, 5, 3, 8);
            }
        }
        Ok(())
    }
    /// Compose once on entry: the sky choice consumes the original campaign RNG.
    pub fn header(
        assets: &Assets,
        state: &Campaign,
        province: u8,
        rng: &mut crate::rng::OriginalRng,
    ) -> Result<Frame> {
        let mut frame = Frame::new();
        frame.rect(0, 0, 320, 200, 116);
        frame.rect(0, 0, 320, 32, 23);
        if u16::from(state.data[0x58a6 + usize::from(province)]) <= rng.bounded(100) {
            frame.blit(
                &assets.bitmap("MNEGO", &format!("SKY{}.CBM", rng.bounded(4)))?,
                0,
                0,
            );
        }
        let terrain = char::from(b'A' + state.data[0x588a + usize::from(province)]);
        frame.blit(
            &assets.bitmap("MNEGO", &format!("NEGBG{terrain}.CBM"))?,
            0,
            0,
        );
        let foreground = char::from(b'@' + province);
        frame.blit(
            &assets.bitmap("MNEGO", &format!("NEGFG{foreground}.CBM"))?,
            0,
            31,
        );
        Ok(frame)
    }
    pub fn load(assets: &Assets) -> Result<Self> {
        let button = assets
            .sprites("MINIT", "INIT")?
            .into_iter()
            .nth(1)
            .ok_or("Missing menu button")?;
        Ok(Self {
            rome: assets.bitmap("MNEGO", "ROME.CBM")?,
            border: assets.sprites("CENT", "MMENU")?,
            button,
            morale: assets.sprites("MINIT", "INIT2")?,
        })
    }
    pub fn window(&self, frame: &mut Frame, left: i32, top: i32) {
        frame.rect(left + 4, top + 3, 143, 111, 26);
        for border in &self.border {
            frame.blit(border, left, top);
        }
    }
    pub fn menu(
        &self,
        assets: &Assets,
        state: &Campaign,
        frame: &mut Frame,
        address: usize,
        selected: Option<usize>,
    ) -> Result<()> {
        let menu = Menu::read(state, address)?;
        self.draw_menu(assets, frame, &menu, selected)
    }
    pub fn draw_menu(
        &self,
        assets: &Assets,
        frame: &mut Frame,
        menu: &Menu,
        selected: Option<usize>,
    ) -> Result<()> {
        self.window(frame, menu.rect.0, menu.rect.1);
        self.menu_contents(assets, frame, menu, selected)
    }
    pub fn plain_menu(
        &self,
        assets: &Assets,
        frame: &mut Frame,
        menu: &Menu,
        selected: Option<usize>,
    ) -> Result<()> {
        frame.bevel(menu.rect.0, menu.rect.1, menu.rect.2, menu.rect.3, 26);
        self.menu_contents(assets, frame, menu, selected)
    }
    fn menu_contents(
        &self,
        assets: &Assets,
        frame: &mut Frame,
        menu: &Menu,
        selected: Option<usize>,
    ) -> Result<()> {
        frame.center_text(
            assets,
            &menu.title,
            (menu.rect.0 + menu.rect.2) / 2,
            menu.rect.1 + 6,
            15,
        )?;
        for (index, item) in menu.items.iter().enumerate() {
            let (x, y, _, _) = item.rect;
            frame.blit(&self.button, x, y + 1);
            let color = if selected == Some(index) {
                15
            } else if item.disabled {
                6
            } else {
                0
            };
            frame.text(assets, &item.label, x + 11, y, color)?;
            if selected == Some(index) {
                frame.rect(x + 2, y + 3, 5, 3, 8);
            }
        }
        Ok(())
    }
    pub fn deactivate_menu(
        &self,
        assets: &Assets,
        state: &Campaign,
        frame: &mut Frame,
        address: usize,
    ) -> Result<()> {
        let menu = Menu::read(state, address)?;
        let first = &menu.items[0];
        let last = menu.items.last().unwrap();
        frame.rect(
            first.rect.0,
            first.rect.1,
            9,
            last.rect.3 - first.rect.1,
            26,
        );
        frame.center_text(
            assets,
            &menu.title,
            (menu.rect.0 + menu.rect.2) / 2,
            menu.rect.1 + 6,
            6,
        )
    }
    pub fn screen(
        &self,
        assets: &Assets,
        state: &Campaign,
        background: Option<(u8, &Frame)>,
        menu: usize,
        selected: Option<usize>,
    ) -> Result<Frame> {
        let mut frame = Frame::new();
        frame.rect(0, 0, 320, 200, 116);
        let province = background.map_or(0, |(province, _)| province);
        if let Some((_, header)) = background {
            frame = header.clone();
        } else {
            frame.blit(&self.rome, 0, 0);
        }
        frame.rect(0, 63, 320, 1, 0);
        frame.rect(0, 64, 320, 1, 8);
        frame.point(0, 63, 116);
        frame.point(0, 64, 116);
        frame.point(1, 64, 116);
        frame.center_text(
            assets,
            &if province == 0 {
                state.string(0x5a42)?
            } else {
                state.province_name(province)?
            },
            160,
            65,
            0,
        )?;
        self.window(&mut frame, 164, 79);
        let at = usize::from(province) * 32;
        let morale = state.morale(province)?;
        let description = state.string(word(&state.data, 0x5734 + morale * 2)? as usize)?;
        frame.center_text(
            assets,
            &format!("{}{description}", state.string(0x59cc)?),
            239,
            86,
            0,
        )?;
        let status = state.string(word(
            &state.data,
            0x57b2 + state.ownership(province)? as usize * 2,
        )? as usize)?;
        let courage = state.string(word(
            &state.data,
            0x7486 + state.data[0x527a + at] as usize / 25 * 2,
        )? as usize)?;
        let details = format!(
            "{}{status}{}{}{}{}{}{courage}",
            state.string(0x584f)?,
            state.string(0x5858)?,
            state.data[0x5281 + at],
            state.string(0x5863)?,
            word(&state.data, 0x5274 + at)?,
            state.string(0x586f)?
        );
        frame.text(assets, &details, 171, 106, 0)?;
        frame.blit(
            self.morale
                .get(morale)
                .ok_or("Invalid morale illustration")?,
            280,
            131,
        );
        let rank =
            state.string(word(&state.data, 0x5972 + state.data[0x5971] as usize * 2)? as usize)?;
        frame.text(
            assets,
            &format!(
                "{}{rank}{}{}",
                state.string(0x1692)?,
                state.string(0x1699)?,
                word(&state.data, 0x634a)?
            ),
            171,
            156,
            0,
        )?;
        let difficulty = if state.data[0x48e..0x492]
            .iter()
            .all(|v| *v == state.data[0x48e])
        {
            state.string(word(&state.data, 0x484 + state.data[0x48e] as usize * 2)? as usize)?
        } else {
            state.string(word(&state.data, 0x48c)? as usize)?
        };
        frame.center_text(
            assets,
            &format!("{difficulty}{}", state.string(0x587a)?),
            239,
            180,
            0,
        )?;
        self.menu(assets, state, &mut frame, menu, selected)?;
        Ok(frame)
    }
    /// Approved UI aid: distinguish the applied level from transient focus.
    pub fn tribute_indicator(
        &self,
        assets: &Assets,
        state: &Campaign,
        frame: &mut Frame,
        address: usize,
        province: u8,
        selected: Option<usize>,
    ) -> Result<()> {
        let level = match address {
            TRIBUTE_MENU => state.tribute(province)?,
            POLICY_MENU => state.data[0x5fc0],
            _ => return Ok(()),
        };
        if level >= 5 {
            return Ok(());
        }
        let menu = Menu::read(state, address)?;
        let item = &menu.items[usize::from(level)];
        let (x, y, _, bottom) = item.rect;
        frame.rect(x + 10, y, menu.rect.2 - x - 20, bottom - y, 8);
        frame.text(
            assets,
            &item.label,
            x + 11,
            y,
            if selected == Some(usize::from(level)) {
                15
            } else {
                7
            },
        )?;
        frame.text(
            assets,
            "Shaded: current",
            menu.rect.0 + 11,
            menu.rect.3 - 18,
            8,
        )?;
        Ok(())
    }
    pub fn list(
        &self,
        assets: &Assets,
        state: &Campaign,
        frame: &mut Frame,
        page: usize,
    ) -> Result<()> {
        let owned = state.owned_provinces();
        let rows = owned.len().min(14);
        if rows == 0 {
            return Ok(());
        }
        let top = 90 - (rows as i32 * 8 + 45) / 2;
        let bottom = top + rows as i32 * 8 + 45;
        frame.bevel(24, top, 295, bottom, 26);
        frame.center_text(assets, &state.string(0x56ef)?, 160, top + 6, 15)?;
        frame.center_text(assets, &state.string(0x3e16)?, 160, bottom - 13, 15)?;
        for index in 0..4 {
            let at = 0x57a2 + index * 4;
            frame.text(
                assets,
                &state.string(word(&state.data, at)? as usize)?,
                i32::from(word(&state.data, at + 2)?),
                top + 17,
                0,
            )?;
        }
        for (row, &province) in owned.iter().skip(page * 14).take(14).enumerate() {
            let y = top + 28 + row as i32 * 8;
            let offset = province as usize * 32;
            frame.text(assets, &state.province_name(province)?, 30, y, 0)?;
            let status = state.string(word(
                &state.data,
                0x57b2 + state.ownership(province)? as usize * 2,
            )? as usize)?;
            frame.text(assets, &status, 96, y, 0)?;
            let morale =
                state.string(word(&state.data, 0x5734 + state.morale(province)? * 2)? as usize)?;
            frame.text(assets, &morale, 142, y, 0)?;
            for (x, value) in [
                (226, word(&state.data, 0x5274 + offset)?),
                (281, u16::from(state.data[0x5281 + offset])),
            ] {
                let text = value.to_string();
                let widths = word(&assets.data_segment, 0xa05c)? as usize;
                let width: i32 = text
                    .bytes()
                    .map(|b| i32::from(assets.data_segment[widths + (b - 32) as usize]) + 1)
                    .sum();
                frame.text(assets, &text, x - width, y, 0)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn province_information_names_the_first_local_roaming_army_without_mutation() {
        let mut data = vec![0; 65536];
        for (address, text) in [
            (0xd000, "Mauretania"),
            (0xd020, "Numidia"),
            (0x58e3, "Army from\n"),
            (0x58f6, "\nin "),
        ] {
            data[address..address + text.len()].copy_from_slice(text.as_bytes());
        }
        data[0x5222 + 2 * 2..0x5224 + 2 * 2].copy_from_slice(&0xd000_u16.to_le_bytes());
        data[0x5222 + 3 * 2..0x5224 + 3 * 2].copy_from_slice(&0xd020_u16.to_le_bytes());
        let mut state = Campaign::test_data(data);
        assert_eq!(state.province_information(2).unwrap(), "Mauretania");
        state.data[0xc113] = 2;
        state.data[0xc114..0xc116].copy_from_slice(&3_u16.to_le_bytes());
        state.data[0xc11a..0xc11c].copy_from_slice(&1_u16.to_le_bytes());
        let before = state.data.clone();
        assert_eq!(
            state.province_information(2).unwrap(),
            "Army from\nNumidia\nin Mauretania"
        );
        assert_eq!(state.data, before);
    }

    #[test]
    fn plunder_requires_local_legion_and_preserves_rome_and_other_provinces() {
        let mut data = vec![0; 65536];
        for i in 0..12 {
            data[0x1925 + i * 20] = 255;
        }
        let at = 11 * 32;
        data[0x5285 + at] = 1;
        let mut state = Campaign::test_data(data);
        let before = state.data.clone();
        assert!(state.plunder(11).is_err());
        assert_eq!(state.data, before);
        state.data[0x1925] = 0;
        state.data[0x1924] = 11;
        state.data[0x634e] = 120;
        state.data[0x527e + at] = 70;
        state.data[0x527f + at] = 19;
        state.data[0x5284 + at] = 31;
        state.data[0x634a..0x634c].copy_from_slice(&65530_u16.to_le_bytes());
        state.data[0xc09a..0xc09e].copy_from_slice(&65530_u32.to_le_bytes());
        assert_eq!(state.plunder(11).unwrap(), 31);
        assert_eq!(word(&state.data, 0x634a).unwrap(), 25);
        assert_eq!(&state.data[0xc09a..0xc09e], &65561_u32.to_le_bytes());
        assert_eq!(
            (
                state.data[0x634e],
                state.data[0x527e + at],
                state.data[0x527f + at],
                state.data[0x5284 + at]
            ),
            (60, 100, 9, 0)
        );
        state.data[0x5285 + at] = 2;
        assert_eq!(state.plunder(11).unwrap(), 0);
        assert_eq!(
            (
                state.data[0x634e],
                state.data[0x527e + at],
                state.data[0x527f + at]
            ),
            (15, 100, 9)
        );
        assert_eq!(state.data[0x5285], 0);
        assert!(state.plunder(0).is_err());
    }
}
