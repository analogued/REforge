// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{
    assets::Sprite,
    frame::{FRAME_BYTES, ROW_BYTES, Rect},
    gameplay::{Steering, raster::Layer},
};

fn assets() -> GraphicsAssets {
    GraphicsAssets {
        primary: vec![0x1b; 28_960],
        alternate: vec![0x6c; 16_000],
        sprites: (0..190)
            .map(|_| Sprite {
                width_bytes: 14,
                height: 18,
                descriptor: [0; 7],
                color: vec![0xff; 14 * 18],
                mask: vec![0; 14 * 18],
            })
            .collect(),
    }
}

fn layer(slot: usize, source: usize, x: i32, y: i32) -> Layer {
    Layer {
        slot,
        source,
        rect: Rect {
            x_bytes: x,
            y,
            width_bytes: 2,
            height: 2,
        },
        color: vec![0xff; 4],
        mask: vec![0; 4],
    }
}

fn observation(i: u64) -> Observation {
    let a = assets();
    let mut game = Gameplay::test_game(&a).unwrap();
    game.advance_subframe(&a, Steering::default()).unwrap();
    let mut f = Observation::capture(&game, &a, i as f64 * 0.04, i).unwrap();
    f.pose = Pose {
        at: f.at,
        id: i,
        body: (i / 3) as u16 + 1,
        pass: (i % 3 + 1) as u8,
        coordinates: [14, i as i16 * 4, -100],
        phase: 0,
        controls: 8,
        horizon: 20,
        origin: 106,
        velocity: [1, 0],
        source: [13, 8, 2, 18, 8, 2],
    };
    f.state.static_lateral = -(i as i16) * 8;
    f.state.speeds = [28, 22, 16];
    f.scene.base = Frame::from_packed(&vec![0; FRAME_BYTES]).unwrap();
    f.scene.layers = vec![layer(0, 13, 25, 100)];
    f.objects.clear();
    f.objects.insert(
        2,
        Object {
            position: [100.0 + i as f64 * 10.0, 0.0, -100.0],
            active: true,
            movement: 254,
            birth: 1,
            projected_birth: 1,
            gate: None,
        },
    );
    f
}

fn tracking() -> Tracker {
    let mut t = Tracker::default();
    for i in 0..4 {
        let f = observation(i);
        let at = f.at;
        t.observe(f, at, false);
    }
    t
}

fn homing_tracking(depth: f64, velocity: i16, delay: f64, target: [i16; 2]) -> Tracker {
    let mut t = Tracker::default();
    for i in 0..4 {
        let mut f = observation(i);
        f.pose.controls = if velocity > 0 { 8 } else { 4 };
        f.pose.velocity = [velocity, 0];
        f.pose.coordinates = [14, 0, 0];
        f.state.static_lateral = 0;
        f.state.enemy_target = target;
        let mut bullet = f.objects.remove(&2).unwrap();
        bullet.position = [depth, 34.0, -60.0];
        bullet.movement = 253;
        f.objects.insert(4, bullet);
        let at = f.at;
        t.observe(f, at + delay, false);
    }
    t
}

fn pixel(f: &Frame, x: usize, y: usize) -> u8 {
    (f.packed()[y * ROW_BYTES + x / 4] >> (6 - x % 4 * 2)) & 3
}

#[test]
fn reciprocal_table_and_fractional_projection_keep_native_precision() {
    for d in 1..1000 {
        assert_eq!(
            scale(f64::from(d)),
            f64::from(2_097_152 / (d + 64)) / 65_536.0
        );
        if d > 1 && d < 999 {
            assert!((scale(f64::from(d) - 1e-8) - scale(f64::from(d) + 1e-8)).abs() < 1e-8);
        }
    }
}

#[test]
fn scalar_has_exact_analytical_response_and_query_independence() {
    let s = Scalar {
        at: 1.0,
        start: 10.0,
        target: 12.0,
        velocity: 100.0,
        stop: 0.05,
    };
    let expected = s.sample(1.127);
    for i in 0..127 {
        s.sample(1.0 + f64::from(i) / 1000.0);
    }
    assert_eq!(s.sample(1.127), expected);
    assert_eq!(s.sample(1.0), 10.0);
    assert!((s.sample(3.0) - 17.0).abs() < 1e-10);
    let moving = Scalar::follow(None, 0.0, 0.0, 10.0, 0.05, 0.0, false);
    assert!((moving.sample(0.02) - 0.2 / std::f64::consts::E).abs() < 1e-12);
}

