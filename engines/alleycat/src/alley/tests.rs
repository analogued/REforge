// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn fence_fixture() -> Alley {
    let mut alley = Alley::synthetic();
    alley.player.x = 160;
    alley.player.y = 96;
    alley.player.vertical_coordinate = 146;
    set_word(&mut alley.data, 0x556, 40);
    for offset in (0x0fb6..0x0fc2).step_by(2) {
        set_word(&mut alley.data, offset, 0x0c03);
    }
    alley.player.write_data(&mut alley.data).unwrap();
    set_word(&mut alley.data, 0x567, 0x0c03);
    alley.player = Player::from_data(&alley.data).unwrap();
    alley
}

#[test]
fn fence_entry_starts_timer_for_grounded_and_airborne_support() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        for airborne in [false, true] {
            let mut alley = fence_fixture();
            alley.machine = machine;
            alley.player.vertical_direction = u8::from(airborne);
            let update = alley
                .advance_player_update_at_tick(Controls::default(), 1000)
                .unwrap();
            assert!(update.fence_entered);
            assert_eq!(alley.player.band_state, 1);
            assert_eq!(word(&alley.data, 0x556), 1000);

            let update = alley
                .advance_player_update_at_tick(Controls::default(), 1010)
                .unwrap();
            assert!(!update.fence_entered);
            assert_eq!(word(&alley.data, 0x556), 1000);

            // Drop from the fence, then land again later.
            alley
                .advance_player_update_at_tick(Controls { x: 0, y: 1 }, 1020)
                .unwrap();
            assert_eq!(alley.player.band_state, 0);
            assert_eq!(word(&alley.data, 0x556), 1000);
            alley.player.y = 96;
            alley.player.vertical_coordinate = 146;
            alley.player.write_data(&mut alley.data).unwrap();
            alley.data[0x55b] = 0;
            alley.player = Player::from_data(&alley.data).unwrap();
            let update = alley
                .advance_player_update_at_tick(Controls::default(), 1100)
                .unwrap();
            assert!(update.fence_entered);
            assert_eq!(word(&alley.data, 0x556), 1100);
        }
    }
}

#[test]
fn fence_entry_timer_survives_an_immediate_jump_or_drop() {
    for (direction, band) in [(-1, 2), (1, 0)] {
        let mut alley = fence_fixture();
        let update = alley
            .advance_player_update_at_tick(Controls { x: 0, y: direction }, 1000)
            .unwrap();
        assert!(update.fence_entered);
        assert_eq!(alley.player.band_state, band);
        assert_eq!(word(&alley.data, 0x556), 1000);
    }
}

#[test]
fn rejected_fence_support_does_not_restart_timer() {
    let mut alley = fence_fixture();
    alley.player.band_state = 2;
    let update = alley
        .advance_player_update_at_tick(Controls::default(), 1000)
        .unwrap();
    assert!(!update.fence_entered);
    assert_eq!(word(&alley.data, 0x556), 40);
}

#[test]
fn targeted_windows_wait_72_ticks_after_fence_entry_including_tick_wrap() {
    for entry in [1000u16, 65520] {
        let mut alley = fence_fixture();
        alley
            .advance_player_update_at_tick(Controls::default(), entry)
            .unwrap();
        for column in 0..4 {
            set_word(
                &mut alley.data,
                0x1658 + column * 2,
                24 + column as u16 * 80,
            );
        }
        alley.data[0x1660..0x1664].copy_from_slice(&[24, 56, 88, 24]);
        for elapsed in [0, 1, 71, 72, 73] {
            alley.data[0x1665] = 0;
            alley.data[0x166a] = 1;
            set_word(&mut alley.data, 0x2ae5, 0x10);
            alley
                .advance_windows(entry.wrapping_add(elapsed), false)
                .unwrap();
            assert_eq!(alley.data[0x1677], u8::from(elapsed >= 72));
            assert_eq!(alley.data[0x1669], if elapsed >= 72 { 0 } else { 8 });
            assert_eq!(word(&alley.data, 0x556), entry);
        }
    }
}

#[test]
fn last_life_handoff_does_not_charge_an_unexecuted_alley_setup() {
    let mut alley = Alley::synthetic();
    set_word(&mut alley.data, 4, 5);
    set_word(&mut alley.data, 0x1c26, 0x3000);
    alley.data[0x552] = 1;
    alley.data[0x1f80] = 1;
    alley.player.x = 160;
    alley.player.y = 96;
    alley.finish_scene(0).unwrap();
    let mut runtime = crate::runtime::Runtime::new(Machine::Cga);
    let mut complete = false;
    for _ in 0..30000 {
        let pass = runtime
            .advance_pass(&mut alley, Controls::default())
            .unwrap();
        if alley.game_over() {
            assert!(pass.scene_changed);
            assert!(pass.end_nanos - pass.start_nanos < 60_000_000);
            complete = true;
            break;
        }
    }
    assert!(complete);
    assert_eq!(alley.scene(), 0);
    assert_eq!(word(&alley.data, 6), 5);
}

#[test]
fn entry_wipes_keep_the_visible_layers_and_stop_sound_after_completed_drawing() {
    let mut alley = Alley::synthetic();
    alley.data[0] = 0xff;
    alley.data[0x1f80] = 3;
    alley.player.x = 160;
    alley.player.y = 96;
    alley.screen.fill_visible(0x55);
    alley.presented.fill_visible(0xff);
    alley.begin_entry(7).unwrap();
    assert_eq!(alley.screen.bytes(), alley.presented.bytes());
    alley.sound.record(true);
    let mut runtime = crate::runtime::Runtime::new(Machine::Cga);
    for _ in 0..15 {
        let pass = runtime
            .advance_pass(&mut alley, Controls { x: 1, y: -1 })
            .unwrap();
        assert!(pass.room_wait && !pass.control_polled && !pass.scene_changed);
    }
    assert_eq!(alley.scene(), 0);
    assert_eq!(alley.player.x, 160);
    assert_eq!(
        alley
            .entry_transition
            .as_ref()
            .unwrap()
            .palette_destination(),
        Some(7)
    );
    let events: Vec<_> = alley.sound.drain().collect();
    assert_eq!(events.iter().filter(|w| w.port == 0x43).count(), 15);
    let stop = events.last().unwrap();
    assert_eq!((stop.port, stop.value & 3), (0x61, 0));
    assert_eq!(stop.tick, runtime.clock.elapsed_pit_ticks() as u64);
    assert!(stop.tick > events[events.len() - 2].tick);
    assert!(runtime.clock.elapsed_nanos().abs_diff(404_250_000) < 1_000_000);
}

