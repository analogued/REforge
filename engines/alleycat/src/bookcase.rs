// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 3 plant collection and the spider's pursuit and background drawing.
//!
//! The caller suspends and recomposes the player image around these helpers.
//! Spider contact starts its original effect and requests a nine-tick wait.
//! The caller completes that wait before setting the room-failure flag.

use crate::assets::{DATA_LENGTH, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TargetStep {
    pub quiet: Option<crate::room::QuietWork>,
    pub accepted: bool,
    pub drew: bool,
    pub collected: bool,
    pub player_changed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SpiderStep {
    pub quiet: Option<crate::room::QuietWork>,
    pub accepted: bool,
    pub drew: bool,
    pub restored: bool,
    pub contact: bool,
    pub wait_ticks: u16,
    pub failure_pending: bool,
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable bookcase data segment is truncated".into())
    } else {
        Ok(())
    }
}

// CS:2e29 uses inclusive endpoints, wrapping sums and borrow-clamped lows.
fn overlap(a: (u16, u8, u16, u8), b: (u16, u8, u16, u8)) -> bool {
    a.0.wrapping_add(a.2) >= b.0
        && a.0.saturating_sub(b.2) <= b.0
        && a.1.wrapping_add(a.3) >= b.1
        && a.1.saturating_sub(b.3) <= b.1
}

fn spider_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x3964), data[0x3966], 24, 14)
}

fn player_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x579), data[0x57b], 24, 14)
}

pub(crate) fn spider_contact(data: &[u8]) -> bool {
    overlap(spider_rect(data), player_rect(data))
}

fn broom_contact(data: &[u8]) -> bool {
    overlap(
        spider_rect(data),
        (word(data, 0x327d), data[0x327f], 16, 30),
    )
}

/// CS:3b30 and CS:3c90 retain both scheduler clocks and the vertical phase.
/// The scenery helper has already drawn the plants; the spider's first image
/// appears on its first accepted update.
pub(crate) fn initialize(data: &mut [u8], _screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    data[0x37af] = 3;
    for slot in 0..3 {
        set_word(data, 0x37b0 + slot * 2, 1);
    }
    data[0x3966] = 8;
    data[0x396a] = 1;
    data[0x3967] = 0;
    data[0x396d] = 2;
    set_word(data, 0x3964, 280);
    set_word(data, 0x396b, 0);
    Ok(())
}

fn restore_spider(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    if data[0x396a] == 0 {
        screen.copy_rect(
            word(data, 0x3968),
            Dimensions(0x0e03),
            sprite_bytes(data, 0x396f, Dimensions(0x0e03))?,
        )?;
        data[0x2ae0] = 3;
        data[0x2ae2] = 0;
    }
    Ok(())
}

fn draw_spider(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let dest = word(data, 0x39ca);
    let source = word(data, 0x396b).wrapping_add(0x38bc);
    let dimensions = Dimensions(0x0e03);
    let saved =
        screen.zero_transparent_rect(dest, dimensions, sprite_bytes(data, source, dimensions)?)?;
    data[0x396f..0x396f + saved.len()].copy_from_slice(&saved);
    set_word(data, 0x3968, dest);
    data[0x396a] = 0;
    data[0x2ae0] = 3;
    data[0x2ae2] = 0;
    set_word(
        data,
        0x2ae3,
        word(data, usize::from(source) + dimensions.len() - 2),
    );
    Ok(())
}

