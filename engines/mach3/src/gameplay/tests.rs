// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

// Authored near-contact geometry deliberately overrides normal boss/patrol
// depths. This tests dispatch effects, not reachability in ordinary play.
fn synthetic_primary_contact(assets: &GraphicsAssets, variant: u8) -> Gameplay {
    let mut game = Gameplay::test_game(assets).unwrap();
    let mut records = game.enemy.snapshots();
    records[1].active = true;
    records[1].variant = variant;
    records[1].movement = match variant {
        5 => 31,
        6 => 34,
        2 => 25,
        _ => 0,
    };
    records[1].depth = 29;
    records[1].lateral = 0;
    records[1].height = -100;
    records[1].resolved_sprite = 13;
    records[6].active = true;
    game.enemy = enemy::Enemy::restore_snapshots(records, assets).unwrap();
    game
}

#[test]
fn primary_variant_contact_preserves_awards_explosion_shadow_and_sound_effects() {
    use crate::sound::{GameplayAction, Ramp};
    let assets = synthetic_assets();
    for variant in [0, 1, 2, 5, 6] {
        let mut game = synthetic_primary_contact(&assets, variant);
        game.hud.set_score_multiplier(2);
        let mut actions = Vec::new();
        game.begin_finish_world(&assets, enemy::EnemyEffects::default(), &mut |action| {
            actions.push(action)
        })
        .unwrap();
        let state = game.snapshot();
        assert_eq!((state.player.phase, state.player.damage), (1, 2));
        let falling = variant == 2;
        assert_eq!(state.hud.pending_score, if falling { 0 } else { 2000 });
        assert_eq!(state.enemies[1].movement, if falling { 251 } else { 3 });
        assert_eq!(state.enemies[6].active, falling);
        assert_eq!(
            actions,
            [
                GameplayAction::Request {
                    site: if falling { 0x39c9 } else { 0x3955 },
                    effect: Effect::Ramp(Ramp::Third)
                },
                GameplayAction::Request {
                    site: 0x3149,
                    effect: Effect::Ramp(Ramp::Second)
                },
            ]
        );
        assert!(
            game.enemy
                .player_collisions(&assets, state.player.coordinates, 13, true)
                .unwrap()[1]
                .is_none()
        );
    }
    for variant in [5, 6] {
        for (pending, expected) in [(30_500, 32_000), (31_000, 33_000)] {
            let mut game = synthetic_primary_contact(&assets, variant);
            game.hud.add_points(pending);
            game.hud.set_score_multiplier(2);
            game.begin_finish_world(&assets, enemy::EnemyEffects::default(), &mut |_| {})
                .unwrap();
            // The existing cap compares signed words; 33,000 is negative.
            assert_eq!(game.snapshot().hud.pending_score, expected);
        }
    }
}

#[test]
fn primary_contact_retains_player_phase_grace_and_projectile_first_order() {
    use crate::sound::{GameplayAction, Ramp};
    let assets = synthetic_assets();
    for variant in [5, 6] {
        for (responding, immune, projectile) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (false, true, true),
        ] {
            let mut game = synthetic_primary_contact(&assets, variant);
            if responding {
                game.player.hit(1);
            }
            if immune {
                game.player.finish_death();
            }
            if projectile {
                let mut records = game.enemy.snapshots();
                records[0].active = true;
                records[0].depth = 29;
                records[0].lateral = 0;
                records[0].height = -100;
                records[0].resolved_sprite = 13;
                game.enemy = enemy::Enemy::restore_snapshots(records, &assets).unwrap();
            }
            let mut actions = Vec::new();
            game.begin_finish_world(&assets, enemy::EnemyEffects::default(), &mut |action| {
                actions.push(action)
            })
            .unwrap();
            let state = game.snapshot();
            if immune {
                // Grace blocks player damage, while target effects still run,
                // even when the preceding projectile also requests damage.
                assert_eq!((state.player.phase, state.player.damage), (0, 0));
                assert_eq!(state.hud.pending_score, 1000);
                assert_eq!(state.enemies[1].movement, 3);
                assert!(!state.enemies[6].active);
                assert_eq!(
                    actions,
                    [GameplayAction::Request {
                        site: 0x3955,
                        effect: Effect::Ramp(Ramp::Third)
                    }]
                );
            } else {
                // An existing response, or the earlier projectile, suppresses
                // the craft's award/explosion rather than applying two hits.
                assert_eq!((state.player.phase, state.player.damage), (1, 1));
                assert_eq!(state.hud.pending_score, 0);
                assert_eq!(
                    state.enemies[1].movement,
                    if variant == 5 { 31 } else { 34 }
                );
                assert!(state.enemies[6].active);
                assert_eq!(actions.len(), usize::from(projectile));
            }
        }
    }
}

