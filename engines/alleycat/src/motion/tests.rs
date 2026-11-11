// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
pub(super) fn layer(id: u8, x: i32, blend: Blend, pixels: Vec<u8>) -> Layer {
    let mut l = Layer::new(id, 0, Dimensions(0x0101), pixels, blend);
    l.native = [x, 40];
    l.position = l.native.map(f64::from);
    l
}
pub(super) fn publication(layers: Vec<Layer>) -> Publication {
    let mut background = Framebuffer::default();
    background.fill_visible(0xaa);
    let mut native = background.clone();
    for l in &layers {
        let offset = crate::framebuffer::pixel_offset(l.native[0] as u16, l.native[1] as u8);
        match l.blend {
            Blend::Or => {
                let old = native.read_rect(offset, l.dimensions).unwrap();
                let pixels: Vec<_> = old.iter().zip(&l.pixels).map(|(a, b)| a | b).collect();
                native.copy_rect(offset, l.dimensions, &pixels).unwrap();
            }
            Blend::And => {
                native.and_rect(offset, l.dimensions, &l.pixels).unwrap();
            }
            Blend::Zero => {
                native
                    .zero_transparent_rect(offset, l.dimensions, &l.pixels)
                    .unwrap();
            }
            Blend::Opaque => native.copy_rect(offset, l.dimensions, &l.pixels).unwrap(),
        }
    }
    Publication {
        native,
        background,
        colors: [
            [0, 0, 0, 255],
            [85, 85, 85, 255],
            [170, 170, 170, 255],
            [255, 255, 255, 255],
        ],
        layers,
        scene: 0,
        supported: true,
        coherent_water: false,
        hearts_presentation: false,
    }
}
#[test]
fn field_limits_apply_after_pose_registration_without_changing_native_anchors() {
    for (native, logical, registration) in [(264, 266.0, 1.0), (36, 37.0, -3.0)] {
        let mut rival = layer(48, native, Blend::Opaque, vec![0; 2]);
        rival.position[0] = logical;
        rival.pose_offset = registration;
        rival.paint_x_bounds = Some([36.0, 264.0]);
        let p = publication(vec![rival]);
        assert!(p.native_composition_matches());
        assert_eq!(p.placed_indices(|l| l.position), p.native.indices());
        assert_eq!(p.layers[0].position[0], logical);
        assert_eq!(p.layers[0].pose_offset, registration);
        assert_eq!(p.layers[0].native[0], native);
    }
    let mut rival = layer(48, 100, Blend::Opaque, vec![0; 2]);
    rival.paint_x_bounds = Some([36.0, 264.0]);
    assert_eq!(rival.paint_origin([101.0, 40.0]), [101.0, 40.0]);
}

#[test]
fn native_composition_matches_every_pixel_for_overlapping_blend_contracts() {
    let p = publication(vec![
        layer(0, 8, Blend::And, vec![0x1b, 0xe4]),
        layer(1, 12, Blend::Zero, vec![0x04, 0x1b]),
        layer(2, 16, Blend::Opaque, vec![0x55, 0xaa]),
        layer(40, 8, Blend::Or, vec![0x55, 0x05]),
    ]);
    assert!(p.native_composition_matches());
    let mut presenter = Presenter::default();
    presenter.publish(p.clone(), 0.0, 0.0, true);
    let expected: Vec<u8> = p
        .native
        .indices()
        .into_iter()
        .flat_map(|i| p.colors[i as usize])
        .collect();
    assert_eq!(presenter.rgba(0).unwrap(), expected);
}
#[test]
fn pixel_painter_handles_one_pixel_translation_and_clipping() {
    let l = layer(0, 0, Blend::Opaque, vec![0x1b, 0xe4]);
    let mut dst = vec![2; WIDTH * HEIGHT];
    l.paint(&mut dst, [-2.0, 199.0]);
    assert_eq!(&dst[199 * WIDTH..199 * WIDTH + 7], &[2, 3, 3, 2, 1, 0, 2]);
    assert!(dst[..199 * WIDTH].iter().all(|&x| x == 2));
}
#[test]
fn motion_creates_intermediate_positions_and_bounded_prediction_then_settles() {
    let mut t = Track::new(layer(0, 40, Blend::And, vec![0; 2]), 0.0, 0.0);
    t.update(layer(0, 48, Blend::And, vec![0; 2]), 0.03, 0.03);
    assert_eq!(t.sample(0.03), [40.0, 40.0]);
    let x = t.sample(0.045)[0];
    assert!(x > 40.0 && x < 48.0, "{x}");
    for i in 0..100 {
        let x = t.sample(0.03 + i as f64 / 1000.0)[0];
        assert!((40.0..=52.0).contains(&x), "{x}");
    }
    assert!((t.sample(1.0)[0] - 48.0).abs() < 0.001);
}
#[test]
fn reversal_teleport_and_shape_change_reset_prediction() {
    let mut t = Track::new(layer(0, 40, Blend::And, vec![0; 2]), 0.0, 0.0);
    t.update(layer(0, 48, Blend::And, vec![0; 2]), 0.03, 0.03);
    t.update(layer(0, 40, Blend::And, vec![0; 2]), 0.06, 0.06);
    assert_eq!(t.sample(0.06), [40.0, 40.0]);
    t.update(layer(0, 200, Blend::And, vec![0; 2]), 0.09, 0.09);
    assert_eq!(t.sample(0.09)[0], 200.0);
    let mut next = layer(0, 204, Blend::And, vec![0; 4]);
    next.dimensions = Dimensions(0x0201);
    t.update(next, 0.12, 0.12);
    assert_eq!(t.sample(0.12)[0], 204.0);
}
#[test]
fn observer_lifetimes_scene_changes_and_blocking_never_bridge_positions() {
    let mut p = Presenter::default();
    p.publish(
        publication(vec![layer(0, 40, Blend::And, vec![0; 2])]),
        0.0,
        0.0,
        true,
    );
    p.publish(
        publication(vec![layer(0, 48, Blend::And, vec![0; 2])]),
        0.03,
        0.03,
        true,
    );
    assert!(p.tracks[&0].sample(0.04)[0] < 48.0);
    p.publish(publication(vec![]), 0.04, 0.04, true);
    assert!(!p.is_active());
    p.publish(
        publication(vec![layer(0, 52, Blend::And, vec![0; 2])]),
        0.05,
        0.05,
        true,
    );
    assert_eq!(p.tracks[&0].sample(0.05)[0], 52.0);
    let mut next = publication(vec![layer(0, 56, Blend::And, vec![0; 2])]);
    next.scene = 2;
    p.publish(next.clone(), 0.06, 0.06, true);
    assert_eq!(p.tracks[&0].sample(0.06)[0], 56.0);
    p.publish(next, 0.07, 0.07, false);
    assert!(!p.is_active());
}
#[test]
fn native_pixel_mismatch_and_unsupported_rooms_use_unmodified_frame() {
    let mut p = publication(vec![layer(0, 40, Blend::And, vec![0; 2])]);
    p.background.fill_visible(0xff);
    assert!(!p.native_composition_matches());
    let native = p.native.indices();
    let colors = p.colors;
    let mut presenter = Presenter::default();
    presenter.publish(p, 0.0, 0.0, true);
    assert!(!presenter.is_active());
    assert_eq!(
        presenter.rgba(0).unwrap(),
        native
            .into_iter()
            .flat_map(|i| colors[i as usize])
            .collect::<Vec<_>>()
    );
    let mut p = publication(vec![layer(0, 40, Blend::And, vec![0; 2])]);
    p.supported = false;
    presenter.publish(p, 0.01, 0.01, true);
    assert!(!presenter.is_active());
}
#[test]
fn publication_does_not_advance_random_state_or_change_native_pixels() {
    let game = Alley::synthetic();
    let before = game.framebuffer().bytes().to_vec();
    let rng = game.selection_trace_state();
    let mut presenter = Presenter::default();
    presenter.observe(&game, 0, 0, true).unwrap();
    for t in 0..120 {
        std::hint::black_box(presenter.rgba(t * 1_000_000_000 / 120));
    }
    assert_eq!(game.framebuffer().bytes().as_slice(), before);
    assert_eq!(game.selection_trace_state(), rng);
    presenter
        .observe(&game, 1_000_000_000, 1_000_000_000, true)
        .unwrap();
    assert_eq!(presenter.observations, 1);
    presenter.observe(&game, 0, 0, true).unwrap();
    assert_eq!(presenter.observations, 1);
}

