// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn scene() -> Alley {
    let mut game = Alley::synthetic();
    set_word(&mut game.data, 4, 7);
    game.screen.fill_visible(0x55);
    game.compositor = PlayerCompositor::default();
    game.data[0x4500..0x4548].fill(0x88);
    for i in 0..16 {
        set_word(&mut game.data, 0x4a60 + i * 2, (0x6200 + i * 72) as u16);
        game.data[0x6200 + i * 72..0x6200 + (i + 1) * 72].fill(0xaa);
    }
    for i in 0..7 {
        let address = 0x6000 + i * 12;
        set_word(&mut game.data, 0x45a8 + i * 2, address as u16);
        set_word(&mut game.data, address, 96);
        game.data[address + 2] = 1;
        game.data[address + 3] = 179 - i as u8 * 24;
        set_word(
            &mut game.data,
            address + 4,
            pixel_offset(96, 179 - i as u8 * 24),
        );
    }
    game.compose_actors().unwrap();
    game
}
fn draw(game: &mut Alley, index: usize, moving: bool) {
    set_word(&mut game.data, 0x45b6, index as u16);
    let layer = game.motion_hearts_cat_layer(index, false, moving).unwrap();
    let offset = pixel_offset(layer.native[0] as u16, layer.native[1] as u8);
    game.screen
        .copy_rect(
            offset,
            layer.dimensions,
            &layer.pixels.iter().map(|p| p | 0x55).collect::<Vec<_>>(),
        )
        .unwrap();
    game.remember_motion_hearts(HeartStep {
        accepted: true,
        drew: true,
        work: Some(Work::CatDraw {
            moving,
            recoil: false,
            pursued: false,
            respawned: false,
            restored: false,
            active_gifts: 0,
            consumed: false,
            arrow: false,
            goal: index == 6,
        }),
        ..Default::default()
    })
    .unwrap();
    game.compose_actors().unwrap();
}
#[test]
fn hearts_peels_arrow_above_rival_and_preserves_every_native_pixel() {
    let mut game = scene();
    draw(&mut game, 2, true);
    game.data[0x6ff0..0x7010].fill(0x22);
    game.data[0x70f2] = 1;
    game.data[0x70f5] = 131;
    game.data[0x70f6] = 1;
    set_word(&mut game.data, 0x70f3, 100);
    let offset = pixel_offset(100, 131);
    let saved = game
        .screen
        .zero_transparent_rect(offset, Dimensions(0x0802), &[0x22; 32])
        .unwrap();
    game.data[0x70cc..0x70ec].copy_from_slice(&saved);
    set_word(&mut game.data, 0x70f8, offset);
    game.remember_motion_hearts(HeartStep {
        accepted: true,
        drew: true,
        work: Some(Work::ArrowDraw {
            spawned: true,
            toggle: false,
        }),
        ..Default::default()
    })
    .unwrap();
    game.compose_actors().unwrap();
    let data = game.data.clone();
    let pixels = game.rgba();
    let publication = game.motion_publication().unwrap();
    assert!(publication.native_composition_matches());
    assert_eq!(
        publication.layers.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![50, 42]
    );
    assert!(publication.background.indices().iter().all(|p| *p == 1));
    assert_eq!(game.data, data);
    assert_eq!(game.rgba(), pixels);
}
#[test]
fn blocked_heart_rival_keeps_its_actual_picture_but_stops_forecasting_motion() {
    let mut game = scene();
    draw(&mut game, 2, true);
    let mut presenter = crate::motion::Presenter::default();
    presenter.observe(&game, 0, 0, true).unwrap();
    let before = presenter
        .samples(50_000_000)
        .into_iter()
        .find(|s| s.actor == 50)
        .unwrap()
        .drawn;
    let old = game.motion_hearts_cats[2].clone().unwrap();
    // The proposed scratch movement was never saved or drawn.
    set_word(&mut game.data, 0x4548, 128);
    game.remember_motion_hearts(HeartStep {
        accepted: true,
        work: Some(Work::CatArrow { moved: true }),
        ..Default::default()
    })
    .unwrap();
    let layer = game.motion_hearts_cats[2].as_ref().unwrap();
    assert_eq!(layer.native, old.native);
    assert_eq!(layer.pixels, old.pixels);
    assert_eq!(layer.position, old.position);
    assert_eq!(layer.stride, [0.0; 2]);
    assert_ne!(layer.observation, old.observation);
    presenter
        .observe(&game, 50_000_000, 50_000_000, true)
        .unwrap();
    assert_eq!(
        presenter
            .samples(50_000_000)
            .into_iter()
            .find(|s| s.actor == 50)
            .unwrap()
            .drawn,
        before
    );
    assert!(
        (presenter
            .samples(500_000_000)
            .into_iter()
            .find(|s| s.actor == 50)
            .unwrap()
            .drawn[0]
            - 96.0)
            .abs()
            < 0.01
    );
    draw(&mut game, 2, false);
    assert_eq!(
        game.motion_hearts_cats[2].as_ref().unwrap().stride,
        [0.0; 2]
    );
}
#[test]
fn occluded_contact_erasure_retires_rival_despite_unchanged_native_pixels_and_flags() {
    let mut game = scene();
    draw(&mut game, 2, true);
    game.player.x = 96;
    game.player.y = 131;
    game.player.vertical_coordinate = 181;
    game.compositor
        .draw(
            &mut game.screen,
            pixel_offset(96, 131),
            Dimensions(0x0c03),
            &[0; 72],
        )
        .unwrap();
    game.compose_actors().unwrap();
    let mut presenter = crate::motion::Presenter::default();
    presenter.observe(&game, 0, 0, true).unwrap();
    assert!(presenter.samples(0).iter().any(|s| s.actor == 50));
    let pixels = game.rgba();
    let step = game
        .room_actor(|_data, screen, _sound| {
            screen.copy_rect(pixel_offset(96, 131), Dimensions(0x0c03), &[0x55; 72])?;
            Ok(HeartStep {
                accepted: true,
                drew: true,
                work: Some(Work::CatContact {
                    goal: false,
                    restored: true,
                    moved: false,
                }),
                ..Default::default()
            })
        })
        .unwrap();
    game.remember_motion_hearts(step).unwrap();
    assert_eq!(game.data[0x6000 + 2 * 12 + 6], 0);
    assert_eq!(game.rgba(), pixels);
    presenter
        .observe(&game, 50_000_000, 50_000_000, true)
        .unwrap();
    assert!(!presenter.samples(50_000_000).iter().any(|s| s.actor == 50));
}
#[test]
fn consumed_gifts_invalidate_images_until_new_draws_and_respawn_starts_a_new_lifetime() {
    let mut game = scene();
    draw(&mut game, 2, true);
    draw(&mut game, 3, true);
    let old_epoch = game.motion_hearts_cats[2].as_ref().unwrap().epoch;
    game.remember_motion_hearts(HeartStep {
        accepted: true,
        drew: true,
        work: Some(Work::Place {
            blocked: false,
            active: 1,
            arrow: false,
            consumed: true,
        }),
        ..Default::default()
    })
    .unwrap();
    assert!(game.motion_hearts_cats.iter().all(Option::is_none));
    draw(&mut game, 2, true);
    assert_ne!(
        game.motion_hearts_cats[2].as_ref().unwrap().epoch,
        old_epoch
    );
}
#[test]
fn hearts_player_full_pose_matches_top_and_bottom_cropped_native_art() {
    for (coordinate, y, hidden, bottom) in [(48, 0, 2, 0), (239, 189, 0, 2), (248, 198, 0, 11)] {
        let mut game = scene();
        game.player.x = 100;
        game.player.y = y;
        game.player.vertical_coordinate = coordinate;
        game.player.vertical_direction = 1;
        game.player.write_data(&mut game.data).unwrap();
        set_word(&mut game.data, 0x567, 0x0c03);
        set_word(&mut game.data, 0x569, 0x6800);
        game.player = Player::from_data(&game.data).unwrap();
        let sprite = game.player.motion_airborne_sprite(&game.data);
        let dim = Dimensions(sprite.dimensions);
        let source = usize::from(sprite.source);
        for (i, p) in game.data[source..source + dim.len()].iter_mut().enumerate() {
            *p = if i % 2 == 0 { 0xc3 } else { 0x3c };
        }
        let visible =
            Dimensions((((dim.rows() - hidden - bottom) as u16) << 8) | (sprite.dimensions & 255));
        let bytes = game.data[source + hidden * dim.width_bytes()
            ..source + (dim.rows() - bottom) * dim.width_bytes()]
            .to_vec();
        game.compositor
            .draw(&mut game.screen, pixel_offset(100, y), visible, &bytes)
            .unwrap();
        game.compose_actors().unwrap();
        let publication = game.motion_publication().unwrap();
        assert!(publication.native_composition_matches());
        let player = publication.layers.iter().find(|l| l.id == 0).unwrap();
        assert_eq!(player.dimensions, dim);
        assert_eq!(player.native[1], i32::from(coordinate) - 50);
        assert_eq!(player.position[1], f64::from(coordinate) - 50.0);
        assert_eq!(player.clip_bottom, 187 + dim.rows() as i32);
        assert_eq!(player.bounds, Some(([36.0, -50.0], [270.0, 198.0])));
    }
}

