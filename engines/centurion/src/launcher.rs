// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-owned fixed VGA preview and display geometry.
use reforge_sdl::launcher::{DISPLAY, OptionDefinition, WINDOW};
pub const OPTIONS: &[OptionDefinition] = &[DISPLAY, WINDOW];
pub const PREVIEW_OPTION: &str = "video";
pub const PREVIEW_VARIANT: &str = "vga";
pub const PREVIEW_VARIANTS: &[&str] = &[PREVIEW_VARIANT];
pub const PREVIEW_REVISION: u32 = 1;

pub fn geometry(_: &std::collections::BTreeMap<String, String>) -> reforge_sdl::DisplayGeometry {
    reforge_sdl::DisplayGeometry::new((320, 200), (320, 240))
}

pub fn preview(
    files: &reforge_inputs::ResolvedFiles,
    variant: &str,
) -> Result<reforge_sdl::preview::Frame, String> {
    if variant != PREVIEW_VARIANT {
        return Err("Unknown Centurion preview variant".into());
    }
    let assets = crate::assets::Assets::load(files, std::path::PathBuf::new())?;
    let session = crate::session::Session::new(&assets, crate::session::Phase::Title, 0, 0)?;
    let frame = session.render(&assets, false)?;
    reforge_sdl::preview::Frame::new(geometry(&Default::default()), rgba(&frame, &assets))
}

pub(crate) fn rgba(frame: &crate::graphics::Frame, assets: &crate::assets::Assets) -> Vec<u8> {
    let palette = frame.palette(assets);
    frame
        .pixels
        .iter()
        .flat_map(|index| {
            let [r, g, b] = palette[usize::from(*index)];
            [r, g, b, 255]
        })
        .collect()
}
