// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Shared ground-dog motion, edge animation and delayed pursuit consequences.
//!
//! Sprite pixels and probability tables come from the external executable.
//! The caller owns compositing and schedules scene 6 only during its pursuit.

use crate::assets::{DATA_LENGTH, next_random, set_word, word};
use crate::framebuffer::{Dimensions, pixel_offset};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisibleSprite {
    pub dimensions: Dimensions,
    pub offset: u16,
    pub pixels: Vec<u8>,
    /// Pursuit pictures include their background and use an opaque blit.
    pub opaque: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Update {
    /// A replacement presentation image; `None` removes the previous image.
    pub sprite: Option<VisibleSprite>,
    pub contact: bool,
    pub started_death: bool,
    pub finished_death: bool,
    pub failure: bool,
    pub life_lost: bool,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else if word(data, 8) >= 8 {
        Err("Ground-dog difficulty must be in 0..8".into())
    } else {
        Ok(())
    }
}

/// CS:1e40 and its logical sound initializer CS:5450. X, tick baseline,
/// animation phase, direction and the bin-cat alert deliberately survive.
pub(crate) fn initialize(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    for offset in [0x1cbf, 0x1cc0, 0x1cc1, 0x1cb8] {
        data[offset] = 0;
    }
    set_word(data, 0x1ce1, 0);
    data[0x1cc8] = 177;
    data[0x5b0f] = 12;
    set_word(data, 0x5b0c, 1);
    set_word(data, 0x5b12, 0x1ff);
    set_word(data, 0x5b0a, 15);
    data[0x5b0e] = 1;
    Ok(())
}

/// CS:20f5's inclusive contact gate, without starting its consequence.
pub(crate) fn contact(data: &[u8]) -> bool {
    if data.len() < DATA_LENGTH
        || data[0x1cb8] != 0
        || (data[0x1cbf] | data[0x1cc0] | data[0x1cc1]) == 0
        || data[0x57b] < 163
        || data[0x558] != 0
    {
        return false;
    }
    let x = word(data, 0x1cc6);
    let upper = x.wrapping_add(32);
    upper >= word(data, 0x579) && upper.saturating_sub(56) <= word(data, 0x579)
}

/// CS:2136 state changes. The caller removes the old dog/player presentation
/// before using the relocated player. Scene 6 first positions the dog at the
/// sleeping-dog contact; other scenes use the running-dog midpoint.
pub(crate) fn begin_death(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    if word(data, 4) == 6 {
        data[0x1cc8] = data[0x57b];
        set_word(data, 0x1cc6, word(data, 0x579));
        set_word(data, 0x1cc4, 0x0f04);
        set_word(data, 0x1cbb, word(data, 0x15c8));
        set_word(data, 0x1cc2, 0x0f04);
        set_word(data, 0x1cbd, pixel_offset(word(data, 0x1cc6), data[0x1cc8]));
    }
    let midpoint = (word(data, 0x1cc6).wrapping_add(word(data, 0x579)) >> 1).min(279);
    set_word(data, 0x1cc6, midpoint);
    if word(data, 4) == 6 {
        set_word(data, 0x1cbd, pixel_offset(midpoint, data[0x1cc8]));
    }
    let (direction, distance) = if midpoint <= 160 {
        (0xff, 161 - midpoint)
    } else {
        (1, midpoint - 159)
    };
    data[0x1cb8] = direction;
    data[0x1cbf] = 1;
    data[0x1cc1] = 0;
    set_word(data, 0x1cb9, distance >> 3);
    let relocated = if word(data, 0x579) < 160 { 290 } else { 0 };
    set_word(data, 0x579, relocated);
    if word(data, 4) == 0 {
        initialize_alley_entry(data);
    }
    Ok(())
}

fn initialize_alley_entry(data: &mut [u8]) {
    let (x, direction) = if word(data, 0x579) < 160 {
        (0, 1)
    } else {
        (296, 0xff)
    };
    set_word(data, 0x579, x);
    data[0x56e] = direction;
    data[0x558] = 3;
    data[0x559] = 12;
    data[0x57b] = 180;
    data[0x57c] = 230;
    set_word(data, 0x55f, pixel_offset(x, 180));
    set_word(data, 0x561, 0x0b03);
    data[0x571] = 0;
    set_word(data, 0x572, 2);
    data[0x576] = 1;
    for offset in [
        0x55b, 0x550, 0x55c, 0x55a, 0x583, 0x698, 0x699, 0x551, 0x584, 0x552, 0x553, 0x127c,
    ] {
        data[offset] = 0;
    }
    set_word(data, 0x554, 0);
    set_word(data, 0x57d, 0);
    set_word(data, 0x684, 0);
}

pub(crate) fn select_sprite(data: &mut [u8]) {
    let index = if data[0x1cb8] != 0 {
        data[0x1ccf] = data[0x1ccf].wrapping_add(1);
        usize::from((data[0x1ccf] & 6) | 8)
    } else {
        data[0x1ccf] = data[0x1ccf].wrapping_add(2);
        usize::from((data[0x1ccf] & 2) | if data[0x1cd0] == 1 { 4 } else { 0 })
    };
    set_word(data, 0x1cbb, word(data, 0x15c8 + index));
}

pub(crate) fn prepare_edge_sprite(data: &mut [u8], crop: u8, side: u8) -> Result<(), String> {
    if crop >= 4 {
        return Err("Ground-dog edge crop must leave a visible column".into());
    }
    let dimensions = Dimensions(0x0f00 | u16::from(4 - crop));
    set_word(data, 0x1cc4, dimensions.0);
    let source = if side == 0xff {
        set_word(data, 0x1cc6, 288 + u16::from(crop) * 8);
        word(data, 0x1cbb)
    } else {
        let source = word(data, 0x1cbb).wrapping_add(u16::from(crop) * 2);
        set_word(data, 0x1cbb, source);
        set_word(data, 0x1cc6, 0);
        source
    };
    let width = dimensions.width_bytes();
    for row in 0..15 {
        let source = usize::from(source) + row * 8;
        let pixels = data
            .get(source..source + width)
            .ok_or("Ground-dog sprite exceeds executable data")?
            .to_vec();
        let destination = 0x0e + row * width;
        data[destination..destination + width].copy_from_slice(&pixels);
    }
    // Original CS:2d70 scratch, retained for helper comparisons.
    set_word(data, 0x2ae9, source.wrapping_add(120));
    data[0x2ae0] = 4 - crop;
    data[0x2ae2] = 0;
    data[0x2aeb] = 8;
    set_word(data, 0x1cbb, 0x0e);
    Ok(())
}

fn visible_sprite(data: &[u8]) -> Result<VisibleSprite, String> {
    let dimensions = Dimensions(word(data, 0x1cc4));
    let source = usize::from(word(data, 0x1cbb));
    if dimensions.is_empty() {
        return Err("Ground-dog sprite has empty dimensions".into());
    }
    let pixels = data
        .get(source..source + dimensions.len())
        .ok_or("Ground-dog sprite exceeds executable data")?
        .to_vec();
    Ok(VisibleSprite {
        dimensions,
        offset: word(data, 0x1cbd),
        pixels,
        opaque: data[0x1cb8] != 0,
    })
}

/// CS:1e63. `None` means its independent BIOS-tick/retrace gate rejected the
/// call. Suppression bypasses the lethal contact sequence while retaining
/// ordinary movement, alert handling, animation, RNG and visible overlap.
pub(crate) fn advance(
    data: &mut [u8],
    tick: u16,
    retrace: bool,
    suppress_damage: bool,
) -> Result<Option<Update>, String> {
    check_data(data)?;
    if tick.wrapping_sub(word(data, 0x1cc9)) < (word(data, 0x1ce1) & 1) + 1 || !retrace {
        return Ok(None);
    }
    set_word(data, 0x1cc9, tick);
    set_word(data, 0x1ce1, word(data, 0x1ce1).wrapping_add(1));
    let mut update = Update::default();
    if data[0x1cc1] != 0 {
        data[0x1cc1] -= 1;
        if data[0x1cc1] == 0 {
            update.finished_death = data[0x1cb8] != 0;
            if update.finished_death && word(data, 4) != 0 {
                data[0x552] = 0xdd;
                set_word(data, 0x579, 160);
                data[0x57b] = 96;
                update.failure = true;
                return Ok(Some(update));
            }
            if update.finished_death && data[0x1f80] != 0 {
                data[0x1f80] -= 1;
                update.life_lost = true;
            }
            initialize(data)?;
            return Ok(Some(update));
        }
        select_sprite(data);
        let side = if data[0x1cd0] == 0xff { 1 } else { 0xff };
        prepare_edge_sprite(data, 4 - data[0x1cc1], side)?;
    } else if data[0x1cb8] == 0 {
        if data[0x1cbf] == 0 {
            if data[0x1cc0] == 0 {
                if data[0x1d58] == 0 {
                    if data[0x57b] < 180 || data[0x558] != 0 {
                        return Ok(Some(update));
                    }
                    if next_random(data) as u8 >= data[0x1cd1 + usize::from(word(data, 8))] {
                        return Ok(Some(update));
                    }
                }
                data[0x1cd0] = if word(data, 0x579) < 160 { 0xff } else { 1 };
                set_word(data, 0x59ba, 0);
                data[0x1cc0] = 4;
            }
            data[0x1cc0] -= 1;
            if data[0x1cc0] != 0 {
                select_sprite(data);
                prepare_edge_sprite(data, data[0x1cc0], data[0x1cd0])?;
            } else {
                data[0x1cbf] = 1;
                advance_position(data, false);
            }
        } else {
            advance_position(data, false);
        }
    } else {
        if word(data, 0x1cb9) != 0 {
            set_word(data, 0x1cb9, word(data, 0x1cb9) - 1);
            data[0x1cd0] = if next_random(data) & 1 != 0 { 1 } else { 0xff };
        } else {
            data[0x1cd0] = data[0x1cb8];
        }
        advance_position(data, true);
    }
    set_word(data, 0x1ccd, pixel_offset(word(data, 0x1cc6), data[0x1cc8]));
    update.contact = contact(data);
    if update.contact && !suppress_damage {
        begin_death(data)?;
        update.started_death = true;
    } else {
        set_word(data, 0x1cbd, word(data, 0x1ccd));
        set_word(data, 0x1cc2, word(data, 0x1cc4));
        update.sprite = Some(visible_sprite(data)?);
    }
    Ok(Some(update))
}

fn advance_position(data: &mut [u8], dying: bool) {
    if !dying {
        data[0x1d58] = 0;
        if data[0x57b] >= 180 && data[0x558] == 0 {
            let random = next_random(data) as u8;
            if random <= data[0x1cd9 + usize::from(word(data, 8))] {
                data[0x1cd0] = if word(data, 0x1cc6) <= word(data, 0x579) {
                    1
                } else {
                    0xff
                };
            }
        }
    }
    let x = word(data, 0x1cc6);
    let (x, edge) = match data[0x1cd0] {
        0 => (x, false),
        1 => {
            let next = x.wrapping_add(8);
            (next.min(286), next >= 286)
        }
        _ => (x.saturating_sub(8), x < 8),
    };
    if edge
        && if dying {
            word(data, 0x1cb9) == 0
        } else {
            data[0x57b] < 180 || data[0x558] != 0
        }
    {
        data[0x1cc1] = 4;
    }
    set_word(data, 0x1cc6, x);
    select_sprite(data);
    set_word(data, 0x1cc4, 0x0f04);
}

#[cfg(test)]
pub(crate) fn draw(
    data: &mut [u8],
    screen: &mut crate::framebuffer::Framebuffer,
) -> Result<(), String> {
    let sprite = visible_sprite(data)?;
    set_word(data, 0x1cc2, sprite.dimensions.0);
    let background = if sprite.opaque {
        let background = screen.read_rect(sprite.offset, sprite.dimensions)?;
        screen.copy_rect(sprite.offset, sprite.dimensions, &sprite.pixels)?;
        background
    } else {
        screen.zero_transparent_rect(sprite.offset, sprite.dimensions, &sprite.pixels)?
    };
    data[0x1c40..0x1c40 + background.len()].copy_from_slice(&background);
    data[0x2ae0] = sprite.dimensions.0 as u8;
    data[0x2ae2] = 0;
    if !sprite.opaque {
        let length = sprite.pixels.len();
        set_word(
            data,
            0x2ae3,
            u16::from_le_bytes([sprite.pixels[length - 2], sprite.pixels[length - 1]]),
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn restore(
    data: &mut [u8],
    screen: &mut crate::framebuffer::Framebuffer,
) -> Result<(), String> {
    let dimensions = Dimensions(word(data, 0x1cc2));
    let source = data
        .get(0x1c40..0x1c40 + dimensions.len())
        .ok_or("Ground-dog saved background exceeds executable data")?;
    screen.copy_rect(word(data, 0x1cbd), dimensions, source)?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0xfa59);
        data[0x57b] = 180;
        set_word(&mut data, 0x579, 80);
        for index in 0..8 {
            let source = 0x1200 + index * 120;
            set_word(&mut data, 0x15c8 + index * 2, source as u16);
            for row in 0..15 {
                for column in 0..8 {
                    data[source + row * 8 + column] = (index * 16 + column + 1) as u8;
                }
            }
        }
        initialize(&mut data).unwrap();
        data
    }

    #[test]
    fn initializer_preserves_inherited_geometry_clock_and_alert() {
        let mut data = vec![0xab; DATA_LENGTH];
        set_word(&mut data, 8, 0);
        let mut expected = data.clone();
        for offset in [0x1cbf, 0x1cc0, 0x1cc1, 0x1cb8] {
            expected[offset] = 0;
        }
        set_word(&mut expected, 0x1ce1, 0);
        expected[0x1cc8] = 177;
        expected[0x5b0f] = 12;
        set_word(&mut expected, 0x5b0c, 1);
        set_word(&mut expected, 0x5b12, 0x1ff);
        set_word(&mut expected, 0x5b0a, 15);
        expected[0x5b0e] = 1;
        initialize(&mut data).unwrap();
        assert_eq!(data, expected);
    }

    #[test]
    fn alternating_tick_and_retrace_gates_preserve_rejected_state() {
        let mut data = data();
        set_word(&mut data, 0x1cc9, 65535);
        data[0x57b] = 0;
        let old = data.clone();
        assert!(advance(&mut data, 0, false, false).unwrap().is_none());
        assert_eq!(data, old);
        assert!(advance(&mut data, 0, true, false).unwrap().is_some());
        let old = data.clone();
        assert!(advance(&mut data, 1, true, false).unwrap().is_none());
        assert_eq!(data, old);
        assert!(advance(&mut data, 2, true, false).unwrap().is_some());
        assert_eq!(word(&data, 0x1ce1), 2);
    }

    #[test]
    fn alert_bypasses_height_and_entry_then_uses_all_three_cropped_frames() {
        let mut data = data();
        data[0x1d58] = 1;
        data[0x57b] = 40;
        data[0x558] = 3;
        for (tick, crop) in [(1, 3), (3, 2), (4, 1)] {
            let update = advance(&mut data, tick, true, false).unwrap().unwrap();
            let sprite = update.sprite.unwrap();
            assert_eq!(sprite.dimensions, Dimensions(0x0f00 | (4 - crop)));
            assert_eq!(word(&data, 0x1cc6), 288 + crop * 8);
            assert_eq!(data[0x1cc0], crop as u8);
            assert_eq!(sprite.pixels[0], if crop & 1 != 0 { 17 } else { 1 });
            assert_eq!(data[0x1d58], 1);
        }
        let update = advance(&mut data, 6, true, false).unwrap().unwrap();
        assert_eq!(data[0x1d58], 0);
        assert_eq!(data[0x1cbf], 1);
        assert_eq!(word(&data, 0x1cc6), 288);
        assert_eq!(data[0x1cc1], 0);
        assert_eq!(update.sprite.unwrap().dimensions, Dimensions(0x0f04));
    }

    #[test]
    fn ordinary_spawn_is_strict_but_chase_probability_is_inclusive() {
        for difficulty in 0..8 {
            for (threshold, spawn) in [(2, false), (3, true)] {
                let mut data = data();
                set_word(&mut data, 8, difficulty);
                set_word(&mut data, 0x2ae5, 4); // First low random byte is 2.
                data[0x1cd1 + usize::from(difficulty)] = threshold;
                let update = advance(&mut data, 1, true, true).unwrap().unwrap();
                assert_eq!(update.sprite.is_some(), spawn);
                assert_eq!(word(&data, 0x2ae5), 2);
                assert_eq!(data[0x1cc0], if spawn { 3 } else { 0 });
            }
            for (threshold, expected_x) in [(1, 64), (2, 80)] {
                let mut data = data();
                set_word(&mut data, 8, difficulty);
                set_word(&mut data, 0x2ae5, 4);
                data[0x1cbf] = 1;
                data[0x1cd0] = 255;
                data[0x1cd9 + usize::from(difficulty)] = threshold;
                set_word(&mut data, 0x1cc6, 72);
                advance(&mut data, 1, true, true).unwrap().unwrap();
                assert_eq!(word(&data, 0x1cc6), expected_x);
                assert_eq!(word(&data, 0x2ae5), 2);
            }
        }
    }

    #[test]
    fn ordinary_exit_requires_high_player_and_keeps_zero_lives_from_wrapping() {
        for (player_y, expected_exit) in [(179, 4), (180, 0)] {
            let mut data = data();
            data[0x1cbf] = 1;
            data[0x1cd0] = 255;
            data[0x57b] = player_y;
            set_word(&mut data, 0x1cc6, 0);
            set_word(&mut data, 0x579, 200);
            advance(&mut data, 1, true, false).unwrap().unwrap();
            assert_eq!(data[0x1cc1], expected_exit);
        }
        let mut data = data();
        data[0x1cb8] = 255;
        data[0x1cc1] = 1;
        data[0x1f80] = 0;
        let update = advance(&mut data, 1, true, false).unwrap().unwrap();
        assert!(update.finished_death && !update.life_lost);
        assert_eq!(data[0x1f80], 0);
    }

    #[test]
    fn contact_includes_both_edges_and_respects_all_gates() {
        let mut data = data();
        data[0x1cbf] = 1;
        set_word(&mut data, 0x1cc6, 80);
        for (x, hit) in [(55, false), (56, true), (112, true), (113, false)] {
            set_word(&mut data, 0x579, x);
            assert_eq!(contact(&data), hit);
        }
        set_word(&mut data, 0x579, 80);
        for (offset, value) in [(0x1cb8, 1), (0x558, 1), (0x57b, 162), (0x1cbf, 0)] {
            let mut data = data.clone();
            data[offset] = value;
            assert!(!contact(&data));
        }
        set_word(&mut data, 0x1cc6, 0xffff);
        assert!(!contact(&data));
    }

    #[test]
    fn suppressed_contact_keeps_overlap_and_running_animation() {
        let mut data = data();
        data[0x1cbf] = 1;
        data[0x1cd0] = 1;
        set_word(&mut data, 0x1cc6, 72);
        let update = advance(&mut data, 1, true, true).unwrap().unwrap();
        assert!(update.contact);
        assert!(!update.started_death);
        assert!(update.sprite.is_some());
        assert_eq!(data[0x1cb8], 0);
        assert_eq!(word(&data, 0x579), 80);
        assert_eq!(word(&data, 0x1cc6), 80);
    }

    #[test]
    fn pursuit_relocates_player_and_loses_alley_life_only_on_final_exit() {
        let mut data = data();
        data[0x1cbf] = 1;
        set_word(&mut data, 0x1cc6, 80);
        data[0x1f80] = 3;
        begin_death(&mut data).unwrap();
        assert_eq!(
            (word(&data, 0x1cc6), data[0x1cb8], word(&data, 0x1cb9)),
            (80, 255, 10)
        );
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x558]),
            (296, 180, 3)
        );
        data[0x1cc1] = 4;
        for tick in [1, 3, 4] {
            let update = advance(&mut data, tick, true, false).unwrap().unwrap();
            assert!(!update.life_lost);
            assert!(update.sprite.unwrap().opaque);
            assert_eq!(data[0x1f80], 3);
        }
        let update = advance(&mut data, 6, true, false).unwrap().unwrap();
        assert!(update.life_lost && update.finished_death);
        assert_eq!(data[0x1f80], 2);
        assert_eq!(data[0x1cb8], 0);
        assert!(update.sprite.is_none());
    }

    #[test]
    fn milk_pursuit_uses_contact_height_and_finishes_with_failure_before_life_loss() {
        let mut data = data();
        set_word(&mut data, 4, 6);
        data[0x57b] = 136;
        set_word(&mut data, 0x579, 44);
        data[0x1f80] = 3;
        begin_death(&mut data).unwrap();
        assert_eq!((word(&data, 0x1cc6), data[0x1cc8]), (44, 136));
        assert_eq!((word(&data, 0x579), data[0x57b]), (290, 136));
        data[0x1cc1] = 1;
        let update = advance(&mut data, 1, true, false).unwrap().unwrap();
        assert!(update.failure && update.finished_death && !update.life_lost);
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x552]),
            (160, 96, 0xdd)
        );
        assert_eq!(data[0x1f80], 3);
        assert_eq!(data[0x1cb8], 255);
    }

    #[test]
    fn native_drawing_saves_and_restores_every_packed_background_byte() {
        let mut data = data();
        data[0x1cd0] = 1;
        select_sprite(&mut data);
        set_word(&mut data, 0x1cc4, 0x0f04);
        set_word(&mut data, 0x1cbd, pixel_offset(88, 177));
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        let original = screen.bytes().to_vec();
        draw(&mut data, &mut screen).unwrap();
        assert_eq!(&data[0x1c40..0x1c40 + 120], &[0xaa; 120]);
        assert_ne!(screen.bytes().as_slice(), original);
        restore(&mut data, &mut screen).unwrap();
        assert_eq!(screen.bytes().as_slice(), original);
    }
}