#[test]
fn initializer_moves_player_and_world_without_advancing_gameplay() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    let before = game.snapshot();
    game.initialize_input(&assets, 8 | 16).unwrap();
    let after = game.snapshot();
    assert_eq!(after.player.coordinates, [14, 4, -100]);
    assert_eq!(after.player.velocity, [1, 0]);
    assert_eq!(after.player.effective_input, 24);
    assert_eq!(after.player.requested_sprite, 12);
    assert_eq!(after.player.shadow_requested_sprite, 17);
    assert_eq!(after.static_lateral, -8);
    assert!(
        after
            .weapon
            .shots
            .iter()
            .all(|shot| !shot.active && shot.lateral == -8)
    );
    assert!(after.enemies.iter().all(|enemy| enemy.lateral == -8));
    assert!(after.terrain.iter().all(|row| row.lateral == -8));
    assert_eq!((after.body, after.render_pass), (0, 0));
    assert_eq!(after.random, before.random);
    assert_eq!(after.hud.fuel, before.hud.fuel);
    assert_eq!(after.weapon.counter, before.weapon.counter);
    assert_eq!(after.script_cursor, before.script_cursor);
    assert_eq!(after.strip_index, before.strip_index);
    assert_eq!(game.frame(), game.pass_frame());
    assert!(game.pass_snapshot().is_none());
    assert!(game.take_transitions().is_empty());
    assert!(game.initialize_input(&assets, 8).is_err());
}

#[test]
fn initializer_retains_poll_bits_and_rejects_late_or_invalid_input() {
    let assets = synthetic_assets();
    for mask in [0, 16, 128] {
        let mut game = Gameplay::test_game(&assets).unwrap();
        game.initialize_input(&assets, mask).unwrap();
        assert_eq!(game.snapshot().player.effective_input, u16::from(mask));
        assert_eq!(game.snapshot().player.coordinates, [14, 0, -100]);
        assert_eq!(game.snapshot().player.velocity, [0, 0]);
        assert!(!game.snapshot().player.game_over);
    }
    let mut game = Gameplay::test_game(&assets).unwrap();
    assert!(game.initialize_input(&assets, 32).is_err());
    game.advance_subframe(&assets, Steering::default()).unwrap();
    assert!(game.initialize_input(&assets, 8).is_err());
}

#[test]
fn random_preserves_sixteen_bit_arithmetic_and_add_carry() {
    let mut random = Random::default();
    let words: Vec<_> = (0..4).map(|_| random.next_word()).collect();
    assert_eq!(words, [0x5630, 0xbe90, 0x6338, 0x4293]);
    assert_eq!(random.state(), [34449, 35689, 17043]);
}

pub(super) fn synthetic_assets() -> GraphicsAssets {
    GraphicsAssets {
        primary: vec![0; 28_960],
        alternate: vec![0; 16_000],
        sprites: (0..190)
            .map(|index| crate::assets::Sprite {
                width_bytes: 14,
                height: 18,
                descriptor: if index == 13 {
                    [0, 0, 15, 0, 0, 0, 0]
                } else {
                    [0; 7]
                },
                color: vec![0; 14 * 18],
                mask: vec![255; 14 * 18],
            })
            .collect(),
    }
}

#[test]
fn first_pass_wait_follows_selected_layer_changes_and_signed_overlay() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.render_pass = 1;
    for (layer, overlay, alternate, expected) in [
        (0, 0, false, true),
        (0, 0, true, true), // Alternate picture alone is not the layer gate.
        (2, 0, true, false),
        (2, 0, true, true),
        (2, 1, true, false), // Positive overlay selects layer1.
        (2, 2, true, true),
        (2, 0x8000, true, false), // Signed negative overlay restores layer2.
        (2, 0xffff, true, true),
        (0, 0, false, false),
        (0, 0, false, true),
    ] {
        game.layer = layer;
        game.overlay = overlay;
        game.alternate_background = alternate;
        game.render(&assets).unwrap();
        assert_eq!(game.pass_one_wait_required(), expected);
    }
    game.render_pass = 2;
    assert!(!game.pass_one_wait_required());
    game.layer = 2;
    game.render(&assets).unwrap();
    game.render_pass = 1;
    game.render(&assets).unwrap();
    assert!(!game.pass_one_wait_required()); // Pass2 did not overwrite the body-local gate.
}

#[test]
fn authored_events_and_supported_boundary_preserve_the_final_state() {
    let assets = synthetic_assets();
    let mut gameplay = Gameplay::test_game(&assets).unwrap();
    assert_eq!(gameplay.frame(), &scene::initial_frame(&assets).unwrap());
    for body in 1..=360 {
        assert!(gameplay.advance(&assets).unwrap());
        let state = gameplay.snapshot();
        assert_eq!(state.body, body);
        assert_eq!(
            state.terrain_selector,
            if body < 21 {
                0
            } else if body < 118 {
                5
            } else if body < 201 {
                7
            } else if body < 284 {
                6
            } else {
                3
            }
        );
        assert_eq!(
            state.enemies[1].active,
            (61..98).contains(&body)
                || (138..181).contains(&body)
                || (221..264).contains(&body)
                || (304..341).contains(&body),
            "enemy activation at body {body}"
        );
        assert_eq!(
            state.script_cursor,
            if body < 21 {
                0
            } else if body < 41 {
                1
            } else if body < 61 {
                2
            } else if body < 118 {
                3
            } else if body < 138 {
                4
            } else if body < 201 {
                5
            } else if body < 221 {
                6
            } else if body < 284 {
                7
            } else if body < 304 {
                8
            } else {
                9
            }
        );
    }
    // Synthetic descriptor geometry does not reproduce the original
    // collision route. Exercise the bounded guard independently of it.
    gameplay.body = DEFAULT_EXPORT_BODY_LIMIT;
    let final_frame = gameplay.frame().clone();
    let final_random = gameplay.snapshot().random;
    for _ in 0..3 {
        assert!(!gameplay.advance(&assets).unwrap());
    }
    assert!(gameplay.complete());
    assert_eq!(gameplay.snapshot().body, DEFAULT_EXPORT_BODY_LIMIT);
    assert_eq!(gameplay.snapshot().random, final_random);
    assert_eq!(gameplay.frame(), &final_frame);
}