#[test]
fn coherent_water_rebuilds_complete_actor_instead_of_native_erasure_fragments() {
    let mut p = publication(vec![layer(16, 40, Blend::Opaque, vec![0xff; 2])]);
    p.scene = 2;
    p.coherent_water = true;
    // A native overlapping restore erased a piece of an otherwise complete fish.
    p.native
        .copy_bytes(crate::framebuffer::pixel_offset(44, 40), 1, 1, &[0xaa])
        .unwrap();
    assert!(!p.native_composition_matches());
    let mut presenter = Presenter::default();
    presenter.publish(p, 0.0, 0.0, true);
    assert!(presenter.is_active());
    let rgba = presenter.rgba(0).unwrap();
    assert_eq!(
        rgba[40 * 320 * 4 + 40 * 4..40 * 320 * 4 + 48 * 4],
        [255; 32]
    );
}

#[test]
fn slower_actor_forecast_never_rewinds_between_native_updates() {
    for seconds in [0.055, 0.110] {
        let mut l = layer(1, 40, Blend::Zero, vec![0; 2]);
        l.forecast = Some(Forecast {
            seconds,
            delta: [8.0, 0.0],
        });
        let mut t = Track::new(l.clone(), 0.0, 0.0);
        let mut last = t.sample(0.0)[0];
        for i in 1..=30 {
            let x = t.sample(i as f64 / 120.0)[0];
            assert!(x >= last - 1e-9, "{seconds}: {last} -> {x}");
            last = x;
        }
        // Lateness holds the endpoint, rather than snapping back to the old pose.
        assert_eq!(t.sample(seconds + 0.01), t.sample(seconds + 0.05));
        l.position[0] += 8.0;
        l.native[0] += 8;
        t.update(l, seconds, seconds);
        assert!(t.horizon >= seconds);
    }
}

#[test]
fn dog_velocity_stays_steady_across_alternating_one_and_two_tick_gates() {
    let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
    for direction in [-1.0, 1.0] {
        let mut l = layer(1, 100, Blend::Zero, vec![0; 2]);
        l.smooth_velocity = true;
        l.cadenced = true;
        l.abrupt_turns = true;
        l.stride = [direction * 8.0, 0.0];
        l.cadence_hint = Some(tick * 1.5);
        let mut track = Track::new(l.clone(), 0.0, 0.0);
        let mut at = 0.0;
        let mut speeds = Vec::new();
        for step in 1..=24 {
            let interval = tick * if step % 2 == 0 { 1.0 } else { 2.0 };
            for sample in 1..=12 {
                let elapsed = interval * f64::from(sample) / 12.0;
                let speed = track.velocity_state(elapsed).1[0] * direction;
                if step > 10 {
                    speeds.push(speed);
                }
            }
            at += interval;
            let before = track.velocity_state(interval);
            l.position[0] += direction * 8.0;
            l.observation += 1;
            track.update(l.clone(), at, at);
            let after = track.velocity_state(0.0);
            assert!((before.0[0] - after.0[0]).abs() < 1e-9);
            assert!((before.1[0] - after.1[0]).abs() < 1e-9);
        }
        let low = speeds.iter().copied().fold(f64::INFINITY, f64::min);
        let high = speeds.iter().copied().fold(0.0, f64::max);
        assert!(low > 0.0 && high / low < 1.10, "{low}..{high}");
        // Stop receiving updates: presentation still has a finite endpoint.
        assert!((track.sample(at + 1.0)[0] - track.sample(at + 2.0)[0]).abs() < 1e-6);
    }
}

#[test]
fn dog_reverses_without_opposing_coast_after_a_stationary_edge_step() {
    let tick = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64;
    for direction in [-1.0, 1.0] {
        let mut dog = layer(1, 100, Blend::Zero, vec![0; 2]);
        dog.smooth_velocity = true;
        dog.cadenced = true;
        dog.abrupt_turns = true;
        dog.coast_stops = true;
        dog.stride = [direction * 8.0, 0.0];
        dog.cadence_hint = Some(tick * 1.5);
        let mut track = Track::new(dog.clone(), 0.0, 0.0);
        dog.position[0] += direction * 8.0;
        dog.observation += 1;
        track.update(dog.clone(), tick, tick);
        dog.stride[0] = 0.0;
        dog.observation += 1;
        track.update(dog.clone(), tick * 3.0, tick * 3.0);
        assert!(track.velocity_state(tick).1[0] * direction > 0.0);
        let at = tick * 4.0;
        let before = track.sample(at);
        dog.position[0] -= direction * 8.0;
        dog.stride[0] = -direction * 8.0;
        dog.observation += 1;
        track.update(dog, at, at);
        assert_eq!(track.sample(at), before);
        for frame in 1..12 {
            let velocity = track.velocity_state(f64::from(frame) / 120.0).1[0];
            assert!(velocity * direction <= 0.0, "{direction}: {velocity}");
        }
    }
}

