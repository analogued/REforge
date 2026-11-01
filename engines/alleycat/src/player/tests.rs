// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn clothes_edges_release_support_without_changing_animation_phase_or_direction() {
    for (x, left) in [(8, true), (290, false)] {
        let mut player = Player::at_alley_floor(x);
        player.support_state = 2;
        player.y = 40;
        player.vertical_coordinate = 90;
        player.horizontal_direction = 0xff;
        player.horizontal_step = 0x1206;
        player.phase = 77;
        player.last_support = Support::UpperBitfield;
        player.carry_with_clothes(left);
        assert_eq!(
            (player.x, player.y, player.vertical_coordinate),
            (x, 40, 90)
        );
        assert_eq!((player.phase, player.horizontal_direction), (77, 0xff));
        assert_eq!(player.horizontal_step, 0x1201);
        assert_eq!(player.support_state, 0);
        assert_eq!(player.support_delay, 17);
        assert_eq!((player.vertical_direction, player.vertical_step), (1, 1));
        assert_eq!(player.phase_decrement, 24);
        assert_eq!(player.last_support, Support::None);
    }
}

fn empty_tables() -> Vec<u8> {
    // Authored empty tables, not a captured original data segment.
    vec![0; 0x1200]
}

fn room_tables() -> Vec<u8> {
    let mut data = vec![0; crate::assets::DATA_LENGTH];
    crate::assets::set_word(&mut data, 4, 5);
    crate::assets::set_word(&mut data, 0x2650, 248);
    data[0x2652] = 96;
    data
}

#[test]
fn horizontal_limits_keep_hearts_inside_cupids_and_preserve_other_scenes() {
    for (scene, left, right) in [
        (0, 8, 290),
        (3, 8, 290),
        (4, 8, 290),
        (5, 8, 290),
        (6, 8, 290),
        (7, 36, 270),
    ] {
        let mut data = room_tables();
        crate::assets::set_word(&mut data, 4, scene);
        for step in [1, 8] {
            for (x, direction, expected_x, crossed) in [
                (left + step, 255, left, false),
                (left + step - 1, 255, left, true),
                (left, 255, left, true),
                (right - step, 1, right, false),
                (right - step + 1, 1, right, true),
                (right, 1, right, true),
                (0, 255, left, true),
            ] {
                let mut player = Player::at_alley_floor(80);
                player.x = x;
                player.horizontal_direction = direction;
                player.horizontal_step = step;
                assert_eq!(player.advance_x(&data), crossed);
                assert_eq!(player.x, expected_x);
            }
        }
        // The native helper clamps attempted movement, not stationary state.
        let mut player = Player::at_alley_floor(80);
        player.x = 0;
        player.horizontal_direction = 0;
        assert!(!player.advance_x(&data));
        assert_eq!(player.x, 0);
    }
}

#[test]
fn hearts_edge_hit_stops_horizontal_motion_and_turns_a_jump_into_a_fall() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 7);
    data[0x2be2..0x2c60].fill(2);
    for (x, direction, limit) in [(37, 255, 36), (269, 1, 270)] {
        let mut player = Player::at_alley_floor(x);
        player.y = 100;
        player.vertical_coordinate = 150;
        player.horizontal_direction = direction;
        player.horizontal_step = 8;
        player.vertical_direction = 255;
        player.vertical_step = 8;
        player.support_delay = 8;
        player.phase = 0xa5;
        player.phase_decrement = 0x10;
        player.airborne_sprite = Sprite {
            source: 0x900,
            dimensions: 0x0d03,
        };
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!((player.x, player.y), (limit, 102));
        assert_eq!(
            (player.horizontal_direction, player.vertical_direction),
            (0, 1)
        );
        assert_eq!(
            (player.vertical_step, player.support_delay, player.phase),
            (2, 0, 0xa5)
        );
        assert!(!outcome.landed && !outcome.edge_reset);
        assert!(!player.take_room_exit());
    }
}

#[test]
fn hearts_fall_has_no_alley_floor_and_exits_below_the_bottom_row() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 7);
    data[0x2be2..0x2c60].fill(2);
    data[0x2bd4..0x2bdb].copy_from_slice(&[184, 160, 136, 112, 88, 64, 40]);
    let mut player = Player::at_alley_floor(80);
    player.vertical_direction = 1;
    player.vertical_step = 4;
    player.phase_decrement = 0;
    player.airborne_sprite = Sprite {
        source: 0x900,
        dimensions: 0x0d03,
    };
    let first = player.update(Controls::default(), &data).unwrap();
    assert!(!first.landed);
    assert_eq!(
        (
            player.y,
            player.vertical_coordinate,
            player.vertical_direction
        ),
        (184, 234, 1)
    );
    for _ in 0..10 {
        player.update(Controls::default(), &data).unwrap();
        if player.room_exit_requested {
            break;
        }
    }
    assert!(player.take_room_exit());
    assert_eq!(player.vertical_direction, 1);
    assert!(player.y > 187);
    assert!(player.vertical_coordinate <= 248);
    assert_eq!(player.entry_state, 0);
}

#[test]
fn stationary_hearts_player_below_180_still_checks_platform_support() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 7);
    data[0x2bd4..0x2bdb].copy_from_slice(&[184, 160, 136, 112, 88, 64, 40]);
    data[0x2be2..0x2c60].fill(2);
    let mut player = Player::at_alley_floor(80);
    player.y = 189;
    player.vertical_coordinate = 239;
    player.update(Controls::default(), &data).unwrap();
    assert_eq!(player.vertical_direction, 1);
    assert_eq!(player.last_support, Support::None);
}

