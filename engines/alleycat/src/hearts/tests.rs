// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn arrow_blocking_preserves_unsaved_cat_motion_and_skips_gift_work() {
    for (arrow_x, moved, scratch_x) in [(100, false, 90), (120, true, 98)] {
        let mut data = data();
        set_word(&mut data, 0x45b6, 2);
        let record = record_address(&data, 3).unwrap();
        set_word(&mut data, record, 90);
        data[record + 2] = 1;
        data[record + 3] = 107;
        set_word(&mut data, record + 7, 0);
        data[0x70f2] = 1;
        set_word(&mut data, 0x70f3, arrow_x);
        data[0x70f5] = 107;
        data[0x57b] = 56;
        let mut screen = Framebuffer::default();
        let before = screen.bytes().to_vec();
        let step = advance_cat(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(step.work, Some(Work::CatArrow { moved }));
        assert!(!step.drew && !step.item_changed);
        assert_eq!(word(&data, 0x4548), scratch_x);
        assert_eq!(word(&data, record), 90);
        assert_eq!(screen.bytes().as_slice(), before);
    }
}

#[test]
fn pickup_work_stops_at_the_first_active_hit_without_consuming_more_gifts() {
    let mut data = data();
    for slot in [6, 7] {
        data[0x2b72 + slot] = 1;
        set_word(&mut data, 0x2b5a + slot * 2, 200);
        data[0x2b6a + slot] = 120;
    }
    let step = pickup_item(
        &mut data,
        &mut Framebuffer::default(),
        1,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert!(matches!(
        step.work,
        Some(Work::Pickup {
            slots: 1,
            active: 1,
            hit: true,
            ..
        })
    ));
    assert_eq!(word(&data, 0x2e8d), 7);
    assert_eq!((data[0x2b72 + 6], data[0x2b72 + 7]), (1, 0));
}

#[cfg(feature = "dev-tools")]
#[test]
fn removing_arrows_restores_active_images_and_clears_cat_interactions() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    screen.fill_visible(0x55);
    data[0x70f2] = 1;
    data[0x70f7] = 0;
    set_word(&mut data, 0x70f3, 100);
    data[0x70f5] = 100;
    let offset = pixel_offset(100, 100);
    set_word(&mut data, 0x70f8, offset);
    data[0x70cc..0x70ec].fill(0x33);
    screen
        .copy_rect(offset, Dimensions(0x0802), &[0xff; 32])
        .unwrap();
    set_word(&mut data, 0x4548, 100);
    data[0x454b] = 100;
    assert!(cat_arrow_contact(&data));
    assert!(remove_arrow(&mut data, &mut screen).unwrap());
    assert!(!cat_arrow_contact(&data));
    assert_eq!((data[0x70f2], data[0x70f7]), (0, 1));
    assert_eq!(
        screen.read_rect(offset, Dimensions(0x0802)).unwrap(),
        vec![0x33; 32]
    );
    screen
        .copy_rect(offset, Dimensions(0x0802), &[0x11; 32])
        .unwrap();
    data[0x70f7] = 0;
    assert!(!remove_arrow(&mut data, &mut screen).unwrap());
    assert_eq!(
        screen.read_rect(offset, Dimensions(0x0802)).unwrap(),
        vec![0x11; 32]
    );
}

#[cfg(feature = "dev-tools")]
#[test]
fn removing_rivals_erases_only_their_images_and_preserves_goal_and_scratch() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    screen.fill_visible(0x55);
    for index in 0..7 {
        let record = record_address(&data, index).unwrap();
        let offset = pixel_offset(40 + index as u16 * 32, 80);
        set_word(&mut data, record + 4, offset);
        data[record + 6] = 0;
        screen
            .copy_rect(offset, Dimensions(0x0c03), &[0xff; 72])
            .unwrap();
    }
    let scratch = data[0x4548..0x4554].to_vec();
    let goal = record_address(&data, 6).unwrap();
    let goal_record = data[goal..goal + 12].to_vec();
    assert!(remove_rivals(&mut data, &mut screen).unwrap());
    assert_eq!(data[0x4548..0x4554], scratch);
    assert_eq!(data[goal..goal + 12], goal_record);
    for index in 0..7 {
        let record = record_address(&data, index).unwrap();
        let pixels = screen
            .read_rect(word(&data, record + 4), Dimensions(0x0c03))
            .unwrap();
        assert_eq!(pixels, vec![if index == 6 { 0xff } else { 0x55 }; 72]);
        if index != 6 {
            assert_eq!((data[record + 6], word(&data, record + 7)), (1, 0xffff));
        }
    }
    assert!(!remove_rivals(&mut data, &mut screen).unwrap());
}

