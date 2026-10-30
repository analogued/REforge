// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Preview-local compositing: one cached physical-resolution texture per frame/content.
mod coverage;
pub(super) mod sources;
#[cfg(test)]
mod tests;
use coverage::FrameMask;
use reforge_sdl::egui::{self, Color32, Pos2, Rect, Shape, TextureHandle, TextureOptions, Vec2};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const BASE: Color32 = Color32::from_rgb(16, 19, 24);
const MARGIN: u32 = 2; // Physical pixels for the outer antialiased contour.
const MASK_BUDGET: usize = 16 * 1024 * 1024;
const TEXTURE_BUDGET: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameKey {
    width: u32,
    height: u32,
    density: u32,
    radius: u8,
    light: bool,
    dimmed: bool,
}
impl FrameKey {
    fn density(self) -> f32 {
        f32::from_bits(self.density)
    }
    fn size(self) -> [usize; 2] {
        [
            (self.width + 2 * MARGIN) as usize,
            (self.height + 2 * MARGIN) as usize,
        ]
    }
    fn rect(self) -> Rect {
        let d = self.density();
        Rect::from_min_max(
            Pos2::new(MARGIN as f32 / d, MARGIN as f32 / d),
            Pos2::new(
                (self.width + MARGIN) as f32 / d,
                (self.height + MARGIN) as f32 / d,
            ),
        )
    }
    fn base(self) -> Color32 {
        dim(BASE, self.dimmed)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct TextureKey {
    frame: FrameKey,
    colours: Vec<[u8; 3]>,
    image: Option<(String, bool)>,
}
struct CachedTexture {
    key: TextureKey,
    handle: TextureHandle,
}
#[derive(Default)]
struct Cache {
    masks: VecDeque<(FrameKey, Arc<FrameMask>)>,
    textures: VecDeque<CachedTexture>,
    mask_bytes: usize,
    texture_bytes: usize,
}
impl Cache {
    fn mask(&mut self, key: FrameKey) -> Arc<FrameMask> {
        if let Some(index) = self.masks.iter().position(|(k, _)| *k == key) {
            let item = self.masks.remove(index).unwrap();
            let mask = item.1.clone();
            self.masks.push_back(item);
            return mask;
        }
        let mask = Arc::new(FrameMask::new(key));
        self.mask_bytes += mask.byte_size();
        self.masks.push_back((key, mask.clone()));
        // Keep one oversized visible preview; never accumulate resize intermediates.
        self.trim_masks(MASK_BUDGET);
        mask
    }
    fn texture(&mut self, ctx: &egui::Context, key: TextureKey) -> TextureHandle {
        self.texture_with_source(ctx, key, None)
    }
    fn texture_with_source(
        &mut self,
        ctx: &egui::Context,
        key: TextureKey,
        source: Option<&reforge_sdl::preview::Frame>,
    ) -> TextureHandle {
        if let Some(index) = self.textures.iter().position(|t| t.key == key) {
            let item = self.textures.remove(index).unwrap();
            let handle = item.handle.clone();
            self.textures.push_back(item);
            return handle;
        }
        let mask = self.mask(key.frame);
        let image = if let Some(source) = source {
            let content = reforge_sdl::preview::render(
                source,
                [key.frame.width as usize, key.frame.height as usize],
                key.frame.density(),
                key.image.as_ref().is_some_and(|(_, crt)| *crt),
            );
            mask.compose(key.frame.size()[1], |uv| {
                let x = (uv.x * key.frame.width as f32)
                    .floor()
                    .clamp(0.0, key.frame.width as f32 - 1.0) as usize;
                let y = (uv.y * key.frame.height as f32)
                    .floor()
                    .clamp(0.0, key.frame.height as f32 - 1.0) as usize;
                dim(
                    content.pixels[y * key.frame.width as usize + x],
                    key.frame.dimmed,
                )
            })
        } else {
            mask.compose(
                (MARGIN as f32 + 4.5 * key.frame.density()).ceil() as usize,
                |uv| {
                    if uv.y * key.frame.height as f32 >= 4.5 * key.frame.density()
                        || key.colours.is_empty()
                    {
                        return key.frame.base();
                    }
                    let index = ((uv.x * key.colours.len() as f32).floor() as usize)
                        .min(key.colours.len() - 1);
                    let [r, g, b] = key.colours[index];
                    dim(Color32::from_rgb(r, g, b), key.frame.dimmed)
                },
            )
        };
        let handle = ctx.load_texture("launcher-preview", image, TextureOptions::NEAREST);
        self.texture_bytes += handle.byte_size();
        self.textures.push_back(CachedTexture {
            key,
            handle: handle.clone(),
        });
        self.trim_textures(TEXTURE_BUDGET);
        handle
    }
    fn trim_masks(&mut self, budget: usize) {
        while self.mask_bytes > budget && self.masks.len() > 1 {
            self.mask_bytes -= self.masks.pop_front().unwrap().1.byte_size();
        }
    }
    fn trim_textures(&mut self, budget: usize) {
        while self.texture_bytes > budget && self.textures.len() > 1 {
            self.texture_bytes -= self.textures.pop_front().unwrap().handle.byte_size();
        }
    }
}

pub(super) fn dim(colour: Color32, dimmed: bool) -> Color32 {
    if !dimmed {
        return colour;
    }
    let [r, g, b, a] = colour.to_array();
    let channel = |v: u8| ((u32::from(v) * 130 + 127) / 255) as u8;
    Color32::from_rgba_premultiplied(channel(r), channel(g), channel(b), a)
}

/// Snap once to the destination grid. The mask is local to those bounds, so
/// fractional scrolling positions do not cause unbounded phase cache entries.
pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    radius: u8,
    light: bool,
    colours: &[[u8; 3]],
    dimmed: bool,
) -> Rect {
    let d = painter.pixels_per_point();
    if !rect.is_finite() || !rect.is_positive() || !painter.clip_rect().intersects(rect.expand(1.0))
    {
        return rect;
    }
    let min = (rect.min * d).round();
    let max = (rect.max * d).round();
    let size = max - min;
    if size.x < 1.0 || size.y < 1.0 {
        return rect;
    }
    let key = TextureKey {
        frame: FrameKey {
            width: size.x as u32,
            height: size.y as u32,
            density: d.to_bits(),
            radius,
            light,
            dimmed,
        },
        colours: colours.to_vec(),
        image: None,
    };
    let cache = painter.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::new("launcher-preview-cache"))
            .clone()
    });
    let texture = cache.lock().unwrap().texture(painter.ctx(), key);
    paint_texture(painter, &texture, min - Vec2::splat(MARGIN as f32));
    Rect::from_min_max(min / d, max / d)
}
/// Real images share the same neutral frame/coverage as Symbolic artwork.
/// The entire native-size result is composed once, preventing translucent frame
/// tinting and software-renderer gaps along independently drawn image pieces.
pub(super) fn paint_image(
    painter: &egui::Painter,
    rect: Rect,
    radius: u8,
    light: bool,
    source: &sources::Source,
    crt: bool,
) -> Rect {
    let d = painter.pixels_per_point();
    if !rect.is_finite() || !rect.is_positive() || !painter.clip_rect().intersects(rect.expand(1.0))
    {
        return rect;
    }
    let min = (rect.min * d).round();
    let max = (rect.max * d).round();
    let size = max - min;
    if size.x < 1.0 || size.y < 1.0 || size.x * size.y > 16.0 * 1024.0 * 1024.0 {
        return rect;
    }
    let key = TextureKey {
        frame: FrameKey {
            width: size.x as u32,
            height: size.y as u32,
            density: d.to_bits(),
            radius,
            light,
            dimmed: false,
        },
        colours: vec![],
        image: Some((source.id.clone(), crt)),
    };
    let cache = painter.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::new("launcher-preview-cache"))
            .clone()
    });
    let texture =
        cache
            .lock()
            .unwrap()
            .texture_with_source(painter.ctx(), key, Some(&source.frame));
    paint_texture(painter, &texture, min - Vec2::splat(MARGIN as f32));
    Rect::from_min_max(min / d, max / d)
}

