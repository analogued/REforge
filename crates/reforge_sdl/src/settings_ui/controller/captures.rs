// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native software-painter coverage. No original data or saved preferences needed.

use std::{fs, io::BufWriter, path::Path};

use egui::{Color32, Event, FullOutput, Modifiers, Pos2, RawInput, Rect, Vec2};
use reforge_settings::{CONTROLLER_MOVEMENT, ControllerMovement};
use sdl3::{
    pixels::{Color, PixelFormat},
    render::WindowCanvas,
};

use crate::{
    FrameCreator,
    gamepad::ControllerStyle,
    settings::{SettingsMenu, VideoCapabilities},
    settings_selector::InputMode,
    ui::themes::{ResolvedAppearance, ThemeId},
};

struct Frame {
    output: FullOutput,
    diagram: Rect,
    pixels: Vec<u8>,
    size: [u32; 2],
}

fn text(output: &FullOutput, label: &str) -> (Rect, Color32) {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some((
                Rect::from_min_size(text.pos, text.galley.size()),
                text.galley.job.sections[0].format.color,
            )),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing label {label:?}"))
}

fn render(menu: &mut SettingsMenu<'_>, canvas: &mut WindowCanvas, mut events: Vec<Event>) -> Frame {
    let (width, height) = canvas.output_size().unwrap();
    menu.navigation.navigate(&menu.context, &mut events);
    let context = menu.context.clone();
    let screen = Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(width as f32, height as f32) / menu.density,
    );
    let mut input = RawInput {
        screen_rect: Some(screen),
        time: Some(menu.started.elapsed().as_secs_f64()),
        events,
        ..Default::default()
    };
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(menu.density);
    let output = context.run(input, |context| {
        assert!(!crate::settings_ui::draw(menu, context, screen));
        crate::settings_navigation::paint_focus(context);
    });
    let diagram = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh)
                if mesh.vertices.len() == 4
                    && mesh.calc_bounds().width() > 90.0
                    && mesh.calc_bounds().height() > 60.0 =>
            {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .unwrap_or(Rect::NOTHING);
    if diagram.is_positive() {
        let texture = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.calc_bounds() == diagram => Some(mesh.texture_id),
                _ => None,
            })
            .unwrap();
        let manager = context.tex_manager();
        let expected = [diagram.width(), diagram.height()].map(|dimension| {
            (dimension * output.pixels_per_point)
                .round()
                .clamp(1.0, 2048.0) as usize
        });
        assert_eq!(
            manager.read().meta(texture).unwrap().size,
            expected,
            "controller must upload physical-size coverage, including fractional DPI"
        );
    }
    menu.pending_textures.append(output.textures_delta.clone());
    menu.jobs = context.tessellate(output.shapes.clone(), output.pixels_per_point);
    canvas.set_draw_color(Color::RGB(15, 15, 15));
    canvas.clear();
    menu.paint(canvas).unwrap();
    let surface = canvas
        .read_pixels(None)
        .unwrap()
        .convert_format(PixelFormat::RGBA32)
        .unwrap();
    let pitch = surface.pitch() as usize;
    let pixels = surface.with_lock(|bytes| {
        bytes
            .chunks(pitch)
            .flat_map(|row| row[..width as usize * 4].iter().copied())
            .collect()
    });
    Frame {
        output,
        diagram,
        pixels,
        size: [width, height],
    }
}

fn click(menu: &mut SettingsMenu<'_>, canvas: &mut WindowCanvas, point: Pos2) -> Frame {
    menu.navigation.set_mode(&menu.context, InputMode::Mouse);
    for pressed in [true, false] {
        let frame = render(
            menu,
            canvas,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        if !pressed {
            return frame;
        }
    }
    unreachable!()
}

fn write_capture(directory: &Path, name: &str, frame: &Frame) {
    let file = fs::File::create(directory.join(format!("{name}.png"))).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), frame.size[0], frame.size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&frame.pixels)
        .unwrap();
}