#[test]
fn clothes_keep_runway_across_late_scrolls_without_restarting_velocity() {
    for left in [false, true] {
        let packed = vec![0; 1280];
        let mut band = ClothBand::advance(None, &packed, &packed, 0, left, 1, 0.165);
        let mut track = Track::new(band.layer(0), 0.0, 0.0);
        let mut at = 0.0;
        let mut minimum_speed = f64::INFINITY;
        for step in 1..=36 {
            let interval = [0.183, 0.183, 0.175, 0.233, 0.183, 0.208][step % 6];
            for sample in 1..=24 {
                let elapsed = interval * f64::from(sample) / 24.0;
                let speed = track.velocity_state(elapsed).1[0] * band.direction;
                if step > 10 {
                    minimum_speed = minimum_speed.min(speed);
                }
            }
            at += interval;
            band = ClothBand::advance(
                Some(band),
                &packed,
                &packed,
                0,
                left,
                step as u64 + 1,
                0.165,
            );
            track.update(band.layer(0), at, at);
        }
        assert!(minimum_speed > 1.0, "clothes paused: {minimum_speed}");
        let stopped = track.sample(at + 2.0);
        assert!((stopped[0] - track.sample(at + 3.0)[0]).abs() < 1e-6);
        assert!(stopped[0] >= track.layer.bounds.unwrap().0[0]);
        assert!(stopped[0] <= track.layer.bounds.unwrap().1[0]);
    }
}

#[test]
fn alley_cat_never_rewinds_between_uneven_walking_updates_or_pose_changes() {
    for direction in [-1.0, 1.0] {
        let mut cat = layer(0, 100, Blend::And, vec![0; 2]);
        cat.cadenced = true;
        cat.smooth_velocity = true;
        cat.coast_stops = true;
        cat.settle_stops[0] = true;
        cat.abrupt_turns = true;
        cat.stride = [8.0 * direction, 0.0];
        let mut track = Track::new(cat.clone(), 0.0, 0.0);
        let mut at = 0.0;
        let mut last = track.sample(0.0)[0];
        for step in 1..=40 {
            let interval = if step % 2 == 0 { 0.012 } else { 0.043 };
            for frame in 1..=8 {
                let position = track.sample(at + interval * f64::from(frame) / 8.0)[0];
                assert!((position - last) * direction >= -1e-9);
                last = position;
            }
            at += interval;
            cat.position[0] += 8.0 * direction;
            cat.observation += 1;
            cat.dimensions = Dimensions(if step % 3 == 0 { 0x0c03 } else { 0x0b03 });
            cat.pixels.resize(cat.dimensions.len(), 0xff);
            track.update(cat.clone(), at, at);
            assert!((track.sample(at)[0] - last).abs() < 1e-9);
        }
    }
}

#[test]
fn unreconciled_actor_coasts_to_rest_without_recoiling() {
    for direction in [-1.0, 1.0] {
        let mut actor = layer(1, 100, Blend::And, vec![0; 2]);
        actor.cadenced = true;
        actor.smooth_velocity = true;
        actor.coast_stops = true;
        actor.stride = [8.0 * direction, 0.0];
        actor.cadence_hint = Some(0.04);
        let mut track = Track::new(actor.clone(), 0.0, 0.0);
        let mut at = 0.0;
        for _ in 0..10 {
            at += 0.04;
            actor.position[0] += 8.0 * direction;
            actor.observation += 1;
            track.update(actor.clone(), at, at);
        }
        let mut last = track.sample(at)[0];
        actor.stride = [0.0; 2];
        for _ in 0..20 {
            at += 0.04;
            actor.observation += 1;
            track.update(actor.clone(), at, at);
            let x = track.sample(at)[0];
            assert!((x - last) * direction >= -1e-9);
            last = x;
        }
        assert!(track.velocity_state(0.0).1[0].abs() < 1e-5);
        assert!((track.sample(at + 1.0)[0] - last).abs() < 1e-5);
    }
}

#[test]
fn delayed_cat_lands_continuously_without_overshooting_support() {
    let mut cat = layer(0, 100, Blend::And, vec![0; 2]);
    cat.position[1] = 60.0;
    cat.smooth_velocity = true;
    cat.cadenced = true;
    cat.coast_stops = true;
    cat.stride = [0.0, 4.0];
    cat.cadence_hint = Some(0.04);
    let mut track = Track::new(cat.clone(), 0.0, 0.0);
    cat.position[1] = 64.0;
    cat.observation += 1;
    track.update(cat.clone(), 0.04, 0.04);
    let before = track.sample(0.08);
    cat.position[1] = 72.0;
    cat.stride = [0.0; 2];
    cat.bounds = Some(([8.0, 72.0], [290.0, 72.0]));
    cat.observation += 1;
    track.update(cat, 0.08, 0.08);
    assert_eq!(track.sample(0.08), before);
    let mut last = before[1];
    for sample in 1..=120 {
        let y = track.sample(0.08 + f64::from(sample) / 120.0)[1];
        assert!(y >= last - 1e-9 && y <= 72.0);
        last = y;
    }
    assert!((last - 72.0).abs() < 1e-5);
}

#[test]
fn cat_attachment_and_detachment_preserve_its_displayed_world_position() {
    let mut cat = layer(0, 100, Blend::Opaque, vec![0xff; 2]);
    cat.smooth_velocity = true;
    cat.cadenced = true;
    cat.coast_stops = true;
    cat.abrupt_turns = true;
    cat.position[1] = 60.0;
    cat.native[1] = 60;
    cat.stride = [0.0, 4.0];
    cat.cadence_hint = Some(0.04);
    let mut p = Presenter::default();
    p.publish(publication(vec![cat.clone()]), 0.0, 0.0, true);
    let before = p.position(&cat, 0.04);
    cat.native = [104, 72];
    cat.position = [100.0, 72.0];
    cat.parent = Some(8);
    cat.epoch = 1;
    cat.observation += 1;
    cat.stride = [0.0; 2];
    cat.bounds = Some(([4.0, 72.0], [286.0, 72.0]));
    let parent = layer(8, 4, Blend::Opaque, vec![0xff; 2]);
    p.publish(publication(vec![parent, cat.clone()]), 0.04, 0.04, true);
    assert_eq!(p.position(&cat, 0.04), before);
    let before = p.position(&cat, 0.08);
    cat.native = [104, 68]; // Packed native X is rounded down to four pixels.
    cat.position = [106.0, 68.0];
    cat.parent = None;
    cat.epoch = 0;
    cat.observation += 1;
    cat.stride = [2.0, -4.0];
    cat.bounds = Some(([8.0, 0.0], [290.0, 180.0]));
    // The old garment can disappear before the new free pose is processed.
    p.publish(publication(vec![cat.clone()]), 0.08, 0.08, true);
    assert_eq!(p.position(&cat, 0.08), before);
}

