// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 2 accepted player movement, animation and surface breathing.
//!
//! Graphics and speed/warning tables come from the external executable. The
//! caller owns acceptance timing, background composition and actor contacts.
//! Air expiry forces sinking and bubbles before the delayed room failure.

use crate::assets::{DATA_LENGTH, set_word, word};
use crate::cga::pixel_offset;
use crate::player::{Controls, Draw, Sprite};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SwimmingStep {
    pub draw: Draw,
    /// Original BIOS background-color request: 0, 1, 5 or 4.
    pub air_warning: u8,
    /// AND-composited before the old player image is restored. Native bubbles
    /// accumulate in the scenery; there is no separate bubble restore.
    pub bubble: Option<(u16, Sprite)>,
    pub failure: bool,
}

fn check(data: &[u8]) -> Result<usize, String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable swimming data segment is truncated".into());
    }
    if word(data, 4) != 2 {
        return Err("Swimming player requires scene two".into());
    }
    let difficulty = usize::from(word(data, 8));
    if difficulty >= 8 {
        return Err("Swimming difficulty exceeds its external tables".into());
    }
    Ok(difficulty)
}

/// CS:07a1 scene-two entry. The caller establishes the initial background.
/// Animation counters, the selected sprite and unrelated speed fields survive
/// entry, as they do in the original initializer.
pub(crate) fn initialize_player(data: &mut [u8], tick: u16) -> Result<(), String> {
    check(data)?;
    let x = word(data, 0x5dd);
    let y = data[0x5eb];
    set_word(data, 0x579, x);
    data[0x57b] = y;
    data[0x57c] = y.wrapping_add(50);
    set_word(data, 0x55f, pixel_offset(x, y));
    set_word(data, 0x569, word(data, 0xfb2));
    let dimensions = word(data, 0xfbe);
    set_word(data, 0x567, dimensions);
    set_word(data, 0x561, dimensions);
    data[0x571] = 1;
    data[0x56e] = 0;
    data[0x578] = 0x40;
    data[0x55b] = 10;
    for offset in [
        0x550, 0x55c, 0x55a, 0x583, 0x698, 0x699, 0x551, 0x584, 0x552, 0x553, 0x127c,
    ] {
        data[offset] = 0;
    }
    for offset in [0x554, 0x57d, 0x684] {
        set_word(data, offset, 0);
    }
    data[0x576] = 16;
    set_word(data, 0x574, 16);
    set_word(data, 0x5f1, tick);
    data[0x5f3] = 0;
    data[0x5f4] = 5;
    data[0x5f5] = 1;
    Ok(())
}

fn direction(value: i8) -> u8 {
    match value {
        -1 => 0xff,
        0 => 0,
        _ => 1,
    }
}

fn advance_x(data: &mut [u8]) {
    set_word(data, 0x5f6, 8);
    set_word(data, 0x5f8, 291);
    let x = word(data, 0x579);
    let step = word(data, 0x572);
    let next = match data[0x56e] {
        0 => return,
        1 => x.wrapping_add(step).min(290),
        _ => match x.checked_sub(step) {
            Some(value) => value.max(8),
            None => 8,
        },
    };
    set_word(data, 0x579, next);
}

fn sprite_index(data: &mut [u8], x_request: u8, y_request: u8) -> usize {
    if data[0x56e] != data[0x56f] || data[0x571] != data[0x570] {
        return 0x18;
    }
    let phase = word(data, 0x587).wrapping_add(1);
    set_word(data, 0x587, phase);
    // The neutral animation slows only BL; the phase word itself still advances.
    let mut index = phase;
    if x_request | y_request == 0 {
        index = (index & 0xff00) | u16::from((index as u8) >> 1);
    }
    let bottom_horizontal = data[0x57b] >= 179 && data[0x571] == 1;
    let surface_vertical = data[0x57b] <= 4 && y_request != 0;
    let horizontal =
        bottom_horizontal || (!surface_vertical && u16::from(data[0x576] >> 1) < word(data, 0x574));
    if horizontal && data[0x56e] != 0 {
        usize::from((index & 6) | if data[0x56e] == 1 { 0 } else { 8 })
    } else {
        usize::from((index & 2) | 0x10) + if data[0x571] == 1 { 4 } else { 0 }
    }
}

