// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::assets::Sprite;

fn assets() -> GraphicsAssets {
    GraphicsAssets {
        primary: vec![],
        alternate: vec![],
        sprites: (0..190)
            .map(|_| Sprite {
                width_bytes: 4,
                height: 7,
                descriptor: [1, 1, 1, 4, 7, 2, 5],
                color: vec![0; 28],
                mask: vec![255; 28],
            })
            .collect(),
    }
}

#[test]
fn seeking_precedes_target_update_and_shadow_observes_old_depth() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 800;
    enemy.crafts[0].lateral = 174;
    enemy.crafts[0].height = 91;
    enemy.crafts[0].phase = 3;
    enemy.shadows[0].active = true;
    enemy.bullet.active = true;
    enemy.bullet.depth = 900;
    enemy.bullet.lateral = 174;
    enemy.bullet.height = 91;
    enemy.target = [10, -100];
    let mut random = Random::default();
    enemy
        .advance_half_step(&assets(), &mut random, true)
        .unwrap();
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (872, 94, 11)
    );
    assert_eq!(enemy.crafts[0].depth, 778);
    assert_eq!(enemy.shadows[0].depth, 800);
    assert_eq!(enemy.crafts[0].requested_sprite, 35);
    assert_eq!(enemy.crafts[0].resolved_sprite, 35);
    assert_eq!(random.next_word(), 0x4293);
}

#[test]
fn second_half_firing_retains_the_previous_projected_source() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 800;
    let mut random = Random::default();
    enemy
        .advance_half_step(&assets(), &mut random, true)
        .unwrap();
    assert!(!enemy.bullet.active);
    enemy
        .advance_half_step(&assets(), &mut random, false)
        .unwrap();
    assert!(enemy.bullet.active);
    assert_eq!(enemy.bullet.depth, 778);
    assert_eq!(enemy.bullet.resolved_sprite, 22);
    assert_eq!(enemy.bullet.rect.x_bytes, 128);
}
#[test]
fn retirement_precedes_collision_and_preserves_craft_source() {
    let assets = assets();
    let mut enemy = Enemy::new(&assets).unwrap();
    enemy.bullet.active = true;
    enemy.bullet.depth = 42;
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 36;
    enemy.shadows[0].active = true;
    enemy.update_half_step(&mut Random::default(), [14, 0, -100]);
    assert!(!enemy.bullet.active);
    assert_eq!(enemy.bullet.requested_sprite, 0);
    assert!(!enemy.crafts[0].active);
    assert!(!enemy.shadows[0].active);
    assert_eq!(enemy.crafts[0].requested_sprite, 31);
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 0, true)
            .unwrap(),
        [None; 6]
    );
    enemy.project(&assets, false, 106, 0).unwrap();
    assert_eq!(enemy.bullet.resolved_sprite, 0);
    assert_eq!(enemy.crafts[0].resolved_sprite, 31);
    assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
}

#[test]
fn retired_projectile_can_be_refired_in_the_same_slot_pass() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    enemy.bullet.active = true;
    enemy.bullet.depth = 42;
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 100;
    enemy.crafts[0].lateral = 200;
    enemy.crafts[0].height = 50;
    let mut random = Random::default();
    random.next_word();
    random.next_word();
    random.next_word(); // Next word is odd, so the craft fires.
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert!(enemy.bullet.active);
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (100, 200, 50)
    );
    assert_eq!(enemy.bullet.requested_sprite, 0);
    assert_eq!(enemy.crafts[0].depth, 78);
}

#[test]
fn near_projection_has_a_strict_depth_gate_and_resolves_inactive_sources() {
    let assets = assets();
    let mut enemy = Enemy::new(&assets).unwrap();
    enemy.bullet.requested_sprite = 65;
    enemy.bullet.depth = 70;
    enemy.project(&assets, false, 106, 0).unwrap();
    assert_eq!(enemy.bullet.resolved_sprite, 22);
    enemy.bullet.depth = 69;
    enemy.project(&assets, false, 106, 0).unwrap();
    assert_eq!(enemy.bullet.resolved_sprite, 65);
    assert_eq!(enemy.bullet.rect.x_bytes, 128);
}

#[test]
fn collision_uses_previous_source_and_inclusive_target_height_extent() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    assets.sprites[64].descriptor[..3].copy_from_slice(&[1, 1, 1]);
    assets.sprites[65].descriptor[..3].copy_from_slice(&[100, 100, 100]);
    let mut enemy = Enemy::new(&assets).unwrap();
    enemy.bullet.active = true;
    enemy.bullet.depth = 29;
    enemy.bullet.lateral = 128;
    enemy.bullet.height = -99;
    enemy.bullet.resolved_sprite = 64;
    enemy.bullet.requested_sprite = 65;
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [
            Some(EnemyCollision::Projectile),
            None,
            None,
            None,
            None,
            None
        ]
    );
    assert!(enemy.bullet.active);
    enemy.bullet.depth = 30;
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [None; 6]
    );
    enemy.bullet.depth = 29;
    enemy.bullet.height = -98;
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [None; 6]
    );
    enemy.bullet.height = -101;
    enemy.bullet.lateral = -1;
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [
            Some(EnemyCollision::Projectile),
            None,
            None,
            None,
            None,
            None
        ]
    );
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, false)
            .unwrap(),
        [None; 6]
    );
}

#[test]
fn primary_spawn_commands_consume_only_their_required_random_words() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x84, &mut random, 123, 1).unwrap();
    assert_eq!(random.state(), [0; 3]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (1600, 0, 0)
    );
    enemy.spawn_command(0x91, &mut random, 123, 1).unwrap();
    assert_eq!(enemy.crafts[0].lateral, (0x5630 & 1023) - 512 - 246);
    assert_eq!(enemy.crafts[0].height, (0xbe90_u16 & 255) as i16 - 50);
    assert_eq!(random.next_word(), 0x6338);
}
#[test]
fn seeking_craft_fires_and_copies_shadow_before_steering_deltas() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
    enemy.crafts[0].lateral = 71;
    for _ in 0..3 {
        random.next_word();
    }
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (41, -30));
    assert_eq!((enemy.bullet.lateral, enemy.bullet.height), (71, 0));
    assert_eq!(enemy.bullet.depth, 1600);
    assert_eq!(enemy.shadows[0].lateral, 71);
    for _ in 0..7 {
        enemy.update_half_step(&mut random, [14, 0, -100]);
    }
    assert_eq!(enemy.crafts[0].height, -100);
}

