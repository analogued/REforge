// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Compact settings pages. Editing is separate from activation and persistence.
//! Register interactive responses with control/group_control/value_control (or Selector/collapsing)
//! so keyboard/controller navigation targets settings rather than scroll geometry.
use crate::settings::{SettingsMenu, VideoCapabilities};
use crate::settings_navigation::{
    Group, Kind, collapsing, control, group_control, primary_footer_control, value_control,
};
use crate::settings_selector::{Choice, Selector};
use egui::{Color32, Pos2, Vec2};
use reforge_settings::{
    AspectRatio, Backend, CONTROL_MODE, ControlMode, DIGITIZED_SAMPLES, FrameLimit, Presentation,
    Renderer, RuntimeSettings, SECTION_KEYS, SHADER_PARAMETERS, STICK_DEAD_ZONE, Sampling,
    ScalingPolicy, SoundCategory, SoundRenderer, section_index,
};
const ACCENT: Color32 = Color32::from_rgb(100, 200, 226);
const PENDING: Color32 = Color32::from_rgb(245, 185, 90);
const CLI_LOCK: Color32 = Color32::from_rgb(182, 169, 214);
const CLI_BANNER: Color32 = Color32::from_rgb(35, 58, 72);
const CLI_NOTICE: Color32 = Color32::from_rgb(190, 214, 228);
const PAGES: [&str; 4] = ["Display", "CRT & Colour", "Audio", "Controls"];
pub(super) fn style(context: &egui::Context) {
    context.style_mut(|s| {
        s.animation_time = 0.0;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.spacing.combo_width = 220.0;
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
    crate::settings_navigation::begin_frame(context);
    let session = menu.creator.session.borrow();
    let saved = session.saved.clone();
    let cli = session.cli.clone();
    let game = session.game.clone();
    let warning = session.warning.clone();
    drop(session);
    let draft = menu.draft.as_mut().expect("open draft");
    let windowed_scale = menu.creator.windowed_scale.get();
    let geometry = menu.creator.geometry.get();
    let width = (screen.width() - 36.0).clamp(300.0, 860.0);
    let height = (screen.height() - 48.0).clamp(260.0, 650.0);
    let body_height = (height - 120.0).clamp(160.0, 450.0);
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
                let first = group_control(
                    ui.selectable_value(&mut menu.scope, 0, "Global defaults"),
                    Group::Scope,
                );
                let game_scope = group_control(
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
                    ),
                    Group::Scope,
                );
                if menu.navigation.focus_first && first.enabled() && game_scope.enabled() {
                    if menu.scope == 0 {
                        first.request_focus();
                    } else {
                        game_scope.request_focus();
                    }
                    menu.navigation.focus_first = false;
                }
            });
            let controls_visible = menu.scope == 1 && menu.alleycat_options;
            ui.separator();
            let body = ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(132.0);
                    ui.add_space(6.0);
                    for (page, label) in PAGES.into_iter().enumerate() {
                        if group_control(
                            ui.add_sized(
                                [132.0, 32.0],
                                egui::Button::new(label).selected(menu.page == page),
                            ),
                            Group::Sidebar,
                        )
                        .clicked()
                        {
                            menu.page = page;
                        }
                    }
                });
                // Reserve the divider's width without expanding this row to
                // all available height. Paint it after measuring both columns.
                let (_, divider) = ui.allocate_space(Vec2::new(6.0, 0.0));
                ui.vertical(|ui| {
                    ui.set_width((width - 164.0).max(180.0));
                    // Every page uses the same body. Headings and notices take
                    // space from its scroll viewport, never from the footer.
                    ui.set_height(body_height);
                    ui.heading(PAGES[menu.page]);
                    if section_has_cli_locks(&cli, menu.page, menu.scope, menu.mach3_options) {
                        cli_notice(ui);
                    }
                    if menu.scope == 1 {
                        let mut inherit = !menu.game_editing[menu.page];
                        if control(ui.checkbox(
                            &mut inherit,
                            if menu.page == 3 || (menu.page == 2 && menu.mach3_options) {
                                "Use defaults for this section"
                            } else {
                                "Use global defaults for this section"
                            },
                        ))
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
                    // Locked widgets show this run's value. The draft below still
                    // compares/updates only unlocked keys, so CLI values never save.
                    for (key, active) in &cli {
                        let _ = value.set(key, active);
                    }
                    let previous = if menu.scope == 0 {
                        saved.global_values()
                    } else {
                        saved.game_values()
                    };
                    let enabled = menu.scope == 0 || menu.game_editing[menu.page];
                    egui::ScrollArea::vertical()
                        .animated(false)
                        .id_salt(("settings_content", menu.page))
                        .max_height(ui.available_height().max(0.0))
                        .min_scrolled_width(0.0)
                        .min_scrolled_height(0.0)
                        .auto_shrink([false, false])
                        .show_viewport(ui, |ui, viewport| {
                            ui.set_width(viewport.width());
                            let first = crate::settings_navigation::scroll_start(ui.ctx());
                            ui.add_enabled_ui(enabled || menu.page == 2, |ui| {
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
                                            control(ui.radio_value(
                                                &mut value.fullscreen,
                                                false,
                                                "Windowed",
                                            ));
                                            ui.add_enabled_ui(
                                                menu.capabilities.allow_fullscreen,
                                                |ui| {
                                                    control(ui.radio_value(
                                                        &mut value.fullscreen,
                                                        true,
                                                        "Fullscreen",
                                                    ));
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
                                            if control(ui.add_enabled(
                                                menu.capabilities.allow_auto || auto,
                                                egui::Checkbox::new(&mut auto, "Auto"),
                                            ))
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
                                    row(
                                        ui,
                                        "Scaling policy",
                                        "scaling_policy",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            Selector::new(
                                                "scaling_policy",
                                                [
                                                    Choice::new(
                                                        ScalingPolicy::Integer,
                                                        "Integer fit",
                                                    ),
                                                    Choice::new(
                                                        ScalingPolicy::Fit,
                                                        "Fit to window (fractional)",
                                                    ),
                                                    Choice::new(
                                                        ScalingPolicy::Stretch,
                                                        "Fill window (stretch)",
                                                    ),
                                                ],
                                            )
                                            .show(ui, &mut value.scaling_policy);
                                        },
                                    );
                                    row(
                                        ui,
                                        "Sampling",
                                        "sampling",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            Selector::new(
                                                "sampling",
                                                [
                                                    Choice::new(
                                                        Sampling::Nearest,
                                                        "Nearest · sharp pixels",
                                                    ),
                                                    Choice::new(
                                                        Sampling::Linear,
                                                        "Linear · smooth pixels",
                                                    ),
                                                ],
                                            )
                                            .show(ui, &mut value.sampling);
                                        },
                                    );
                                    row(
                                        ui,
                                        "Aspect ratio",
                                        "aspect_ratio",
                                        value.clone(),
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        |ui| {
                                            Selector::new(
                                                "aspect_ratio",
                                                [AspectRatio::Original, AspectRatio::Square].map(
                                                    |aspect| {
                                                        Choice::new(
                                                            aspect,
                                                            geometry.label(aspect, menu.scope == 1),
                                                        )
                                                    },
                                                ),
                                            )
                                            .show(ui, &mut value.aspect_ratio);
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
                                            Selector::new(
                                                "renderer",
                                                [
                                                    Renderer::Auto,
                                                    Renderer::Gpu,
                                                    Renderer::Sdl,
                                                    Renderer::Software,
                                                ]
                                                .map(|r| Choice::new(r, renderer_label(r))),
                                            )
                                            .show(ui, &mut value.renderer);
                                        },
                                    );
                                    if value.renderer != original.renderer
                                        && !cli.contains_key("renderer")
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
                                            control(ui.checkbox(
                                                &mut value.vsync,
                                                "Synchronise with the display",
                                            ));
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
                                            Selector::new(
                                                "frame_limit",
                                                FrameLimit::ALL
                                                    .map(|limit| Choice::new(limit, limit.label())),
                                            )
                                            .show(ui, &mut value.frame_limit);
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
                                } else if menu.page == 2 {
                                    audio_controls(
                                        ui,
                                        &mut value,
                                        base.clone(),
                                        previous.clone(),
                                        menu.scope,
                                        &cli,
                                        &menu.audio_devices,
                                        &menu.audio_renderers,
                                        &menu.audio_categories,
                                        enabled,
                                        menu.mach3_options,
                                    );
                                } else if menu.page == 3 {
                                    controls_controls(
                                        ui,
                                        &mut value,
                                        base.clone(),
                                        previous.clone(),
                                        &cli,
                                        menu.scope,
                                        controls_visible,
                                    );
                                }
                            });
                            crate::settings_navigation::scroll_focus(ui, first);
                        });
                    if menu.page == 2
                        && menu.scope == 1
                        && menu.mach3_options
                        && original.mach3 != value.mach3
                        && !cli.contains_key(DIGITIZED_SAMPLES)
                    {
                        draft.edit(true, DIGITIZED_SAMPLES, value.get(DIGITIZED_SAMPLES));
                        menu.game_touched[menu.page] = true;
                    }
                    if menu.page == 3
                        && controls_visible
                        && original.alleycat != value.alleycat
                        && !cli.contains_key(CONTROL_MODE)
                    {
                        draft.edit(true, CONTROL_MODE, value.get(CONTROL_MODE));
                        menu.game_touched[menu.page] = true;
                    }
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
                divider.center().x
            });
            ui.painter().vline(
                body.inner,
                body.response.rect.y_range(),
                ui.visuals().widgets.noninteractive.bg_stroke,
            );
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                // The footer shares a vertical navigation column with the
                // settings, even when its buttons sit left of the value fields.
                let bounds = ui.max_rect();
                crate::settings_navigation::row_controls(ui, bounds, |ui| {
                    let reset_label = if menu.scope == 0 {
                        "Reset section"
                    } else if menu.page == 3 || (menu.page == 2 && menu.mach3_options) {
                        "Restore defaults"
                    } else {
                        "Restore global defaults"
                    };
                    // Shorter section-specific labels must not shift Apply/Cancel.
                    let reset_width = ui
                        .painter()
                        .layout_no_wrap(
                            if menu.scope == 0 {
                                "Reset section"
                            } else {
                                "Restore global defaults"
                            }
                            .into(),
                            egui::TextStyle::Button.resolve(ui.style()),
                            ui.visuals().text_color(),
                        )
                        .size()
                        .x
                        + ui.spacing().button_padding.x * 2.0;
                    if group_control(
                        ui.add(
                            egui::Button::new(reset_label)
                                .min_size(Vec2::new(reset_width, ui.spacing().interact_size.y)),
                        ),
                        Group::Footer,
                    )
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
                    let apply = primary_footer_control(ui.add_enabled(
                        *draft != saved || menu.error.is_some(),
                        egui::Button::new("Apply"),
                    ));
                    #[cfg(test)]
                    {
                        menu.apply_rect = Some(apply.rect);
                    }
                    if apply.clicked() {
                        draft.normalize_treatments(&saved);
                        let baseline = draft.global_values();
                        draft.game.retain(|key, v| {
                            let page = section_index(key);
                            !menu.game_touched[page] || *v != baseline.get(key)
                        });
                        menu.pending = Some(draft.clone());
                    }
                    let cancel = group_control(ui.button("Cancel"), Group::Footer);
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
fn controls_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    base: RuntimeSettings,
    previous: RuntimeSettings,
    cli: &reforge_settings::Patch,
    scope: usize,
    alleycat: bool,
) {
    section(ui, "Controller");
    row(
        ui,
        "Stick dead zone",
        STICK_DEAD_ZONE,
        value.clone(),
        base.clone(),
        previous.clone(),
        scope,
        cli,
        |ui| {
            value_control(
                ui.add(egui::Slider::new(&mut value.stick_dead_zone, 5..=90).suffix("%")),
                Kind::Slider,
            );
        },
    );
    if !alleycat {
        return;
    }
    section(ui, "Gameplay");
    row(
        ui,
        "Input response",
        CONTROL_MODE,
        value.clone(),
        base,
        previous,
        1,
        cli,
        |ui| {
            Selector::new(CONTROL_MODE, ControlMode::ALL.map(|mode| Choice::new(mode, mode.label()))).show(ui, &mut value.alleycat.control_mode).on_hover_text("Responsive reacts sooner to direction and action changes. Original preserves the game's original input response.");
        },
    );
}

fn crt_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    base: RuntimeSettings,
    previous: RuntimeSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
) {
    // Use this scope's pending Display choices, with temporary CLI overrides,
    // rather than the renderer that remains active until Apply.
    let mut display = value.clone();
    display.renderer = cli
        .get("renderer")
        .and_then(|v| v.parse().ok())
        .unwrap_or(value.renderer);
    display.backend = cli
        .get("backend")
        .and_then(|v| v.parse().ok())
        .unwrap_or(value.backend);
    let hyllian_available = display.supports_hyllian();
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
            Selector::new(
                "presentation",
                [
                    Choice::new(Presentation::Pixels, "Sharp pixels"),
                    Choice::new(Presentation::Hyllian, "Hyllian CRT").enabled(
                        hyllian_available,
                        "Select Automatic or GPU with a compatible backend in Display.",
                    ),
                ],
            )
            .selected_text(if value.presentation == Presentation::Pixels {
                "Sharp pixels"
            } else if !hyllian_available {
                "Sharp pixels (on Apply)"
            } else {
                "Hyllian CRT"
            })
            .show(ui, &mut value.presentation);
        },
    );
    // Treatment-specific controls share one advanced section. The CLI layer
    // determines which treatment is active, without entering the saved draft.
    let treatment = cli
        .get("presentation")
        .and_then(|v| v.parse().ok())
        .unwrap_or(value.presentation);
    ui.add_enabled_ui(
        treatment != Presentation::Pixels && hyllian_available,
        |ui| {
            collapsing(
                egui::CollapsingHeader::new("Fine adjustments")
                    .id_salt("treatment_fine_adjustments")
                    .default_open(false)
                    .show(ui, |ui| match treatment {
                        Presentation::Hyllian => {
                            hyllian_controls(ui, value, base.clone(), previous.clone(), scope, cli)
                        }
                        Presentation::Pixels => {}
                    }),
            );
        },
    );
    ui.weak(if hyllian_available {
        "Hyllian requires the GPU renderer; fallbacks use sharp pixels."
    } else {
        "Hyllian is unavailable with this renderer/backend; sharp pixels will be used."
    });
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
            Selector::new(
                "horizontal_filter",
                [
                    Choice::new(false, "Hermite"),
                    Choice::new(true, "Catmull–Rom"),
                ],
            )
            .show(ui, &mut value.hyllian.catmull_rom);
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
                ui.spacing_mut().slider_width = 150.0_f32.min(slider_track_width(ui, 56.0));
                let mut n = f32::from(value.hyllian.values[i]) / 100.0;
                let mut low = param.min;
                let mut high = param.max;
                if i == 1 {
                    high = high.min(value.hyllian.values[2]);
                }
                if i == 2 {
                    low = low.max(value.hyllian.values[1]);
                }
                if value_control(
                    ui.add(
                        egui::Slider::new(&mut n, f32::from(low) / 100.0..=f32::from(high) / 100.0)
                            .step_by(0.01)
                            .fixed_decimals(2),
                    ),
                    Kind::Slider,
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
    categories: &[SoundCategory],
    section_editable: bool,
    mach3_options: bool,
) {
    let available = !renderers.is_empty();
    if !available {
        ui.weak("Audio playback is unavailable in this run. Shared defaults can still be saved.");
    }
    let shared_enabled = section_editable && (scope == 0 || available);
    ui.add_enabled_ui(shared_enabled, |ui| {
        row(
            ui,
            "",
            "sound_enabled",
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            |ui| {
                control(ui.checkbox(&mut value.audio.enabled, "Enable sound"));
            },
        );
    });
    let output_enabled = shared_enabled && value.audio.enabled;
    ui.add_enabled_ui(output_enabled, |ui| {
        section(ui, "Levels");
        let volume_width = 220.0 + ui.spacing().item_spacing.x + 56.0;
        row_with_width(
            ui,
            "Master volume",
            "master_volume",
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            volume_width,
            |ui| {
                ui.spacing_mut().slider_width = slider_track_width(ui, 56.0);
                value_control(
                    ui.add(egui::Slider::new(&mut value.audio.volume, 0..=100).show_value(false)),
                    Kind::Slider,
                );
                ui.spacing_mut().interact_size.x = 56.0;
                value_control(
                    ui.add(
                        egui::DragValue::new(&mut value.audio.volume)
                            .range(0..=100)
                            .suffix("%"),
                    ),
                    Kind::Number,
                );
            },
        );
        for category in [
            SoundCategory::Music,
            SoundCategory::Effects,
            SoundCategory::Speech,
        ] {
            row_with_width(
                ui,
                category.label(),
                category.key(),
                value.clone(),
                base.clone(),
                previous.clone(),
                scope,
                cli,
                volume_width,
                |ui| {
                    ui.add_enabled_ui(scope == 0 || categories.contains(&category), |ui| {
                        ui.spacing_mut().slider_width = slider_track_width(ui, 56.0);
                        value_control(
                            ui.add(
                                egui::Slider::new(
                                    &mut value.audio.categories.0[category.index()],
                                    0..=100,
                                )
                                .show_value(false),
                            ),
                            Kind::Slider,
                        );
                        ui.spacing_mut().interact_size.x = 56.0;
                        value_control(
                            ui.add(
                                egui::DragValue::new(
                                    &mut value.audio.categories.0[category.index()],
                                )
                                .range(0..=100)
                                .suffix("%"),
                            ),
                            Kind::Number,
                        );
                    })
                    .response
                    .on_disabled_hover_text("This game does not provide this audio category.");
                },
            );
        }
        section(ui, "Playback");
        row(
            ui,
            "Sound renderer",
            "audio_renderer",
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            |ui| {
                ui.add_enabled_ui(scope == 1 && available, |ui| {
                    Selector::new(
                        "audio_renderer",
                        std::iter::once(&SoundRenderer::GameDefault)
                            .chain(renderers)
                            .map(|r| Choice::new(*r, sound_renderer_label(*r))),
                    )
                    .show(ui, &mut value.audio.renderer);
                });
            },
        );
        if scope == 0 {
            ui.weak(
                "Sound renderer is selected per game; Game default follows its original hardware.",
            );
        }
    });
    // The native option shares Audio's override/reset control, but its default
    // is built in rather than inherited from a global digitized-samples value.
    if scope == 1 && mach3_options {
        ui.add_enabled_ui(section_editable, |ui| {
            row(
                ui,
                "Digitized samples",
                DIGITIZED_SAMPLES,
                value.clone(),
                base.clone(),
                previous.clone(),
                scope,
                cli,
                |ui| {
                    Selector::new(
                        DIGITIZED_SAMPLES,
                        [Choice::new(true, "On"), Choice::new(false, "Off")],
                    )
                    .show(ui, &mut value.mach3.digitized_samples)
                    .on_hover_text("Applies on next launch.");
                },
            );
        });
    }
    ui.add_enabled_ui(output_enabled, |ui| {
        row(
            ui,
            "Output device",
            "output_device",
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            |ui| {
                Selector::new(
                    ("output_device", devices),
                    std::iter::once(Choice::new(None, "System default"))
                        .chain(
                            value
                                .audio
                                .device
                                .clone()
                                .filter(|name| !devices.contains(name))
                                .map(|name| {
                                    Choice::new(Some(name.clone()), format!("Unavailable: {name}"))
                                        .enabled(false, "This output device is unavailable.")
                                }),
                        )
                        .chain(
                            devices
                                .iter()
                                .map(|name| Choice::new(Some(name.clone()), name)),
                        ),
                )
                .truncate()
                .selected_text(value.audio.device.as_deref().unwrap_or("System default"))
                .show(ui, &mut value.audio.device);
            },
        );
        row(
            ui,
            "",
            "mute_unfocused",
            value.clone(),
            base.clone(),
            previous.clone(),
            scope,
            cli,
            |ui| {
                control(ui.checkbox(&mut value.audio.mute_unfocused, "Mute when unfocused"));
            },
        );
    });
}
fn sound_renderer_label(renderer: SoundRenderer) -> &'static str {
    match renderer {
        SoundRenderer::GameDefault => "Game default",
        SoundRenderer::Speaker => "PC speaker",
        SoundRenderer::EnhancedPcm => "Enhanced PCM",
    }
}