#[test]
fn fixed_clipping_plane_preserves_body_shape_during_fall() {
    let mut l = layer(2, 8, Blend::Opaque, vec![0xff; 12]);
    l.dimensions = Dimensions(0x0601);
    l.native[1] = 38;
    l.position[1] = 38.0;
    l.clip_bottom = 42;
    let mut dst = vec![0; WIDTH * HEIGHT];
    l.paint(&mut dst, l.position);
    assert!(dst[38 * WIDTH + 8..38 * WIDTH + 16].iter().all(|&p| p == 3));
    assert!(dst[41 * WIDTH + 8..41 * WIDTH + 16].iter().all(|&p| p == 3));
    assert!(dst[42 * WIDTH..].iter().all(|&p| p == 0));
    let mut t = Track::new(l.clone(), 0.0, 0.0);
    l.position[1] += 2.0;
    l.native[1] += 2;
    l.forecast = Some(Forecast {
        seconds: 0.055,
        delta: [0.0, 4.0],
    });
    t.update(l, 0.055, 0.055);
    assert!(t.horizon > 0.05);
}

#[test]
fn aquatic_observations_follow_their_cadence_and_hold_without_rewinding() {
    // Actor-ring updates span one/two tick gates and variable drawing work.
    for seconds in [0.033, 0.055, 0.085, 0.110, 0.150] {
        for delta in [[4.0, 1.0], [-2.0, -1.0], [0.0, 3.0]] {
            let mut l = layer(16, 100, Blend::Opaque, vec![0; 2]);
            l.cadenced = true;
            let mut t = Track::new(l.clone(), 0.0, 0.0);
            l.position = std::array::from_fn(|i| l.position[i] + delta[i]);
            t.update(l, seconds, seconds);
            assert!((t.horizon - seconds).abs() < 1e-9);
            let mut last = t.sample(seconds);
            for step in 1..=60 {
                let next = t.sample(seconds + f64::from(step) / 120.0);
                for axis in 0..2 {
                    assert!((next[axis] - last[axis]) * delta[axis] >= -1e-9);
                }
                last = next;
            }
            assert_eq!(
                t.sample(seconds * 2.0 + 0.01),
                t.sample(seconds * 2.0 + 0.2)
            );
        }
    }
}

#[test]
fn aquatic_prediction_stops_at_tank_and_vertical_band_limits() {
    let mut l = layer(16, 298, Blend::Opaque, vec![0; 2]);
    l.position[1] = 63.0;
    l.cadenced = true;
    l.bounds = Some(([0.0, 40.0], [302.0, 64.0]));
    let mut t = Track::new(l.clone(), 0.0, 0.0);
    l.position = [302.0, 64.0];
    t.update(l, 0.11, 0.11);
    let mut previous = t.sample(0.11);
    for i in 1..=60 {
        let next = t.sample(0.11 + f64::from(i) / 120.0);
        assert!(next[0] >= previous[0] && next[0] <= 302.0);
        assert!(next[1] >= previous[1] && next[1] <= 64.0);
        previous = next;
    }
}

#[test]
fn swimmer_preserves_position_and_velocity_across_animation_and_uneven_gates() {
    let mut l = layer(0, 100, Blend::And, vec![0; 2]);
    l.cadenced = true;
    l.smooth_velocity = true;
    l.stride = [6.0, 2.0];
    let mut track = Track::new(l.clone(), 0.0, 0.0);
    let mut previous_at = 0.0;
    for (i, at) in [0.033, 0.083, 0.150, 0.200, 0.283, 0.333, 0.367, 0.434]
        .into_iter()
        .enumerate()
    {
        let before = track.velocity_state(at - previous_at);
        l.position[0] += 6.0;
        l.position[1] += 2.0;
        l.dimensions = Dimensions(if i % 2 == 0 { 0x0902 } else { 0x0d03 });
        l.pixels.resize(l.dimensions.len(), 0);
        track.update(l.clone(), at, at);
        let after = track.velocity_state(0.0);
        for axis in 0..2 {
            assert!((before.0[axis] - after.0[axis]).abs() < 1e-9);
            assert!((before.1[axis] - after.1[axis]).abs() < 1e-9);
        }
        assert!(track.horizon >= 0.080);
        let mut last = track.sample(at);
        for frame in 1..=16 {
            let next = track.sample(at + f64::from(frame) / 120.0);
            assert!(next[0] > last[0], "swimmer paused or rewound at {at}");
            last = next;
        }
        previous_at = at;
    }
    let horizon = track.horizon;
    let before = track.velocity_state(horizon - 1e-8);
    let after = track.velocity_state(horizon + 1e-8);
    for axis in 0..2 {
        assert!((before.0[axis] - after.0[axis]).abs() < 1e-4);
        assert!((before.1[axis] - after.1[axis]).abs() < 1e-3);
        let settled = track.velocity_state(1.0);
        assert!((settled.0[axis] - track.velocity_state(2.0).0[axis]).abs() < 1e-6);
        assert!(settled.1[axis].abs() < 1e-6);
        assert!((settled.0[axis] - track.layer.position[axis]).abs() < 5.0 * l.stride[axis]);
    }
}

#[test]
fn continuous_swimmer_obeys_bounds_and_resets_after_teleport() {
    let mut l = layer(0, 280, Blend::And, vec![0; 2]);
    l.cadenced = true;
    l.smooth_velocity = true;
    l.stride = [6.0, 2.0];
    l.bounds = Some(([8.0, 2.0], [290.0, 179.0]));
    let mut track = Track::new(l.clone(), 0.0, 0.0);
    l.position = [286.0, 42.0];
    track.update(l.clone(), 0.05, 0.05);
    for i in 0..120 {
        let (position, velocity) = track.velocity_state(f64::from(i) / 120.0);
        assert!((280.0..=290.0).contains(&position[0]));
        assert!((2.0..=179.0).contains(&position[1]));
        if position[0] == 290.0 {
            assert_eq!(velocity[0], 0.0);
        }
    }
    l.position[0] = 100.0;
    track.update(l, 0.10, 0.10);
    assert_eq!(track.sample(0.10)[0], 100.0);
    assert_eq!(track.velocity_state(0.0).1, [0.0; 2]);
}