#[test]
fn airborne_support_work_follows_delay_direction_and_disable_gates() {
    let data = room_tables();
    for (direction, delay, disabled, phase, decrement, step, searched, landed) in [
        (255, 2, 0, 128, 0, 3, false, false),
        (1, 2, 0, 128, 0, 3, false, false),
        (1, 1, 0, 128, 0, 3, true, false),
        (1, 0, 0, 128, 0, 3, true, false),
        (1, 0, 1, 128, 0, 3, false, false),
        (255, 1, 0, 0, 1, 1, true, false),
        (1, 1, 0, 128, 0, 7, true, true),
    ] {
        let mut player = Player::at_alley_floor(125);
        player.y = 176;
        player.vertical_coordinate = 226;
        player.vertical_direction = direction;
        player.support_delay = delay;
        player.support_disabled = disabled;
        player.phase = phase;
        player.phase_decrement = decrement;
        player.vertical_step = step;
        player.airborne_sprite = Sprite {
            source: 0x900,
            dimensions: 0x0c03,
        };
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(outcome.airborne_support_checked, searched);
        assert_eq!(outcome.landed, landed);
        let Draw::Sprite(sprite) = outcome.draw else {
            panic!("Expected complete drawing");
        };
        assert!(player.is_unclipped_airborne_sprite(sprite));
        player.alternate_animation = 1;
        assert!(!player.is_unclipped_airborne_sprite(sprite));
        player.alternate_animation = 0;
        assert!(!player.is_unclipped_airborne_sprite(Sprite {
            dimensions: 0x0b03,
            ..sprite
        }));
    }
}

#[test]
fn state_zero_surface_hit_survives_a_jump_but_excludes_cling_and_misses() {
    for state in [0, 1] {
        let mut data = room_tables();
        data[0x1029] = 136;
        data[0x1089] = state;
        crate::assets::set_word(&mut data, 0x10e9, 120);
        crate::assets::set_word(&mut data, 0x11a9, 40);
        for y in [-1, 0, 1] {
            let mut player = Player::at_alley_floor(125);
            player.y = 136;
            player.vertical_coordinate = 186;
            let outcome = player.update(Controls { x: 1, y }, &data).unwrap();
            assert_eq!(outcome.room_surface_hit, state == 0);
            if y == 0 {
                assert_eq!(outcome.support, Support::RoomTable);
            } else {
                assert_eq!(outcome.support, Support::None);
                assert_eq!(player.support_state, 0);
                assert_eq!(player.vertical_direction, y as u8);
            }
        }
    }
    let data = room_tables();
    for y in [136, 180] {
        let mut player = Player::at_alley_floor(125);
        player.y = y;
        player.vertical_coordinate = y + 50;
        let outcome = player.update(Controls { x: 1, y: -1 }, &data).unwrap();
        assert!(!outcome.room_surface_hit);
        assert_eq!(player.vertical_direction, if y == 136 { 1 } else { 255 });
    }
}

#[test]
fn room_surface_work_counts_the_first_hit_from_the_room_table_start() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 0x1269 + 5 * 2, 7);
    // Earlier entries mismatch height; the final two overlap, and the
    // resolver must stop at the first. Entries before the room are ignored.
    data[0x1029 + 7..0x1029 + 10].copy_from_slice(&[88, 136, 136]);
    for index in 7..10 {
        crate::assets::set_word(&mut data, 0x10e9 + index * 2, 120);
        crate::assets::set_word(&mut data, 0x11a9 + index * 2, 40);
    }
    for (state, y, entries) in [(0, 136, 2), (1, 136, 0), (0, 112, 0), (0, 180, 0)] {
        data[0x1089 + 8] = state;
        let mut player = Player::at_alley_floor(125);
        player.y = y;
        player.vertical_coordinate = y + 50;
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(outcome.room_surface_entries, entries);
        assert_eq!(outcome.room_surface_hit, entries != 0);
    }
    data[0x1089 + 8] = 0;
    let mut player = Player::at_alley_floor(125);
    player.y = 136;
    player.vertical_coordinate = 186;
    player.support_state = 2;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert_eq!(outcome.room_surface_entries, 0);
    assert!(!outcome.room_surface_hit);
}

#[test]
fn airborne_cling_search_counts_entries_only_when_the_search_runs() {
    for scene in [1, 4, 5, 6] {
        let mut data = room_tables();
        crate::assets::set_word(&mut data, 4, scene);
        crate::assets::set_word(&mut data, 0x1269 + usize::from(scene) * 2, 7);
        data[0x1029 + 7..0x1029 + 10].copy_from_slice(&[88, 136, 136]);
        for index in 7..10 {
            crate::assets::set_word(&mut data, 0x10e9 + index * 2, 120);
            crate::assets::set_word(&mut data, 0x11a9 + index * 2, 40);
            data[0x1089 + index] = 1;
        }
        for (direction, delay, disabled, expected) in
            [(1, 0, 0, 2), (1, 2, 0, 0), (1, 0, 1, 0), (255, 0, 0, 0)]
        {
            let mut player = Player::at_alley_floor(125);
            player.y = 139;
            player.vertical_coordinate = 189;
            player.vertical_direction = direction;
            player.support_delay = delay;
            player.support_disabled = disabled;
            player.phase = 128;
            player.phase_decrement = 0;
            player.airborne_sprite = Sprite {
                source: 0x900,
                dimensions: 0x0c03,
            };
            let outcome = player.update(Controls::default(), &data).unwrap();
            assert_eq!(outcome.room_surface_entries, expected);
            assert_eq!(outcome.landed, expected != 0);
            assert!(!outcome.room_surface_hit);
            if expected != 0 {
                assert_eq!(player.support_state, 1);
            }
        }
    }
}

