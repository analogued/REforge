// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 4 mice emerging from holes and the paired-hole transfer animation.
//!
//! The caller suspends the current player image before drawing here. Transfers
//! suppress that image until their final frame, while ordinary actor scheduling
//! continues. Coordinates, phases, pairing and graphics remain external data.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MouseStep {
    pub accepted: bool,
    pub drew: bool,
    pub collected: bool,
    pub player_changed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TransferStep {
    pub accepted: bool,
    pub drew: bool,
    pub started: bool,
    pub player_changed: bool,
    pub active: bool,
    pub finished: bool,
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable cheese data segment is truncated".into())
    } else {
        Ok(())
    }
}

fn overlap(a: (u16, u8, u16, u8), b: (u16, u8, u16, u8)) -> bool {
    a.0.wrapping_add(a.2) >= b.0
        && a.0.saturating_sub(b.2) <= b.0
        && a.1.wrapping_add(a.3) >= b.1
        && a.1.saturating_sub(b.3) <= b.1
}

fn player_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x579), data[0x57b], 24, 14)
}

fn broom_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x327d), data[0x327f], 16, 30)
}

fn mouse_rect(data: &[u8], slot: usize) -> (u16, u8, u16, u8) {
    (word(data, 0x3ecc + slot * 2), data[0x3ed4 + slot], 16, 12)
}

pub(crate) fn mouse_contact(data: &[u8], slot: usize) -> bool {
    overlap(mouse_rect(data, slot), player_rect(data))
}

fn broom_contact(data: &[u8], slot: usize) -> bool {
    overlap(mouse_rect(data, slot), broom_rect(data))
}

// CS:431c uses one's complements on borrowed differences rather than ABS.
fn near_player(data: &[u8], slot: usize, limit: u16) -> bool {
    let (mut x, borrow) = word(data, 0x3ecc + slot * 2).overflowing_sub(word(data, 0x579));
    if borrow {
        x = !x;
    }
    let (mut y, borrow) = data[0x3ed4 + slot].overflowing_sub(data[0x57b]);
    if borrow {
        y = !y;
    }
    x.wrapping_add(u16::from(y)) < limit
}

fn position(data: &mut [u8], slot: usize) -> Result<(), String> {
    let hole = usize::from(word(data, 0x3eba + slot * 2));
    if hole >= 16 {
        return Err("Cheese mouse hole exceeds the sixteen-hole table".into());
    }
    data[0x3ed4 + slot] = data[0x1050 + hole]
        .wrapping_sub(if hole < 3 { 0 } else { 10 })
        .wrapping_add(3);
    set_word(
        data,
        0x3ecc + slot * 2,
        word(data, 0x1137 + hole * 2).wrapping_add(8),
    );
    Ok(())
}

/// CS:4277 rejects other slots' hole IDs before its bounded proximity retry.
fn choose_hole(data: &mut [u8], slot: usize) -> Result<(), String> {
    data[0x3ed9] = 32;
    loop {
        let candidate = next_random(data) & 15;
        if (0..4).any(|other| other != slot && word(data, 0x3eba + other * 2) == candidate) {
            continue;
        }
        set_word(data, 0x3eba + slot * 2, candidate);
        position(data, slot)?;
        if data[0x3ed9] == 0 || !near_player(data, slot, 50) {
            return Ok(());
        }
        data[0x3ed9] -= 1;
    }
}

/// CS:4090 is called after room scenery, player entry and broom initialization.
/// It preserves the scheduler clock and initializes slots in reverse order.
pub(crate) fn initialize(data: &mut [u8], _screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    for slot in (0..4).rev() {
        data[0x3eae + slot] = 1;
        data[0x3eb2 + slot] = 0;
        choose_hole(data, slot)?;
        data[0x3eb6 + slot] = (next_random(data) as u8 & 15) + 20;
    }
    set_word(data, 0x3eda, 0);
    data[0x3ed8] = 4;
    Ok(())
}