fn paint_texture(painter: &egui::Painter, texture: &TextureHandle, min: Pos2) {
    let bounds = texture_bounds(min, texture.size_vec2(), painter.pixels_per_point());
    painter.add(Shape::image(
        texture.id(),
        bounds,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    ));
}

// Independent endpoints prevent logical addition from shortening a native quad
// (e.g. 350 -> 349.99994 pixels). Choose representable bounds whose converted
// position and span truncate to exactly the intended physical integers in SDL.
fn texture_bounds(min: Pos2, size: Vec2, density: f32) -> Rect {
    fn next(v: f32, up: bool) -> f32 {
        if v == 0.0 {
            return f32::from_bits(if up { 1 } else { 0x8000_0001 });
        }
        f32::from_bits(if (v > 0.0) == up {
            v.to_bits() + 1
        } else {
            v.to_bits() - 1
        })
    }
    fn axis(pixel: f32, span: f32, d: f32) -> (f32, f32) {
        let mut a = pixel / d;
        while (pixel >= 0.0 && a * d < pixel) || (pixel < 0.0 && a * d > pixel) {
            a = next(a, pixel >= 0.0);
        }
        let mut b = (pixel + span) / d;
        while b * d - a * d < span {
            b = next(b, true);
        }
        (a, b)
    }
    let (x0, x1) = axis(min.x, size.x, density);
    let (y0, y1) = axis(min.y, size.y, density);
    Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
}