#[test]
fn retained_idle_shape_excludes_hidden_clipped_and_alternate_images() {
    let mut player = Player::at_alley_floor(125);
    for (dimensions, hidden, alternate, retained) in [
        (0x0b03, false, 0, true),
        (0x0c02, false, 0, true),
        (0x0b02, false, 0, false),
        (0x0c03, false, 0, false),
        (0x0c02, true, 0, false),
        (0x0b03, false, 1, false),
    ] {
        player.previous_dimensions = dimensions;
        player.image_hidden = hidden;
        player.alternate_animation = alternate;
        assert_eq!(player.has_retained_idle_image(), retained);
        assert_eq!(
            player.has_retained_walking_image(),
            retained && dimensions == 0x0b03
        );
    }
}

#[test]
fn fence_fall_fit_requires_a_complete_retained_twelve_row_image() {
    let mut player = Player::at_alley_floor(64);
    player.band_state = 2;
    player.airborne_sprite = Sprite {
        source: 0x900,
        dimensions: 0x0c03,
    };
    for (dimensions, hidden, alternate, band, expected) in [
        (0x0c03, false, 0, 2, true),
        (0x0b03, false, 0, 2, false),
        (0x0c03, true, 0, 2, false),
        (0x0c03, false, 1, 2, false),
        (0x0c03, false, 0, 0, false),
    ] {
        player.previous_dimensions = dimensions;
        player.image_hidden = hidden;
        player.alternate_animation = alternate;
        player.band_state = band;
        assert_eq!(player.has_retained_fence_fall_image(), expected);
    }
    player.band_state = 2;
    player.previous_dimensions = 0x0d03;
    player.airborne_sprite.dimensions = 0x0d03;
    assert!(player.has_retained_airborne_image());
    assert!(!player.has_retained_fence_fall_image());
}

#[test]
fn hearts_bottom_clip_keeps_source_and_changes_saved_image_shape() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 7);
    data[0x2be2..0x2c60].fill(2);
    let mut player = Player::at_alley_floor(125);
    player.y = 186;
    player.vertical_coordinate = 236;
    player.vertical_direction = 1;
    player.vertical_step = 3;
    player.phase_decrement = 0;
    player.support_delay = 8;
    player.airborne_sprite = Sprite {
        source: 0x900,
        dimensions: 0x0c03,
    };
    player.previous_dimensions = 0x0c03;
    assert!(player.has_retained_airborne_image());
    let outcome = player.update(Controls::default(), &data).unwrap();
    let Draw::Sprite(sprite) = outcome.draw else {
        panic!("Expected bottom-clipped drawing");
    };
    assert_eq!(sprite.source, 0x900);
    assert_eq!(sprite.dimensions, 0x0a03);
    assert!(player.is_hearts_ten_row_clip(sprite));
    assert!(!player.has_retained_airborne_image());
    assert!(!player.is_hearts_ten_row_clip(Sprite {
        source: 0x901,
        ..sprite
    }));
    assert!(!player.is_hearts_ten_row_clip(Sprite {
        dimensions: 0x0903,
        ..sprite
    }));
    player.alternate_animation = 1;
    assert!(!player.is_hearts_ten_row_clip(sprite));
}

#[test]
fn hearts_clipping_reports_restored_and_drawn_rows_on_actual_paths() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 7);
    for (rows, coordinate, step, kind, before, after, exited) in [
        (12, 236, 3, HeartsClipKind::Bottom, 12, 10, false),
        (12, 239, 2, HeartsClipKind::Bottom, 10, 8, false),
        (12, 247, 2, HeartsClipKind::Bottom, 2, 1, true),
        (13, 246, 2, HeartsClipKind::Bottom, 4, 2, true),
        (12, 47, 3, HeartsClipKind::Top, 9, 6, false),
        (12, 41, 4, HeartsClipKind::Hidden, 3, 0, false),
    ] {
        let mut player = Player::at_alley_floor(125);
        player.vertical_coordinate = coordinate;
        player.y = coordinate.saturating_sub(50);
        player.vertical_direction = if coordinate < 50 { 255 } else { 1 };
        player.vertical_step = step;
        player.phase_decrement = 0;
        player.support_delay = 8;
        player.airborne_sprite = Sprite {
            source: 0x900,
            dimensions: rows << 8 | 3,
        };
        player.previous_dimensions = u16::from(before) << 8 | 3;
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(
            outcome.hearts_clipping,
            Some(HeartsClippingWork {
                kind,
                restored_rows: before,
                drawn_rows: after,
            })
        );
        assert_eq!(player.take_room_exit(), exited);
        assert_eq!(outcome.draw == Draw::Erase, kind == HeartsClipKind::Hidden);
    }
}