#[cfg(feature = "dev-tools")]
#[test]
fn disabled_rivals_skip_motion_and_contact_but_goal_contact_still_succeeds() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    initialize_dev(&mut data, &mut screen, 1, true).unwrap();
    let rival_records = data[0x4554..0x459c].to_vec();
    let random = word(&data, 0x2ae5);
    set_word(&mut data, 0x45b8, 0);
    let step = advance_cat_dev(
        &mut data,
        &mut screen,
        2,
        true,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert!(step.accepted && !step.drew && !step.player_changed);
    assert_eq!(data[0x4554..0x459c], rival_records);
    assert_eq!(word(&data, 0x2ae5), random);
    let goal = record_address(&data, 6).unwrap();
    set_word(&mut data, goal, 200);
    data[goal + 3] = 120;
    set_word(&mut data, goal + 7, 0);
    set_word(&mut data, 0x45b6, 5);
    let step = advance_cat_dev(
        &mut data,
        &mut screen,
        3,
        true,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert!(step.accepted && step.success);
    assert_eq!(data[0x553], 1);
}

#[cfg(feature = "dev-tools")]
#[test]
fn absent_rivals_cannot_consume_placed_gifts_and_goal_still_can() {
    for goal_overlaps in [false, true] {
        let mut data = data();
        let mut screen = Framebuffer::default();
        screen.fill_visible(0x55);
        set_word(&mut data, 0x2e8d, 0);
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 175;
        for index in 0..7 {
            let record = record_address(&data, index).unwrap();
            set_word(
                &mut data,
                record,
                if index == 6 && !goal_overlaps {
                    264
                } else {
                    100
                },
            );
            data[record + 3] = 179;
            set_word(&mut data, record + 7, 0);
        }
        let result = place_item_dev(
            &mut data,
            &mut screen,
            10,
            true,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(result.item_changed);
        assert_eq!(data[0x2b72], if goal_overlaps { 0 } else { 1 });
        for index in 0..6 {
            let record = record_address(&data, index).unwrap();
            assert_eq!(word(&data, record + 7), 0xffff);
        }
    }
}

#[cfg(feature = "dev-tools")]
#[test]
fn development_false_wrappers_preserve_default_hearts_state_and_pixels() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    let mut alternate = data.clone();
    let mut alternate_screen = screen.clone();
    initialize(&mut data, &mut screen, 5).unwrap();
    initialize_dev(&mut alternate, &mut alternate_screen, 5, false).unwrap();
    assert_eq!(data, alternate);
    assert_eq!(screen.bytes(), alternate_screen.bytes());
    assert_eq!(
        advance_cat(
            &mut data,
            &mut screen,
            6,
            &mut crate::sound::Sound::default()
        )
        .unwrap(),
        advance_cat_dev(
            &mut alternate,
            &mut alternate_screen,
            6,
            false,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
    );
    assert_eq!(data, alternate);
    assert_eq!(screen.bytes(), alternate_screen.bytes());
    assert_eq!(
        place_item(
            &mut data,
            &mut screen,
            6,
            &mut crate::sound::Sound::default()
        )
        .unwrap(),
        place_item_dev(
            &mut alternate,
            &mut alternate_screen,
            6,
            false,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
    );
    assert_eq!(data, alternate);
    assert_eq!(screen.bytes(), alternate_screen.bytes());
}

fn data() -> Vec<u8> {
    let mut data = vec![0; DATA_LENGTH];
    set_word(&mut data, 4, 7);
    set_word(&mut data, 0x2ae5, 0xfa59);
    data[0x2bd4..0x2bdb].copy_from_slice(&[176, 152, 128, 104, 80, 56, 32]);
    data[0x2bdb..0x2be2].copy_from_slice(&[0, 18, 36, 54, 72, 90, 108]);
    set_word(&mut data, 0x2e20, 0x6100);
    set_word(&mut data, 0x2e22, 0x6120);
    set_word(&mut data, 0x2e24, 0x0101);
    set_word(&mut data, 0x2e26, 0xffff);
    data[0x6100..0x6120].fill(0x11);
    data[0x6120..0x6140].fill(0x22);
    data[0x2af0..0x2b4a].fill(0x33);
    data[0x2b7a..0x2bd4].fill(0x55);
    set_word(&mut data, 0x2e8d, 0xffff);
    set_word(&mut data, 0x2e94, 0xffff);
    for index in 0..8 {
        set_word(&mut data, 0x2b4a + index * 2, 40 + index as u16 * 24);
    }
    for row in 0..7 {
        set_word(&mut data, 0x45a8 + row * 2, 0x4554 + row as u16 * 12);
        data[0x7120 + row] = 191 - row as u8 * 24;
        set_word(&mut data, 0x45c7 + row * 2, 540);
        let record = 0x4554 + row * 12;
        data[record + 6] = 1;
        set_word(&mut data, record + 7, 100);
    }
    set_word(&mut data, 0x45d5, 140);
    for column in 0..16 {
        set_word(&mut data, 0x70fc + column * 2, 32 + column as u16 * 16);
    }
    set_word(&mut data, 0x579, 200);
    data[0x57b] = 120;
    data
}

#[test]
fn initialization_preserves_edge_cells_and_clocks_and_clamps_only_item_count() {
    let mut data = data();
    data[0x2bf2] = 99;
    data[0x2bf3] = 100;
    set_word(&mut data, 0x414, 11);
    set_word(&mut data, 0x45b8, 0x9876);
    set_word(&mut data, 0x70ee, 0x1234);
    set_word(&mut data, 0x2e8f, 0x5678);
    let mut screen = Framebuffer::default();
    initialize(&mut data, &mut screen, 0).unwrap();
    assert_eq!((data[0x2bf2], data[0x2bf3]), (99, 100));
    assert_eq!(word(&data, 0x414), 11);
    assert_eq!(&data[0x2b72..0x2b7a], &[1; 8]);
    assert_eq!(
        (
            word(&data, 0x45b8),
            word(&data, 0x70ee),
            word(&data, 0x2e8f)
        ),
        (0x9876, 0x1234, 0x5678)
    );
    assert_eq!((word(&data, 0x2e8d), word(&data, 0x2e94)), (0xffff, 0xffff));
    for index in 0..7 {
        let record = record_address(&data, index).unwrap();
        assert!((96..=223).contains(&word(&data, record)));
        assert_eq!(data[record + 3], 179 - index as u8 * 24);
        assert_eq!(word(&data, record + 7), if index == 0 { 0xffff } else { 0 });
    }
}

#[test]
fn hearts_support_uses_shifted_x_and_undrawn_edge_cells() {
    let mut data = data();
    let mut player = Player::at_alley_floor(41);
    player.y = 181;
    data[0x2be3] = 2;
    assert!(resolve_support(&data, &player).unwrap().hit.is_none());
    player.x = 40;
    let hit = resolve_support(&data, &player).unwrap().hit.unwrap();
    assert_eq!((hit.x, hit.y, hit.index), (40, 181, 0));
    player.x = 310;
    assert_eq!(
        resolve_support(&data, &player).unwrap().hit.unwrap().index,
        17
    );
    data[0x2bf3] = 2;
    assert!(resolve_support(&data, &player).unwrap().hit.is_none());
}

#[test]
fn cat_contacts_keep_lives_support_phase_and_inclusive_edges() {
    let mut data = data();
    set_word(&mut data, 0x4548, 100);
    data[0x454b] = 35;
    set_word(&mut data, 0x579, 124);
    data[0x57b] = 47;
    data[0x55c] = 9;
    data[0x577] = 73;
    data[0x1f80] = 3;
    set_word(&mut data, 0x45b6, 5);
    assert!(cat_contact(
        &mut data,
        200,
        &mut crate::sound::Sound::default()
    ));
    assert_eq!(
        (data[0x571], data[0x576], data[0x578], data[0x55b]),
        (1, 4, 8, 4)
    );
    assert_eq!(
        (data[0x55c], data[0x577], data[0x1f80], data[0x553]),
        (9, 73, 3, 0)
    );
    set_word(&mut data, 0x579, 125);
    assert!(!cat_contact(
        &mut data,
        200,
        &mut crate::sound::Sound::default()
    ));
    set_word(&mut data, 0x579, 124);
    set_word(&mut data, 0x45b6, 6);
    assert!(cat_contact(
        &mut data,
        200,
        &mut crate::sound::Sound::default()
    ));
    assert_eq!((data[0x553], data[0x1f80]), (1, 3));
}

#[test]
fn hidden_cats_commit_only_the_group_boundaries_and_do_not_save_scratch() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    let calls = [
        (101, Some(1)),
        (101, None),
        (102, Some(2)),
        (102, Some(3)),
        (102, Some(4)),
        (102, None),
        (103, Some(5)),
        (103, Some(6)),
        (103, Some(0)),
        (103, None),
    ];
    let old_records = data[0x4554..0x45a8].to_vec();
    for (tick, expected) in calls {
        let update = advance_cat(
            &mut data,
            &mut screen,
            tick,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(update.accepted, expected.is_some());
        if let Some(index) = expected {
            assert_eq!(word(&data, 0x45b6), index);
        }
        assert!(!update.drew);
    }
    assert_eq!(&data[0x4554..0x45a8], &old_records);
}

#[test]
fn pickup_is_automatic_and_recent_placement_blocks_only_first_matching_slot() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    set_word(&mut data, 0x2b5a, 100);
    data[0x2b6a] = 176;
    data[0x2b72] = 1;
    set_word(&mut data, 0x579, 100);
    data[0x57b] = 176;
    data[0x69a] = 128;
    set_word(&mut data, 0x2e94, 0);
    assert!(
        !pickup_item(
            &mut data,
            &mut screen,
            1,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
        .item_changed
    );
    set_word(&mut data, 0x2e94, 0xffff);
    assert!(
        pickup_item(
            &mut data,
            &mut screen,
            2,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
        .item_changed
    );
    assert_eq!((data[0x2b72], word(&data, 0x2e8d)), (0, 0));
    let before = data.clone();
    assert!(
        !pickup_item(
            &mut data,
            &mut screen,
            2,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
        .accepted
    );
    assert_eq!(data, before);
}

#[test]
fn blocked_placement_keeps_carried_item_but_updates_candidate_before_overlap() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    set_word(&mut data, 0x2e8d, 0);
    set_word(&mut data, 0x579, 290);
    data[0x57b] = 175;
    data[0x2b73] = 1;
    set_word(&mut data, 0x2b5c, 260);
    data[0x2b6b] = 176;
    let update = place_item(
        &mut data,
        &mut screen,
        5,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert!(update.accepted && !update.item_changed);
    assert_eq!(
        (word(&data, 0x2b5a), data[0x2b6a], word(&data, 0x2e8d)),
        (260, 176, 0)
    );
    data[0x2b73] = 0;
    assert!(
        place_item(
            &mut data,
            &mut screen,
            5,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
        .item_changed
    );
    assert_eq!(
        (data[0x2b72], word(&data, 0x2e8d), word(&data, 0x2e94)),
        (1, 0xffff, 0)
    );
}

#[test]
fn gift_consumption_hides_ordinary_cat_at_nonzero_tick_but_leaves_goal_visible() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    set_word(&mut data, 0x4548, 100);
    data[0x454b] = 179;
    data[0x454e] = 1;
    data[0x45be] = 1;
    set_word(&mut data, 0x2b5a, 100);
    data[0x2b6a] = 176;
    for (cat, hidden) in [(5, 0xffff), (6, 0)] {
        data[0x2b72] = 1;
        set_word(&mut data, 0x45b6, cat);
        assert!(consume_item(&mut data, &mut screen, 0).unwrap());
        assert_eq!((data[0x2b72], word(&data, 0x454f)), (0, hidden));
    }
}

#[test]
fn arrows_toggle_both_ways_and_preserve_cell_latch_across_repeated_calls() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    data[0x70f5] = 184;
    data[0x70f7] = 1;
    set_word(&mut data, 0x70f3, 32);
    set_word(&mut data, 0x70fa, pixel_offset(32, 184));
    set_word(&mut data, 0x70ec, 0xffff);
    assert!(toggle_platform(&mut data, &mut screen).unwrap());
    assert_eq!((data[0x2be2], word(&data, 0x70ec)), (2, 0));
    assert!(!toggle_platform(&mut data, &mut screen).unwrap());
    assert_eq!(data[0x2be2], 2);
    set_word(&mut data, 0x70ec, 0xffff);
    assert!(toggle_platform(&mut data, &mut screen).unwrap());
    assert_eq!(data[0x2be2], 0);
}

#[test]
fn arrow_motion_rejects_same_tick_and_retires_before_bottom_horizontal_move() {
    let mut data = data();
    let mut screen = Framebuffer::default();
    data[0x70f2] = 1;
    data[0x70f7] = 1;
    data[0x70f6] = 1;
    data[0x70f5] = 180;
    set_word(&mut data, 0x70f3, 32);
    set_word(&mut data, 0x70ec, 0xffff);
    for tick in 1..=5 {
        let update = advance_arrow(
            &mut data,
            &mut screen,
            tick,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(update.accepted);
        assert_eq!(
            (word(&data, 0x70f3), data[0x70f5]),
            (32 + tick * 5, 180 + tick as u8 * 2)
        );
        let old_data = data.clone();
        let old_screen = screen.bytes().to_vec();
        assert!(
            !advance_arrow(
                &mut data,
                &mut screen,
                tick,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .accepted
        );
        assert_eq!(data, old_data);
        assert_eq!(screen.bytes().as_slice(), &old_screen);
    }
    advance_arrow(
        &mut data,
        &mut screen,
        6,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(
        (word(&data, 0x70f3), data[0x70f5], data[0x70f2]),
        (57, 192, 0)
    );
}