fn section_has_cli_locks(
    cli: &reforge_settings::Patch,
    page: usize,
    scope: usize,
    mach3_options: bool,
) -> bool {
    SECTION_KEYS[page].iter().any(|key| cli.contains_key(*key))
        || (page == 2 && scope == 1 && mach3_options && cli.contains_key(DIGITIZED_SAMPLES))
}

fn paint_lock(painter: &egui::Painter, center: Pos2, colour: Color32) {
    let shackle = [
        center + Vec2::new(-3.5, -1.0),
        center + Vec2::new(-3.5, -5.0),
        center + Vec2::new(-2.0, -7.0),
        center + Vec2::new(2.0, -7.0),
        center + Vec2::new(3.5, -5.0),
        center + Vec2::new(3.5, -1.0),
    ];
    painter.add(egui::Shape::line(
        shackle.to_vec(),
        egui::Stroke::new(1.4_f32, colour),
    ));
    painter.rect_filled(
        egui::Rect::from_center_size(center + Vec2::new(0.0, 3.0), Vec2::new(12.0, 9.0)),
        2.0,
        colour,
    );
    let keyhole = Color32::from_gray(40);
    painter.circle_filled(center + Vec2::new(0.0, 2.0), 1.2, keyhole);
    painter.line_segment(
        [center + Vec2::new(0.0, 2.0), center + Vec2::new(0.0, 5.0)],
        egui::Stroke::new(1.2_f32, keyhole),
    );
}