#[test]
fn stationary_swimming_observations_do_not_pollute_restart_cadence() {
    let mut l = layer(0, 100, Blend::And, vec![0; 2]);
    l.cadenced = true;
    l.smooth_velocity = true;
    let mut track = Track::new(l.clone(), 0.0, 0.0);
    // Accepted bodies can leave exactly the same position and artwork. They
    // still count when estimating cadence before movement resumes.
    for step in 1..=20 {
        l.observation = step;
        track.update(l.clone(), step as f64 * 0.05, step as f64 * 0.05);
    }
    l.position[0] += 6.0;
    l.stride = [6.0, 0.0];
    l.observation += 1;
    track.update(l, 1.05, 1.05);
    assert!((track.intervals.iter().sum::<f64>() / 8.0 - 0.05).abs() < 1e-9);
    assert!((110.0..140.0).contains(&track.velocity[0]));
}

#[test]
fn alternating_swimming_gates_do_not_become_large_speed_pulses() {
    let mut l = layer(0, 40, Blend::And, vec![0; 2]);
    l.cadenced = true;
    l.smooth_velocity = true;
    l.stride = [6.0, 0.0];
    let mut track = Track::new(l.clone(), 0.0, 0.0);
    let mut next_at = 0.033;
    let mut step = 0;
    let mut last = None;
    let mut speeds = Vec::new();
    for frame in 0..240 {
        let now = f64::from(frame) / 120.0;
        while next_at <= now {
            step += 1;
            l.position[0] += 6.0;
            l.observation += 1;
            track.update(l.clone(), next_at, next_at);
            next_at += if step % 2 == 0 { 0.033 } else { 0.077 };
        }
        let x = track.sample(now)[0];
        if let Some(previous) = last
            && now > 0.6
        {
            speeds.push((x - previous) * 120.0);
        }
        last = Some(x);
    }
    let mean = speeds.iter().sum::<f64>() / speeds.len() as f64;
    let variance = speeds.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / speeds.len() as f64;
    assert!((100.0..120.0).contains(&mean), "mean={mean}");
    assert!(speeds.iter().all(|&speed| speed > 0.0));
    assert!(
        variance.sqrt() / mean < 0.06,
        "speed variation={}",
        variance.sqrt() / mean
    );
}

#[test]
fn fish_turns_change_visual_direction_without_an_opposite_coast() {
    for stride in [4.0, 2.0] {
        let mut l = layer(16, 100, Blend::Opaque, vec![0; 2]);
        l.cadenced = true;
        l.smooth_velocity = true;
        l.abrupt_turns = true;
        l.cadence_hint = Some(0.110);
        l.stride = [stride, 1.0];
        let mut track = Track::new(l.clone(), 0.0, 0.0);
        l.position[0] += stride;
        l.position[1] += 1.0;
        track.update(l.clone(), 0.110, 0.110);
        let before = track.velocity_state(0.190 - 0.110);
        assert!(before.1[0] > 0.0 && before.1[1] > 0.0);
        l.stride[0] = -stride;
        l.position[0] -= stride;
        l.position[1] += 1.0;
        track.update(l.clone(), 0.190, 0.190);
        let after = track.velocity_state(0.0);
        assert_eq!(after.0, before.0);
        assert_eq!(after.1[0], 0.0);
        assert_eq!(after.1[1], before.1[1]);
        let mut previous = after.0;
        for frame in 1..=20 {
            let next = track.sample(0.190 + f64::from(frame) / 120.0);
            assert!(next[0] < previous[0]);
            previous = next;
        }
    }
}

#[test]
fn fish_cadence_changes_with_the_known_actor_ring_regime() {
    let mut l = layer(16, 100, Blend::Opaque, vec![0; 2]);
    l.cadenced = true;
    l.smooth_velocity = true;
    l.abrupt_turns = true;
    l.cadence_hint = Some(0.055);
    l.stride = [4.0, 1.0];
    let mut track = Track::new(l.clone(), 0.0, 0.0);
    for step in 1..=10 {
        l.position[0] += 4.0;
        track.update(l.clone(), f64::from(step) * 0.055, f64::from(step) * 0.055);
    }
    l.cadence_hint = Some(0.110);
    l.position[0] += 4.0;
    let position = track.sample(0.660);
    track.update(l, 0.660, 0.660);
    assert_eq!(track.sample(0.660), position);
    assert!((track.intervals.iter().sum::<f64>() / 8.0 - 0.110).abs() < 1e-9);
    assert!(track.velocity[0] < 45.0);
}

#[test]
fn a_riding_cat_shares_the_clothes_visual_offset_and_detaches_cleanly() {
    let old = vec![0x55; 1280];
    let mut next = old.clone();
    for row in next.as_chunks_mut::<80>().0 {
        row[0] = 0xaa;
    }
    let band = ClothBand::advance(
        None,
        &old,
        &next,
        crate::framebuffer::pixel_offset(0, 40),
        false,
        1,
        0.11,
    );
    let cloth = band.layer(0);
    let mut player = layer(0, 104, Blend::Opaque, vec![0xff; 2]);
    player.native[1] = 36;
    player.position = [100.0, 36.0];
    player.parent = Some(8);
    player.epoch = band.epoch;
    let mut p = publication(vec![player.clone()]);
    p.background
        .copy_rect(band.offset, Dimensions(0x1028), &next)
        .unwrap();
    p.native = p.background.clone();
    p.native
        .copy_rect(
            crate::framebuffer::pixel_offset(104, 36),
            player.dimensions,
            &player.pixels,
        )
        .unwrap();
    p.layers = vec![cloth.clone(), player.clone()];
    assert!(p.native_composition_matches());
    let mut presenter = Presenter::default();
    presenter.publish(p.clone(), 0.0, 0.0, true);
    for frame in 0..40 {
        let now = f64::from(frame) / 120.0;
        let x = presenter.position(&player, now)[0];
        assert!((x - presenter.position(&cloth, now)[0] - 100.0).abs() < 1e-9);
        let rgba = presenter
            .rgba(u128::from(frame as u64) * 1_000_000_000 / 120)
            .unwrap();
        assert_eq!(
            &rgba[(36 * WIDTH + x.round() as usize) * 4..(36 * WIDTH + x.round() as usize) * 4 + 4],
            &[255; 4]
        );
    }
    player.parent = None;
    player.position[0] = 104.0;
    p.layers[1] = player.clone();
    presenter.publish(p, 0.05, 0.05, true);
    assert_eq!(presenter.position(&player, 0.05)[0], 104.0);
}

