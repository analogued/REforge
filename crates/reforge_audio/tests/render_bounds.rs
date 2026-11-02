// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "blip")]

use reforge_audio::render::{BlipRenderer, Config, Error, MAX_LEVEL, SignalRenderer};

const CLOCK_HZ: u32 = 1_193_182;

fn config(chunk_ticks: u32) -> Config {
    Config {
        clock_hz: CLOCK_HZ,
        sample_rate: 48_000,
        chunk_ticks,
    }
}

#[test]
fn rejects_invalid_rates_and_unsafe_frames() {
    let invalid = [
        Config {
            clock_hz: 0,
            ..config(10_000)
        },
        Config {
            sample_rate: 0,
            ..config(10_000)
        },
        config(0),
        Config {
            sample_rate: CLOCK_HZ + 1,
            ..config(10_000)
        },
        Config {
            clock_hz: (1 << 20) + 1,
            sample_rate: 1,
            chunk_ticks: 1,
        },
        Config {
            clock_hz: 48_000,
            sample_rate: 48_000,
            chunk_ticks: 4_000,
        },
        config(u32::MAX),
    ];
    for config in invalid {
        assert!(matches!(
            BlipRenderer::new(config, 0, 0),
            Err(Error::Invalid(_))
        ));
    }
    for config in [
        Config {
            clock_hz: 1 << 20,
            sample_rate: 1,
            chunk_ticks: 1,
        },
        Config {
            clock_hz: 48_000,
            sample_rate: 48_000,
            chunk_ticks: 3_999,
        },
    ] {
        assert!(BlipRenderer::new(config, 0, 0).is_ok());
    }
    assert!(BlipRenderer::new(config(10_000), 0, MAX_LEVEL + 1).is_err());
    assert!(BlipRenderer::new(config(10_000), 0, -MAX_LEVEL - 1).is_err());
}

fn render_pattern(epoch: u64, chunk_ticks: u32) -> Vec<i16> {
    let mut renderer = BlipRenderer::new(config(chunk_ticks), epoch, -MAX_LEVEL).unwrap();
    let mut samples = Vec::new();
    let mut output = |block: &[i16]| {
        samples.extend_from_slice(block);
        Ok(())
    };
    for (offset, level) in [
        (0, MAX_LEVEL),
        (1, -MAX_LEVEL),
        (9_999, MAX_LEVEL),
        (10_000, -MAX_LEVEL),
        (10_000, 0),
        (20_001, MAX_LEVEL),
        (49_999, -MAX_LEVEL),
    ] {
        renderer
            .level_at(epoch + offset, level, &mut output)
            .unwrap();
    }
    renderer.finish_until(epoch + 50_000, &mut output).unwrap();
    assert_eq!(renderer.tick(), epoch + 50_000);
    assert_eq!(renderer.samples_written(), samples.len() as u64);
    samples
}

#[test]
fn large_absolute_epoch_preserves_pcm_and_reaches_max_timestamp() {
    let baseline = render_pattern(0, 10_000);
    assert!(!baseline.is_empty());
    assert_eq!(baseline, render_pattern(u64::MAX - 50_000, 10_000));
    assert_eq!(baseline, render_pattern(u64::MAX - 50_000, 3_279));
}

#[test]
fn initial_level_matches_an_edge_at_the_epoch() {
    for initial in [-MAX_LEVEL, MAX_LEVEL] {
        let mut seeded = BlipRenderer::new(config(10_000), 7, initial).unwrap();
        let mut edged = BlipRenderer::new(config(10_000), 7, 0).unwrap();
        let mut seeded_pcm = Vec::new();
        let mut edged_pcm = Vec::new();
        edged.level_at(7, initial, &mut |_| Ok(())).unwrap();
        seeded
            .finish_until(50_007, &mut |block| {
                seeded_pcm.extend_from_slice(block);
                Ok(())
            })
            .unwrap();
        edged
            .finish_until(50_007, &mut |block| {
                edged_pcm.extend_from_slice(block);
                Ok(())
            })
            .unwrap();
        assert_eq!(seeded_pcm, edged_pcm);
        assert!(seeded_pcm.iter().any(|sample| *sample != 0));
    }
}

fn render_headroom(sample_rate: u32, chunk_ticks: u32) -> Vec<i16> {
    let mut renderer = BlipRenderer::new(
        Config {
            sample_rate,
            ..config(chunk_ticks)
        },
        0,
        -MAX_LEVEL,
    )
    .unwrap();
    let mut pcm = Vec::new();
    let mut output = |block: &[i16]| {
        pcm.extend_from_slice(block);
        Ok(())
    };
    let mut noise = 17_u32;
    for tick in 0..60_000_u64 {
        let level = if tick < 20_000 {
            if (tick / 597) % 2 == 0 {
                MAX_LEVEL
            } else {
                -MAX_LEVEL
            }
        } else if tick < 40_000 {
            if tick % 2 == 0 { MAX_LEVEL } else { -MAX_LEVEL }
        } else {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((noise % (2 * MAX_LEVEL as u32 + 1)) as i32 - i32::from(MAX_LEVEL)) as i16
        };
        if tick % 73 == 0 {
            renderer.level_at(tick, MAX_LEVEL, &mut output).unwrap();
            renderer.level_at(tick, -MAX_LEVEL, &mut output).unwrap();
        }
        renderer.level_at(tick, level, &mut output).unwrap();
    }
    renderer.finish_until(60_000, &mut output).unwrap();
    pcm
}

#[test]
fn full_headroom_handles_rapid_and_coincident_changes_at_multiple_rates() {
    for sample_rate in [8_000, 48_000, 192_000] {
        let pcm = render_headroom(sample_rate, 10_000);
        assert!(!pcm.is_empty());
        assert_eq!(pcm, render_headroom(sample_rate, 3_279));
        assert!(
            pcm.iter()
                .all(|sample| *sample != i16::MIN && *sample != i16::MAX)
        );
        assert!(pcm.iter().any(|sample| *sample > 0));
        assert!(pcm.iter().any(|sample| *sample < 0));
    }
}