#[test]
fn craft_explosion_retains_source_then_animates_without_movement_or_random() {
    let assets = assets();
    let mut enemy = Enemy::new(&assets).unwrap();
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 16;
    enemy.crafts[0].height = -100;
    enemy.crafts[0].requested_sprite = 33;
    enemy.crafts[0].resolved_sprite = 33;
    enemy.shadows[0].active = true;
    enemy.explode_craft(5).unwrap();
    assert_eq!(enemy.crafts[0].requested_sprite, 33);
    assert!(!enemy.shadows[0].active);
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 0, true)
            .unwrap(),
        [None; 6]
    );
    let mut random = Random::default();
    for source in [69, 69, 61, 61, 69, 69, 67, 67, 65, 65, 64, 64] {
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert!(enemy.crafts[0].active);
        assert_eq!(enemy.crafts[0].requested_sprite, source);
        assert_eq!((enemy.crafts[0].depth, enemy.crafts[0].height), (16, -100));
        assert_eq!(random.state(), [0; 3]);
    }
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert!(!enemy.crafts[0].active);
    assert_eq!(enemy.crafts[0].requested_sprite, 64);
    enemy.project(&assets, false, 106, 0).unwrap();
    assert_eq!(enemy.crafts[0].rect.x_bytes, 128);
}

#[test]
fn orbit_changes_lateral_before_firing_but_defers_height_until_after_firing() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x8e, &mut random, 50, 1).unwrap();
    assert_eq!(random.state(), [0; 3]);
    enemy.crafts[0].depth = 1024;
    for _ in 0..3 {
        random.next_word();
    }
    enemy.update_half_step(&mut random, [14, 50, -28]);
    assert_eq!(enemy.orbit_center, [9, -91]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (1002, 9, 85)
    );
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (1024, 9, 0)
    );
    assert_eq!(enemy.shadows[0].lateral, 9);
    assert!((18..=81).contains(&enemy.target[0]));
    assert!((-60..=3).contains(&enemy.target[1]));
}

#[test]
fn orbit_switches_to_seeking_only_below_the_old_depth_threshold() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 240;
    enemy.update_half_step(&mut random, [14, 50, -28]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (218, 185, -75)
    );
    enemy.update_half_step(&mut random, [14, 50, -28]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (196, 155, -52)
    );
    assert_eq!(enemy.orbit_center, [9, -91]);
    enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
    assert_eq!(enemy.orbit_center, [9, -91]);
}

#[test]
fn recovered_player_coordinates_drive_seeking_targets_and_retirement() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x84, &mut random, 0, 1).unwrap();
    enemy.update_half_step(&mut random, [28, 100, -28]);
    assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (30, -14));
    assert!((68..=131).contains(&enemy.target[0]));
    assert!((-60..=3).contains(&enemy.target[1]));
    enemy.crafts[0].active = false;
    enemy.bullet.active = true;
    enemy.bullet.depth = 56;
    enemy.update_half_step(&mut random, [28, 100, -28]);
    assert!(!enemy.bullet.active);
    assert_eq!(enemy.bullet.depth, 28);
}
#[test]
fn orbit_height_clamps_before_the_world_delta_is_applied() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x8e, &mut random, 0, 1).unwrap();
    enemy.orbit_center[1] = 200;
    enemy.crafts[0].depth = 1024;
    enemy.update_half_step(&mut random, [14, 0, 230]);
    assert_eq!(enemy.orbit_center[1], 209);
    assert_eq!(enemy.crafts[0].height, 250);
    enemy.orbit_center[1] = -100;
    enemy.crafts[0].depth = 512;
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!(enemy.crafts[0].height, -100);
}
#[test]
fn lateral_orbit_preserves_height_and_does_not_seek_at_near_depth() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
    assert_eq!(random.state(), [0; 3]);
    enemy.crafts[0].depth = 128;
    enemy.update_half_step(&mut random, [14, 50, -28]);
    assert_eq!(enemy.orbit_center, [9, -91]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (106, 137, 0)
    );
    enemy.update_half_step(&mut random, [14, 50, -28]);
    assert_eq!(enemy.orbit_center, [18, -82]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (84, 114, 0)
    );
}

#[test]
fn lateral_orbit_firing_observes_the_new_lateral_and_preserved_height() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.orbit_center = [0, -28];
    enemy.spawn_command(0x8a, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 38;
    for _ in 0..3 {
        random.next_word();
    }
    enemy.update_half_step(&mut random, [14, 0, -28]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (16, 32, 0)
    );
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (38, 32, 0)
    );
    assert_eq!((enemy.shadows[0].depth, enemy.shadows[0].lateral), (38, 32));
    assert!(enemy.crafts[0].active);
}
#[test]
fn collision_candidates_preserve_slot_order_for_grace_gated_damage() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    let mut enemy = Enemy::new(&assets).unwrap();
    for slot in [&mut enemy.bullet, &mut enemy.crafts[0]] {
        slot.active = true;
        slot.depth = 29;
        slot.lateral = 128;
        slot.height = -99;
    }
    enemy.bullet.resolved_sprite = 64;
    enemy.crafts[0].resolved_sprite = 33;
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [
            Some(EnemyCollision::Projectile),
            Some(EnemyCollision::Craft(5)),
            None,
            None,
            None,
            None
        ]
    );
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, false)
            .unwrap(),
        [None; 6]
    );
}
#[test]
fn formation_spawn_staggers_depth_and_retains_cached_projection() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.crafts[1].resolved_sprite = 32;
    enemy.crafts[1].rect.y = 71;
    enemy.spawn_command(0x88, &mut random, 100, 2).unwrap();
    assert_eq!(random.state(), [0; 3]);
    assert_eq!(enemy.crafts.map(|c| c.depth), [1600, 1500, 0, 0, 0]);
    assert_eq!(
        enemy.crafts.map(|c| c.active),
        [true, true, false, false, false]
    );
    assert_eq!(
        enemy.shadows.map(|c| c.active),
        [true, true, false, false, false]
    );
    assert_eq!(enemy.crafts[1].requested_sprite, 0);
    assert_eq!(enemy.crafts[1].movement, 8);
    assert_eq!(enemy.crafts[1].resolved_sprite, 32);
    assert_eq!(enemy.crafts[1].rect.y, 71);
    assert_eq!(
        enemy.snapshots().map(|c| c.slot),
        [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    );
    enemy.spawn_command(0x88, &mut random, 100, 20).unwrap();
    assert_eq!(
        enemy.crafts.map(|c| c.depth),
        [1600, 1500, 1400, 1300, 1200]
    );
}

#[test]
fn full_orbit_keeps_vertical_motion_below_the_seeking_threshold() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.orbit_center = [0, -28];
    enemy.spawn_command(0x88, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 128;
    enemy.update_half_step(&mut random, [14, 0, -28]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (106, 128, 100)
    );
}

#[test]
fn formation_updates_shared_center_and_random_target_in_slot_order() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.spawn_command(0x88, &mut random, 0, 2).unwrap();
    enemy.crafts[0].depth = 1024;
    enemy.crafts[1].depth = 512;
    enemy.bullet.active = true;
    enemy.bullet.depth = 600;
    enemy.update_half_step(&mut random, [14, 50, -28]);
    for _ in 0..4 {
        expected.next_word();
    }
    let lateral = 50 + (expected.next_word() & 63) as i16 - 32;
    let height = -28 + (expected.next_word() & 63) as i16 - 32;
    assert_eq!(random.state(), expected.state());
    assert_eq!(enemy.target(), [lateral, height]);
    assert_eq!(enemy.orbit_center, [18, -82]);
    assert_eq!((enemy.crafts[0].lateral, enemy.crafts[0].height), (9, 85));
    assert_eq!(
        (enemy.crafts[1].lateral, enemy.crafts[1].height),
        (18, -100)
    );
    assert_eq!(
        (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
        (9, 18)
    );
}