#[test]
fn slow_ordered_cadence_and_body_wrap_keep_tracks_but_missing_pass_resets() {
    let mut t = Tracker::default();
    for i in 0..7 {
        let mut f = observation(i);
        f.at = i as f64 * 0.17851 / 3.0;
        f.pose.at = f.at;
        f.pose.body = u16::MAX.wrapping_add((i / 3) as u16);
        let at = f.at;
        t.observe(f, at, false);
    }
    assert_eq!(t.cadence.len(), 4);
    assert!(t.tracks[&2].segments[0].velocity > 0.0);
    let mut f = observation(8);
    f.at = 0.5;
    f.pose.at = f.at;
    t.observe(f, 0.5, false);
    assert_eq!(t.cadence.len(), 1);
    assert_eq!(t.tracks[&2].segments[0].velocity, 0.0);
}

#[test]
fn second_pass_preserves_intrinsic_segment_and_new_shot_predicts_immediately() {
    let mut t = tracking();
    let old = t.tracks[&2].segments[0];
    let f = observation(4);
    t.observe(f, 0.16, false);
    assert_eq!(old.at, t.tracks[&2].segments[0].at);
    let mut f = observation(5);
    f.objects.get_mut(&2).unwrap().birth = 2;
    f.objects.get_mut(&2).unwrap().projected_birth = 2;
    t.observe(f, 0.2, false);
    let s = t.tracks[&2].segments[0];
    assert_eq!(s.start, 150.0);
    assert!(s.velocity > 0.0 && s.stop > 0.0);
    assert_eq!(t.tracks[&2].identity.0, 2);
}

#[test]
fn projected_birth_guard_keeps_cached_image_at_native_placement() {
    let a = assets();
    let mut t = tracking();
    let mut f = observation(4);
    f.scene.layers.push(layer(2, 22, 10, 70));
    f.objects.get_mut(&2).unwrap().birth = 2;
    t.observe(f, 0.16, false);
    assert!(!t.tracks.contains_key(&2));
    let image = t.render(0.18, None, &a).unwrap().unwrap();
    assert_eq!(pixel(&image, 40, 70), 3);
    assert_eq!(pixel(&image, 160, 120), 0);
}

#[test]
fn reset_requires_full_first_pass_and_wait_freezes_every_coordinate() {
    let a = assets();
    let mut t = Tracker::default();
    for i in [1, 2] {
        let f = observation(i);
        let at = f.at;
        t.observe(f, at, false);
        assert!(t.render(at, None, &a).unwrap().is_none());
    }
    t = tracking();
    t.suspend(0.135);
    assert_eq!(t.positions(0.135), t.positions(8.0));
    let f = observation(4);
    t.observe(f, 0.16, false);
    assert_eq!(t.cadence.len(), 1);
    assert_eq!(t.camera.unwrap()[0].velocity, 0.0);
    assert_eq!(t.tracks[&2].segments[0].velocity, 0.0);
}

#[test]
fn receipt_delay_consumes_forecast_and_rebase_clears_velocity() {
    for delay in [0.015, 0.08] {
        let mut t = Tracker::default();
        for i in 0..4 {
            let f = observation(i);
            let at = f.at;
            t.observe(f, at + delay, false);
        }
        for s in t.tracks[&2].segments {
            assert!(s.stop <= (HORIZON - delay).max(0.0) + 1e-12);
        }
        assert!(t.render(0.119 + delay, None, &assets()).unwrap().is_none());
        let f = observation(4);
        t.observe(f, 0.5, true);
        assert_eq!(t.tracks[&2].segments[0].velocity, 0.0);
        assert_eq!(t.camera.unwrap()[0].velocity, 0.0);
    }
}

#[test]
fn damage_changes_only_craft_placement_while_world_continues() {
    let a = assets();
    let mut t = tracking();
    let mut f = observation(4);
    f.pose.phase = 1;
    let before = t.tracks[&2].segments[0].sample(f.at);
    t.observe(f, 0.16, false);
    assert_eq!(t.tracks[&2].segments[0].sample(0.16), before);
    assert_eq!(t.cadence.len(), 4);
    for anchors in [None, Some([200.0, 120.0, 130.0])] {
        let image = t.render(0.17, anchors, &a).unwrap().unwrap();
        assert_eq!(pixel(&image, 100, 100), 3);
        assert_eq!(pixel(&image, 200, 120), 0);
    }
    let mut f = observation(5);
    f.pose.phase = 2;
    t.observe(f, 0.2, false);
    assert!(t.tracks[&2].segments[0].velocity > 0.0);
}

