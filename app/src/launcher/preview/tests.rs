// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn frame(density: f32, radius: u8, light: bool, size: Vec2, dimmed: bool) -> FrameKey {
    FrameKey {
        width: size.x.round() as u32,
        height: size.y.round() as u32,
        density: density.to_bits(),
        radius,
        light,
        dimmed,
    }
}

#[test]
fn physical_quads_preserve_native_position_and_span_after_density_conversion() {
    for d in [0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0, 3.5] {
        for position in [-1001.0, -71.0, -1.0, 0.0, 1.0, 3.0, 257.0, 400.0, 10000.0] {
            for width in [1.0, 2.0, 7.0, 113.0, 253.0, 350.0, 414.0, 630.0, 1023.0] {
                let rect =
                    texture_bounds(Pos2::new(position, position), Vec2::new(width, width), d);
                let left = rect.left() * d;
                let right = rect.right() * d;
                assert_eq!(left as i32, position as i32, "{d} {position} {width}");
                assert_eq!(
                    (right - left) as u32,
                    width as u32,
                    "{d} {position} {width}: {left}..{right}"
                );
                assert!((left - position).abs() < 0.01 && (right - position - width).abs() < 0.01);
            }
        }
    }
}

#[test]
fn masks_and_textures_are_reused_until_their_actual_inputs_change() {
    let ctx = egui::Context::default();
    let mut cache = Cache::default();
    let key = TextureKey {
        frame: frame(2.25, 2, false, Vec2::new(253.0, 190.0), false),
        image: None,
        colours: vec![[0, 0, 0], [0, 170, 170], [170, 0, 170], [170, 170, 170]],
    };
    let first = cache.texture(&ctx, key.clone());
    for _ in 0..10 {
        assert_eq!(cache.texture(&ctx, key.clone()).id(), first.id());
    }
    assert_eq!(cache.masks.len(), 1);
    assert_eq!(cache.textures.len(), 1);
    let mask = cache.masks[0].1.clone();
    let mut other = key.clone();
    other.colours[1] = [0, 255, 255];
    assert_ne!(cache.texture(&ctx, other).id(), first.id());
    assert!(Arc::ptr_eq(&mask, &cache.masks[0].1));
    for change in [
        frame(1.0, 2, false, Vec2::new(113.0, 85.0), false),
        frame(2.25, 6, false, Vec2::new(253.0, 190.0), false),
        frame(2.25, 2, true, Vec2::new(253.0, 190.0), false),
        frame(2.25, 2, false, Vec2::new(253.0, 190.0), true),
    ] {
        let mut other = key.clone();
        other.frame = change;
        assert_ne!(cache.texture(&ctx, other).id(), first.id());
    }
    assert_eq!(cache.masks.len(), 5);
    // A new renderer/Gui owns a new context and texture manager, never a global cache.
    for _ in 0..3 {
        let _ = ctx.run(Default::default(), |ctx| {
            paint(
                &ctx.layer_painter(egui::LayerId::background()),
                Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(113.0, 85.0)),
                2,
                false,
                &key.colours,
                false,
            );
        });
    }
    let stored = ctx
        .data(|d| d.get_temp::<Arc<Mutex<Cache>>>(egui::Id::new("launcher-preview-cache")))
        .unwrap();
    assert_eq!(stored.lock().unwrap().textures.len(), 1);
    let other = egui::Context::default();
    assert!(
        other
            .data(|d| d.get_temp::<Arc<Mutex<Cache>>>(egui::Id::new("launcher-preview-cache")))
            .is_none()
    );
}

#[test]
fn resized_entries_evict_old_masks_and_textures_with_bounded_memory() {
    let ctx = egui::Context::default();
    let mut cache = Cache::default();
    for width in 100..112 {
        let _ = cache.texture(
            &ctx,
            TextureKey {
                frame: frame(2.25, 2, false, Vec2::new(width as f32, 85.0), false),
                image: None,
                colours: vec![[170, 0, 170]],
            },
        );
        cache.trim_masks(160_000);
        cache.trim_textures(64_000);
        assert!(cache.mask_bytes <= 160_000 || cache.masks.len() == 1);
        assert!(cache.texture_bytes <= 64_000 || cache.textures.len() == 1);
    }
    assert!(cache.masks.len() < 12);
    assert!(cache.textures.len() < 12);
}

