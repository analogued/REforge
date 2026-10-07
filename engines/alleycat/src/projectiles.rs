// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Window projectile launch, falling sprites and the timed first-hit contract.
//! The caller owns background composition and pauses gameplay during the effect.

use crate::assets::{DATA_LENGTH, next_random, set_word, word};
use crate::cga::{Dimensions, pixel_offset};
use crate::player::Player;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VisibleSprite {
    pub source: u16,
    pub dimensions: Dimensions,
    pub offset: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Contact {
    #[default]
    None,
    RepeatedHit,
    /// CS:1166 draws this image, waits ten BIOS ticks, then finishes the hit.
    FirstHit {
        effect: VisibleSprite,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Update {
    pub restore_previous: bool,
    pub sprite: Option<VisibleSprite>,
    pub contact: Contact,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else {
        Ok(())
    }
}

/// CS:1a05..1a75. Called only inside the window's unexpired phase-15 hold.
/// Spawning does not reset the projectile tick or draw its first sprite.
pub(crate) fn launch(data: &mut [u8], player: &Player) -> Result<bool, String> {
    check_data(data)?;
    if data[0x1670] == 0 || data[0x1673] != 0 || data[0x418] != 0 {
        return Ok(false);
    }
    data[0x1670] = data[0x1670].wrapping_sub(1);
    data[0x1678] = 1;
    data[0x1673] = data[0x1668];
    let x = word(data, 0x1666).wrapping_add(next_random(data) & 15);
    set_word(data, 0x1671, x);
    data[0x1674] = if x < player.x { 1 } else { 0xff };
    let index = usize::from(next_random(data) & 6);
    let source = word(data, 0x17c9 + index);
    let dimensions = word(data, 0x17d1 + index);
    set_word(data, 0x17dd, source);
    set_word(data, 0x17df, dimensions);
    data[0x1676] = data[0x17d9 + index / 2];
    set_word(data, 0x17ea, 32);
    data[0x17e9] = 1;
    data[0x1675] = 0;
    Ok(true)
}

/// CS:1b7a/2e29. The source height, not the currently cropped height, is used.
pub(crate) fn overlaps(data: &[u8], player: &Player) -> bool {
    if data.len() < DATA_LENGTH || word(data, 4) != 0 || data[0x1673] == 0 {
        return false;
    }
    let x = word(data, 0x1671);
    let y = data[0x1673];
    let height = (word(data, 0x17df) >> 8) as u8;
    x.wrapping_add(16) >= player.x
        && x.saturating_sub(24) <= player.x
        && y.wrapping_add(height) >= player.y
        && y.saturating_sub(14) <= player.y
}

/// Begin CS:1b7a. Damage suppression bypasses the complete response, allowing
/// overlap while ordinary movement and drawing continue. A damaging overlap
/// restores both images in the caller and starts CS:10dd's forced fall.
pub(crate) fn contact(
    data: &mut [u8],
    player: &Player,
    tick: u16,
    damage_enabled: bool,
) -> Contact {
    if !damage_enabled || !overlaps(data, player) {
        return Contact::None;
    }
    for (offset, value) in [
        (0x55c, 0),
        (0x571, 1),
        (0x576, 2),
        (0x578, 1),
        (0x577, 0xff),
        (0x56e, 0),
        (0x55a, 1),
        (0x550, 2),
    ] {
        data[offset] = value;
    }
    let source = word(data, 0xfac);
    set_word(data, 0x569, source);
    let source = word(data, 0xfb8);
    set_word(data, 0x567, source);
    if data[0x1675] != 0 {
        return Contact::RepeatedHit;
    }
    data[0x1675] = 1;
    let offset = pixel_offset(player.x.saturating_sub(12).min(270), player.y);
    set_word(data, 0x581, offset);
    set_word(data, 0x57f, tick);
    set_word(data, 0x5a3c, 0);
    set_word(data, 0x5a3e, 0);
    Contact::FirstHit {
        effect: VisibleSprite {
            source: 0x1679,
            dimensions: Dimensions(0x1205),
            offset,
        },
    }
}

/// Finish CS:1166 and the first-hit continuation of CS:1b7a after restoring
/// the effect background. The caller must wait tick.wrapping_sub(57f) >= 10.
/// Repeated contacts do not call this helper or charge another life.
pub(crate) fn finish_contact(data: &mut [u8]) -> Result<bool, String> {
    check_data(data)?;
    data[0x583] = 0;
    let lost_life = data[0x1678] != 0 && data[0x1f80] != 0;
    if lost_life {
        data[0x1f80] -= 1;
    }
    data[0x1674] = if data[0x1674] == 0xff { 1 } else { 0xff };
    set_word(data, 0x17ea, 96);
    data[0x17e9] = 1;
    data[0x55c] = 0;
    Ok(lost_life)
}

/// CS:184b, independently gated by a changed BIOS tick without a retrace gate.
/// CS:1922 background restoration is reported separately; the caller records
/// the saved image position/dimensions at 17e5/17e3 when a sprite is drawn.
pub(crate) fn advance(
    data: &mut [u8],
    player: &Player,
    tick: u16,
    damage_enabled: bool,
) -> Result<Option<Update>, String> {
    check_data(data)?;
    if data[0x1673] == 0 || tick == word(data, 0x17ec) {
        return Ok(None);
    }
    set_word(data, 0x17ec, tick);
    let mut x = word(data, 0x1671);
    if data[0x1677] != 0 && x & 0xfff8 == player.x & 0xfff8 {
        data[0x1674] = 0;
    }
    data[0x17e9] = data[0x17e9].wrapping_add(1);
    let mut delay = word(data, 0x17ea);
    if delay <= 1 {
        delay = delay.wrapping_sub(1);
        set_word(data, 0x17ea, delay);
    }
    // The original shifts DL alone, leaving DH unchanged.
    let step = (delay & 0xff00) | u16::from((delay as u8) >> 3);
    x = match data[0x1674] {
        0 => x,
        1 => {
            let next = x.wrapping_add(step);
            if next >= 303 { 302 } else { next }
        }
        _ => x.saturating_sub(step),
    };
    set_word(data, 0x1671, x);
    let y = data[0x1673].wrapping_add(data[0x17e9] >> 1);
    let mut dimensions = word(data, 0x17df);
    if let Some(crop) = y.checked_sub(data[0x1676]) {
        let height = (dimensions >> 8) as u8;
        if crop >= height {
            data[0x1673] = 0;
            data[0x1678] = 0;
            return Ok(Some(Update {
                restore_previous: true,
                sprite: None,
                contact: Contact::None,
            }));
        }
        dimensions = (u16::from(height - crop) << 8) | (dimensions & 0xff);
    }
    data[0x1673] = y;
    let offset = pixel_offset(x, y);
    set_word(data, 0x17e1, dimensions);
    set_word(data, 0x17e7, offset);
    let hit = contact(data, player, tick, damage_enabled);
    Ok(Some(Update {
        restore_previous: data[0x17e9] != 2 || hit != Contact::None,
        sprite: if hit == Contact::None {
            Some(VisibleSprite {
                source: word(data, 0x17dd),
                dimensions: Dimensions(dimensions),
                offset,
            })
        } else {
            None
        },
        contact: hit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0x1024);
        set_word(&mut data, 0x1666, 80);
        data[0x1668] = 32;
        data[0x1670] = 1;
        for index in 0..4 {
            set_word(&mut data, 0x17c9 + index * 2, 0x2000 + index as u16 * 0x100);
            set_word(&mut data, 0x17d1 + index * 2, 0x0c02);
            data[0x17d9 + index] = 100;
        }
        data
    }

    fn active(data: &mut [u8], x: u16, y: u8, phase: u8) {
        set_word(data, 0x1671, x);
        data[0x1673] = y;
        data[0x1674] = 1;
        data[0x1676] = 100;
        data[0x17e9] = phase;
        set_word(data, 0x17ea, 32);
        set_word(data, 0x17df, 0x0c02);
        set_word(data, 0x17dd, 0x2000);
    }

    #[test]
    fn launch_consumes_exactly_two_draws_and_reads_external_type_tables() {
        let mut data = data();
        let player = Player::at_alley_floor(100);
        set_word(&mut data, 0x17ec, 88);
        assert!(launch(&mut data, &player).unwrap());
        assert_eq!(word(&data, 0x2ae5), 0x8409);
        assert_eq!(word(&data, 0x1671), 82);
        assert_eq!(word(&data, 0x17dd), 0x2000);
        assert_eq!((data[0x1674], data[0x17e9], data[0x1678]), (1, 1, 1));
        assert_eq!(word(&data, 0x17ec), 88);
        assert_eq!(data[0x1670], 0);
    }

    #[test]
    fn launch_gates_are_state_and_random_neutral() {
        for (offset, value) in [(0x1670, 0), (0x1673, 1), (0x418, 1)] {
            let mut data = data();
            data[offset] = value;
            let before = data.clone();
            assert!(!launch(&mut data, &Player::at_alley_floor(100)).unwrap());
            assert_eq!(data, before);
        }
    }

    #[test]
    fn inactive_and_equal_tick_gates_preserve_every_field() {
        let mut data = data();
        let player = Player::at_alley_floor(250);
        let before = data.clone();
        assert_eq!(advance(&mut data, &player, 1, true).unwrap(), None);
        assert_eq!(data, before);
        active(&mut data, 80, 32, 1);
        let before = data.clone();
        assert_eq!(advance(&mut data, &player, 0, true).unwrap(), None);
        assert_eq!(data, before);
    }

    #[test]
    fn first_draw_skips_restore_then_acceleration_advances_each_tick() {
        let mut data = data();
        active(&mut data, 80, 32, 1);
        let player = Player::at_alley_floor(250);
        let first = advance(&mut data, &player, 1, true).unwrap().unwrap();
        assert!(!first.restore_previous);
        assert_eq!(first.sprite.unwrap().offset, pixel_offset(84, 33));
        let second = advance(&mut data, &player, 2, true).unwrap().unwrap();
        assert!(second.restore_previous);
        assert_eq!(second.sprite.unwrap().offset, pixel_offset(88, 34));
        assert_eq!(word(&data, 0x2ae5), 0x1024);
        assert_eq!(word(&data, 0x17ea), 32);
    }

    #[test]
    fn bottom_crop_retains_top_source_then_expires_and_clears_life_flag() {
        let mut data = data();
        active(&mut data, 80, 100, 1);
        data[0x1678] = 1;
        let player = Player::at_alley_floor(250);
        let first = advance(&mut data, &player, 1, true).unwrap().unwrap();
        assert_eq!(first.sprite.unwrap().dimensions, Dimensions(0x0b02));
        assert_eq!(first.sprite.unwrap().source, 0x2000);
        active(&mut data, 80, 111, 1);
        let last = advance(&mut data, &player, 2, true).unwrap().unwrap();
        assert!(last.restore_previous);
        assert_eq!(last.sprite, None);
        assert_eq!((data[0x1673], data[0x1678]), (0, 0));
    }

    #[test]
    fn targeted_projectile_stops_only_when_eight_pixel_cells_match() {
        let mut data = data();
        active(&mut data, 80, 32, 1);
        data[0x1677] = 1;
        let mut player = Player::at_alley_floor(87);
        advance(&mut data, &player, 1, false).unwrap();
        assert_eq!(word(&data, 0x1671), 80);
        assert_eq!(data[0x1674], 0);
        active(&mut data, 80, 32, 1);
        player.x = 88;
        advance(&mut data, &player, 2, false).unwrap();
        assert_eq!(word(&data, 0x1671), 84);
    }

    #[test]
    fn horizontal_edges_and_low_byte_shift_keep_native_wrapping() {
        let mut data = data();
        let player = Player::at_alley_floor(250);
        active(&mut data, 302, 32, 1);
        advance(&mut data, &player, 1, false).unwrap();
        assert_eq!(word(&data, 0x1671), 302);
        data[0x1674] = 0xff;
        set_word(&mut data, 0x1671, 2);
        advance(&mut data, &player, 2, false).unwrap();
        assert_eq!(word(&data, 0x1671), 0);
        set_word(&mut data, 0x17ea, 0);
        data[0x1674] = 1;
        advance(&mut data, &player, 3, false).unwrap();
        assert_eq!(word(&data, 0x17ea), 0xffff);
        assert_eq!(word(&data, 0x1671), 302);
    }

    #[test]
    fn overlap_is_inclusive_and_uses_full_sprite_height_and_scene_gate() {
        let mut data = data();
        active(&mut data, 80, 32, 1);
        set_word(&mut data, 0x17e1, 0x0102);
        let mut player = Player::at_alley_floor(100);
        for (x, y, hit) in [
            (56, 18, true),
            (96, 44, true),
            (55, 32, false),
            (97, 32, false),
            (80, 17, false),
            (80, 45, false),
        ] {
            player.x = x;
            player.y = y;
            assert_eq!(overlaps(&data, &player), hit);
        }
        player.x = 80;
        player.y = 32;
        set_word(&mut data, 4, 1);
        assert!(!overlaps(&data, &player));
    }

    #[test]
    fn protected_overlap_preserves_player_and_latch_and_draws_normally() {
        let mut data = data();
        active(&mut data, 80, 32, 1);
        let mut player = Player::at_alley_floor(80);
        player.y = 32;
        let before = data.clone();
        assert_eq!(contact(&mut data, &player, 1, false), Contact::None);
        assert_eq!(data, before);
        let update = advance(&mut data, &player, 1, false).unwrap().unwrap();
        assert_eq!(update.contact, Contact::None);
        assert!(update.sprite.is_some());
        assert_eq!(data[0x1675], 0);
        assert_eq!(&data[0x550..0x5a4], &before[0x550..0x5a4]);
    }

    #[test]
    fn first_hit_defers_life_and_reversal_until_effect_finishes() {
        let mut data = data();
        active(&mut data, 80, 32, 1);
        data[0x1678] = 1;
        data[0x1f80] = 3;
        let mut player = Player::at_alley_floor(80);
        player.y = 32;
        let result = contact(&mut data, &player, 0xfffc, true);
        assert_eq!(
            result,
            Contact::FirstHit {
                effect: VisibleSprite {
                    source: 0x1679,
                    dimensions: Dimensions(0x1205),
                    offset: pixel_offset(68, 32),
                }
            }
        );
        assert_eq!((data[0x1675], data[0x1f80], data[0x1674]), (1, 3, 1));
        assert_eq!(word(&data, 0x57f), 0xfffc);
        assert_eq!(
            (data[0x55a], data[0x571], data[0x576], data[0x577]),
            (1, 1, 2, 255)
        );
        assert!(finish_contact(&mut data).unwrap());
        assert_eq!((data[0x1f80], data[0x1674], data[0x17e9]), (2, 255, 1));
        assert_eq!(word(&data, 0x17ea), 96);
        assert_eq!(contact(&mut data, &player, 6, true), Contact::RepeatedHit);
        assert_eq!(data[0x1f80], 2);
    }

    #[test]
    fn life_charge_respects_flag_and_zero_count_and_effect_clamps_x() {
        for (flag, lives) in [(0, 3), (1, 0)] {
            let mut data = data();
            active(&mut data, 0, 32, 1);
            data[0x1678] = flag;
            data[0x1f80] = lives;
            let mut player = Player::at_alley_floor(0);
            player.y = 32;
            assert!(
                matches!(contact(&mut data, &player, 1, true), Contact::FirstHit { effect: VisibleSprite { offset, .. } } if offset == pixel_offset(0, 32))
            );
            assert!(!finish_contact(&mut data).unwrap());
            assert_eq!(data[0x1f80], lives);
        }
    }
}