#[test]
fn displayed_passes_sample_input_and_finish_the_boundary_only_after_pass_three() {
    let assets = synthetic_assets();
    let mut gameplay = Gameplay::test_controlled_game(&assets).unwrap();
    gameplay.body_limit = Some(1);
    let initial = gameplay.snapshot();
    assert!(gameplay.at_body_boundary());
    for (pass, lateral, velocity) in [(1, -4, -1), (2, -12, -2), (3, -24, -3)] {
        assert!(
            gameplay
                .advance_subframe(
                    &assets,
                    Steering {
                        left: true,
                        ..Steering::default()
                    }
                )
                .unwrap()
        );
        let state = gameplay.snapshot();
        assert_eq!(state.body, 1);
        assert_eq!(state.render_pass, pass);
        assert_eq!(state.player.coordinates[1], lateral);
        assert_eq!(state.player.velocity[0], velocity);
        assert_eq!(gameplay.at_body_boundary(), pass == 3);
        assert_eq!(gameplay.complete(), pass == 3);
        assert_eq!(state.script_delay, u16::from(pass == 3));
        assert_eq!(
            state.player.grace,
            initial.player.grace + u16::from(pass == 3 && initial.player.grace < 4)
        );
    }
    let last_frame = gameplay.frame().clone();
    assert!(
        !gameplay
            .advance_subframe(&assets, Steering::default())
            .unwrap()
    );
    assert_eq!(gameplay.frame(), &last_frame);
}

#[test]
fn input_changes_between_passes_do_not_repeat_world_motion() {
    let assets = synthetic_assets();
    let mut gameplay = Gameplay::test_game(&assets).unwrap();
    let left = Steering {
        left: true,
        ..Steering::default()
    };
    gameplay.advance_subframe(&assets, left).unwrap();
    let first = gameplay.snapshot();
    assert!(gameplay.advance(&assets).is_err());
    gameplay
        .advance_subframe(&assets, Steering::default())
        .unwrap();
    let second = gameplay.snapshot();
    assert_eq!(second.player.velocity[0], 0);
    assert_eq!(second.player.coordinates[1], first.player.coordinates[1]);
    assert_eq!(second.terrain[0].depth, first.terrain[0].depth);
    assert_eq!(second.hud.score, first.hud.score);
    assert_eq!(second.random, first.random);
    assert_eq!(second.player.grace, first.player.grace);
    gameplay
        .advance_subframe(
            &assets,
            Steering {
                right: true,
                ..Steering::default()
            },
        )
        .unwrap();
    assert_eq!(gameplay.snapshot().player.velocity[0], 1);
    assert_eq!(gameplay.snapshot().player.coordinates[1], 0);
    assert!(gameplay.at_body_boundary());
    assert!(gameplay.advance(&assets).unwrap());
    assert_eq!(gameplay.snapshot().body, 2);
}

#[test]
fn formation_count_command_consumes_one_word_and_waits_to_spawn() {
    let assets = synthetic_assets();
    let mut baseline = Gameplay::test_game(&assets).unwrap();
    baseline.script_cursor = 9;
    baseline.script_delay = 20;
    baseline.terrain_selector = 7;
    let mut formation = Gameplay::test_game(&assets).unwrap();
    formation.script_cursor = 10;
    formation.script_delay = 20;
    formation.terrain_selector = 7;
    baseline.advance(&assets).unwrap();
    formation.advance(&assets).unwrap();
    let mut expected_random = baseline.random;
    expected_random.next_word();
    assert_eq!(formation.random, expected_random);
    assert_eq!(formation.primary_count, 2);
    assert_eq!(formation.script_cursor, 11);
    assert_eq!(formation.script_delay, 1);
    assert!(!formation.enemy.active());
    for _ in 0..20 {
        formation.advance(&assets).unwrap();
    }
    let state = formation.snapshot();
    assert_eq!(state.script_cursor, 12);
    assert_eq!(state.script_delay, 0);
    assert_eq!(state.enemies[1].depth, 1600);
    assert_eq!(state.enemies[2].depth, 1500);
    assert!(state.enemies[1].active && state.enemies[2].active);
    assert!(state.enemies[6].active && state.enemies[7].active);
}