#[test]
fn entry_and_failure_palettes_switch_only_after_the_black_wipe() {
    for entry in [false, true] {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 4, 3);
        alley.player.x = 160;
        alley.player.y = 96;
        alley.data[0x1856] = 0;
        alley.data[0x185a] = 1;
        alley.presented.fill_visible(0x55);
        if entry {
            alley.begin_entry(7).unwrap();
        } else {
            alley.failure = Some(crate::failure::Failure::new(160, 96, false));
        }
        assert_eq!(&alley.rgba()[..4], &[0, 170, 0, 255]);
        for _ in 0..15 {
            alley.advance_presentation(0, false).unwrap();
        }
        // Inspect colour mapping independently of the now-black image.
        alley.presented.fill_visible(0x55);
        assert_eq!(&alley.rgba()[..4], &alley.machine.colors()[1]);
    }
}

#[test]
fn transition_work_is_consumed_once_and_restart_discards_pending_work() {
    let mut alley = Alley::synthetic();
    let work = crate::windows::SelectionWork {
        attempts: 2,
        extra_draws: 1,
        history_rejections: 1,
    };
    alley.selection_work = Some(work);
    alley.begin_entry(7).unwrap();
    assert_eq!(alley.take_selection_work().unwrap().attempts, 2);
    assert!(alley.take_selection_work().is_none());
    alley.selection_work = Some(work);
    set_word(&mut alley.data, 0x28a2, 0xffff);
    set_word(&mut alley.data, 0x5908, 0x7100);
    set_word(&mut alley.data, 0x590e, 0x7100);
    set_word(&mut alley.data, 0x7100, 0xffff);
    alley.reset_run(0, true).unwrap();
    assert!(alley.take_selection_work().is_none());
    assert!(alley.entry_transition.is_none());
}

#[test]
fn final_entry_idle_keeps_the_previous_image_offset_but_walking_moves_it() {
    for (x, direction) in [(0, 1), (304, 255)] {
        for walking in [false, true] {
            let mut alley = Alley::synthetic();
            alley.screen.fill_visible(0xff);
            alley.player = Player::at_alley_floor(x);
            alley.player.x = x;
            alley.player.entry_state = 1;
            alley.player.entry_delay = 0;
            alley.player.horizontal_direction = direction;
            alley.player.write_data(&mut alley.data).unwrap();
            alley.data[0x56f] = direction;
            set_word(&mut alley.data, 0x561, 0x0b02);
            alley.player = Player::from_data(&alley.data).unwrap();
            for index in 0..12 {
                set_word(&mut alley.data, 0xf7a + index * 2, 0x3000);
            }
            for index in 0..8 {
                set_word(&mut alley.data, 0xf92 + index * 2, 0x3000);
            }
            for index in 0..4 {
                set_word(&mut alley.data, 0xfa2 + index * 2, 0x3000);
            }
            alley
                .compositor
                .draw(
                    &mut alley.screen,
                    pixel_offset(x, 180),
                    Dimensions(0x0b02),
                    &[0xff; 44],
                )
                .unwrap();
            alley
                .advance_player_update(Controls {
                    x: if walking { direction as i8 } else { 0 },
                    y: 0,
                })
                .unwrap();
            let next_x = if direction == 1 {
                if walking { 16 } else { 8 }
            } else if walking {
                288
            } else {
                296
            };
            assert_eq!((alley.player.x, alley.player.entry_state), (next_x, 0));
            let dimensions = Dimensions(if walking { 0x0b03 } else { 0x0c02 });
            let mut expected = Framebuffer::default();
            expected.fill_visible(0xff);
            expected
                .copy_rect(
                    pixel_offset(if walking { next_x } else { x }, 180),
                    dimensions,
                    &vec![0; dimensions.len()],
                )
                .unwrap();
            assert_eq!(alley.screen.bytes(), expected.bytes());
        }
    }
}

#[test]
fn eel_deadline_waits_for_retrace_extra_pulses_and_palette() {
    let mut alley = Alley::synthetic();
    let since = u16::MAX - 5;
    let expired = since.wrapping_add(13);
    set_word(&mut alley.data, 4, 2);
    alley.sound.eel_start(&mut alley.data);
    alley.damage_effect = Some(DamageEffect {
        since,
        duration: 13,
        kind: EffectKind::Eel {
            phase: EelPhase::Poll,
            palette_tick: since,
            background: 0,
        },
    });
    // The native tick test is outside the retrace loop, including at wrap.
    assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
    assert!(!alley.advance_damage_effect(expired, 0, true).unwrap());
    assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
    assert!(!alley.advance_damage_effect(expired, 0, false).unwrap());
    assert_eq!(alley.data[0x5acf], 4);
    assert!(matches!(alley.damage_poll(), DamagePoll::EelRestart));
    // Palette parity uses the saved outer-loop BIOS sample, not the
    // newer odd tick supplied while finishing the sequence.
    assert_eq!(&alley.rgba()[..4], &hardware_color(15));
    assert!(alley.advance_damage_effect(expired, 0, false).unwrap());
    assert!(!alley.damage_waiting());
}
use crate::assets::word;

fn initial_title_data() -> Vec<u8> {
    let mut data = Alley::synthetic().data;
    set_word(&mut data, 0x28a2, 0xffff);
    set_word(&mut data, 0x5908, 0x7100);
    set_word(&mut data, 0x590e, 0x7100);
    set_word(&mut data, 0x7100, 0xffff);
    set_word(&mut data, 0x56da, 846);
    data[0x538c] = 0x66;
    data
}

fn initial_title_fixture(machine: Machine, watched_ticks: u16) -> crate::splash::Splash {
    let mut splash = crate::splash::Splash::new(&initial_title_data(), machine).unwrap();
    for tick in 0..=watched_ticks {
        splash.advance(tick, true, tick == watched_ticks).unwrap();
    }
    assert!(splash.finished());
    splash
}

#[test]
fn initial_title_state_reaches_first_game_without_reseeding() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut runs = Vec::new();
        for watched in [20, 120, 853] {
            let mut alley = Alley::synthetic();
            alley.machine = machine;
            let splash = initial_title_fixture(machine, watched);
            alley.inherit_initial_splash(&splash).unwrap();
            assert_eq!(alley.data, splash.data());
            assert_eq!(alley.data[0x69a], 16);
            assert_eq!(word(&alley.data, 0x41d), 0xffff);
            assert_eq!(word(&alley.data, 0x41f), 0xffff);
            // Changes made in the menu after the handoff must survive.
            alley.data[0] = 0;
            alley.start_initial_game(3, 42).unwrap();
            assert_eq!(alley.data[0], 0);
            assert_eq!(alley.data[0x41a], 1);
            assert_eq!(alley.data[0x1f80], 3);
            assert_eq!(word(&alley.data, 8), 3);
            assert_eq!(word(&alley.data, 0x6df8), 3);
            assert_eq!(word(&alley.data, 0x412), 42);
            assert_eq!(alley.initial_seed, 0xfa59);
            assert!(alley.start_initial_game(3, 42).is_err());
            runs.push(alley);
        }
        assert_ne!(word(&runs[0].data, 0x2ae5), word(&runs[1].data, 0x2ae5));
        assert_ne!(runs[0].framebuffer().bytes(), runs[1].framebuffer().bytes());
        // Explicit restarts keep the configured seed, independent of how
        // long the initial title played. Automatic game-over is separate.
        for run in &mut runs {
            run.restart(0).unwrap();
        }
        assert_eq!(word(&runs[0].data, 0x2ae5), word(&runs[1].data, 0x2ae5));
        assert_eq!(runs[0].framebuffer().bytes(), runs[1].framebuffer().bytes());
    }
}

