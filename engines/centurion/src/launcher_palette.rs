// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Artwork weights and recovered drawing references, read from selected resources.
use crate::assets::{Assets, Bitmap, Result, word};
use reforge_sdl::launcher::{Palette, PaletteHint};

pub(super) fn extract(assets: &Assets) -> Result<Palette> {
    let mut usage = Usage::default();
    assets.visit_stored_artwork(|pixels, visible| usage.stored(pixels, visible))?;

    // Persistent expansion differs from the stored bitmap at the aliased final
    // byte. The world-map buffer is restored opaquely; arena floor is masked.
    for (bank, name, opaque) in [("MMAP", "MAP.CBM", true), ("MGLAD", "ARENABTM.CBM", false)] {
        let bitmap = assets.cached_bitmap(bank, name)?;
        usage.variant(&bitmap, opaque);
    }

    // Gallery::new restores the lower strip opaquely through palette 3, with
    // the alias applied after translation rather than before it.
    let mut gallery = assets.bitmap("MRACE", "GALLERY2.CBM")?;
    let ds = &assets.data_segment;
    for pixel in &mut gallery.pixels {
        *pixel = ds[0x60f0 + usize::from(*pixel)];
    }
    let end = gallery.pixels.len();
    if end < 2 {
        return Err("Truncated race gallery bitmap".into());
    }
    gallery.pixels[end - 1] = ds[0x60f0 + usize::from(gallery.pixels[end - 2] & 15)];
    usage.variant(&gallery, true);

    // Battle::new chooses an enemy sprite family and remaps its uniform pens
    // for each province. Enumerate all provinces without multiplying weights.
    let base_pens = ds.get(0x7252..0x7262).ok_or("Truncated enemy pens")?;
    let uniforms = ds
        .get(0x7262..0x7262 + 27)
        .ok_or("Truncated enemy uniforms")?;
    for (province, uniform) in uniforms.iter().enumerate() {
        let mut pens: [u8; 16] = base_pens.try_into().unwrap();
        pens[4] = *uniform;
        if pens.iter().any(|pen| *pen > 15) {
            return Err("Invalid enemy sprite pen".into());
        }
        let barbarian = (11..=14).contains(&province) || (23..=26).contains(&province);
        for stem in [
            if barbarian { "BI" } else { "RI" },
            "RC",
            if barbarian { "BG" } else { "RG" },
        ] {
            for bitmap in assets.sprites_with_pens("BATL", stem, Some(&pens))? {
                usage.variant(&bitmap, false);
            }
        }
    }

    // Literal drawing pens in the reconstructed UI, map, combat and race
    // renderers, including the initialized VGA UI pens. These are rendering
    // rules, not an extracted palette or a list of artwork's used colours.
    for index in [0, 1, 2, 4, 6, 7, 8, 9, 15, 17, 23, 26, 54, 82, 86, 116, 128] {
        usage.reference(index);
    }
    for (start, count, stride) in [
        (0x7bda, 16, 4), // Arena floor scanlines.
        (0x87de, 4, 2),  // Race labels.
        (0x7544, 5, 2),  // Battle morale text.
    ] {
        for row in 0..count {
            usage.reference(word(ds, start + row * stride)? as u8);
        }
    }
    usage.palette(&assets.palette)
}

struct Usage([u64; 256]);

impl Default for Usage {
    fn default() -> Self {
        Self([0; 256])
    }
}

impl Usage {
    fn stored(&mut self, pixels: &[u8], visible: Option<&[bool]>) {
        for (position, pixel) in pixels.iter().enumerate() {
            if visible.is_none_or(|mask| mask[position]) {
                self.0[usize::from(*pixel)] += 1;
            }
        }
    }

    fn reference(&mut self, index: u8) {
        self.0[usize::from(index)] = self.0[usize::from(index)].max(1);
    }

    fn variant(&mut self, bitmap: &Bitmap, opaque: bool) {
        for (pixel, visible) in bitmap.pixels.iter().zip(&bitmap.visible) {
            if opaque || *visible {
                self.reference(*pixel);
            }
        }
    }

    fn palette(&self, source: &[[u8; 3]; 256]) -> Result<Palette> {
        let mut colours = Vec::new();
        let mut weights: Vec<u64> = Vec::new();
        for (index, weight) in self.0.iter().copied().enumerate() {
            if weight == 0 {
                continue;
            }
            if index >= 250 {
                return Err("Artwork references an undefined CMAP colour".into());
            }
            let rgb = source[index];
            if let Some(position) = colours.iter().position(|colour| *colour == rgb) {
                weights[position] += weight;
            } else {
                colours.push(rgb);
                weights.push(weight);
            }
        }
        Ok(Palette {
            colours,
            weights: Some(weights),
            hint: PaletteHint::Auto,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_count_visible_storage_once_and_variants_only_add_missing_pens() {
        let mut usage = Usage::default();
        usage.stored(&[0, 1, 1, 2], Some(&[true, true, false, false]));
        usage.stored(&[1, 3], None);
        let bitmap = Bitmap {
            width: 3,
            height: 1,
            origin_x: 0,
            origin_y: 0,
            pixels: vec![1, 2, 4],
            visible: vec![true, true, false],
        };
        usage.variant(&bitmap, false);
        usage.variant(&bitmap, false);
        assert_eq!(&usage.0[..5], &[1, 2, 1, 1, 0]);
        usage.variant(&bitmap, true);
        assert_eq!(&usage.0[..5], &[1, 2, 1, 1, 1]);
    }

    #[test]
    fn output_preserves_index_order_combines_rgb_aliases_and_rejects_undefined_pens() {
        let mut source = [[0; 3]; 256];
        source[2] = [10, 20, 30];
        source[4] = [30, 20, 10];
        source[6] = source[2];
        let mut usage = Usage::default();
        usage.stored(&[6, 4, 2, 6], None);
        let palette = usage.palette(&source).unwrap();
        assert_eq!(palette.colours, [[10, 20, 30], [30, 20, 10]]);
        assert_eq!(palette.weights.unwrap(), [3, 1]);
        usage.reference(250);
        assert!(usage.palette(&source).is_err());
    }
}