#[test]
fn gate_allocation_preserves_the_delay_before_the_scripted_enemy() {
    let assets = synthetic_assets();
    let mut gameplay = Gameplay::test_game(&assets).unwrap();
    gameplay.script_cursor = 13;
    gameplay.script_delay = 20;
    gameplay.terrain_selector = 7;
    gameplay.primary_count = 2;
    gameplay.advance(&assets).unwrap();
    let state = gameplay.snapshot();
    assert_eq!(state.script_cursor, 14);
    assert_eq!(state.script_delay, 1);
    assert_eq!(
        state.special.map(|slot| slot.depth),
        [916, 1140, 1364, 1588, 1812]
    );
    assert!(state.special.iter().all(|slot| slot.active
        && slot.rect.x_bytes == 128
        && slot.rect.width_bytes == 43
        && slot.rect.height == 0));
    assert!(!gameplay.enemy.active());
    for _ in 0..20 {
        gameplay.advance(&assets).unwrap();
    }
    let state = gameplay.snapshot();
    assert_eq!(state.script_cursor, 15);
    assert_eq!(state.script_delay, 0);
    assert!(state.enemies[1].active);
    assert!(!state.enemies[2].active);
    assert_eq!(state.enemies[1].variant, 1);
    assert_eq!(state.enemies[1].animation_phase, 0);
    assert_eq!(state.enemies[1].depth, 1000);
    assert_eq!(state.enemies[2].depth, 1000);
    assert!(state.enemies[6].active);
    assert!(!state.enemies[7].active);
}

#[test]
fn zero_movement_entries_keep_order_until_their_movement_changes() {
    let mut gameplay = Gameplay::test_game(&synthetic_assets()).unwrap();
    gameplay.world_order = vec![4, 5, 6, 15];
    gameplay.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 900, 0), (15, 200, 2)]);
    assert_eq!(gameplay.world_order, [5, 6, 15, 4]);
    gameplay.sort_world(&[(4, 100, 253), (5, 300, 0), (6, 200, 8), (15, 200, 2)]);
    assert_eq!(gameplay.world_order, [5, 6, 15, 4]);
}

#[test]
fn equal_depth_objects_retain_their_order_from_the_preceding_pass() {
    let mut gameplay = Gameplay::test_game(&synthetic_assets()).unwrap();
    let mut depths: Vec<_> = gameplay
        .world_order
        .iter()
        .map(|slot| (*slot, 100, 2))
        .collect();
    depths
        .iter_mut()
        .find(|(slot, _, _)| *slot == 22)
        .unwrap()
        .1 = 200;
    gameplay.sort_world(&depths);
    assert_eq!(gameplay.world_order[0], 22);
    depths
        .iter_mut()
        .find(|(slot, _, _)| *slot == 22)
        .unwrap()
        .1 = 100;
    gameplay.sort_world(&depths);
    assert_eq!(gameplay.world_order[0], 22);
}
pub(super) fn synthetic_rules(commands: &[u8]) -> ScriptRules {
    let mut data = vec![0; 65536];
    data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
    for index in 0..21 {
        let at = 0x0a3e + 50 * index;
        data[at..at + commands.len()].copy_from_slice(commands);
        data[at + commands.len()] = 0xcf;
        data[0x0e58 + 4 * index..0x0e5a + 4 * index]
            .copy_from_slice(&(((index + 1) % 21) as u16).to_le_bytes());
        data[0x0e5a + 4 * index..0x0e5c + 4 * index].copy_from_slice(&(index as u16).to_le_bytes());
    }
    ScriptRules::from_data_segment(&data).unwrap()
}

#[test]
fn mode_reset_persists_until_an_actual_palette_command() {
    let assets = synthetic_assets();
    let start = SessionState::default().begin_game();
    let mut game = Gameplay::for_play(&assets, synthetic_rules(&[0xce, 0xce]), start).unwrap();
    for _ in 0..21 {
        game.advance(&assets).unwrap();
    }
    assert_eq!(game.palette_family(), 0);
    let mut before = game.snapshot();
    let pixels = game.frame().clone();
    game.reset_video_palette();
    assert_eq!(game.palette_family(), 1);
    assert_eq!(game.frame(), &pixels);
    before.palette_family = 1;
    assert_eq!(format!("{:?}", game.snapshot()), format!("{before:?}"));
    for _ in 0..19 {
        game.advance(&assets).unwrap();
        assert_eq!(game.palette_family(), 1);
    }
    // Even a repeated CE is a new register write, not a changed-value test.
    game.advance(&assets).unwrap();
    assert_eq!(game.snapshot().body, 41);
    assert_eq!(game.palette_family(), 0);
}

