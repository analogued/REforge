// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Compact Display, CRT and Audio pages. Editing is separate from activation and persistence.
use crate::settings::{SettingsMenu, VideoCapabilities};
use egui::{Color32, Pos2, Vec2};
use reforge_settings::{
    Backend, FrameLimit, Presentation, Renderer, RuntimeSettings, SECTION_KEYS, SHADER_PARAMETERS,
    SoundRenderer, section_index,
};
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
                    for (page, label) in
                        ["Display", "CRT & Colour", "Audio"].into_iter().enumerate()
                    {
                        if ui
                            .add_sized(
                                [132.0, 32.0],
                                egui::Button::new(label).selected(menu.page == page),
                            )
                            .clicked()
                        {
                            menu.page = page;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width((width - 164.0).max(180.0));
                    ui.heading(["Display", "CRT & Colour", "Audio"][menu.page]);
                    if menu.scope == 1 {
                        let mut inherit = !menu.game_editing[menu.page];
                        if ui
                            .checkbox(&mut inherit, "Use global defaults for this section")
                            .changed()
                        {
                            menu.game_editing[menu.page] = !inherit;
                            if inherit {
                                draft.reset_section(true, menu.page);
                                menu.game_touched[menu.page] = true;
                            }
                        }
                    } else {
                        ui.weak("Shared defaults · individual games can override these values");
                    }
                    ui.add_space(8.0);
                    let base = draft.global_values();
                    let original = if menu.scope == 0 {
                        base.clone()
                    } else {
                        draft.game_values()
                    };
                    let mut value = original.clone();
                    let previous = if menu.scope == 0 {
                        saved.global_values()
                    } else {
                        saved.game_values()
                    };
                    let enabled = menu.scope == 0 || menu.game_editing[menu.page];
                    egui::ScrollArea::vertical()
                        .id_salt(("settings_content", menu.page))
                        .max_height((height - 215.0).max(80.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                if menu.page == 0 {
                                    section(ui, "Window & scaling");
                                    row(
                                        ui,
                                        "Window mode",
                                        "window_mode",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            ui.radio_value(
                                                &mut value.fullscreen,
                                                false,
                                                "Windowed",
                                            );
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
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
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
                                            if value.scale.is_none() {
                                                ui.label("Largest fit");
                                            }
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
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
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
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            ui.add_enabled_ui(
                                                value.renderer != Renderer::Software,
                                                |ui| {
                                                    backend_selector(ui, &mut value);
                                                },
                                            );
                                        },
                                    );
                                    row(
                                        ui,
                                        "VSync",
                                        "vsync",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            ui.checkbox(
                                                &mut value.vsync,
                                                "Synchronise with the display",
                                            );
                                        },
                                    );
                                    row(
                                        ui,
                                        "Frame limit",
                                        "frame_limit",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            egui::ComboBox::from_id_salt("frame_limit")
                                                .width(220.0)
                                                .selected_text(value.frame_limit.label())
                                                .show_ui(ui, |ui| {
                                                    for limit in FrameLimit::ALL {
                                                        ui.selectable_value(
                                                            &mut value.frame_limit,
                                                            limit,
                                                            limit.label(),
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                    ui.weak(
                                        "Presentation ceiling · simulation timing is unchanged",
                                    );
                                } else if menu.page == 1 {
                                    crt_controls(
                                        ui,
                                        &mut value,
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                    );
                                } else {
                                    audio_controls(
                                        ui,
                                        &mut value,
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        &menu.audio_devices,
                                        &menu.audio_renderers,
                                    );
                                }
                            });
                        });
                    let keys = SECTION_KEYS[menu.page];
                    for &key in keys {
                        if original.get(key) != value.get(key) && !cli.contains_key(key) {
                            draft.edit(menu.scope == 1, key, value.get(key));
                            if menu.scope == 1 {
                                menu.game_touched[menu.page] = true;
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
                        draft.reset_section(false, menu.page);
                    } else {
                        draft.reset_section(true, menu.page);
                        menu.game_editing[menu.page] = false;
                        menu.game_touched[menu.page] = true;
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
                    let baseline = draft.global_values();
                    draft.game.retain(|key, v| {
                        let page = section_index(key);
                        !menu.game_touched[page] || *v != baseline.get(key)
                    });
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
fn crt_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    base: RuntimeSettings,
    previous: RuntimeSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
) {
    section(ui, "Presentation");
    row(
        ui,
        "Treatment",
        "presentation",
        value.clone(),
        base.clone(),
        previous.clone(),
        scope,
        cli,
        |ui| {
            egui::ComboBox::from_id_salt("presentation")
                .width(220.0)
                .selected_text(if value.presentation == Presentation::Pixels {
                    "Sharp pixels"
                } else {
                    "Hyllian CRT"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut value.presentation,
                        Presentation::Pixels,
                        "Sharp pixels",
                    );
                    ui.selectable_value(
                        &mut value.presentation,
                        Presentation::Hyllian,
                        "Hyllian CRT",
                    );
                });
        },
    );
    // Treatment-specific controls share one advanced section. The CLI layer
    // determines which treatment is active, without entering the saved draft.
    let treatment = cli
        .get("presentation")
        .and_then(|v| v.parse().ok())
        .unwrap_or(value.presentation);
    ui.add_enabled_ui(treatment != Presentation::Pixels, |ui| {
        egui::CollapsingHeader::new("Fine adjustments")
            .id_salt("treatment_fine_adjustments")
            .default_open(false)
            .show(ui, |ui| match treatment {
                Presentation::Hyllian => {
                    hyllian_controls(ui, value, base.clone(), previous.clone(), scope, cli)
                }
                Presentation::Pixels => {}
            });
    });
    ui.weak("Hyllian requires the GPU renderer; fallbacks use sharp pixels.");
}

fn hyllian_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    base: RuntimeSettings,
    previous: RuntimeSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
) {
    row(
        ui,
        "Horizontal filter",
        "horizontal_filter",
        value.clone(),
        base.clone(),
        previous.clone(),
        scope,
        cli,
        |ui| {
            egui::ComboBox::from_id_salt("horizontal_filter")
                .width(220.0)
                .selected_text(if value.hyllian.catmull_rom {
                    "Catmull–Rom"
                } else {
                    "Hermite"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut value.hyllian.catmull_rom, false, "Hermite");
                    ui.selectable_value(&mut value.hyllian.catmull_rom, true, "Catmull–Rom");
                });
        },
    );
    for (i, param) in SHADER_PARAMETERS.into_iter().enumerate() {
        row(
            ui,
            param.label,
            param.key,
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            |ui| {
                ui.spacing_mut().slider_width = 150.0;
                let mut n = f32::from(value.hyllian.values[i]) / 100.0;
                let mut low = param.min;
                let mut high = param.max;
                if i == 1 {
                    high = high.min(value.hyllian.values[2]);
                }
                if i == 2 {
                    low = low.max(value.hyllian.values[1]);
                }
                if ui
                    .add(
                        egui::Slider::new(&mut n, f32::from(low) / 100.0..=f32::from(high) / 100.0)
                            .step_by(0.01)
                            .fixed_decimals(2),
                    )
                    .on_hover_text(param.help)
                    .changed()
                {
                    value.hyllian.values[i] = (n * 100.0).round() as u16;
                }
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn audio_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    base: RuntimeSettings,
    previous: RuntimeSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
    devices: &[String],
    renderers: &[SoundRenderer],
) {
    let available = !renderers.is_empty();
    if !available {
        ui.weak("Audio playback is unavailable in this run. Shared defaults can still be saved.");
    }
    ui.add_enabled_ui(scope == 0 || available, |ui| {
        row(ui, "Sound", "sound_enabled", value.clone(), base.clone(), previous.clone(), scope, cli, |ui| { ui.checkbox(&mut value.audio.enabled, "Enable sound"); });
        ui.add_enabled_ui(value.audio.enabled, |ui| {
            section(ui, "Levels");
            row(ui, "Master volume", "master_volume", value.clone(), base.clone(), previous.clone(), scope, cli, |ui| {
                ui.spacing_mut().slider_width = 150.0;
                ui.add(egui::Slider::new(&mut value.audio.volume, 0..=100).suffix("%"));
            });
            section(ui, "Playback");
            row(ui, "Sound renderer", "audio_renderer", value.clone(), base.clone(), previous.clone(), scope, cli, |ui| {
                ui.add_enabled_ui(scope == 1 && available, |ui| {
                    egui::ComboBox::from_id_salt("audio_renderer").width(220.0).selected_text(sound_renderer_label(value.audio.renderer)).show_ui(ui, |ui| {
                        for renderer in std::iter::once(&SoundRenderer::GameDefault).chain(renderers) {
                            ui.selectable_value(&mut value.audio.renderer, *renderer, sound_renderer_label(*renderer));
                        }
                    });
                });
            });
            if scope == 0 { ui.weak("Sound renderer is selected per game; Game default follows its original hardware."); }
            row(ui, "Output device", "output_device", value.clone(), base.clone(), previous.clone(), scope, cli, |ui| {
                egui::ComboBox::from_id_salt(("output_device", devices)).width(220.0).truncate()
                    .selected_text(value.audio.device.as_deref().unwrap_or("System default")).show_ui(ui, |ui| {
                        ui.selectable_value(&mut value.audio.device, None, "System default");
                        if let Some(name) = value.audio.device.clone().filter(|n| !devices.contains(n)) {
                            ui.add_enabled(false, egui::Label::new(format!("Unavailable: {name}")));
                        }
                        for name in devices { ui.selectable_value(&mut value.audio.device, Some(name.clone()), name); }
                    });
            });
            row(ui, "Focus", "mute_unfocused", value.clone(), base.clone(), previous.clone(), scope, cli, |ui| { ui.checkbox(&mut value.audio.mute_unfocused, "Mute when unfocused"); });
        });
    });
}
fn sound_renderer_label(renderer: SoundRenderer) -> &'static str {
    match renderer {
        SoundRenderer::GameDefault => "Game default",
        SoundRenderer::Speaker => "PC speaker",
        SoundRenderer::EnhancedPcm => "Enhanced PCM",
    }
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
    value: RuntimeSettings,
    global: RuntimeSettings,
    saved: RuntimeSettings,
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
fn backend_selector(ui: &mut egui::Ui, value: &mut RuntimeSettings) -> egui::Response {
    let choices = backends(value.renderer);
    // egui remembers the popup's size. A short GPU list must not constrain
    // the longer Automatic/SDL list after switching renderers.
    egui::ComboBox::from_id_salt(("backend", value.renderer.as_str()))
        .width(220.0)
        .selected_text(backend_label(value.backend))
        .show_ui(ui, |ui| {
            for backend in choices {
                ui.selectable_value(&mut value.backend, backend, backend_label(backend));
            }
        })
        .response
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
        if renderer == Renderer::Sdl && backend == Backend::Vulkan {
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_popup_grows_when_renderer_adds_choices() {
        let context = egui::Context::default();
        style(&context);
        let mut value = RuntimeSettings::default();
        let mut button = egui::Rect::NOTHING;
        let mut time = 0.0;
        for renderer in [Renderer::Gpu, Renderer::Auto, Renderer::Sdl, Renderer::Auto] {
            value.renderer = renderer;
            context.memory_mut(|memory| memory.close_popup());
            // Settle the closed control before opening it with an actual click.
            for step in 0..6 {
                time += 0.02;
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(900.0, 700.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                };
                if step == 2 || step == 3 {
                    input.events = vec![
                        egui::Event::PointerMoved(button.center()),
                        egui::Event::PointerButton {
                            pos: button.center(),
                            button: egui::PointerButton::Primary,
                            pressed: step == 2,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ];
                }
                let output = context.run(input, |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        row(
                            ui,
                            "Backend",
                            "backend",
                            value.clone(),
                            value.clone(),
                            value.clone(),
                            0,
                            &Default::default(),
                            |ui| {
                                button = backend_selector(ui, &mut value).rect;
                            },
                        );
                    });
                });
                if step == 5 {
                    for backend in backends(renderer) {
                        let label = backend_label(backend);
                        let mut found = false;
                        for shape in &output.shapes {
                            if let egui::Shape::Text(text) = &shape.shape {
                                if text.galley.text() == label && text.pos.y > button.bottom() {
                                    found = true;
                                    let bounds =
                                        egui::Rect::from_min_size(text.pos, text.galley.size());
                                    assert!(
                                        shape.clip_rect.contains_rect(bounds),
                                        "{renderer:?}: {label} at {bounds:?} clipped by {:?}",
                                        shape.clip_rect
                                    );
                                }
                            }
                        }
                        assert!(found, "{renderer:?}: {label} must be drawn in the popup");
                    }
                }
            }
        }
    }
}