#[test]
fn secondary_explosion_preserves_other_craft_and_parser_activity() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
    enemy.crafts[1].depth = 16;
    enemy.explode_craft(6).unwrap();
    assert_eq!(enemy.crafts[1].movement, 3);
    assert_eq!(enemy.crafts[0].movement, 4);
    assert!(enemy.shadows[0].active);
    assert!(!enemy.shadows[1].active);
    for _ in 0..13 {
        enemy.update_half_step(&mut random, [14, 0, -100]);
    }
    assert!(!enemy.crafts[1].active);
    assert!(enemy.crafts[0].active);
    assert!(enemy.active());
    assert_eq!(enemy.shadow_draws().count(), 1);
    enemy.crafts[0].active = false;
    assert!(!enemy.active());
    assert!(enemy.explode_craft(4).is_err());
    assert!(enemy.explode_craft(10).is_err());
}
#[test]
fn later_craft_can_fire_into_shared_slot_before_its_steering_delta() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.spawn_command(0x84, &mut random, 0, 2).unwrap();
    enemy.crafts[0].depth = 800;
    enemy.crafts[0].lateral = 200;
    enemy.crafts[0].height = 50;
    enemy.crafts[1].depth = 700;
    enemy.crafts[1].lateral = -40;
    enemy.crafts[1].height = -10;
    assert_eq!(expected.next_word() & 1, 0);
    expected.next_word();
    expected.next_word();
    assert_eq!(expected.next_word() & 1, 1);
    let lateral = (expected.next_word() & 63) as i16 - 32;
    let height = -100 + (expected.next_word() & 63) as i16 - 32;
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert!(enemy.bullet.active);
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (700, -40, -10)
    );
    assert_eq!(
        (
            enemy.crafts[1].depth,
            enemy.crafts[1].lateral,
            enemy.crafts[1].height
        ),
        (678, -20, -40)
    );
    assert_eq!(enemy.bullet.requested_sprite, 22);
    assert_eq!(enemy.target(), [lateral, height]);
    assert_eq!(random.state(), expected.state());
}

#[test]
fn all_primary_collision_candidates_identify_slots_and_skip_explosions() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    let mut enemy = Enemy::new(&assets).unwrap();
    for slot in std::iter::once(&mut enemy.bullet).chain(enemy.crafts.iter_mut()) {
        slot.active = true;
        slot.depth = 29;
        slot.lateral = 128;
        slot.height = -99;
        slot.resolved_sprite = 33;
    }
    enemy.explode_craft(7).unwrap();
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [
            Some(EnemyCollision::Projectile),
            Some(EnemyCollision::Craft(5)),
            Some(EnemyCollision::Craft(6)),
            None,
            Some(EnemyCollision::Craft(8)),
            Some(EnemyCollision::Craft(9)),
        ]
    );
}

#[test]
fn primary_contact_dispatch_accepts_original_variants_and_rejects_invalid_bytes() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    let mut enemy = Enemy::new(&assets).unwrap();
    let craft = &mut enemy.crafts[0];
    craft.active = true;
    craft.depth = 29;
    craft.height = -99;
    craft.lateral = 128;
    craft.resolved_sprite = 33;
    for variant in 0..=u8::MAX {
        enemy.crafts[0].variant = variant;
        let expected = if [0, 1, 2, 5, 6].contains(&variant) {
            Some(EnemyCollision::Craft(5))
        } else {
            None
        };
        assert_eq!(
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, true)
                .unwrap()[1],
            expected,
            "variant {variant}"
        );
    }
}

#[test]
fn boss_and_patrol_contact_keep_geometry_activity_explosion_and_phase_guards() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    assets.sprites[33].descriptor[..3].copy_from_slice(&[1, 1, 1]);
    assets.sprites[65].descriptor[..3].copy_from_slice(&[100, 100, 100]);
    for variant in [5, 6] {
        let mut enemy = Enemy::new(&assets).unwrap();
        enemy.crafts[0].active = true;
        enemy.crafts[0].variant = variant;
        enemy.crafts[0].depth = 29;
        enemy.crafts[0].height = -99;
        enemy.crafts[0].lateral = 128;
        enemy.crafts[0].resolved_sprite = 33;
        enemy.crafts[0].requested_sprite = 65;
        let contact = |enemy: &Enemy, collidable| {
            enemy
                .player_collisions(&assets, [14, 0, -100], 13, collidable)
                .unwrap()[1]
        };
        assert_eq!(contact(&enemy, true), Some(EnemyCollision::Craft(5)));
        assert_eq!(contact(&enemy, false), None);
        enemy.crafts[0].depth = 30;
        assert_eq!(contact(&enemy, true), None);
        enemy.crafts[0].depth = 29;
        enemy.crafts[0].height = -98;
        assert_eq!(contact(&enemy, true), None); // Preceding source, not request 65.
        enemy.crafts[0].height = -99;
        enemy.crafts[0].active = false;
        assert_eq!(contact(&enemy, true), None);
        enemy.crafts[0].active = true;
        enemy.crafts[0].movement = 251;
        assert_eq!(contact(&enemy, true), None);
        enemy.crafts[0].movement = 31;
        enemy.explode_craft(5).unwrap();
        assert_eq!(contact(&enemy, true), None);
    }
}

