// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Resized preview pixels and CRT treatment, independent of the surrounding frame.
use reforge_sdl::{egui::ColorImage, preview::Frame};
use std::{collections::VecDeque, sync::Arc};

const BUDGET: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub source: String,
    pub size: [usize; 2],
    pub density: u32,
    pub crt: bool,
}

#[derive(Default)]
pub(super) struct Cache {
    images: VecDeque<(Key, Arc<ColorImage>)>,
    bytes: usize,
    #[cfg(test)]
    pub(super) renders: usize,
}

impl Cache {
    pub(super) fn image(&mut self, key: Key, source: &Frame) -> Arc<ColorImage> {
        if let Some(index) = self.images.iter().position(|(cached, _)| cached == &key) {
            let entry = self.images.remove(index).unwrap();
            let image = entry.1.clone();
            self.images.push_back(entry);
            return image;
        }
        let image = Arc::new(reforge_sdl::preview::render(
            source,
            key.size,
            f32::from_bits(key.density),
            key.crt,
        ));
        #[cfg(test)]
        {
            self.renders += 1;
        }
        self.bytes += image.pixels.len() * std::mem::size_of::<reforge_sdl::egui::Color32>();
        self.images.push_back((key, image.clone()));
        self.trim(BUDGET);
        image
    }

    fn trim(&mut self, budget: usize) {
        // As with final textures, retain one oversized image without allowing
        // successive sizes or sources to accumulate beyond the budget.
        while self.bytes > budget && self.images.len() > 1 {
            let (_, image) = self.images.pop_front().unwrap();
            self.bytes -= image.pixels.len() * std::mem::size_of::<reforge_sdl::egui::Color32>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reforge_sdl::DisplayGeometry;

    #[test]
    fn memory_limit_evicts_the_least_recent_image_and_keeps_one_oversized_result() {
        let source = Frame::new(
            DisplayGeometry::new((2, 2), (4, 3)),
            [80, 100, 140, 255].repeat(4),
        )
        .unwrap();
        let key = |name: &str| Key {
            source: name.into(),
            size: [8, 6],
            density: 1.0_f32.to_bits(),
            crt: false,
        };
        let mut cache = Cache::default();
        let first = cache.image(key("first"), &source);
        cache.image(key("second"), &source);
        assert!(Arc::ptr_eq(&first, &cache.image(key("first"), &source)));
        cache.image(key("third"), &source);
        cache.trim(2 * first.pixels.len() * std::mem::size_of::<reforge_sdl::egui::Color32>());
        assert_eq!(
            cache
                .images
                .iter()
                .map(|(key, _)| key.source.as_str())
                .collect::<Vec<_>>(),
            ["first", "third"]
        );
        let renders = cache.renders;
        assert!(Arc::ptr_eq(&first, &cache.image(key("first"), &source)));
        assert_eq!(cache.renders, renders);
        cache.image(key("second"), &source);
        assert_eq!(cache.renders, renders + 1);
        cache.trim(1);
        assert_eq!(cache.images.len(), 1);
        assert_eq!(cache.images[0].0.source, "second");
        assert_eq!(
            cache.bytes,
            first.pixels.len() * std::mem::size_of::<reforge_sdl::egui::Color32>()
        );
    }
}
