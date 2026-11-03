// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[cfg(feature = "dev-tools")]
#[test]
fn disabled_eels_are_erased_and_never_move_contact_or_reactivate() {
    let (mut data, mut screen) = fixture();
    place_contact(&mut data, 12);
    data[0x348f + 12] = 0;
    let dest = pixel_offset(100, 100);
    set_word(&mut data, 0x34bf + 24, dest);
    screen
        .copy_rect(dest, Dimensions(0x0202), &[0xff; 8])
        .unwrap();
    assert!(remove_eels(&mut data, &mut screen).unwrap());
    assert_eq!(
        screen.read_rect(dest, Dimensions(0x0202)).unwrap(),
        vec![0x55; 8]
    );
    let positions = data[0x3447 + 24..0x3477].to_vec();
    let directions = data[0x3417 + 12..0x342f].to_vec();
    let random = word(&data, 0x2ae5);
    set_word(&mut data, 0x3415, 11);
    let actor = advance_actors_dev(&mut data, &mut screen, 1, true).unwrap();
    assert!(actor.accepted && !actor.drew);
    assert_eq!(word(&data, 0x2ae5), random);
    place_contact(&mut data, 0);
    data[0x3410] = 1;
    let step = contacts_dev(
        &mut data,
        &mut screen,
        2,
        true,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(
        (
            step.collected,
            step.contacts,
            step.hazard_contacts,
            step.success
        ),
        (1, 1, 0, true)
    );
    assert_eq!(data[0x34b3..0x34bf], [1; 12]);
    assert_eq!(data[0x349b..0x34a7], [1; 12]);
    assert_eq!(data[0x3447 + 24..0x3477], positions);
    assert_eq!(data[0x3417 + 12..0x342f], directions);
    assert_eq!((data[0x552], data[0x1f80]), (0, 3));
}

#[cfg(feature = "dev-tools")]
#[test]
fn development_initialization_retains_fish_and_false_wrappers_match_defaults() {
    let (mut data, mut screen) = fixture();
    initialize_underwater_dev(&mut data, &mut screen, true).unwrap();
    assert_eq!((data[0x3410], &data[0x34a7..0x34b3]), (12, &[0; 12][..]));
    assert_eq!(data[0x34b3..0x34bf], [1; 12]);
    let (mut original, mut pixels) = fixture();
    let (mut alternate, mut alternate_pixels) = fixture();
    initialize_underwater(&mut original, &mut pixels).unwrap();
    initialize_underwater_dev(&mut alternate, &mut alternate_pixels, false).unwrap();
    assert_eq!(original, alternate);
    assert_eq!(pixels.bytes(), alternate_pixels.bytes());
    assert_eq!(
        advance_actors(&mut original, &mut pixels, 1).unwrap(),
        advance_actors_dev(&mut alternate, &mut alternate_pixels, 1, false).unwrap()
    );
    assert_eq!(original, alternate);
    assert_eq!(pixels.bytes(), alternate_pixels.bytes());
    assert_eq!(
        contacts(
            &mut original,
            &mut pixels,
            2,
            &mut crate::sound::Sound::default()
        )
        .unwrap(),
        contacts_dev(
            &mut alternate,
            &mut alternate_pixels,
            2,
            false,
            &mut crate::sound::Sound::default()
        )
        .unwrap()
    );
    assert_eq!(original, alternate);
    assert_eq!(pixels.bytes(), alternate_pixels.bytes());
}

fn fixture() -> (Vec<u8>, Framebuffer) {
    let mut data = vec![0; DATA_LENGTH];
    let mut screen = Framebuffer::default();
    screen.fill_visible(0x55);
    set_word(&mut data, 0x2ae5, 0xfa59);
    set_word(&mut data, 0x579, 100);
    data[0x57b] = 100;
    data[0x1f80] = 3;
    data[0x34a7..0x34bf].fill(1);
    data[0x348f..0x34a7].fill(1);
    data[0x3417..0x3447].fill(1);
    data[0x3410] = 12;
    for slot in 0..24 {
        data[0x34f1 + slot] = 8 + slot as u8 * 6;
        data[0x3477 + slot] = data[0x34f1 + slot];
        set_word(&mut data, 0x3447 + slot * 2, 40);
    }
    for phase in 0..4 {
        data[0x2020 + phase * 8..0x2028 + phase * 8].fill(0x11 * (phase as u8 + 1));
        data[0x3300 + phase * 12..0x330c + phase * 12].fill(0x11 * (phase as u8 + 1));
        data[0x3330 + phase * 8..0x3338 + phase * 8].fill(0x11 * (phase as u8 + 1));
        let source = 0x3530 + phase * 40;
        set_word(&mut data, 0x35d0 + phase * 2, source as u16);
        data[source..source + 40].fill(0x11 * (phase as u8 + 1));
    }
    data[0x3404..0x3410].fill(0x55);
    set_word(&mut data, 0x3513, 8);
    set_word(&mut data, 0x3515, 16);
    set_word(&mut data, 0x3517, 6);
    set_word(&mut data, 0x3519, 2);
    data[0x351c..0x3524].fill(2);
    (data, screen)
}

#[test]
fn water_draw_uses_two_colors_and_rejects_repeated_surface_phases() {
    let (mut data, mut screen) = fixture();
    draw_underwater(&mut data, &mut screen).unwrap();
    assert_eq!(word(&data, 0x2654), 40);
    assert!(
        data[0x2656..0x267e]
            .windows(2)
            .all(|pair| pair[0] != pair[1])
    );
    assert!(data[0x2656..0x267e].iter().all(|phase| phase & !0x18 == 0));
    assert_eq!(
        screen.read_rect(0, Dimensions(0x0428)).unwrap(),
        vec![0xaa; 320]
    );
    assert_eq!(
        screen
            .read_rect(pixel_offset(0, 8), Dimensions(0x0128))
            .unwrap(),
        vec![0x55; 80]
    );
    assert_eq!((data[0x2ae0], data[0x2ae2]), (1, 0));
}

#[test]
fn actor_initialization_preserves_clocks_hazard_phase_and_screen() {
    let (mut data, mut screen) = fixture();
    set_word(&mut data, 0x3413, 123);
    set_word(&mut data, 0x3509, 456);
    set_word(&mut data, 0x350d, 17);
    set_word(&mut data, 0x350f, 789);
    let before = screen.bytes().to_vec();
    initialize_underwater(&mut data, &mut screen).unwrap();
    assert_eq!(
        (
            word(&data, 0x3413),
            word(&data, 0x3509),
            word(&data, 0x350d),
            word(&data, 0x350f)
        ),
        (123, 456, 17, 789)
    );
    assert_eq!(
        (data[0x3410], word(&data, 0x3411), word(&data, 0x3415)),
        (12, 0, 0)
    );
    assert_eq!(data[0x34a7..0x34b3], [0; 12]);
    assert_eq!(
        data[0x34b3..0x34bf]
            .iter()
            .filter(|&&disabled| disabled != 0)
            .count(),
        2
    );
    for slot in 0..24 {
        assert_eq!(
            (
                data[0x348f + slot],
                data[0x3477 + slot],
                data[0x342f + slot]
            ),
            (1, data[0x34f1 + slot], 1)
        );
        assert!(matches!(data[0x3417 + slot], 1 | 255));
        assert!(word(&data, 0x3447 + slot * 2) <= 255);
    }
    assert_eq!(screen.bytes().as_slice(), before);
}

#[test]
fn bowl_gate_includes_contact_and_uses_inclusive_edges() {
    let (data, screen) = fixture();
    for (x, y, entered) in [
        (204, 138, true),
        (244, 138, true),
        (203, 138, false),
        (245, 138, false),
        (228, 124, true),
        (228, 148, true),
        (228, 123, false),
        (228, 149, false),
    ] {
        let mut data = data.clone();
        let mut screen = screen.clone();
        set_word(&mut data, 0x579, x);
        data[0x57b] = y;
        set_word(&mut data, 0x35da, 0xfffc);
        assert!(!advance_bowl(&mut data, &mut screen, 1).unwrap().accepted);
        let step = advance_bowl(&mut data, &mut screen, 2).unwrap();
        assert!(step.accepted && step.drew);
        assert_eq!(step.entered, entered);
        assert_eq!(data[0x554] != 0, entered);
        assert_eq!(
            screen.read_rect(0x15c9, Dimensions(0x0a02)).unwrap(),
            vec![0x22; 40]
        );
    }
}

#[test]
fn actor_groups_preserve_pcjr_surface_midpoint_exception() {
    for (model, y, group) in [(0xff, 4, 12), (0xfd, 4, 24), (0xfd, 48, 12)] {
        let (mut data, mut screen) = fixture();
        data[0x697] = model;
        data[0x57b] = y;
        for cursor in 1..=group {
            let step = advance_actors(&mut data, &mut screen, 1).unwrap();
            assert!(step.accepted && !step.drew);
            assert_eq!(word(&data, 0x3415), cursor % 24);
        }
        let before = data.clone();
        assert!(!advance_actors(&mut data, &mut screen, 1).unwrap().accepted);
        assert_eq!(data, before);
        assert_eq!(word(&data, 0x3411), if group == 24 { 12 } else { 0 });
        assert_eq!(word(&data, 0x3413), if group == 24 { 8 } else { 0 });
    }
}

#[test]
fn actor_clamps_reverses_and_restores_uniform_water() {
    let (mut data, mut screen) = fixture();
    let slot = 1;
    data[0x34a7 + slot] = 0;
    data[0x348f + slot] = 0;
    set_word(&mut data, 0x3447 + slot * 2, 300);
    data[0x3477 + slot] = data[0x34f1 + slot] + 24;
    let old = pixel_offset(20, 20);
    set_word(&mut data, 0x34bf + slot * 2, old);
    screen
        .copy_rect(old, Dimensions(0x0601), &[0xff; 12])
        .unwrap();
    let step = advance_actors(&mut data, &mut screen, 1).unwrap();
    assert!(step.accepted && step.drew);
    assert_eq!(
        (
            word(&data, 0x3447 + slot * 2),
            data[0x3417 + slot],
            data[0x3477 + slot],
            data[0x342f + slot]
        ),
        (302, 255, data[0x34f1 + slot] + 24, 255)
    );
    assert_eq!(
        screen.read_rect(old, Dimensions(0x0601)).unwrap(),
        vec![0x55; 12]
    );
    assert_eq!(
        screen
            .read_rect(word(&data, 0x34bf + slot * 2), Dimensions(0x0601))
            .unwrap(),
        vec![0x33; 12]
    );
    set_word(&mut data, 0x3415, 0);
    set_word(&mut data, 0x2ae5, 0xfa59);
    set_word(&mut data, 0x3447 + slot * 2, 2);
    data[0x3477 + slot] = data[0x34f1 + slot];
    advance_actors(&mut data, &mut screen, 2).unwrap();
    assert_eq!(
        (
            word(&data, 0x3447 + slot * 2),
            data[0x3417 + slot],
            data[0x3477 + slot],
            data[0x342f + slot]
        ),
        (0, 1, data[0x34f1 + slot], 1)
    );
}

#[test]
fn actor_random_reroll_consumes_second_draw_only_below_threshold() {
    let (mut data, mut screen) = fixture();
    data[0x34a8] = 0;
    data[0x3478] = data[0x34f2] + 12;
    set_word(&mut data, 0x2ae5, 32);
    let mut reference = data.clone();
    let horizontal = next_random(&mut reference) as u8;
    let vertical = next_random(&mut reference) as u8;
    assert_eq!(horizontal, 16);
    advance_actors(&mut data, &mut screen, 1).unwrap();
    assert_eq!(word(&data, 0x2ae5), word(&reference, 0x2ae5));
    assert_eq!(data[0x3418], if horizontal & 1 != 0 { 1 } else { 255 });
    assert_eq!(data[0x3430], if vertical & 1 != 0 { 1 } else { 255 });
}

#[test]
fn surface_groups_commit_at_wrap_and_skip_near_player_without_advancing_tile() {
    let (mut data, mut screen) = fixture();
    data[0x57b] = 4;
    set_word(&mut data, 0x579, 0);
    assert!(!advance_surface(&mut data, &mut screen, 7).unwrap().accepted);
    let step = advance_surface(&mut data, &mut screen, 8).unwrap();
    assert!(step.accepted && !step.drew);
    assert_eq!((word(&data, 0x350d), data[0x2657]), (1, 0));
    data[0x57b] = 8;
    for cursor in 2..=40 {
        let step = advance_surface(&mut data, &mut screen, 8).unwrap();
        assert!(step.accepted && step.drew);
        assert_eq!(word(&data, 0x350d), cursor % 40);
    }
    assert_eq!(word(&data, 0x350f), 8);
    let before = data.clone();
    assert!(
        !advance_surface(&mut data, &mut screen, 15)
            .unwrap()
            .accepted
    );
    assert_eq!(data, before);
}

fn place_contact(data: &mut [u8], slot: usize) {
    data[0x34a7 + slot] = 0;
    data[0x348f + slot] = 1;
    set_word(data, 0x3447 + slot * 2, 100);
    data[0x3477 + slot] = 100;
}

#[test]
fn contact_requests_count_native_enablement_and_remain_nonblocking() {
    for enabled in [0, 1] {
        let (mut data, mut screen) = fixture();
        data[0] = enabled;
        data[0x3410] = 12;
        for slot in [0, 1, 12] {
            place_contact(&mut data, slot);
        }
        let mut sound = crate::sound::Sound::default();
        sound.record(true);
        let step = contacts_with_damage(&mut data, &mut screen, 7, false, &mut sound).unwrap();
        assert!(step.failure);
        assert_eq!((step.contacts, step.collected), (2, 2));
        assert_eq!(step.audible_requests, 2 * enabled);
        assert_eq!(sound.take_busy_ticks(), 0);
        assert_eq!(
            sound.drain().filter(|write| write.port == 0x43).count(),
            usize::from(2 * enabled)
        );
        assert_eq!((data[0x3410], data[0x1f80]), (10, 3));
    }
}

#[test]
fn final_fish_protects_later_eel_and_reintroduces_highest_disabled_hazards() {
    let (mut data, mut screen) = fixture();
    place_contact(&mut data, 0);
    place_contact(&mut data, 12);
    data[0x3410] = 1;
    let step = contacts(
        &mut data,
        &mut screen,
        1,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(
        (
            step.contacts,
            step.collected,
            step.hazard_contacts,
            step.success
        ),
        (2, 1, 1, true)
    );
    assert_eq!(
        (data[0x3410], data[0x553], data[0x552], data[0x1f80]),
        (0, 1, 0, 3)
    );
    assert_eq!(
        (
            data[0x34a7],
            data[0x34b3],
            data[0x34bd],
            data[0x34be],
            data[0x351b]
        ),
        (1, 1, 0, 0, 0)
    );
    for slot in [22, 23] {
        assert_eq!(
            (word(&data, 0x3447 + slot * 2), data[0x3417 + slot]),
            (302, 255)
        );
    }
    assert_eq!(word(&data, 0x3511), 24);
}

#[test]
fn ordinary_eel_contact_is_nonlethal_and_timeout_suppresses_completion() {
    let (mut data, mut screen) = fixture();
    place_contact(&mut data, 12);
    let before = screen.bytes().to_vec();
    let step = contacts_with_damage(
        &mut data,
        &mut screen,
        1,
        true,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(
        (
            step.contacts,
            step.hazard_contacts,
            step.success,
            data[0x34b3],
            data[0x552],
            data[0x1f80]
        ),
        (0, 1, false, 0, 0, 3)
    );
    assert_eq!(screen.bytes().as_slice(), before);
    place_contact(&mut data, 0);
    data[0x3410] = 1;
    data[0x5f3] = 1;
    set_word(&mut data, 0x579, 170);
    set_word(&mut data, 0x3447, 170);
    set_word(&mut data, 0x345f, 170);
    let step = contacts(
        &mut data,
        &mut screen,
        2,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(
        (
            step.contacts,
            step.collected,
            step.success,
            data[0x3410],
            data[0x553],
            data[0x552]
        ),
        (2, 1, false, 0, 0, 0)
    );
    assert_eq!((word(&data, 0x3473), data[0x342d]), (0, 1));
}

#[test]
fn fish_and_eel_contact_tables_use_inclusive_player_edges() {
    for (slot, right, bottom) in [(0, 108, 106), (12, 116, 102)] {
        let (data, screen) = fixture();
        for (x, y, hit) in [
            (76, 100, true),
            (right, 100, true),
            (75, 100, false),
            (right + 1, 100, false),
            (100, 86, true),
            (100, bottom, true),
            (100, 85, false),
            (100, bottom + 1, false),
        ] {
            let mut data = data.clone();
            let mut screen = screen.clone();
            place_contact(&mut data, slot);
            data[0x553] = 1;
            set_word(&mut data, 0x579, x);
            data[0x57b] = y;
            assert_eq!(
                contacts(
                    &mut data,
                    &mut screen,
                    1,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .contacts
                    != 0,
                hit
            );
        }
    }
}

#[test]
fn eel_harm_stops_scan_sets_failure_and_retains_lives_and_active_eel() {
    let (mut data, mut screen) = fixture();
    place_contact(&mut data, 12);
    place_contact(&mut data, 13);
    data[0x3350..0x3404].fill(0x33);
    let step = contacts(
        &mut data,
        &mut screen,
        123,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert_eq!(step.visited, 13);
    assert_eq!(
        (
            step.failure,
            step.wait_ticks,
            step.hazard_contacts,
            step.contacts
        ),
        (true, 13, 1, 0)
    );
    assert_eq!(
        (
            data[0x552],
            data[0x1f80],
            data[0x34b3],
            word(&data, 0x3511),
            word(&data, 0x3509)
        ),
        (1, 3, 0, 12, 123)
    );
    assert_eq!(
        screen
            .read_rect(
                pixel_offset(word(&data, 0x579).saturating_sub(8), data[0x57b]),
                Dimensions(0x1205)
            )
            .unwrap(),
        [0x33; 180]
    );
}
