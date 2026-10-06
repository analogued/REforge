// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Alley window animation and entry, using the external executable's tables.
//!
//! The caller supplies the player and scenery framebuffer. Projectiles are
//! deliberately disabled: their launch block and random draws are omitted,
//! while the window's original opening, timed hold and closing remain intact.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};
use crate::player::Player;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Update {
    /// The invocation countdown expired and the retrace-clear gate passed.
    pub accepted: bool,
    pub drawn_row: Option<u8>,
    pub contact_hold: bool,
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
pub(crate) fn advance(
    data: &mut [u8],
    screen: &mut Framebuffer,
    player: &Player,
    tick: u16,
    retrace: bool,
) -> Result<Update, String> {
    check_data(data)?;
    data[0x166a] = data[0x166a].wrapping_sub(1);
    if data[0x166a] != 0 {
        return Ok(Update::default());
    }
    data[0x166a] = 13;
    if retrace {
        return Ok(Update::default());
    }
    let mut update = Update {
        accepted: true,
        ..Update::default()
    };
    if data[0x1665] != 0 {
        contact(data, player);
    }
    if data[0x1673] != 0 {
        return Ok(update);
    }
    if data[0x1665] == 0 {
        if player.y > 96 {
            return Ok(update);
        }
        let difficulty = usize::from(word(data, 8));
        if difficulty > 7 {
            return Err("Window difficulty exceeds its timing table".into());
        }
        data[0x1677] = u8::from(
            player.band_state == 1
                && data[0x418] == 0
                && tick.wrapping_sub(word(data, 0x556)) >= 72,
        );
        // A complete generator period bounds diagnostics with malformed tables
        // or zero seed; supported original states always offer a free window.
        let mut selected = false;
        for _ in 0..=u16::MAX {
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
        let delay = word(data, 0x181e + difficulty * 2);
        set_word(data, 0x166c, delay);
        data[0x1670] = 1;
    }
    if contact(data, player) {
        update.contact_hold = true;
        return Ok(update);
    }
    data[0x1664] = u8::from(mouse_overlap(data));
    if data[0x1664] != 0 {
        update.contact_hold = true;
        return Ok(update);
    }
    if data[0x1665] == 16 {
        set_word(data, 0x166e, tick);
    }
    if data[0x1665] == 15 && tick.wrapping_sub(word(data, 0x166e)) < word(data, 0x166c) {
        // CS:1a05..1a75's projectile launch is deliberately excluded.
        return Ok(update);
    }
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
        screen.copy_rect(offset, Dimensions(0x0104), &[0; 8])?;
    } else {
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

/// CS:01e5..0238 ordinary selection and forced-bonus override. This helper
/// retains the original choices even if the caller currently supports fewer
/// rooms. It does not save the alley return position; that precedes selection.
pub(crate) fn choose_room(data: &mut [u8]) -> Result<u16, String> {
    check_data(data)?;
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
            let first = draw(data)?;
            let candidate = if first & 0xa0 != 0 && difficulty != 3 {
                data[0x421 + difficulty * 4 + usize::from(first & 3)]
            } else {
                let index = loop {
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
        };
        set_word(data, 0x41f, word(data, 0x41d));
        set_word(data, 0x41d, candidate);
        candidate
    };
    set_word(data, 4, selected);
    set_word(data, 6, 0);
    Ok(selected)
}

/// CS:01b7..0238 saves the contact position and clock before selecting a room.
pub(crate) fn select_room(data: &mut [u8], player: &Player, tick: u16) -> Result<u16, String> {
    check_data(data)?;
    set_word(data, 0x410, tick);
    set_word(data, 1, player.x);
    data[3] = player.y;
    data[0x419] = 1;
    choose_room(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0x40);
        set_word(&mut data, 0x41d, 0xffff);
        set_word(&mut data, 0x41f, 0xffff);
        for column in 0..4 {
            set_word(&mut data, 0x1658 + column * 2, 24 + column as u16 * 80);
        }
        data[0x1660..0x1664].copy_from_slice(&[24, 56, 88, 24]);
        data[0x421..0x432].copy_from_slice(&[4, 4, 1, 1, 3, 3, 5, 5, 6, 6, 4, 1, 1, 3, 4, 5, 6]);
        for difficulty in 0..8 {
            set_word(
                &mut data,
                0x181e + difficulty * 2,
                45 - (difficulty.min(4) as u16 * 9),
            );
        }
        data[0x166a] = 1;
        data
    }

    fn active(data: &mut [u8], phase: u8) {
        data[0x1665] = phase;
        data[0x1669] = 8;
        set_word(data, 0x1666, 104);
        data[0x1668] = 88;
        data[0x166a] = 1;
    }

    #[test]
    fn initializer_changes_only_the_original_reset_fields() {
        let mut data = vec![0xab; DATA_LENGTH];
        let mut expected = data.clone();
        for offset in [0x1665, 0x1673, 0x1677, 0x1678] {
            expected[offset] = 0;
        }
        set_word(&mut expected, 0x166c, 9);
        initialize(&mut data).unwrap();
        assert_eq!(data, expected);
        assert_eq!(data[0x166a], 0xab);
    }

    #[test]
    fn countdown_wraps_and_retrace_rejection_still_resets_it() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let player = Player::at_alley_floor(8);
        data[0x166a] = 0;
        assert!(
            !advance(&mut data, &mut screen, &player, 1, false)
                .unwrap()
                .accepted
        );
        assert_eq!(data[0x166a], 255);
        data[0x166a] = 1;
        let seed = word(&data, 0x2ae5);
        assert!(
            !advance(&mut data, &mut screen, &player, 1, true)
                .unwrap()
                .accepted
        );
        assert_eq!(data[0x166a], 13);
        assert_eq!(word(&data, 0x2ae5), seed);
        assert!(
            advance(&mut data, &mut screen, &player, 1, false)
                .unwrap()
                .drawn_row
                .is_none()
        );
        assert_eq!(data[0x166a], 12);
    }

    #[test]
    fn inactive_selection_rejects_high_player_or_id_without_creating_a_window() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let mut player = Player::at_alley_floor(8);
        let seed = word(&data, 0x2ae5);
        assert!(
            advance(&mut data, &mut screen, &player, 1, false)
                .unwrap()
                .accepted
        );
        assert_eq!(word(&data, 0x2ae5), seed);
        assert_eq!(data[0x1665], 0);
        player.y = 96;
        data[0x166a] = 1;
        set_word(&mut data, 0x2ae5, 0x18);
        assert_eq!(
            advance(&mut data, &mut screen, &player, 1, false)
                .unwrap()
                .drawn_row,
            None
        );
        assert_eq!(word(&data, 0x2ae5), 12);
        assert_eq!(data[0x1665], 0);
        data[0x166a] = 1;
        set_word(&mut data, 0x2ae5, 0x40);
        let update = advance(&mut data, &mut screen, &player, 1, false).unwrap();
        assert_eq!(data[0x1665], 28);
        assert_eq!(data[0x1669], 0);
        assert_eq!(update.drawn_row, Some(38));
        assert_eq!(word(&data, 0x166c), 45);
        assert_eq!(data[0x1670], 1);
    }

    #[test]
    fn long_band96_wait_limits_selection_to_top_row_unless_bonus_is_pending() {
        for (tick, bonus, expected_id) in [(71, 0, 8), (72, 0, 0), (72, 1, 8)] {
            let mut data = data();
            set_word(&mut data, 0x2ae5, 0x10);
            data[0x418] = bonus;
            let mut player = Player::at_alley_floor(64);
            player.y = 96;
            player.band_state = 1;
            let mut screen = Framebuffer::default();
            advance(&mut data, &mut screen, &player, tick, false).unwrap();
            assert_eq!(data[0x1669], expected_id);
            assert_eq!(data[0x1677], u8::from(tick == 72 && bonus == 0));
            assert_eq!(word(&data, 0x2ae5), 8);
        }
    }

    #[test]
    fn entry_gate_preserves_overlap_carry_and_only_writes_eligible_flag() {
        let mut data = data();
        active(&mut data, 15);
        let mut player = Player::at_alley_floor(104);
        player.y = 88;
        player.vertical_direction = 1;
        for (phase, direction, forced, y, expected) in [
            (4, 1, 0, 88, false),
            (5, 1, 0, 88, true),
            (24, 1, 0, 88, true),
            (25, 1, 0, 88, false),
            (15, 0, 0, 88, false),
            (15, 255, 0, 88, false),
            (15, 1, 1, 88, false),
            (15, 1, 0, 95, true),
            (15, 1, 0, 96, false),
        ] {
            data[0x1665] = phase;
            data[0x55a] = forced;
            data[0x551] = 0;
            player.y = y;
            player.vertical_direction = direction;
            let mut expected_data = data.clone();
            expected_data[0x551] = u8::from(expected);
            assert!(contact(&mut data, &player));
            assert_eq!(data, expected_data);
        }
        for (x, y, hit) in [
            (80, 74, true),
            (136, 103, true),
            (79, 88, false),
            (137, 88, false),
            (104, 73, false),
            (104, 104, false),
        ] {
            player.x = x;
            player.y = y;
            assert_eq!(contact(&mut data, &player), hit);
        }
        set_word(&mut data, 0x1666, 0xfff0);
        assert!(
            !contact(&mut data, &player),
            "upper addition wraps before comparison"
        );
    }

    #[test]
    fn opening_and_closing_change_exactly_one_eight_byte_row_per_step() {
        let mut data = data();
        active(&mut data, 29);
        set_word(&mut data, 0x166c, 9);
        let player = Player::at_alley_floor(8);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        for row in 0..15 {
            data[0x2681 + row * 10..0x2681 + row * 10 + 8].fill(row as u8 + 1);
        }
        let seed = word(&data, 0x2ae5);
        for phase in (16..=29).rev() {
            data[0x166a] = 1;
            let update = advance(&mut data, &mut screen, &player, 65534, false).unwrap();
            assert_eq!(data[0x1665], phase - 1);
            assert_eq!(update.drawn_row, Some(88 + phase - 15));
        }
        assert_eq!(word(&data, 0x166e), 65534);
        data[0x166a] = 1;
        let before = screen.bytes().to_vec();
        assert_eq!(
            advance(&mut data, &mut screen, &player, 2, false)
                .unwrap()
                .drawn_row,
            None
        );
        assert_eq!(screen.bytes().as_slice(), before);
        assert_eq!(data[0x1673], 0);
        for phase in (1..=15).rev() {
            data[0x166a] = 1;
            let update = advance(&mut data, &mut screen, &player, 50, false).unwrap();
            let row = 15 - phase;
            assert_eq!(update.drawn_row, Some(88 + row));
            assert_eq!(
                screen
                    .read_rect(pixel_offset(104, 88 + row), Dimensions(0x0104))
                    .unwrap(),
                vec![row + 1; 8]
            );
        }
        assert_eq!(data[0x1665], 0);
        assert_eq!(word(&data, 0x2ae5), seed);
    }

    #[test]
    fn player_and_mouse_holds_preserve_phase_and_soundless_shared_seed() {
        let mut data = data();
        active(&mut data, 15);
        let mut player = Player::at_alley_floor(104);
        player.y = 88;
        player.vertical_direction = 1;
        let mut screen = Framebuffer::default();
        let seed = word(&data, 0x2ae5);
        let update = advance(&mut data, &mut screen, &player, 100, false).unwrap();
        assert!(update.contact_hold);
        assert_eq!(data[0x551], 1);
        assert_eq!(data[0x1665], 15);
        player.y = 180;
        active(&mut data, 29);
        data[0x1669] = 0;
        set_word(&mut data, 0x1f32, 104);
        data[0x1f51] = 1; // Inactivity does not exempt this mouse rectangle.
        let update = advance(&mut data, &mut screen, &player, 100, false).unwrap();
        assert!(update.contact_hold);
        assert_eq!((data[0x1665], data[0x1664]), (29, 1));
        assert_eq!(word(&data, 0x2ae5), seed);
    }

    #[test]
    fn bonus_opening_uses_external_picture_but_closing_uses_wall() {
        let mut data = data();
        active(&mut data, 29);
        data[0x418] = 1;
        data[0x15e0 + 14 * 8..0x15e0 + 15 * 8].fill(0x35);
        let mut screen = Framebuffer::default();
        let player = Player::at_alley_floor(8);
        advance(&mut data, &mut screen, &player, 1, false).unwrap();
        assert_eq!(
            screen
                .read_rect(pixel_offset(104, 102), Dimensions(0x0104))
                .unwrap(),
            [0x35; 8]
        );
        active(&mut data, 1);
        data[0x2681 + 14 * 10..0x2681 + 14 * 10 + 8].fill(0x72);
        advance(&mut data, &mut screen, &player, 1, false).unwrap();
        assert_eq!(
            screen
                .read_rect(pixel_offset(104, 102), Dimensions(0x0104))
                .unwrap(),
            [0x72; 8]
        );
    }

    #[test]
    fn selector_retains_weighted_fallback_and_two_entry_history_contracts() {
        for (seed, difficulty, latest, previous, room, final_seed) in [
            (0x40, 0, 0xffff, 0xffff, 4, 0x20),
            (0x40, 1, 0xffff, 0xffff, 3, 0x20),
            (0x40, 2, 0xffff, 0xffff, 6, 0x20),
            (0x44, 0, 0xffff, 0xffff, 1, 0x22),
            (0x44, 1, 0xffff, 0xffff, 5, 0x22),
            (0x40, 3, 0xffff, 0xffff, 1, 0x10),
            (0x14, 0, 0xffff, 0xffff, 4, 0x4002),
            (0x40, 0, 4, 4, 1, 8),
            (0x40, 0, 4, 3, 4, 0x20),
        ] {
            let mut data = data();
            set_word(&mut data, 0x2ae5, seed);
            set_word(&mut data, 8, difficulty);
            set_word(&mut data, 0x41d, latest);
            set_word(&mut data, 0x41f, previous);
            assert_eq!(choose_room(&mut data).unwrap(), room);
            assert_eq!(word(&data, 0x2ae5), final_seed);
            assert_eq!(
                (
                    word(&data, 4),
                    word(&data, 6),
                    word(&data, 0x41d),
                    word(&data, 0x41f)
                ),
                (room, 0, room, latest)
            );
        }
    }

    #[test]
    fn bonus_selection_saves_return_state_without_random_or_history_changes() {
        let mut data = data();
        data[0x418] = 1;
        let player = Player::at_alley_floor(110);
        let seed = word(&data, 0x2ae5);
        let history = data[0x41d..0x421].to_vec();
        assert_eq!(select_room(&mut data, &player, 65535).unwrap(), 7);
        assert_eq!(word(&data, 0x410), 65535);
        assert_eq!(
            (word(&data, 1), data[3], data[0x419], data[0x418]),
            (110, 180, 1, 0)
        );
        assert_eq!(word(&data, 0x2ae5), seed);
        assert_eq!(data[0x41d..0x421], history);
    }
}