fn cli_notice(ui: &mut egui::Ui) -> egui::Response {
    egui::Frame::NONE
        .fill(CLI_BANNER)
        .corner_radius(3.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 18.0), egui::Sense::hover());
                paint_lock(ui.painter(), rect.center(), CLI_NOTICE);
                control(
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Some settings are locked by command-line arguments.",
                            )
                            .size(13.0)
                            .color(CLI_NOTICE),
                        )
                        .wrap(),
                    ),
                );
            });
        })
        .response
}

fn cli_tooltip(key: &str, active: &str) -> String {
    let argument = match active {
        "true" => "on",
        "false" => "off",
        _ => active,
    };
    format!(
        "Locked for this run by --{} {argument}.\nRemove the command-line argument to edit this setting.\nSaved preferences are unchanged.",
        key.replace('_', "-"),
    )
}

fn dependency_lock(key: &str, value: &RuntimeSettings) -> Option<&'static str> {
    match key {
        "aspect_ratio" if value.scaling_policy == ScalingPolicy::Stretch => Some(
            "Aspect ratio is ignored by Fill window (stretch). Choose another scaling policy to edit it.",
        ),
        "sampling" if value.presentation == Presentation::Hyllian => Some(
            "Hyllian CRT uses its own reconstruction filter. Select Sharp pixels in CRT & Colour to edit Sampling.",
        ),
        _ => None,
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
    draw_control: impl FnOnce(&mut egui::Ui),
) {
    row_with_width(
        ui,
        label,
        key,
        value,
        global,
        saved,
        scope,
        cli,
        220.0,
        draw_control,
    );
}