#[test]
fn hearts_clipping_excludes_invalid_hidden_alternate_and_other_scene_history() {
    for (scene, previous, hidden, alternate) in [
        (7, 0x0c03, false, 0), // Too tall for Y197: invalid old exit fixture.
        (7, 0x0203, true, 0),
        (7, 0x0203, false, 1),
        (3, 0x0203, false, 0),
    ] {
        let mut data = room_tables();
        crate::assets::set_word(&mut data, 4, scene);
        let mut player = Player::at_alley_floor(125);
        player.y = 197;
        player.vertical_coordinate = 247;
        player.vertical_direction = 1;
        player.vertical_step = 2;
        player.phase_decrement = 0;
        player.support_delay = 8;
        player.airborne_sprite = Sprite {
            source: 0x900,
            dimensions: 0x0c03,
        };
        player.previous_dimensions = previous;
        player.image_hidden = hidden;
        player.alternate_animation = alternate;
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(outcome.hearts_clipping, None);
    }
}

#[test]
fn retained_airborne_shape_requires_a_complete_visible_draw() {
    let data = room_tables();
    let mut player = Player::at_alley_floor(125);
    player.y = 176;
    player.vertical_coordinate = 226;
    player.vertical_direction = 255;
    player.vertical_step = 3;
    player.airborne_sprite = Sprite {
        source: 0x900,
        dimensions: 0x0c03,
    };
    assert!(!player.has_retained_airborne_image());
    let first = player.update(Controls::default(), &data).unwrap();
    assert_eq!(first.draw, Draw::Sprite(player.airborne_sprite));
    assert!(player.has_retained_airborne_image());
    player.y = 0;
    player.vertical_coordinate = 45;
    let clipped = player.update(Controls::default(), &data).unwrap();
    let Draw::Sprite(sprite) = clipped.draw else {
        panic!("Expected cropped image");
    };
    assert_eq!(sprite.dimensions, 0x0403);
    assert!(!player.has_retained_airborne_image());
    player.vertical_coordinate = 35;
    let hidden = player.update(Controls::default(), &data).unwrap();
    assert_eq!(hidden.draw, Draw::Erase);
    assert!(!player.has_retained_airborne_image());
}

#[test]
fn room_landing_uses_table_geometry_without_alley_band_clipping() {
    let mut data = room_tables();
    data[0x1029] = 136;
    crate::assets::set_word(&mut data, 0x10e9, 128);
    crate::assets::set_word(&mut data, 0x11a9, 40);
    let mut player = Player::at_alley_floor(163);
    player.y = 139;
    player.vertical_coordinate = 189;
    player.vertical_direction = 1;
    player.vertical_step = 2;
    player.band_state = 2;
    player.alternate_animation = 1;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert!(outcome.landed && !outcome.edge_reset);
    assert_eq!(outcome.support, Support::RoomTable);
    assert_eq!(
        (player.x, player.y, player.vertical_coordinate),
        (163, 136, 186)
    );
    assert_eq!(
        (player.vertical_direction, player.alternate_animation),
        (0, 0)
    );
}

#[test]
fn cheese_hole_marker_follows_original_support_indices_and_transition_clear() {
    for (index, marker) in [(38, 0), (39, 1), (54, 16), (55, 0)] {
        let mut data = room_tables();
        crate::assets::set_word(&mut data, 4, 4);
        crate::assets::set_word(&mut data, 0x1271, index);
        data[0x1029 + usize::from(index)] = 88;
        crate::assets::set_word(&mut data, 0x10e9 + usize::from(index) * 2, 120);
        crate::assets::set_word(&mut data, 0x11a9 + usize::from(index) * 2, 24);
        let mut player = Player::at_alley_floor(139);
        player.y = 91;
        player.vertical_coordinate = 141;
        player.vertical_direction = 1;
        player.band_state = 2;
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert!(outcome.landed && !outcome.edge_reset);
        assert_eq!((player.x, player.y, player.hole_marker), (139, 88, marker));
        player.write_data(&mut data).unwrap();
        assert_eq!(data[0x39e0], marker);
        player.update(Controls { x: 0, y: -1 }, &data).unwrap();
        player.write_data(&mut data).unwrap();
        assert_eq!(data[0x39e0], 0);
    }
}

#[test]
fn cheese_transfer_gates_movement_and_preserves_marker_until_support_checks() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 4);
    data[0x39e1] = 6;
    data[0x39e0] = 9;
    let mut player = Player::at_alley_floor(120);
    player.y = 88;
    player.vertical_coordinate = 138;
    player.vertical_direction = 1;
    player.hole_marker = 9;
    let before = player.clone();
    let outcome = player.update(Controls { x: 1, y: -1 }, &data).unwrap();
    assert_eq!(outcome.draw, Draw::Keep);
    assert_eq!(
        (player.x, player.y, player.phase, player.vertical_direction),
        (before.x, before.y, before.phase, before.vertical_direction)
    );
    player.write_data(&mut data).unwrap();
    assert_eq!(data[0x39e0], 9);
    data[0x39e1] = 0;
    player.check_support(&data).unwrap();
    player.write_data(&mut data).unwrap();
    assert_eq!(data[0x39e0], 0);
}

#[test]
fn drinking_gates_room_six_movement_until_released() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 4, 6);
    data[0x44bd] = 1;
    let mut player = Player::at_alley_floor(144);
    let before = player.clone();
    let outcome = player.update(Controls { x: 1, y: -1 }, &data).unwrap();
    assert_eq!(outcome.draw, Draw::Keep);
    assert_eq!(
        (player.x, player.y, player.phase, player.vertical_direction),
        (before.x, before.y, before.phase, before.vertical_direction)
    );
    data[0x44bd] = 0;
    let outcome = player.update(Controls { x: 1, y: -1 }, &data).unwrap();
    assert_eq!(outcome.support, Support::None);
    assert_eq!(player.vertical_direction, 255);
}