fn assert_visible_text(frame: &Frame, label: &str, background: Color32) {
    let (rect, _) = text(&frame.output, label);
    let rect = Rect::from_min_max(
        rect.min * frame.output.pixels_per_point,
        rect.max * frame.output.pixels_per_point,
    );
    let [width, height] = frame.size;
    let mut visible = 0;
    for y in rect.top().max(0.0) as u32..rect.bottom().ceil().min(height as f32) as u32 {
        for x in rect.left().max(0.0) as u32..rect.right().ceil().min(width as f32) as u32 {
            let index = (y as usize * width as usize + x as usize) * 4;
            if frame.pixels[index..index + 3]
                .iter()
                .zip(background.to_array())
                .any(|(&actual, expected)| actual.abs_diff(expected) > 24)
            {
                visible += 1;
            }
        }
    }
    assert!(
        visible >= 15,
        "{label:?} must remain visible in native readback; only {visible} contrast pixels"
    );
}

fn assert_diagram_readback(frame: &Frame, neutral: Color32) {
    let density = frame.output.pixels_per_point;
    let rect = Rect::from_min_max(frame.diagram.min * density, frame.diagram.max * density);
    let [width, height] = frame.size;
    let mut outline = 0;
    for y in rect.top().max(0.0) as u32..rect.bottom().ceil().min(height as f32) as u32 {
        for x in rect.left().max(0.0) as u32..rect.right().ceil().min(width as f32) as u32 {
            let index = (y as usize * width as usize + x as usize) * 4;
            if frame.pixels[index..index + 3]
                .iter()
                .zip(neutral.to_array())
                .all(|(&actual, expected)| actual.abs_diff(expected) <= 24)
            {
                outline += 1;
            }
        }
    }
    assert!(
        outline > 100,
        "fractional-DPI native readback must retain the controller outline; found {outline} pixels"
    );
}