#[test]
fn initial_title_handoff_rejects_unfinished_wrong_machine_and_repeated_titles() {
    let mut alley = Alley::synthetic();
    let completed = initial_title_fixture(Machine::Cga, 20);
    let unfinished = crate::splash::Splash::new(completed.data(), Machine::Cga).unwrap();
    let wrong_machine = initial_title_fixture(Machine::Pcjr, 20);
    let mut repeated =
        crate::splash::Splash::after_game_over(completed.data(), Machine::Cga, 0).unwrap();
    repeated.advance(1, false, true).unwrap();
    let before = alley.data.clone();
    for splash in [&unfinished, &wrong_machine, &repeated] {
        assert!(alley.inherit_initial_splash(splash).is_err());
        assert_eq!(alley.data, before);
    }
    assert!(alley.start_initial_game(8, 0).is_err());
    assert_eq!(alley.data, before);
}

#[test]
fn later_title_promotes_score_and_resumes_without_reseeding_or_clearing_history() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut alley = Alley::synthetic();
        alley.machine = machine;
        alley.initial_difficulty = 3;
        set_word(&mut alley.data, 0x28a2, 0xffff);
        set_word(&mut alley.data, 0x590e, 0x7100);
        set_word(&mut alley.data, 0x7100, 0xffff);
        set_word(&mut alley.data, 0x6df8, 3);
        set_word(&mut alley.data, 8, 7);
        set_word(&mut alley.data, 0x2ae5, 0xbeef);
        set_word(&mut alley.data, 0x41d, 6);
        set_word(&mut alley.data, 0x41f, 3);
        alley.data[0x1f82] = 2;
        alley.data[0x1f89] = 1;
        alley.data[0x1f80] = 0;
        assert!(alley.game_over_splash().is_err());
        alley.screen.fill_visible(0x55);
        alley.finish_run();
        assert!(
            alley.framebuffer().bytes()[..8000]
                .iter()
                .all(|&byte| byte == 0x55)
        );
        let mut splash = alley.game_over_splash().unwrap();
        assert_eq!(splash.data()[0x1f82], 2);
        assert_eq!(splash.data()[0x1f89], 2);
        assert_eq!(splash.data()[0x1f80], 9);
        assert_eq!(word(splash.data(), 0x6df8), 3);
        assert!(alley.start_after_game_over(&splash).is_err());
        splash.advance(1, false, true).unwrap();
        let mut expected_data = alley.data.clone();
        set_word(&mut expected_data, 8, 3);
        set_word(&mut expected_data, 0x2ae5, word(splash.data(), 0x2ae5));
        let mut expected_frame = Framebuffer::default();
        initialize_alley(&mut expected_data, &mut expected_frame).unwrap();
        draw_ground_objects(&mut expected_data, &mut expected_frame).unwrap();
        alley.start_after_game_over(&splash).unwrap();
        assert!(!alley.game_over());
        assert_eq!(word(&alley.data, 8), 3);
        assert_eq!(alley.data[0x1f80], 3);
        assert_eq!(alley.data[0x1f82..0x1f89], [0; 7]);
        assert_eq!(alley.data[0x1f89], 2);
        assert_eq!((word(&alley.data, 0x41d), word(&alley.data, 0x41f)), (6, 3));
        assert_eq!(word(&alley.data, 0x2ae5), word(&expected_data, 0x2ae5));
        assert_eq!(
            (alley.player.entry_state, alley.player.entry_delay),
            (3, 12)
        );
    }
}

#[test]
fn room_floor_mark_work_reports_only_a_newly_drawn_tile() {
    for (last, count, expected) in [(18, 0, true), (17, 2, false), (18, 4, false)] {
        let mut alley = Alley::synthetic();
        set_word(&mut alley.data, 4, 3);
        set_word(&mut alley.data, 0x32b6, last);
        alley.data[0x329f] = count;
        alley.data[0x32c2..0x32cc].fill(0x55);
        for frame in 0..6 {
            set_word(&mut alley.data, 0xf7a + frame * 2, 0x2000);
        }
        alley.data[0x2000..0x2042].fill(0xff);
        alley.player = Player::at_alley_floor(125);
        let outcome = alley
            .advance_player_update_at_tick(Controls { x: 1, y: 0 }, 10)
            .unwrap();
        assert_eq!(outcome.floor_mark, expected);
        assert_eq!(word(&alley.data, 0x32b6), 17);
        assert_eq!(alley.data[0x329f], count + u8::from(expected));
        assert_eq!(
            alley.screen.bytes()[0x1e22],
            if expected { 0x55 } else { 0 }
        );
        assert_eq!(alley.player.x, 127);
    }
}

#[test]
fn menu_selection_sets_starting_skill_and_preserves_high_score_on_restart() {
    let mut alley = Alley::synthetic();
    set_word(&mut alley.data, 0x28a2, 0xffff);
    set_word(&mut alley.data, 0x5908, 0x7100);
    set_word(&mut alley.data, 0x590e, 0x7100);
    set_word(&mut alley.data, 0x7100, 0xffff);
    alley.data[0x1f82] = 2;
    alley.data[0x1f89] = 1;
    alley.mouse_effect = Some(Image {
        source: 0,
        dimensions: 0x0801,
        offset: 0,
        blend: Blend::And,
    });
    alley.start_new_game(3, 42).unwrap();
    assert!(alley.mouse_effect.is_none());
    assert_eq!(word(&alley.data, 8), 3);
    assert_eq!(word(&alley.data, 0x6df8), 3);
    assert_eq!(alley.data[0x1f80], 3);
    assert_eq!(alley.data[0x1f82..0x1f89], [0; 7]);
    assert_eq!(alley.data[0x1f89], 2);
    set_word(&mut alley.data, 8, 7);
    alley.restart(0).unwrap();
    assert_eq!(word(&alley.data, 8), 3);
    assert_eq!(word(&alley.data, 0x6df8), 3);
    assert!(alley.start_new_game(8, 0).is_err());
    assert_eq!(alley.initial_difficulty, 3);
}

