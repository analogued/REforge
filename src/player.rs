// Copyright 2026 analogued. License selection is pending.

//! Accepted-update movement for the alley and ordinary room player paths.
//!
//! The caller supplies the current external game data segment, including alley
//! support and sprite tables. Timing, actors, collision effects,
//! sound, and idle sprite randomness belong to the caller.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Controls {
    /// -1 means left/up, 0 means neutral, 1 means right/down.
    pub x: i8,
    pub y: i8,
}

#[cfg(test)]
mod tests {
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
            173, 167, 162, 158, 155, 152, 150, 149, 150, 152, 154, 157, 161, 165, 169, 173, 177,
            180,
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
        assert_eq!(player.check_support(&data).unwrap(), Support::None);
        player.x = 16;
        assert_eq!(player.check_support(&data).unwrap(), Support::LowerList);
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sprite {
    pub source: u16,
    /// Low byte: words per row. High byte: row count.
    pub dimensions: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Draw {
    Keep,
    /// Restore the current player background without drawing a replacement.
    Erase,
    /// Select and draw the original two idle pieces using the shared PRNG.
    Idle,
    Sprite(Sprite),
    /// Copy each visible row from a wider original source row before drawing.
    StridedSprite {
        sprite: Sprite,
        stride_bytes: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Support {
    #[default]
    None,
    Floor,
    LowerList,
    Band96,
    UpperBitfield,
    RoomTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateOutcome {
    pub draw: Draw,
    pub landed: bool,
    pub support: Support,
    /// The original 070d edge initializer ran during this update.
    pub edge_reset: bool,
    /// The lower-support caller tested the bin cat on this update.
    pub bin_contact_checked: bool,
    /// Logical input written by the bin cat, retained until the next poll.
    pub forced_controls: Option<Controls>,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub x: u16,
    pub y: u8,
    pub vertical_coordinate: u8,
    pub horizontal_direction: u8,
    pub vertical_direction: u8,
    pub horizontal_step: u16,
    pub vertical_step: u8,
    pub phase: u8,
    pub phase_decrement: u8,
    /// Original upper-support state: 0 free, 1 newly attached, 2 attached.
    pub support_state: u8,
    /// Original band-96 state at data offset 0550.
    pub band_state: u8,
    pub animation_frame: u8,
    previous_direction: u8,
    support_delay: u8,
    support_disabled: u8,
    pub entry_state: u8,
    pub entry_delay: u8,
    alternate_animation: u8,
    alternate_phase: u16,
    idle_counter: u8,
    previous_dimensions: u16,
    airborne_sprite: Sprite,
    last_support: Support,
    room_exit_requested: bool,
    /// Scene-four support-table hole identity at DS:39e0 (zero off a hole).
    hole_marker: u8,
}

fn word(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

impl Player {
    /// CS:2567 drops an attached cat without resetting its airborne phase,
    /// vertical step, or sprite history, and without removing a life.
    pub(crate) fn knock_down_from_mouse(&mut self) {
        self.support_state = 0;
        self.support_delay = 17;
        self.vertical_direction = 1;
        self.horizontal_direction = 0;
        self.last_support = Support::None;
    }

    pub(crate) fn clothes_motion_disabled(&self) -> bool {
        self.support_disabled != 0
    }

    /// CS:04a0 carries an attached cat with its garment. At either edge it
    /// releases support without losing a life or moving X beyond the boundary.
    pub(crate) fn carry_with_clothes(&mut self, left: bool) {
        let next = if left {
            self.x.checked_sub(4).filter(|x| *x >= 8)
        } else {
            Some(self.x.wrapping_add(4)).filter(|x| *x < 0x123)
        };
        if let Some(next) = next {
            self.x = next;
        } else {
            self.support_delay = 17;
            self.vertical_direction = 1;
            self.vertical_step = 1;
            self.phase_decrement = 0x18;
            self.horizontal_step = (self.horizontal_step & 0xff00) | 1;
            self.support_state = 0;
            self.last_support = Support::None;
        }
    }

    /// Start after the original alley entry has completed.
    pub fn at_alley_floor(x: u16) -> Self {
        Self {
            x: x.clamp(8, 290),
            y: 180,
            vertical_coordinate: 230,
            horizontal_direction: 0,
            vertical_direction: 0,
            horizontal_step: 2,
            vertical_step: 1,
            phase: 8,
            phase_decrement: 0,
            support_state: 0,
            band_state: 0,
            animation_frame: 0,
            previous_direction: 0,
            support_delay: 0,
            support_disabled: 0,
            entry_state: 0,
            entry_delay: 0,
            alternate_animation: 0,
            alternate_phase: 0,
            idle_counter: 0,
            previous_dimensions: 0,
            airborne_sprite: Sprite {
                source: 0,
                dimensions: 0,
            },
            last_support: Support::Floor,
            room_exit_requested: false,
            hole_marker: 0,
        }
    }

    /// Import state from a data-segment capture. No original data is embedded.
    pub fn from_data(data: &[u8]) -> Result<Self, &'static str> {
        Self::check_data(data)?;
        if !matches!(word(data, 4), 0 | 4 | 5 | 7) {
            return Err("player core supports the alley, cheese, birdcage and hearts scenes only");
        }
        Ok(Self {
            x: word(data, 0x579),
            y: data[0x57b],
            vertical_coordinate: data[0x57c],
            horizontal_direction: data[0x56e],
            previous_direction: data[0x56f],
            vertical_direction: data[0x571],
            horizontal_step: word(data, 0x572),
            vertical_step: data[0x576],
            phase: data[0x577],
            phase_decrement: data[0x578],
            support_state: data[0x55c],
            band_state: data[0x550],
            animation_frame: data[0x56b],
            support_delay: data[0x55b],
            support_disabled: data[0x55a],
            entry_state: data[0x558],
            entry_delay: data[0x559],
            alternate_animation: data[0x584],
            alternate_phase: word(data, 0x585),
            idle_counter: data[0x56d],
            previous_dimensions: word(data, 0x561),
            airborne_sprite: Sprite {
                source: word(data, 0x569),
                dimensions: word(data, 0x567),
            },
            last_support: Support::None,
            room_exit_requested: false,
            hole_marker: data.get(0x39e0).copied().unwrap_or(0),
        })
    }

    /// Mirror owned movement state for original-style room actor helpers.
    pub(crate) fn write_data(&self, data: &mut [u8]) -> Result<(), &'static str> {
        Self::check_data(data)?;
        for (offset, value) in [
            (0x579, self.x),
            (0x572, self.horizontal_step),
            (0x561, self.previous_dimensions),
            (0x569, self.airborne_sprite.source),
            (0x567, self.airborne_sprite.dimensions),
            (0x585, self.alternate_phase),
        ] {
            crate::assets::set_word(data, offset, value);
        }
        for (offset, value) in [
            (0x57b, self.y),
            (0x57c, self.vertical_coordinate),
            (0x56e, self.horizontal_direction),
            (0x56f, self.previous_direction),
            (0x571, self.vertical_direction),
            (0x576, self.vertical_step),
            (0x577, self.phase),
            (0x578, self.phase_decrement),
            (0x55c, self.support_state),
            (0x550, self.band_state),
            (0x56b, self.animation_frame),
            (0x55b, self.support_delay),
            (0x55a, self.support_disabled),
            (0x558, self.entry_state),
            (0x559, self.entry_delay),
            (0x584, self.alternate_animation),
            (0x56d, self.idle_counter),
        ] {
            data[offset] = value;
        }
        if data.len() > 0x39e0 {
            data[0x39e0] = self.hole_marker;
        }
        Ok(())
    }

    pub(crate) fn apply_actor_state(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let mut updated = Self::from_data(data)?;
        updated.last_support = if updated.vertical_direction != 0 && updated.support_state == 0 {
            Support::None
        } else {
            self.last_support
        };
        *self = updated;
        Ok(())
    }

    pub(crate) fn take_room_exit(&mut self) -> bool {
        std::mem::take(&mut self.room_exit_requested)
    }

    pub(crate) fn floor_track_due(&self, controls: Controls) -> bool {
        self.entry_state == 0
            && self.support_state == 0
            && self.vertical_direction == 0
            && self.y >= 180
            && controls.y != -1
            && controls.x != 0
    }

    fn check_data(data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 0x1028 {
            Err("external data segment is too short for alley player tables")
        } else if matches!(word(data, 4), 4 | 5 | 7) && data.len() < crate::assets::DATA_LENGTH {
            Err("external data segment is too short for room player tables")
        } else {
            Ok(())
        }
    }

    /// Advance exactly one accepted update. This does not poll a host clock.
    pub fn update(
        &mut self,
        mut controls: Controls,
        data: &[u8],
    ) -> Result<UpdateOutcome, &'static str> {
        Self::check_data(data)?;
        self.room_exit_requested = false;
        if !(-1..=1).contains(&controls.x) || !(-1..=1).contains(&controls.y) {
            return Err("control directions must be -1, 0, or 1");
        }
        if self.alternate_animation != 0 && !matches!(word(data, 4), 4 | 5 | 7) {
            return Err("collision-effect animation is outside the player core");
        }
        let mut outcome = UpdateOutcome {
            draw: Draw::Keep,
            landed: false,
            support: self.last_support,
            edge_reset: false,
            bin_contact_checked: false,
            forced_controls: None,
        };
        // CS:092d..0939 gates all movement during a paired-hole transfer.
        // Its sprite and background are owned by the separate room helper.
        if word(data, 4) == 4 && data[0x39e1] != 0 {
            return Ok(outcome);
        }
        if data.get(0x1cb8).copied().unwrap_or(0) != 0 {
            return Ok(outcome);
        }
        if self.entry_state != 0 {
            if self.entry_delay != 0 {
                if data.get(0x1cbf).copied().unwrap_or(0) == 0 {
                    self.entry_delay -= 1;
                }
                return Ok(outcome);
            }
            self.entry_state = self.entry_state.wrapping_sub(1);
            if self.entry_state != 0 {
                let mut sprite = self.select_walk_sprite(data);
                let crop_words = self.entry_state;
                sprite.dimensions = 0x0b00 | u16::from(3u8.wrapping_sub(crop_words));
                if self.horizontal_direction == 255 {
                    self.x = 296 + u16::from(crop_words) * 8;
                } else {
                    self.x = 0;
                    sprite.source = sprite.source.wrapping_add(u16::from(crop_words) * 2);
                }
                self.previous_dimensions = sprite.dimensions;
                outcome.draw = Draw::StridedSprite {
                    sprite,
                    stride_bytes: 6,
                };
                return Ok(outcome);
            }
            self.horizontal_step = 8;
            self.advance_x();
            // The original falls through to ordinary controls on this same update.
        }
        if self.support_state != 0 {
            if self.support_state == 1 {
                self.support_state = 2;
                self.horizontal_step = 6;
                outcome.draw = self.draw_sprite(Sprite {
                    source: 0x9da,
                    dimensions: 0x0e03,
                });
            }
            if controls.y != 0 {
                self.request_transition(controls, data);
                outcome.support = Support::None;
            }
            return Ok(outcome);
        }
        if self.vertical_direction != 0 {
            self.advance_airborne(data, &mut outcome)?;
            return Ok(outcome);
        }

        if self.y < 180 || word(data, 4) == 7 {
            let support = self.check_support(data)?;
            if support == Support::None {
                self.horizontal_direction = 0;
                self.vertical_direction = 1;
                self.start_fall(data);
                self.last_support = Support::None;
                outcome.support = Support::None;
                return Ok(outcome);
            }
            self.last_support = support;
            outcome.bin_contact_checked = word(data, 4) == 0;
            if outcome.bin_contact_checked && crate::bin_cat::overlaps(data, self) {
                // Predict the one shared draw without mutating the read-only
                // player tables. Alley commits it before any later consumer.
                let random = crate::assets::random_successor(word(data, 0x2ae5));
                controls = Controls {
                    x: if random & 1 != 0 { 1 } else { -1 },
                    y: 1,
                };
                outcome.forced_controls = Some(controls);
            }
        } else {
            self.last_support = Support::Floor;
        }
        outcome.support = self.last_support;
        self.previous_direction = self.horizontal_direction;
        self.horizontal_direction = controls.x as u8;
        self.vertical_direction = controls.y as u8;
        if self.vertical_direction != 0 {
            if self.vertical_direction == 1 && self.y >= 180 {
                self.vertical_direction = 0;
                self.alternate_animation = 0;
            } else {
                self.start_vertical_transition(data);
                outcome.support = Support::None;
                return Ok(outcome);
            }
        }
        self.advance_x();
        if self.horizontal_direction == 0 {
            self.horizontal_step = 2;
            self.phase = 8;
            if self.previous_dimensions == 0x0c02 {
                self.idle_counter = self.idle_counter.wrapping_add(1);
                if self.idle_counter & 7 != 0 {
                    return Ok(outcome);
                }
            }
            self.previous_dimensions = 0x0c02;
            outcome.draw = Draw::Idle;
        } else {
            let sprite = self.select_walk_sprite(data);
            outcome.draw = self.draw_sprite(sprite);
        }
        Ok(outcome)
    }

    fn draw_sprite(&mut self, sprite: Sprite) -> Draw {
        self.previous_dimensions = sprite.dimensions;
        Draw::Sprite(sprite)
    }

    fn select_walk_sprite(&mut self, data: &[u8]) -> Sprite {
        if self.horizontal_direction != self.previous_direction {
            self.horizontal_step = 2;
        }
        if self.horizontal_step < 8 {
            self.phase = self.phase.wrapping_sub(1);
            if self.phase & 3 == 0 {
                self.horizontal_step += 1;
            }
        }
        self.animation_frame = self.animation_frame.wrapping_add(1);
        if self.animation_frame >= 6 {
            self.animation_frame = 0;
        }
        let index = usize::from(self.animation_frame)
            + if self.horizontal_direction == 255 {
                6
            } else {
                0
            };
        Sprite {
            source: word(data, 0xf7a + index * 2),
            dimensions: 0x0b03,
        }
    }

    fn initialize_alley_edge(&mut self) {
        let right_edge = self.x >= 160;
        self.x = if right_edge { 296 } else { 0 };
        self.horizontal_direction = if right_edge { 255 } else { 1 };
        self.y = 180;
        self.vertical_coordinate = 230;
        self.entry_state = 3;
        self.entry_delay = 12;
        self.previous_dimensions = 0x0b03;
        self.vertical_direction = 0;
        self.horizontal_step = 2;
        self.vertical_step = 1;
        self.support_delay = 0;
        self.band_state = 0;
        self.support_state = 0;
        self.support_disabled = 0;
        self.alternate_animation = 0;
        self.last_support = Support::Floor;
    }

    fn request_transition(&mut self, controls: Controls, data: &[u8]) {
        self.previous_direction = self.horizontal_direction;
        self.horizontal_direction = controls.x as u8;
        self.vertical_direction = controls.y as u8;
        self.start_vertical_transition(data);
    }

    fn start_vertical_transition(&mut self, data: &[u8]) {
        if self.vertical_direction == 1 {
            self.start_fall(data);
        } else {
            self.support_delay = 0;
            let old_step = self.horizontal_step as u8;
            if old_step > 2 {
                // The original changes AL only, preserving the high byte.
                self.horizontal_step =
                    (self.horizontal_step & 0xff00) | u16::from(old_step.wrapping_sub(2));
            }
            self.vertical_step = 8;
            self.phase_decrement = (old_step ^ 15).wrapping_shl(4);
            if self.band_state == 1 {
                self.band_state = 2;
            }
            self.initialize_transition_sprite(data);
        }
    }

    fn start_fall(&mut self, data: &[u8]) {
        self.vertical_step = 1;
        self.phase_decrement = 0x20;
        self.support_delay = 8;
        if self.band_state == 1 {
            self.band_state = 0;
        }
        self.initialize_transition_sprite(data);
    }

    fn initialize_transition_sprite(&mut self, data: &[u8]) {
        self.phase = 1;
        self.support_state = 0;
        self.hole_marker = 0;
        let direction = self.horizontal_direction.wrapping_add(1).wrapping_mul(2);
        let index = usize::from(direction) + if self.vertical_direction == 255 { 0 } else { 6 };
        self.airborne_sprite = Sprite {
            source: word(data, 0xfaa + index),
            dimensions: word(data, 0xfb6 + index),
        };
        self.last_support = Support::None;
    }

    /// Return whether the original helper would set carry after clamping.
    fn advance_x(&mut self) -> bool {
        match self.horizontal_direction {
            0 => false,
            1 => {
                let next = self.x.wrapping_add(self.horizontal_step);
                self.x = if next >= 291 { 290 } else { next };
                next >= 291
            }
            _ => {
                let (next, borrow) = self.x.overflowing_sub(self.horizontal_step);
                self.x = if borrow || next < 8 { 8 } else { next };
                borrow || next < 8
            }
        }
    }

    fn advance_airborne(
        &mut self,
        data: &[u8],
        outcome: &mut UpdateOutcome,
    ) -> Result<(), &'static str> {
        if self.advance_x() {
            self.horizontal_direction = 0;
            self.vertical_step = 2;
            self.vertical_direction = 1;
            self.support_delay = 0;
        } else {
            let (phase, borrow) = self.phase.overflowing_sub(self.phase_decrement);
            self.phase = phase;
            if borrow {
                if self.vertical_direction == 1 {
                    if self.vertical_step < 4 {
                        self.vertical_step += 1;
                    }
                } else if self.vertical_step > 1 {
                    self.vertical_step -= 1;
                } else {
                    self.vertical_direction = 1;
                }
            }
        }
        let mut support = Support::None;
        if self.support_disabled == 0 {
            if self.support_delay != 0 {
                self.support_delay -= 1;
            }
            if self.support_delay == 0 && self.vertical_direction == 1 {
                support = self.check_support(data)?;
            }
        }
        if support != Support::None {
            self.finish_landing();
        } else if self.vertical_direction == 1 {
            let next = self.vertical_coordinate.wrapping_add(self.vertical_step);
            if word(data, 4) == 7 {
                // CS:0d0e..0d20: scene seven has no floor at Y180. Falling
                // continues below its bottom hearts until coordinate248.
                self.vertical_coordinate = next.min(248);
                if next >= 248 {
                    self.room_exit_requested = true;
                }
            } else if next > 230 {
                self.vertical_coordinate = 230;
                self.band_state = 0;
                self.finish_landing();
                support = Support::Floor;
            } else {
                self.vertical_coordinate = next;
            }
        } else {
            let (next, borrow) = self.vertical_coordinate.overflowing_sub(self.vertical_step);
            self.vertical_coordinate = if borrow { 0 } else { next };
            if borrow {
                self.vertical_direction = 1;
                self.vertical_step = 1;
            }
        }
        self.y = self.vertical_coordinate.saturating_sub(50);
        self.last_support = support;
        outcome.support = support;
        outcome.landed = support != Support::None;
        let mut sprite = self.airborne_sprite;
        if self.alternate_animation != 0 {
            self.alternate_phase = self.alternate_phase.wrapping_add(2);
            let index = usize::from(self.alternate_phase & 14);
            sprite = Sprite {
                source: word(data, 0xfc2 + index),
                dimensions: word(data, 0xfd2 + index),
            };
        }
        let hidden_rows = 50u8.saturating_sub(self.vertical_coordinate);
        if hidden_rows != 0 {
            let rows = (sprite.dimensions >> 8) as u8;
            if hidden_rows >= rows {
                outcome.draw = Draw::Keep;
                return Ok(());
            }
            sprite.source = if self.alternate_animation != 0 {
                self.airborne_sprite.source
            } else {
                sprite.source
            }
            .wrapping_add(u16::from(hidden_rows) * (sprite.dimensions & 255) * 2);
            sprite.dimensions -= u16::from(hidden_rows) << 8;
        }
        // CS:0dde..0e1a clips below Y187 in scene seven, versus Y94 for
        // the alley's band-two fall. Only the latter resets to an alley edge.
        let clip_y = if word(data, 4) == 7 {
            Some(187)
        } else if word(data, 4) == 0 && self.band_state == 2 {
            Some(94)
        } else {
            None
        };
        if let Some(clip_y) = clip_y.filter(|boundary| self.y >= *boundary) {
            let rows = (sprite.dimensions >> 8) as u8;
            let hidden_rows = self.y - clip_y;
            if hidden_rows >= rows {
                if word(data, 4) == 7 {
                    self.room_exit_requested = true;
                    outcome.draw = Draw::Erase;
                    return Ok(());
                }
                self.initialize_alley_edge();
                outcome.draw = Draw::Erase;
                outcome.edge_reset = true;
                outcome.support = Support::Floor;
                return Ok(());
            }
            sprite.dimensions -= u16::from(hidden_rows) << 8;
            self.vertical_step = 2;
        }
        outcome.draw = self.draw_sprite(sprite);
        Ok(())
    }

    fn finish_landing(&mut self) {
        self.vertical_direction = 0;
        self.horizontal_step = 2;
        self.support_delay = 0;
        self.support_disabled = 0;
        self.alternate_animation = 0;
    }

    fn check_support(&mut self, data: &[u8]) -> Result<Support, &'static str> {
        if word(data, 4) == 0 {
            return Ok(self.check_alley_support(data));
        }
        if word(data, 4) != 7 {
            self.hole_marker = 0;
        }
        let check = if word(data, 4) == 7 {
            crate::hearts::resolve_support(data, self)
        } else {
            crate::room::resolve_scene_support(data, self)
        }
        .map_err(|_| "invalid room support tables")?;
        self.room_exit_requested |= check.exit;
        if let Some(hit) = check.hit {
            self.y = hit.y;
            self.vertical_coordinate = hit.y.wrapping_add(50);
            self.support_state = hit.state;
            if hit.state != 0 {
                self.x &= 0xfffc;
            }
            if word(data, 4) == 4 && (39..55).contains(&hit.index) {
                self.hole_marker = (hit.index - 38) as u8;
            }
            Ok(Support::RoomTable)
        } else {
            Ok(Support::None)
        }
    }

    fn check_alley_support(&mut self, data: &[u8]) -> Support {
        let band = self.y & 0xf8;
        if band == 0x60 {
            if self.band_state >= 2 {
                return Support::None;
            }
            self.y = band;
            self.vertical_coordinate = band.wrapping_add(50);
            self.band_state = 1;
            return Support::Band96;
        }
        let list_band = self.y.wrapping_add(2) & 0xf8;
        let difficulty = usize::from(word(data, 8));
        if difficulty < 4 {
            let mut index = usize::from(data[0x1006 + difficulty]);
            // The accepted input's lists end before their index table.
            while 0xff0 + index < 0x1006 {
                let entry = data[0xff0 + index];
                if entry == 0 {
                    break;
                }
                index += 1;
                let target_band = if entry & 0x80 != 0 { 0x88 } else { 0x90 };
                let target_x = u16::from(entry & 0x7f) * 4;
                if list_band == target_band
                    && (self.x & 0xfff8) >= target_x
                    && (self.x.wrapping_sub(15) & 0xfff8) <= target_x
                {
                    self.y = target_band - 2;
                    self.vertical_coordinate = self.y + 50;
                    return Support::LowerList;
                }
            }
        }
        let row = match band {
            0x08 => 0,
            0x28 => 1,
            0x48 => 2,
            _ => return Support::None,
        };
        let mut x = self.x;
        if row == usize::from(word(data, 0x52f)) && data[0x525] <= 3 {
            x = if row == 1 {
                x.wrapping_sub(u16::from(data[0x525] + 1) * 4)
            } else {
                x.wrapping_add(u16::from(4 - data[0x525]) * 4)
            };
        }
        let position = x.wrapping_add(10);
        let byte_index = usize::from(position >> 6) + usize::from(data[0x1025 + row]);
        let mask = 0x80 >> ((position >> 3) & 7);
        if data
            .get(0x1016 + byte_index)
            .is_some_and(|byte| byte & mask != 0)
        {
            self.y = band;
            self.vertical_coordinate = band + 50;
            self.x &= 0xfff8;
            self.support_state = 1;
            Support::UpperBitfield
        } else {
            Support::None
        }
    }
}