#[test]
fn failed_arrow_peel_retains_an_overlapping_rival_even_when_its_pixels_match() {
    let mut game = scene();
    draw(&mut game, 2, true);
    game.data[0x6ff0..0x7010].fill(0xff);
    game.data[0x70f2] = 1;
    game.data[0x70f5] = 131;
    game.data[0x70f6] = 1;
    set_word(&mut game.data, 0x70f3, 112);
    let offset = pixel_offset(112, 131);
    let saved = game
        .screen
        .zero_transparent_rect(offset, Dimensions(0x0802), &[0xff; 32])
        .unwrap();
    game.data[0x70cc..0x70ec].copy_from_slice(&saved);
    set_word(&mut game.data, 0x70f8, offset);
    // Damage the arrow outside the cat; overlapping white pixels still match
    // the cat's exact expected image. A cat-only guard would erase arrow pixels.
    game.screen
        .copy_rect(pixel_offset(124, 131), Dimensions(0x0101), &[0xaa; 2])
        .unwrap();
    game.compose_actors().unwrap();
    let publication = game.motion_publication().unwrap();
    assert!(publication.native_composition_matches());
    assert!(!publication.layers.iter().any(|l| l.id == 42 || l.id == 50));
    assert_eq!(publication.background.bytes(), game.framebuffer().bytes());
}