#[test]
fn alley_reconstruction_discards_the_previous_bin_cat_image() {
    for restart in [false, true] {
        for (x, y, height) in [(24, 146, 1), (24, 130, 13), (100, 138, 9)] {
            let mut alley = Alley::synthetic();
            // Minimal terminated scenery/decoration lists, with authored pixels.
            set_word(&mut alley.data, 0x28a2, 0xffff);
            set_word(&mut alley.data, 0x5908, 0x7100);
            set_word(&mut alley.data, 0x7100, 0xffff);
            alley.data[0x1cf0..0x1d24].fill(0x55);
            alley.data[0x1d59] = 13;
            set_word(&mut alley.data, 0x1d5a, 400);
            let dimensions = Dimensions((height << 8) | 2);
            alley.bin_image = Some(crate::bin_cat::VisibleSprite {
                source: 0x1cf0,
                dimensions,
                offset: pixel_offset(x, y),
            });
            let previous_bin = alley.data[0x1028];
            if restart {
                alley.data[0x1f80] = 0;
                alley.finish_run();
                alley.restart(0).unwrap();
            } else {
                alley.return_to_alley().unwrap();
            }
            assert_eq!(alley.data[0x1d59], 0);
            assert_eq!(word(&alley.data, 0x1d5a), 400);
            assert_eq!(alley.data[0x1028], previous_bin);
            // The inactive cat must contribute no pixels, even while the
            // retrace/tick gates delay its next animation opportunity.
            assert!(
                alley.framebuffer().bytes() == alley.screen.bytes(),
                "inactive bin cat added pixels after alley reconstruction"
            );
            assert!(alley.bin_image.is_none());
            alley.advance_bin_cat(401, false).unwrap();
            alley.advance_bin_cat(400, true).unwrap();
            alley.compose_actors().unwrap();
            assert!(
                alley.framebuffer().bytes() == alley.screen.bytes(),
                "inactive bin cat added pixels after alley reconstruction"
            );
        }
    }
}

fn bin_animation_fixture(machine: Machine) -> Alley {
    let mut alley = Alley::synthetic();
    alley.machine = machine;
    alley.player = Player::at_alley_floor(8);
    alley.screen.fill_visible(0xaa);
    alley.data[0x1cf0..0x1d24].fill(0x55);
    alley.data[0x1d59] = 27;
    set_word(&mut alley.data, 0x1d5a, u16::MAX);
    set_word(&mut alley.data, 0x1d5c, 200);
    alley.data[0x1d5e] = 147;
    alley.data[0x1d5f] = 159;
    set_word(&mut alley.data, 0x1d64, 2);
    alley
}

#[test]
fn active_bin_uses_each_new_tick_without_retrace_and_retains_all_frames() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut alley = bin_animation_fixture(machine);
        let initial_random = word(&alley.data, 0x2ae5);
        for tick in 0..27u16 {
            let work = alley.advance_bin_cat(tick, false).unwrap().unwrap();
            let count = 26 - tick;
            let height = count.min(27 - count);
            assert!(!work.started);
            assert_eq!(work.draw_rows, height as u8);
            assert_eq!(alley.data[0x1d59], count as u8);
            assert_eq!(alley.data[0x1d5f], 159 - height as u8);
            assert_eq!(alley.bin_image.is_some(), count != 0);
            if let Some(image) = alley.bin_image {
                assert_eq!(image.dimensions.rows(), usize::from(height));
                assert_eq!(image.offset, pixel_offset(200, 159 - height as u8));
            }
            let before = alley.data.clone();
            let frame = alley.framebuffer().clone();
            for retrace in [false, true] {
                assert!(alley.advance_bin_cat(tick, retrace).unwrap().is_none());
                assert_eq!(alley.data, before);
                assert_eq!(alley.framebuffer().bytes(), frame.bytes());
            }
        }
        assert_eq!(word(&alley.data, 0x2ae5), initial_random);
        assert_eq!(alley.framebuffer().bytes(), alley.screen.bytes());
        // Once hidden, the next spawn still requires retrace.
        let before = alley.data.clone();
        assert!(alley.advance_bin_cat(27, false).unwrap().is_none());
        assert_eq!(alley.data, before);
    }
}

#[test]
fn active_bin_does_not_skip_frames_across_tick_gaps_or_wraparound() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut alley = bin_animation_fixture(machine);
        set_word(&mut alley.data, 0x1d5a, u16::MAX - 1);
        for (tick, count) in [(u16::MAX, 26), (0, 25), (100, 24)] {
            alley.advance_bin_cat(tick, false).unwrap().unwrap();
            assert_eq!(alley.data[0x1d59], count);
            assert_eq!(word(&alley.data, 0x1d5a), tick);
        }
    }
}

#[test]
fn active_bin_contact_still_holds_the_image_and_alerts_the_dog() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut alley = bin_animation_fixture(machine);
        alley.advance_bin_cat(0, false).unwrap().unwrap();
        let image = alley.bin_image;
        alley.player.x = 200;
        alley.player.y = 158;
        let random = word(&alley.data, 0x2ae5);
        assert!(alley.advance_bin_cat(1, false).unwrap().is_none());
        assert_eq!(alley.data[0x1d59], 26);
        assert_eq!(alley.data[0x1d58], 1);
        assert_eq!(word(&alley.data, 0x1d5a), 1);
        assert_eq!(alley.bin_image, image);
        assert_eq!(word(&alley.data, 0x2ae5), random);
        alley.player = Player::at_alley_floor(8);
        alley.advance_bin_cat(2, false).unwrap().unwrap();
        assert_eq!(alley.data[0x1d59], 25);
    }
}

fn milk_approach_fixture(machine: Machine, difficulty: u16) -> Alley {
    let mut alley = Alley::synthetic();
    alley.machine = machine;
    set_word(&mut alley.data, 4, 6);
    set_word(&mut alley.data, 8, difficulty);
    // Authored bowls and a walkable surface; dogs and broom movement omitted.
    alley.player = Player::at_alley_floor(80);
    alley.player.y = 136;
    alley.player.vertical_coordinate = 186;
    alley.data[0x1029] = 136;
    set_word(&mut alley.data, 0x11a9, 300);
    alley.data[0x43e0] = 1;
    alley.data[0x44c4] = 4;
    alley.data[0x44c5] = 4;
    set_word(&mut alley.data, 0x3260, 0x2000);
    crate::room::initialize_broom(&mut alley.data).unwrap();
    set_word(&mut alley.data, 0x32f2 + 6 * 2, u16::MAX);
    for offset in (0x0fb6..0x0fc2).step_by(2) {
        set_word(&mut alley.data, offset, 0x0c03);
    }
    for (slot, x) in [(0, 144), (1, 240)] {
        alley.data[0x4499 + slot] = 144;
        set_word(&mut alley.data, 0x4481 + slot * 2, x);
        set_word(&mut alley.data, 0x44a5 + slot * 2, 0x410c);
    }
    alley
}