#[test]
fn room_exit_is_a_request_without_a_landing_or_fence_reset() {
    let data = room_tables();
    let mut player = Player::at_alley_floor(248);
    player.y = 96;
    player.vertical_coordinate = 146;
    player.vertical_direction = 1;
    player.vertical_step = 2;
    player.band_state = 2;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert!(!outcome.landed && !outcome.edge_reset);
    assert!(player.take_room_exit());
    assert!(!player.take_room_exit());
    assert_eq!((player.y, player.vertical_direction), (98, 1));
}

#[test]
fn broom_animation_keeps_physics_and_exports_its_separate_phase() {
    let mut data = room_tables();
    crate::assets::set_word(&mut data, 0xfc4, 0x6100);
    crate::assets::set_word(&mut data, 0xfd4, 0x0e03);
    let mut player = Player::at_alley_floor(80);
    player.y = 120;
    player.vertical_coordinate = 170;
    player.vertical_direction = 1;
    player.vertical_step = 8;
    player.phase_decrement = 1;
    player.alternate_animation = 1;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert_eq!((player.y, player.vertical_step), (128, 8));
    assert_eq!(
        outcome.draw,
        Draw::Sprite(Sprite {
            source: 0x6100,
            dimensions: 0x0e03
        })
    );
    player.write_data(&mut data).unwrap();
    assert_eq!(word(&data, 0x585), 2);
    assert_eq!(data[0x584], 1);
}

#[test]
fn lower_support_transition_keeps_the_direction_before_control_latching() {
    let mut data = empty_tables();
    data[0xff0] = 20;
    let mut player = Player::at_alley_floor(80);
    player.y = 142;
    player.vertical_coordinate = 192;
    player.horizontal_direction = 1;
    player.update(Controls { x: -1, y: 1 }, &data).unwrap();
    assert_eq!(player.previous_direction, 1);
    assert_eq!(player.horizontal_direction, 0xff);
    assert_eq!(player.vertical_direction, 1);
}

#[test]
fn walking_moves_before_accelerating_and_wraps_the_shared_phase() {
    let data = empty_tables();
    let mut player = Player::at_alley_floor(8);
    let expected = [10, 12, 14, 16, 19, 22, 25, 28, 32, 36, 40, 44, 49, 54];
    for x in expected {
        player.update(Controls { x: 1, y: 0 }, &data).unwrap();
        assert_eq!(player.x, x);
    }
    assert_eq!(player.horizontal_step, 5);
    assert_eq!(player.phase, 250);
    for _ in 0..40 {
        player.update(Controls { x: 1, y: 0 }, &data).unwrap();
    }
    assert_eq!(player.horizontal_step, 8);
    assert_eq!(player.x, 290);
    player.update(Controls { x: -1, y: 0 }, &data).unwrap();
    assert_eq!(player.x, 282, "turn uses the previous speed for this move");
    assert_eq!(player.horizontal_step, 2);
}

#[test]
fn standing_jump_uses_byte_borrow_and_returns_to_the_floor() {
    let data = empty_tables();
    let mut player = Player::at_alley_floor(54);
    let outcome = player.update(Controls { x: 0, y: -1 }, &data).unwrap();
    assert_eq!(outcome.draw, Draw::Keep);
    assert_eq!(
        (player.y, player.phase, player.phase_decrement),
        (180, 1, 208)
    );
    let expected_y = [
        173, 167, 162, 158, 155, 152, 150, 149, 150, 152, 154, 157, 161, 165, 169, 173, 177, 180,
    ];
    for (index, y) in expected_y.into_iter().enumerate() {
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(player.y, y);
        assert_eq!(outcome.landed, index == 17);
    }
    assert_eq!(player.vertical_direction, 0);
    assert_eq!(player.vertical_step, 4);
    assert_eq!(player.phase, 97);
}

#[test]
fn support_is_checked_after_horizontal_motion_and_before_vertical_motion() {
    let mut data = empty_tables();
    // A synthetic list entry describes X=104 at the lower band.
    data[0xff0] = 26;
    let mut player = Player::at_alley_floor(102);
    player.y = 149;
    player.vertical_coordinate = 199;
    player.horizontal_direction = 1;
    player.horizontal_step = 3;
    player.vertical_direction = 1;
    player.vertical_step = 3;
    player.phase = 1;
    player.phase_decrement = 160;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert_eq!(
        (player.x, player.y, player.vertical_coordinate),
        (105, 142, 192)
    );
    assert_eq!(
        player.vertical_step, 4,
        "phase update precedes support testing"
    );
    assert_eq!(outcome.support, Support::LowerList);
    assert!(outcome.landed);
}