#[test]
fn terrain_conversion_retains_filter_and_recycling_starts_fresh() {
    let mut t = Tracker::default();
    for i in 0..5 {
        let mut f = observation(i);
        let mut terrain = f.objects.remove(&2).unwrap();
        terrain.position[0] = 500.0 - i as f64 * 28.0;
        terrain.movement = 2;
        f.objects.insert(16, terrain);
        let at = f.at;
        t.observe(f, at, false);
    }
    let mut f = observation(5);
    let mut terrain = f.objects.remove(&2).unwrap();
    terrain.position[0] = 360.0;
    terrain.movement = 252;
    f.objects.insert(16, terrain);
    let before = t.tracks[&16].segments[0].sample(0.2);
    t.observe(f.clone(), 0.2, false);
    assert_eq!(t.tracks[&16].segments[0].start, before);
    assert_eq!(t.tracks[&16].segments[0].velocity, 0.0);
    f.at = 0.24;
    f.pose.at = 0.24;
    f.pose.pass = 1;
    f.pose.body += 1;
    f.objects.get_mut(&16).unwrap().position[0] = 700.0;
    t.observe(f, 0.24, false);
    assert_eq!(t.tracks[&16].segments[0].start, 700.0);
}

#[test]
fn terrain_and_gate_forecasts_stop_at_player_plane_and_shots_at_depth_300() {
    for slot in [2, 16, 23] {
        for depth in [15.0, 20.0, 42.0, 48.0, 70.0, 150.0, 299.0] {
            let mut t = Tracker::default();
            for i in 0..4 {
                let mut f = observation(i);
                let mut o = f.objects.remove(&2).unwrap();
                o.position[0] = depth;
                o.movement = if slot == 16 { 2 } else { 1 };
                f.objects.insert(slot, o);
                let at = f.at;
                t.observe(f, at, false);
            }
            let s = t.tracks[&slot].segments[0];
            let end = s.target + s.velocity * s.stop;
            if slot == 2 {
                assert!(end <= 300.0 + 1e-10);
            } else {
                assert!(end >= 14.0 - 1e-10);
                for i in 0..100 {
                    assert!(s.sample(s.at + f64::from(i) / 100.0) >= 14.0 - 1e-10);
                }
            }
        }
    }
}

#[test]
fn homing_at_target_cancels_held_strafe_without_lateral_wobble() {
    for direction in [-1, 1] {
        for delay in [0.0, 0.015, 0.08] {
            let mut t = homing_tracking(500.0, 3 * direction, delay, [34, -60]);
            assert_ne!(t.camera.unwrap()[0].velocity, 0.0);
            for step in 0..101 {
                let at = 0.12 + delay + f64::from(step) / 1000.0;
                let lateral = t.positions(at).0[&4][1];
                assert!(
                    (lateral - 34.0).abs() < 1e-10,
                    "direction {direction}, delay {delay}, at {at}: {lateral}"
                );
            }
            // Pass two really shifts the camera without running homing. Keep
            // the old intrinsic segment rather than locking the bullet to target.
            let before = t.tracks[&4].segments[1];
            let mut f = t.frame.clone().unwrap();
            f.at += 0.04;
            f.pose.at = f.at;
            f.pose.id += 1;
            f.pose.pass = 2;
            f.state.static_lateral = 16;
            f.objects.get_mut(&4).unwrap().position[1] += 16.0;
            let at = f.at + delay;
            t.observe(f, at, false);
            assert_eq!(t.tracks[&4].segments[1].at, before.at);
            assert!((t.positions(at + 0.02).0[&4][1] - 34.0).abs() > 1e-6);
        }
    }
}

#[test]
fn homing_cancellation_survives_either_prediction_stopping_first() {
    for direction in [-1, 1] {
        for (depth, camera_first) in [(20.0, false), (500.0, true)] {
            for delay in [0.0, 0.015, 0.08] {
                let t = homing_tracking(depth, 5 * direction, delay, [34, -60]);
                if delay == 0.0 {
                    let camera_stop = t.camera.unwrap()[0].stop;
                    let bullet_stop = t.tracks[&4].segments[0].stop;
                    assert_eq!(camera_stop < bullet_stop, camera_first);
                    assert!((camera_stop - bullet_stop).abs() > 0.001);
                }
                for step in 0..201 {
                    let at = 0.12 + delay + f64::from(step) / 1000.0;
                    let p = t.positions(at).0[&4];
                    assert!(
                        (p[1] - 34.0).abs() < 1e-10,
                        "direction {direction}, depth {depth}, delay {delay}, at {at}: {p:?}"
                    );
                }
                if camera_first && delay == 0.0 {
                    let stop = 0.12 + t.camera.unwrap()[0].stop;
                    assert!(t.positions(stop + 0.005).0[&4][0] < t.positions(stop).0[&4][0]);
                }
            }
        }
    }
}

