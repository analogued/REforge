// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Alley window animation and entry, using the external executable's tables.
//!
//! The caller supplies the player and scenery framebuffer. Projectile launch
//! shares the original window hold and random generator. The difficulty-six
//! one-tick hold is corrected by default, with an optional development override.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::framebuffer::{Dimensions, Framebuffer, pixel_offset};
use crate::player::Player;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum HoldTiming {
    #[default]
    Corrected,
    #[cfg(feature = "dev-tools")]
    Original,
}

impl HoldTiming {
    fn delay(self, data: &[u8], difficulty: usize) -> u16 {
        let delay = word(data, 0x181e + difficulty * 2);
        #[cfg(feature = "dev-tools")]
        if matches!(self, Self::Original) {
            return delay;
        }
        // Deliberate correction: the isolated one-tick entry at difficulty six
        // makes windows close almost immediately. Eighteen ticks match the
        // neighboring difficulties; the historical intended value is unknown.
        if difficulty == 6 && delay == 1 {
            18
        } else {
            delay
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Path {
    #[default]
    Countdown,
    Retrace,
    PlayerBelow,
    Projectile,
    Selection,
    PlayerHold,
    MouseHold,
    Holding,
    OpenRow,
    CopyRow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Work {
    pub path: Path,
    pub selections: u32,
    pub band_check: bool,
    pub hold_started: bool,
    pub hold_expired: bool,
    pub launched: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Update {
    /// The invocation countdown expired and the retrace-clear gate passed.
    pub accepted: bool,
    pub drawn_row: Option<u8>,
    pub contact_hold: bool,
    pub work: Work,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else {
        Ok(())
    }
}

/// CS:1830 preserves the invocation countdown and previous window position.
pub(crate) fn initialize(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    for offset in [0x1665, 0x1673, 0x1677, 0x1678] {
        data[offset] = 0;
    }
    set_word(data, 0x166c, 9);
    Ok(())
}

/// CS:1b05 returns overlap separately from the entry flag. Equality counts as
/// contact; upper additions wrap and lower bounds saturate, as in CS:2e29.
/// The forced-fall latch remains in the supplied runtime data at 055a.
pub(crate) fn contact(data: &mut [u8], player: &Player) -> bool {
    if data.len() < DATA_LENGTH {
        return false;
    }
    let x = word(data, 0x1666);
    let y = data[0x1668];
    let overlap = x.wrapping_add(32) >= player.x
        && x.saturating_sub(24) <= player.x
        && y.wrapping_add(15) >= player.y
        && y.saturating_sub(14) <= player.y;
    if overlap
        && player.vertical_direction == 1
        && data[0x55a] == 0
        && player.y < 96
        && (5..25).contains(&data[0x1665])
    {
        data[0x551] = 1;
    }
    overlap
}

/// CS:1b4c checks the middle/bottom mouse for first/second-row windows. The
/// original does not consult mouse inactivity or direction in this predicate.
fn mouse_overlap(data: &[u8]) -> bool {
    let id = data[0x1669];
    if id >= 8 {
        return false;
    }
    let mouse = word(data, if id & 4 == 0 { 0x1f32 } else { 0x1f34 });
    let right = mouse.wrapping_add(16);
    let window = word(data, 0x1666);
    right >= window && right.saturating_sub(48) <= window
}

/// One grouped actor call at CS:0197. Unlike the bin cat, this routine requires
/// retrace bit 8 clear. The countdown decrements even on rejected calls and is
/// reset to 13 before testing retrace. One accepted animation step writes only
/// eight packed bytes, with the closing row copied from the external wall tile.
#[cfg(test)]
pub(crate) fn advance(
    data: &mut [u8],
    screen: &mut Framebuffer,
    player: &Player,
    tick: u16,
    retrace: bool,
    timing: HoldTiming,
) -> Result<Update, String> {
    advance_with_projectiles(data, screen, player, tick, retrace, timing, false)
}

pub(crate) fn advance_with_projectiles(
    data: &mut [u8],
    screen: &mut Framebuffer,
    player: &Player,
    tick: u16,
    retrace: bool,
    timing: HoldTiming,
    projectiles_enabled: bool,
) -> Result<Update, String> {
    check_data(data)?;
    data[0x166a] = data[0x166a].wrapping_sub(1);
    if data[0x166a] != 0 {
        return Ok(Update::default());
    }
    data[0x166a] = 13;
    if retrace {
        return Ok(Update {
            work: Work {
                path: Path::Retrace,
                ..Work::default()
            },
            ..Update::default()
        });
    }
    let mut update = Update {
        accepted: true,
        ..Update::default()
    };
    if data[0x1665] != 0 {
        contact(data, player);
    }
    if data[0x1673] != 0 {
        update.work.path = Path::Projectile;
        return Ok(update);
    }
    if data[0x1665] == 0 {
        if player.y > 96 {
            update.work.path = Path::PlayerBelow;
            return Ok(update);
        }
        let difficulty = usize::from(word(data, 8));
        if difficulty > 7 {
            return Err("Window difficulty exceeds its timing table".into());
        }
        update.work.path = Path::Selection;
        update.work.band_check = player.band_state == 1 && data[0x418] == 0;
        data[0x1677] =
            u8::from(update.work.band_check && tick.wrapping_sub(word(data, 0x556)) >= 72);
        // A complete generator period bounds diagnostics with malformed tables
        // or zero seed; supported original states always offer a free window.
        let mut selected = false;
        for _ in 0..=u16::MAX {
            update.work.selections += 1;
            let id = (next_random(data) as u8) & if data[0x1677] != 0 { 3 } else { 15 };
            if id >= 12 {
                return Ok(update);
            }
            data[0x1669] = id;
            let x = word(data, 0x1658 + usize::from(id & 3) * 2);
            let y = data[0x1660 + usize::from((id >> 2) & 3)];
            set_word(data, 0x1666, x);
            data[0x1668] = y;
            if !contact(data, player) {
                selected = true;
                break;
            }
        }
        if !selected {
            return Err("Window selection cannot find a nonoverlapping window".into());
        }
        data[0x1665] = 29;
        let delay = timing.delay(data, difficulty);
        set_word(data, 0x166c, delay);
        data[0x1670] = 1;
    }
    if contact(data, player) {
        update.work.path = Path::PlayerHold;
        update.contact_hold = true;
        return Ok(update);
    }
    data[0x1664] = u8::from(mouse_overlap(data));
    if data[0x1664] != 0 {
        update.work.path = Path::MouseHold;
        update.contact_hold = true;
        return Ok(update);
    }
    if data[0x1665] == 16 {
        update.work.hold_started = true;
        set_word(data, 0x166e, tick);
    }
    if data[0x1665] == 15 && tick.wrapping_sub(word(data, 0x166e)) < word(data, 0x166c) {
        update.work.path = Path::Holding;
        if projectiles_enabled {
            update.work.launched = crate::projectiles::launch(data, player)?;
        }
        return Ok(update);
    }
    update.work.hold_expired = data[0x1665] == 15;
    data[0x1665] = data[0x1665].wrapping_sub(1);
    let phase = data[0x1665];
    let y = if phase > 14 {
        data[0x1668].wrapping_add(phase).wrapping_sub(14)
    } else {
        data[0x1668].wrapping_add(14).wrapping_sub(phase)
    };
    data[0x166b] = y;
    let offset = pixel_offset(word(data, 0x1666), y);
    if phase > 14 && data[0x418] == 0 {
        update.work.path = Path::OpenRow;
        screen.copy_rect(offset, Dimensions(0x0104), &[0; 8])?;
    } else {
        update.work.path = Path::CopyRow;
        let row = usize::from(y.wrapping_sub(data[0x1668]));
        let source = if phase > 14 {
            0x15e0 + row * 8
        } else {
            0x2681 + row * 10
        };
        screen.copy_rect(
            offset,
            Dimensions(0x0104),
            sprite_bytes(data, source as u16, Dimensions(0x0104))?,
        )?;
    }
    update.drawn_row = Some(y);
    Ok(update)
}

/// Selection work counted along the actual RNG/history path.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SelectionWork {
    pub attempts: u32,
    pub extra_draws: u32,
    pub history_rejections: u32,
}

/// CS:01e5..0238 retains weighted choices and two-entry history.
/// Saving the alley return position precedes this helper.
fn choose_room(data: &mut [u8]) -> Result<(u16, SelectionWork), String> {
    check_data(data)?;
    let mut work = SelectionWork::default();
    let selected = if data[0x418] != 0 {
        data[0x418] = 0;
        7
    } else {
        let difficulty = usize::from(word(data, 8) & 3);
        let mut calls = 0u32;
        let mut draw = |data: &mut [u8]| -> Result<u16, String> {
            if calls > u32::from(u16::MAX) {
                return Err("Room selection exhausted the random generator period".into());
            }
            calls += 1;
            Ok(next_random(data))
        };
        let candidate = loop {
            work.attempts += 1;
            let first = draw(data)?;
            let candidate = if first & 0xa0 != 0 && difficulty != 3 {
                data[0x421 + difficulty * 4 + usize::from(first & 3)]
            } else {
                let index = loop {
                    work.extra_draws += 1;
                    let random = draw(data)? & 7;
                    if random < 5 {
                        break random;
                    }
                };
                data[0x42d + usize::from(index)]
            };
            let candidate = u16::from(candidate);
            if candidate != word(data, 0x41d) || candidate != word(data, 0x41f) {
                break candidate;
            }
            work.history_rejections += 1;
        };
        set_word(data, 0x41f, word(data, 0x41d));
        set_word(data, 0x41d, candidate);
        candidate
    };
    set_word(data, 4, selected);
    set_word(data, 6, 0);
    Ok((selected, work))
}

/// CS:01b7..0238 saves the contact position and clock before selecting a room.
pub(crate) fn select_room(
    data: &mut [u8],
    player: &Player,
    tick: u16,
) -> Result<(u16, SelectionWork), String> {
    check_data(data)?;
    set_word(data, 0x410, tick);
    set_word(data, 1, player.x);
    data[3] = player.y;
    data[0x419] = 1;
    choose_room(data)
}

#[cfg(test)]
mod tests;