#[test]
fn released_and_opposite_controls_preserve_airborne_direction() {
    let data = empty_tables();
    let mut player = Player::at_alley_floor(8);
    for _ in 0..14 {
        player.update(Controls { x: 1, y: 0 }, &data).unwrap();
    }
    player.update(Controls { x: 1, y: -1 }, &data).unwrap();
    assert_eq!(
        (player.x, player.horizontal_step, player.phase_decrement),
        (54, 3, 160)
    );
    player.update(Controls::default(), &data).unwrap();
    assert_eq!(
        (player.x, player.y, player.horizontal_direction),
        (57, 173, 1)
    );
    player.update(Controls { x: -1, y: 1 }, &data).unwrap();
    assert_eq!(
        (player.x, player.y, player.horizontal_direction),
        (60, 167, 1)
    );
}

#[test]
fn lower_list_subtraction_wraps_instead_of_saturating() {
    let mut data = empty_tables();
    data[0xff0] = 1; // Synthetic X=4 at band 144.
    let mut player = Player::at_alley_floor(8);
    player.y = 142;
    player.vertical_coordinate = 192;
    assert_eq!(player.check_support(&data).unwrap().0, Support::None);
    player.x = 16;
    assert_eq!(player.check_support(&data).unwrap().0, Support::LowerList);
}

#[test]
fn lower_support_indexes_all_eight_difficulties_and_keeps_landing_bounds() {
    let mut data = empty_tables();
    // Independent synthetic lists distinguish all eight difficulty entries.
    // Alternate tall/short bins so both landing bands are exercised.
    for difficulty in 0..8 {
        data[0x1006 + difficulty] = (difficulty * 2) as u8;
        data[0xff0 + difficulty * 2] =
            (8 + difficulty as u8 * 4) | if difficulty % 2 == 0 { 0x80 } else { 0 };
    }
    for difficulty in 0..8 {
        crate::assets::set_word(&mut data, 8, difficulty as u16);
        let target_x = 32 + difficulty as u16 * 16;
        let target_y = if difficulty % 2 == 0 { 134 } else { 142 };
        for (x, y, expected) in [
            (target_x - 1, target_y, Support::None),
            (target_x, target_y, Support::LowerList),
            (target_x + 22, target_y, Support::LowerList),
            (target_x + 23, target_y, Support::None),
            (target_x, target_y - 1, Support::None),
            (target_x, target_y + 7, Support::LowerList),
            (target_x, target_y + 8, Support::None),
        ] {
            let mut player = Player::at_alley_floor(x);
            player.y = y;
            player.vertical_coordinate = y + 50;
            assert_eq!(
                player.check_support(&data).unwrap().0,
                expected,
                "difficulty={difficulty}, x={x}, y={y}"
            );
            if expected == Support::LowerList {
                assert_eq!(
                    (player.y, player.vertical_coordinate),
                    (target_y, target_y + 50)
                );
            }
        }
    }
}

#[test]
fn upper_support_aligns_x_and_holds_until_a_vertical_request() {
    let mut data = empty_tables();
    data[0x1025] = 0;
    data[0x52f] = 2; // Moving-row adjustment does not apply to row zero.
    data[0x1016] = 0x04; // Synthetic occupied cell 5, covering X=40..47.
    let mut player = Player::at_alley_floor(35);
    player.y = 8;
    player.vertical_coordinate = 58;
    player.vertical_direction = 1;
    player.vertical_step = 1;
    let outcome = player.update(Controls::default(), &data).unwrap();
    assert_eq!(outcome.support, Support::UpperBitfield);
    assert_eq!((player.x, player.y, player.support_state), (32, 8, 1));
    let outcome = player.update(Controls { x: 1, y: 0 }, &data).unwrap();
    assert_eq!(
        (player.x, player.support_state, player.horizontal_step),
        (32, 2, 6)
    );
    assert_eq!(
        outcome.draw,
        Draw::Sprite(Sprite {
            source: 0x9da,
            dimensions: 0x0e03
        })
    );
    player.update(Controls { x: 1, y: -1 }, &data).unwrap();
    assert_eq!(
        (player.x, player.support_state, player.horizontal_step),
        (32, 0, 4)
    );
}

#[test]
fn exact_floor_height_does_not_land_until_the_next_descent_update() {
    let data = empty_tables();
    let mut player = Player::at_alley_floor(54);
    player.vertical_coordinate = 226;
    player.vertical_direction = 1;
    player.vertical_step = 4;
    let first = player.update(Controls::default(), &data).unwrap();
    assert!(!first.landed);
    assert_eq!(player.vertical_direction, 1);
    let second = player.update(Controls::default(), &data).unwrap();
    assert!(second.landed);
    assert_eq!((player.y, player.vertical_direction), (180, 0));
}

#[test]
fn idle_gate_resets_speed_and_phase_even_without_redrawing() {
    let data = empty_tables();
    let mut player = Player::at_alley_floor(8);
    assert_eq!(
        player.update(Controls::default(), &data).unwrap().draw,
        Draw::Idle
    );
    for _ in 0..7 {
        player.horizontal_step = 5;
        player.phase = 0;
        assert_eq!(
            player.update(Controls::default(), &data).unwrap().draw,
            Draw::Keep
        );
        assert_eq!((player.horizontal_step, player.phase), (2, 8));
    }
    assert_eq!(
        player.update(Controls::default(), &data).unwrap().draw,
        Draw::Idle
    );
}

fn walking_tables() -> Vec<u8> {
    let mut data = empty_tables();
    for index in 0..12 {
        let source = (0x400 + index * 0x60) as u16;
        data[0xf7a + index * 2..0xf7c + index * 2].copy_from_slice(&source.to_le_bytes());
    }
    data
}

