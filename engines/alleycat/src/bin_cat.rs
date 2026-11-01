// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Nonlethal bin cat animation and the original dog-alert contact predicate.
//!
//! Rendering and the player's response belong to the caller. The alert is kept
//! in original state even while the damaging dog remains deliberately absent.

use crate::assets::{DATA_LENGTH, next_random, set_word, word};
use crate::framebuffer::{Dimensions, pixel_offset};
use crate::player::{Controls, Player};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VisibleSprite {
    pub source: u16,
    pub dimensions: Dimensions,
    pub offset: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Update {
    /// CS:22dc skips restoration only for the first frame, count 26.
    pub restore_previous: bool,
    pub sprite: Option<VisibleSprite>,
    /// Contact before advancing preserves the previous image; contact after
    /// advancing restores the previous image and suppresses the new one.
    pub contact_hold: bool,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else {
        Ok(())
    }
}

/// CS:2210 resets only the animation count, preserving previous bin selection.
pub(crate) fn initialize(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    data[0x1d59] = 0;
    Ok(())
}

/// CS:22f7 uses the inclusive, wrapping CS:2e29 predicate. It sets the dog
/// alert on contact, without changing the player or life count.
pub(crate) fn contact(data: &mut [u8], player: &Player) -> bool {
    let hit = overlaps(data, player);
    if hit {
        data[0x1d58] = 1;
    }
    hit
}

pub(crate) fn overlaps(data: &[u8], player: &Player) -> bool {
    if data.len() < DATA_LENGTH {
        return false;
    }
    if data[0x1d59] == 0 {
        return false;
    }
    let x = word(data, 0x1d5c);
    let y = data[0x1d5f];
    let height = (word(data, 0x1d64) >> 8) as u8;
    x.wrapping_add(16) >= player.x
        && x.saturating_sub(24) <= player.x
        && y.wrapping_add(height) >= player.y
        && y.saturating_sub(14) <= player.y
}

/// CS:0e60..0e74 writes logical control latches before the player initializes
/// its downward transition. The original chooses a new direction on every hit.
pub(crate) fn response_controls(data: &mut [u8]) -> Controls {
    data[0x699] = 1;
    data[0x56c] = 1;
    let x = if next_random(data) & 1 != 0 { 1 } else { -1 };
    data[0x698] = x as u8;
    Controls { x, y: 1 }
}

/// CS:0e4f clears the contact-effect latch on a supported alley miss.
pub(crate) fn clear_response_latch(data: &mut [u8]) {
    data[0x56c] = 0;
}

/// CS:15d0 samples the external difficulty ranges, rejecting the previous bin.
fn select_position(data: &mut [u8]) -> Result<(u16, u8), String> {
    let difficulty = usize::from(word(data, 8));
    if difficulty > 7 {
        return Err("Bin cat difficulty exceeds its selection tables".into());
    }
    let limit = data[0x100e + difficulty];
    let base = data[0x1006 + difficulty];
    // Valid original tables always offer a new index. Reject a malformed table
    // instead of spinning forever in the original rejection-sampling loop.
    if !(0..=limit.min(7)).any(|candidate| candidate.wrapping_add(base) != data[0x1028]) {
        return Err("Bin cat selection table offers only the previous bin".into());
    }
    let index = loop {
        let candidate = (next_random(data) & 7) as u8;
        if candidate <= limit {
            let index = candidate.wrapping_add(base);
            if index != data[0x1028] {
                break index;
            }
        }
    };
    data[0x1028] = index;
    let position = data[0xff0 + usize::from(index)];
    Ok((
        u16::from(position & 0x7f) * 4,
        if position & 0x80 != 0 { 0x88 } else { 0x90 },
    ))
}

/// One scheduled CS:2216 call. The tick is stored after the retrace gate and
/// before contact, including a contact-held update or a failed random start.
/// Background-save geometry at 1d60/1d66 belongs to the caller's compositor.
pub(crate) fn advance(
    data: &mut [u8],
    player: &Player,
    tick: u16,
    retrace: bool,
) -> Result<Option<Update>, String> {
    check_data(data)?;
    if tick == word(data, 0x1d5a) || !retrace {
        return Ok(None);
    }
    set_word(data, 0x1d5a, tick);
    if contact(data, player) {
        return Ok(Some(Update {
            restore_previous: false,
            sprite: None,
            contact_hold: true,
        }));
    }
    if data[0x1d59] == 0 {
        if player.y != 0x86 && player.y != 0x8e && next_random(data) as u8 > 5 {
            return Ok(None);
        }
        let (x, y) = select_position(data)?;
        data[0x1d5e] = y.wrapping_add(3);
        let offset = next_random(data) & 7;
        set_word(data, 0x1d5c, x.wrapping_add(offset).wrapping_add(6));
        data[0x1d59] = 27;
    }
    data[0x1d59] = data[0x1d59].wrapping_sub(1);
    let count = data[0x1d59];
    let (y, height) = if count > 13 {
        (
            data[0x1d5e].wrapping_add(count).wrapping_sub(15),
            27u8.wrapping_sub(count),
        )
    } else {
        (data[0x1d5e].wrapping_add(12).wrapping_sub(count), count)
    };
    let dimensions = Dimensions((u16::from(height) << 8) | 2);
    let offset = pixel_offset(word(data, 0x1d5c), y);
    set_word(data, 0x1d64, dimensions.0);
    data[0x1d5f] = y;
    set_word(data, 0x1d62, offset);
    let hit = contact(data, player);
    Ok(Some(Update {
        restore_previous: count != 26,
        sprite: if count == 0 || hit {
            None
        } else {
            Some(VisibleSprite {
                source: 0x1cf0,
                dimensions,
                offset,
            })
        },
        contact_hold: hit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0x1024);
        data[0x100e] = 7;
        data[0x1028] = 255;
        for index in 0..8 {
            data[0xff0 + index] = 20 + index as u8;
        }
        data
    }

    fn active(data: &mut [u8], count: u8, x: u16, y: u8, height: u8) {
        data[0x1d59] = count;
        set_word(data, 0x1d5c, x);
        data[0x1d5e] = y;
        data[0x1d5f] = y;
        set_word(data, 0x1d64, (u16::from(height) << 8) | 2);
    }

    #[test]
    fn initializer_preserves_every_other_field() {
        let mut data = data();
        data[0x1d59] = 13;
        let before = data.clone();
        initialize(&mut data).unwrap();
        assert_eq!(data[0x1d59], 0);
        data[0x1d59] = 13;
        assert_eq!(data, before);
    }

    #[test]
    fn tick_and_retrace_gates_do_not_consume_random_or_store_rejected_ticks() {
        let mut data = data();
        let player = Player::at_alley_floor(100);
        let before = data.clone();
        assert_eq!(advance(&mut data, &player, 0, true).unwrap(), None);
        assert_eq!(advance(&mut data, &player, 1, false).unwrap(), None);
        assert_eq!(data, before);
        assert_eq!(advance(&mut data, &player, 1, true).unwrap(), None);
        assert_eq!(word(&data, 0x1d5a), 1);
        assert_eq!(word(&data, 0x2ae5), 0x0812);
    }

    #[test]
    fn forced_start_rejects_previous_index_and_crops_the_original_head() {
        let mut data = data();
        data[0x1028] = 2;
        let mut player = Player::at_alley_floor(8);
        player.y = 0x86;
        let update = advance(&mut data, &player, 1, true).unwrap().unwrap();
        // 1024 -> 0812 (previous bin) -> 8409 (bin 1) -> 4204 (jitter 4).
        assert_eq!(data[0x1028], 1);
        assert_eq!(word(&data, 0x2ae5), 0x4204);
        assert_eq!(word(&data, 0x1d5c), 94);
        assert_eq!(data[0x1d5e], 147);
        assert_eq!(data[0x1d5f], 158);
        assert!(!update.restore_previous);
        assert!(!update.contact_hold);
        assert_eq!(
            update.sprite,
            Some(VisibleSprite {
                source: 0x1cf0,
                dimensions: Dimensions(0x0102),
                offset: pixel_offset(94, 158),
            })
        );
    }

    #[test]
    fn external_difficulty_range_and_upper_bin_row_control_selection() {
        let mut data = data();
        set_word(&mut data, 8, 5);
        set_word(&mut data, 0x2ae5, 2);
        data[0x1006 + 5] = 3;
        data[0x100e + 5] = 0;
        data[0xff0 + 3] = 0x80 | 15;
        let mut player = Player::at_alley_floor(8);
        player.y = 0x8e;
        let update = advance(&mut data, &player, 1, true).unwrap().unwrap();
        // The first candidate, 1, exceeds the allowed relative index 0.
        assert_eq!(data[0x1028], 3);
        assert_eq!(word(&data, 0x2ae5), 0x2000);
        assert_eq!(word(&data, 0x1d5c), 66);
        assert_eq!(data[0x1d5e], 139);
        assert_eq!(data[0x1d5f], 150);
        assert_eq!(update.sprite.unwrap().offset, pixel_offset(66, 150));
    }

    #[test]
    fn full_animation_has_two_full_height_frames_and_finishes_with_restore() {
        let mut data = data();
        active(&mut data, 27, 100, 147, 0);
        let player = Player::at_alley_floor(8);
        let random = word(&data, 0x2ae5);
        for tick in 1..=27 {
            let update = advance(&mut data, &player, tick, true).unwrap().unwrap();
            let count = 27 - tick;
            let height = count.min(27 - count);
            assert_eq!(word(&data, 0x1d64), (height << 8) | 2);
            assert_eq!(data[0x1d5f], 159 - height as u8);
            assert_eq!(update.restore_previous, count != 26);
            assert_eq!(update.sprite.is_some(), count != 0);
        }
        assert_eq!(word(&data, 0x2ae5), random);
    }

    #[test]
    fn contact_is_inclusive_saturating_and_only_sets_alert() {
        let mut data = data();
        active(&mut data, 13, 100, 140, 13);
        let mut player = Player::at_alley_floor(100);
        for (x, y, expected) in [
            (76, 126, true),
            (116, 153, true),
            (75, 140, false),
            (117, 140, false),
            (100, 125, false),
            (100, 154, false),
        ] {
            player.x = x;
            player.y = y;
            data[0x1d58] = 0;
            let before = data.clone();
            assert_eq!(contact(&mut data, &player), expected);
            assert_eq!(data[0x1d58], u8::from(expected));
            data[0x1d58] = 0;
            assert_eq!(data, before);
        }
        active(&mut data, 1, 8, 8, 1);
        player.x = 0;
        player.y = 0;
        assert!(contact(&mut data, &player));
        active(&mut data, 1, 0xfff8, 250, 13);
        assert!(!contact(&mut data, &player));
        data[0x1d59] = 0;
        assert!(!contact(&mut data, &player));
    }

    #[test]
    fn contact_before_and_after_geometry_have_distinct_restore_events() {
        let mut data = data();
        active(&mut data, 13, 100, 140, 13);
        let mut player = Player::at_alley_floor(100);
        player.y = 140;
        let before = data.clone();
        let update = advance(&mut data, &player, 1, true).unwrap().unwrap();
        assert_eq!(
            update,
            Update {
                restore_previous: false,
                sprite: None,
                contact_hold: true,
            }
        );
        assert_eq!(data[0x1d59], before[0x1d59]);
        assert_eq!(word(&data, 0x1d5a), 1);
        assert_eq!(word(&data, 0x2ae5), word(&before, 0x2ae5));
        // At count 25 the rising head's top moves from 158 to 157. Contact
        // against player Y=143 appears only after that geometry update.
        active(&mut data, 26, 100, 147, 1);
        data[0x1d5f] = 158;
        player.y = 143;
        assert!(!contact(&mut data, &player));
        let update = advance(&mut data, &player, 2, true).unwrap().unwrap();
        assert!(update.restore_previous && update.contact_hold);
        assert_eq!(update.sprite, None);
        assert_eq!(data[0x1d59], 25);
    }

    #[test]
    fn response_updates_control_latches_with_one_random_draw() {
        let mut data = data();
        let controls = response_controls(&mut data);
        assert_eq!(controls, Controls { x: -1, y: 1 });
        assert_eq!(word(&data, 0x2ae5), 0x0812);
        assert_eq!((data[0x698], data[0x699], data[0x56c]), (255, 1, 1));
        assert_eq!(response_controls(&mut data), Controls { x: 1, y: 1 });
        assert_eq!(word(&data, 0x2ae5), 0x8409);
        clear_response_latch(&mut data);
        assert_eq!(data[0x56c], 0);
        assert_eq!((data[0x698], data[0x699]), (1, 1));
    }
}