/// CS:0926..0ba0, after the caller's BIOS/retrace acceptance gate.
#[cfg(test)]
pub(crate) fn advance(
    data: &mut [u8],
    controls: Controls,
    tick: u16,
) -> Result<SwimmingStep, String> {
    advance_with_damage(data, controls, tick, false)
}

pub(crate) fn advance_with_damage(
    data: &mut [u8],
    controls: Controls,
    tick: u16,
    suppress_air: bool,
) -> Result<SwimmingStep, String> {
    let difficulty = check(data)?;
    if !(-1..=1).contains(&controls.x) || !(-1..=1).contains(&controls.y) {
        return Err("Swimming control directions must be -1, 0, or 1".into());
    }
    set_word(data, 0x57d, tick);
    let elapsed = tick.wrapping_sub(word(data, 0x5f1));
    let sinking = !suppress_air && elapsed >= word(data, 0x589 + difficulty * 2);
    let air_warning = if sinking || elapsed < word(data, 0x5a9 + difficulty * 2) {
        0
    } else if elapsed < word(data, 0x5b9 + difficulty * 2) {
        1
    } else if elapsed < word(data, 0x5c9 + difficulty * 2) {
        5
    } else {
        4
    };
    let x_request = direction(controls.x);
    let y_request = direction(controls.y);
    data[0x698] = x_request;
    data[0x699] = y_request;
    let mut bubble = None;
    if sinking {
        if elapsed >= word(data, 0x599 + difficulty * 2) {
            data[0x552] = 1;
        }
        data[0x5f5] = data[0x5f5].wrapping_sub(1);
        if data[0x5f5] == 0 {
            if data[0] != 0 {
                let tone_phase = word(data, 0x5b08);
                set_word(data, 0x5923, 0x1312u16.wrapping_add(tone_phase));
                set_word(data, 0x5b08, tone_phase.wrapping_add(0x15e));
                data[0x5920] = 2;
                set_word(data, 0x5921, tick);
                data[0x5b07] = 0x18;
            }
            data[0x5f5] = 6;
            let mut rise = data[0x5f4];
            if data[0x57b] >= 179 && rise < 200 {
                rise = rise.wrapping_add(30);
                data[0x5f4] = rise;
            }
            let y = data[0x57b].saturating_sub(rise) & 0xf8;
            bubble = Some((
                pixel_offset(word(data, 0x579), y),
                Sprite {
                    source: 0x64e,
                    dimensions: 0x0503,
                },
            ));
        }
        data[0x56e] = 0;
        data[0x571] = 1;
        data[0x5f3] = 1;
        data[0x576] = 32;
    } else {
        data[0x56f] = data[0x56e];
        data[0x570] = data[0x571];

        let inertia = word(data, 0x574);
        let inertia = if x_request == 0 && inertia >= 16 {
            inertia - 1
        } else if x_request != 0 && x_request == data[0x56e] {
            if inertia < 48 {
                inertia.wrapping_add(3)
            } else {
                inertia
            }
        } else {
            data[0x56e] = x_request;
            32
        };
        set_word(data, 0x574, inertia);
        set_word(
            data,
            0x572,
            (inertia >> 3).min(word(data, 0x66c + difficulty * 2)),
        );
        advance_x(data);

        let inertia = data[0x576];
        data[0x576] = if y_request == 0 && inertia >= 16 {
            inertia - 1
        } else if y_request != 0 && y_request == data[0x571] {
            if inertia < 64 {
                inertia.wrapping_add(4)
            } else {
                inertia
            }
        } else {
            data[0x571] = if y_request == 0 { 0xff } else { y_request };
            32
        };
    }
    let step = (data[0x576] >> 4).min(data[0x67c + difficulty]);
    let y = data[0x57b];
    data[0x57b] = match data[0x571] {
        0 => y,
        1 => y.wrapping_add(step).min(179),
        _ => match y.checked_sub(step) {
            Some(next) if next > 3 => next,
            _ => {
                // Compare the pre-update selected source, before sprite_index.
                if word(data, 0x55d) == word(data, 0x9b8) {
                    set_word(data, 0x5f1, tick);
                }
                2
            }
        },
    };
    let destination = pixel_offset(word(data, 0x579), data[0x57b]);
    set_word(data, 0x563, destination);
    let index = if data[0x5f3] != 0 && !suppress_air {
        0x10
    } else {
        sprite_index(data, x_request, y_request)
    };
    let sprite = Sprite {
        source: word(data, 0x9a6 + index),
        dimensions: word(data, 0x9c0 + index),
    };
    set_word(data, 0x55d, sprite.source);
    set_word(data, 0x565, sprite.dimensions);
    set_word(data, 0x55f, destination);
    set_word(data, 0x561, sprite.dimensions);
    data[0x583] = 0;
    Ok(SwimmingStep {
        draw: Draw::Sprite(sprite),
        air_warning,
        bubble,
        failure: data[0x552] != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Vec<u8> {
        // Authored sentinels and artificial limits; no original assets.
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 4, 2);
        for difficulty in 0..8 {
            set_word(&mut data, 0x66c + difficulty * 2, 4 + difficulty as u16);
            data[0x67c + difficulty] = 3;
            for (table, threshold) in [
                (0x589, 300),
                (0x599, 372),
                (0x5a9, 20),
                (0x5b9, 40),
                (0x5c9, 60),
            ] {
                set_word(&mut data, table + difficulty * 2, threshold);
            }
        }
        for index in (0..=0x18).step_by(2) {
            set_word(&mut data, 0x9a6 + index, 0x1000 + index as u16);
            set_word(&mut data, 0x9c0 + index, 0x0b03);
        }
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        data[0x57c] = 150;
        data[0x56e] = 1;
        data[0x571] = 1;
        set_word(&mut data, 0x574, 32);
        data[0x576] = 32;
        data
    }

    #[test]
    fn entry_uses_external_tables_and_preserves_uninitialized_history() {
        let mut data = tables();
        set_word(&mut data, 0x5dd, 144);
        data[0x5eb] = 6;
        set_word(&mut data, 0xfb2, 0x900);
        set_word(&mut data, 0xfbe, 0x0c03);
        set_word(&mut data, 0x587, 0xabcd);
        set_word(&mut data, 0x55d, 0x1234);
        set_word(&mut data, 0x57f, 0x5678);
        data[0x552] = 1;
        data[0x558] = 7;
        initialize_player(&mut data, 0xfffe).unwrap();
        assert_eq!((word(&data, 0x579), data[0x57b], data[0x57c]), (144, 6, 56));
        assert_eq!((word(&data, 0x574), data[0x576]), (16, 16));
        assert_eq!((word(&data, 0x569), word(&data, 0x561)), (0x900, 0x0c03));
        assert_eq!((word(&data, 0x587), word(&data, 0x55d)), (0xabcd, 0x1234));
        assert_eq!((word(&data, 0x57f), data[0x558]), (0x5678, 7));
        assert_eq!(
            (word(&data, 0x5f1), data[0x5f3], data[0x552]),
            (0xfffe, 0, 0)
        );
        assert_eq!((data[0x5f4], data[0x5f5]), (5, 1));
    }

    #[test]
    fn repeated_input_accelerates_and_uses_external_caps() {
        let mut data = tables();
        set_word(&mut data, 0x574, 47);
        data[0x576] = 63;
        advance(&mut data, Controls { x: 1, y: 1 }, 10).unwrap();
        assert_eq!((word(&data, 0x574), data[0x576]), (50, 67));
        assert_eq!((word(&data, 0x579), data[0x57b]), (104, 103));
        assert_eq!(data[0x57c], 150);
        advance(&mut data, Controls { x: 1, y: 1 }, 11).unwrap();
        assert_eq!((word(&data, 0x574), data[0x576]), (50, 67));
    }

    #[test]
    fn changed_direction_has_transition_sprite_without_advancing_phase() {
        let mut data = tables();
        set_word(&mut data, 0x587, 70);
        let result = advance(&mut data, Controls { x: -1, y: -1 }, 10).unwrap();
        assert_eq!((word(&data, 0x579), data[0x57b]), (96, 98));
        assert_eq!((word(&data, 0x574), data[0x576]), (32, 32));
        assert_eq!(word(&data, 0x587), 70);
        assert_eq!(
            result.draw,
            Draw::Sprite(Sprite {
                source: 0x1018,
                dimensions: 0x0b03
            })
        );
    }

    #[test]
    fn neutral_decay_retains_directions_until_the_next_call_below_sixteen() {
        let mut data = tables();
        set_word(&mut data, 0x574, 16);
        data[0x576] = 16;
        advance(&mut data, Controls::default(), 1).unwrap();
        assert_eq!((data[0x56e], data[0x571]), (1, 1));
        assert_eq!((word(&data, 0x574), data[0x576]), (15, 15));
        assert_eq!((word(&data, 0x579), data[0x57b]), (101, 100));
        advance(&mut data, Controls::default(), 2).unwrap();
        assert_eq!((data[0x56e], data[0x571]), (0, 255));
        assert_eq!((word(&data, 0x574), data[0x576]), (32, 32));
        assert_eq!((word(&data, 0x579), data[0x57b]), (101, 98));
    }

    #[test]
    fn horizontal_and_vertical_bounds_clamp_attempted_motion() {
        for (x, y, controls, expected) in [
            (9, 3, Controls { x: -1, y: -1 }, (8, 2)),
            (289, 178, Controls { x: 1, y: 1 }, (290, 179)),
        ] {
            let mut data = tables();
            set_word(&mut data, 0x579, x);
            data[0x57b] = y;
            advance(&mut data, controls, 10).unwrap();
            assert_eq!((word(&data, 0x579), data[0x57b]), expected);
        }
    }

    #[test]
    fn surface_reset_uses_previous_sprite_and_keeps_this_updates_warning() {
        for (matches, expected_tick) in [(false, 0), (true, 45)] {
            let mut data = tables();
            data[0x57b] = 4;
            let source = if matches { word(&data, 0x9b8) } else { 0x9999 };
            set_word(&mut data, 0x55d, source);
            let result = advance(&mut data, Controls { x: 0, y: -1 }, 45).unwrap();
            assert_eq!(word(&data, 0x5f1), expected_tick);
            assert_eq!(data[0x57b], 2);
            assert_eq!(result.air_warning, 5);
        }
    }

    #[test]
    fn warning_thresholds_wrap_and_do_not_disable_movement_or_completion() {
        for (elapsed, warning) in [
            (19, 0),
            (20, 1),
            (39, 1),
            (40, 5),
            (59, 5),
            (60, 4),
            (600, 4),
        ] {
            let mut data = tables();
            set_word(&mut data, 0x5f1, 0xfff0);
            data[0x553] = 1;
            let result = advance_with_damage(
                &mut data,
                Controls { x: -1, y: -1 },
                0xfff0u16.wrapping_add(elapsed),
                true,
            )
            .unwrap();
            assert_eq!(result.air_warning, warning);
            assert_eq!((data[0x552], data[0x553], data[0x5f3]), (0, 1, 0));
            assert_eq!((word(&data, 0x579), data[0x57b]), (96, 98));
            assert_eq!((data[0x5f4], data[0x5f5]), (0, 0));
        }
    }

    #[test]
    fn sprite_axis_and_direction_follow_inertia_bottom_and_surface_rules() {
        for (y, x_direction, y_direction, x_inertia, y_inertia, controls, source) in [
            (100, 1, 1, 48, 32, Controls { x: 1, y: 1 }, 0x1000),
            (100, 255, 1, 48, 32, Controls { x: -1, y: 1 }, 0x1008),
            (100, 1, 255, 16, 64, Controls { x: 1, y: -1 }, 0x1010),
            (100, 0, 1, 32, 32, Controls { x: 0, y: 1 }, 0x1014),
            (179, 1, 1, 16, 64, Controls { x: 1, y: 1 }, 0x1000),
            (4, 1, 255, 48, 32, Controls { x: 1, y: -1 }, 0x1010),
        ] {
            let mut data = tables();
            data[0x57b] = y;
            data[0x56e] = x_direction;
            data[0x571] = y_direction;
            set_word(&mut data, 0x574, x_inertia);
            data[0x576] = y_inertia;
            let result = advance(&mut data, controls, 10).unwrap();
            assert_eq!(
                result.draw,
                Draw::Sprite(Sprite {
                    source,
                    dimensions: 0x0b03
                })
            );
            assert_eq!(word(&data, 0x587), 1);
            assert_eq!(word(&data, 0x55f), word(&data, 0x563));
        }
    }

    #[test]
    fn neutral_animation_halves_low_phase_without_halving_the_counter() {
        let mut data = tables();
        set_word(&mut data, 0x587, 3);
        let result = advance(&mut data, Controls::default(), 10).unwrap();
        assert_eq!(word(&data, 0x587), 4);
        assert_eq!(
            result.draw,
            Draw::Sprite(Sprite {
                source: 0x1002,
                dimensions: 0x0b03
            })
        );
    }

    #[test]
    fn rejected_inputs_leave_data_unchanged() {
        let mut data = tables();
        let initial = data.clone();
        assert!(advance(&mut data, Controls { x: 2, y: 0 }, 10).is_err());
        assert_eq!(data, initial);
        set_word(&mut data, 8, 8);
        let initial = data.clone();
        assert!(advance(&mut data, Controls::default(), 10).is_err());
        assert_eq!(data, initial);
        assert!(initialize_player(&mut data[..0x1000], 10).is_err());
    }

    #[test]
    fn air_expiry_forces_sinking_then_failure_at_external_thresholds() {
        for elapsed in [299, 300, 371, 372] {
            let mut data = tables();
            set_word(&mut data, 0x5f1, 0xfff0);
            data[0x5f4] = 5;
            data[0x5f5] = 1;
            let tick = 0xfff0u16.wrapping_add(elapsed);
            let step = advance(&mut data, Controls { x: -1, y: -1 }, tick).unwrap();
            let sinking = elapsed >= 300;
            assert_eq!((data[0x5f3] != 0, step.failure), (sinking, elapsed >= 372));
            if sinking {
                assert_eq!(
                    (
                        word(&data, 0x579),
                        data[0x57b],
                        data[0x56e],
                        data[0x571],
                        data[0x576],
                        step.air_warning
                    ),
                    (100, 102, 0, 1, 32, 0)
                );
                assert_eq!(
                    step.draw,
                    Draw::Sprite(Sprite {
                        source: 0x1010,
                        dimensions: 0x0b03
                    })
                );
                assert_eq!(
                    step.bubble,
                    Some((
                        pixel_offset(100, 88),
                        Sprite {
                            source: 0x64e,
                            dimensions: 0x0503
                        }
                    ))
                );
                assert_eq!((data[0x5f4], data[0x5f5], word(&data, 0x587)), (5, 6, 0));
            } else {
                assert!(step.bubble.is_none());
                assert_eq!((word(&data, 0x579), data[0x57b]), (96, 98));
            }
        }
    }

    #[test]
    fn air_bubbles_repeat_every_six_updates_and_rise_farther_from_bottom() {
        let mut data = tables();
        data[0x57b] = 179;
        data[0x5f4] = 5;
        data[0x5f5] = 1;
        let first = advance(&mut data, Controls::default(), 300).unwrap();
        assert_eq!(first.bubble.unwrap().0, pixel_offset(100, 144));
        assert_eq!(data[0x5f4], 35);
        for tick in 301..306 {
            assert!(
                advance(&mut data, Controls::default(), tick)
                    .unwrap()
                    .bubble
                    .is_none()
            );
        }
        let next = advance(&mut data, Controls::default(), 306).unwrap();
        assert_eq!(next.bubble.unwrap().0, pixel_offset(100, 112));
        assert_eq!((data[0x5f4], data[0x5f5]), (65, 6));
    }
}