#[test]
fn homing_motion_continues_after_camera_stops_and_reference_changes_are_continuous() {
    for direction in [-1, 1] {
        let mut t = homing_tracking(500.0, 6 * direction, 0.0, [54, -40]);
        let lateral = t.tracks[&4].segments[1];
        assert!(lateral.stop > t.camera.unwrap()[0].stop);
        assert!((lateral.velocity - 20.0 / 0.06).abs() < 1e-9);
        // Unsaturated genuine homing follows its own response with the whole
        // camera trajectory cancelled, even after that trajectory has stopped.
        let expected = Scalar {
            start: 34.0,
            target: 34.0,
            ..lateral
        };
        for step in 0..201 {
            let at = 0.12 + f64::from(step) / 1000.0;
            assert!((t.positions(at).0[&4][1] - expected.sample(at)).abs() < 1e-10);
        }
        for pass in [2, 3] {
            let mut f = t.frame.clone().unwrap();
            f.at += 0.04;
            f.pose.at = f.at;
            f.pose.id += 1;
            f.pose.pass = pass;
            f.state.static_lateral += 16;
            f.objects.get_mut(&4).unwrap().position[1] += 10.0;
            let at = f.at;
            let before = t.positions(at).0[&4];
            t.observe(f, at, false);
            let after = t.positions(at).0[&4];
            for i in 0..3 {
                assert!((before[i] - after[i]).abs() < 1e-10);
            }
        }
        let at = t.seen + 0.005;
        let before = t.positions(at);
        t.suspend(at);
        assert_eq!(t.positions(at), before);
        assert_eq!(t.positions(at + 10.0), before);
    }
}

#[test]
fn homing_forecast_clamps_after_camera_shift_in_both_directions() {
    let mut f = observation(3);
    let mut bullet = f.objects.remove(&2).unwrap();
    bullet.position = [500.0, 10.0, -60.0];
    bullet.movement = 253;
    f.objects.insert(4, bullet);
    f.state.static_lateral = 0;
    let period = 0.04;
    let world_period = period * 1.5;
    // Camera displacement, target offset, combined lateral displacement over
    // one mean world update. Large shifts distinguish clamping before/after
    // camera compensation, including saturation even when already at target.
    for (camera_step, target_offset, expected) in [
        (0.0, 0, 0.0),
        (48.0, 0, 0.0),
        (-48.0, 0, 0.0),
        (48.0, 20, 20.0),
        (-48.0, 20, 20.0),
        (48.0, -20, -20.0),
        (-48.0, -20, -20.0),
        (120.0, 0, 40.0),
        (-120.0, 0, -40.0),
        (48.0, 200, 128.0),
        (-48.0, 200, 32.0),
        (48.0, -200, -32.0),
        (-48.0, -200, -128.0),
    ] {
        f.state.enemy_target = [10 + target_offset, -60];
        let (rates, _, fraction) = object_forecast(
            &f,
            4,
            253,
            bullet.position,
            &VecDeque::new(),
            period,
            Scalar {
                velocity: camera_step / world_period,
                stop: world_period,
                ..Scalar::fixed(f.at, 0.0)
            },
        );
        assert!(
            ((1.0 - fraction) * camera_step + rates[1] * world_period - expected).abs() < 1e-10,
            "camera {camera_step}, target offset {target_offset}"
        );
        assert_eq!(rates[0], -28.0 / world_period);
        assert_eq!(rates[2], 0.0);
    }
}

#[test]
fn intrinsic_forecast_obeys_projected_travel_bound_and_homing_target() {
    let mut t = Tracker::default();
    for i in 0..4 {
        let mut f = observation(i);
        f.objects.get_mut(&2).unwrap().position = [10.0, 3000.0, -100.0];
        let mut homing = f.objects[&2];
        homing.position = [500.0, 0.0, 0.0];
        f.objects.insert(4, homing);
        f.state.enemy_target = [200, -200];
        let at = f.at;
        t.observe(f, at, false);
    }
    let s = t.tracks[&2].segments;
    let mut start = s.map(|s| s.target);
    start[1] += -24.0;
    let end = std::array::from_fn(|i| start[i] + s[i].velocity * s[i].stop);
    let p = project(start, 20.0, 106.0);
    let q = project(end, 20.0, 106.0);
    assert!((p[0] - q[0]).hypot(p[1] - q[1]) <= BOUND + 1e-8);
    assert!((t.tracks[&4].segments[1].velocity - 80.0 / 0.06).abs() < 1e-9);
    assert!((t.tracks[&4].segments[2].velocity + 80.0 / 0.06).abs() < 1e-9);
}