#[test]
#[ignore = "owns SDL video; run serially with SDL_VIDEODRIVER=dummy SDL_RENDER_DRIVER=software"]
fn native_controller_templates_and_themes() {
    let output_dir =
        std::env::var_os("REFORGE_CONTROLLER_CAPTURE_DIR").map(std::path::PathBuf::from);
    if let Some(directory) = &output_dir {
        fs::create_dir_all(directory).unwrap();
    }
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("Controller template integration", 960, 720)
        .hidden()
        .build()
        .unwrap();
    let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
    assert_eq!(canvas.renderer_name, "software");
    let creator = FrameCreator::for_ui(&canvas);
    let mut cases = Vec::new();
    for theme in ThemeId::ALL {
        for enabled in [true, false] {
            cases.push((theme, ControllerStyle::Xbox, [960, 720], enabled, 1.0));
        }
    }
    for style in [ControllerStyle::DualSense, ControllerStyle::Switch] {
        cases.push((ThemeId::CrispLight, style, [960, 720], true, 1.0));
    }
    for theme in [ThemeId::ClassicDark, ThemeId::CrispLight] {
        cases.push((theme, ControllerStyle::Xbox, [630, 470], true, 1.0));
        cases.push((theme, ControllerStyle::Xbox, [960, 720], true, 2.25));
    }
    let mut captured = 0;
    for (theme_id, style, size, enabled, density) in cases {
        let physical = size.map(|size| (size as f32 * density).round() as u32);
        canvas
            .window_mut()
            .set_size(physical[0], physical[1])
            .unwrap();
        let suffix = if density == 1.0 { "" } else { "-225" };
        let theme = ResolvedAppearance::resolve(theme_id).theme;
        let mut menu =
            SettingsMenu::new(&creator, VideoCapabilities::default()).with_alleycat_options();
        menu.set_theme(theme);
        menu.toggle();
        menu.scope = 1;
        menu.page = 3;
        menu.game_editing[3] = enabled;
        menu.draft = Some(Default::default());
        menu.controller_style = style;
        menu.density = density;
        menu.navigation.focus_first = false;
        // Stabilize the native settings window's initial measurements.
        for _ in 0..3 {
            render(&mut menu, &mut canvas, vec![]);
        }
        let directional = render(&mut menu, &mut canvas, vec![]);
        assert_eq!(directional.output.pixels_per_point, density);
        assert_eq!(directional.size, physical);
        if density > 1.0 {
            assert_diagram_readback(&directional, theme.palette.noninteractive.foreground);
        }
        assert!(
            directional.diagram.is_positive(),
            "full controller image must be present after initial layout"
        );
        assert_visible_text(&directional, "Movement", theme.palette.window);
        assert_visible_text(
            &directional,
            "Directional jump",
            theme.palette.inactive.background,
        );
        let movement = text(&directional.output, "Directional jump").0.center();
        let before = menu.draft.clone();
        let switched = click(&mut menu, &mut canvas, movement);
        if enabled {
            if let Some(directory) = &output_dir {
                for (name, frame) in [("directional", &directional), ("button", &switched)] {
                    write_capture(
                        directory,
                        &format!(
                            "{}-{:?}-{}x{}-{name}{suffix}",
                            theme_id.slug(),
                            style,
                            size[0],
                            size[1]
                        ),
                        frame,
                    );
                    captured += 1;
                }
            }
        }
        assert_eq!(
            switched.diagram, directional.diagram,
            "diagram moved: {theme_id:?}, {size:?}, enabled={enabled}"
        );
        if !enabled {
            assert_eq!(
                menu.draft, before,
                "disabled carousel must not edit the draft"
            );
            if let Some(directory) = &output_dir {
                write_capture(
                    directory,
                    &format!(
                        "{}-{:?}-{}x{}-disabled{suffix}",
                        theme_id.slug(),
                        style,
                        size[0],
                        size[1]
                    ),
                    &switched,
                );
                captured += 1;
            }
            continue;
        }
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .alleycat
                .controller_movement,
            ControllerMovement::Button
        );
        let focused = menu
            .context
            .memory(|memory| memory.focused())
            .expect("movement carousel keeps focus");
        assert_visible_text(&switched, "Button jump", theme.palette.active.background);
        let jump_label = format!(
            "{} Jump / Drop",
            crate::controller_art::button_name(style, reforge_settings::FaceButton::West)
        );
        assert_visible_text(&switched, &jump_label, theme.palette.window);
        menu.navigation.set_mode(&menu.context, InputMode::Keyboard);
        let restored = render(
            &mut menu,
            &mut canvas,
            [true, false]
                .into_iter()
                .map(|pressed| Event::Key {
                    key: egui::Key::ArrowRight,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .collect(),
        );
        assert_eq!(
            menu.draft
                .as_ref()
                .unwrap()
                .game_values()
                .get(CONTROLLER_MOVEMENT),
            "directional"
        );
        assert_eq!(
            menu.context.memory(|memory| memory.focused()),
            Some(focused)
        );
        assert_eq!(restored.diagram, directional.diagram);
        assert_eq!(
            text(&restored.output, "Directional jump").0,
            text(&directional.output, "Directional jump").0
        );
        if size[1] < 600 {
            menu.navigation.set_mode(&menu.context, InputMode::Mouse);
            render(
                &mut menu,
                &mut canvas,
                vec![
                    Event::PointerMoved(egui::pos2(size[0] as f32 - 80.0, size[1] as f32 / 2.0)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -600.0),
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            let scrolled = render(&mut menu, &mut canvas, vec![]);
            assert_visible_text(&scrolled, "Input response", theme.palette.window);
            let label = text(&scrolled.output, "Input response").0;
            let clip = scrolled
                .output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Input response" => {
                        Some(shape.clip_rect)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(
                clip.contains_rect(label),
                "small-window Input response must be reachable by scrolling"
            );
            if let Some(directory) = &output_dir {
                write_capture(
                    directory,
                    &format!(
                        "{}-{:?}-{}x{}-scrolled",
                        theme_id.slug(),
                        style,
                        size[0],
                        size[1]
                    ),
                    &scrolled,
                );
                captured += 1;
            }
        }
    }
    eprintln!("Native controller integration passed; wrote {captured} PNG captures.");
}