#[test]
fn stepped_room_actor_keeps_native_placement_between_publications() {
    let mut dog = layer(1, 8, Blend::Zero, vec![0xff; 2]);
    dog.stepped = true;
    let mut presenter = Presenter::default();
    presenter.publish(publication(vec![dog.clone()]), 0.0, 0.0, true);
    dog.native[0] += 8;
    dog.position[0] += 8.0;
    let p = publication(vec![dog.clone()]);
    presenter.publish(p.clone(), 0.055, 0.055, true);
    assert_eq!(presenter.position(&dog, 0.055), dog.position);
    assert_eq!(presenter.position(&dog, 0.070), dog.position);
    assert_eq!(presenter.position(&dog, 0.100), dog.position);
}

#[test]
fn stopped_indoor_cat_cannot_cross_a_pixel_boundary_from_a_tiny_tail() {
    let mut cat = layer(0, 185, Blend::And, vec![0; 2]);
    cat.indoor_cat = true;
    cat.coast_stops = true;
    cat.smooth_velocity = true;
    cat.cadenced = true;
    cat.settle_stops[0] = true;
    let mut track = Track::new(cat.clone(), 0.0, 0.0);
    track.start[0] = 185.48;
    track.start_velocity[0] = 0.6;
    track.horizon = 0.12;
    let pixel = track.sample(0.02)[0].round();
    assert_eq!(pixel, 185.0);
    cat.observation = 1;
    track.update(cat, 0.02, 0.02);
    for i in 0..=240 {
        assert_eq!(track.sample(0.02 + f64::from(i) / 120.0)[0].round(), pixel);
    }
}

#[test]
fn stopped_cat_converges_from_large_offsets_even_with_zero_remaining_velocity() {
    for side in [-1.0, 1.0] {
        let mut cat = layer(0, 180, Blend::And, vec![0; 2]);
        cat.indoor_cat = true;
        cat.smooth_velocity = true;
        cat.cadenced = true;
        cat.coast_stops = true;
        cat.settle_stops[0] = true;
        let mut track = Track::new(cat.clone(), 0.0, 0.0);
        track.start[0] = 180.0 + side * 12.0;
        track.start_velocity[0] = 0.0;
        let initial = track.sample(0.0);
        cat.observation = 1;
        track.update(cat, 0.001, 0.0);
        assert_eq!(track.sample(0.0), initial);
        let mut previous = 12.0;
        for frame in 1..=120 {
            let now = f64::from(frame) / 120.0;
            let error = (track.sample(now)[0] - 180.0) * side;
            assert!(error >= -1e-9 && error <= previous + 1e-9);
            previous = error;
        }
        assert!((track.sample(0.2)[0] - 180.0).abs() < 0.05);
        assert!((track.sample(1.0)[0] - 180.0).abs() < 1e-9);
    }
}

#[test]
fn imminent_support_limit_prevents_horizontal_overrun_and_landing_recoil() {
    for side in [-1.0, 1.0] {
        let mut cat = layer(0, 180, Blend::And, vec![0; 2]);
        cat.position[0] = 180.0 + side * 4.0;
        cat.indoor_cat = true;
        cat.smooth_velocity = true;
        cat.cadenced = true;
        cat.coast_stops = true;
        cat.settle_stops[0] = true;
        cat.stride = [-side * 4.0, 0.0];
        cat.cadence_hint = Some(0.03);
        cat.bounds = Some(if side > 0.0 {
            ([180.0, 40.0], [290.0, 40.0])
        } else {
            ([8.0, 40.0], [180.0, 40.0])
        });
        let mut track = Track::new(cat.clone(), 0.0, 0.0);
        for frame in 0..=24 {
            assert!((track.sample(f64::from(frame) / 120.0)[0] - 180.0) * side >= 0.0);
        }
        assert_eq!(track.sample(0.12)[0], 180.0);
        cat.position[0] = 180.0;
        cat.stride = [0.0; 2];
        cat.bounds = Some(([8.0, 40.0], [290.0, 40.0]));
        cat.observation += 1;
        track.update(cat, 0.12, 0.12);
        for frame in 0..=120 {
            assert_eq!(track.sample(0.12 + f64::from(frame) / 120.0)[0], 180.0);
        }
    }
}

#[test]
fn late_landing_limit_reconciles_an_already_crossed_display_without_snapping() {
    for axis in 0..2 {
        for side in [-1.0, 1.0] {
            let mut cat = layer(0, 180, Blend::And, vec![0; 2]);
            cat.position[axis] = 180.0 - side * 4.0;
            cat.smooth_velocity = true;
            cat.coast_stops = true;
            cat.settle_stops[axis] = true;
            cat.stride[axis] = side * 4.0;
            cat.cadence_hint = Some(0.03);
            let mut track = Track::new(cat.clone(), 0.0, 0.0);
            track.start[axis] = 180.0 + side * 6.0;
            track.start_velocity[axis] = side * 80.0;
            let before = track.sample(0.01);
            let mut low = [-50.0; 2];
            let mut high = [290.0; 2];
            if side < 0.0 {
                low[axis] = 180.0;
            } else {
                high[axis] = 180.0;
            }
            cat.bounds = Some((low, high));
            cat.observation += 1;
            track.update(cat, 0.01, 0.01);
            assert_eq!(track.sample(0.01), before);
            let mut previous = (before[axis] - 180.0) * side;
            for frame in 1..=120 {
                let error = (track.sample(0.01 + f64::from(frame) / 120.0)[axis] - 180.0) * side;
                assert!(error >= -1e-9 && error <= previous + 1e-9);
                previous = error;
            }
            assert!((track.sample(1.0)[axis] - 180.0).abs() < 1e-5);
        }
    }
}