#[test]
fn transition_compositor_does_not_change_the_pinned_body_layer() {
    let mut assets = synthetic_assets();
    assets.primary.fill(0x55);
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.advance_subframe(&assets, Steering::default()).unwrap();
    assert_eq!(game.selected_layer, 0);
    game.layer = 2;
    game.queue_transition(&assets).unwrap();
    assert_eq!(game.selected_layer, 0);
    assert_eq!(game.snapshot().background_layer, 2);
    assert_eq!(game.frame().packed()[140 * 80 + 37], 0);
    for _ in 0..2 {
        game.advance_subframe(
            &assets,
            Steering {
                right: true,
                ..Steering::default()
            },
        )
        .unwrap();
        assert_eq!(game.selected_layer, 0);
        assert_eq!(game.snapshot().background_layer, 0);
        assert!(
            game.frame()
                .packed()
                .chunks(80)
                .enumerate()
                .any(|(y, row)| {
                    (44..158).contains(&y) && !(92..107).contains(&y) && row[4..76].contains(&0x55)
                })
        );
    }
    game.advance_subframe(&assets, Steering::default()).unwrap();
    assert_eq!(game.selected_layer, 2);
    assert_eq!(game.snapshot().background_layer, 2);
}

#[test]
fn background_change_preserves_static_hud_border() {
    let mut assets = synthetic_assets();
    assets.primary.fill(0x55);
    assets.alternate.fill(0xaa);
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.alternate_background = true;
    game.advance_subframe(&assets, Steering::default()).unwrap();
    // This border is outside both the world reveal and HUD placements.
    assert_eq!(game.frame().packed()[43 * 80 + 20], 0x55);
    // The score's restore rectangle still uses the newly selected source.
    assert_eq!(game.frame().packed()[32 * 80 + 34], 0xaa);
}

#[test]
fn ordinary_pass_capture_precedes_post_body_scene_transitions() {
    let mut assets = synthetic_assets();
    assets.alternate.fill(0x55);
    let start = SessionState::default().begin_game();
    let mut game = Gameplay::for_play(&assets, synthetic_rules(&[0xd6, 0x85]), start).unwrap();
    game.capture_pass_snapshots(true);
    game.script_delay = 20;
    game.advance(&assets).unwrap();
    let pass_state = game.pass_snapshot().unwrap();
    assert_eq!(pass_state.script_delay, 20);
    assert_ne!(pass_state.script_cursor, game.snapshot().script_cursor);
    let transitions = game.take_transitions();
    assert_eq!(transitions.len(), 7);
    assert!(transitions.iter().all(|frame| !frame.before_pass));
    assert!(transitions.iter().all(|frame| frame.state.is_some()));
    assert_ne!(game.pass_frame(), game.frame());
    assert_eq!(game.frame(), &transitions.last().unwrap().frame);
    assert_eq!(game.snapshot().render_pass, 3);
}

#[test]
fn command_delay_accepts_its_boundary_without_high_difficulty_overflow() {
    let assets = synthetic_assets();
    for (difficulty, threshold) in [
        (0, 20_i16),
        (1, 12),
        (2, 4),
        (3, -4),
        (4098, -32764),
        (4099, 32764),
        (8191, 28),
        (8192, 20),
        (8195, -4),
        (12290, -32764),
        (12291, 32764),
        (32767, 28),
        (32768, 20),
        (65535, 28),
    ] {
        let threshold = if difficulty >= 3 { -4 } else { threshold };
        let mut game = Gameplay::for_play(
            &assets,
            synthetic_rules(&[0x85]),
            SessionState::default().begin_game(),
        )
        .unwrap();
        game.difficulty = difficulty;
        game.script_delay = (threshold - 1) as u16;
        game.parse_commands(&assets, &mut |_| {}).unwrap();
        assert_eq!(game.script_cursor, 0, "difficulty {difficulty}");
        assert!(!game.enemy.active());
        assert_eq!(game.script_delay, threshold as u16);
        game.parse_commands(&assets, &mut |_| {}).unwrap();
        assert_eq!(game.script_cursor, 1, "difficulty {difficulty}");
        assert!(game.enemy.active());
        assert_eq!(game.script_delay, 0);
        assert_eq!(game.difficulty, difficulty);
    }
}

#[test]
fn stage_completion_wraps_difficulty_and_resumes_with_first_visual_family() {
    let assets = synthetic_assets();
    let mut data = synthetic_rules(&[0x9b, 0x85]).data_segment().to_vec();
    data[0x04ac + 9 * 3..0x04ac + 10 * 3].copy_from_slice(&[184, 185, 186]);
    let rules = ScriptRules::from_data_segment(&data).unwrap();
    let mut game =
        Gameplay::for_play(&assets, rules, SessionState::default().begin_game()).unwrap();
    game.difficulty = u16::MAX;
    assert_eq!(
        game.enemy_context().unwrap().patrol_sources,
        Some([184, 185, 186])
    );
    game.script_delay = 28;
    game.parse_commands(&assets, &mut |_| {}).unwrap();
    assert_eq!(game.difficulty, 0);
    assert_eq!(game.script_cursor, 1);
    assert_eq!(game.enemy.snapshots()[1].movement, 27);
    assert_eq!(
        game.enemy_context().unwrap().patrol_sources,
        Some([1, 2, 3])
    );
    for _ in 0..8 {
        assert!(game.advance(&assets).unwrap());
        assert!((1..=3).contains(&game.snapshot().hud.marker_requested));
        assert_eq!(game.difficulty, 0);
    }
    // The next accepted stage, rather than a fixture assignment, advances 0→1.
    game.enemy.clear_primary();
    game.script_cursor = 0;
    game.script_delay = 20;
    game.parse_commands(&assets, &mut |_| {}).unwrap();
    assert_eq!(game.difficulty, 1);
}