#[allow(clippy::too_many_arguments)]
fn row_with_width(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    value: RuntimeSettings,
    global: RuntimeSettings,
    saved: RuntimeSettings,
    scope: usize,
    cli: &reforge_settings::Patch,
    control_width: f32,
    draw_control: impl FnOnce(&mut egui::Ui),
) {
    let lock_tooltip = cli
        .get(key)
        .map(|active| cli_tooltip(key, active))
        .or_else(|| dependency_lock(key, &value).map(str::to_owned));
    let locked = lock_tooltip.is_some();
    let different = scope == 1 && value.get(key) != global.get(key);
    let dirty = !cli.contains_key(key) && value.get(key) != saved.get(key);
    let available = visible_width(ui);
    let spacing = ui.spacing().item_spacing.x;
    let markers = 20.0 + 12.0 + spacing * 2.0;
    let stacked = !label.is_empty() && available < 145.0 + spacing + control_width + markers;
    let control_width = if stacked {
        (available - markers).max(1.0)
    } else {
        control_width.min((available - markers).max(1.0))
    };
    let colour = if locked {
        Color32::from_gray(100)
    } else if different {
        ACCENT
    } else {
        ui.visuals().text_color()
    };
    let mut tooltip = format!(
        "{}: {}\nSaved: {}",
        if key == DIGITIZED_SAMPLES {
            "Default"
        } else {
            "Global"
        },
        global.get(key),
        saved.get(key)
    );
    if let Some(reason) = &lock_tooltip {
        tooltip = format!("{reason}\n\n{tooltip}");
    }
    let draw_label = |ui: &mut egui::Ui| {
        let label = egui::Label::new(egui::RichText::new(label).color(colour));
        let response = if stacked {
            ui.add(label.wrap())
        } else {
            ui.add_sized([145.0, 22.0], label)
        };
        control(response)
            .on_hover_text(&tooltip)
            .on_disabled_hover_text(&tooltip);
    };
    // Explicit row/value identities survive a switch between side-by-side and
    // stacked labels, preserving focus and an open selector during a resize.
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(key)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            if stacked {
                draw_label(ui);
            }
            ui.allocate_new_ui(
                egui::UiBuilder::new()
                    .id_salt("values")
                    .max_rect(egui::Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(available, 22.0),
                    ))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    if !stacked && !label.is_empty() {
                        draw_label(ui);
                    }
                    let rect =
                        egui::Rect::from_min_size(ui.cursor().min, Vec2::new(control_width, 22.0));
                    ui.allocate_new_ui(
                        egui::UiBuilder::new()
                            .id_salt("control")
                            .max_rect(rect)
                            .layout(
                                egui::Layout::left_to_right(egui::Align::Center)
                                    .with_main_wrap(true),
                            ),
                        |child| {
                            child.set_min_size(Vec2::new(control_width, 22.0));
                            child.spacing_mut().combo_width = control_width;
                            child.spacing_mut().slider_width = child
                                .spacing()
                                .slider_width
                                .min(slider_track_width(child, 56.0));
                            if different && !locked {
                                child.visuals_mut().override_text_color = Some(ACCENT);
                            }
                            crate::settings_navigation::row_controls(child, rect, |child| {
                                child
                                    .add_enabled_ui(!locked, draw_control)
                                    .response
                                    .on_hover_text(&tooltip)
                                    .on_disabled_hover_text(&tooltip);
                            });
                        },
                    );
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(20.0, 22.0), egui::Sense::hover());
                    if let Some(reason) = &lock_tooltip {
                        if ui.is_visible() {
                            let mut painter = ui.painter().clone();
                            painter.set_fade_to_color(None);
                            paint_lock(&painter, rect.center(), CLI_LOCK);
                        }
                        response
                            .on_hover_text(reason)
                            .on_disabled_hover_text(reason);
                    }
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(12.0, 22.0), egui::Sense::hover());
                    if dirty {
                        ui.painter().circle_filled(rect.center(), 3.0, PENDING);
                        response
                            .on_hover_text(format!("Unapplied change. Saved: {}", saved.get(key)));
                    }
                },
            );
        },
    );
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
    Selector::new(
        ("backend", value.renderer.as_str()),
        choices
            .into_iter()
            .map(|backend| Choice::new(backend, backend_label(backend))),
    )
    .show(ui, &mut value.backend)
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

