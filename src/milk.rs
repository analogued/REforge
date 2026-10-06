// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 6 milk consumption and sleeping-dog attention.
//!
//! The caller suspends its ordinary player image before these drawing helpers.
//! The drinking image has its own original background buffer. Dog contact keeps
//! the warning animation but defers the lethal pursuit, relocation and failure.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};
use crate::room;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DrinkStep {
    pub accepted: bool,
    pub drew: bool,
    pub consumed: bool,
    pub player_changed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AttentionStep {
    pub accepted: bool,
    pub drew: bool,
    pub contact: bool,
    pub warning: bool,
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable milk-room data segment is truncated".into())
    } else if word(data, 8) > 7 {
        Err("Milk-room difficulty exceeds its external tables".into())
    } else {
        Ok(())
    }
}

fn copy(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    destination: u16,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    screen.copy_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

fn draw_bowl(data: &mut [u8], screen: &mut Framebuffer, slot: usize) -> Result<(), String> {
    let destination = pixel_offset(word(data, 0x4481 + slot * 2), data[0x4499 + slot]);
    let source = 0x41fcu16.wrapping_add(u16::from(data[0x44c4 + slot]) << 5);
    copy(data, screen, source, destination, 0x0802)
}

/// CS:4b47. Placement retains rejected PRNG draws and selected-only attention
/// reset; the two independent scheduler tick baselines are inherited.
pub(crate) fn initialize(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    let difficulty = usize::from(word(data, 8));
    let count = usize::from(data[0x4471 + difficulty]);
    if !(1..=12).contains(&count) || word(data, 0x2ae5) == 0 {
        return Err("Milk-room dog placement has invalid count or zero random state".into());
    }
    data[0x4441..0x4459].fill(0);
    for _ in 0..count {
        let offset = loop {
            let offset = usize::from(next_random(data) & 0x1e);
            if offset < 24 && word(data, 0x4441 + offset) == 0 {
                break offset;
            }
        };
        set_word(data, 0x4459 + offset, 0);
        set_word(data, 0x4441 + offset, 1);
        copy(
            data,
            screen,
            word(data, 0x4429 + offset),
            word(data, 0x4411 + offset),
            0x0d05,
        )?;
    }
    for slot in (0..12).rev() {
        data[0x44c4 + slot] = data[0x4479 + difficulty];
        draw_bowl(data, screen, slot)?;
    }
    data[0x44d0] = 0;
    data[0x44bd] = 0;
    data[0x43e0] = 1;
    data[0x44d6] = 12;
    data[0x44be] = 0;
    Ok(())
}

fn restore_image(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    if data[0x43e0] != 0 {
        return Ok(false);
    }
    copy(data, screen, 0x43a0, word(data, 0x43de), 0x0a03)?;
    Ok(true)
}

/// Suspend the separately owned drinking image before another actor draws.
/// Unlike the native restore-only helper, this marks the image suspended so
/// the next accepted drinking update cannot restore a stale background.
pub(crate) fn restore_drinking(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    check(data)?;
    let restored = restore_image(data, screen)?;
    data[0x43e0] = 1;
    Ok(restored)
}

/// CS:4b1d. Recomposition does not advance phase or consume shared randomness.
pub(crate) fn draw_drinking(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    let source = word(data, 0x44d1).wrapping_add(if data[0x44d0] >= 0x80 { 0x3c } else { 0 });
    let destination = word(data, 0x43dc);
    let dimensions = Dimensions(0x0a03);
    let background = screen.and_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    data[0x43a0..0x43dc].copy_from_slice(&background);
    set_word(data, 0x43de, destination);
    data[0x43e0] = 0;
    data[0x2ae0] = 3;
    data[0x2ae2] = 0;
    Ok(())
}

fn stop_drinking(
    data: &mut [u8],
    screen: &mut Framebuffer,
    step: &mut DrinkStep,
) -> Result<(), String> {
    if data[0x44bd] != 0 {
        step.drew |= restore_image(data, screen)?;
        data[0x69a] = 0x10;
        data[0x583] = 0;
        step.player_changed = true;
    }
    data[0x44bd] = 0;
    data[0x43e0] = 1;
    data[0x44d0] = 0;
    data[0x44be] = 0;
    Ok(())
}

fn overlap(a: (u16, u8, u16, u8), b: (u16, u8, u16, u8)) -> bool {
    a.0.wrapping_add(a.2) >= b.0
        && a.0.saturating_sub(b.2) <= b.0
        && a.1.wrapping_add(a.3) >= b.1
        && a.1.saturating_sub(b.3) <= b.1
}

fn drinking_broom_contact(data: &[u8]) -> bool {
    overlap(
        (word(data, 0x327d), data[0x327f], 16, 30),
        (
            word(data, 0x579).saturating_sub(8),
            data[0x57b].wrapping_add(3),
            40,
            14,
        ),
    )
}

#[cfg(any(not(feature = "dev-tools"), test))]
pub(crate) fn advance_drinking(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<DrinkStep, String> {
    advance_drinking_inner(data, screen, tick, false)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_drinking_with_actor_mask(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
) -> Result<DrinkStep, String> {
    advance_drinking_inner(data, screen, tick, disable_broom)
}

/// CS:4943. The caller owns normal player restore/save/draw operations; their
/// need is reported without choosing another player animation or RNG value.
fn advance_drinking_inner(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
) -> Result<DrinkStep, String> {
    check(data)?;
    let mut step = DrinkStep::default();
    if data[0x1cb8] != 0 {
        return Ok(step);
    }
    if data[0x44be] != 0 {
        data[0x698] = data[0x44be];
        data[0x699] = 0;
    }
    if tick == word(data, 0x44d3) {
        return Ok(step);
    }
    set_word(data, 0x44d3, tick);
    step.accepted = true;
    if data[0x584] != 0 {
        if data[0x44bd] != 0 {
            step.drew = restore_image(data, screen)?;
            if !disable_broom {
                room::restore_broom(data, screen)?;
                room::draw_broom(data, screen)?;
            }
            data[0x583] = 0;
            data[0x44bd] = 0;
            data[0x43e0] = 1;
            data[0x44be] = 0;
            step.player_changed = true;
        }
        return Ok(step);
    }
    if data[0x69a] != 0 {
        stop_drinking(data, screen, &mut step)?;
        return Ok(step);
    }
    set_word(data, 0x44c1, 0xffff);
    set_word(data, 0x44bf, 0xffff);
    let x = word(data, 0x579);
    let y = data[0x57b].wrapping_add(8);
    for slot in (0..12).rev() {
        if data[0x44c4 + slot] == 0 || data[0x4499 + slot] != y {
            continue;
        }
        let (difference, borrow) = x.overflowing_sub(word(data, 0x4481 + slot * 2));
        let distance = if borrow { !difference } else { difference };
        if distance <= word(data, 0x44bf) {
            set_word(data, 0x44bf, distance);
            set_word(data, 0x44d1, word(data, 0x44a5 + slot * 2));
            set_word(data, 0x44c1, slot as u16);
            data[0x44c3] = if borrow { 1 } else { 0xff };
        }
    }
    if word(data, 0x44c1) >= 12 {
        stop_drinking(data, screen, &mut step)?;
        return Ok(step);
    }
    let distance = word(data, 0x44bf);
    if distance >= 4 {
        if distance <= 8 {
            data[0x572] = 4;
        }
        data[0x698] = data[0x44c3];
        data[0x56e] = data[0x44c3];
        data[0x44be] = data[0x44c3];
        data[0x699] = 0;
        data[0x571] = 0;
        step.player_changed = true;
        stop_drinking(data, screen, &mut step)?;
        return Ok(step);
    }
    data[0x44be] = 0;
    data[0x583] = 0;
    data[0x44bd] = 1;
    let (phase, carry) = data[0x44d0].overflowing_add(0x30);
    data[0x44d0] = phase;
    data[0x44d5] = u8::from(carry);
    let aligned_x = x & 0x0ffc;
    let sprite_x = if word(data, 0x44d1) == 0x410c {
        aligned_x.saturating_sub(8)
    } else {
        aligned_x.wrapping_add(8).min(294)
    };
    set_word(
        data,
        0x43dc,
        pixel_offset(sprite_x, data[0x57b].wrapping_add(3)),
    );
    restore_image(data, screen)?;
    if carry {
        let slot = usize::from(word(data, 0x44c1));
        if data[0x44c4 + slot] != 0 {
            data[0x44c4 + slot] -= 1;
            step.consumed = true;
            if data[0x44c4 + slot] == 0 {
                if data[0] != 0 {
                    set_word(data, 0x5923, 0x0723);
                    data[0x5920] = 2;
                    set_word(data, 0x5921, tick);
                }
                data[0x698] = 0;
                data[0x44be] = 0;
                data[0x69a] = 0x10;
                data[0x44d6] = data[0x44d6].wrapping_sub(1);
                if data[0x44d6] == 0 {
                    data[0x553] = 1;
                }
            }
            let broom_overlap = !disable_broom && drinking_broom_contact(data);
            if broom_overlap {
                room::restore_broom(data, screen)?;
            }
            draw_bowl(data, screen, slot)?;
            if broom_overlap {
                room::draw_broom(data, screen)?;
            }
        }
    }
    draw_drinking(data, screen)?;
    step.drew = true;
    step.player_changed = true;
    Ok(step)
}

fn draw_attention(data: &mut [u8], screen: &mut Framebuffer, offset: usize) -> Result<(), String> {
    let level = word(data, 0x4459 + offset);
    if level > 2 {
        return Err("Sleeping-dog attention exceeds its sprite table".into());
    }
    let reversed = word(data, 0x4429 + offset) != 0x429c;
    let destination = word(data, 0x4411 + offset)
        .wrapping_add(0xa7)
        .wrapping_sub(if reversed { 6 } else { 0 });
    let source = 0x4100 + level * 2 + if reversed { 6 } else { 0 };
    copy(data, screen, source, destination, 0x0101)
}

/// CS:47d6's attention clocks, slot order, warning and probabilistic decay.
/// At level two the native lethal branch is omitted, retaining harmless
/// overlap and the existing eye frame without stun, knockback or relocation.
pub(crate) fn advance_attention(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<AttentionStep, String> {
    check(data)?;
    let mut step = AttentionStep::default();
    let difficulty_offset = usize::from(word(data, 8)) * 2;
    if tick.wrapping_sub(word(data, 0x44d7)) <= word(data, 0x44dc + difficulty_offset) {
        return Ok(step);
    }
    set_word(data, 0x44d7, tick);
    step.accepted = true;
    if data[0x1cb8] != 0 {
        return Ok(step);
    }
    data[0x44fc] = 0;
    for slot in (0..12).rev() {
        let offset = slot * 2;
        if word(data, 0x4441 + offset) == 0 {
            continue;
        }
        let (difference, borrow) = word(data, 0x43e1 + offset).overflowing_sub(word(data, 0x579));
        let distance = if borrow { !difference } else { difference };
        let near = data[0x43f9 + offset] == data[0x57b]
            && distance <= word(data, 0x44ec + difficulty_offset);
        let level = word(data, 0x4459 + offset);
        if near {
            if level >= 2 {
                step.contact = true;
                continue;
            }
            set_word(data, 0x4459 + offset, level + 1);
            if level == 1 {
                data[0x44fc] = data[0x44fc].wrapping_add(1);
                step.warning = true;
            }
        } else {
            if level == 0 {
                continue;
            }
            if next_random(data) as u8 <= 0x38 {
                set_word(data, 0x4459 + offset, level - 1);
            }
        }
        let player_overlap = overlap(
            (
                word(data, 0x43e1 + offset).wrapping_sub(20),
                data[0x43f9 + offset],
                40,
                6,
            ),
            (word(data, 0x579), data[0x57b], 24, 14),
        );
        data[0x44d9] = u8::from(player_overlap);
        let drinking = player_overlap && data[0x44bd] != 0;
        if drinking {
            restore_image(data, screen)?;
        }
        draw_attention(data, screen, offset)?;
        if drinking {
            draw_drinking(data, screen)?;
        } else if player_overlap {
            data[0x583] = 0;
        }
        step.drew = true;
    }
    Ok(step)
}

/// Remove the initialized sleeping dogs while retaining milk objectives and
/// original placement randomness. The native dog eraser uses the room's AA.
#[cfg(feature = "dev-tools")]
pub(crate) fn remove_dogs(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    for slot in (0..12).rev() {
        let offset = slot * 2;
        if word(data, 0x4441 + offset) != 0 {
            screen.copy_rect(
                word(data, 0x4411 + offset),
                Dimensions(0x0d05),
                &[0xaa; 130],
            )?;
        }
        set_word(data, 0x4441 + offset, 0);
    }
    for slot in (0..12).rev() {
        draw_bowl(data, screen, slot)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 4, 6);
        set_word(&mut data, 0x2ae5, 0xfa59);
        data[0x4471..0x4479].fill(2);
        data[0x4479..0x4481].fill(4);
        for slot in 0..12 {
            let offset = slot * 2;
            set_word(&mut data, 0x43e1 + offset, 124);
            set_word(&mut data, 0x43f9 + offset, 136);
            set_word(
                &mut data,
                0x4411 + offset,
                pixel_offset((slot % 6) as u16 * 48, 80 + (slot / 6) as u8 * 20),
            );
            set_word(&mut data, 0x4429 + offset, 0x6000);
            set_word(&mut data, 0x4481 + offset, 144);
            data[0x4499 + slot] = 144;
            set_word(&mut data, 0x44a5 + offset, 0x410c);
        }
        data[0x6000..0x6082].fill(0x55);
        data[0x410c..0x4184].fill(0xf0);
        data[0x41fc..0x429c].fill(0x55);
        data[0x43e0] = 1;
        data[0x44c4] = 1;
        data[0x44d6] = 1;
        data[0x3286] = 1;
        set_word(&mut data, 0x579, 144);
        data[0x57b] = 136;
        set_word(&mut data, 0x44dc, 18);
        set_word(&mut data, 0x44ec, 40);
        data
    }

    #[test]
    fn initializer_preserves_unselected_attention_and_inherited_clocks() {
        let mut data = data();
        for slot in 0..12 {
            set_word(&mut data, 0x4459 + slot * 2, 0x1234);
        }
        set_word(&mut data, 0x44d3, 123);
        set_word(&mut data, 0x44d7, 456);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        initialize(&mut data, &mut screen).unwrap();
        assert_eq!((word(&data, 0x44d3), word(&data, 0x44d7)), (123, 456));
        assert_eq!(&data[0x44c4..0x44d0], &[4; 12]);
        assert_eq!((data[0x44d6], data[0x43e0]), (12, 1));
        let mut active = 0;
        for slot in 0..12 {
            let selected = word(&data, 0x4441 + slot * 2) != 0;
            active += usize::from(selected);
            assert_eq!(
                word(&data, 0x4459 + slot * 2),
                if selected { 0 } else { 0x1234 }
            );
            let image = screen
                .read_rect(word(&data, 0x4411 + slot * 2), Dimensions(0x0d05))
                .unwrap();
            assert!(
                image
                    .iter()
                    .all(|byte| *byte == if selected { 0x55 } else { 0xaa })
            );
        }
        assert_eq!(active, 2);
        assert_ne!(word(&data, 0x2ae5), 0xfa59);
    }

    #[test]
    fn drinking_uses_not_distance_and_lower_slot_ties() {
        for (x, drinks, direction, distance) in [(140, true, 1, 3), (148, false, 0xff, 4)] {
            let mut data = data();
            set_word(&mut data, 0x579, x);
            data[0x44c5] = 1;
            let step = advance_drinking(&mut data, &mut Framebuffer::default(), 1).unwrap();
            assert!(step.accepted);
            assert_eq!(word(&data, 0x44c1), 0);
            assert_eq!(word(&data, 0x44bf), distance);
            assert_eq!(data[0x44c3], direction);
            assert_eq!(data[0x44bd] != 0, drinks);
            assert_eq!(data[0x44be], 0, "Alignment common exit clears the latch");
            if !drinks {
                assert_eq!((data[0x698], data[0x56e], data[0x572]), (0xff, 0xff, 4));
                assert_eq!(data[0x44d0], 0);
            }
        }
    }

    #[test]
    fn phase_carry_alone_consumes_and_last_bowl_success_has_no_failure_guard() {
        for (phase, consumed, next) in [(207, false, 255), (208, true, 0)] {
            let mut data = data();
            data[0x44d0] = phase;
            data[0x552] = 0xdd;
            data[0x1f80] = 3;
            let step = advance_drinking(&mut data, &mut Framebuffer::default(), 1).unwrap();
            assert_eq!((step.consumed, data[0x44d0]), (consumed, next));
            assert_eq!(data[0x553], u8::from(consumed));
            assert_eq!(data[0x44c4], u8::from(!consumed));
            assert_eq!((data[0x552], data[0x1f80]), (0xdd, 3));
            assert_eq!(data[0x69a], if consumed { 0x10 } else { 0 });
        }
    }

    #[test]
    fn changed_tick_gate_still_reapplies_alignment_before_rejection() {
        let mut data = data();
        data[0x44be] = 0xff;
        data[0x699] = 1;
        let random = word(&data, 0x2ae5);
        assert_eq!(
            advance_drinking(&mut data, &mut Framebuffer::default(), 0).unwrap(),
            DrinkStep::default()
        );
        assert_eq!((data[0x698], data[0x699]), (0xff, 0));
        assert_eq!(word(&data, 0x2ae5), random);
        data[0x1cb8] = 1;
        data[0x698] = 0;
        assert!(
            !advance_drinking(&mut data, &mut Framebuffer::default(), 1)
                .unwrap()
                .accepted
        );
        assert_eq!(data[0x698], 0, "Death gate precedes alignment replay");
    }

    #[test]
    fn drinking_suspension_preserves_frame_and_phase_without_random_draws() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        advance_drinking(&mut data, &mut screen, 1).unwrap();
        let drawn = screen.bytes().to_vec();
        let random = word(&data, 0x2ae5);
        assert!(restore_drinking(&mut data, &mut screen).unwrap());
        assert!(!restore_drinking(&mut data, &mut screen).unwrap());
        assert_eq!(
            screen
                .read_rect(word(&data, 0x43dc), Dimensions(0x0a03))
                .unwrap(),
            [0xaa; 60]
        );
        draw_drinking(&mut data, &mut screen).unwrap();
        assert_eq!(screen.bytes().as_slice(), drawn);
        assert_eq!((data[0x44d0], word(&data, 0x2ae5)), (48, random));
        data[0x69a] = 0x10;
        advance_drinking(&mut data, &mut screen, 2).unwrap();
        assert_eq!((data[0x44bd], data[0x43e0], data[0x44d0]), (0, 1, 0));
        assert_eq!(
            screen
                .read_rect(word(&data, 0x43dc), Dimensions(0x0a03))
                .unwrap(),
            [0xaa; 60]
        );
    }

    #[test]
    fn attention_strict_clock_wrap_warning_and_nonlethal_overlap() {
        let mut data = data();
        set_word(&mut data, 0x4441, 1);
        set_word(&mut data, 0x4459, 1);
        set_word(&mut data, 0x44d7, 0xfff0);
        data[0x1f80] = 3;
        let random = word(&data, 0x2ae5);
        let mut screen = Framebuffer::default();
        assert!(
            !advance_attention(&mut data, &mut screen, 2)
                .unwrap()
                .accepted
        );
        let step = advance_attention(&mut data, &mut screen, 3).unwrap();
        assert!(step.warning && step.drew);
        assert_eq!(
            (word(&data, 0x4459), data[0x44fc], word(&data, 0x44d7)),
            (2, 1, 3)
        );
        let step = advance_attention(&mut data, &mut screen, 22).unwrap();
        assert!(step.contact && step.accepted);
        assert_eq!(
            (
                word(&data, 0x579),
                data[0x57b],
                data[0x1cb8],
                data[0x552],
                data[0x1f80]
            ),
            (144, 136, 0, 0, 3)
        );
        assert_eq!(word(&data, 0x2ae5), random);
        data[0x1cb8] = 1;
        data[0x44fc] = 7;
        let step = advance_attention(&mut data, &mut screen, 41).unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!((word(&data, 0x44d7), data[0x44fc]), (41, 7));
    }

    #[test]
    fn away_attention_consumes_random_only_for_nonzero_active_slots() {
        let mut data = data();
        data[0x57b] = 0;
        set_word(&mut data, 0x4441, 1);
        set_word(&mut data, 0x4443, 1);
        set_word(&mut data, 0x4459, 1);
        set_word(&mut data, 0x445b, 0);
        set_word(&mut data, 0x2ae5, 2);
        advance_attention(&mut data, &mut Framebuffer::default(), 19).unwrap();
        assert_eq!((word(&data, 0x4459), word(&data, 0x2ae5)), (0, 0x8001));
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn actor_removal_retains_consumption_objectives_and_native_random_consumption() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        initialize(&mut data, &mut screen).unwrap();
        let random = word(&data, 0x2ae5);
        remove_dogs(&mut data, &mut screen).unwrap();
        assert_eq!(&data[0x4441..0x4459], &[0; 24]);
        assert_eq!(&data[0x44c4..0x44d0], &[4; 12]);
        assert_eq!((data[0x44d6], word(&data, 0x2ae5)), (12, random));
        for slot in 0..12 {
            assert_eq!(
                screen
                    .read_rect(word(&data, 0x4411 + slot * 2), Dimensions(0x0d05))
                    .unwrap(),
                [0xaa; 130]
            );
        }
    }
}
