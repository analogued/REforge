// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-owned launcher choices. Default alley colours come from Machine;
//! clean scene generation stays in the engine; the launcher owns presentation.

use crate::alley::Machine;
use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};

const CGA: [[u8; 3]; 4] = Machine::Cga.rgb_colors();
const PCJR: [[u8; 3]; 4] = Machine::Pcjr.rgb_colors();

pub const OPTIONS: &[OptionDefinition] = &[
    OptionDefinition {
        label: "Machine",
        choices: &[
            Choice {
                value: "cga",
                label: "IBM CGA",
                short_label: "CGA",
                badge: Some("CGA"),
                palette: Some(&CGA),
            },
            Choice {
                value: "pcjr",
                label: "IBM PCjr",
                short_label: "PCjr",
                badge: Some("PCjr"),
                palette: Some(&PCJR),
            },
        ],
        default: "cga",
        storage: Storage::Native {
            key: "machine",
            argument: "--machine",
        },
    },
    DISPLAY,
    WINDOW,
];

/// Native dimensions for Settings' aspect labels, independent of Symbolic art.
pub fn geometry(
    _native: &std::collections::BTreeMap<String, String>,
) -> reforge_sdl::DisplayGeometry {
    crate::platform::DISPLAY_GEOMETRY
}

pub const PREVIEW_OPTION: &str = "machine";
/// Change when the representative scene recipe or native rendering changes.
pub const PREVIEW_REVISION: u32 = 3;
/// All visual variants are warmed by the launcher after verification.
pub const PREVIEW_VARIANTS: &[&str] = &["cga", "pcjr"];

/// Run a private, deterministic intro until the cat naturally sits on the
/// fence with the SynSoft footer. No host time, player session or sprite edits.
pub fn preview(
    original: &std::path::Path,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    let machine = match variant {
        "cga" => Machine::Cga,
        "pcjr" => Machine::Pcjr,
        _ => return Err("Unknown preview machine".into()),
    };
    let assets = crate::assets::Assets::load(original)?;
    let mut data = assets.runtime_data();
    crate::assets::set_word(&mut data, 0x2ae5, 0xcafe);
    let mut splash = crate::splash::Splash::new(&data, machine)?;
    let mut clock =
        crate::timing::VirtualClock::new(crate::timing::RetraceProfile::for_machine(machine));
    // Use the ordinary startup loop, including its sound sequencing, random
    // draws, retrace and XT cost profile. Virtual seconds take no wall-time wait.
    for _ in 0..200_000 {
        let step = crate::cli::advance_splash(&mut splash, &mut clock, false)?;
        let player = splash.player();
        if matches!(step.work.draw, Some(crate::player::Draw::Idle))
            && player.entry_state == 0
            && player.y == 96
            && (96..=224).contains(&player.x)
            && splash.synsoft_credit_visible()
        {
            return reforge_sdl::preview::Frame::new(
                crate::platform::DISPLAY_GEOMETRY,
                splash.rgba(),
            );
        }
        if step.finished || clock.elapsed_nanos() > 15_000_000_000 {
            break;
        }
    }
    Err("Intro preview did not reach a sitting pose with the SynSoft credit".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_modes_share_native_and_display_geometry() {
        for mode in ["cga", "pcjr"] {
            let native = std::collections::BTreeMap::from([("machine".into(), mode.into())]);
            let geometry = geometry(&native);
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Original), (4, 3));
            assert_eq!(geometry.ratio(reforge_sdl::AspectRatio::Square), (8, 5));
        }
    }

    #[test]
    fn symbolic_palettes_match_engine_machine_colours() {
        for (choice, machine) in OPTIONS[0].choices.iter().zip([Machine::Cga, Machine::Pcjr]) {
            let expected = machine.colors().map(|[r, g, b, _]| [r, g, b]);
            assert_eq!(choice.palette.unwrap(), expected);
        }
    }
}
