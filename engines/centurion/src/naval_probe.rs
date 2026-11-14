// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Private algorithm comparison entry, using read-only DOS observations.
use crate::assets::{Assets, Result};
use crate::naval_combat::{self as combat, Geometry};
use crate::naval_motion::{self as motion, Clock, put, signed};
use crate::rng::OriginalRng;
pub fn run(assets: &Assets, kind: &str, side: &str, input: &str, output: &str) -> Result<()> {
    let mut ds = std::fs::read(input).map_err(|e| e.to_string())?;
    if ds.len() != 65536 {
        return Err("Expected a complete data segment".into());
    }
    let side = side.parse::<usize>().map_err(|e| e.to_string())?;
    let mut clock = Clock {
        now: u32::from_le_bytes(ds[0x42..0x46].try_into().unwrap()),
        ..Clock::default()
    };
    for i in 0..16 {
        clock.stamps[i] = u32::from_le_bytes(ds[0x46 + i * 4..0x4a + i * 4].try_into().unwrap());
    }
    let mut rng = OriginalRng {
        first: signed(&ds, 0xc8) as u16,
        second: signed(&ds, 0xca) as u16,
    };
    if kind == "render" {
        let frame = crate::naval::render_observation(assets, &ds)?;
        return std::fs::write(output, frame.pixels).map_err(|e| e.to_string());
    }
    if kind == "render-live" {
        let frame = crate::naval::render_live_observation(assets, &ds)?;
        return std::fs::write(output, frame.pixels).map_err(|e| e.to_string());
    }
    if kind == "boarding-step" || kind == "boarding-frame" {
        let (frame, mut updated) = crate::naval::boarding_observation(assets, &ds, &mut rng)?;
        put(&mut updated, 0xc8, rng.first);
        put(&mut updated, 0xca, rng.second);
        return std::fs::write(
            output,
            if kind == "boarding-step" {
                updated
            } else {
                frame.pixels
            },
        )
        .map_err(|e| e.to_string());
    }
    if kind == "resolve" || kind == "resolve-frame" {
        let (frame, mut resolved) =
            crate::naval::resolve_observation(assets, &ds, side as u16, &mut rng)?;
        put(&mut resolved, 0xc8, rng.first);
        put(&mut resolved, 0xca, rng.second);
        return std::fs::write(
            output,
            if kind == "resolve" {
                resolved
            } else {
                frame.pixels
            },
        )
        .map_err(|e| e.to_string());
    }
    let mut geometry = Geometry::new(assets)?;
    for side in 0..2 {
        geometry.bounds[side] = (
            signed(&ds, 0xc3f0 + side * 8),
            signed(&ds, 0xc3f2 + side * 8),
            signed(&ds, 0xc3f4 + side * 8),
            signed(&ds, 0xc3f6 + side * 8),
        );
    }
    let mut returned = None;
    match kind {
        "ai" => {
            if motion::ai(&mut ds, &mut rng) {
                combat::shoot(&mut ds, &mut clock, 0);
            }
        }
        "move" => motion::movement(&mut ds, &mut clock, side),
        "turn" => motion::turn(&mut ds, &mut clock, side),
        "speed" => motion::speed(&mut ds, &mut clock, side),
        "bearing" => {
            let headings = [ds[0xc370], ds[0xc371]];
            motion::bearings(&mut ds)?;
            returned = Some(u16::from(ds[0xc371]));
            ds[0xc370..0xc372].copy_from_slice(&headings);
        }
        "shoot" => combat::shoot(&mut ds, &mut clock, side),
        "arrow" => geometry.arrow(&mut ds, &mut clock, side, &mut rng),
        "fire" => geometry.fire(&mut ds, &mut clock, side, &mut rng),
        "attrition" => {
            returned = Some(
                if crate::naval_fleet::attrition(&mut ds, (side / 16) % 16, side % 16, &mut rng)? {
                    65535
                } else {
                    0
                },
            );
        }
        "ram" => {
            returned = Some(geometry.ram(&mut ds, &mut rng));
        }
        "bounds" => returned = Some(motion::bounds(&mut ds)),
        "board" => returned = Some(geometry.board(&mut ds, side)),
        "melee" => returned = Some(combat::marine_hits(&ds, side / 256, side as u8, &mut rng)?),
        _ => return Err("Unknown naval probe".into()),
    }
    if let Some(value) = returned {
        put(&mut ds, 0xfffe, value);
    }
    put(&mut ds, 0xc8, rng.first);
    put(&mut ds, 0xca, rng.second);
    for i in 0..16 {
        ds[0x46 + i * 4..0x4a + i * 4].copy_from_slice(&clock.stamps[i].to_le_bytes());
    }
    std::fs::write(output, ds).map_err(|e| e.to_string())
}
