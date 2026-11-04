// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Renderer-independent copies of live egui textures, including atlas patches.

use std::{collections::HashMap, sync::Arc};

use egui::{ImageData, TextureId, epaint::ImageDelta};

#[derive(Default)]
pub(super) struct Images(HashMap<TextureId, ImageDelta>);

impl Images {
    pub(super) fn update(&mut self, id: TextureId, delta: &ImageDelta) -> Result<(), String> {
        let Some(pos) = delta.pos else {
            self.0.insert(id, delta.clone());
            return Ok(());
        };
        let full = self.0.get_mut(&id).ok_or("missing egui texture image")?;
        let size = full.image.size();
        let patch = delta.image.size();
        if (0..2).any(|axis| pos[axis] > size[axis] || patch[axis] > size[axis] - pos[axis]) {
            return Err("egui texture patch exceeds image bounds".into());
        }
        match (&mut full.image, &delta.image) {
            (ImageData::Color(image), ImageData::Color(patch_image)) => {
                copy_patch(
                    &mut Arc::make_mut(image).pixels,
                    size[0],
                    &patch_image.pixels,
                    patch,
                    pos,
                );
            }
            (ImageData::Font(image), ImageData::Font(patch_image)) => {
                copy_patch(&mut image.pixels, size[0], &patch_image.pixels, patch, pos);
            }
            _ => return Err("egui texture patch changes image format".into()),
        }
        full.options = delta.options;
        Ok(())
    }

    pub(super) fn reload(&self) -> egui::TexturesDelta {
        egui::TexturesDelta {
            set: self
                .0
                .iter()
                .map(|(id, image)| (*id, image.clone()))
                .collect(),
            free: Vec::new(),
        }
    }

    pub(super) fn remove(&mut self, id: &TextureId) {
        self.0.remove(id);
    }
}

fn copy_patch<T: Copy>(
    target: &mut [T],
    width: usize,
    source: &[T],
    size: [usize; 2],
    pos: [usize; 2],
) {
    for row in 0..size[1] {
        let start = (pos[1] + row) * width + pos[0];
        target[start..start + size[0]].copy_from_slice(&source[row * size[0]..(row + 1) * size[0]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, ColorImage, FontImage, TextureOptions};

    #[test]
    fn reload_retains_patches_options_and_live_images_only() {
        let mut images = Images::default();
        let color = TextureId::Managed(1);
        let font = TextureId::Managed(0);
        images
            .update(
                color,
                &ImageDelta::full(
                    ColorImage::new([4, 3], Color32::RED),
                    TextureOptions::NEAREST,
                ),
            )
            .unwrap();
        images
            .update(
                font,
                &ImageDelta::full(FontImage::new([4, 3]), TextureOptions::LINEAR),
            )
            .unwrap();
        images
            .update(
                color,
                &ImageDelta::partial(
                    [1, 1],
                    ColorImage::new([2, 2], Color32::BLUE),
                    TextureOptions::LINEAR,
                ),
            )
            .unwrap();
        let mut patch = FontImage::new([2, 1]);
        patch.pixels = vec![0.25, 0.75];
        images
            .update(
                font,
                &ImageDelta::partial([2, 1], patch, TextureOptions::NEAREST),
            )
            .unwrap();
        let reload = images.reload();
        assert!(reload.set.iter().all(|(_, delta)| delta.pos.is_none()));
        let color_delta = &reload.set.iter().find(|(id, _)| *id == color).unwrap().1;
        let ImageData::Color(image) = &color_delta.image else {
            panic!("color image")
        };
        assert_eq!(&image.pixels[..4], &[Color32::RED; 4]);
        for row in image.pixels[4..].chunks_exact(4) {
            assert_eq!(
                row,
                [Color32::RED, Color32::BLUE, Color32::BLUE, Color32::RED]
            );
        }
        assert_eq!(color_delta.options, TextureOptions::LINEAR);
        let font_delta = &reload.set.iter().find(|(id, _)| *id == font).unwrap().1;
        let ImageData::Font(image) = &font_delta.image else {
            panic!("font image")
        };
        assert_eq!(&image.pixels[6..8], &[0.25, 0.75]);
        assert_eq!(font_delta.options, TextureOptions::NEAREST);
        images.remove(&color);
        assert_eq!(images.reload().set.len(), 1);
        assert!(
            images
                .update(
                    color,
                    &ImageDelta::partial(
                        [0, 0],
                        ColorImage::new([1, 1], Color32::WHITE),
                        TextureOptions::LINEAR
                    )
                )
                .is_err()
        );
        assert!(
            images
                .update(
                    font,
                    &ImageDelta::partial([4, 0], FontImage::new([1, 1]), TextureOptions::LINEAR)
                )
                .is_err()
        );
    }
}