fn fence_fall(x: u16) -> Player {
    let mut player = Player::at_alley_floor(x);
    player.y = 104;
    player.vertical_coordinate = 154;
    player.vertical_direction = 1;
    player.band_state = 2;
    player.support_disabled = 1;
    player.previous_direction = 255;
    player.animation_frame = 5;
    player.phase = 7;
    player.idle_counter = 6;
    player.airborne_sprite = Sprite {
        source: 0x900,
        dimensions: 0x0b03,
    };
    player
}

#[test]
fn fence_reset_uses_advanced_x_and_preserves_animation_history() {
    let data = walking_tables();
    for (initial_x, edge_x, direction) in [(157, 0, 1), (158, 296, 255)] {
        let mut player = fence_fall(initial_x);
        player.horizontal_direction = 1;
        let outcome = player.update(Controls::default(), &data).unwrap();
        assert_eq!(outcome.draw, Draw::Erase);
        assert!(outcome.edge_reset);
        assert!(!outcome.landed);
        assert_eq!((player.x, player.horizontal_direction), (edge_x, direction));
        assert_eq!((player.y, player.vertical_coordinate), (180, 230));
        assert_eq!((player.entry_state, player.entry_delay), (3, 12));
        assert_eq!((player.vertical_direction, player.vertical_step), (0, 1));
        assert_eq!(player.horizontal_step, 2);
        assert_eq!((player.band_state, player.support_disabled), (0, 0));
        assert_eq!(player.previous_dimensions, 0x0b03);
        assert_eq!(
            (
                player.previous_direction,
                player.animation_frame,
                player.phase
            ),
            (255, 5, 7)
        );
        assert_eq!(player.idle_counter, 6);
        assert_eq!(player.airborne_sprite.source, 0x900);
        assert_eq!(player.airborne_sprite.dimensions, 0x0b03);
    }
}

#[test]
fn final_visible_fence_row_is_drawn_before_the_reset_update() {
    let data = walking_tables();
    let mut player = fence_fall(80);
    player.vertical_coordinate = 153;
    let visible = player.update(Controls::default(), &data).unwrap();
    assert_eq!(player.y, 104);
    assert_eq!(
        visible.draw,
        Draw::Sprite(Sprite {
            source: 0x900,
            dimensions: 0x0103,
        })
    );
    assert!(!visible.edge_reset);
    assert_eq!(player.vertical_step, 2);
    let hidden = player.update(Controls::default(), &data).unwrap();
    assert_eq!(hidden.draw, Draw::Erase);
    assert!(hidden.edge_reset);
}

#[test]
fn edge_entry_waits_then_crops_walking_rows_from_both_sides() {
    let data = walking_tables();
    for (x, direction, first_x, second_x, first_source, second_source) in [
        (80, 1, 0, 0, 0x404, 0x462),
        (180, 255, 312, 304, 0x640, 0x6a0),
    ] {
        let mut player = fence_fall(x);
        player.previous_direction = direction ^ 254;
        player.update(Controls::default(), &data).unwrap();
        for delay in (0..12).rev() {
            let wait = player.update(Controls { x: 0, y: -1 }, &data).unwrap();
            assert_eq!(wait.draw, Draw::Keep);
            assert_eq!((player.entry_state, player.entry_delay), (3, delay));
            assert_eq!((player.phase, player.animation_frame), (7, 5));
        }
        for (state, expected_x, source, dimensions, phase) in [
            (2, first_x, first_source, 0x0b01, 6),
            (1, second_x, second_source, 0x0b02, 5),
        ] {
            let frame = player.update(Controls { x: 0, y: -1 }, &data).unwrap();
            assert_eq!(player.entry_state, state);
            assert_eq!(player.x, expected_x);
            assert_eq!(player.phase, phase);
            assert_eq!(
                frame.draw,
                Draw::StridedSprite {
                    sprite: Sprite { source, dimensions },
                    stride_bytes: 6,
                }
            );
            assert_eq!(player.vertical_direction, 0);
        }
        let complete = player.update(Controls::default(), &data).unwrap();
        assert_eq!(complete.draw, Draw::Idle);
        assert_eq!(player.x, if direction == 1 { 8 } else { 296 });
        assert_eq!(
            (player.entry_state, player.phase, player.horizontal_step),
            (0, 8, 2)
        );
    }
}

#[test]
fn final_entry_update_processes_held_controls_after_inward_step() {
    let data = walking_tables();
    for (direction, expected_x) in [(1, 16), (255, 288)] {
        let mut player = Player::at_alley_floor(8);
        player.entry_state = 1;
        player.horizontal_direction = direction;
        player.x = if direction == 1 { 0 } else { 304 };
        let complete = player
            .update(
                Controls {
                    x: direction as i8,
                    y: 0,
                },
                &data,
            )
            .unwrap();
        assert_eq!(player.x, expected_x);
        assert_eq!(player.horizontal_step, 8);
        assert!(matches!(complete.draw, Draw::Sprite(_)));

        player.entry_state = 1;
        player.horizontal_direction = direction;
        player.x = if direction == 1 { 0 } else { 304 };
        let jump = player.update(Controls { x: 0, y: -1 }, &data).unwrap();
        assert_eq!(player.x, if direction == 1 { 8 } else { 296 });
        assert_eq!(jump.draw, Draw::Keep);
        assert_eq!((player.entry_state, player.vertical_direction), (0, 255));
        assert_eq!((player.horizontal_step, player.phase_decrement), (6, 112));
    }
}