#[test]
fn milk_action_approaches_and_drinks_with_either_input_mode() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        for difficulty in 0..8 {
            for action in [
                crate::control_history::Key::LeftAlt,
                crate::control_history::Key::PadAction,
            ] {
                let run = |responsive| {
                    let mut alley = milk_approach_fixture(machine, difficulty);
                    let mut keys = crate::control_history::Keys::default();
                    keys.set(action, true);
                    // Other inputs, including a connected controller, are neutral.
                    let (controls, held) = keys.controls(false);
                    let mut runtime = crate::runtime::Runtime::new(machine);
                    runtime.responsive_controls = responsive;
                    let mut trace = Vec::new();
                    while runtime.clock.elapsed_nanos() < 5_000_000_000 {
                        let event = runtime
                            .advance_pass_with_action(&mut alley, controls, held)
                            .unwrap();
                        trace.push(format!("{event:?};{:?}", alley.player));
                        if alley.data[0x44c4] == 0 {
                            break;
                        }
                    }
                    assert_eq!(
                        alley.data[0x44c4], 0,
                        "{machine:?}, difficulty {difficulty}, responsive {responsive}"
                    );
                    (trace, alley.framebuffer().bytes().to_vec())
                };
                let original = run(false);
                let responsive = run(true);
                assert_eq!(
                    original.0.len(),
                    responsive.0.len(),
                    "{machine:?}, difficulty {difficulty}"
                );
                for (pass, (expected, actual)) in original.0.iter().zip(&responsive.0).enumerate() {
                    assert_eq!(
                        expected, actual,
                        "pass {pass}, {machine:?}, difficulty {difficulty}"
                    );
                }
                assert_eq!(original.1, responsive.1);
            }
        }
    }
}

#[test]
fn milk_responsive_input_changes_interrupt_auto_approach_between_ticks() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        for (controls, action) in [
            (Controls::default(), false),
            (Controls { x: -1, y: 0 }, true),
            (Controls { x: 0, y: -1 }, true),
        ] {
            let mut alley = milk_approach_fixture(machine, 3);
            let mut runtime = crate::runtime::Runtime::new(machine);
            runtime.responsive_controls = true;
            runtime
                .advance_pass_with_action(&mut alley, Controls::default(), true)
                .unwrap();
            // Start safely inside a fresh BIOS tick, accept the milk request,
            // then change host input before either native gate opens again.
            runtime.clock.advance_to_next_bios_tick();
            runtime.controls_gate =
                crate::timing::ControlsGate::new(runtime.clock.sample().bios.tick_low);
            runtime
                .advance_pass_with_action(&mut alley, Controls::default(), true)
                .unwrap();
            assert_eq!(runtime.controls, Controls { x: 1, y: 0 });
            let milk_tick = word(&alley.data, 0x44d3);
            let event = runtime
                .advance_pass_with_action(&mut alley, controls, action)
                .unwrap();
            assert!(!event.control_polled);
            assert_eq!(word(&alley.data, 0x44d3), milk_tick);
            assert_eq!(alley.data[0x69a], if action { 0 } else { 16 });
            assert_eq!(runtime.controls, controls);
        }
    }
}

#[test]
fn drinking_recomposition_preserves_scenery_and_restores_the_normal_cat() {
    let mut alley = Alley::synthetic();
    set_word(&mut alley.data, 4, 6);
    alley.player.x = 144;
    alley.player.y = 136;
    alley.player.vertical_coordinate = 186;
    alley.player.vertical_direction = 0;
    alley.data[0x43e0] = 1;
    alley.data[0x44c4] = 2;
    alley.data[0x4499] = 144;
    set_word(&mut alley.data, 0x4481, 144);
    set_word(&mut alley.data, 0x44a5, 0x410c);
    alley.data[0x410c..0x410c + 120].fill(0x55);
    alley.screen.fill_visible(0xaa);
    alley
        .compositor
        .draw(
            &mut alley.screen,
            pixel_offset(144, 136),
            Dimensions(0x0602),
            &[0; 24],
        )
        .unwrap();
    alley.compose_actors().unwrap();
    let normal = alley.framebuffer().bytes().to_vec();
    alley.latch_action(true);
    let drinking = alley.advance_milk_drinking(1).unwrap();
    assert!(drinking.drew);
    assert!(alley.milk_player_image.is_some());
    assert_eq!(alley.data[0x44bd], 1);
    let drink_frame = alley.framebuffer().bytes().to_vec();
    for _ in 0..3 {
        alley.room_actor(|_, _, _| Ok(())).unwrap();
        assert_eq!(alley.framebuffer().bytes().as_slice(), drink_frame);
    }
    alley.latch_action(false);
    alley.advance_milk_drinking(2).unwrap();
    assert_eq!(alley.data[0x44bd], 0);
    assert!(alley.milk_player_image.is_none());
    assert_eq!(alley.framebuffer().bytes().as_slice(), normal);
}

#[cfg(feature = "dev-tools")]
#[test]
fn original_window_timing_override_reaches_both_window_kinds() {
    for bonus in [0, 1] {
        for original in [false, true] {
            let mut alley = Alley::synthetic();
            let mut options = alley.dev_options;
            options.original_window_timing = original;
            options.set_disabled_actors("window-projectiles").unwrap();
            alley.set_dev_options(options).unwrap();
            set_word(&mut alley.data, 8, 6);
            set_word(&mut alley.data, 0x181e + 6 * 2, 1);
            set_word(&mut alley.data, 0x2ae5, 0x10);
            set_word(&mut alley.data, 0x1658, 104);
            alley.data[0x1662] = 88;
            alley.data[0x166a] = 1;
            alley.data[0x418] = bonus;
            alley.player.x = 200;
            alley.player.y = 96;
            alley.advance_windows(65530, false).unwrap();
            let delay = if original { 1 } else { 18 };
            assert_eq!(word(&alley.data, 0x166c), delay);
            assert_eq!(word(&alley.data, 0x181e + 6 * 2), 1);
            alley.data[0x1665] = 16;
            alley.data[0x166a] = 1;
            alley.advance_windows(65530, false).unwrap();
            for (elapsed, phase) in [(delay - 1, 15), (delay, 14)] {
                alley.data[0x166a] = 1;
                alley
                    .advance_windows(65530u16.wrapping_add(elapsed), false)
                    .unwrap();
                assert_eq!(alley.data[0x1665], phase);
            }
        }
    }
}

#[test]
fn milk_warning_starts_after_timed_eye_work() {
    let mut alley = Alley::synthetic();
    set_word(&mut alley.data, 4, 6);
    alley.player.y = 136;
    alley.player.vertical_coordinate = 186;
    set_word(&mut alley.data, 0x4441, 1);
    set_word(&mut alley.data, 0x4459, 1);
    set_word(&mut alley.data, 0x43e1, alley.player.x);
    alley.data[0x43f9] = 136;
    set_word(&mut alley.data, 0x44ec, 40);
    alley.data[0x43e0] = 1;
    let mut ended = false;
    let step = alley
        .advance_milk_attention_timed(1, &mut |work, sound| {
            if matches!(work, crate::milk::Work::End) {
                ended = true;
                sound.at(7 * crate::timing::BIOS_TICK_PIT_CLOCKS + 100);
            }
            7
        })
        .unwrap();
    assert!(step.warning && ended);
    assert_eq!(word(&alley.data, 0x5ae2), 7);
    assert_eq!(alley.sound.tick(), 9 * crate::timing::BIOS_TICK_PIT_CLOCKS);
    assert_eq!(
        alley.sound.take_busy_ticks(),
        2 * crate::timing::BIOS_TICK_PIT_CLOCKS - 100
    );
}