#[test]
fn uninterrupted_stage_sequence_continues_through_visual_family_limit() {
    let assets = synthetic_assets();
    let mut data = synthetic_rules(&[0x9b]).data_segment().to_vec();
    for family in 0..10 {
        data[0x04ac + family * 3..0x04af + family * 3].copy_from_slice(&[1, 2, 3]);
    }
    let rules = ScriptRules::from_data_segment(&data).unwrap();
    let mut game =
        Gameplay::for_play(&assets, rules, SessionState::default().begin_game()).unwrap();
    game.difficulty = 9;
    let mut reached = vec![9];
    for _ in 0..1600 {
        assert!(
            game.advance_subframe(
                &assets,
                Steering {
                    up: true,
                    ..Steering::default()
                }
            )
            .unwrap()
        );
        if reached.last() != Some(&game.difficulty) {
            reached.push(game.difficulty);
        }
        if game.difficulty == 12 {
            break;
        }
    }
    assert_eq!(reached, [9, 10, 11, 12]);
    assert!(game.progression >= 3);
    assert!(!game.complete());
}

#[test]
fn high_difficulty_keeps_gameplay_rules_and_reuses_valid_hud_and_patrol_artwork() {
    let assets = synthetic_assets();
    let mut data = synthetic_rules(&[0x85]).data_segment().to_vec();
    data[0x04ac + 9 * 3..0x04ac + 10 * 3].copy_from_slice(&[184, 185, 186]);
    for choice in 0..3 {
        let offset = 9 * 6 + choice * 2;
        data[0x04ca + offset..0x04cc + offset].copy_from_slice(&(choice as i16 + 1).to_le_bytes());
        data[0x0506 + offset..0x0508 + offset].copy_from_slice(&(choice as i16 + 4).to_le_bytes());
    }
    let rules = ScriptRules::from_data_segment(&data).unwrap();
    // The decoder still exposes the original bad entries for inspection.
    assert_eq!(rules.patrol_sources(10).unwrap(), [0; 3]);
    for difficulty in [9, 10, 11, 12, 255, u16::MAX] {
        let mut game =
            Gameplay::for_play(&assets, rules.clone(), SessionState::default().begin_game())
                .unwrap();
        game.difficulty = difficulty;
        let context = game.enemy_context().unwrap();
        assert_eq!(context.difficulty, difficulty);
        assert_eq!(context.patrol_sources, Some([184, 185, 186]));
        game.enemy
            .spawn_command_with_context(0xa2, &mut game.random, [14, 0, -100], 3, context)
            .unwrap();
        for _ in 0..8 {
            assert!(game.advance(&assets).unwrap());
            let state = game.snapshot();
            assert_eq!(state.difficulty, difficulty);
            let choice = usize::from(state.hud.marker_requested - 184);
            assert!(choice < 3);
            assert_eq!(state.hud.marker_x, 37 - choice as i32);
            assert_eq!(state.hud.marker_y, 8 + choice as i32);
        }
    }
}

#[test]
fn live_play_passes_export_limits_wraps_the_counter_and_obeys_game_over() {
    let assets = synthetic_assets();
    let start = SessionState::default().begin_game();
    let mut game = Gameplay::for_play(&assets, synthetic_rules(&[0x85]), start).unwrap();
    for boundary in [
        DEFAULT_CONTROLLED_EXPORT_BODY_LIMIT,
        DEFAULT_EXPORT_BODY_LIMIT,
    ] {
        game.body = boundary - 1;
        for expected in [boundary, boundary + 1] {
            assert!(game.advance(&assets).unwrap());
            assert_eq!(game.snapshot().body, expected);
            assert!(!game.complete());
        }
    }
    game.body = u16::MAX;
    assert!(game.advance_subframe(&assets, Steering::default()).unwrap());
    assert_eq!(game.snapshot().body, 0);
    game.request_game_over();
    assert!(!game.complete());
    for _ in 0..2 {
        assert!(game.advance_subframe(&assets, Steering::default()).unwrap());
    }
    assert!(game.complete());
    assert!(!game.advance(&assets).unwrap());
}

#[test]
fn new_game_reprojects_reset_shots_and_preserves_unused_enemy_coordinates() {
    let assets = synthetic_assets();
    let previous = Gameplay::test_game(&assets).unwrap();
    let ending = Ending::new(&assets, &previous.snapshot(), previous.frame()).unwrap();
    let mut session = SessionState::default();
    session.remember_ending(&ending.snapshot(), ending.special_snapshots());
    let world = session.retained_world.as_mut().unwrap();
    world.records[5].depth = 692;
    for shot in &mut world.records[2..4] {
        shot.requested_sprite = 23;
        shot.resolved_sprite = 23;
        shot.rect.x_bytes = 17;
    }
    let start = session.begin_game();
    let expected_random = start.random;
    let game = Gameplay::from_game_start(&assets, synthetic_rules(&[0x85]), start, 90).unwrap();
    let state = game.snapshot();
    assert_eq!(state.random, expected_random);
    assert_eq!(state.enemies[1].depth, 692);
    assert!(!state.enemies[1].active);
    for shot in state.weapon.shots {
        assert_eq!(shot.requested_sprite, 22);
        assert_eq!(shot.resolved_sprite, 22);
        assert_eq!(shot.rect.x_bytes, 128);
        assert!(!shot.active);
    }
}