#[test]
fn imported_entry_delay_obeys_dog_activity_and_death_gates() {
    let mut data = vec![0; 0x1cc0];
    data[0x558] = 3;
    data[0x559] = 12;
    data[0x56e] = 1;
    data[0x57b] = 180;
    let mut player = Player::from_data(&data).unwrap();
    data[0x1cbf] = 1;
    assert_eq!(
        player.update(Controls::default(), &data).unwrap().draw,
        Draw::Keep
    );
    assert_eq!(player.entry_delay, 12);
    data[0x1cbf] = 0;
    data[0x1cb8] = 255;
    player.update(Controls::default(), &data).unwrap();
    assert_eq!(player.entry_delay, 12);
    data[0x1cb8] = 0;
    player.update(Controls::default(), &data).unwrap();
    assert_eq!(player.entry_delay, 11);
}

fn projectile_tables(x: u16, y: u8) -> Vec<u8> {
    let mut data = vec![0; crate::assets::DATA_LENGTH];
    crate::assets::set_word(&mut data, 0x1671, x);
    data[0x1673] = y;
    crate::assets::set_word(&mut data, 0x17df, 0x0c02);
    data
}

#[test]
fn projectile_checkpoint_precedes_full_band_clip_and_edge_reset() {
    let data = projectile_tables(80, 120);
    let mut player = Player::at_alley_floor(50);
    player.y = 104;
    player.vertical_coordinate = 154;
    player.horizontal_direction = 1;
    player.horizontal_step = 8;
    player.vertical_direction = 1;
    player.vertical_step = 4;
    player.support_disabled = 1;
    player.band_state = 2;
    player.airborne_sprite.dimensions = 0x0c02;
    assert!(!crate::projectiles::overlaps(&data, &player));
    let mut protected = player.clone();
    let hit = player
        .update_with_projectiles(Controls::default(), &data, true)
        .unwrap();
    assert!(hit.projectile_contact);
    assert!(!hit.edge_reset);
    assert_eq!(hit.draw, Draw::Keep);
    assert_eq!((player.x, player.y, player.entry_state), (58, 108, 0));
    let ordinary = protected
        .update_with_projectiles(Controls::default(), &data, false)
        .unwrap();
    assert!(!ordinary.projectile_contact);
    assert!(ordinary.edge_reset);
    assert_eq!(
        (protected.x, protected.y, protected.entry_state),
        (0, 180, 3)
    );
}

#[test]
fn projectile_checkpoint_detects_hidden_top_motion_before_returning_keep() {
    let data = projectile_tables(80, 1);
    let mut player = Player::at_alley_floor(50);
    player.y = 0;
    player.vertical_coordinate = 10;
    player.horizontal_direction = 1;
    player.horizontal_step = 8;
    player.vertical_direction = 255;
    player.vertical_step = 8;
    player.support_disabled = 1;
    player.airborne_sprite.dimensions = 0x0c02;
    let hit = player
        .update_with_projectiles(Controls::default(), &data, true)
        .unwrap();
    assert!(hit.projectile_contact);
    assert_eq!(hit.draw, Draw::Keep);
    assert_eq!((player.x, player.y, player.vertical_coordinate), (58, 0, 2));
}

#[test]
fn walking_contact_preserves_moved_position_before_committing_dimensions() {
    let data = projectile_tables(80, 180);
    let mut player = Player::at_alley_floor(50);
    player.horizontal_step = 8;
    player.previous_dimensions = 0x0c02;
    let hit = player
        .update_with_projectiles(Controls { x: 1, y: 0 }, &data, true)
        .unwrap();
    assert!(hit.projectile_contact);
    assert_eq!(player.x, 58);
    assert_eq!(player.previous_dimensions, 0x0c02);
    assert_eq!(hit.draw, Draw::Keep);
}

#[test]
fn initial_projectile_contact_precedes_death_and_entry_delay_gates() {
    let mut data = projectile_tables(80, 180);
    data[0x1cb8] = 255;
    let mut player = Player::at_alley_floor(80);
    player.entry_state = 3;
    player.entry_delay = 12;
    let hit = player
        .update_with_projectiles(Controls::default(), &data, true)
        .unwrap();
    assert!(hit.projectile_contact);
    assert_eq!(player.entry_delay, 12);
}

#[test]
fn fully_hidden_top_erases_the_image_and_clears_the_latch_when_reappearing() {
    let mut data = room_tables();
    let mut player = Player::at_alley_floor(100);
    player.vertical_coordinate = 37;
    player.y = 0;
    player.vertical_direction = 255;
    player.vertical_step = 1;
    player.support_disabled = 1;
    player.airborne_sprite = Sprite {
        source: 0x6100,
        dimensions: 0x0e03,
    };
    let hidden = player.update(Controls::default(), &data).unwrap();
    assert_eq!(hidden.draw, Draw::Erase);
    player.write_data(&mut data).unwrap();
    assert_eq!(data[0x583], 1);
    player = Player::from_data(&data).unwrap();
    player.vertical_direction = 1;
    player.vertical_step = 2;
    let visible = player.update(Controls::default(), &data).unwrap();
    assert_eq!(
        visible.draw,
        Draw::Sprite(Sprite {
            source: 0x6100 + 12 * 6,
            dimensions: 0x0203,
        })
    );
    player.write_data(&mut data).unwrap();
    assert_eq!(data[0x583], 0);
}
