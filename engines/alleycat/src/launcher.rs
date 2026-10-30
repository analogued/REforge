// Copyright (c) 2026 analogued. Project license has not yet been selected.
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
pub const PREVIEW_REVISION: u32 = 1;
/// All visual variants are warmed by the launcher after verification.
pub const PREVIEW_VARIANTS: &[&str] = &["cga", "pcjr"];

/// A deterministic static alley, independent of the player's session and seed.
pub fn preview(
    original: &std::path::Path,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    let machine = match variant {
        "cga" => Machine::Cga,
        "pcjr" => Machine::Pcjr,
        _ => return Err("Unknown preview machine".into()),
    };
    let alley = crate::alley::Alley::load(original, 0xfa59, 0, machine)?;
    reforge_sdl::preview::Frame::new(crate::platform::DISPLAY_GEOMETRY, alley.rgba())
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