/// Real shared-painter regression, independent X/Y phases and parent surfaces.
/// Run serially on each installed renderer; an optional authored BMP fixture
/// supplies ordinary UI image content without making private data a dependency.
#[test]
#[ignore = "owns SDL video; run alone with SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software (or offscreen/opengl)"]
fn native_composed_previews_preserve_content_frame_and_physical_sampling() {
    use reforge_sdl::{
        FrameCreator,
        sdl3::{
            self,
            pixels::{Color, PixelFormat},
        },
        settings::{Gui, UiMetrics},
    };
    use std::path::PathBuf;
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("Preview regression", 1200, 650)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas();
    let creator = FrameCreator::for_ui(&canvas);
    let mut gui = Gui::new(&creator);
    gui.set_focus_overlay(false);
    native_physical_quads(&mut gui, &mut canvas);
    let screenshot = std::env::var_os("REFORGE_PREVIEW_FIXTURE").map(|path| {
        let surface = sdl3::surface::Surface::load_bmp(path)
            .unwrap()
            .convert_format(PixelFormat::RGBA32)
            .unwrap();
        let mut pixels = Vec::new();
        surface.with_lock(|bytes| {
            for y in 0..surface.height() as usize {
                pixels.extend(
                    bytes[y * surface.pitch() as usize
                        ..y * surface.pitch() as usize + surface.width() as usize * 4]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| Color32::from_rgb(c[0], c[1], c[2])),
                );
            }
        });
        egui::ColorImage {
            size: [surface.width() as usize, surface.height() as usize],
            pixels,
        }
    });
    let capture = std::env::var_os("REFORGE_PREVIEW_CAPTURE").map(PathBuf::from);
    if let Some(path) = &capture {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut cache = Cache::default();
    let mut count = 0;
    let mut maximum = 0;
    let mut reference_maximum = 0;
    let mut generation = std::time::Duration::ZERO;
    for (radius, light) in [(2, false), (6, false), (1, false), (3, true)] {
        for d in [1.0, 1.25, 1.5, 2.25, 3.0] {
            for x in [0.0, 0.25, 0.5, 0.75] {
                for y in [0.0, 0.25, 0.5, 0.75] {
                    let size = if x == y {
                        Vec2::new(184.0, 138.0)
                    } else {
                        Vec2::new(112.3, 84.4)
                    };
                    let rect = Rect::from_min_size(Pos2::new(10.0 + x, 10.0 + y), size);
                    let min = (rect.min * d).round();
                    let max = (rect.max * d).round();
                    let key = frame(d, radius, light, max - min, false);
                    let start = std::time::Instant::now();
                    let mask = cache.mask(key);
                    generation += start.elapsed();
                    let neutral = mask.compose(key.size()[1], |_| key.base());
                    for fixture in 0..3 {
                        let image = mask
                            .compose(key.size()[1], |uv| sample(uv, fixture, screenshot.as_ref()));
                        for bg in [
                            Color32::from_rgb(28, 28, 28),
                            if light {
                                Color32::from_rgb(248, 249, 251)
                            } else {
                                Color32::from_rgb(35, 39, 47)
                            },
                        ] {
                            let position = min - Vec2::splat(MARGIN as f32);
                            let mut handle = None;
                            gui.prepare(
                                UiMetrics {
                                    drawable: (1200, 650),
                                    window: (1200, 650),
                                    pixels_per_point: d,
                                },
                                |ctx| {
                                    let texture = ctx.load_texture(
                                        "preview-fixture",
                                        image.clone(),
                                        TextureOptions::NEAREST,
                                    );
                                    paint_texture(
                                        &ctx.layer_painter(egui::LayerId::background()),
                                        &texture,
                                        position,
                                    );
                                    let p = ctx.layer_painter(egui::LayerId::background());
                                    let reference = Rect::from_min_max(
                                        (min + Vec2::new(600.0, 0.0)) / d,
                                        (max + Vec2::new(600.0, 0.0)) / d,
                                    );
                                    p.rect_filled(reference, radius, key.base());
                                    p.rect_stroke(
                                        reference,
                                        radius,
                                        egui::Stroke::new(
                                            1.0_f32,
                                            if light {
                                                Color32::from_black_alpha(38)
                                            } else {
                                                Color32::from_white_alpha(22)
                                            },
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                    handle = Some(texture);
                                },
                            );
                            canvas.set_draw_color(Color::RGB(bg.r(), bg.g(), bg.b()));
                            canvas.clear();
                            gui.paint(&mut canvas).unwrap();
                            let crop = reforge_sdl::sdl3::rect::Rect::new(
                                position.x as i32,
                                position.y as i32,
                                image.size[0] as u32,
                                image.size[1] as u32,
                            );
                            let surface = canvas
                                .read_pixels(Some(crop))
                                .unwrap()
                                .convert_format(PixelFormat::RGB24)
                                .unwrap();
                            let mut error = 0;
                            surface.with_lock(|bytes| {
                                for row in 0..image.size[1] {
                                    for col in 0..image.size[0] {
                                        let pixel = image.pixels[row * image.size[0] + col];
                                        for c in 0..3 {
                                            let expected = (f32::from(pixel[c])
                                                + f32::from(bg[c])
                                                    * (1.0 - f32::from(pixel.a()) / 255.0))
                                                .round()
                                                .clamp(0.0, 255.0)
                                                as u8;
                                            error = error.max(expected.abs_diff(
                                                bytes[row * surface.pitch() as usize + col * 3 + c],
                                            ));
                                        }
                                    }
                                }
                            });
                            maximum = maximum.max(error);
                            assert!(
                                error <= 2,
                                "{} radius={radius} d={d} x={x} y={y} fixture={fixture} error={error}",
                                canvas.renderer_name
                            );
                            if canvas.renderer_name == "opengl" {
                                let reference_crop = reforge_sdl::sdl3::rect::Rect::new(
                                    position.x as i32 + 600,
                                    position.y as i32,
                                    image.size[0] as u32,
                                    image.size[1] as u32,
                                );
                                let reference = canvas
                                    .read_pixels(Some(reference_crop))
                                    .unwrap()
                                    .convert_format(PixelFormat::RGB24)
                                    .unwrap();
                                let mut error = 0;
                                reference.with_lock(|bytes| {
                                    for row in 0..neutral.size[1] {
                                        for col in 0..neutral.size[0] {
                                            let pixel = neutral.pixels[row * neutral.size[0] + col];
                                            for c in 0..3 {
                                                let expected = (f32::from(pixel[c])
                                                    + f32::from(bg[c])
                                                        * (1.0 - f32::from(pixel.a()) / 255.0))
                                                    .round()
                                                    .clamp(0.0, 255.0)
                                                    as u8;
                                                error = error.max(expected.abs_diff(
                                                    bytes[row * reference.pitch() as usize
                                                        + col * 3
                                                        + c],
                                                ));
                                            }
                                        }
                                    }
                                });
                                reference_maximum = reference_maximum.max(error);
                                assert!(
                                    error <= 2,
                                    "neutral reference: radius={radius} d={d} x={x} y={y} error={error}"
                                );
                            }
                            if let Some(path) = capture
                                .as_ref()
                                .filter(|_| d == 2.25 && x == 0.25 && y == 0.5)
                            {
                                surface
                                    .save_bmp(path.join(format!(
                                        "{}-{radius}-{fixture}-{}.bmp",
                                        canvas.renderer_name,
                                        bg.r()
                                    )))
                                    .unwrap();
                            }
                            count += 1;
                            drop(handle);
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "{}: {count} composed cases, max channel error {maximum}, neutral reference error {reference_maximum}, uncached mask work {generation:?}",
        canvas.renderer_name
    );
}

fn sample(uv: Pos2, fixture: usize, screenshot: Option<&egui::ColorImage>) -> Color32 {
    let x = (uv.x * 64.0).floor().clamp(0.0, 63.0) as usize;
    let y = (uv.y * 48.0).floor().clamp(0.0, 47.0) as usize;
    match fixture {
        0 => {
            if x >= 61 {
                Color32::from_rgb(90, 235, 255)
            } else if y < 3 {
                Color32::WHITE
            } else if (x / 8 + y / 8).is_multiple_of(2) {
                Color32::from_rgb(38, 50, 70)
            } else {
                Color32::from_rgb(100, 130, 170)
            }
        }
        1 => {
            if (x / 4 + y / 4).is_multiple_of(2) {
                Color32::WHITE
            } else {
                Color32::from_gray(22)
            }
        }
        _ => {
            if let Some(image) = screenshot {
                let [w, h] = image.size;
                image.pixels[(uv.y * h as f32).floor().clamp(0.0, (h - 1) as f32) as usize * w
                    + (uv.x * w as f32).floor().clamp(0.0, (w - 1) as f32) as usize]
            } else {
                Color32::from_rgb(
                    (x * 255 / 63) as u8,
                    (y * 255 / 47) as u8,
                    ((63 - x) * 255 / 63) as u8,
                )
            }
        }
    }
}

fn native_physical_quads(
    gui: &mut reforge_sdl::settings::Gui<'_>,
    canvas: &mut reforge_sdl::sdl3::render::WindowCanvas,
) {
    use reforge_sdl::{
        sdl3::{
            pixels::{Color, PixelFormat},
            rect::Rect as PixelRect,
        },
        settings::UiMetrics,
    };
    let mut count = 0;
    for density in [1.0, 1.25, 2.25, 3.0] {
        for width in [1, 2, 113, 253, 350, 414, 630] {
            for x in [0, 1, 257, 400] {
                let pixels: Vec<_> = (0..width * 37)
                    .map(|i| {
                        Color32::from_rgb(
                            (i % width % 251) as u8,
                            (73 + i / width * 5) as u8,
                            (255 - i % width % 239) as u8,
                        )
                    })
                    .collect();
                let mut handle = None;
                gui.prepare(
                    UiMetrics {
                        drawable: (1200, 650),
                        window: (1200, 650),
                        pixels_per_point: density,
                    },
                    |ctx| {
                        let texture = ctx.load_texture(
                            "native-pixel-grid",
                            egui::ColorImage {
                                size: [width, 37],
                                pixels: pixels.clone(),
                            },
                            TextureOptions::NEAREST,
                        );
                        paint_texture(
                            &ctx.layer_painter(egui::LayerId::background()),
                            &texture,
                            Pos2::new(x as f32, 75.0),
                        );
                        handle = Some(texture);
                    },
                );
                canvas.set_draw_color(Color::RGB(28, 28, 28));
                canvas.clear();
                gui.paint(canvas).unwrap();
                let surface = canvas
                    .read_pixels(Some(PixelRect::new(x, 75, width as u32, 37)))
                    .unwrap()
                    .convert_format(PixelFormat::RGB24)
                    .unwrap();
                surface.with_lock(|data| {
                    for row in 0..37 {
                        for col in 0..width {
                            let i = row * surface.pitch() as usize + col * 3;
                            let p = pixels[row * width + col];
                            assert_eq!(
                                &data[i..i + 3],
                                &p.to_array()[..3],
                                "{}: d={density} x={x} width={width} pixel={col},{row}",
                                canvas.renderer_name
                            );
                        }
                    }
                });
                count += 1;
                drop(handle);
            }
        }
    }
    eprintln!(
        "{}: {count} physical quad cases, byte-exact opaque pixels",
        canvas.renderer_name
    );
}

#[test]
#[ignore = "release-profile generation measurements; no timing threshold"]
fn generation_costs_at_compact_large_and_high_density_sizes() {
    for size in [
        Vec2::new(253.0, 190.0),
        Vec2::new(414.0, 311.0),
        Vec2::new(1440.0, 1080.0),
    ] {
        let key = frame(2.25, 6, false, size, false);
        let mut builds = Vec::new();
        let mut compositions = Vec::new();
        let mut palettes = Vec::new();
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let mask = FrameMask::new(key);
            builds.push(start.elapsed());
            let start = std::time::Instant::now();
            let image = mask.compose(key.size()[1], |uv| sample(uv, 2, None));
            compositions.push(start.elapsed());
            assert_eq!(image.size, key.size());
            let start = std::time::Instant::now();
            let _ = mask.compose(
                (MARGIN as f32 + 4.5 * key.density()).ceil() as usize,
                |_| Color32::WHITE,
            );
            palettes.push(start.elapsed());
        }
        builds.sort();
        compositions.sort();
        palettes.sort();
        eprintln!(
            "{}x{}: mask median {:?}, image compose median {:?}, palette compose median {:?}",
            key.width, key.height, builds[1], compositions[1], palettes[1]
        );
    }
}

#[test]
fn real_images_reuse_frames_and_invalidate_textures_for_source_and_treatment() {
    let ctx = egui::Context::default();
    let frame = reforge_sdl::preview::Frame::new(
        reforge_sdl::DisplayGeometry::new((2, 2), (4, 3)),
        [128, 128, 128, 255].repeat(4),
    )
    .unwrap();
    let mut cache = Cache::default();
    let key = TextureKey {
        frame: frame_key_for_image(),
        colours: vec![],
        image: Some(("source-one".into(), false)),
    };
    let plain = cache.texture_with_source(&ctx, key.clone(), Some(&frame));
    assert_eq!(
        plain.id(),
        cache
            .texture_with_source(&ctx, key.clone(), Some(&frame))
            .id()
    );
    let mut crt = key.clone();
    crt.image.as_mut().unwrap().1 = true;
    assert_ne!(
        plain.id(),
        cache.texture_with_source(&ctx, crt, Some(&frame)).id()
    );
    let mut other = key;
    other.image.as_mut().unwrap().0 = "source-two".into();
    assert_ne!(
        plain.id(),
        cache.texture_with_source(&ctx, other, Some(&frame)).id()
    );
    assert_eq!(cache.masks.len(), 1);
}
fn frame_key_for_image() -> FrameKey {
    frame(2.25, 6, false, Vec2::new(350.0, 263.0), false)
}