/// CS:3b42 scans active plants in reverse order and consumes at most one on
/// each changed BIOS tick. The final plant succeeds only while failure is clear.
pub(crate) fn advance_targets(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<TargetStep, String> {
    advance_targets_inner(
        data,
        screen,
        tick,
        #[cfg(feature = "dev-tools")]
        false,
        sound,
    )
}

fn advance_targets_inner(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    #[cfg(feature = "dev-tools")] disable_spider: bool,
    sound: &mut crate::sound::Sound,
) -> Result<TargetStep, String> {
    check(data)?;
    if tick == word(data, 0x37b8) {
        return Ok(TargetStep {
            quiet: Some(crate::room::QuietWork::PlantTick),
            ..Default::default()
        });
    }
    set_word(data, 0x37b8, tick);
    let mut result = TargetStep {
        accepted: true,
        ..TargetStep::default()
    };
    for slot in (0..3).rev() {
        let offset = slot * 2;
        set_word(data, 0x37b6, offset as u16);
        if word(data, 0x37b0 + offset) == 0
            || !overlap((word(data, 0x37a3 + offset), 24, 16, 16), player_rect(data))
        {
            continue;
        }
        sound.request(data, tick, 0x0c00, 0x08fd);
        #[cfg(feature = "dev-tools")]
        if !disable_spider {
            restore_spider(data, screen)?;
        }
        #[cfg(not(feature = "dev-tools"))]
        restore_spider(data, screen)?;
        set_word(data, 0x37b0 + offset, 0);
        data[0x0e..0x4e].fill(0xaa);
        screen.copy_rect(
            word(data, 0x37a9 + offset),
            Dimensions(0x1002),
            &data[0x0e..0x4e],
        )?;
        data[0x2ae0] = 2;
        data[0x2ae2] = 0;
        data[0x37af] = data[0x37af].wrapping_sub(1);
        if data[0x37af] == 0 && data[0x552] == 0 {
            data[0x553] = 1;
        }
        // CS:1124 refreshes the player background after erasing the plant.
        // PlayerCompositor owns that buffer in the authored runtime.
        data[0x583] = 0;
        #[cfg(feature = "dev-tools")]
        if !disable_spider {
            draw_spider(data, screen)?;
        }
        #[cfg(not(feature = "dev-tools"))]
        draw_spider(data, screen)?;
        result.drew = true;
        result.collected = true;
        result.player_changed = true;
        return Ok(result);
    }
    set_word(data, 0x37b6, 0xfffe);
    result.quiet = Some(crate::room::QuietWork::PlantMiss);
    Ok(result)
}

/// CS:3cb1 accepts every two ticks, moves horizontally at the top, then follows
/// the original cyclic vertical step. Broom collisions roll back coordinates
/// while retaining the proposed direction and phase changes.
#[cfg(test)]
pub(crate) fn advance_spider(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<SpiderStep, String> {
    advance_spider_with_damage(data, screen, tick, false)
}

pub(crate) fn advance_spider_with_damage(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    suppress_damage: bool,
) -> Result<SpiderStep, String> {
    advance_spider_inner(data, screen, tick, false, suppress_damage)
}

fn spider_failure_effect(
    data: &mut [u8],
    screen: &mut Framebuffer,
    result: &mut SpiderStep,
) -> Result<(), String> {
    if data[0x553] != 0 {
        return Ok(());
    }
    draw_spider_failure(data, screen)?;
    result.drew = true;
    result.wait_ticks = 9;
    result.failure_pending = true;
    Ok(())
}

pub(crate) fn draw_spider_failure(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    let x = word(data, 0x579).saturating_sub(12).min(270);
    let y = data[0x57b].saturating_sub(4);
    let dimensions = Dimensions(0x1506);
    let saved = screen.zero_transparent_rect(
        crate::cga::pixel_offset(x, y),
        dimensions,
        sprite_bytes(data, 0x37c0, dimensions)?,
    )?;
    data[0x000e..0x000e + saved.len()].copy_from_slice(&saved);
    data[0x2ae0] = 6;
    data[0x2ae2] = 0;
    Ok(())
}

/// CS:3de8 runs after the contact effect's nine-tick wait. The room return
/// presentation, rather than this helper, removes the life.
pub(crate) fn finish_spider_failure(data: &mut [u8]) -> Result<(), String> {
    check(data)?;
    data[0x552] = 1;
    Ok(())
}

fn advance_spider_inner(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
    suppress_damage: bool,
) -> Result<SpiderStep, String> {
    check(data)?;
    if tick.wrapping_sub(word(data, 0x39c8)) < 2 {
        return Ok(SpiderStep {
            quiet: Some(crate::room::QuietWork::SpiderTick),
            ..Default::default()
        });
    }
    set_word(data, 0x39c8, tick);
    let mut result = SpiderStep {
        accepted: true,
        ..SpiderStep::default()
    };
    let broom_enabled = !disable_broom;
    if broom_enabled && broom_contact(data) {
        result.quiet = Some(crate::room::QuietWork::SpiderBlocked);
        return Ok(result);
    }
    result.contact = spider_contact(data);
    if result.contact && !suppress_damage {
        spider_failure_effect(data, screen, &mut result)?;
        return Ok(result);
    }
    let difficulty = usize::from(word(data, 8));
    if difficulty >= 8 {
        return Err("Bookcase spider difficulty exceeds its speed table".into());
    }
    let speed = word(data, 0x39cc + difficulty * 2);
    set_word(data, 0x39c6, speed);
    let old_x = word(data, 0x3964);
    let old_y = data[0x3966];
    set_word(data, 0x39c3, old_x);
    data[0x39c5] = old_y;
    let aligned = old_x & 0xfff8 == word(data, 0x579) & 0xfff8;
    if old_y == 8 && !aligned {
        let x = if old_x & 0xfff8 > word(data, 0x579) & 0xfff8 {
            old_x.saturating_sub(speed)
        } else {
            old_x.wrapping_add(speed)
        };
        set_word(data, 0x3964, x);
    } else {
        if old_y == 8 {
            data[0x3967] = 1;
            data[0x396e] = 1;
        }
        data[0x396e] = data[0x396e].wrapping_add(1);
        let step = ((data[0x396e] >> 2) & 3) + 2;
        let y = if data[0x3967] != 1 {
            let (y, borrowed) = old_y.overflowing_sub(step);
            if borrowed || y < 9 {
                data[0x3967] = 0;
                8
            } else {
                y
            }
        } else {
            let mut y = old_y.wrapping_add(step);
            let (distance, borrowed) = old_x.overflowing_sub(word(data, 0x579));
            let distance = if borrowed { !distance } else { distance };
            if y > data[0x57b] || distance > 48 {
                data[0x3967] = 255;
            } else if y >= 160 {
                y = 159;
                data[0x3967] = 255;
            }
            y
        };
        data[0x3966] = y;
    }
    if broom_enabled && broom_contact(data) {
        set_word(data, 0x3964, old_x);
        data[0x3966] = old_y;
        result.quiet = Some(crate::room::QuietWork::SpiderRollback);
        return Ok(result);
    }
    let moved_contact = spider_contact(data);
    result.contact |= moved_contact;
    if moved_contact && !suppress_damage {
        spider_failure_effect(data, screen, &mut result)?;
        return Ok(result);
    }
    set_word(data, 0x39ca, pixel_offset(word(data, 0x3964), data[0x3966]));
    result.restored = data[0x396a] == 0;
    restore_spider(data, screen)?;
    data[0x396d] = data[0x396d].wrapping_sub(1);
    if data[0x396d] == 0 {
        data[0x396d] = 2;
        set_word(data, 0x396b, word(data, 0x396b) ^ 0x54);
    }
    draw_spider(data, screen)?;
    result.drew = true;
    Ok(result)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn remove_spider(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    check(data)?;
    let drew = data[0x396a] == 0;
    restore_spider(data, screen)?;
    data[0x396a] = 1;
    Ok(drew)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_targets_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_spider: bool,
    sound: &mut crate::sound::Sound,
) -> Result<TargetStep, String> {
    if !disable_spider {
        return advance_targets(data, screen, tick, sound);
    }
    let erased = if disable_spider {
        remove_spider(data, screen)?
    } else {
        false
    };
    let mut result = advance_targets_inner(data, screen, tick, disable_spider, sound)?;
    result.drew |= erased;
    Ok(result)
}

#[cfg(all(feature = "dev-tools", test))]
pub(crate) fn advance_spider_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
) -> Result<SpiderStep, String> {
    advance_spider_dev_with_damage(data, screen, tick, disable_broom, false)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_spider_dev_with_damage(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_broom: bool,
    suppress_damage: bool,
) -> Result<SpiderStep, String> {
    if !disable_broom {
        return advance_spider_with_damage(data, screen, tick, suppress_damage);
    }
    advance_spider_inner(data, screen, tick, disable_broom, suppress_damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "dev-tools")]
    #[test]
    fn collecting_plants_cannot_redraw_a_disabled_spider() {
        let (mut data, mut screen) = fixture();
        draw_spider(&mut data, &mut screen).unwrap();
        set_word(&mut data, 0x579, 208);
        data[0x57b] = 24;
        let step = advance_targets_dev(
            &mut data,
            &mut screen,
            1,
            true,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.collected && step.drew);
        assert_eq!(data[0x396a], 1);
        assert_eq!(
            screen
                .read_rect(pixel_offset(280, 8), Dimensions(0x0e03))
                .unwrap(),
            vec![0xaa; 84]
        );
        assert!(!remove_spider(&mut data, &mut screen).unwrap());
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn disabled_broom_cannot_stop_or_roll_back_the_spider() {
        for broom_x in [264, 263] {
            let (mut data, mut screen) = fixture();
            set_word(&mut data, 0x327d, broom_x);
            data[0x327f] = 8;
            let step = advance_spider_dev(&mut data, &mut screen, 2, true).unwrap();
            assert!(step.accepted && step.drew);
            assert_eq!(word(&data, 0x3964), 276);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn development_false_wrappers_preserve_default_bookcase_state_and_pixels() {
        let (mut data, mut screen) = fixture();
        let mut alternate = data.clone();
        let mut alternate_screen = screen.clone();
        assert_eq!(
            advance_spider_with_damage(&mut data, &mut screen, 2, true).unwrap(),
            advance_spider_dev(&mut alternate, &mut alternate_screen, 2, false).unwrap()
        );
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());
        set_word(&mut data, 0x579, 208);
        set_word(&mut alternate, 0x579, 208);
        data[0x57b] = 24;
        alternate[0x57b] = 24;
        assert_eq!(
            advance_targets(
                &mut data,
                &mut screen,
                3,
                &mut crate::sound::Sound::default()
            )
            .unwrap(),
            advance_targets_dev(
                &mut alternate,
                &mut alternate_screen,
                3,
                false,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
        );
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());
    }

    fn fixture() -> (Vec<u8>, Framebuffer) {
        let mut data = vec![0; DATA_LENGTH];
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        for slot in 0..3 {
            let x = 192 + slot as u16 * 32;
            set_word(&mut data, 0x37a3 + slot * 2, x);
            set_word(&mut data, 0x37a9 + slot * 2, pixel_offset(x, 24));
        }
        for difficulty in 0..8 {
            set_word(&mut data, 0x39cc + difficulty * 2, 4);
        }
        data[0x38bc..0x3964].fill(0x33);
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        set_word(&mut data, 0x327d, 0);
        data[0x327f] = 170;
        data[0x1f80] = 3;
        initialize(&mut data, &mut screen).unwrap();
        set_word(&mut data, 0x39ca, pixel_offset(280, 8));
        (data, screen)
    }

    #[test]
    fn initialization_preserves_clocks_vertical_phase_and_framebuffer() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x37b8, 123);
        set_word(&mut data, 0x39c8, 456);
        data[0x396e] = 99;
        let before = screen.bytes().to_vec();
        initialize(&mut data, &mut screen).unwrap();
        assert_eq!(
            (word(&data, 0x37b8), word(&data, 0x39c8), data[0x396e]),
            (123, 456, 99)
        );
        assert_eq!(
            (
                data[0x37af],
                word(&data, 0x37b0),
                word(&data, 0x37b2),
                word(&data, 0x37b4)
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(
            (
                word(&data, 0x3964),
                data[0x3966],
                data[0x396a],
                data[0x396d]
            ),
            (280, 8, 1, 2)
        );
        assert_eq!(screen.bytes().as_slice(), before);
    }

    #[test]
    fn plant_contact_is_inclusive_and_consumes_only_highest_active_slot() {
        let (data, screen) = fixture();
        for (x, y, hit) in [
            (232, 24, true),
            (272, 24, true),
            (231, 24, false),
            (273, 24, false),
            (256, 10, true),
            (256, 40, true),
            (256, 9, false),
            (256, 41, false),
        ] {
            let mut trial = data.clone();
            let mut pixels = screen.clone();
            set_word(&mut trial, 0x37b0, 0);
            set_word(&mut trial, 0x37b2, 0);
            set_word(&mut trial, 0x579, x);
            trial[0x57b] = y;
            assert_eq!(
                advance_targets(
                    &mut trial,
                    &mut pixels,
                    1,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .collected,
                hit
            );
        }
        let mut data = data;
        let mut screen = screen;
        set_word(&mut data, 0x579, 208);
        data[0x57b] = 24;
        let step = advance_targets(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.accepted && step.collected && step.drew && step.player_changed);
        assert_eq!(
            (
                word(&data, 0x37b0),
                word(&data, 0x37b2),
                word(&data, 0x37b4),
                data[0x37af]
            ),
            (1, 0, 1, 2)
        );
        assert!(
            !advance_targets(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .accepted
        );
    }

    #[test]
    fn final_plant_guards_success_and_erases_with_original_background_color() {
        for failure in [0, 1] {
            let (mut data, mut screen) = fixture();
            set_word(&mut data, 0x37b0, 0);
            set_word(&mut data, 0x37b2, 0);
            data[0x37af] = 1;
            data[0x552] = failure;
            data[0x583] = 7;
            set_word(&mut data, 0x579, 256);
            data[0x57b] = 24;
            screen
                .copy_rect(pixel_offset(256, 24), Dimensions(0x1002), &[0xff; 64])
                .unwrap();
            advance_targets(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(
                (
                    data[0x37af],
                    data[0x553],
                    data[0x552],
                    data[0x1f80],
                    data[0x583]
                ),
                (0, 1 - failure, failure, 3, 0)
            );
            assert_eq!(
                screen
                    .read_rect(pixel_offset(256, 24), Dimensions(0x1002))
                    .unwrap(),
                vec![0xaa; 64]
            );
        }
    }

    #[test]
    fn spider_scheduler_wrap_and_animation_draw_restore_the_full_background() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x39c8, 0xffff);
        assert!(
            !advance_spider_with_damage(&mut data, &mut screen, 0, true)
                .unwrap()
                .accepted
        );
        let first = advance_spider_with_damage(&mut data, &mut screen, 1, true).unwrap();
        assert!(first.drew && !first.restored);
        assert_eq!(
            (word(&data, 0x3964), word(&data, 0x396b), data[0x396d]),
            (276, 0, 1)
        );
        assert_eq!(&data[0x396f..0x39c3], &[0xaa; 84]);
        assert_eq!(word(&data, 0x2ae3), 0x3333);
        let next = advance_spider_with_damage(&mut data, &mut screen, 3, true).unwrap();
        assert!(next.drew && next.restored);
        assert_eq!(
            (word(&data, 0x3964), word(&data, 0x396b), data[0x396d]),
            (272, 0x54, 2)
        );
        assert_eq!(
            screen
                .read_rect(pixel_offset(296, 8), Dimensions(0x0e01))
                .unwrap(),
            vec![0xaa; 28]
        );
    }

    #[test]
    fn spider_tracks_rounded_x_then_cycles_vertical_steps_and_turns_at_bottom() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x579, 283);
        data[0x57b] = 180;
        advance_spider_with_damage(&mut data, &mut screen, 2, true).unwrap();
        assert_eq!(
            (
                word(&data, 0x3964),
                data[0x3966],
                data[0x3967],
                data[0x396e]
            ),
            (280, 10, 1, 2)
        );
        advance_spider_with_damage(&mut data, &mut screen, 4, true).unwrap();
        assert_eq!(data[0x3966], 12);
        advance_spider_with_damage(&mut data, &mut screen, 6, true).unwrap();
        assert_eq!(data[0x3966], 15);
        data[0x3966] = 159;
        data[0x396e] = 0;
        advance_spider_with_damage(&mut data, &mut screen, 8, true).unwrap();
        assert_eq!((data[0x3966], data[0x3967]), (159, 255));
        data[0x3966] = 10;
        data[0x396e] = 11;
        advance_spider_with_damage(&mut data, &mut screen, 10, true).unwrap();
        assert_eq!((data[0x3966], data[0x3967]), (8, 0));
    }

    #[test]
    fn broom_blocks_before_movement_and_rolls_back_only_proposed_coordinates() {
        let (mut data, mut screen) = fixture();
        data[0x327f] = 8;
        // Native predicate extends the spider left by the broom's width.
        set_word(&mut data, 0x327d, 264);
        let before = data.clone();
        let pixels = screen.bytes().to_vec();
        let step = advance_spider_with_damage(&mut data, &mut screen, 2, true).unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!(word(&data, 0x3964), word(&before, 0x3964));
        assert_eq!(screen.bytes().as_slice(), pixels);
        // One pixel beyond the old overlap; the four-pixel proposed move hits.
        set_word(&mut data, 0x327d, 263);
        advance_spider_with_damage(&mut data, &mut screen, 4, true).unwrap();
        assert_eq!(word(&data, 0x3964), 280);
        assert_eq!(word(&data, 0x39c3), 280);
        assert_eq!(data[0x396d], 2);

        set_word(&mut data, 0x327d, 280);
        data[0x327f] = 46;
        data[0x3966] = 31;
        data[0x3967] = 1;
        data[0x396e] = 3;
        set_word(&mut data, 0x579, 280);
        let step = advance_spider_with_damage(&mut data, &mut screen, 6, true).unwrap();
        assert!(step.accepted && !step.drew);
        assert_eq!((data[0x3966], data[0x3967], data[0x396e]), (31, 1, 4));
    }

    #[test]
    fn spider_contact_preserves_lives_and_failure_and_does_not_freeze_pursuit() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x579, 280);
        data[0x57b] = 8;
        for (x, y, hit) in [
            (256, 8, true),
            (304, 8, true),
            (255, 8, false),
            (305, 8, false),
            (280, 22, true),
            (280, 23, false),
        ] {
            set_word(&mut data, 0x579, x);
            data[0x57b] = y;
            assert_eq!(spider_contact(&data), hit);
        }
        set_word(&mut data, 0x579, 280);
        data[0x57b] = 8;
        let step = advance_spider_with_damage(&mut data, &mut screen, 2, true).unwrap();
        assert!(step.contact && step.drew);
        assert_eq!(
            (data[0x3966], data[0x552], data[0x553], data[0x1f80]),
            (10, 0, 0, 3)
        );
    }

    #[test]
    fn spider_harm_draws_effect_then_defers_failure_and_preserves_success_immunity() {
        for completed in [false, true] {
            let (mut data, mut screen) = fixture();
            set_word(&mut data, 0x579, 280);
            data[0x57b] = 8;
            data[0x553] = u8::from(completed);
            data[0x37c0..0x38bc].fill(0x55);
            let before = screen.bytes().to_vec();
            let step = advance_spider(&mut data, &mut screen, 2).unwrap();
            assert!(step.contact && step.accepted);
            assert_eq!(
                (step.failure_pending, step.wait_ticks, data[0x552]),
                (!completed, if completed { 0 } else { 9 }, 0)
            );
            assert_eq!(
                (word(&data, 0x3964), data[0x3966], data[0x1f80]),
                (280, 8, 3)
            );
            if completed {
                assert_eq!(screen.bytes().as_slice(), before);
            } else {
                assert_eq!(
                    screen
                        .read_rect(pixel_offset(268, 4), Dimensions(0x1506))
                        .unwrap(),
                    [0x55; 252]
                );
                assert_eq!(&data[0x000e..0x010a], &[0xaa; 252]);
                finish_spider_failure(&mut data).unwrap();
                assert_eq!((data[0x552], data[0x1f80]), (1, 3));
            }
        }
    }
}