#[test]
fn deactivated_gate_hides_cached_layer_and_gate_geometry_survives_view_clipping() {
    let a = assets();
    let mut f = observation(0);
    let mut gate = f.state.special[0];
    gate.coordinates = [100, -400, -100];
    gate.extents = [28, 800, 130];
    gate.rect.width_bytes = 30;
    gate.requested_sprite = 3;
    let rects = paint::gate_rectangles(gate, gate.coordinates.map(f64::from), 20.0, 106.0);
    assert!(rects[0][2] > 100.0);
    f.objects.insert(
        23,
        Object {
            position: [-8.0, 0.0, -100.0],
            active: false,
            movement: 1,
            birth: 0,
            projected_birth: 0,
            gate: Some(gate),
        },
    );
    f.scene.layers.push(layer(23, 0, 10, 70));
    let mut t = Tracker::default();
    t.observe(f, 0.0, false);
    let image = t.render(0.0, None, &a).unwrap().unwrap();
    assert_eq!(pixel(&image, 40, 70), 0);
}

#[test]
fn full_asset_sprite_and_ordered_masks_match_independent_two_bit_painter() {
    let mut a = assets();
    for bm in 0..4 {
        for bc in 0..4 {
            for am in 0..4 {
                for ac in 0..4 {
                    let mut f = observation(0);
                    f.pose.horizon = 0;
                    f.objects.clear();
                    f.scene.layers = vec![layer(0, 13, 0, 0), layer(29, 0, 6, 50)];
                    f.scene.base = Frame::from_packed(&vec![0x55; FRAME_BYTES]).unwrap();
                    f.scene.layers[1].mask.fill(bm * 85);
                    f.scene.layers[1].color.fill(bc * 85);
                    a.sprites[13].mask.fill(am * 85);
                    a.sprites[13].color.fill(ac * 85);
                    let mut t = Tracker::default();
                    t.observe(f, 0.0, false);
                    let image = t
                        .render(0.0, Some([24.0, 50.0, 50.0]), &a)
                        .unwrap()
                        .unwrap();
                    assert_eq!(pixel(&image, 24, 50), (((1 & am) | ac) & bm) | bc);
                    assert_eq!(pixel(&image, 15, 50), 1);
                    assert_eq!(pixel(&image, 24, 43), 1);
                }
            }
        }
    }
}

#[test]
fn capture_is_read_only_and_render_keeps_native_background_and_hud_bytes() {
    let a = assets();
    let mut game = Gameplay::test_game(&a).unwrap();
    for i in 0..9 {
        game.advance_subframe(&a, Steering::default()).unwrap();
        let before = format!("{game:?}");
        let mut f = Observation::capture(&game, &a, f64::from(i) * 0.04, i as u64).unwrap();
        assert_eq!(before, format!("{game:?}"));
        // Remove only actor operations to inspect the exact observed base.
        let base = f.scene.base.clone();
        f.scene.layers.clear();
        let mut t = Tracker::default();
        t.observe(f, f64::from(i) * 0.04, false);
        if let Some(image) = t.render(1.0, None, &a).unwrap() {
            assert_eq!(image, base);
        }
    }
}

#[test]
fn reported_craft_anchors_match_shared_camera_projection_and_painted_pixels() {
    let a = assets();
    let mut t = Tracker::default();
    assert!(t.craft_anchors(0.0, [0.0; 3]).is_none());
    let f = observation(0);
    let native = f.pose.anchors();
    let projected = project(f.pose.coordinates.map(f64::from), 20.0, 106.0 + 20.0 / 16.0);
    t.observe(f, 0.0, false);
    let corrected = t.craft_anchors(0.0, native).unwrap();
    assert_eq!(corrected[0], projected[0]);
    assert_eq!(corrected[1], projected[1]);
    assert_eq!(corrected[2] - native[2], 20.0 / 16.0);
    assert!(t.craft_anchors(-0.001, native).is_none());
    let image = t.render(0.0, Some(native), &a).unwrap().unwrap();
    let y = (projected[1] + 0.5).floor() as usize;
    assert_eq!(pixel(&image, 160, y), 3);
    assert_eq!(pixel(&image, 160, y - 1), 0);
    let mut damaged = observation(1);
    damaged.pose.phase = 1;
    t.observe(damaged, 0.04, false);
    assert!(t.craft_anchors(0.04, native).is_none());
}