#[test]
fn milk_pursuit_keeps_normal_image_metadata_while_drinking() {
    for dimensions in [Dimensions(0x0b03), Dimensions(0x0602)] {
        for drinking in [false, true] {
            let mut alley = Alley::synthetic();
            set_word(&mut alley.data, 4, 6);
            alley.player.x = 144;
            alley.player.y = 136;
            alley.player.vertical_coordinate = 186;
            alley.player.vertical_direction = 0;
            alley.data[0x43e0] = 1;
            alley.data[0x44c4] = 2;
            alley.data[0x4499] = 144;
            set_word(&mut alley.data, 0x4481, 144);
            set_word(&mut alley.data, 0x44a5, 0x410c);
            alley.screen.fill_visible(0xaa);
            alley
                .compositor
                .draw(
                    &mut alley.screen,
                    pixel_offset(144, 136),
                    dimensions,
                    &vec![0; dimensions.len()],
                )
                .unwrap();
            if drinking {
                alley.latch_action(true);
                alley.advance_milk_drinking(1).unwrap();
                assert!(alley.milk_player_image.is_some());
                assert!(alley.compositor.dimensions().is_none());
            }
            set_word(&mut alley.data, 0x4441, 1);
            set_word(&mut alley.data, 0x4459, 2);
            set_word(&mut alley.data, 0x43e1, 144);
            alley.data[0x43f9] = 136;
            let mut entries = Vec::new();
            for tick in [2, 3] {
                alley
                    .advance_milk_attention_timed(tick, &mut |work, _| {
                        if let crate::milk::Work::PursuitEntry { player_dimensions } = work {
                            entries.push(player_dimensions);
                        }
                        tick
                    })
                    .unwrap();
            }
            assert_eq!(entries, [dimensions.0]);
            assert!(alley.dying());
            assert_eq!(alley.data[0x44bd], 0);
            assert!(alley.milk_player_image.is_none());
        }
    }
}

#[test]
fn scene_palettes_and_carried_gift_change_colors_without_changing_pixels() {
    let mut alley = Alley::synthetic();
    set_word(&mut alley.data, 4, 7);
    set_word(&mut alley.data, 0x2e8d, 0xffff);
    alley.data[0x185a] = 0;
    alley.presented.fill_visible(0x1b);
    let before = alley.framebuffer().bytes().to_vec();
    let rgba = alley.rgba();
    assert_eq!(
        &rgba[..16],
        &[
            0, 0, 0, 255, 0, 170, 0, 255, 170, 0, 0, 255, 170, 85, 0, 255
        ]
    );
    set_word(&mut alley.data, 0x2e8d, 0);
    assert_eq!(&alley.rgba()[..4], &[0, 0, 170, 255]);
    alley.machine = Machine::Pcjr;
    alley.data[0x1842] = 10;
    alley.data[0x184a] = 4;
    alley.data[0x1852] = 15;
    assert_eq!(
        &alley.rgba()[4..16],
        &[85, 255, 85, 255, 170, 0, 0, 255, 255, 255, 255, 255]
    );
    assert_eq!(alley.framebuffer().bytes().as_slice(), before);
}

#[test]
fn pause_captions_are_external_and_bounded() {
    let mut alley = Alley::synthetic();
    alley.data[0x6d91..0x6d96].copy_from_slice(b"test\0");
    alley.data[0x6db2..0x6db9].copy_from_slice(b"resume\0");
    assert_eq!(alley.pause_text().unwrap(), ["test", "resume"]);
    alley.data[0x6d91..0x6db1].fill(b'x');
    alley.data[0x6db1] = 0;
    assert_eq!(alley.pause_text().unwrap()[0].len(), 32);
    alley.data[0x6d91..0x6db2].fill(b'x');
    assert!(alley.pause_text().is_err());
    alley.data.truncate(0x6d90);
    assert!(alley.pause_text().is_err());
}

fn actor_fixture() -> Alley {
    let mut alley = Alley::synthetic();
    alley.screen.fill_visible(0xaa);
    initialize_score_state(&mut alley.data).unwrap();
    alley.data[0x1f36..0x1f39].copy_from_slice(&[0, 32, 64]);
    alley.data[0x1f39..0x1f3c].copy_from_slice(&[9, 7, 5]);
    for row in 0..3 {
        set_word(&mut alley.data, 0x1f5f + row * 2, 0x1ee0);
    }
    alley.data[0x1cf0..0x1d24].fill(0xff);
    alley.data[0x1d70..0x1dd0].fill(0xff);
    alley.data[0x1e30..0x1f00].fill(0x55);
    alley.mice = Mice::initialize(&mut alley.data, 8).unwrap();
    set_word(&mut alley.data, 0x1f32, 80);
    set_word(&mut alley.data, 0x1f44, pixel_offset(80, 32));
    alley.player.x = 80;
    alley.player.y = 32;
    alley.player.vertical_coordinate = 82;
    alley
}

#[test]
fn airborne_mouse_catch_updates_the_display_without_losing_lives_or_erasing_player() {
    let mut alley = actor_fixture();
    let dimensions = Dimensions(0x0e03);
    let source = vec![0; dimensions.len()];
    alley
        .compositor
        .draw(&mut alley.screen, pixel_offset(80, 32), dimensions, &source)
        .unwrap();
    assert!(!alley.advance_mice(1).unwrap().paused);
    assert_eq!(alley.data[0x1f51], 1);
    assert_eq!(&alley.data[0x1f82..0x1f89], &[0, 0, 0, 0, 0, 7, 0]);
    assert_eq!(alley.data[0x1f80], 3);
    assert_eq!(
        alley
            .screen
            .read_rect(pixel_offset(80, 32), dimensions)
            .unwrap(),
        source
    );
    assert_ne!(alley.framebuffer().bytes(), alley.screen.bytes());
    alley.compositor.restore(&mut alley.screen).unwrap();
    assert_eq!(
        alley
            .screen
            .read_rect(pixel_offset(80, 32), dimensions)
            .unwrap(),
        vec![0xaa; dimensions.len()]
    );
}