#[test]
fn normal_boss_and_patrol_lifecycle_depths_remain_outside_player_contact() {
    let mut assets = assets();
    assets.sprites[13].descriptor[2] = 15;
    for command in [0x9f, 0xa0, 0xa1, 0xa2] {
        for speed_policy in [22, 50, 0] {
            let mut enemy = Enemy::new(&assets).unwrap();
            let mut random = Random::default();
            let mut context = EnemyContext {
                difficulty: 2,
                patrol_sources: Some([178, 179, 180]),
                ..EnemyContext::default()
            };
            enemy
                .spawn_command_with_context(command, &mut random, [14, 0, -100], 4, context)
                .unwrap();
            let mut retired = false;
            for step in 0..400 {
                context.speeds[1] = if speed_policy == 0 {
                    22 + step % 29
                } else {
                    speed_policy
                };
                enemy
                    .update_half_step_with_context(&mut random, [14, 0, -100], context)
                    .unwrap();
                for craft in enemy.crafts.iter().filter(|craft| craft.active) {
                    if command == 0xa2 {
                        assert_eq!(craft.depth, 188);
                    } else {
                        assert!(craft.depth >= 150);
                    }
                }
                assert!(
                    enemy
                        .player_collisions(&assets, [14, 0, -100], 13, true)
                        .unwrap()[1..]
                        .iter()
                        .all(Option::is_none)
                );
                if command != 0xa2 && !enemy.crafts.iter().any(|craft| craft.active) {
                    assert!(enemy.shadows.iter().all(|shadow| !shadow.active));
                    retired = true;
                    break;
                }
            }
            assert!(command == 0xa2 || retired);
        }
    }
}
#[test]
fn sweep_initializes_variant_one_but_only_the_first_primary_is_active() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.crafts[0].resolved_sprite = 33;
    enemy.crafts[0].rect.y = 51;
    enemy.spawn_command(0x97, &mut random, 100, 3).unwrap();
    assert_eq!(random.state(), [0; 3]);
    assert_eq!(enemy.crafts.map(|c| c.depth), [1000, 1000, 1000, 0, 0]);
    assert_eq!(
        enemy.crafts.map(|c| c.active),
        [true, false, false, false, false]
    );
    assert_eq!(
        enemy.shadows.map(|c| c.active),
        [true, false, false, false, false]
    );
    assert_eq!(enemy.crafts.map(|c| c.variant), [1, 1, 1, 0, 0]);
    assert_eq!(enemy.crafts[0].requested_sprite, 0);
    assert_eq!(enemy.crafts[0].resolved_sprite, 33);
    assert_eq!(enemy.crafts[0].rect.y, 51);
}

#[test]
fn sweep_copies_absolute_coordinates_before_firing_and_updates_shared_target() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
    enemy.crafts[0].animation_phase = 33;
    enemy.crafts[0].depth = 800;
    for _ in 0..3 {
        random.next_word();
    }
    let mut expected = random;
    assert_eq!(expected.next_word() & 1, 1);
    let target = [
        50 + (expected.next_word() & 63) as i16 - 32,
        -28 + (expected.next_word() & 63) as i16 - 32,
    ];
    enemy.update_half_step(&mut random, [14, 50, -28]);
    let craft = enemy.crafts[0];
    assert_eq!(
        (craft.depth, craft.lateral, craft.height),
        (460, -1040, 194)
    );
    assert_eq!((craft.requested_sprite, craft.animation_phase), (91, 34));
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (460, -1040, 194)
    );
    assert_eq!(
        (enemy.shadows[0].depth, enemy.shadows[0].lateral),
        (460, -1040)
    );
    assert_eq!(enemy.shadows[0].requested_sprite, 37);
    assert_eq!(enemy.target(), target);
    assert_eq!(random.state(), expected.state());
}

#[test]
fn sweep_terminal_coordinates_retire_before_collision_and_keep_the_requested_source() {
    let assets = assets();
    let mut enemy = Enemy::new(&assets).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x97, &mut random, 0, 2).unwrap();
    enemy.crafts[0].animation_phase = 77;
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert!(!enemy.active());
    assert!(!enemy.shadows[0].active);
    assert_eq!(enemy.crafts[0].depth, -50);
    assert_eq!(enemy.crafts[0].animation_phase, 0);
    assert_eq!(enemy.crafts[0].requested_sprite, 100);
    assert_eq!(
        enemy
            .player_collisions(&assets, [14, 0, -100], 13, true)
            .unwrap(),
        [None; 6]
    );
}

#[test]
fn sweep_collision_keeps_the_preceding_descriptor_until_projection() {
    let mut assets = assets();
    assets.sprites[13].descriptor[..3].copy_from_slice(&[32, 128, 15]);
    assets.sprites[90].descriptor[..3].copy_from_slice(&[40, 158, 15]);
    assets.sprites[91].descriptor[..3].copy_from_slice(&[100, 158, 15]);
    let mut enemy = Enemy::new(&assets).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x97, &mut random, 0, 1).unwrap();
    enemy.crafts[0].animation_phase = 29;
    enemy.crafts[0].resolved_sprite = 90;
    let player = [453, -1182, 120];
    enemy.update_half_step(&mut random, player);
    assert_eq!(enemy.crafts[0].requested_sprite, 91);
    assert_eq!(enemy.crafts[0].resolved_sprite, 90);
    assert_eq!(
        enemy.player_collisions(&assets, player, 13, true).unwrap(),
        [None; 6]
    );
    enemy.project(&assets, true, 106, 0).unwrap();
    assert_eq!(
        enemy.player_collisions(&assets, player, 13, true).unwrap(),
        [None, Some(EnemyCollision::Craft(5)), None, None, None, None]
    );
}
#[test]
fn relative_sweep_initializes_all_primaries_with_staggered_row_phases() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x96, &mut random, 0, 5).unwrap();
    assert_eq!(enemy.crafts.map(|c| c.animation_phase), [0, 4, 8, 12, 16]);
    assert_eq!(enemy.crafts.map(|c| c.variant), [1; 5]);
    assert_eq!(enemy.crafts.map(|c| c.depth), [1000; 5]);
    assert!(enemy.crafts.iter().all(|c| c.active && c.movement == 22));
    assert!(enemy.shadows.iter().all(|c| c.active));
    assert_eq!(random.state(), [0; 3]);
}

#[test]
fn relative_sweep_follows_center_per_craft_and_spaces_absolute_firing_coordinates() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.spawn_command(0x96, &mut random, 0, 2).unwrap();
    enemy.update_half_step(&mut random, [14, 100, -80]);
    assert_eq!(enemy.orbit_center, [8, -92]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (500, -2756, -80)
    );
    assert_eq!(
        (
            enemy.crafts[1].depth,
            enemy.crafts[1].lateral,
            enemy.crafts[1].height
        ),
        (500, -2488, -76)
    );
    assert_eq!(
        (
            enemy.crafts[0].animation_phase,
            enemy.crafts[1].animation_phase
        ),
        (1, 5)
    );
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height
        ),
        (500, -2488, -76)
    );
    assert_eq!(
        (enemy.shadows[0].lateral, enemy.shadows[1].lateral),
        (-2756, -2488)
    );
    for _ in 0..4 {
        expected.next_word();
    }
    let target = [
        100 + (expected.next_word() & 63) as i16 - 32,
        -80 + (expected.next_word() & 63) as i16 - 32,
    ];
    assert_eq!(enemy.target(), target);
    assert_eq!(random.state(), expected.state());
}

#[test]
fn negative_depth_projection_wraps_the_table_index_and_resolves_the_source() {
    let mut assets = assets();
    assets.sprites[1].descriptor[5..].fill(0);
    for (depth, lateral, height, origin, camera, expected) in [
        (-50, -160, -28, 107, 18, (38, 109, 3)),
        (-64, 128, 64, 100, 0, (41, 98, 0)),
        (-1024, 128, 64, 100, 0, (24, 132, 0)),
    ] {
        let mut slot = EnemySnapshot::new(4, 0, depth, height);
        slot.lateral = lateral;
        slot.active = true;
        slot.requested_sprite = 1;
        slot.project(&assets, origin, camera).unwrap();
        assert_eq!((slot.rect.x_bytes, slot.rect.y, slot.phase), expected);
        assert_eq!(slot.resolved_sprite, 1);
        assert_eq!((slot.rect.width_bytes, slot.rect.height), (4, 7));

        slot.active = false;
        slot.depth -= 28;
        slot.requested_sprite = 0;
        slot.project(&assets, 200, 100).unwrap();
        assert_eq!(slot.rect.x_bytes, 128);
        assert_eq!((slot.rect.y, slot.phase), (expected.1, expected.2));
        assert_eq!(slot.resolved_sprite, 0);
    }
}

