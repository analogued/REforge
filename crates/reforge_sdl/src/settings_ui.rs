// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Compact Display page. Editing is separate from activation and persistence.
use crate::settings::{SettingsMenu, VideoCapabilities};
use egui::{Color32, Pos2, Vec2};
use reforge_settings::{Backend, KEYS, Renderer, VideoSettings};
const ACCENT: Color32 = Color32::from_rgb(100, 200, 226);
const PENDING: Color32 = Color32::from_rgb(245, 185, 90);
pub(super) fn style(context: &egui::Context) {
    context.style_mut(|s| {
        s.animation_time = 0.0;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
            s.text_styles.insert(text, egui::FontId::proportional(14.0));
        }
        s.visuals.selection.bg_fill = Color32::from_rgb(43, 103, 131);
        s.visuals.slider_trailing_fill = true;
    });
}
pub(super) fn draw(
    menu: &mut SettingsMenu<'_>,
    context: &egui::Context,
    screen: egui::Rect,
) -> bool {
    let session = menu.creator.session.borrow();
    let saved = session.saved.clone();
    let cli = session.cli.clone();
    let game = session.game.clone();
    let warning = session.warning.clone();
    drop(session);
    let draft = menu.draft.as_mut().expect("open draft");
    let windowed_scale = menu.creator.windowed_scale.get();
    let width = (screen.width() - 36.0).clamp(300.0, 860.0);
    let height = (screen.height() - 48.0).clamp(260.0, 650.0);
    let mut close = false;
    egui::Window::new("Settings")
        .id(egui::Id::new("reforge_settings"))
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size([width, height])
        .show(context, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Editing");
                ui.selectable_value(&mut menu.scope, 0, "Global defaults");
                ui.selectable_value(
                    &mut menu.scope,
                    1,
                    format!(
                        "{} overrides",
                        match game.as_str() {
                            "alley-cat" => "Alley Cat",
                            "mach-3" => "Mach 3",
                            _ => "Game",
                        }
                    ),
                );
            });
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(132.0);
                    ui.add_space(6.0);
                    ui.add_sized([132.0, 32.0], egui::Button::new("Display").selected(true));
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width((width - 164.0).max(180.0));
                    ui.heading("Display");
                    if menu.scope == 1 {
                        let mut inherit = !menu.game_editing;
                        if ui
                            .checkbox(&mut inherit, "Use global defaults for this section")
                            .changed()
                        {
                            menu.game_editing = !inherit;
                            if inherit {
                                draft.game.clear();
                                menu.game_touched = true;
                            }
                        }
                    } else {
                        ui.weak("Shared defaults · individual games can override these values");
                    }
                    ui.add_space(8.0);
                    let base = draft.global_values();
                    let original = if menu.scope == 0 {
                        base
                    } else {
                        draft.game_values()
                    };
                    let mut value = original;
                    let previous = if menu.scope == 0 {
                        saved.global_values()
                    } else {
                        saved.game_values()
                    };
                    let enabled = menu.scope == 0 || menu.game_editing;
                    egui::ScrollArea::vertical()
                        .id_salt("display_content")
                        .max_height((height - 215.0).max(80.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                section(ui, "Window & scaling");
                                row(
                                    ui,
                                    "Window mode",
                                    "window_mode",
                                    value,
                                    base,
                                    previous,
                                    menu.scope,
                                    &cli,
                                    |ui| {
                                        ui.radio_value(&mut value.fullscreen, false, "Windowed");
                                        ui.add_enabled_ui(
                                            menu.capabilities.allow_fullscreen,
                                            |ui| {
                                                ui.radio_value(
                                                    &mut value.fullscreen,
                                                    true,
                                                    "Fullscreen",
                                                );
                                            },
                                        );
                                    },
                                );
                                row(
                                    ui,
                                    "Window scale",
                                    "scale",
                                    value,
                                    base,
                                    previous,
                                    menu.scope,
                                    &cli,
                                    |ui| {
                                        let mut auto = value.scale.is_none();
                                        if ui
                                            .add_enabled(
                                                menu.capabilities.allow_auto || auto,
                                                egui::Checkbox::new(&mut auto, "Auto"),
                                            )
                                            .changed()
                                        {
                                            value.scale = if auto {
                                                None
                                            } else {
                                                Some(windowed_scale.clamp(
                                                    menu.capabilities.min_scale.max(1),
                                                    menu.capabilities.max_scale,
                                                ))
                                            };
                                        }
                                        ui.label(
                                            value
                                                .scale
                                                .map_or("Largest fit".into(), |x| format!("×{x}")),
                                        );
                                    },
                                );
                                ui.add_enabled_ui(
                                    value.scale.is_some()
                                        && !value.fullscreen
                                        && !cli.contains_key("scale"),
                                    |ui| {
                                        let mut n = value.scale.unwrap_or(windowed_scale);
                                        let mut cap = menu.capabilities;
                                        if value.scale.is_none() {
                                            cap.min_scale = cap.min_scale.min(n);
                                            cap.max_scale = cap.max_scale.max(n);
                                        }
                                        scale_slider(ui, &mut n, cap);
                                        if value.scale.is_some() {
                                            value.scale = Some(n);
                                        }
                                    },
                                );
                                section(ui, "Rendering");
                                row(
                                    ui,
                                    "Renderer",
                                    "renderer",
                                    value,
                                    base,
                                    previous,
                                    menu.scope,
                                    &cli,
                                    |ui| {
                                        egui::ComboBox::from_id_salt("renderer")
                                            .width(220.0)
                                            .selected_text(renderer_label(value.renderer))
                                            .show_ui(ui, |ui| {
                                                for r in [
                                                    Renderer::Auto,
                                                    Renderer::Gpu,
                                                    Renderer::Sdl,
                                                    Renderer::Software,
                                                ] {
                                                    ui.selectable_value(
                                                        &mut value.renderer,
                                                        r,
                                                        renderer_label(r),
                                                    );
                                                }
                                            });
                                    },
                                );
                                if value.renderer != original.renderer
                                    && !cli.contains_key("backend")
                                    && !backends(value.renderer).contains(&value.backend)
                                {
                                    value.backend = Backend::Auto;
                                }
                                row(
                                    ui,
                                    "Backend",
                                    "backend",
                                    value,
                                    base,
                                    previous,
                                    menu.scope,
                                    &cli,
                                    |ui| {
                                        ui.add_enabled_ui(
                                            value.renderer != Renderer::Software,
                                            |ui| {
                                                let choices = backends(value.renderer);
                                                egui::ComboBox::from_id_salt("backend")
                                                    .width(220.0)
                                                    .selected_text(backend_label(value.backend))
                                                    .show_ui(ui, |ui| {
                                                        for backend in choices {
                                                            ui.selectable_value(
                                                                &mut value.backend,
                                                                backend,
                                                                backend_label(backend),
                                                            );
                                                        }
                                                    });
                                            },
                                        );
                                    },
                                );
                                row(
                                    ui,
                                    "VSync",
                                    "vsync",
                                    value,
                                    base,
                                    previous,
                                    menu.scope,
                                    &cli,
                                    |ui| {
                                        ui.checkbox(
                                            &mut value.vsync,
                                            "Synchronise with the display",
                                        );
                                    },
                                );
                            });
                        });
                    for key in KEYS {
                        if original.get(key) != value.get(key) && !cli.contains_key(key) {
                            draft.edit(menu.scope == 1, key, value.get(key));
                            if menu.scope == 1 {
                                menu.game_touched = true;
                            }
                        }
                    }
                });
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(if menu.scope == 0 {
                        "Reset section"
                    } else {
                        "Restore global defaults"
                    })
                    .clicked()
                {
                    if menu.scope == 0 {
                        draft.global.clear();
                    } else {
                        draft.game.clear();
                        menu.game_editing = false;
                        menu.game_touched = true;
                    }
                }
                let apply = ui.add_enabled(
                    *draft != saved || menu.error.is_some(),
                    egui::Button::new("Apply"),
                );
                #[cfg(test)]
                {
                    menu.apply_rect = Some(apply.rect);
                }
                if apply.clicked() {
                    if menu.game_touched {
                        let baseline = draft.global_values();
                        draft.game.retain(|key, v| *v != baseline.get(key));
                    }
                    menu.pending = Some(draft.clone());
                }
                let cancel = ui.button("Cancel");
                #[cfg(test)]
                {
                    menu.cancel_rect = Some(cancel.rect);
                }
                if cancel.clicked() {
                    close = true;
                }
                ui.weak(if *draft != saved {
                    "Unapplied changes"
                } else {
                    "No pending changes"
                });
            });
            if let Some(error) = menu.error.as_ref().or(warning.as_ref()) {
                ui.colored_label(Color32::from_rgb(255, 169, 144), error);
            }
            if let Some(status) = &menu.status {
                ui.weak(status);
            }
        });
    close
}
fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.0);
    ui.strong(label);
    ui.separator();
}
fn renderer_label(r: Renderer) -> &'static str {
    match r {
        Renderer::Auto => "Automatic · GPU preferred",
        Renderer::Gpu => "GPU",
        Renderer::Sdl => "SDL renderer",
        Renderer::Software => "Software",
    }
}
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    value: VideoSettings,
    global: VideoSettings,
    saved: VideoSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
    control: impl FnOnce(&mut egui::Ui),
) {
    let locked = cli.contains_key(key);
    let different = scope == 1 && value.get(key) != global.get(key);
    let dirty = value.get(key) != saved.get(key);
    ui.horizontal(|ui| {
        let colour = if different && !locked {
            ACCENT
        } else {
            ui.visuals().text_color()
        };
        ui.add_sized(
            [145.0, 22.0],
            egui::Label::new(egui::RichText::new(label).color(colour)),
        )
        .on_hover_text(format!(
            "Global: {}\nSaved: {}",
            global.get(key),
            saved.get(key)
        ));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(220.0, 22.0), egui::Sense::hover());
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if different && !locked {
            child.visuals_mut().override_text_color = Some(ACCENT);
        }
        child.add_enabled_ui(!locked, control);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(12.0, 22.0), egui::Sense::hover());
        if dirty {
            ui.painter().circle_filled(rect.center(), 3.0, PENDING);
            response.on_hover_text(format!("Unapplied change. Saved: {}", saved.get(key)));
        }
    });
    if let Some(active) = cli.get(key) {
        ui.weak(format!(
            "This run: {active}, set by --{}",
            key.replace('_', "-")
        ));
    }
}
fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::Auto => "Automatic",
        Backend::Vulkan => "Vulkan",
        Backend::OpenGl => "OpenGL",
        Backend::OpenGlEs => "OpenGL ES 2",
        Backend::Direct3d11 => "Direct3D 11",
        Backend::Direct3d12 => "Direct3D 12",
        Backend::Metal => "Metal",
    }
}
fn backends(renderer: Renderer) -> Vec<Backend> {
    let mut values = vec![Backend::Auto];
    for backend in [
        Backend::Vulkan,
        Backend::OpenGl,
        Backend::OpenGlEs,
        Backend::Direct3d11,
        Backend::Direct3d12,
        Backend::Metal,
    ] {
        let gpu = matches!(
            backend,
            Backend::Vulkan | Backend::Direct3d12 | Backend::Metal
        );
        if renderer == Renderer::Gpu && !gpu || renderer == Renderer::Software {
            continue;
        }
        if cfg!(target_os = "linux")
            && !matches!(
                backend,
                Backend::Vulkan | Backend::OpenGl | Backend::OpenGlEs
            )
        {
            continue;
        }
        if cfg!(target_os = "macos") && !matches!(backend, Backend::Metal | Backend::OpenGl) {
            continue;
        }
        if gpu && renderer != Renderer::Sdl
            || sdl3::render::drivers().any(|d| d == backend.as_str())
        {
            values.push(backend);
        }
    }
    values
}
fn scale_slider(ui: &mut egui::Ui, value: &mut u32, cap: VideoCapabilities) {
    ui.spacing_mut().slider_width = (ui.available_width() - 75.0).max(100.0);
    let r = ui
        .horizontal(|ui| {
            let r = ui.add(
                egui::Slider::new(value, cap.min_scale.max(1)..=cap.max_scale)
                    .step_by(1.0)
                    .integer()
                    .handle_shape(egui::style::HandleShape::Circle)
                    .show_value(false),
            );
            ui.add(
                egui::DragValue::new(value)
                    .range(cap.min_scale.max(1)..=cap.max_scale)
                    .prefix("×"),
            );
            r
        })
        .inner;
    let inset = r.rect.height() / 2.5;
    let top = r.rect.bottom() + 2.0;
    ui.allocate_space(Vec2::new(1.0, 27.0));
    for n in cap.min_scale.max(1)..=cap.max_scale {
        let x = egui::lerp(
            r.rect.left() + inset..=r.rect.right() - inset,
            (n - cap.min_scale.max(1)) as f32
                / (cap.max_scale - cap.min_scale.max(1)).max(1) as f32,
        );
        let length = if n == 1 || n % 4 == 0 { 7.0 } else { 4.0 };
        ui.painter().line_segment(
            [Pos2::new(x, top), Pos2::new(x, top + length)],
            egui::Stroke::new(1.0_f32, ui.visuals().text_color()),
        );
        ui.painter().text(
            Pos2::new(x, top + 10.0),
            egui::Align2::CENTER_TOP,
            n.to_string(),
            egui::FontId::proportional(11.0),
            ui.visuals().text_color(),
        );
    }
}