#[test]
fn stationary_spider_axes_settle_continuously_from_either_side() {
    for axis in 0..2 {
        for side in [-1.0, 1.0] {
            for speed in [-100.0, 100.0] {
                let mut spider = layer(41, 176, Blend::Zero, vec![0; 2]);
                spider.position[1] = 40.0;
                spider.settle_stops = [true; 2];
                spider.smooth_velocity = true;
                spider.cadenced = true;
                spider.coast_stops = true;
                spider.stride = [3.0; 2];
                spider.stride[axis] = 0.0;
                spider.cadence_hint = Some(0.11);
                spider.bounds = Some(([0.0, 8.0], [296.0, 164.0]));
                let target = spider.position[axis];
                let mut track = Track::new(spider, 0.0, 0.0);
                track.start[axis] = target + side * 6.0;
                track.start_velocity[axis] = speed;
                let mut ordinary = track.clone();
                ordinary.layer.settle_stops = [false; 2];
                assert_eq!(track.sample(0.0), track.start);
                let mut previous = 6.0;
                for frame in 1..=120 {
                    let age = f64::from(frame) / 120.0;
                    let state = track.velocity_state(age);
                    let error = (state.0[axis] - target) * side;
                    assert!(error >= -1e-9 && error <= previous + 1e-9);
                    assert_eq!(state.0[1 - axis], ordinary.velocity_state(age).0[1 - axis]);
                    previous = error;
                }
                assert!((track.sample(0.125)[axis] - target).abs() < 0.5);
                assert!((track.sample(1.0)[axis] - target).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn indoor_support_settling_remains_continuous_and_finishes_promptly() {
    let mut cat = layer(0, 100, Blend::And, vec![0; 2]);
    cat.position[1] = 60.0;
    cat.indoor_cat = true;
    cat.smooth_velocity = true;
    cat.cadenced = true;
    cat.coast_stops = true;
    cat.stride = [0.0, 4.0];
    cat.cadence_hint = Some(0.04);
    let mut track = Track::new(cat.clone(), 0.0, 0.0);
    cat.position[1] = 64.0;
    cat.observation += 1;
    track.update(cat.clone(), 0.04, 0.04);
    let before = track.sample(0.08);
    cat.position[1] = 72.0;
    cat.stride = [0.0; 2];
    cat.bounds = Some(([8.0, 72.0], [290.0, 72.0]));
    cat.observation += 1;
    track.update(cat, 0.08, 0.08);
    assert_eq!(track.sample(0.08), before);
    let mut last = before[1];
    for i in 1..=24 {
        let y = track.sample(0.08 + f64::from(i) / 120.0)[1];
        assert!(y >= last - 1e-9 && y <= 72.0);
        last = y;
    }
    assert!((72.0 - track.sample(0.08 + 0.125)[1]).abs() < 0.5);
}

#[test]
fn short_indoor_acceptance_bursts_do_not_exceed_the_sustained_gate_rate() {
    let period = crate::timing::BIOS_TICK_PIT_CLOCKS as f64 / crate::timing::PIT_HZ as f64 / 2.0;
    let mut cat = layer(0, 100, Blend::And, vec![0; 2]);
    cat.indoor_cat = true;
    cat.smooth_velocity = true;
    cat.cadenced = true;
    cat.stride = [2.0, 0.0];
    cat.cadence_hint = Some(period);
    let mut track = Track::new(cat.clone(), 0.0, 0.0);
    for i in 1..=12 {
        cat.position[0] += 2.0;
        cat.observation += 1;
        let at = f64::from(i) * 0.014;
        track.update(cat.clone(), at, at);
        assert!(track.velocity[0] <= 2.0 / period * 1.12 + 1e-9);
    }
}

#[test]
fn stepped_and_smooth_rendering_share_placement_without_changing_native_pixels() {
    let mut cat = layer(0, 100, Blend::And, vec![0; 2]);
    cat.pose_offset = 2.0;
    let p = publication(vec![cat]);
    let native = p.native.bytes().to_vec();
    assert!(p.native_composition_matches());
    let stepped = p.placed_rgba();
    let mut presenter = Presenter::default();
    presenter.publish(p.clone(), 0.0, 0.0, true);
    assert_eq!(stepped, presenter.rgba(0).unwrap());
    assert_eq!(stepped, presenter.stepped_rgba().unwrap());
    let pixel = |x| &stepped[(40 * WIDTH + x) * 4..(40 * WIDTH + x + 1) * 4];
    assert_eq!(pixel(100), [170, 170, 170, 255]);
    assert_eq!(pixel(102), [0, 0, 0, 255]);
    assert_eq!(p.native.bytes().as_slice(), native);
}

#[cfg(feature = "dev-tools")]
#[test]
fn presentation_cycle_keeps_raw_stepped_and_smooth_modes_distinct() {
    assert_eq!(Mode::Original.next(), Mode::Aligned);
    assert_eq!(Mode::Aligned.next(), Mode::Smooth);
    assert_eq!(Mode::Smooth.next(), Mode::History25);
    assert_eq!(Mode::History25.next(), Mode::HistoryRivals25);
    assert_eq!(Mode::HistoryRivals25.next(), Mode::HeadHistory60);
    assert_eq!(Mode::HeadHistory60.next(), Mode::HeadNative60);
    assert_eq!(Mode::HeadNative60.next(), Mode::BirdHistory25);
    assert_eq!(Mode::HistoryRivals40.next(), Mode::BirdVelocity);
    assert_eq!(Mode::BirdVelocity.next(), Mode::HistorySwim25);
    assert_eq!(Mode::BirdHistory25.next(), Mode::HistoryRivals40);
    assert_eq!(Mode::HistorySwim25.next(), Mode::History35);
    assert_eq!(Mode::History35.next(), Mode::History40);
    assert_eq!(Mode::History40.next(), Mode::History60);
    assert_eq!(Mode::History60.next(), Mode::Original);
    assert_eq!(Mode::History40.delay_ns(), Some(40_000_000));
    assert_eq!(Mode::History35.delay_ns(), Some(35_000_000));
    assert_eq!(Mode::History25.delay_ns(), Some(25_000_000));
    assert_eq!(Mode::HistoryRivals25.delay_ns(), Some(25_000_000));
    assert_eq!(Mode::HistoryRivals40.delay_ns(), Some(25_000_000));
    assert_eq!(Mode::HistorySwim25.delay_ns(), Some(25_000_000));
    assert_eq!(Mode::HistoryRivals25.rival_delay_ns(), Some(25_000_000));
    assert_eq!(Mode::HistoryRivals40.rival_delay_ns(), Some(40_000_000));
    assert_eq!(Mode::HistorySwim25.rival_delay_ns(), Some(40_000_000));
    assert_eq!(Mode::History60.delay_ns(), Some(60_000_000));
    // Frontend observation routing must use history even though only an ending
    // switches to its longer noninteractive clock.
    for mode in [Mode::HeadHistory60, Mode::HeadNative60] {
        assert_eq!(mode.delay_ns(), Some(25_000_000));
        assert_eq!(mode.rival_delay_ns(), Some(40_000_000));
    }
}

#[test]
fn bounded_walking_forecast_cannot_accumulate_drift_through_unchanged_draws() {
    for direction in [-1.0, 1.0] {
        let mut rival = layer(48, 100, Blend::Or, vec![0; 2]);
        rival.smooth_velocity = true;
        rival.coast_stops = true;
        rival.settle_stops[0] = true;
        rival.stride[0] = direction * 8.0;
        rival.cadence_hint = Some(0.165);
        rival.bounds = Some(([92.0, 40.0], [108.0, 40.0]));
        let mut track = Track::new(rival.clone(), 0.0, 0.0);
        for frame in 1..=1200 {
            let now = f64::from(frame) / 120.0;
            if frame % 20 == 0 {
                rival.observation += 1;
                track.update(rival.clone(), now, now);
            }
            let at = track.sample(now)[0];
            assert!((92.0..=108.0).contains(&at), "{frame}: {at}");
        }
    }
}

#[test]
fn rival_stop_reconciles_ahead_and_behind_without_a_fast_catchup_burst() {
    for side in [-1.0, 1.0] {
        let mut rival = layer(48, 100, Blend::Or, vec![0; 2]);
        rival.smooth_velocity = true;
        rival.coast_stops = true;
        rival.settle_stops[0] = true;
        rival.stop_response = 0.075;
        let mut track = Track::new(rival, 0.0, 0.0);
        track.start[0] = 100.0 + side * 16.0;
        track.start_velocity[0] = side * 50.0;
        let mut previous = 16.0;
        for frame in 0..=120 {
            let (at, velocity) = track.velocity_state(f64::from(frame) / 120.0);
            let error = (at[0] - 100.0) * side;
            assert!(error >= 0.0 && error <= previous);
            assert!(velocity[0].abs() < 79.0);
            previous = error;
        }
        assert!(previous < 0.001);
    }
}

#[test]
fn opted_in_walking_reversal_keeps_the_native_step_and_existing_offset() {
    for direction in [-1_i8, 1] {
        for stopped in [false, true] {
            let mut rival = layer(48, 100, Blend::Or, vec![0; 2]);
            rival.smooth_velocity = true;
            rival.abrupt_turns = true;
            rival.coast_stops = true;
            rival.settle_stops[0] = true;
            rival.turn_direction = -direction;
            rival.stride[0] = f64::from(-direction) * 8.0;
            rival.cadence_hint = Some(0.165);
            let mut track = Track::new(rival.clone(), 0.0, 0.0);
            track.start[0] += f64::from(-direction) * 6.0;
            track.start_velocity[0] = f64::from(-direction) * 48.0;
            let before = track.sample(0.1)[0];
            rival.position[0] += f64::from(direction) * 8.0;
            rival.stride[0] = if stopped {
                0.0
            } else {
                f64::from(direction) * 8.0
            };
            rival.turn_direction = direction;
            rival.observation += 1;
            track.update(rival, 0.1, 0.1);
            assert!((track.sample(0.1)[0] - before - f64::from(direction) * 8.0).abs() < 1e-9);
            assert_eq!(track.start_velocity[0], 0.0);
            // Subsequent sampling or identical observations cannot repeat it.
            let repeated = track.layer.clone();
            track.update(repeated, 0.11, 0.11);
            assert!((track.sample(0.1)[0] - before - f64::from(direction) * 8.0).abs() < 1e-9);
        }
    }
}

#[test]
fn ordinary_turns_and_sitting_transitions_retain_continuous_placement() {
    for (old_direction, new_direction) in [(0, 0), (1, 0), (0, -1)] {
        let mut actor = layer(48, 100, Blend::Or, vec![0; 2]);
        actor.smooth_velocity = true;
        actor.turn_direction = old_direction;
        actor.stride[0] = 8.0;
        actor.cadence_hint = Some(0.165);
        let mut track = Track::new(actor.clone(), 0.0, 0.0);
        let before = track.sample(0.1);
        actor.position[0] -= 8.0;
        actor.stride[0] = -8.0;
        actor.turn_direction = new_direction;
        actor.observation += 1;
        track.update(actor, 0.1, 0.1);
        assert_eq!(track.sample(0.1), before);
    }
}

#[test]
fn retained_turn_step_shortens_at_a_limit_without_inventing_a_snap() {
    for direction in [-1_i8, 1] {
        for outside in [false, true] {
            let edge = if direction == 1 { 266.0 } else { 36.0 };
            let prior = edge - f64::from(direction) * 8.0;
            let mut rival = layer(48, prior as i32, Blend::Or, vec![0; 2]);
            rival.smooth_velocity = true;
            rival.turn_direction = -direction;
            rival.stride[0] = -f64::from(direction) * 8.0;
            let mut track = Track::new(rival.clone(), 0.0, 0.0);
            track.start[0] = edge + f64::from(direction) * if outside { 2.0 } else { -3.0 };
            let before = track.sample(0.1)[0];
            rival.position[0] = edge;
            rival.turn_direction = direction;
            rival.observation += 1;
            rival.bounds = Some(([36.0, 40.0], [266.0, 40.0]));
            rival.settle_stops[0] = true;
            rival.stride[0] = 0.0;
            track.update(rival, 0.1, 0.1);
            let actual = track.sample(0.1)[0];
            assert_eq!(actual, if outside { before } else { edge });
            let step = (actual - before) * f64::from(direction);
            assert!((0.0..=8.0).contains(&step));
        }
    }
}

#[test]
fn reversal_does_not_repeat_a_step_already_covered_by_prediction() {
    for direction in [-1_i8, 1] {
        let mut rival = layer(48, 100, Blend::Or, vec![0; 2]);
        rival.smooth_velocity = true;
        rival.turn_direction = -direction;
        rival.stride[0] = -f64::from(direction) * 8.0;
        let mut track = Track::new(rival.clone(), 0.0, 0.0);
        track.start[0] = 100.0 + f64::from(direction) * 10.0;
        let before = track.sample(0.1)[0];
        rival.position[0] += f64::from(direction) * 8.0;
        rival.turn_direction = direction;
        rival.observation += 1;
        track.update(rival, 0.1, 0.1);
        assert_eq!(track.sample(0.1)[0], before);
    }
}