#[test]
fn randomized_forward_spawn_staggers_three_craft_and_preserves_far_coordinates() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    let mut coordinates = [[0; 2]; 3];
    for pair in &mut coordinates {
        *pair = [
            (expected.next_word() & 1023) as i16 - 512 - 2 * 90,
            (expected.next_word() & 255) as i16 - 50,
        ];
    }
    enemy.spawn_command(0x92, &mut random, 90, 3).unwrap();
    for (i, pair) in coordinates.into_iter().enumerate() {
        let craft = enemy.crafts[i];
        assert_eq!((craft.lateral, craft.height), (pair[0], pair[1]));
        assert_eq!(craft.depth, 1600 - 100 * i as i16);
        assert_eq!(
            (craft.variant, craft.movement, craft.animation_phase),
            (0, 18, 0)
        );
        assert!(craft.active && enemy.shadows[i].active);
    }
    assert_eq!(random.state(), expected.state());
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!(
        enemy.crafts[..3]
            .iter()
            .map(|c| c.depth)
            .collect::<Vec<_>>(),
        [1578, 1478, 1378]
    );
    for (craft, pair) in enemy.crafts[..3].iter().zip(coordinates) {
        assert_eq!((craft.lateral, craft.height), (pair[0], pair[1]));
    }
    assert_eq!(
        enemy.shadows[..3]
            .iter()
            .map(|s| s.depth)
            .collect::<Vec<_>>(),
        [1600, 1500, 1400]
    );
    for _ in 0..9 {
        expected.next_word();
    }
    assert_eq!(random.state(), expected.state());
}
#[test]
fn forward_motion_switches_to_seeking_below_the_old_depth_threshold() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x92, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 240;
    enemy.crafts[0].lateral = 100;
    enemy.crafts[0].height = 60;
    let center = enemy.orbit_center;
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (218, 100, 60)
    );
    assert_eq!(
        (enemy.shadows[0].depth, enemy.shadows[0].lateral),
        (240, 100)
    );
    assert_eq!(enemy.orbit_center, center);
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height
        ),
        (196, 70, 30)
    );
    assert_eq!(
        (enemy.shadows[0].depth, enemy.shadows[0].lateral),
        (218, 100)
    );
    assert_eq!(enemy.orbit_center, center);
}
#[test]
fn all_enemy_commands_initialize_their_variant_and_formation() {
    for command in 0x84..=0xa2 {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        let context = EnemyContext {
            difficulty: 2,
            ..EnemyContext::default()
        };
        let effects = enemy
            .spawn_command_with_context(command, &mut random, [14, 80, -100], 5, context)
            .unwrap();
        assert!(
            enemy
                .crafts
                .iter()
                .all(|c| c.movement == command & 63 && c.requested_sprite == 0)
        );
        let variant = match command {
            0x95..=0x98 | 0x9d..=0x9e => 1,
            0x99..=0x9a => 2,
            0x9b..=0x9c => 3,
            0x9f..=0xa1 => 5,
            0xa2 => 6,
            _ => 0,
        };
        assert!(enemy.crafts.iter().all(|c| c.variant == variant));
        let expected_active = match command {
            0x95 | 0x97 | 0x99..=0x9d => 1,
            0x9f | 0xa2 => 3,
            0xa0..=0xa1 => 4,
            _ => 5,
        };
        assert_eq!(
            enemy.crafts.iter().filter(|c| c.active).count(),
            expected_active,
            "command {command:02x}"
        );
        assert_eq!(
            (effects.difficulty, effects.lives),
            if matches!(command, 0x9b | 0x9c) {
                (1, 1)
            } else {
                (0, 0)
            }
        );
    }
}

#[test]
fn world_speeds_distinguish_primary_slow_and_projectile_handlers() {
    let context = EnemyContext {
        speeds: [40, 28, 10],
        ..EnemyContext::default()
    };
    for (command, depth, height) in [
        (0x84, 1572, -30),
        (0x85, 1590, -30),
        (0x86, 1572, 0),
        (0x87, 1590, 0),
    ] {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(command, &mut random, 0, 1).unwrap();
        enemy.bullet.active = true;
        enemy.bullet.depth = 200;
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(
            (enemy.crafts[0].depth, enemy.crafts[0].height),
            (depth, height)
        );
        assert_eq!(enemy.bullet.depth, 160);
        assert_eq!(enemy.shadows[0].depth, 1600);
    }
}

#[test]
fn quarter_speed_orbits_seek_only_below_the_old_depth_threshold() {
    let context = EnemyContext {
        speeds: [40, 28, 10],
        ..EnemyContext::default()
    };
    for command in [0x89, 0x8b, 0x8d] {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(command, &mut random, 0, 1).unwrap();
        enemy.crafts[0].depth = 240;
        enemy.crafts[0].lateral = 100;
        enemy.crafts[0].height = 60;
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(enemy.crafts[0].depth, 233);
        enemy.crafts[0].lateral = 100;
        enemy.crafts[0].height = 60;
        enemy
            .update_half_step_with_context(&mut random, [14, 0, -100], context)
            .unwrap();
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height
            ),
            (205, 70, 30)
        );
    }
}

#[test]
fn vertical_orbit_preserves_lateral_and_alternate_mode_hides_shadows() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x8c, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 800;
    enemy.crafts[0].lateral = 123;
    enemy
        .update_half_step_with_context(
            &mut random,
            [14, 0, -100],
            EnemyContext {
                alternate_shadows: true,
                ..EnemyContext::default()
            },
        )
        .unwrap();
    assert_eq!(enemy.crafts[0].lateral, 123);
    assert_eq!(enemy.shadows[0].lateral, 123);
    assert!(!enemy.shadows[0].active);
    assert_ne!(enemy.crafts[0].height, 0);
}