#[test]
fn mouse_knockdown_preserves_motion_history_and_restores_the_clean_layers() {
    let mut alley = actor_fixture();
    alley.player.support_state = 2;
    alley.player.vertical_step = 4;
    alley.player.phase = 77;
    alley.data[0] = 1;
    assert!(alley.advance_mice(1).unwrap().paused);
    assert_eq!(
        (
            alley.player.support_state,
            alley.player.vertical_direction,
            alley.player.horizontal_direction
        ),
        (0, 1, 0)
    );
    assert_eq!((alley.player.vertical_step, alley.player.phase), (4, 77));
    assert_eq!(alley.data[0x1f80], 3);
    assert_ne!(alley.framebuffer().bytes(), alley.screen.bytes());
    let seed = word(&alley.data, 0x2ae5);
    alley.mouse_effect_random(1);
    let first = word(&alley.data, 0x2ae5);
    assert_ne!(first, seed);
    alley.mouse_effect_random(1);
    assert_eq!(word(&alley.data, 0x2ae5), first);
    alley.finish_mouse_effect().unwrap();
    assert_eq!(alley.framebuffer().bytes(), alley.screen.bytes());
}

#[test]
fn mouse_exit_poll_can_observe_a_tick_after_the_sound_poll() {
    use crate::runtime::Runtime;
    use crate::timing::{BIOS_TICK_PIT_CLOCKS, PlayerGate};
    let mut alley = actor_fixture();
    alley.player.support_state = 2;
    alley.data[0] = 1;
    alley.data[0x531] = 255;
    let mut runtime = Runtime::new(Machine::Cga);
    runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS);
    runtime.player_gate = PlayerGate::new(1, 0);
    for _ in 0..4 {
        runtime
            .advance_pass(&mut alley, Controls::default())
            .unwrap();
    }
    assert!(alley.mouse_effect.is_some());
    let start = word(&alley.data, 0x1f65);
    let deadline = u64::from(start + 8) * BIOS_TICK_PIT_CLOCKS;
    runtime
        .clock
        .advance_pit_ticks(deadline - 200 - runtime.clock.elapsed_pit_ticks() as u64);
    set_word(&mut alley.data, 0x5a14, start + 7);
    let seed = word(&alley.data, 0x2ae5);
    let event = runtime
        .advance_pass(&mut alley, Controls { x: -1, y: -1 })
        .unwrap();
    assert!(event.mouse_wait && !event.control_polled);
    assert!(alley.mouse_effect.is_none());
    assert_eq!(word(&alley.data, 0x5a14), start + 7);
    assert_eq!(word(&alley.data, 0x2ae5), seed);
    assert_eq!(runtime.clock.sample().bios.tick_low, start + 8);
}

#[test]
fn eel_wait_baseline_and_palette_sample_follow_setup_across_tick_wrap() {
    let mut alley = Alley::synthetic();
    alley.damage_effect = Some(DamageEffect {
        since: u16::MAX,
        duration: 13,
        kind: EffectKind::Eel {
            phase: EelPhase::Restart,
            palette_tick: u16::MAX,
            background: 0,
        },
    });
    alley.start_eel_effect_wait(0);
    assert_eq!(word(&alley.data, 0x3509), 0);
    assert!(matches!(
        alley.damage_effect,
        Some(DamageEffect {
            since: 0,
            kind: EffectKind::Eel {
                palette_tick: 0,
                ..
            },
            ..
        })
    ));
    assert!(!alley.advance_damage_effect(12, 0, false).unwrap());
    assert!(!alley.advance_damage_effect(13, 0, true).unwrap());
    assert!(!alley.advance_damage_effect(13, 0, false).unwrap());
    assert!(!alley.advance_damage_effect(13, 0, false).unwrap());
    assert!(alley.advance_damage_effect(13, 0, false).unwrap());
}

#[test]
fn spider_wait_baseline_follows_completed_drawing_across_tick_wrap() {
    let mut alley = Alley::synthetic();
    alley.damage_effect = Some(DamageEffect {
        since: u16::MAX - 1,
        duration: 9,
        kind: EffectKind::Spider,
    });
    alley.start_spider_effect_wait(0);
    assert_eq!(word(&alley.data, 0x39c8), 0);
    assert!(!alley.advance_damage_effect(8, 0, false).unwrap());
    assert!(alley.advance_damage_effect(9, 0, false).unwrap());
    assert_eq!(alley.data[0x552], 1);
}

#[test]
fn scheduled_mouse_contact_blocks_and_resumes_without_life_loss() {
    use crate::runtime::Runtime;
    use crate::timing::{BIOS_TICK_PIT_CLOCKS, PlayerGate};
    let mut alley = actor_fixture();
    alley.player.support_state = 2;
    alley.data[0x531] = 255;
    let mut runtime = Runtime::new(Machine::Cga);
    runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS);
    runtime.player_gate = PlayerGate::new(1, 0);
    for _ in 0..4 {
        runtime
            .advance_pass(&mut alley, Controls::default())
            .unwrap();
    }
    assert!(alley.mouse_effect.is_some());
    let start = word(&alley.data, 0x1f65);
    let phase = alley.player.phase;
    while alley.mouse_effect.is_some() {
        let event = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: -1 })
            .unwrap();
        assert!(event.mouse_wait);
        assert!(!event.control_polled);
        assert!(!event.actor_group_due);
        assert!(event.acceptance.is_none());
        assert_eq!(alley.player.phase, phase);
    }
    assert_eq!(runtime.clock.sample().bios.tick_low.wrapping_sub(start), 8);
    assert_eq!(alley.data[0x1f80], 3);
    assert_eq!(alley.player.vertical_direction, 1);
    assert_eq!(alley.framebuffer().bytes(), alley.screen.bytes());
}

#[test]
fn lower_support_takeoff_preserves_attachment_then_release_sound() {
    use crate::runtime::{ReferenceCosts, Runtime};
    use crate::timing::{RetraceProfile, VirtualClock};

    for machine in [Machine::Cga, Machine::Pcjr] {
        for legacy in [false, true] {
            for latch in [0, 1] {
                for enabled in [false, true] {
                    let mut alley = actor_fixture();
                    alley.machine = machine;
                    alley.player.y = 142;
                    alley.player.vertical_coordinate = 192;
                    alley.player.horizontal_direction = 1;
                    alley.data[0xff0] = 20;
                    alley.data[0x127c] = latch;
                    alley.data[0x127d] = 0xa5;
                    alley.data[0] = if enabled { 0xff } else { 0 };
                    alley.data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
                    alley.sound.record(true);
                    let costs = if legacy {
                        ReferenceCosts::dosbox_x_240(machine)
                    } else {
                        ReferenceCosts::ibm_xt(machine)
                    };
                    let mut runtime = Runtime::with_costs(machine, costs);
                    runtime.clock =
                        VirtualClock::with_initial_tick(RetraceProfile::default(), 1000);
                    let event = runtime
                        .advance_pass(&mut alley, Controls { x: 1, y: -1 })
                        .unwrap();
                    assert!(event.acceptance.is_some());
                    assert_eq!(alley.player.vertical_direction, 255);
                    assert_eq!(&alley.data[0x127c..0x127e], &[0, 0xa5]);
                    let writes: Vec<_> = alley.sound.drain().collect();
                    let divisors: Vec<_> = writes
                        .iter()
                        .filter(|w| w.port == 0x42)
                        .map(|w| w.value)
                        .collect::<Vec<_>>()
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect();
                    let expected = if !enabled {
                        vec![]
                    } else if latch == 0 {
                        vec![0x400, 0x390]
                    } else {
                        vec![0x390]
                    };
                    assert_eq!(divisors, expected);
                    assert_eq!(
                        writes.iter().filter(|w| w.port == 0x43).count(),
                        expected.len()
                    );
                }
            }
        }
    }
}

