// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

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
fn corrected_hold_changes_only_the_one_tick_difficulty_six_entry() {
    for difficulty in 0..8 {
        for bonus in [0, 1] {
            let mut data = data();
            set_word(&mut data, 8, difficulty);
            data[0x418] = bonus;
            let native_delay = if difficulty == 6 { 1 } else { 60 + difficulty };
            set_word(
                &mut data,
                0x181e + usize::from(difficulty) * 2,
                native_delay,
            );
            set_word(&mut data, 0x2ae5, 0x10);
            let mut player = Player::at_alley_floor(200);
            player.y = 96;
            let mut screen = Framebuffer::default();
            advance(
                &mut data,
                &mut screen,
                &player,
                65530,
                false,
                HoldTiming::default(),
            )
            .unwrap();
            let delay = if difficulty == 6 { 18 } else { native_delay };
            assert_eq!(word(&data, 0x166c), delay);
            assert_eq!(
                word(&data, 0x181e + usize::from(difficulty) * 2),
                native_delay
            );
            assert_eq!(data[0x1669], 8);
            data[0x1665] = 16;
            data[0x166a] = 1;
            advance(
                &mut data,
                &mut screen,
                &player,
                65530,
                false,
                HoldTiming::default(),
            )
            .unwrap();
            assert_eq!(data[0x1665], 15);
            for (elapsed, phase) in [(delay - 1, 15), (delay, 14)] {
                data[0x166a] = 1;
                advance(
                    &mut data,
                    &mut screen,
                    &player,
                    65530u16.wrapping_add(elapsed),
                    false,
                    HoldTiming::default(),
                )
                .unwrap();
                assert_eq!(data[0x1665], phase);
            }
        }
    }
    let mut data = data();
    set_word(&mut data, 0x181e + 6 * 2, 7);
    assert_eq!(HoldTiming::default().delay(&data, 6), 7);
}

#[test]
fn countdown_wraps_and_retrace_rejection_still_resets_it() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    let player = Player::at_alley_floor(8);
    data[0x166a] = 0;
    assert!(
        !advance(
            &mut data,
            &mut screen,
            &player,
            1,
            false,
            HoldTiming::default()
        )
        .unwrap()
        .accepted
    );
    assert_eq!(data[0x166a], 255);
    data[0x166a] = 1;
    let seed = word(&data, 0x2ae5);
    assert!(
        !advance(
            &mut data,
            &mut screen,
            &player,
            1,
            true,
            HoldTiming::default()
        )
        .unwrap()
        .accepted
    );
    assert_eq!(data[0x166a], 13);
    assert_eq!(word(&data, 0x2ae5), seed);
    assert!(
        advance(
            &mut data,
            &mut screen,
            &player,
            1,
            false,
            HoldTiming::default()
        )
        .unwrap()
        .drawn_row
        .is_none()
    );
    assert_eq!(data[0x166a], 12);
}

#[test]
fn selection_work_counts_retries_without_advancing_extra_random_values() {
    let mut data = data();
    let mut player = Player::at_alley_floor(24);
    player.y = 24;
    let update = advance(
        &mut data,
        &mut Framebuffer::default(),
        &player,
        110,
        false,
        HoldTiming::default(),
    )
    .unwrap();
    assert_eq!(update.work.selections, 3);
    assert_eq!(word(&data, 0x2ae5), 8);
    assert_eq!((data[0x1669], data[0x1665]), (8, 28));
    assert!(update.drawn_row.is_some());
}