#[test]
fn new_sweep_aliases_follow_their_center_steps_and_absolute_spacing() {
    for (command, step, stagger) in [
        (0x95, 8, false),
        (0x98, 0, true),
        (0x9d, 17, false),
        (0x9e, 6, true),
    ] {
        let mut enemy = Enemy::new(&assets()).unwrap();
        let mut random = Random::default();
        enemy.spawn_command(command, &mut random, 0, 2).unwrap();
        let mut effects = EnemyEffects::default();
        let delta = enemy
            .motion(
                0,
                &mut random,
                [14, 100, 100],
                EnemyContext::default(),
                &mut effects,
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(delta, Some([0; 3]));
        assert_eq!(enemy.orbit_center, [step, -100 + step]);
        assert_eq!(
            (
                enemy.crafts[0].depth,
                enemy.crafts[0].lateral,
                enemy.crafts[0].height,
                enemy.crafts[0].requested_sprite
            ),
            (
                500,
                -2600 + step + if stagger { -160 } else { 0 },
                16 + if step == 0 { 0 } else { -100 + step },
                90
            )
        );
        assert_eq!(enemy.crafts[1].animation_phase, if stagger { 4 } else { 0 });
    }
}

#[test]
fn falling_spawn_consumes_four_words_even_for_disabled_primaries() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.spawn_command(0x99, &mut random, 0, 3).unwrap();
    for _ in 0..12 {
        expected.next_word();
    }
    assert_eq!(random.state(), expected.state());
    assert_eq!(enemy.falling_count, 1);
    assert_eq!(enemy.crafts.map(|c| c.depth), [20, 14, 14, 0, 0]);
    assert_eq!(
        enemy.crafts[..3]
            .iter()
            .map(|c| (c.lateral, c.height))
            .collect::<Vec<_>>(),
        [(2000, 0); 3]
    );
    assert_eq!(
        enemy.crafts.map(|c| c.active),
        [true, false, false, false, false]
    );
    assert!(enemy.shadows.iter().all(|c| !c.active));
    enemy.spawn_command(0x9a, &mut random, 0, 3).unwrap();
    assert_eq!(enemy.falling_count, 2);
    assert!(enemy.crafts[1].active);
}

#[test]
fn falling_ground_recycle_reuses_depth_and_does_not_consume_a_firing_word() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.falling_movement = 26;
    enemy.crafts[0].active = true;
    enemy.crafts[0].variant = 2;
    enemy.crafts[0].movement = 251;
    enemy.crafts[0].depth = 20;
    enemy.crafts[0].height = -120;
    enemy.crafts[0].animation_phase = 12;
    let height = (expected.next_word() & 255) as i16 + 280;
    let side = (expected.next_word() & 256) as i16;
    let lateral = (expected.next_word() & 255) as i16 + 3 * side - 512;
    let phase = (expected.next_word() & 1) as u8;
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert_eq!(random.state(), expected.state());
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].height,
            enemy.crafts[0].lateral,
            enemy.crafts[0].movement,
            enemy.crafts[0].animation_phase
        ),
        (19, height, lateral, 26, phase)
    );
    assert_eq!(enemy.target, [0, 0]);
    assert!(!enemy.bullet.active);
}

#[test]
fn falling_ground_conversion_advances_the_current_craft_immediately() {
    for index in 0..5 {
        for (movement, drop) in [(25, 20), (26, 13)] {
            let mut enemy = Enemy::new(&assets()).unwrap();
            let mut random = Random::default();
            let craft = &mut enemy.crafts[index];
            craft.active = true;
            craft.variant = 2;
            craft.movement = movement;
            craft.depth = 20;
            craft.height = -100;
            craft.animation_phase = 0;
            craft.requested_sprite = 129;
            let before = enemy.crafts;

            enemy.update_half_step(&mut random, [14, 0, -100]);
            let craft = enemy.crafts[index];
            assert_eq!(
                (
                    craft.movement,
                    craft.animation_phase,
                    craft.requested_sprite
                ),
                (251, 1, 118)
            );
            assert_eq!(craft.height, -100 - drop);
            for (i, previous) in before.iter().enumerate() {
                if i != index {
                    let other = enemy.crafts[i];
                    assert_eq!(
                        (
                            other.active,
                            other.movement,
                            other.animation_phase,
                            other.requested_sprite
                        ),
                        (
                            previous.active,
                            previous.movement,
                            previous.animation_phase,
                            previous.requested_sprite
                        )
                    );
                }
            }

            enemy.update_half_step(&mut random, [14, 0, -100]);
            let craft = enemy.crafts[index];
            assert_eq!((craft.animation_phase, craft.requested_sprite), (2, 118));
        }
    }
}

#[test]
fn falling_recycle_at_player_depth_retires_with_the_new_movement() {
    let mut assets = assets();
    assets.sprites[123].width_bytes = 10;
    assets.sprites[123].height = 31;
    for movement in [25, 26] {
        let mut enemy = Enemy::new(&assets).unwrap();
        let mut random = Random::default();
        let mut expected = Random::default();
        enemy.falling_movement = movement;
        let craft = &mut enemy.crafts[0];
        craft.active = true;
        craft.variant = 2;
        craft.movement = 251;
        craft.depth = 15;
        craft.height = -120;
        craft.animation_phase = 12;
        craft.requested_sprite = 123;
        enemy.project(&assets, false, 106, 0).unwrap();
        let old_rect = enemy.crafts[0].rect;
        let height = (expected.next_word() & 255) as i16 + 280;
        let side = (expected.next_word() & 256) as i16;
        let lateral = (expected.next_word() & 255) as i16 + 3 * side - 512;
        expected.next_word();

        enemy.update_half_step(&mut random, [14, 0, -100]);
        let craft = enemy.crafts[0];
        assert!(!craft.active);
        assert_eq!(
            (craft.depth, craft.lateral, craft.height),
            (14, lateral, height)
        );
        assert_eq!((craft.movement, craft.animation_phase), (movement, 0));
        assert_eq!(craft.requested_sprite, 123);
        assert_eq!(random.state(), expected.state());

        enemy.project(&assets, false, 106, 0).unwrap();
        let craft = enemy.crafts[0];
        assert_eq!(craft.resolved_sprite, 123);
        assert_eq!(craft.rect.x_bytes, 128);
        assert_eq!(craft.rect.y, old_rect.y);
        assert_eq!((craft.rect.width_bytes, craft.rect.height), (10, 31));
        enemy.update_half_step(&mut random, [14, 0, -100]);
        assert_eq!(random.state(), expected.state());
        assert_eq!(enemy.crafts[0].requested_sprite, 123);
    }
}

#[test]
fn falling_variants_split_contact_recycling_from_weapon_explosions() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x99, &mut random, 0, 1).unwrap();
    enemy.explode_craft(5).unwrap();
    assert_eq!(enemy.crafts[0].movement, 251);
    assert_eq!(enemy.craft_animations[0], None);
    enemy.hit_by_weapon(5).unwrap();
    assert_eq!(enemy.crafts[0].movement, 3);
    assert_eq!(enemy.craft_animations[0], Some(0));
    enemy.bullet.active = true;
    enemy.bullet.requested_sprite = 67;
    enemy.deactivate_projectile();
    assert!(!enemy.bullet.active);
    assert_eq!(enemy.bullet.requested_sprite, 67);
}