#[test]
fn bin_push_overrides_requested_jump_and_commits_one_shared_random_draw() {
    let mut alley = actor_fixture();
    alley.player.y = 142;
    alley.player.vertical_coordinate = 192;
    alley.data[0xff0] = 20;
    alley.data[0x1d59] = 13;
    alley.data[0x127c] = 1;
    alley.data[0x127d] = 0xa5;
    set_word(&mut alley.data, 0x1d5c, 80);
    alley.data[0x1d5f] = 147;
    set_word(&mut alley.data, 0x1d64, 0x0d02);
    let next = crate::assets::random_successor(word(&alley.data, 0x2ae5));
    let outcome = alley
        .advance_player_update(Controls { x: 1, y: -1 })
        .unwrap();
    assert!(outcome.bin_contact_checked);
    let forced = outcome.forced_controls.unwrap();
    assert_eq!(forced.y, 1);
    assert_eq!(forced.x, if next & 1 == 0 { -1 } else { 1 });
    assert_eq!(word(&alley.data, 0x2ae5), next);
    assert_eq!(alley.data[0x1d58], 1);
    assert_eq!(&alley.data[0x127c..0x127e], &[0, 0xa5]);
    assert_eq!(alley.player.vertical_direction, 1);
    assert_eq!(alley.data[0x1f80], 3);
}

#[test]
fn moving_clothes_recompose_the_carried_cat_without_extra_random_calls_or_ghosts() {
    let mut alley = Alley::synthetic();
    alley.screen.fill_visible(0xaa);
    let dimensions = Dimensions(0x1028);
    let band: Vec<u8> = (0..1280).map(|index| (index % 251) as u8).collect();
    alley.screen.copy_rect(0x140, dimensions, &band).unwrap();
    alley.data[0x531] = 1;
    alley.data[0x532] = 100;
    alley.data[0x529] = 0xff;
    alley.data[0x525] = 2;
    alley.data[0x53a] = 75;
    alley.data[0x53d] = 46;
    set_word(&mut alley.data, 0x51d, 0x140);
    alley.player.x = 80;
    alley.player.y = 8;
    alley.player.vertical_coordinate = 58;
    alley.player.support_state = 2;
    let source = vec![0; Dimensions(0x0e03).len()];
    alley
        .compositor
        .draw(
            &mut alley.screen,
            pixel_offset(80, 8),
            Dimensions(0x0e03),
            &source,
        )
        .unwrap();
    let seed = word(&alley.data, 0x2ae5);
    assert!(alley.advance_scenery(1, false).unwrap().is_some());
    assert_eq!(alley.player.x, 84);
    assert_eq!(word(&alley.data, 0x2ae5), seed);
    assert_eq!(
        alley
            .screen
            .read_rect(pixel_offset(84, 8), Dimensions(0x0e03))
            .unwrap(),
        source
    );
    alley.compositor.restore(&mut alley.screen).unwrap();
    let clean = alley.screen.read_rect(0x140, dimensions).unwrap();
    for row in 0..16 {
        assert_eq!(
            &clean[row * 80 + 1..row * 80 + 80],
            &band[row * 80..row * 80 + 79]
        );
        assert_eq!(clean[row * 80], 0);
    }
}

#[test]
fn deferred_scenery_matches_the_previous_initial_title_handoff_and_following_passes() {
    use crate::runtime::Runtime;
    for machine in [Machine::Cga, Machine::Pcjr] {
        for seed in [2, 0xfa59] {
            for (skill, watched) in [(0, 20), (3, 120)] {
                let mut data = initial_title_data();
                // Authored valid scrolling tables for the subsequent timed passes.
                data[0x541..0x544].copy_from_slice(&[0, 9, 10]);
                for (row, offset) in [0x140, 0x68f, 0xb40].into_iter().enumerate() {
                    set_word(&mut data, 0x51d + row * 2, offset);
                }
                set_word(&mut data, 0x2ae5, seed);
                // Reproduce the former eager alley construction, whose state
                // is discarded when the title returns.
                let mut eager =
                    Alley::from_runtime_data(data.clone(), seed, skill, machine).unwrap();
                let mut splash = crate::splash::Splash::new(&data, machine).unwrap();
                let mut deferred = Alley::for_initial_title(&splash, seed, skill).unwrap();
                assert_eq!(deferred.data, splash.data());
                assert_eq!(
                    deferred.screen.bytes(),
                    &[0; crate::framebuffer::MEMORY_SIZE]
                );
                for tick in 0..=watched {
                    splash.advance(tick, true, tick == watched).unwrap();
                }
                assert!(splash.finished());
                for game in [&mut eager, &mut deferred] {
                    #[cfg(feature = "dev-tools")]
                    {
                        let mut options = DevOptions::default();
                        options.set_disabled_damage("all").unwrap();
                        game.set_dev_options(options).unwrap();
                    }
                    game.inherit_initial_splash(&splash).unwrap();
                    game.start_initial_game(skill, 0).unwrap();
                    game.sound.record(true);
                }
                let mut first_clock = Runtime::new(machine);
                let mut second_clock = Runtime::new(machine);
                for pass in 0..128 {
                    let controls = Controls {
                        x: if pass < 64 { 1 } else { -1 },
                        y: 0,
                    };
                    let first = first_clock.advance_pass(&mut eager, controls).unwrap();
                    let second = second_clock.advance_pass(&mut deferred, controls).unwrap();
                    assert_eq!(format!("{first:?}"), format!("{second:?}"));
                    assert_eq!(eager.data, deferred.data);
                    assert_eq!(
                        format!("{:?}", eager.player),
                        format!("{:?}", deferred.player)
                    );
                    assert_eq!(eager.screen.bytes(), deferred.screen.bytes());
                    assert_eq!(eager.framebuffer().bytes(), deferred.framebuffer().bytes());
                    assert_eq!(
                        eager.sound.drain().collect::<Vec<_>>(),
                        deferred.sound.drain().collect::<Vec<_>>()
                    );
                }
                eager.restart(0).unwrap();
                deferred.restart(0).unwrap();
                assert_eq!(eager.data, deferred.data);
                assert_eq!(eager.framebuffer().bytes(), deferred.framebuffer().bytes());
            }
        }
    }
}