#[test]
fn external_parser_preserves_delay_and_continues_after_non_wave_commands() {
    let assets = synthetic_assets();
    let mut game = Gameplay::from_game_start(
        &assets,
        synthetic_rules(&[1, 0xc5, 0x85]),
        GameStart {
            script_index: 2,
            random: [0; 3],
            retained_world: None,
        },
        90,
    )
    .unwrap();
    for _ in 0..20 {
        game.advance(&assets).unwrap();
    }
    assert_eq!((game.script_cursor, game.terrain_selector), (0, 0));
    game.advance(&assets).unwrap();
    assert_eq!(
        (game.script_cursor, game.terrain_selector, game.script_delay),
        (1, 1, 1)
    );
    for _ in 0..20 {
        game.advance(&assets).unwrap();
    }
    assert_eq!(game.speed_mode, 1);
    assert_eq!(game.speeds, [28, 22, 16]);
    game.advance(&assets).unwrap();
    assert_eq!(game.speeds, [29, 23, 17]);
    assert_eq!(game.script_index, 2);
}

#[test]
fn negative_delay_can_execute_several_commands_and_follow_both_edges() {
    let assets = synthetic_assets();
    let mut game = Gameplay::from_game_start(
        &assets,
        synthetic_rules(&[1, 0xc5, 0x85]),
        GameStart {
            script_index: 2,
            random: [0; 3],
            retained_world: None,
        },
        90,
    )
    .unwrap();
    game.difficulty = 3;
    game.parse_commands(&assets, &mut |_| {}).unwrap();
    assert_eq!(game.script_cursor, 3);
    assert!(game.enemy.active());
    assert_eq!(game.speed_mode, 1);
    game.follow_edge(true).unwrap();
    assert_eq!(
        (game.script_index, game.script_cursor, game.progression),
        (2, 0, 2)
    );
    assert!(!game.enemy.active());
    game.follow_edge(false).unwrap();
    assert_eq!(
        (game.script_index, game.script_cursor, game.progression),
        (3, 0, 3)
    );
}

#[test]
fn speed_limits_and_deceleration_keep_the_three_speeds_in_step() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.speed_mode = 1;
    for _ in 0..40 {
        game.advance_speed();
    }
    assert_eq!(game.speeds, [56, 50, 44]);
    game.speed_mode = 2;
    for _ in 0..14 {
        game.advance_speed();
    }
    assert_eq!(game.speeds, [28, 22, 16]);
    game.advance_speed();
    assert_eq!(game.speed_mode, 0);
    game.request_game_over();
    assert!(game.complete());
}
#[test]
fn weapon_hits_destroy_craft_pay_score_and_leave_other_slots_available() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.enemy
        .spawn_command(0x84, &mut game.random, 0, 1)
        .unwrap();
    let target = game.enemy.snapshots()[1];
    let mut shots = game.weapon.snapshot().shots;
    shots[0].active = true;
    shots[0].depth = target.depth;
    shots[0].lateral = target.lateral;
    shots[0].height = target.height;
    game.weapon.restore(shots);
    game.collide_weapon(&assets).unwrap();
    assert!(!game.weapon.snapshot().shots[0].active);
    assert_eq!(game.enemy.snapshots()[1].movement, 3);
    assert_eq!(game.hud.snapshot().pending_score, 1500);
    game.collide_weapon(&assets).unwrap();
    assert_eq!(game.hud.snapshot().pending_score, 1500);
}