#[test]
fn transition_grants_life_once_then_scores_and_departs_from_phase_thirty() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let context = EnemyContext {
        horizon: 18,
        speeds: [40, 28, 10],
        ..EnemyContext::default()
    };
    let spawn = enemy
        .spawn_command_with_context(0x9b, &mut random, [14, 70, -28], 5, context)
        .unwrap();
    assert_eq!((spawn.lives, spawn.difficulty), (1, 1));
    enemy.crafts[0].depth = 50;
    let enter = enemy
        .update_half_step_with_context(&mut random, [14, 70, -28], context)
        .unwrap();
    assert_eq!(enter.points, 0);
    assert_eq!(enter.points, 0);
    assert_eq!(
        (enemy.crafts[0].depth, enemy.crafts[0].animation_phase),
        (50, 2)
    );
    enemy.crafts[0].animation_phase = 30;
    let depart = enemy
        .update_half_step_with_context(&mut random, [14, 70, -28], context)
        .unwrap();
    assert_eq!(depart.points, 3000);
    assert_eq!(
        (
            enemy.crafts[0].depth,
            enemy.crafts[0].lateral,
            enemy.crafts[0].height,
            enemy.crafts[0].animation_phase
        ),
        (90, 0, 18, 31)
    );
    enemy.crafts[0].depth = 1000;
    enemy
        .update_half_step_with_context(&mut random, [14, 70, -28], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].depth, -50);
    assert!(!enemy.crafts[0].active);
    assert_eq!(enemy.crafts[0].requested_sprite, 160);
}

#[test]
fn boss_variants_use_narrow_targets_and_phase_driven_departure() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let mut expected = Random::default();
    enemy.spawn_command(0x9f, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 200;
    let context = EnemyContext::default();
    enemy
        .update_half_step_with_context(&mut random, [14, 0, -100], context)
        .unwrap();
    expected.next_word();
    let target = [
        (expected.next_word() & 7) as i16 - 3,
        -100 + (expected.next_word() & 7) as i16 - 3,
    ];
    assert_eq!(enemy.target, target);
    assert_eq!(random.state(), expected.state());
    assert_eq!(enemy.crafts[0].animation_phase, 1);
    enemy.crafts[0].animation_phase = 120;
    enemy
        .update_half_step_with_context(&mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].animation_phase, 255);
    let old = enemy.crafts[0].depth;
    enemy
        .update_half_step_with_context(&mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].depth, old + 22);
}

#[test]
fn crossing_boss_uses_two_parameterized_segments_before_departure() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0xa1, &mut random, 0, 4).unwrap();
    enemy.orbit_center = [0, -100];
    enemy.crafts[0].animation_phase = 40;
    enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
    assert_eq!(
        (
            enemy.crafts[0].lateral,
            enemy.crafts[0].height,
            enemy.crafts[0].animation_phase
        ),
        (400, 120, 41)
    );
    enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
    assert_eq!(
        (enemy.crafts[0].lateral, enemy.crafts[0].height),
        (400, 115)
    );
    enemy.crafts[0].animation_phase = 81;
    enemy.crafts[0].depth = 200;
    let depth = enemy.crafts[0].depth;
    enemy.boss_formation(0, [14, 0, -100], EnemyContext::default());
    assert_eq!(enemy.crafts[0].depth, depth + 22);
    assert_eq!(enemy.crafts[0].animation_phase, 81);
}

#[test]
fn patrol_changes_source_after_resolution_and_crosses_all_four_turns() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let context = EnemyContext {
        difficulty: 2,
        patrol_sources: Some([178, 179, 180]),
        ..EnemyContext::default()
    };
    enemy
        .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 5, context)
        .unwrap();
    assert_eq!(
        enemy.crafts.map(|c| c.active),
        [true, true, true, false, false]
    );
    enemy.crafts[0].lateral = 400;
    enemy
        .patrol(0, &mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].animation_phase, 1);
    assert!((178..=180).contains(&enemy.crafts[0].requested_sprite));
    let state = random.state();
    enemy.crafts[0].height = 0;
    enemy
        .patrol(0, &mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].animation_phase, 2);
    assert_eq!(random.state(), state);
    enemy.crafts[0].lateral = -400;
    enemy
        .patrol(0, &mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].animation_phase, 3);
    enemy.crafts[0].height = -100;
    enemy
        .patrol(0, &mut random, [14, 0, -100], context)
        .unwrap();
    assert_eq!(enemy.crafts[0].animation_phase, 0);
}

#[test]
fn game_reset_retains_primary_cache_and_restores_global_target_and_center() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x92, &mut random, 0, 1).unwrap();
    enemy.crafts[0].depth = 90;
    enemy.crafts[0].height = 120;
    enemy.crafts[0].phase = 3;
    enemy.crafts[0].variant = 5;
    enemy.crafts[0].animation_phase = 87;
    enemy.crafts[0].requested_sprite = 139;
    enemy.crafts[0].resolved_sprite = 138;
    enemy.target = [99, 77];
    enemy.orbit_center = [20, 40];
    let old = enemy.crafts[0];
    enemy.reset_for_game(&assets()).unwrap();
    let after = enemy.crafts[0];
    assert_eq!(
        (
            after.depth,
            after.height,
            after.phase,
            after.variant,
            after.animation_phase,
            after.requested_sprite,
            after.resolved_sprite
        ),
        (
            old.depth,
            old.height,
            old.phase,
            old.variant,
            old.animation_phase,
            old.requested_sprite,
            old.resolved_sprite
        )
    );
    assert!(!after.active);
    assert_eq!(after.movement, 0);
    assert_eq!(enemy.target, [0, 0]);
    assert_eq!(enemy.orbit_center, [0, -100]);
    assert_eq!(
        (
            enemy.bullet.depth,
            enemy.bullet.lateral,
            enemy.bullet.height,
            enemy.bullet.requested_sprite,
            enemy.bullet.resolved_sprite
        ),
        (14, 0, -100, 22, 22)
    );
}
#[test]
fn malformed_patrol_choices_report_retry_stall() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    let context = EnemyContext {
        difficulty: 10,
        patrol_sources: Some([0; 3]),
        ..EnemyContext::default()
    };
    enemy
        .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 1, context)
        .unwrap();
    let before = random.state();
    let error = enemy
        .update_half_step_with_context(&mut random, [14, 0, -100], context)
        .unwrap_err();
    assert!(matches!(error,Error::InvalidData(message) if message.contains("stalls")));
    assert_eq!(enemy.crafts[0].requested_sprite, 0);
    assert_eq!(random.state(), before);
}

#[test]
fn patrol_population_does_not_disappear_when_difficulty_exceeds_signed_range() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    for difficulty in 0..=u16::MAX {
        let context = EnemyContext {
            difficulty,
            ..EnemyContext::default()
        };
        enemy
            .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 5, context)
            .unwrap();
        let expected = match difficulty {
            0 => 1,
            1 => 2,
            _ => 3,
        };
        assert_eq!(
            enemy.crafts.iter().filter(|craft| craft.active).count(),
            expected,
            "difficulty {difficulty}"
        );
    }
}