fn copy_sprite(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    dest: u16,
    dimensions: Dimensions,
) -> Result<(), String> {
    screen.copy_rect(dest, dimensions, sprite_bytes(data, source, dimensions)?)?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

fn draw_mask(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    dest: u16,
    background: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(0x0c02);
    let saved = screen.and_rect(dest, dimensions, sprite_bytes(data, source, dimensions)?)?;
    let start = usize::from(background);
    let end = start + saved.len();
    if end > data.len() {
        return Err("Cheese mouse background exceeds executable data".into());
    }
    data[start..end].copy_from_slice(&saved);
    data[0x2ae0] = 2;
    data[0x2ae2] = 0;
    Ok(())
}

fn restore_mouse(data: &mut [u8], screen: &mut Framebuffer, slot: usize) -> Result<bool, String> {
    if data[0x3eae + slot] != 0 {
        return Ok(false);
    }
    copy_sprite(
        data,
        screen,
        word(data, 0x3ec2 + slot * 2),
        word(data, 0x3ea6 + slot * 2),
        Dimensions(0x0c02),
    )?;
    Ok(true)
}

/// CS:40c2 accepts one slot per tick, with the original extra slot-zero call
/// after slot three leaves the tick unlatched. Contact precedes phase motion.
pub(crate) fn advance_mice(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<MouseStep, String> {
    advance_mice_inner(
        data,
        screen,
        tick,
        #[cfg(feature = "dev-tools")]
        false,
        sound,
    )
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_mice_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
    sound: &mut crate::sound::Sound,
) -> Result<MouseStep, String> {
    if !disable_broom {
        return advance_mice(data, screen, tick, sound);
    }
    advance_mice_inner(data, screen, tick, true, sound)
}

fn advance_mice_inner(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    #[cfg(feature = "dev-tools")] disable_broom: bool,
    sound: &mut crate::sound::Sound,
) -> Result<MouseStep, String> {
    #[cfg(feature = "dev-tools")]
    let broom_enabled = !disable_broom;
    #[cfg(not(feature = "dev-tools"))]
    let broom_enabled = true;
    check(data)?;
    if tick == word(data, 0x3edc) {
        return Ok(MouseStep::default());
    }
    let cursor = word(data, 0x3eda).wrapping_add(1);
    let slot = if cursor >= 4 { 0 } else { usize::from(cursor) };
    set_word(data, 0x3eda, slot as u16);
    if slot != 3 {
        set_word(data, 0x3edc, tick);
    }
    let mut result = MouseStep {
        accepted: true,
        ..MouseStep::default()
    };
    if data[0x3eb2 + slot] != 0 {
        return Ok(result);
    }
    let mut contact = mouse_contact(data, slot);
    if !contact {
        if broom_enabled && broom_contact(data, slot) {
            return Ok(result);
        }
        if data[0x3eb6 + slot] == 0 {
            choose_hole(data, slot)?;
            data[0x3eb6 + slot] = (next_random(data) as u8 & 7) + 20;
        }
        data[0x3eb6 + slot] = data[0x3eb6 + slot].wrapping_sub(1);
        contact = mouse_contact(data, slot);
    }
    if contact {
        if data[0x3eae + slot] != 0 || data[0x3eb6 + slot] >= 20 {
            return Ok(result);
        }
        restore_mouse(data, screen, slot)?;
        data[0x3eb2 + slot] = 1;
        data[0x583] = 0;
        data[0x55c] = 0;
        data[0x3ed8] = data[0x3ed8].wrapping_sub(1);
        if data[0x3ed8] == 0 {
            data[0x553] = 1;
        }
        let dest = 0x51 + u16::from(4_u8.wrapping_sub(data[0x3ed8])) * 4;
        draw_mask(data, screen, 0x3d20, dest, 0x0e)?;
        sound.request(data, tick, 0x03e8, 0x2ee);
        result.drew = true;
        result.collected = true;
        result.player_changed = true;
        return Ok(result);
    }
    position(data, slot)?;
    let difficulty = usize::from(word(data, 8));
    if difficulty > 7 {
        return Err("Cheese difficulty exceeds its avoidance table".into());
    }
    let phase = data[0x3eb6 + slot];
    if near_player(data, slot, word(data, 0x3ede + difficulty * 2)) && (2..20).contains(&phase) {
        data[0x3eb6 + slot] = if phase <= 17 { 1 } else { 0 };
    }
    let phase = data[0x3eb6 + slot];
    let source = if phase <= 1 || phase >= 18 {
        data[0x3ed4 + slot] =
            data[0x3ed4 + slot].wrapping_add(if word(data, 0x3eba + slot * 2) < 3 {
                3
            } else {
                1
            });
        match phase {
            0 | 19 => 0x3db0,
            1 | 18 => 0x3d80,
            _ => 0,
        }
    } else {
        word(data, 0x3de0 + usize::from((phase * 2) & 2))
    };
    set_word(data, 0x3eca, source);
    let dest = pixel_offset(word(data, 0x3ecc + slot * 2), data[0x3ed4 + slot]);
    set_word(data, 0x3de4, dest);
    result.drew = restore_mouse(data, screen, slot)?;
    if broom_enabled && broom_contact(data, slot) {
        return Ok(result);
    }
    if source == 0 {
        data[0x3eae + slot] = 1;
        return Ok(result);
    }
    data[0x3eae + slot] = 0;
    set_word(data, 0x3ea6 + slot * 2, dest);
    draw_mask(data, screen, source, dest, word(data, 0x3ec2 + slot * 2))?;
    result.drew = true;
    Ok(result)
}

/// CS:3e90 advances one animation frame per changed BIOS tick. Transfer start
/// requires twelve elapsed ticks, a support marker, and an unobstructed player.
pub(crate) fn advance_transfer(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<TransferStep, String> {
    advance_transfer_inner(
        data,
        screen,
        tick,
        #[cfg(feature = "dev-tools")]
        false,
    )
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_transfer_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
) -> Result<TransferStep, String> {
    if !disable_broom {
        return advance_transfer(data, screen, tick);
    }
    advance_transfer_inner(data, screen, tick, true)
}

fn advance_transfer_inner(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    #[cfg(feature = "dev-tools")] disable_broom: bool,
) -> Result<TransferStep, String> {
    #[cfg(feature = "dev-tools")]
    let broom_enabled = !disable_broom;
    #[cfg(not(feature = "dev-tools"))]
    let broom_enabled = true;
    check(data)?;
    let mut result = TransferStep {
        active: data[0x39e1] != 0,
        ..TransferStep::default()
    };
    if result.active {
        if tick == word(data, 0x3d16) {
            return Ok(result);
        }
    } else {
        if data[0x584] != 0
            || data[0x69a] != 0
            || data[0x39e0] == 0
            || (broom_enabled && overlap(broom_rect(data), player_rect(data)))
            || tick.wrapping_sub(word(data, 0x3d18)) < 12
        {
            return Ok(result);
        }
        let hole = usize::from(data[0x39e0] - 1);
        if hole >= 16 {
            return Err("Cheese player marker exceeds the sixteen-hole table".into());
        }
        let destination = usize::from(data[0x3ce3 + hole]);
        if destination >= 16 {
            return Err("Cheese paired destination exceeds the sixteen-hole table".into());
        }
        set_word(data, 0x3d18, tick);
        data[0x55c] = 0;
        set_word(data, 0x39e2, word(data, 0x3c5a + hole * 4));
        set_word(data, 0x39e4, if hole < 3 { 0x80 } else { 0 });
        set_word(data, 0x39e6, word(data, 0x3c5a + destination * 4));
        set_word(data, 0x39e8, if destination < 3 { 0x80 } else { 0 });
        data[0x3d05] = data[0x1050 + destination];
        set_word(
            data,
            0x3d03,
            word(data, 0x1137 + destination * 2).wrapping_add(8),
        );
        data[0x39e1] = 14;
        data[0x69a] = 16;
        result.started = true;
        result.player_changed = true;
    }
    if broom_enabled && data[0x1cbf] == 0 {
        crate::room::restore_broom(data, screen)?;
    }
    data[0x39e1] = data[0x39e1].wrapping_sub(2);
    let countdown = data[0x39e1];
    let (dest, offset) = if countdown >= 8 {
        (word(data, 0x39e2), word(data, 0x39e4))
    } else {
        data[0x57b] = data[0x3d05];
        data[0x57c] = data[0x57b].wrapping_add(50);
        set_word(data, 0x579, word(data, 0x3d03));
        result.player_changed = true;
        (word(data, 0x39e6), word(data, 0x39e8))
    };
    if countdown > 12 || countdown & 1 != 0 {
        return Err("Cheese transfer countdown is outside its frame table".into());
    }
    let source = offset.wrapping_add(word(data, 0x3d06 + usize::from(countdown)));
    copy_sprite(data, screen, source, dest, Dimensions(0x1002))?;
    if broom_enabled && data[0x1cbf] == 0 {
        crate::room::draw_broom(data, screen)?;
    }
    set_word(data, 0x3d16, tick);
    result.accepted = true;
    result.drew = true;
    result.active = countdown != 0;
    result.finished = countdown == 0;
    if result.finished {
        data[0x583] = 0;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "dev-tools")]
    #[test]
    fn development_false_adapters_preserve_mouse_and_transfer_state_and_pixels() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x3eda, 3);
        data[0x3eb6] = 10;
        data[0x3eae] = 1;
        let mut alternate = data.clone();
        let mut alternate_screen = screen.clone();
        assert_eq!(
            advance_mice(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default()
            )
            .unwrap(),
            advance_mice_dev(
                &mut alternate,
                &mut alternate_screen,
                1,
                false,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
        );
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());

        let (mut data, mut screen) = fixture();
        data[0x39e0] = 1;
        data[0x1cbf] = 0;
        set_word(&mut data, 0x3260, 0x6000);
        data[0x6000..0x6078].fill(0xff);
        set_word(&mut data, 0x3284, pixel_offset(100, 100));
        let mut alternate = data.clone();
        let mut alternate_screen = screen.clone();
        for tick in 12..=18 {
            assert_eq!(
                advance_transfer(&mut data, &mut screen, tick).unwrap(),
                advance_transfer_dev(&mut alternate, &mut alternate_screen, tick, false).unwrap()
            );
            assert_eq!(data, alternate);
            assert_eq!(screen.bytes(), alternate_screen.bytes());
        }
        assert_eq!(data[0x39e1], 0);
        assert_eq!(data[0x3286], 0);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn disabled_broom_cannot_block_mouse_phase_or_its_new_image() {
        for (broom_x, broom_y, expected_normal_phase) in [(100, 100, 10), (8, 51, 9)] {
            let (mut data, mut screen) = fixture();
            set_word(&mut data, 0x3eda, 3);
            set_word(&mut data, 0x3ecc, 100);
            data[0x3ed4] = 100;
            data[0x3eb6] = 10;
            data[0x3eae] = 1;
            set_word(&mut data, 0x327d, broom_x);
            data[0x327f] = broom_y;
            let mut alternate = data.clone();
            let mut alternate_screen = screen.clone();
            let normal = advance_mice(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            let unblocked = advance_mice_dev(
                &mut alternate,
                &mut alternate_screen,
                1,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert!(normal.accepted && !normal.drew);
            assert_eq!(data[0x3eb6], expected_normal_phase);
            assert!(unblocked.accepted && unblocked.drew);
            assert_eq!((alternate[0x3eb6], alternate[0x3eae]), (9, 0));
            assert_eq!(word(&alternate, 0x3ea6), pixel_offset(8, 51));
            assert_eq!(
                alternate_screen
                    .read_rect(pixel_offset(8, 51), Dimensions(0x0c02))
                    .unwrap(),
                vec![0; 48]
            );
            assert_eq!((alternate[0x552], alternate[0x1f80]), (0, 3));
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn disabled_broom_allows_transfer_without_restoring_or_redrawing_its_image() {
        let (mut data, mut screen) = fixture();
        data[0x39e0] = 1;
        data[0x1cbf] = 0;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        let broom_dest = pixel_offset(100, 100);
        data[0x3286] = 0;
        set_word(&mut data, 0x3282, broom_dest);
        set_word(&mut data, 0x3284, broom_dest);
        set_word(&mut data, 0x3260, 0x6000);
        data[0x6000..0x6078].fill(0xff);
        data[0x31e8..0x3260].fill(0x11);
        let unchanged_broom_state = data[0x327a..0x328e].to_vec();
        assert!(
            !advance_transfer(&mut data.clone(), &mut screen.clone(), 12)
                .unwrap()
                .accepted
        );
        for tick in 12..=18 {
            let step = advance_transfer_dev(&mut data, &mut screen, tick, true).unwrap();
            assert!(step.accepted && step.drew);
            assert_eq!(step.started, tick == 12);
            assert_eq!(step.finished, tick == 18);
            assert_eq!(
                screen.read_rect(broom_dest, Dimensions(0x1e02)).unwrap(),
                vec![0xaa; 120]
            );
            assert_eq!(data[0x31e8..0x3260], [0x11; 120]);
            assert_eq!(data[0x327a..0x328e], unchanged_broom_state);
        }
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x39e1], data[0x1f80]),
            (232, 96, 0, 3)
        );
    }

    fn fixture() -> (Vec<u8>, Framebuffer) {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0xfa59);
        set_word(&mut data, 0x579, 280);
        data[0x57b] = 180;
        data[0x1f80] = 3;
        set_word(&mut data, 0x327d, 0);
        data[0x327f] = 160;
        data[0x3286] = 1;
        // Dormant original dog bypasses broom composition in isolated tests.
        data[0x1cbf] = 1;
        for hole in 0..16 {
            set_word(&mut data, 0x1137 + hole * 2, (hole as u16 % 8) * 32);
            data[0x1050 + hole] = 48 + (hole as u8 / 8) * 48;
            data[0x3ce3 + hole] = (15 - hole) as u8;
            let y = data[0x1050 + hole];
            set_word(
                &mut data,
                0x3c5a + hole * 4,
                pixel_offset((hole as u16 % 8) * 32, y),
            );
        }
        for slot in 0..4 {
            set_word(&mut data, 0x3ec2 + slot * 2, 0x3de6 + slot as u16 * 48);
            set_word(&mut data, 0x3eba + slot * 2, slot as u16);
        }
        set_word(&mut data, 0x3de0, 0x3d20);
        set_word(&mut data, 0x3de2, 0x3d50);
        for phase in (0..=12).step_by(2) {
            set_word(&mut data, 0x3d06 + phase, 0x4000 + phase as u16 * 32);
        }
        data[0x3d20..0x3de0].fill(0x55);
        data[0x4000..0x4280].fill(0x33);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        (data, screen)
    }

    #[test]
    fn initializer_uses_distinct_external_holes_without_resetting_clock() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x3edc, 234);
        initialize(&mut data, &mut screen).unwrap();
        assert_eq!(data[0x3ed8], 4);
        assert_eq!(word(&data, 0x3eda), 0);
        assert_eq!(word(&data, 0x3edc), 234);
        for slot in 0..4 {
            assert_eq!((data[0x3eae + slot], data[0x3eb2 + slot]), (1, 0));
            assert!((20..=35).contains(&data[0x3eb6 + slot]));
            let hole = usize::from(word(&data, 0x3eba + slot * 2));
            assert!(hole < 16);
            assert_eq!(
                word(&data, 0x3ecc + slot * 2),
                word(&data, 0x1137 + hole * 2) + 8
            );
            for other in 0..slot {
                assert_ne!(word(&data, 0x3eba + other * 2), hole as u16);
            }
        }
    }

    #[test]
    fn cadence_leaves_slot_three_unlatched_and_consumed_slots_unchanged() {
        let (mut data, mut screen) = fixture();
        data[0x3eb2..0x3eb6].fill(1);
        for (tick, expected) in [
            (1, Some(1)),
            (1, None),
            (2, Some(2)),
            (3, Some(3)),
            (3, Some(0)),
            (3, None),
        ] {
            let update = advance_mice(
                &mut data,
                &mut screen,
                tick,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(update.accepted, expected.is_some());
            assert!(!update.drew);
            if let Some(slot) = expected {
                assert_eq!(word(&data, 0x3eda), slot);
            }
        }
    }

    #[test]
    fn inclusive_mouse_contact_collects_only_drawn_phase_below_twenty() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x3ecc, 100);
        data[0x3ed4] = 100;
        for (x, y, hit) in [
            (76, 86, true),
            (116, 112, true),
            (75, 100, false),
            (117, 100, false),
            (100, 85, false),
            (100, 113, false),
        ] {
            set_word(&mut data, 0x579, x);
            data[0x57b] = y;
            assert_eq!(mouse_contact(&data, 0), hit);
        }
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        set_word(&mut data, 0x3ea6, pixel_offset(100, 100));
        data[0x3ed8] = 1;
        data[0x552] = 1;
        data[0x55c] = 1;
        for (not_drawn, phase, collected) in [(1, 19, false), (0, 20, false), (0, 19, true)] {
            set_word(&mut data, 0x3eda, 3);
            set_word(&mut data, 0x3edc, 0);
            data[0x3eae] = not_drawn;
            data[0x3eb6] = phase;
            let step = advance_mice(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(step.collected, collected);
            assert_eq!(data[0x3eb6], phase);
        }
        assert_eq!(
            (
                data[0x3eb2],
                data[0x55c],
                data[0x3ed8],
                data[0x553],
                data[0x552],
                data[0x1f80]
            ),
            (1, 0, 0, 1, 1, 3)
        );
    }

    #[test]
    fn broom_freezes_phase_before_motion_and_near_player_uses_complement_distance() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x3eda, 3);
        set_word(&mut data, 0x3ecc, 100);
        data[0x3ed4] = 100;
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        data[0x3eb6] = 10;
        let before = screen.bytes().to_vec();
        let step = advance_mice(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!(data[0x3eb6], 10);
        assert_eq!(screen.bytes().as_slice(), before);
        set_word(&mut data, 0x579, 102);
        data[0x57b] = 102;
        assert!(near_player(&data, 0, 3));
        assert!(!near_player(&data, 0, 2));
    }

    #[test]
    fn transfer_gates_then_copies_source_and_destination_frames_without_life_loss() {
        let (mut data, mut screen) = fixture();
        data[0x39e0] = 1;
        data[0x55c] = 1;
        assert!(
            !advance_transfer(&mut data, &mut screen, 11)
                .unwrap()
                .accepted
        );
        for (offset, value) in [(0x584, 1), (0x69a, 128), (0x39e0, 0)] {
            let old = data[offset];
            data[offset] = value;
            assert!(
                !advance_transfer(&mut data, &mut screen, 12)
                    .unwrap()
                    .accepted
            );
            data[offset] = old;
        }
        let old_player = (word(&data, 0x579), data[0x57b]);
        let first = advance_transfer(&mut data, &mut screen, 12).unwrap();
        assert!(first.started && first.accepted && first.active);
        assert_eq!((data[0x39e1], data[0x69a], data[0x55c]), (12, 16, 0));
        assert_eq!((word(&data, 0x579), data[0x57b]), old_player);
        let before = data.clone();
        assert!(
            !advance_transfer(&mut data, &mut screen, 12)
                .unwrap()
                .accepted
        );
        assert_eq!(data, before);
        for tick in 13..=18 {
            let step = advance_transfer(&mut data, &mut screen, tick).unwrap();
            assert!(step.accepted && step.drew);
            assert_eq!(step.finished, tick == 18);
            if tick < 15 {
                assert_eq!((word(&data, 0x579), data[0x57b]), old_player);
            } else {
                assert_eq!(
                    (word(&data, 0x579), data[0x57b], data[0x57c]),
                    (232, 96, 146)
                );
            }
        }
        assert_eq!(data[0x39e1], 0);
        assert_eq!(data[0x1f80], 3);
    }
}
