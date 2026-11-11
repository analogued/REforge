// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn scene() -> Alley {
    let mut game = Alley::synthetic();
    set_word(&mut game.data, 4, 5);
    game.screen.fill_visible(0xaa);
    game.compositor = PlayerCompositor::default();
    game.data[0x3286] = 1;
    for phase in (0..8).step_by(2) {
        set_word(&mut game.data, 0x40c0 + phase, 0x5000);
    }
    game.data[0x5000..0x5040].fill(0x55);
    game.data[0x40b7] = 1;
    game.data[0x40b8] = 1;
    game
}
fn draw(game: &mut Alley, x: u16, y: u8) {
    set_word(&mut game.data, 0x40b2, x);
    game.data[0x40b4] = y;
    let offset = pixel_offset(x, y);
    set_word(&mut game.data, 0x40ba, offset);
    let saved = game
        .screen
        .zero_transparent_rect(offset, Dimensions(0x0501), &[0x55; 10])
        .unwrap();
    game.data[0x3f2c..0x3f36].copy_from_slice(&saved);
    game.remember_motion_bird(BirdStep {
        drew: true,
        work: Some(crate::birdcage::BirdWork {
            gate: BirdGate::Motion,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    game.compose_actors().unwrap();
}
#[test]
fn bird_publication_keeps_native_default_pixels_and_bounds_its_optional_forecast() {
    let mut game = scene();
    draw(&mut game, 101, 80);
    let data = game.data.clone();
    let p = game.motion_publication().unwrap();
    assert!(p.native_composition_matches());
    let bird = p.layers.iter().find(|l| l.id == 43).unwrap();
    assert!(bird.native_by_default && bird.cadenced && bird.smooth_velocity);
    assert_eq!(bird.native, [100, 80]);
    assert_eq!(bird.position, [101.0, 80.0]);
    assert_eq!(bird.bounds, Some(([97.0, 78.0], [105.0, 82.0])));
    let mut presenter = crate::motion::Presenter::default();
    presenter.observe(&game, 0, 0, true).unwrap();
    assert!(presenter.rgba(100_000_000).unwrap() == game.rgba());
    assert_eq!(game.data, data);
}
#[test]
fn pre_motion_broom_block_holds_the_actual_image_but_restore_without_redraw_retires_it() {
    let mut game = scene();
    draw(&mut game, 100, 80);
    let epoch = game.motion_bird.as_ref().unwrap().epoch;
    game.remember_motion_bird(BirdStep {
        work: Some(crate::birdcage::BirdWork {
            gate: BirdGate::Broom,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let bird = game.motion_bird.as_ref().unwrap();
    assert_eq!(bird.stride, [0.0; 2]);
    assert_eq!(bird.epoch, epoch);
    game.screen
        .copy_rect(
            pixel_offset(100, 80),
            Dimensions(0x0501),
            &game.data[0x3f2c..0x3f36],
        )
        .unwrap();
    game.remember_motion_bird(BirdStep {
        work: Some(crate::birdcage::BirdWork {
            gate: BirdGate::Motion,
            restored: true,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    assert!(game.motion_bird.is_none());
    game.remember_motion_bird(BirdStep {
        work: Some(crate::birdcage::BirdWork {
            gate: BirdGate::Broom,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    assert!(game.motion_bird.is_none());
    draw(&mut game, 104, 80);
    assert_ne!(game.motion_bird.as_ref().unwrap().epoch, epoch);
}
#[test]
fn bird_broom_overlap_keeps_both_native_until_a_clean_bird_redraw() {
    let mut game = scene();
    game.data[0x3286] = 0;
    set_word(&mut game.data, 0x3282, pixel_offset(100, 70));
    draw(&mut game, 100, 80);
    assert!(game.motion_bird_group_native());
    let p = game.motion_publication().unwrap();
    assert!(!p.layers.iter().any(|l| l.id == 43 || l.id == 40));
    // Moving the current broom away does not make its saved older picture safe.
    set_word(&mut game.data, 0x3282, pixel_offset(240, 70));
    assert!(game.motion_bird_group_native());
    game.data[0x3286] = 1;
    draw(&mut game, 104, 80);
    assert!(!game.motion_bird_group_native());
    assert!(
        game.motion_publication()
            .unwrap()
            .layers
            .iter()
            .any(|l| l.id == 43)
    );
}