#[test]
fn later_patrol_difficulties_reuse_the_last_artwork_family() {
    for difficulty in [9, 10, 11, 12, 32767, 32768, u16::MAX] {
        let resources = assets();
        let mut enemy = Enemy::new(&resources).unwrap();
        let mut random = Random::default();
        let context = EnemyContext {
            difficulty,
            ..EnemyContext::default()
        };
        enemy
            .spawn_command_with_context(0xa2, &mut random, [14, 0, -100], 3, context)
            .unwrap();
        for _ in 0..16 {
            enemy
                .update_half_step_with_context(&mut random, [14, 0, -100], context)
                .unwrap();
            enemy.project(&resources, true, 106, 0).unwrap();
            for craft in enemy.crafts.iter().take(3) {
                if craft.active {
                    assert!((184..=186).contains(&craft.requested_sprite));
                }
            }
        }
    }
}

#[test]
fn restored_records_keep_cache_and_goal_clear_defers_shadow_retirement() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut random = Random::default();
    enemy.spawn_command(0x88, &mut random, 0, 2).unwrap();
    enemy.crafts[1].requested_sprite = 33;
    enemy.crafts[1].resolved_sprite = 32;
    enemy.crafts[1].phase = 2;
    let snapshots = enemy.snapshots();
    let mut restored = Enemy::restore_snapshots(snapshots, &assets()).unwrap();
    assert_eq!(
        (
            restored.crafts[1].requested_sprite,
            restored.crafts[1].resolved_sprite,
            restored.crafts[1].phase
        ),
        (33, 32, 2)
    );
    restored.clear_primary();
    assert!(!restored.active());
    assert!(restored.shadows[0].active);
    restored.update_half_step(&mut random, [14, 0, -100]);
    assert!(restored.shadows.iter().all(|shadow| !shadow.active));
    let mut malformed = snapshots;
    malformed[0].slot = 5;
    assert!(Enemy::restore_snapshots(malformed, &assets()).is_err());
}
#[test]
fn falling_spawn_requests_one_tone_for_every_initialized_slot() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    let mut actions = Vec::new();
    enemy
        .spawn_with_sound(
            0x99,
            &mut Random::default(),
            [14, 0, -100],
            5,
            EnemyContext::default(),
            &mut |a| actions.push(a),
        )
        .unwrap();
    assert_eq!(
        actions,
        vec![
            GameplayAction::Request {
                site: 0x1a7e,
                effect: Effect::Tone(Tone::Second)
            };
            5
        ]
    );
    assert!(!enemy.crafts[4].active);
}

#[test]
fn transition_sample_suspends_before_clamping_rng_and_remaining_slots() {
    let mut direct = Enemy::new(&assets()).unwrap();
    let mut synchronous = Enemy::new(&assets()).unwrap();
    for enemy in [&mut direct, &mut synchronous] {
        enemy
            .spawn_command(0x9b, &mut Random::default(), 0, 1)
            .unwrap();
        enemy.crafts[0].depth = 50;
        enemy.crafts[1] = enemy.crafts[0];
        enemy.crafts[1].slot = 6;
    }
    let player = [14, 0, -100];
    let context = EnemyContext::default();
    let mut direct_random = Random::default();
    let mut synchronous_random = direct_random;
    let mut actions = Vec::new();
    let later = format!("{:?}", direct.crafts[1]);
    let mut task = direct.begin_update(player, context);
    let sample = direct
        .resume_update(&mut task, &mut direct_random, &mut |a| actions.push(a))
        .unwrap()
        .unwrap();
    assert!(matches!(
        sample,
        GameplayAction::PlaySample { site: 0x42bf, .. }
    ));
    assert_eq!(direct.crafts[0].animation_phase, 0);
    assert_eq!(direct.crafts[0].depth, 22);
    assert_eq!(format!("{:?}", direct.crafts[1]), later);
    assert_eq!(direct_random, synchronous_random);
    assert_eq!(actions.len(), 1);
    actions.push(sample);
    let sample = direct
        .resume_update(&mut task, &mut direct_random, &mut |a| actions.push(a))
        .unwrap()
        .unwrap();
    assert_eq!(
        (direct.crafts[0].animation_phase, direct.crafts[0].depth),
        (2, 50)
    );
    assert_eq!(
        (direct.crafts[1].animation_phase, direct.crafts[1].depth),
        (0, 22)
    );
    actions.push(sample);
    assert!(
        direct
            .resume_update(&mut task, &mut direct_random, &mut |a| actions.push(a))
            .unwrap()
            .is_none()
    );
    let mut synchronous_actions = Vec::new();
    let effects = synchronous
        .update_with_sound(&mut synchronous_random, player, context, &mut |a| {
            synchronous_actions.push(a)
        })
        .unwrap();
    assert_eq!(actions, synchronous_actions);
    assert_eq!(task.effects(), effects);
    assert_eq!(direct_random, synchronous_random);
    assert_eq!(format!("{direct:?}"), format!("{synchronous:?}"));
}

#[test]
fn transition_requests_tone_then_the_guarded_sample_before_departure() {
    let mut enemy = Enemy::new(&assets()).unwrap();
    enemy
        .spawn_command(0x9b, &mut Random::default(), 0, 1)
        .unwrap();
    enemy.crafts[0].depth = 50;
    let mut actions = Vec::new();
    enemy
        .update_with_sound(
            &mut Random::default(),
            [14, 0, -100],
            EnemyContext::default(),
            &mut |a| actions.push(a),
        )
        .unwrap();
    assert_eq!(
        actions,
        [
            GameplayAction::Request {
                site: 0x42bc,
                effect: Effect::Tone(Tone::First)
            },
            GameplayAction::PlaySample {
                site: 0x42bf,
                request: SampleRequest {
                    resource: Resource::Data8,
                    offset: 0,
                    count: 9723,
                    reverse: false
                }
            },
        ]
    );
}

#[cfg(feature = "dev-tools")]
#[test]
fn refired_projectile_keeps_old_artwork_epoch_until_projected() {
    let assets = assets();
    let mut enemy = Enemy::new(&assets).unwrap();
    enemy.bullet.active = true;
    enemy.bullet.depth = 42;
    enemy.bullet.requested_sprite = 67;
    enemy.project(&assets, true, 106, 0).unwrap();
    enemy.crafts[0].active = true;
    enemy.crafts[0].depth = 100;
    let mut random = Random::default();
    for _ in 0..3 {
        random.next_word();
    }
    enemy.update_half_step(&mut random, [14, 0, -100]);
    assert!(enemy.bullet.active);
    assert_eq!(enemy.bullet_birth, 1);
    assert_eq!(enemy.projected_births[0], 0);
    assert_eq!(enemy.bullet.resolved_sprite, 67);
    enemy.project(&assets, false, 106, 0).unwrap();
    assert_eq!(enemy.projected_births[0], 0);
    enemy.project(&assets, true, 106, 0).unwrap();
    assert_eq!(enemy.projected_births[0], 1);
}