#[cfg(feature = "dev-tools")]
#[test]
fn disabling_fully_occluded_hearts_rivals_invalidates_the_observer_immediately() {
    let mut game = scene();
    draw(&mut game, 2, true);
    game.player.x = 96;
    game.player.y = 131;
    game.player.vertical_coordinate = 181;
    game.compositor
        .draw(
            &mut game.screen,
            pixel_offset(96, 131),
            Dimensions(0x0c03),
            &[0; 72],
        )
        .unwrap();
    game.compose_actors().unwrap();
    let mut presenter = crate::motion::Presenter::default();
    presenter.observe(&game, 0, 0, true).unwrap();
    assert!(presenter.samples(0).iter().any(|s| s.actor == 50));
    let pixels = game.rgba();
    let mut options = DevOptions::default();
    options.set_disabled_actors("hearts-rivals").unwrap();
    game.set_dev_options(options).unwrap();
    assert_eq!(game.rgba(), pixels);
    presenter
        .observe(&game, 50_000_000, 50_000_000, true)
        .unwrap();
    assert!(!presenter.samples(50_000_000).iter().any(|s| s.actor == 50));
}

fn heart_floor(game: &mut Alley) {
    for row in 0..7 {
        game.data[0x2bd4 + row] = 176 - row as u8 * 24;
        game.data[0x2bdb + row] = row as u8 * 18;
    }
    game.data[0x2be2..0x2c62].fill(0);
}
fn player_image(game: &mut Alley) {
    game.compositor.restore(&mut game.screen).unwrap();
    game.compositor
        .draw(
            &mut game.screen,
            pixel_offset(game.player.x, game.player.y),
            Dimensions(0x0101),
            &[0; 2],
        )
        .unwrap();
    game.compose_actors().unwrap();
}