/// Scroll content may be wider than its viewport after a resize. Size controls
/// from the visible width, independently of the last frame's content extent.
fn visible_width(ui: &egui::Ui) -> f32 {
    ui.available_width().min(ui.clip_rect().width()).max(1.0)
}
fn slider_track_width(ui: &egui::Ui, number_width: f32) -> f32 {
    (visible_width(ui) - number_width - ui.spacing().item_spacing.x).max(1.0)
}

fn scale_slider(
    ui: &mut egui::Ui,
    value: &mut u32,
    cap: VideoCapabilities,
) -> (egui::Response, egui::Response) {
    let width = visible_width(ui);
    let (r, number) = ui
        .horizontal(|ui| {
            ui.set_width(width);
            ui.spacing_mut().slider_width = slider_track_width(ui, 48.0);
            ui.spacing_mut().interact_size.x = 48.0;
            let r = value_control(
                ui.add(
                    egui::Slider::new(value, cap.min_scale.max(1)..=cap.max_scale)
                        .step_by(1.0)
                        .integer()
                        .handle_shape(egui::style::HandleShape::Circle)
                        .show_value(false),
                ),
                Kind::Slider,
            );
            let number = value_control(
                ui.add(
                    egui::DragValue::new(value)
                        .range(cap.min_scale.max(1)..=cap.max_scale)
                        .prefix("×"),
                ),
                Kind::Number,
            );
            (r, number)
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
    (r, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_rows_stack_labels_and_keep_selectors_and_markers_inside_the_viewport() {
        for width in [180.0, 260.0, 370.0, 620.0] {
            for locked in [false, true] {
                let context = egui::Context::default();
                style(&context);
                let cli = if locked {
                    reforge_settings::Patch::from([("renderer".into(), "auto".into())])
                } else {
                    Default::default()
                };
                for _ in 0..3 {
                    let mut control_rect = egui::Rect::NOTHING;
                    let output = context.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width + 16.0, 500.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let left = ui.cursor().left();
                                let value = RuntimeSettings::default();
                                let mut selected = Renderer::Auto;
                                row(
                                    ui,
                                    "Renderer",
                                    "renderer",
                                    value.clone(),
                                    value.clone(),
                                    value,
                                    0,
                                    &cli,
                                    |ui| {
                                        let response = Selector::new(
                                            "responsive renderer",
                                            [Choice::new(
                                                Renderer::Auto,
                                                "Automatic · GPU preferred",
                                            )],
                                        )
                                        .show(ui, &mut selected);
                                        control_rect = response.rect;
                                        assert!(response.rect.left() >= left);
                                        assert!(response.rect.right() <= left + width + 0.5);
                                    },
                                );
                                assert!(ui.min_rect().right() <= left + width + 0.5);
                            });
                        },
                    );
                    let text = |label: &str| {
                        output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some(text.pos)
                            }
                            _ => None,
                        })
                    };
                    let label = text("Renderer").unwrap();
                    if width == 620.0 {
                        let value = text("Automatic · GPU preferred").unwrap();
                        assert!((label.y - value.y).abs() < 4.0);
                    } else {
                        assert!(label.y + 14.0 <= control_rect.top());
                    }
                }
            }
        }
    }

    #[test]
    fn scale_slider_keeps_a_fixed_value_box_and_remains_clickable_in_a_narrow_viewport() {
        for width in [180.0, 280.0, 370.0, 620.0] {
            let context = egui::Context::default();
            style(&context);
            let mut value = 1;
            let mut position = Pos2::ZERO;
            for step in 0..5 {
                let events = if matches!(step, 2 | 3) {
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: step == 2,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                } else {
                    Vec::new()
                };
                let _ = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(width + 16.0, 300.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let right = ui.cursor().left() + width;
                            let (track, number) =
                                scale_slider(ui, &mut value, VideoCapabilities::default());
                            assert!(number.rect.right() <= right + 0.5);
                            assert_eq!(number.rect.width(), 48.0);
                            assert!(track.rect.right() < number.rect.left());
                            position = Pos2::new(track.rect.right() - 3.0, track.rect.center().y);
                        });
                    },
                );
            }
            assert!(
                value >= 15,
                "narrow slider must still reach its upper values"
            );
        }
    }

    #[test]
    fn dependency_locks_preserve_values_geometry_and_pink_when_inherited() {
        for (key, policy, treatment) in [
            ("aspect_ratio", ScalingPolicy::Stretch, Presentation::Pixels),
            ("sampling", ScalingPolicy::Integer, Presentation::Hyllian),
        ] {
            let mut next_rows = Vec::new();
            for inherited in [false, true] {
                let context = egui::Context::default();
                style(&context);
                let mut value = RuntimeSettings {
                    aspect_ratio: AspectRatio::Square,
                    sampling: Sampling::Linear,
                    scaling_policy: policy,
                    presentation: treatment,
                    ..Default::default()
                };
                let saved = value.clone();
                let output = context.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(!inherited, |ui| {
                            row(
                                ui,
                                "Dependent setting",
                                key,
                                value.clone(),
                                saved.clone(),
                                saved.clone(),
                                1,
                                &Default::default(),
                                |ui| {
                                    let response = if key == "aspect_ratio" {
                                        Selector::new(
                                            "aspect-test",
                                            [Choice::new(
                                                AspectRatio::Square,
                                                "Square pixels (8:5)",
                                            )],
                                        )
                                        .show(ui, &mut value.aspect_ratio)
                                    } else {
                                        Selector::new(
                                            "sampling-test",
                                            [Choice::new(
                                                Sampling::Linear,
                                                "Linear · smooth pixels",
                                            )],
                                        )
                                        .show(ui, &mut value.sampling)
                                    };
                                    assert!(!response.enabled());
                                },
                            );
                            next_rows.push(ui.label("Next row").rect.top());
                        });
                    });
                });
                assert_eq!(value, saved);
                assert!(
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.fill == CLI_LOCK)),
                    "lock colour must survive inherited UI opacity"
                );
            }
            assert_eq!(next_rows[0], next_rows[1]);
        }
    }

    #[test]
    fn cli_locks_disable_radio_and_checkbox_without_inserting_rows() {
        for key in ["window_mode", "vsync"] {
            let mut bottoms = Vec::new();
            for locked in [false, true] {
                let context = egui::Context::default();
                style(&context);
                let cli = if locked {
                    reforge_settings::Patch::from([(
                        key.into(),
                        if key == "window_mode" {
                            "fullscreen"
                        } else {
                            "on"
                        }
                        .into(),
                    )])
                } else {
                    Default::default()
                };
                let mut selected = false;
                let mut target = Pos2::ZERO;
                for step in 0..5 {
                    let events = if matches!(step, 2 | 3) {
                        vec![
                            egui::Event::PointerMoved(target),
                            egui::Event::PointerButton {
                                pos: target,
                                button: egui::PointerButton::Primary,
                                pressed: step == 2,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]
                    } else {
                        Vec::new()
                    };
                    let _ = context.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(900.0, 600.0),
                            )),
                            time: Some(step as f64 * 0.02),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let value = RuntimeSettings::default();
                                row(
                                    ui,
                                    "Setting",
                                    key,
                                    value.clone(),
                                    value.clone(),
                                    value,
                                    1,
                                    &cli,
                                    |ui| {
                                        let response = if key == "window_mode" {
                                            ui.radio_value(&mut selected, true, "Fullscreen")
                                        } else {
                                            ui.checkbox(&mut selected, "Synchronise with display")
                                        };
                                        target = response.rect.center();
                                        assert_eq!(response.enabled(), !locked);
                                    },
                                );
                                let next = ui.label("Next row");
                                if step == 4 {
                                    bottoms.push(next.rect.top());
                                }
                            });
                        },
                    );
                }
                assert_eq!(selected, !locked);
            }
            assert_eq!(
                bottoms[0], bottoms[1],
                "locks must not add explanation rows"
            );
        }
    }

    #[test]
    fn cli_notice_is_scoped_to_the_current_section_and_native_option() {
        let cli = reforge_settings::Patch::from([
            ("scale".into(), "8".into()),
            (DIGITIZED_SAMPLES.into(), "false".into()),
        ]);
        assert!(section_has_cli_locks(&cli, 0, 0, true));
        assert!(section_has_cli_locks(&cli, 0, 1, false));
        assert!(!section_has_cli_locks(&cli, 1, 1, true));
        assert!(!section_has_cli_locks(&cli, 2, 0, true));
        assert!(!section_has_cli_locks(&cli, 2, 1, false));
        assert!(section_has_cli_locks(&cli, 2, 1, true));
        assert!(!section_has_cli_locks(&Default::default(), 2, 1, true));
        assert!(cli_tooltip(DIGITIZED_SAMPLES, "false").contains("--digitized-samples off"));
    }

    #[test]
    fn mach3_sample_control_follows_section_defaults_and_cli_locks() {
        fn text_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
        }
        for (scope, mach3, section_editable, muted, locked) in [
            (0, true, true, false, false),
            (1, false, true, false, false),
            (1, true, false, true, false),
            (1, true, false, false, false),
            (1, true, true, false, false),
            (1, true, true, true, false),
            (1, true, true, false, true),
            (1, true, false, false, true),
        ] {
            let context = egui::Context::default();
            style(&context);
            let mut value = RuntimeSettings::default();
            value.audio.enabled = !muted;
            let cli = if locked {
                reforge_settings::Patch::from([(DIGITIZED_SAMPLES.into(), "false".into())])
            } else {
                Default::default()
            };
            let mut button = Pos2::ZERO;
            let mut off = Pos2::ZERO;
            for step in 0..9 {
                let point = if step < 6 { button } else { off };
                let events = if matches!(step, 2 | 3 | 6 | 7) {
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: step == 2 || step == 6,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                } else {
                    Vec::new()
                };
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(900.0, 900.0),
                        )),
                        time: Some(step as f64 * 0.02),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            audio_controls(
                                ui,
                                &mut value,
                                RuntimeSettings::default(),
                                RuntimeSettings::default(),
                                scope,
                                &cli,
                                &[],
                                &[SoundRenderer::Speaker],
                                &SoundCategory::ALL,
                                section_editable,
                                mach3,
                            );
                        });
                    },
                );
                let label = text_rect(&output, "Digitized samples");
                if scope == 0 || !mach3 {
                    assert!(label.is_none());
                    break;
                }
                let label = label.expect("Mach 3 control must be visible");
                if locked {
                    assert!(
                        output.shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == CLI_LOCK)
                        }),
                        "CLI lock must retain its colour when section defaults disable the row"
                    );
                }
                assert!(label.top() > text_rect(&output, "Sound renderer").unwrap().bottom());
                if step < 2 {
                    button = text_rect(&output, "On").unwrap().center();
                }
                if step == 5 {
                    if locked || !section_editable {
                        assert!(text_rect(&output, "Off").is_none());
                        assert!(value.mach3.digitized_samples);
                        break;
                    }
                    off = text_rect(&output, "Off")
                        .expect("popup opens only when this section is editable")
                        .center();
                }
            }
            if scope == 1 && mach3 && section_editable && !locked {
                assert!(!value.mach3.digitized_samples);
                assert_eq!(value.audio.enabled, !muted);
            }
        }
    }

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
