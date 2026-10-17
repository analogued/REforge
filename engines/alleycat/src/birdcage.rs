// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 5 cage pushing, a bounded cage fall, and the released bird.
//!
//! Player coordinates are mirrored in the original data segment. After a cage
//! step moves the player, the caller synchronizes its Player and redraws its
//! current image. A falling cage blocks ordinary room updates until released.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CageStep {
    pub player_moved: bool,
    pub forced_descent: bool,
    pub drew: bool,
    pub falling: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FallStep {
    pub drew: bool,
    pub released: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BirdStep {
    pub drew: bool,
    pub caught: bool,
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable birdcage data segment is truncated".into())
    } else {
        Ok(())
    }
}

// CS:2e29: inclusive endpoints, wrapping addition, and borrow-clamped lower
// endpoints. Preserve byte arithmetic for Y, including its overflow behavior.
fn overlap(first: (u16, u8, u16, u8), second: (u16, u8, u16, u8)) -> bool {
    let (x, y, w, h) = first;
    let (other_x, other_y, ow, oh) = second;
    x.wrapping_add(w) >= other_x
        && x.saturating_sub(ow) <= other_x
        && y.wrapping_add(h) >= other_y
        && y.saturating_sub(oh) <= other_y
}

pub(crate) fn cage_contact(data: &[u8]) -> bool {
    overlap(
        (word(data, 0x40a8), data[0x40aa], 24, 16),
        (word(data, 0x579), data[0x57b], 24, 14),
    )
}

/// CS:452d sets success only while failure is clear. Its original CF is not a
/// reliable contact result because the failure comparison overwrites flags.
pub(crate) fn capture(data: &mut [u8]) -> bool {
    let hit = overlap(
        (word(data, 0x40b2), data[0x40b4], 8, 5),
        (word(data, 0x579), data[0x57b], 24, 14),
    );
    if hit && data[0x552] == 0 {
        data[0x553] = 1;
    }
    hit
}

pub(crate) fn broom_contact(data: &mut [u8]) -> bool {
    let hit = overlap(
        (word(data, 0x40b2), data[0x40b4], 8, 5),
        (word(data, 0x327d), data[0x327f], 16, 30),
    );
    if hit {
        data[0x40b8] = 255;
    }
    hit
}

fn shelf_player(data: &[u8]) -> bool {
    data[0x571] == 0 && data[0x57b] & 0xf8 == 0x88
}

fn force_descent(data: &mut [u8]) -> bool {
    if shelf_player(data) {
        data[0x571] = 1;
        data[0x55b] = 16;
        true
    } else {
        false
    }
}

fn shelf_broom(data: &[u8]) -> bool {
    let low = word(data, 0x40a8).wrapping_sub(20);
    let broom = word(data, 0x327d);
    data[0x327f] >= 102 && low <= broom && low.wrapping_add(48) >= broom
}

fn draw_cage(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let dest = word(data, 0x40ab);
    let saved = screen.and_rect(
        dest,
        Dimensions(0x1003),
        sprite_bytes(data, 0x3fbe, Dimensions(0x1003))?,
    )?;
    data[0x401e..0x401e + saved.len()].copy_from_slice(&saved);
    set_word(data, 0x40a6, dest);
    data[0x2ae0] = 3;
    data[0x2ae2] = 0;
    Ok(())
}

fn restore_cage(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    data[0x2ae0] = 3;
    data[0x2ae2] = 0;
    screen.copy_rect(
        word(data, 0x40a6),
        Dimensions(0x1003),
        &data[0x401e..0x407e],
    )
}

/// CS:457a resets only the listed cage/bird fields, retaining clocks and phase.
pub(crate) fn initialize(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    set_word(data, 0x40a8, 144);
    data[0x40aa] = 134;
    set_word(data, 0x40ab, pixel_offset(144, 134));
    draw_cage(data, screen)?;
    data[0x40af] = 0;
    data[0x40b1] = 0;
    data[0x40b9] = 1;
    data[0x40b8] = 0;
    set_word(data, 0x40c8, 255);
    Ok(())
}

// CS:595d consumes shared random state only when a new asynchronous tone starts.
fn random_tone(data: &mut [u8], tick: u16, sound: &mut crate::sound::Sound) {
    if data[0] != 0 && data[0x5920] == 0 {
        let period = (next_random(data) & 127).wrapping_add(0xaa);
        sound.request(data, tick, period.wrapping_add(30), period);
    }
}