#[test]
fn hearts_edge_jump_that_native_update_cancels_never_predicts_vertical_travel() {
    for (x, direction) in [(36, 0xff), (270, 1)] {
        let mut game = scene();
        heart_floor(&mut game);
        game.player = Player::at_alley_floor(x);
        game.player.y = 181;
        game.player.vertical_coordinate = 231;
        game.player.horizontal_direction = direction;
        game.player.horizontal_step = 6;
        game.player.vertical_direction = 0xff;
        game.player.vertical_step = 8;
        game.player.phase = 1;
        game.player.phase_decrement = 112;
        player_image(&mut game);
        let before = format!("{:?}", game.player);
        let data = game.data.clone();
        let publication = game.motion_publication().unwrap();
        assert!(publication.native_composition_matches());
        let layer = publication.layers.iter().find(|l| l.id == 0).unwrap();
        assert_eq!(layer.stride[1], 0.0);
        assert_eq!(layer.bounds.unwrap().0[1], 181.0);
        assert_eq!(layer.bounds.unwrap().1[1], 181.0);
        assert_eq!(game.data, data);
        assert_eq!(format!("{:?}", game.player), before);
    }
}

#[test]
fn hearts_state_zero_landing_limits_falling_prediction_at_the_known_floor() {
    for nearby_rival in [false, true] {
        let mut game = scene();
        heart_floor(&mut game);
        game.player = Player::at_alley_floor(100);
        game.player.y = 182;
        game.player.vertical_coordinate = 232;
        game.player.horizontal_direction = 1;
        game.player.horizontal_step = 2;
        game.player.vertical_direction = 1;
        game.player.vertical_step = 4;
        if nearby_rival {
            draw(&mut game, 0, true);
        }
        player_image(&mut game);
        let (next, outcome) = game.motion_hearts_next_player().unwrap();
        assert!(outcome.landed);
        assert_eq!(next.support_state, 0);
        assert_eq!(next.y, 181);
        let publication = game.motion_publication().unwrap();
        assert!(publication.native_composition_matches());
        let layer = publication.layers.iter().find(|l| l.id == 0).unwrap();
        assert_eq!(layer.bounds.unwrap().0[1], 181.0);
        assert_eq!(layer.bounds.unwrap().1[1], 181.0);
        assert!(layer.stride[1] > 0.0);
        assert!(layer.settle_stops[1]);
        let mut presenter = crate::motion::Presenter::default();
        presenter.observe(&game, 0, 0, true).unwrap();
        assert_eq!(
            presenter
                .samples(0)
                .iter()
                .find(|s| s.actor == 0)
                .unwrap()
                .drawn[1],
            182.0
        );
    }
}

