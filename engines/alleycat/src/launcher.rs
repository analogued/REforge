// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Engine-owned launcher choices. Native palette colours match Machine::colors;
//! the launcher supplies presentation, shapes and Symbolic artwork.

use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};

const CGA: [[u8; 3]; 4] = [[0, 0, 0], [0, 170, 170], [170, 0, 170], [170, 170, 170]];
const PCJR: [[u8; 3]; 4] = [[0, 0, 0], [85, 255, 85], [85, 85, 255], [255, 255, 255]];

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alley::Machine;

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