/// CS:45ab through entry to its blocking fall. Player image restoration/redraw
/// at CS:11e3/1112 is delegated to the caller's PlayerCompositor.
pub(crate) fn advance_cage(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<CageStep, String> {
    check(data)?;
    let mut result = CageStep::default();
    if tick == word(data, 0x40ad) {
        return Ok(result);
    }
    set_word(data, 0x40ad, tick);
    if data[0x40aa] >= 164 {
        return Ok(result);
    }
    if shelf_broom(data) {
        result.forced_descent = force_descent(data);
        return Ok(result);
    }
    if cage_contact(data) {
        if data[0x40af] == 0 {
            data[0x40b0] = if data[0x56e] != 0 {
                data[0x56e]
            } else if word(data, 0x40a8) > word(data, 0x579) {
                1
            } else {
                255
            };
        }
        data[0x40af] = 1;
        for _ in 0..32 {
            let right = data[0x40b0] != 1;
            let x = word(data, 0x579);
            set_word(
                data,
                0x579,
                if right {
                    x.wrapping_add(8)
                } else {
                    x.wrapping_sub(8)
                },
            );
            data[0x56e] = if right { 1 } else { 255 };
            match data[0x571] {
                0 => {}
                1 => {
                    data[0x57b] = data[0x57b].wrapping_sub(3);
                    data[0x57c] = data[0x57b].wrapping_add(50);
                }
                _ => {
                    data[0x57b] = data[0x57b].wrapping_add(3);
                    data[0x57c] = data[0x57b].wrapping_add(50);
                }
            }
            if !cage_contact(data) {
                break;
            }
        }
        result.player_moved = true;
        return Ok(result);
    }
    if data[0x40b1] == 0 {
        if data[0x40af] == 0 {
            return Ok(result);
        }
        let x = word(data, 0x40a8);
        let x = if data[0x40b0] == 1 {
            x.wrapping_add(8)
        } else {
            x.wrapping_sub(8)
        };
        set_word(data, 0x40a8, x);
        if cage_contact(data) {
            return Ok(result);
        }
        sound.request(data, tick, 0xc00, 0xb54);
        data[0x40af] = 0;
        set_word(data, 0x40ab, pixel_offset(x, data[0x40aa]));
        restore_cage(data, screen)?;
        draw_cage(data, screen)?;
        result.drew = true;
        if (120..=168).contains(&x) {
            return Ok(result);
        }
    }
    data[0x40b1] = 1;
    if data[0x1cbf] != 0 {
        result.forced_descent = force_descent(data);
    } else {
        result.falling = true;
    }
    Ok(result)
}

/// Advance one iteration of the blocking CS:46be fall. Same-tick calls wait;
/// a changed tick grows Y by five, or breaks a previously landed cage. The
/// caller keeps room/player work suspended until `released` becomes true.
pub(crate) fn advance_fall(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<FallStep, String> {
    check(data)?;
    let mut result = FallStep::default();
    if tick == word(data, 0x40ad) {
        return Ok(result);
    }
    set_word(data, 0x40ad, tick);
    if data[0x40aa] < 164 {
        data[0x40aa] = data[0x40aa].wrapping_add(5);
        set_word(data, 0x40ab, pixel_offset(word(data, 0x40a8), data[0x40aa]));
        restore_cage(data, screen)?;
        draw_cage(data, screen)?;
        result.drew = true;
    } else {
        restore_cage(data, screen)?;
        let dest = word(data, 0x40a6).wrapping_sub(1);
        set_word(data, 0x40a6, dest);
        let saved = screen.and_rect(
            dest,
            Dimensions(0x1104),
            sprite_bytes(data, 0x3f36, Dimensions(0x1104))?,
        )?;
        data[0x401e..0x401e + saved.len()].copy_from_slice(&saved);
        data[0x2ae0] = 4;
        data[0x2ae2] = 0;
        set_word(data, 0x40b2, word(data, 0x40a8));
        data[0x40b4] = data[0x40aa];
        measure_separation(data);
        data[0x40b7] = data[0x40ca];
        result.drew = true;
        result.released = true;
    }
    Ok(result)
}

fn measure_separation(data: &mut [u8]) {
    let (mut x, borrow) = word(data, 0x40b2).overflowing_sub(word(data, 0x579));
    data[0x40ca] = if borrow {
        x = !x;
        255
    } else {
        1
    };
    let (mut y, borrow) = data[0x40b4].overflowing_sub(data[0x57b]);
    data[0x40cb] = if borrow {
        y = !y;
        255
    } else {
        1
    };
    set_word(data, 0x40cc, x.wrapping_add(u16::from(y) * 2));
}

/// CS:442e movement/drawing entry, separately useful for controlled comparisons.
pub(crate) fn advance_motion(
    data: &mut [u8],
    screen: &mut Framebuffer,
) -> Result<BirdStep, String> {
    check(data)?;
    match data[0x40b8] {
        0 => {}
        1 => {
            let y = data[0x40b4].wrapping_add(2);
            data[0x40b4] = if y >= 168 {
                data[0x40b8] = 255;
                167
            } else {
                y
            };
        }
        _ => {
            let y = data[0x40b4].wrapping_sub(2);
            data[0x40b4] = if y < 48 {
                data[0x40b8] = 1;
                48
            } else {
                y
            };
        }
    }
    match data[0x40b7] {
        0 => {}
        1 => {
            let x = word(data, 0x40b2).wrapping_add(4);
            let next = if x >= 310 {
                data[0x40b7] = 255;
                309
            } else {
                x
            };
            set_word(data, 0x40b2, next);
        }
        _ => {
            let (x, borrow) = word(data, 0x40b2).overflowing_sub(4);
            let next = if borrow {
                data[0x40b7] = 1;
                0
            } else {
                x
            };
            set_word(data, 0x40b2, next);
        }
    }
    capture(data);
    let dest = pixel_offset(word(data, 0x40b2), data[0x40b4]);
    set_word(data, 0x40bc, dest);
    if data[0x40b9] == 0 {
        screen.copy_rect(
            word(data, 0x40ba),
            Dimensions(0x0501),
            &data[0x3f2c..0x3f36],
        )?;
        data[0x2ae0] = 1;
        data[0x2ae2] = 0;
    }
    if broom_contact(data) {
        return Ok(BirdStep {
            drew: false,
            caught: data[0x553] != 0,
        });
    }
    data[0x40b9] = 0;
    let phase = word(data, 0x40be).wrapping_add(2);
    set_word(data, 0x40be, phase);
    let source = word(data, 0x40c0 + usize::from(phase & 6)).wrapping_add(if data[0x40b7] == 255 {
        30
    } else {
        0
    });
    set_word(data, 0x40ba, dest);
    let saved = screen.zero_transparent_rect(
        dest,
        Dimensions(0x0501),
        sprite_bytes(data, source, Dimensions(0x0501))?,
    )?;
    data[0x3f2c..0x3f36].copy_from_slice(&saved);
    data[0x2ae0] = 1;
    data[0x2ae2] = 0;
    set_word(data, 0x2ae3, word(data, usize::from(source) + 8));
    Ok(BirdStep {
        drew: true,
        caught: data[0x553] != 0,
    })
}

/// CS:4340 scheduler, random choices, avoidance and external waypoints.
pub(crate) fn advance_bird(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<BirdStep, String> {
    check(data)?;
    let held = BirdStep {
        drew: false,
        caught: data[0x553] != 0,
    };
    if tick == word(data, 0x40b5) {
        return Ok(held);
    }
    data[0x40ff] = data[0x40ff].wrapping_add(1);
    if data[0x40ff] & 3 != 0 {
        set_word(data, 0x40b5, tick);
    }
    if data[0x40aa] < 164 {
        return Ok(held);
    }
    capture(data);
    if broom_contact(data) {
        return Ok(BirdStep {
            drew: false,
            caught: data[0x553] != 0,
        });
    }
    if next_random(data) as u8 <= 0x30 {
        measure_separation(data);
        let difficulty = usize::from(word(data, 8));
        if difficulty > 7 {
            return Err("Bird difficulty exceeds its avoidance table".into());
        }
        if word(data, 0x40cc) <= word(data, 0x40ce + difficulty * 2) {
            random_tone(data, tick, sound);
            set_word(data, 0x40c8, 255);
            data[0x40b7] = data[0x40ca];
            data[0x40b8] = data[0x40cb];
            return advance_motion(data, screen);
        }
    }
    if word(data, 0x40c8) <= 10 {
        if next_random(data) as u8 <= 6 {
            set_word(data, 0x40c8, 255);
        } else {
            let target = usize::from(word(data, 0x40c8));
            let x = word(data, 0x40b2) & 0x0ffc;
            let tx = word(data, 0x40de + target * 2);
            data[0x40b7] = if x == tx {
                0
            } else if x < tx {
                1
            } else {
                255
            };
            let y = data[0x40b4] & 0xfe;
            let ty = data[0x40f4 + target];
            data[0x40b8] = if y == ty {
                0
            } else if y < ty {
                1
            } else {
                255
            };
            if data[0x40b7] | data[0x40b8] != 0 || next_random(data) as u8 > 0x10 {
                return advance_motion(data, screen);
            }
            set_word(data, 0x40c8, 255);
            random_tone(data, tick, sound);
        }
    }
    let random = next_random(data) as u8;
    if random <= 0x30 {
        data[0x40b7] = if random & 1 != 0 { 1 } else { 255 };
        data[0x40b8] = if next_random(data) & 1 != 0 { 1 } else { 255 };
    }
    let target = next_random(data) & 255;
    set_word(data, 0x40c8, target);
    advance_motion(data, screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Vec<u8>, Framebuffer) {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x579, 8);
        data[0x57b] = 180;
        data[0x57c] = 230;
        data[0x1f80] = 3;
        set_word(&mut data, 0x2ae5, 0xffff);
        data[0x3fbe..0x401e].fill(0xff);
        data[0x3f36..0x3fbe].fill(0x55);
        for index in 0..4 {
            set_word(&mut data, 0x40c0 + index * 2, 0x4100);
        }
        data[0x4100..0x4128].fill(0x40);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        (data, screen)
    }

    #[test]
    fn initialization_uses_external_mask_and_preserves_clocks_and_phase() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x40ad, 99);
        set_word(&mut data, 0x40b5, 87);
        set_word(&mut data, 0x40be, 0x1002);
        data[0x40ff] = 127;
        data[0x3fbe..0x401e].fill(0x55);
        initialize(&mut data, &mut screen).unwrap();
        assert_eq!(word(&data, 0x40a8), 144);
        assert_eq!(data[0x40aa], 134);
        assert_eq!(word(&data, 0x40a6), pixel_offset(144, 134));
        assert_eq!(
            screen
                .read_rect(pixel_offset(144, 134), Dimensions(0x1003))
                .unwrap(),
            vec![0; 96]
        );
        assert_eq!(&data[0x401e..0x407e], &[0xaa; 96]);
        assert_eq!(word(&data, 0x40ad), 99);
        assert_eq!(word(&data, 0x40b5), 87);
        assert_eq!(word(&data, 0x40be), 0x1002);
        assert_eq!(data[0x40ff], 127);
        assert_eq!(
            (data[0x40af], data[0x40b1], data[0x40b9], data[0x40b8]),
            (0, 0, 1, 0)
        );
    }

    #[test]
    fn pushing_uses_remembered_direction_and_adjusts_airborne_player_y() {
        for (direction, expected_x) in [(1, 112), (255, 176)] {
            let (mut data, mut screen) = fixture();
            initialize(&mut data, &mut screen).unwrap();
            set_word(&mut data, 0x579, 144);
            data[0x57b] = 134;
            data[0x56e] = direction;
            let step = advance_cage(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert!(step.player_moved);
            assert_eq!(word(&data, 0x579), expected_x);
            assert_eq!((data[0x40af], data[0x40b0]), (1, direction));
            assert_eq!(data[0x56e], if direction == 1 { 255 } else { 1 });
            assert_eq!(word(&data, 0x40a8), 144);
        }
        let (mut data, mut screen) = fixture();
        initialize(&mut data, &mut screen).unwrap();
        set_word(&mut data, 0x579, 144);
        data[0x57b] = 134;
        data[0x56e] = 1;
        data[0x571] = 1;
        advance_cage(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x57c]),
            (112, 122, 172)
        );
    }

    #[test]
    fn drop_waits_each_tick_then_breaks_and_preserves_vertical_direction() {
        let (mut data, mut screen) = fixture();
        initialize(&mut data, &mut screen).unwrap();
        set_word(&mut data, 0x40a8, 168);
        data[0x40af] = 1;
        data[0x40b0] = 1;
        data[0x40b8] = 255;
        let step = advance_cage(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.drew && step.falling);
        assert_eq!(word(&data, 0x40a8), 176);
        assert_eq!(
            advance_fall(&mut data, &mut screen, 1).unwrap(),
            FallStep::default()
        );
        for tick in 2..=7 {
            let step = advance_fall(&mut data, &mut screen, tick).unwrap();
            assert!(step.drew && !step.released);
            assert_eq!(data[0x40aa], 134 + (tick as u8 - 1) * 5);
        }
        assert!(advance_fall(&mut data, &mut screen, 8).unwrap().released);
        assert_eq!((word(&data, 0x40b2), data[0x40b4]), (176, 164));
        assert_eq!(data[0x40b8], 255);
        assert_eq!(word(&data, 0x40a6), pixel_offset(176, 164) - 1);
        assert_eq!(data[0x1f80], 3);
        assert_eq!(data[0x553], 0);
    }

    #[test]
    fn success_is_inclusive_and_failure_suppresses_it_without_life_loss() {
        let (mut data, _) = fixture();
        set_word(&mut data, 0x40b2, 100);
        data[0x40b4] = 100;
        for (x, y, hit) in [
            (76, 86, true),
            (108, 105, true),
            (75, 100, false),
            (109, 100, false),
            (100, 85, false),
            (100, 106, false),
        ] {
            set_word(&mut data, 0x579, x);
            data[0x57b] = y;
            data[0x553] = 0;
            assert_eq!(capture(&mut data), hit);
            assert_eq!(data[0x553], u8::from(hit));
        }
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        data[0x552] = 1;
        assert!(capture(&mut data));
        assert_eq!(data[0x553], 0);
        assert_eq!(data[0x1f80], 3);
    }

    #[test]
    fn bird_cadence_permits_one_additional_same_tick_call_every_fourth_accept() {
        let (mut data, mut screen) = fixture();
        set_word(&mut data, 0x40b5, 19);
        data[0x40ff] = 3;
        advance_bird(
            &mut data,
            &mut screen,
            20,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(word(&data, 0x40b5), 19);
        assert_eq!(data[0x40ff], 4);
        advance_bird(
            &mut data,
            &mut screen,
            20,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(word(&data, 0x40b5), 20);
        assert_eq!(data[0x40ff], 5);
        let before = data.clone();
        advance_bird(
            &mut data,
            &mut screen,
            20,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(data, before);
    }

    #[test]
    fn external_waypoint_moves_toward_target_with_two_random_draws() {
        let (mut data, mut screen) = fixture();
        data[0x40aa] = 164;
        data[0x40b9] = 1;
        set_word(&mut data, 0x40b2, 96);
        data[0x40b4] = 98;
        set_word(&mut data, 0x40c8, 0);
        set_word(&mut data, 0x40de, 100);
        data[0x40f4] = 100;
        let step = advance_bird(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.drew && !step.caught);
        assert_eq!((word(&data, 0x40b2), data[0x40b4]), (100, 100));
        assert_eq!(word(&data, 0x2ae5), 0x3fff);
        assert_eq!(word(&data, 0x40c8), 0);
    }

    #[test]
    fn avoidance_uses_not_magnitude_and_sound_gated_shared_random_draw() {
        for sound in [0, 255] {
            let (mut data, mut screen) = fixture();
            data[0] = sound;
            data[0x40aa] = 164;
            data[0x40b9] = 1;
            set_word(&mut data, 0x40b2, 100);
            data[0x40b4] = 100;
            set_word(&mut data, 0x579, 110);
            data[0x57b] = 105;
            set_word(&mut data, 0x40ce, 17);
            set_word(&mut data, 0x2ae5, 2);
            advance_bird(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(word(&data, 0x40cc), 17);
            assert_eq!((word(&data, 0x40b2), data[0x40b4]), (96, 98));
            assert_eq!((data[0x40b7], data[0x40b8]), (255, 255));
            assert_eq!(word(&data, 0x40c8), 255);
            assert_eq!(
                word(&data, 0x2ae5),
                if sound == 0 { 0x8001 } else { 0x4000 }
            );
            assert_eq!(data[0x5920], if sound == 0 { 0 } else { 2 });
        }
    }
}