#[test]
fn imminent_hearts_contact_limits_prediction_and_releases_after_escape() {
    let mut game = scene();
    heart_floor(&mut game);
    game.player = Player::at_alley_floor(104);
    game.player.y = 168;
    game.player.vertical_coordinate = 218;
    game.player.horizontal_direction = 1;
    game.player.horizontal_step = 2;
    game.player.vertical_direction = 0xff;
    game.player.vertical_step = 5;
    let address = usize::from(word(&game.data, 0x45a8 + 2));
    set_word(&mut game.data, address, 99);
    set_word(&mut game.data, address + 4, pixel_offset(99, 155));
    game.data[address + 2] = 0xff;
    draw(&mut game, 1, true);
    player_image(&mut game);
    assert_eq!(game.motion_hearts_cats[1].as_ref().unwrap().stride[0], 0.0);
    let publication = game.motion_publication().unwrap();
    assert!(publication.native_composition_matches());
    let player = publication.layers.iter().find(|l| l.id == 0).unwrap();
    assert_eq!(player.bounds.unwrap().1[0], 105.0);
    assert_eq!(player.bounds.unwrap().0[1], 165.5);
    // A changed player path does not replay an old rival stride mid-interval.
    game.player = Player::at_alley_floor(240);
    game.player.y = 181;
    game.player.vertical_coordinate = 231;
    assert_eq!(game.motion_hearts_cats[1].as_ref().unwrap().stride[0], 0.0);
    // The next accepted rival draw resumes ordinary movement if contact missed.
    draw(&mut game, 1, true);
    assert_eq!(game.motion_hearts_cats[1].as_ref().unwrap().stride[0], -8.0);
}

#[test]
fn repeated_boundary_clamping_does_not_forecast_travel_from_inward_facing() {
    for (index, x, direction) in [(1, 266, 0xff), (6, 37, 1)] {
        let mut game = scene();
        let address = usize::from(word(&game.data, 0x45a8 + index * 2));
        let y = game.data[address + 3];
        set_word(&mut game.data, address, x);
        set_word(&mut game.data, address + 4, pixel_offset(x, y));
        game.data[address + 2] = direction;
        draw(&mut game, index, true);
        // The first observation has no preceding displacement to establish a
        // repeated clamp. A second accepted draw at the same edge does.
        draw(&mut game, index, true);
        let layer = game.motion_hearts_cats[index].as_ref().unwrap();
        assert_eq!(layer.position[0], f64::from(x));
        assert_eq!(layer.stride[0], 0.0);
        assert!(
            game.motion_publication()
                .unwrap()
                .native_composition_matches()
        );
        // A new lifetime and an actual move away must not inherit that hold.
        let fresh = game.motion_hearts_cat_layer(index, true, true).unwrap();
        assert_ne!(fresh.stride[0], 0.0);
        let moved_x = if x == 266 { 258 } else { 45 };
        set_word(&mut game.data, address, moved_x);
        set_word(&mut game.data, address + 4, pixel_offset(moved_x, y));
        let moved = game.motion_hearts_cat_layer(index, false, true).unwrap();
        assert_ne!(moved.stride[0], 0.0);
    }
}

#[test]
fn hearts_rival_forecast_runway_is_local_to_its_native_position() {
    let mut game = scene();
    draw(&mut game, 2, true);
    let layer = game.motion_hearts_cats[2].as_ref().unwrap();
    let (low, high) = layer.bounds.unwrap();
    assert_eq!((low[0], high[0]), (88.0, 104.0));
    assert_eq!(layer.stop_response, 0.025);
    let address = usize::from(word(&game.data, 0x45a8 + 4));
    game.data[address + 2] = 0;
    let sitting = game.motion_hearts_cat_layer(2, false, true).unwrap();
    assert_eq!(sitting.stop_response, 0.075);
}

#[test]
fn only_normal_hearts_walking_opts_into_preserved_reversal_steps() {
    let mut game = scene();
    let address = usize::from(word(&game.data, 0x45a8));
    for (direction, recoil, expected) in [(1, 0, 1), (0xff, 0, -1), (0, 0, 0), (1, 4, 0)] {
        game.data[address + 2] = direction;
        game.data[address + 11] = recoil;
        let layer = game.motion_hearts_cat_layer(0, false, true).unwrap();
        assert_eq!(layer.turn_direction, expected);
    }
}