#[test]
fn weapon_terrain_conversion_does_not_consume_the_projectile_or_collect_fuel() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.terrain.spawn_goal_barrier(28, 0);
    let target = game.terrain.snapshots()[0];
    let mut shots = game.weapon.snapshot().shots;
    shots[0].active = true;
    shots[0].depth = target.depth;
    shots[0].lateral = target.lateral;
    shots[0].height = -100;
    game.weapon.restore(shots);
    let fuel = game.hud.snapshot().fuel;
    game.collide_weapon(&assets).unwrap();
    assert!(game.weapon.snapshot().shots[0].active);
    assert_eq!(game.terrain.snapshots()[0].movement, 3);
    assert_eq!(game.hud.snapshot().pending_score, 900);
    assert_eq!(game.hud.snapshot().fuel, fuel);
}
#[test]
fn extra_life_awards_fill_only_missing_symbols_and_terrain_awards_cap_separately() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.apply_enemy_effects(enemy::EnemyEffects {
        lives: 1,
        difficulty: 1,
        ..Default::default()
    });
    assert_eq!((game.player.snapshot().lives, game.difficulty), (6, 1));
    game.terrain.spawn_goal_barrier(28, 0);
    let target = game.terrain.snapshots()[0];
    let mut shots = game.weapon.snapshot().shots;
    shots[0].active = true;
    shots[0].depth = target.depth;
    shots[0].lateral = target.lateral;
    shots[0].height = -100;
    game.weapon.restore(shots);
    game.hud.add_points(31_900);
    game.collide_weapon(&assets).unwrap();
    assert_eq!(game.hud.snapshot().pending_score, 32_000);
}
#[test]
fn sound_execution_leaves_sampled_gameplay_and_pixels_unchanged() {
    use crate::{
        sound::{GameplayAction, ProceduralProgram, ProceduralState},
        timer::{Clock, Progress, Wait},
    };
    use reforge_audio::speaker::PcSpeaker;
    let mut image = vec![0; 0x78c6];
    for (anchor, base, words) in [
        (0x7541, 0x7583, 17),
        (0x762a, 0x766c, 47),
        (0x774d, 0x778c, 14),
        (0x7833, 0x7872, 42),
    ] {
        image[anchor..anchor + 6].copy_from_slice(&[
            0xd1,
            0xe0,
            0x8d,
            0x1e,
            base as u8,
            (base >> 8) as u8,
        ]);
        for i in 0..words {
            image[base + 2 * i..base + 2 * i + 2].copy_from_slice(&32u16.to_le_bytes());
        }
    }
    let program = ProceduralProgram::from_expanded(&image).unwrap();
    let mut state = ProceduralState::default();
    let mut clock = Clock::new(
        PcSpeaker::new(0, [true; 3]),
        [Default::default(); 2],
        true,
        |_, _| {},
    );
    state.request(Effect::Reset, 0x1234, 0, &mut clock);
    let assets = synthetic_assets();
    let mut plain = Gameplay::test_game(&assets).unwrap();
    let mut active = Gameplay::test_game(&assets).unwrap();
    for pass in 0..60 {
        let wait = Wait::from_call(1, 1);
        while !wait.poll(&mut state.counters) {
            let target = clock.tick() + 10000;
            if let Progress::Interrupt(irq) = clock.advance_to(target).unwrap() {
                state
                    .dispatch(irq, &program, 0x1234, 0, &mut clock)
                    .unwrap();
            }
        }
        let input = Steering {
            left: pass < 15,
            right: (20..35).contains(&pass),
            fire: pass % 4 != 0,
            ..Steering::default()
        };
        plain.advance_subframe(&assets, input).unwrap();
        active
            .advance_subframe_with_sound(&assets, input, &mut |action| match action {
                GameplayAction::Request { effect, .. } => {
                    state.request(effect, 0x1234, 0, &mut clock);
                }
                _ => panic!("unexpected blocking operation in this prefix"),
            })
            .unwrap();
        clock.check().unwrap();
        assert_eq!(plain.frame(), active.frame());
        assert_eq!(
            format!("{:?}", plain.snapshot()),
            format!("{:?}", active.snapshot())
        );
    }
    assert_ne!(state.noise.seed_bytes, [0; 4]);
    assert!(clock.tick() > 0);
}

#[test]
fn repeated_gate_contacts_emit_individual_tones_before_body_noise() {
    let mut assets = synthetic_assets();
    assets.sprites[13].descriptor[2] = 100;
    let mut game = Gameplay::test_game(&assets).unwrap();
    let mut records = game.special.snapshots();
    for record in &mut records {
        record.active = true;
        record.depth = 60;
        record.coordinates = [60, -100, -200];
        record.lateral = -100;
        record.height = -200;
        record.extents = [50, 200, 300];
        record.movement = 1;
        record.requested_sprite = 0;
    }
    game.special = special::Special::from_snapshots(records);
    let mut actions = Vec::new();
    game.advance_subframe_with_sound(&assets, Steering::default(), &mut |a| actions.push(a))
        .unwrap();
    assert_eq!(
        &actions[..5],
        vec![
            sound::GameplayAction::Request {
                site: 0x37a3,
                effect: Effect::Tone(Tone::First)
            };
            5
        ]
    );
    assert!(matches!(
        actions[5],
        sound::GameplayAction::Request { site: 0x067b, .. }
    ));
}

#[test]
fn a_boundary_exit_requests_default_once_and_a_bounded_export_does_not() {
    let assets = synthetic_assets();
    let mut game = Gameplay::test_game(&assets).unwrap();
    game.request_game_over();
    let mut actions = Vec::new();
    for _ in 0..2 {
        assert!(
            !game
                .advance_subframe_with_sound(&assets, Steering::default(), &mut |a| actions.push(a))
                .unwrap()
        );
    }
    assert_eq!(
        actions,
        [sound::GameplayAction::Request {
            site: 0x0452,
            effect: Effect::Reset
        }]
    );
    let mut bounded = Gameplay::test_game(&assets).unwrap();
    bounded.body_limit = Some(0);
    bounded
        .advance_subframe_with_sound(&assets, Steering::default(), &mut |a| actions.push(a))
        .unwrap();
    assert_eq!(actions.len(), 1);
}