#[test]
fn inactive_selection_rejects_high_player_or_id_without_creating_a_window() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    let mut player = Player::at_alley_floor(8);
    let seed = word(&data, 0x2ae5);
    assert!(
        advance(
            &mut data,
            &mut screen,
            &player,
            1,
            false,
            HoldTiming::default()
        )
        .unwrap()
        .accepted
    );
    assert_eq!(word(&data, 0x2ae5), seed);
    assert_eq!(data[0x1665], 0);
    player.y = 96;
    data[0x166a] = 1;
    set_word(&mut data, 0x2ae5, 0x18);
    assert_eq!(
        advance(
            &mut data,
            &mut screen,
            &player,
            1,
            false,
            HoldTiming::default()
        )
        .unwrap()
        .drawn_row,
        None
    );
    assert_eq!(word(&data, 0x2ae5), 12);
    assert_eq!(data[0x1665], 0);
    data[0x166a] = 1;
    set_word(&mut data, 0x2ae5, 0x40);
    let update = advance(
        &mut data,
        &mut screen,
        &player,
        1,
        false,
        HoldTiming::default(),
    )
    .unwrap();
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
        advance(
            &mut data,
            &mut screen,
            &player,
            tick,
            false,
            HoldTiming::default(),
        )
        .unwrap();
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
        let update = advance(
            &mut data,
            &mut screen,
            &player,
            65534,
            false,
            HoldTiming::default(),
        )
        .unwrap();
        assert_eq!(data[0x1665], phase - 1);
        assert_eq!(update.drawn_row, Some(88 + phase - 15));
    }
    assert_eq!(word(&data, 0x166e), 65534);
    data[0x166a] = 1;
    let before = screen.bytes().to_vec();
    assert_eq!(
        advance(
            &mut data,
            &mut screen,
            &player,
            2,
            false,
            HoldTiming::default()
        )
        .unwrap()
        .drawn_row,
        None
    );
    assert_eq!(screen.bytes().as_slice(), before);
    assert_eq!(data[0x1673], 0);
    for phase in (1..=15).rev() {
        data[0x166a] = 1;
        let update = advance(
            &mut data,
            &mut screen,
            &player,
            50,
            false,
            HoldTiming::default(),
        )
        .unwrap();
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
    let update = advance(
        &mut data,
        &mut screen,
        &player,
        100,
        false,
        HoldTiming::default(),
    )
    .unwrap();
    assert!(update.contact_hold);
    assert_eq!(data[0x551], 1);
    assert_eq!(data[0x1665], 15);
    player.y = 180;
    active(&mut data, 29);
    data[0x1669] = 0;
    set_word(&mut data, 0x1f32, 104);
    data[0x1f51] = 1; // Inactivity does not exempt this mouse rectangle.
    let update = advance(
        &mut data,
        &mut screen,
        &player,
        100,
        false,
        HoldTiming::default(),
    )
    .unwrap();
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
    advance(
        &mut data,
        &mut screen,
        &player,
        1,
        false,
        HoldTiming::default(),
    )
    .unwrap();
    assert_eq!(
        screen
            .read_rect(pixel_offset(104, 102), Dimensions(0x0104))
            .unwrap(),
        [0x35; 8]
    );
    active(&mut data, 1);
    data[0x2681 + 14 * 10..0x2681 + 14 * 10 + 8].fill(0x72);
    advance(
        &mut data,
        &mut screen,
        &player,
        1,
        false,
        HoldTiming::default(),
    )
    .unwrap();
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
        let (selected, work) = choose_room(&mut data).unwrap();
        assert_eq!(selected, room);
        assert!(work.attempts >= 1);
        assert_eq!(work.history_rejections, work.attempts - 1);
        if seed == 0x40 && latest == 4 && previous == 4 {
            assert_eq!((work.attempts, work.extra_draws), (2, 1));
        }
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
    let (room, work) = select_room(&mut data, &player, 65535).unwrap();
    assert_eq!(room, 7);
    assert_eq!(
        (work.attempts, work.extra_draws, work.history_rejections),
        (0, 0, 0)
    );
    assert_eq!(word(&data, 0x410), 65535);
    assert_eq!(
        (word(&data, 1), data[3], data[0x419], data[0x418]),
        (110, 180, 1, 0)
    );
    assert_eq!(word(&data, 0x2ae5), seed);
    assert_eq!(data[0x41d..0x421], history);
}
