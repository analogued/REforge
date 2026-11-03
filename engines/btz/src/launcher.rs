// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player presentation choices; the interpreter remains frontend independent.

use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};

pub const OPTIONS: &[OptionDefinition] = &[
    OptionDefinition {
        label: "Renderer",
        choices: &[
            Choice {
                value: "graphical",
                label: "Graphical",
                short_label: "Graphical",
                badge: None,
                palette: Some(&crate::display::TEXT_PALETTE),
            },
            Choice {
                value: "terminal",
                label: "Terminal",
                short_label: "Terminal",
                badge: Some("Terminal"),
                palette: Some(&crate::display::TEXT_PALETTE),
            },
        ],
        default: "graphical",
        storage: Storage::TerminalMode,
    },
    DISPLAY,
    WINDOW,
];

/// Change when the title recipe or native rasterization changes.
pub const PREVIEW_REVISION: u32 = 2;
/// Renderer choice does not change the original title screen.
pub const PREVIEW_VARIANT: &str = "text";
pub const PREVIEW_VARIANTS: &[&str] = &[PREVIEW_VARIANT];

/// Decode the original title without advancing wordcode or opening a player host.
/// Read exactly the detector-selected resources; previews never write game files.
pub fn preview(
    files: &std::collections::BTreeMap<&str, std::path::PathBuf>,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    if variant != PREVIEW_VARIANT {
        return Err("Unknown BTZ preview variant".into());
    }
    let read = |name| {
        let path = files
            .get(name)
            .ok_or_else(|| format!("Missing verified preview resource {name}"))?;
        std::fs::read(path).map_err(|error| error.to_string())
    };
    if files.contains_key("TITLE.EXE") {
        let sequence = crate::player::presentation::TitleSequence::from_breakers_executable(&read(
            "TITLE.EXE",
        )?)
        .map_err(|error| error.to_string())?;
        return title_frame(sequence.preview().dos_cells());
    }
    let exe = read("GO.EXE")?;
    let (definition, resources) = crate::player::identify_dos_interpreter(&exe)
        .ok_or_else(|| "Unknown BTZ preview interpreter".to_owned())?;
    let title = || -> Result<_, crate::Error> {
        let image =
            crate::image::StoryImage::from_dos_obj(read("ADISK.OBJ").map_err(crate::Error)?)?;
        if crate::read_word(image.initial_variables(), 0x112, "title message")? == 0xffff {
            return Err(crate::Error("BTZ game has no title screen".into()));
        }
        crate::session::Session::with_interpreter(
            image,
            crate::text::TextCodec::from_dos_resources(&exe, resources)?,
            crate::random::DosRandom::from_dos_resources(&exe, resources)?,
            definition,
        )
    };
    let session = title().map_err(|error| error.to_string())?;
    title_frame(session.screen().dos_cells())
}

fn title_frame(source: &[u8]) -> Result<reforge_sdl::preview::Frame, String> {
    if source.len() != 80 * 25 * 2 {
        return Err("BTZ title must contain 80 by 25 DOS cells".into());
    }
    let mut cells = source.to_vec();
    // Remove only a standalone loading label, retaining credits and the frame.
    for row in cells.as_chunks_mut::<160>().0 {
        let characters: Vec<_> = row.as_chunks::<2>().0.iter().map(|cell| cell[0]).collect();
        let label = b"Loading...";
        if let Some(start) = characters
            .windows(label.len())
            .position(|word| word == label)
            && characters[..start]
                .iter()
                .chain(&characters[start + label.len()..])
                .all(|&byte| matches!(byte, b' ' | 0xba))
        {
            for cell in row[start * 2..(start + label.len()) * 2]
                .as_chunks_mut::<2>()
                .0
            {
                cell[0] = b' ';
            }
        }
    }
    // The physical status line holds the continue prompt. The input cursor is
    // initialized below the divider even before any player input is requested.
    for cell in cells[24 * 80 * 2..].as_chunks_mut::<2>().0 {
        cell[0] = b' ';
    }
    let cursor = (19 * 80 + 1) * 2;
    if cells[cursor] == 0xdb {
        cells[cursor] = b' ';
    }
    let rgba = crate::display::render(&cells, true)?
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
        .collect();
    reforge_sdl::preview::Frame::new_monochrome(crate::windowed::GEOMETRY, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_removes_status_loading_and_cursor_without_altering_title_or_source() {
        let mut source = crate::text::TextScreen::default().dos_cells().to_vec();
        for (row, column, text) in [
            (3, 35, "TITLE"),
            (7, 20, "Loading... Tales"),
            (13, 20, "Copyright and credits"),
            (15, 1, "Loading..."),
            (24, 6, "Press any key to continue."),
        ] {
            for (index, byte) in text.bytes().enumerate() {
                source[(row * 80 + column + index) * 2] = byte;
            }
        }
        let original = source.clone();
        let frame = title_frame(&source).unwrap();
        assert_eq!(source, original);
        let mut expected = original;
        for column in 1..11 {
            expected[(15 * 80 + column) * 2] = b' ';
        }
        for column in 0..80 {
            expected[(24 * 80 + column) * 2] = b' ';
        }
        expected[(19 * 80 + 1) * 2] = b' ';
        let rgba: Vec<_> = crate::display::render(&expected, true)
            .unwrap()
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect();
        assert_eq!(frame.rgba(), rgba);
        assert_eq!(frame.geometry(), crate::windowed::GEOMETRY);
        assert!(frame.is_monochrome());
        assert!(title_frame(&source[..100]).is_err());
    }

    #[test]
    fn preview_requires_a_known_variant_and_explicit_verified_resources() {
        let files = std::collections::BTreeMap::new();
        assert_eq!(
            preview(&files, "unknown").unwrap_err(),
            "Unknown BTZ preview variant"
        );
        assert_eq!(
            preview(&files, PREVIEW_VARIANT).unwrap_err(),
            "Missing verified preview resource GO.EXE"
        );
    }
}
