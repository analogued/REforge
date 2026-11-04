// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
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
    AspectRatio, BTZ_FILE_CURSOR, Backend, CONTROL_MODE, CONTROLLER_ACTION, CONTROLLER_BUTTONS,
    CONTROLLER_MOVEMENT, ControlMode, DIGITIZED_SAMPLES, FileCursorStyle, FrameLimit, MOTION,
    MotionMode, Presentation, Renderer, RuntimeSettings, SECTION_KEYS, SHADER_PARAMETERS,
    STICK_DEAD_ZONE, Sampling, ScalingPolicy, SoundCategory, SoundRenderer,
};
mod controller;
const PAGES: [&str; 4] = ["Display", "CRT & Colour", "Audio", "Controls"];
// Presence is distinct from equality: explicit overrides may match global.
#[derive(Clone, Copy, Default)]
struct Overrides<'a> {
    current: Option<&'a reforge_settings::Patch>,
    saved: Option<&'a reforge_settings::Patch>,
}
impl<'a> Overrides<'a> {
    fn current(self, key: &str) -> Option<&'a str> {
        self.current
            .and_then(|patch| patch.get(key))
            .map(String::as_str)
    }
    fn saved(self, key: &str) -> Option<&'a str> {
        self.saved
            .and_then(|patch| patch.get(key))
            .map(String::as_str)
    }
}

// Borrow page snapshots; each widget receives only its own marker/tooltip data.
#[derive(Clone, Copy)]
struct RowContext<'a> {
    base: &'a RuntimeSettings,
    previous: &'a RuntimeSettings,
    scope: usize,
    overrides: Overrides<'a>,
    cli: &'a reforge_settings::Patch,
}
struct RowState {
    lock_tooltip: Option<String>,
    overridden: bool,
    dirty: bool,
    tooltip: String,
    saved: String,
}
impl RowContext<'_> {
    fn state(self, key: &str, value: &RuntimeSettings) -> RowState {
        let lock_tooltip = self
            .cli
            .get(key)
            .map(|active| cli_tooltip(key, active))
            .or_else(|| dependency_lock(key, value).map(str::to_owned));
        let saved = self.previous.get(key);
        let dirty = !self.cli.contains_key(key)
            && if self.scope == 1 {
                self.overrides.current(key) != self.overrides.saved(key)
            } else {
                value.get(key) != saved
            };
        let mut tooltip = format!(
            "{}: {}\nSaved: {}",
            if matches!(
                key,
                DIGITIZED_SAMPLES
                    | BTZ_FILE_CURSOR
                    | CONTROL_MODE
                    | MOTION
                    | CONTROLLER_MOVEMENT
                    | CONTROLLER_ACTION
                    | CONTROLLER_BUTTONS
            ) {
                "Default"
            } else {
                "Global"
            },
            self.base.get(key),
            saved
        );
        if self.scope == 1 {
            tooltip.push_str(if self.overrides.saved(key).is_some() {
                " (game override)"
            } else {
                " (inherited)"
            });
            tooltip.push_str(&match self.overrides.current(key) {
                Some(value) => format!("\nGame override: {value}"),
                None => "\nFollows defaults; no game override.".into(),
            });
        }
        if let Some(reason) = &lock_tooltip {
            tooltip = format!("{reason}\n\n{tooltip}");
        }
        RowState {
            lock_tooltip,
            dirty,
            tooltip,
            saved,
            overridden: self.scope == 1 && self.overrides.current(key).is_some(),
        }
    }
}

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
        s.visuals.slider_trailing_fill = true;
    });
    crate::ui::current_theme(context).apply_palette(context);
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
    let game = session.game_id.clone();
    let warning = session.warning.clone();
    drop(session);
    let draft = menu.draft.as_mut().expect("open draft");
    let windowed_scale = menu.creator.windowed_scale.get();
    let geometry = menu.creator.geometry.get();
    let width = (screen.width() - 36.0).clamp(300.0, 860.0);
    let height = (screen.height() - 48.0).max(260.0);
    let body_height = (height - 120.0).max(160.0);
    let mut close = false;
    if !menu.game_overrides {
        menu.scope = 0;
    }
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
                let game_scope = menu.game_overrides.then(|| {
                    group_control(
                        ui.selectable_value(
                            &mut menu.scope,
                            1,
                            format!(
                                "{} overrides",
                                match game.as_str() {
                                    "alley-cat" => "Alley Cat",
                                    "mach-3" => "Mach 3",
                                    "mindwheel" => "Mindwheel",
                                    "essex" => "Essex",
                                    "brimstone" => "Brimstone",
                                    _ => "Game",
                                }
                            ),
                        ),
                        Group::Scope,
                    )
                });
                if menu.navigation.focus_first && first.enabled() {
                    if menu.scope == 0 {
                        first.request_focus();
                    } else if let Some(game_scope) = game_scope {
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
                                // egui otherwise makes selected buttons square.
                                egui::Button::new({
                                    let palette = crate::ui::current_palette(ui.ctx());
                                    let text = egui::RichText::new(label);
                                    if !palette.dark && menu.page == page {
                                        text.color(palette.selection.foreground)
                                    } else {
                                        text
                                    }
                                })
                                .selected(menu.page == page)
                                .corner_radius(3.0),
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
                    // Floating bars expand over content unless their full width
                    // is reserved. Keep the gutter even while the handle fades
                    // so right-edge controls stay visible and clickable.
                    let scroll = &mut ui.spacing_mut().scroll;
                    if scroll.floating {
                        scroll.floating_allocated_width =
                            scroll.bar_inner_margin + scroll.bar_width + scroll.bar_outer_margin;
                    }
                    ui.heading(PAGES[menu.page]);
                    if section_has_cli_locks(&cli, menu.page, menu.scope, menu.mach3_options) {
                        cli_notice(ui);
                    }
                    if menu.scope == 1 {
                        let mut override_defaults = menu.game_editing[menu.page];
                        if control(ui.checkbox(
                            &mut override_defaults,
                            canvas_text(
                                ui,
                                "Override global defaults · only differences are saved",
                            ),
                        ))
                        .changed()
                        {
                            menu.game_editing[menu.page] = override_defaults;
                            if !override_defaults {
                                draft.reset_section(true, menu.page);
                                menu.game_reset[menu.page] = true;
                            }
                        }
                    } else {
                        ui.weak("Shared defaults · individual games can override these values");
                    }
                    ui.add_space(8.0);
                    let overrides = Overrides {
                        current: Some(&draft.game),
                        saved: Some(&saved.game),
                    };
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
                    let rows = RowContext {
                        base: &base,
                        previous: &previous,
                        scope: menu.scope,
                        overrides,
                        cli: &cli,
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
                                    display_controls(
                                        ui,
                                        &mut value,
                                        rows,
                                        DisplayOptions {
                                            capabilities: menu.capabilities,
                                            geometry,
                                            windowed_scale,
                                            original_renderer: original.renderer,
                                            mach3: menu.scope == 1 && menu.mach3_options,
                                            terminal: (menu.scope == 1)
                                                .then_some(menu.btz_terminal)
                                                .flatten(),
                                        },
                                    );
                                } else if menu.page == 1 {
                                    crt_controls(ui, &mut value, rows);
                                } else if menu.page == 2 {
                                    audio_controls(
                                        ui,
                                        &mut value,
                                        rows,
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
                                        rows,
                                        controls_visible,
                                        menu.controller_style,
                                    );
                                }
                            });
                            crate::settings_navigation::scroll_focus(ui, first);
                        });
                    if menu.page == 0
                        && menu.scope == 1
                        && menu.btz_terminal.is_some()
                        && original.btz != value.btz
                        && !cli.contains_key(BTZ_FILE_CURSOR)
                    {
                        draft.edit_game(
                            BTZ_FILE_CURSOR,
                            value.get(BTZ_FILE_CURSOR),
                            (!menu.game_reset[menu.page]).then_some(&saved),
                        );
                    }
                    if menu.page == 0
                        && menu.scope == 1
                        && menu.mach3_options
                        && original.mach3.motion != value.mach3.motion
                        && !cli.contains_key(MOTION)
                    {
                        draft.edit_game(
                            MOTION,
                            value.get(MOTION),
                            (!menu.game_reset[menu.page]).then_some(&saved),
                        );
                    }
                    if menu.page == 2
                        && menu.scope == 1
                        && menu.mach3_options
                        && original.mach3 != value.mach3
                        && !cli.contains_key(DIGITIZED_SAMPLES)
                    {
                        draft.edit_game(
                            DIGITIZED_SAMPLES,
                            value.get(DIGITIZED_SAMPLES),
                            (!menu.game_reset[menu.page]).then_some(&saved),
                        );
                    }
                    if menu.page == 3 && controls_visible {
                        for key in [
                            CONTROL_MODE,
                            CONTROLLER_MOVEMENT,
                            CONTROLLER_ACTION,
                            CONTROLLER_BUTTONS,
                        ] {
                            if original.get(key) != value.get(key) && !cli.contains_key(key) {
                                draft.edit_game(
                                    key,
                                    value.get(key),
                                    (!menu.game_reset[menu.page]).then_some(&saved),
                                );
                            }
                        }
                    }
                    let keys = SECTION_KEYS[menu.page];
                    for &key in keys {
                        if original.get(key) != value.get(key) && !cli.contains_key(key) {
                            if menu.scope == 1 {
                                draft.edit_game(
                                    key,
                                    value.get(key),
                                    (!menu.game_reset[menu.page]).then_some(&saved),
                                );
                            } else {
                                draft.edit(false, key, value.get(key));
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
                    } else if menu.page == 3
                        || ([0, 2].contains(&menu.page) && menu.mach3_options)
                        || (menu.page == 0 && menu.btz_terminal.is_some())
                    {
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
                            menu.game_reset[menu.page] = true;
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
                        if !menu.game_overrides {
                            draft.game.clone_from(&saved.game);
                        }
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
                ui.colored_label(crate::ui::current_palette(ui.ctx()).notice_error, error);
            }
            if let Some(status) = &menu.status {
                ui.weak(status);
            }
        });
    close
}
struct DisplayOptions {
    capabilities: VideoCapabilities,
    geometry: crate::DisplayGeometry,
    windowed_scale: u32,
    original_renderer: Renderer,
    terminal: Option<bool>,
    mach3: bool,
}

fn display_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    rows: RowContext<'_>,
    options: DisplayOptions,
) {
    let DisplayOptions {
        capabilities,
        geometry,
        windowed_scale,
        original_renderer,
        terminal,
        mach3,
    } = options;
    let RowContext { scope, cli, .. } = rows;
    ui.add_enabled_ui(value.renderer != Renderer::Terminal, |ui| {
        section(ui, "Window & scaling");
        row(
            ui,
            "Window mode",
            "window_mode",
            rows.state("window_mode", value),
            |ui| {
                control(ui.radio_value(&mut value.fullscreen, false, canvas_text(ui, "Windowed")));
                ui.add_enabled_ui(capabilities.allow_fullscreen, |ui| {
                    control(ui.radio_value(
                        &mut value.fullscreen,
                        true,
                        canvas_text(ui, "Fullscreen"),
                    ));
                });
            },
        );
        row(
            ui,
            "Window scale",
            "scale",
            rows.state("scale", value),
            |ui| {
                let mut auto = value.scale.is_none();
                if control(ui.add_enabled(
                    capabilities.allow_auto || auto,
                    egui::Checkbox::new(&mut auto, canvas_text(ui, "Auto")),
                ))
                .changed()
                {
                    value.scale = if auto {
                        None
                    } else {
                        Some(
                            windowed_scale
                                .clamp(capabilities.min_scale.max(1), capabilities.max_scale),
                        )
                    };
                }
                if value.scale.is_none() {
                    ui.label("Largest fit");
                }
            },
        );
        ui.add_enabled_ui(
            value.scale.is_some() && !value.fullscreen && !cli.contains_key("scale"),
            |ui| {
                let mut n = value.scale.unwrap_or(windowed_scale);
                let mut cap = capabilities;
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
            rows.state("scaling_policy", value),
            |ui| {
                Selector::new(
                    "scaling_policy",
                    [
                        Choice::new(ScalingPolicy::Integer, "Integer fit"),
                        Choice::new(ScalingPolicy::Fit, "Fit to window (fractional)"),
                        Choice::new(ScalingPolicy::Stretch, "Fill window (stretch)"),
                    ],
                )
                .show(ui, &mut value.scaling_policy);
            },
        );
        row(
            ui,
            "Sampling",
            "sampling",
            rows.state("sampling", value),
            |ui| {
                Selector::new(
                    "sampling",
                    [
                        Choice::new(Sampling::Nearest, "Nearest · sharp pixels"),
                        Choice::new(Sampling::Linear, "Linear · smooth pixels"),
                    ],
                )
                .show(ui, &mut value.sampling);
            },
        );
        row(
            ui,
            "Aspect ratio",
            "aspect_ratio",
            rows.state("aspect_ratio", value),
            |ui| {
                Selector::new(
                    "aspect_ratio",
                    [AspectRatio::Original, AspectRatio::Square]
                        .map(|aspect| Choice::new(aspect, geometry.label(aspect, scope == 1))),
                )
                .show(ui, &mut value.aspect_ratio);
            },
        );
    });
    if mach3 {
        mach3_motion_controls(ui, value, rows);
    }
    section(ui, "Rendering");
    row(
        ui,
        "Renderer",
        "renderer",
        rows.state("renderer", value),
        |ui| {
            Selector::new(
                "renderer",
                renderer_options(terminal).into_iter().map(|(renderer, available)| {
                    Choice::new(renderer, renderer_label(renderer)).enabled(
                        available,
                        "Terminal play requires an attached terminal and is selected before the game starts. Use --renderer terminal.",
                    )
                }),
            )
            .show(ui, &mut value.renderer);
        },
    );
    if value.renderer != Renderer::Terminal
        && value.renderer != original_renderer
        && !cli.contains_key("renderer")
        && !cli.contains_key("backend")
        && !backends(value.renderer).contains(&value.backend)
    {
        value.backend = Backend::Auto;
    }
    ui.add_enabled_ui(value.renderer != Renderer::Terminal, |ui| {
        row(
            ui,
            "Backend",
            "backend",
            rows.state("backend", value),
            |ui| {
                ui.add_enabled_ui(value.renderer != Renderer::Software, |ui| {
                    backend_selector(ui, value);
                });
            },
        );
        row(ui, "VSync", "vsync", rows.state("vsync", value), |ui| {
            control(ui.checkbox(
                &mut value.vsync,
                canvas_text(ui, "Synchronise with the display"),
            ));
        });
        row(
            ui,
            "Frame limit",
            "frame_limit",
            rows.state("frame_limit", value),
            |ui| {
                Selector::new(
                    "frame_limit",
                    FrameLimit::ALL.map(|limit| Choice::new(limit, limit.label())),
                )
                .show(ui, &mut value.frame_limit);
            },
        );
        ui.weak("Presentation ceiling · simulation timing is unchanged");
    });
    if value.renderer == Renderer::Terminal {
        ui.weak("Terminal play uses the attached terminal. Window and CRT settings do not apply.");
    }
    if terminal.is_some() {
        section(ui, "Text interface");
        ui.add_enabled_ui(value.renderer != Renderer::Terminal, |ui| {
            row(
                ui,
                "File-dialog cursor",
                BTZ_FILE_CURSOR,
                rows.state(BTZ_FILE_CURSOR, value),
                |ui| {
                    Selector::new(
                        BTZ_FILE_CURSOR,
                        [
                            Choice::new(FileCursorStyle::PulsingUnderline, "Pulsing underline"),
                            Choice::new(FileCursorStyle::SteadyBlock, "Steady block"),
                        ],
                    )
                    .show(ui, &mut value.btz.file_cursor);
                },
            );
            ui.weak("Bookmark and transcript dialogs · the underline stays visible and alternates between two heights.");
        });
    }
}

fn renderer_options(terminal: Option<bool>) -> Vec<(Renderer, bool)> {
    let mut choices = vec![
        (Renderer::Auto, true),
        (Renderer::Gpu, true),
        (Renderer::Sdl, true),
        (Renderer::Software, true),
    ];
    if let Some(available) = terminal {
        choices.push((Renderer::Terminal, available));
    }
    choices
}

fn controls_controls(
    ui: &mut egui::Ui,
    value: &mut RuntimeSettings,
    rows: RowContext<'_>,
    alleycat: bool,
    controller_style: crate::gamepad::ControllerStyle,
) {
    if alleycat {
        controller::controls(ui, value, rows, controller_style);
    }
    section(ui, "Controller");
    row(
        ui,
        "Stick dead zone",
        STICK_DEAD_ZONE,
        rows.state(STICK_DEAD_ZONE, value),
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
        rows.state(CONTROL_MODE, value),
        |ui| {
            Selector::new(CONTROL_MODE, ControlMode::ALL.map(|mode| Choice::new(mode, mode.label()))).show(ui, &mut value.alleycat.control_mode).on_hover_text("Responsive reacts sooner to direction and action changes. Original preserves the game's original input response.");
        },
    );
}

fn crt_controls(ui: &mut egui::Ui, value: &mut RuntimeSettings, rows: RowContext<'_>) {
    let terminal = value.renderer == Renderer::Terminal
        || rows
            .cli
            .get("renderer")
            .is_some_and(|renderer| renderer == "terminal");
    ui.add_enabled_ui(!terminal, |ui| {
        let cli = rows.cli;
        // Use this scope's pending Display choices, with temporary CLI
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
            rows.state("presentation", value),
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
                            Presentation::Hyllian => hyllian_controls(ui, value, rows),
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
    });
    if terminal {
        ui.weak("CRT settings apply to windowed play; the terminal controls its own appearance.");
    }
}

fn hyllian_controls(ui: &mut egui::Ui, value: &mut RuntimeSettings, rows: RowContext<'_>) {
    row(
        ui,
        "Horizontal filter",
        "horizontal_filter",
        rows.state("horizontal_filter", value),
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
            rows.state(param.key, value),
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
    rows: RowContext<'_>,
    devices: &[String],
    renderers: &[SoundRenderer],
    categories: &[SoundCategory],
    section_editable: bool,
    mach3_options: bool,
) {
    let scope = rows.scope;
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
            rows.state("sound_enabled", value),
            |ui| {
                control(ui.checkbox(&mut value.audio.enabled, canvas_text(ui, "Enable sound")));
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
            rows.state("master_volume", value),
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
                rows.state(category.key(), value),
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
            rows.state("audio_renderer", value),
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
                rows.state(DIGITIZED_SAMPLES, value),
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
            rows.state("output_device", value),
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
            rows.state("mute_unfocused", value),
            |ui| {
                control(ui.checkbox(
                    &mut value.audio.mute_unfocused,
                    canvas_text(ui, "Mute when unfocused"),
                ));
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
        || (page == 0 && scope == 1 && mach3_options && cli.contains_key(MOTION))
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
    let keyhole = crate::ui::current_palette(painter.ctx()).lock_keyhole;
    painter.circle_filled(center + Vec2::new(0.0, 2.0), 1.2, keyhole);
    painter.line_segment(
        [center + Vec2::new(0.0, 2.0), center + Vec2::new(0.0, 5.0)],
        egui::Stroke::new(1.2_f32, keyhole),
    );
}

fn cli_notice(ui: &mut egui::Ui) -> egui::Response {
    let palette = crate::ui::current_palette(ui.ctx());
    egui::Frame::NONE
        .fill(palette.cli_background)
        .corner_radius(3.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 18.0), egui::Sense::hover());
                paint_lock(ui.painter(), rect.center(), palette.cli_text);
                control(
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Some settings are locked by command-line arguments.",
                            )
                            .size(13.0)
                            .color(palette.cli_text),
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
    ui.label(
        egui::RichText::new(label)
            .strong()
            .color(crate::ui::current_palette(ui.ctx()).strong_text),
    );
    ui.separator();
}
fn mach3_motion_controls(ui: &mut egui::Ui, value: &mut RuntimeSettings, rows: RowContext<'_>) {
    let legacy_id = ui.make_persistent_id("mach3_original_scene_motion");
    if value.mach3.motion != MotionMode::Reforged {
        ui.data_mut(|data| data.insert_temp(legacy_id, value.mach3.motion));
    }
    let reforged_scene = value.mach3.motion == MotionMode::Reforged;
    let mut scene = reforged_scene;
    row(
        ui,
        "Scene movement",
        MOTION,
        rows.state(MOTION, value),
        |ui| {
            Selector::new(
                MOTION,
                [
                    Choice::new(true, "Reforged"),
                    Choice::new(false, "Original"),
                ],
            )
            .show(ui, &mut scene)
            .on_hover_text(concat!(
                "Reforged smooths scene movement. Original retains native scene updates ",
                "and lets you choose player craft movement. Gameplay timing and collisions ",
                "are unchanged.",
            ));
            if scene != reforged_scene {
                value.mach3.motion = if scene {
                    MotionMode::Reforged
                } else {
                    ui.data(|data| data.get_temp(legacy_id))
                        .unwrap_or(MotionMode::Enhanced)
                };
            }
        },
    );
    if value.mach3.motion != MotionMode::Reforged {
        row(
            ui,
            "Player craft movement",
            "player_craft_motion",
            rows.state(MOTION, value),
            |ui| {
                Selector::new(
                    "player_craft_motion",
                    [
                        Choice::new(MotionMode::Enhanced, "Enhanced"),
                        Choice::new(MotionMode::Original, "Original"),
                    ],
                )
                .show(ui, &mut value.mach3.motion)
                .on_hover_text(concat!(
                    "Enhanced smooths the player craft and its shadow. Original retains ",
                    "native craft placement and publication rhythm.",
                ));
            },
        );
        ui.data_mut(|data| data.insert_temp(legacy_id, value.mach3.motion));
    }
}

fn renderer_label(r: Renderer) -> &'static str {
    match r {
        Renderer::Auto => "Automatic · GPU preferred",
        Renderer::Gpu => "GPU",
        Renderer::Sdl => "SDL renderer",
        Renderer::Software => "Software",
        Renderer::Terminal => "Terminal",
    }
}
// Checkbox/radio labels sit on the canvas; only their icons use the active fill.
// Keep their text independent of that fill's contrasting foreground, while
// retaining row override colours and egui's normal disabled fading.
fn canvas_text(ui: &egui::Ui, label: &str) -> egui::RichText {
    egui::RichText::new(label).color(
        ui.visuals()
            .override_text_color
            .unwrap_or(ui.visuals().widgets.inactive.fg_stroke.color),
    )
}

fn row(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    state: RowState,
    draw_control: impl FnOnce(&mut egui::Ui),
) {
    row_with_width(ui, label, key, state, 220.0, draw_control);
}

fn row_with_width(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    state: RowState,
    control_width: f32,
    draw_control: impl FnOnce(&mut egui::Ui),
) {
    let RowState {
        lock_tooltip,
        overridden,
        dirty,
        tooltip,
        saved,
    } = state;
    let locked = lock_tooltip.is_some();
    let palette = crate::ui::current_palette(ui.ctx());
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
        palette.locked_text
    } else if overridden {
        palette.override_text
    } else {
        ui.visuals().text_color()
    };
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
                            if overridden && !locked {
                                // Accent ordinary text and neutral controls without
                                // replacing the active fill's contrasting foreground.
                                let visuals = child.visuals_mut();
                                visuals.override_text_color = None;
                                for widget in [
                                    &mut visuals.widgets.noninteractive,
                                    &mut visuals.widgets.inactive,
                                    &mut visuals.widgets.hovered,
                                    &mut visuals.widgets.open,
                                ] {
                                    widget.fg_stroke.color = palette.override_text;
                                }
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
                            paint_lock(&painter, rect.center(), palette.cli_lock);
                        }
                        response
                            .on_hover_text(reason)
                            .on_disabled_hover_text(reason);
                    }
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(12.0, 22.0), egui::Sense::hover());
                    if dirty {
                        ui.painter()
                            .circle_filled(rect.center(), 3.0, palette.pending);
                        response.on_hover_text(format!("Unapplied change. Saved: {}", saved));
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
    if renderer == Renderer::Terminal {
        return values;
    }
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
    const ACCENT: Color32 = Color32::from_rgb(100, 200, 226);
    const PENDING: Color32 = Color32::from_rgb(245, 185, 90);
    const CLI_LOCK: Color32 = Color32::from_rgb(182, 169, 214);
    #[test]
    fn override_colours_and_pending_dots_follow_presence_even_when_values_match() {
        for scope in [0, 1] {
            for current in [false, true] {
                for saved in [false, true] {
                    let context = egui::Context::default();
                    style(&context);
                    let mut current_patch = reforge_settings::Patch::new();
                    let mut saved_patch = reforge_settings::Patch::new();
                    if current {
                        current_patch.insert("window_mode".into(), "windowed".into());
                    }
                    if saved {
                        saved_patch.insert("window_mode".into(), "windowed".into());
                    }
                    let output = context.run(Default::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            row(
                                ui,
                                "Window mode",
                                "window_mode",
                                RowContext {
                                    base: &RuntimeSettings::default(),
                                    previous: &RuntimeSettings::default(),
                                    scope,
                                    overrides: Overrides {
                                        current: Some(&current_patch),
                                        saved: Some(&saved_patch),
                                    },
                                    cli: &Default::default(),
                                }
                                .state("window_mode", &RuntimeSettings::default()),
                                |ui| {
                                    ui.label("Windowed");
                                },
                            );
                        });
                    });
                    let text = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.text() == "Window mode" => Some(t),
                            _ => None,
                        })
                        .unwrap();
                    let accent = text
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|s| s.format.color == ACCENT);
                    let dot = output
                        .shapes
                        .iter()
                        .any(|s| matches!(&s.shape, egui::Shape::Circle(c) if c.fill==PENDING));
                    assert_eq!(accent, scope == 1 && current);
                    assert_eq!(dot, scope == 1 && current != saved);
                }
            }
        }
    }

    use super::*;

    #[test]
    fn themed_control_text_contrasts_with_its_canvas_or_focused_fill() {
        use crate::settings_selector::{InputMode, set_mode};
        use crate::ui::themes::{ResolvedAppearance, ThemeId};

        for theme in ThemeId::ALL {
            for overridden in [false, true] {
                for mode in [InputMode::Mouse, InputMode::Keyboard, InputMode::Controller] {
                    for kind in ["checkbox", "radio", "selector", "number"] {
                        let context = egui::Context::default();
                        style(&context);
                        let appearance = ResolvedAppearance::resolve(theme);
                        appearance.theme.apply_palette(&context);
                        set_mode(&context, mode);
                        let palette = appearance.theme.palette;
                        let mut response: Option<egui::Response> = None;
                        for focused in [false, true] {
                            // First settle layout, then request focus before the next frame.
                            if focused {
                                response.as_ref().unwrap().request_focus();
                            }
                            let output = context.run(Default::default(), |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    row(
                                        ui,
                                        "Option",
                                        "colour regression",
                                        RowState {
                                            lock_tooltip: None,
                                            overridden,
                                            dirty: false,
                                            tooltip: String::new(),
                                            saved: String::new(),
                                        },
                                        |ui| {
                                            response = Some(match kind {
                                                "checkbox" => ui.checkbox(
                                                    &mut true,
                                                    canvas_text(ui, "Canvas label"),
                                                ),
                                                "radio" => ui.radio_value(
                                                    &mut true,
                                                    true,
                                                    canvas_text(ui, "Canvas label"),
                                                ),
                                                "selector" => Selector::new(
                                                    "colour selector",
                                                    [Choice::new(0, "Control value")],
                                                )
                                                .show(ui, &mut 0),
                                                "number" => ui.add(egui::DragValue::new(&mut 8)),
                                                _ => unreachable!(),
                                            });
                                        },
                                    );
                                });
                            });
                            let expected =
                                if matches!(kind, "checkbox" | "radio" | "number") || !focused {
                                    if overridden {
                                        palette.override_text
                                    } else {
                                        palette.inactive.foreground
                                    }
                                } else {
                                    palette.active.foreground
                                };
                            let label = match kind {
                                "checkbox" | "radio" => "Canvas label",
                                "selector" => "Control value",
                                _ => "8",
                            };
                            let colours: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.text() == label => {
                                        let colour = text.galley.job.sections[0].format.color;
                                        Some(text.override_text_color.unwrap_or(
                                            if colour == Color32::PLACEHOLDER {
                                                text.fallback_color
                                            } else {
                                                colour
                                            },
                                        ))
                                    }
                                    _ => None,
                                })
                                .filter(|colour| *colour != Color32::TRANSPARENT)
                                .collect();
                            assert_eq!(
                                colours,
                                [expected],
                                "{theme:?} override={overridden} focused={focused} {kind}"
                            );
                            assert_eq!(response.as_ref().unwrap().has_focus(), focused);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn terminal_choice_is_explicitly_scoped_and_requires_an_attached_terminal() {
        assert!(
            renderer_options(None)
                .iter()
                .all(|(renderer, _)| *renderer != Renderer::Terminal)
        );
        for available in [false, true] {
            let choices = renderer_options(Some(available));
            assert_eq!(choices.last(), Some(&(Renderer::Terminal, available)));
            assert!(
                choices[..choices.len() - 1]
                    .iter()
                    .all(|(_, enabled)| *enabled)
            );
        }
        assert_eq!(backends(Renderer::Terminal), vec![Backend::Auto]);
        assert_eq!(renderer_label(Renderer::Terminal), "Terminal");
    }

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
                                    RowContext {
                                        base: &value,
                                        previous: &value,
                                        scope: 0,
                                        overrides: Overrides::default(),
                                        cli: &cli,
                                    }
                                    .state("renderer", &value),
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
                                RowContext {
                                    base: &saved,
                                    previous: &saved,
                                    scope: 1,
                                    overrides: Overrides::default(),
                                    cli: &Default::default(),
                                }
                                .state(key, &value),
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
                                    RowContext {
                                        base: &value,
                                        previous: &value,
                                        scope: 1,
                                        overrides: Overrides::default(),
                                        cli: &cli,
                                    }
                                    .state(key, &value),
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
                                RowContext {
                                    base: &RuntimeSettings::default(),
                                    previous: &RuntimeSettings::default(),
                                    scope,
                                    overrides: Overrides::default(),
                                    cli: &cli,
                                },
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
    fn mach3_motion_control_follows_display_defaults_scope_and_cli_locks() {
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
                reforge_settings::Patch::from([(MOTION.into(), "original".into())])
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
                            ui.add_enabled_ui(section_editable, |ui| {
                                display_controls(
                                    ui,
                                    &mut value,
                                    RowContext {
                                        base: &RuntimeSettings::default(),
                                        previous: &RuntimeSettings::default(),
                                        scope,
                                        overrides: Overrides::default(),
                                        cli: &cli,
                                    },
                                    DisplayOptions {
                                        capabilities: VideoCapabilities::default(),
                                        geometry: crate::DisplayGeometry::new(
                                            (320, 200),
                                            (320, 240),
                                        ),
                                        windowed_scale: 2,
                                        original_renderer: Renderer::Auto,
                                        terminal: None,
                                        mach3: scope == 1 && mach3,
                                    },
                                );
                            });
                        });
                    },
                );
                let label = text_rect(&output, "Scene movement");
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
                assert!(label.top() > text_rect(&output, "Aspect ratio").unwrap().bottom());
                if step < 2 {
                    button = text_rect(&output, "Reforged").unwrap().center();
                }
                if step == 5 {
                    if locked || !section_editable {
                        assert!(text_rect(&output, "Original").is_none());
                        assert_eq!(value.mach3.motion, MotionMode::Reforged);
                        break;
                    }
                    off = text_rect(&output, "Original")
                        .expect("popup opens only when this section is editable")
                        .center();
                }
            }
            if scope == 1 && mach3 && section_editable && !locked {
                assert_eq!(value.mach3.motion, MotionMode::Enhanced);
                assert_eq!(value.audio.enabled, !muted);
            }
        }
    }

    #[test]
    fn mach3_scene_and_craft_selectors_preserve_legacy_choice_and_locks() {
        use crate::settings_selector::{InputMode, set_mode};
        struct Harness {
            context: egui::Context,
            value: RuntimeSettings,
            cli: reforge_settings::Patch,
            editable: bool,
            time: f64,
        }
        impl Harness {
            fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
                self.time += 0.02;
                self.context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::splat(900.0),
                        )),
                        time: Some(self.time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        crate::settings_navigation::begin_frame(ctx);
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.add_enabled_ui(self.editable, |ui| {
                                mach3_motion_controls(
                                    ui,
                                    &mut self.value,
                                    RowContext {
                                        base: &RuntimeSettings::default(),
                                        previous: &RuntimeSettings::default(),
                                        scope: 1,
                                        overrides: Overrides::default(),
                                        cli: &self.cli,
                                    },
                                );
                            });
                        });
                    },
                )
            }
            fn text(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .collect()
            }
            fn focus(&mut self, row: &str, selected: &str, mode: InputMode) {
                let output = self.frame(Vec::new());
                let label = Self::text(&output, row)[0];
                let point = Self::text(&output, selected)
                    .into_iter()
                    .find(|rect| (rect.center().y - label.center().y).abs() < 3.0)
                    .unwrap()
                    .center();
                for pressed in [true, false] {
                    self.frame(vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                }
                set_mode(&self.context, mode);
            }
            fn next(&mut self) -> egui::FullOutput {
                self.frame(vec![egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    physical_key: Some(egui::Key::ArrowRight),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }])
            }
        }
        for mode in [InputMode::Keyboard, InputMode::Controller] {
            for (locked, editable) in [(false, true), (true, true), (true, false)] {
                let mut h = Harness {
                    context: egui::Context::default(),
                    value: RuntimeSettings::default(),
                    cli: if locked {
                        [(MOTION.into(), "original".into())].into()
                    } else {
                        Default::default()
                    },
                    editable,
                    time: 0.0,
                };
                style(&h.context);
                h.value.mach3.motion = MotionMode::Original;
                h.frame(Vec::new());
                h.focus("Player craft movement", "Original", mode);
                let output = h.next();
                if locked {
                    assert_eq!(h.value.mach3.motion, MotionMode::Original);
                    assert_eq!(output.shapes.iter().filter(|shape|
                        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == CLI_LOCK)
                    ).count(), 2, "both rows retain CLI lock markers when inherited");
                    h.focus("Scene movement", "Original", mode);
                    h.next();
                    assert_eq!(h.value.mach3.motion, MotionMode::Original);
                    continue;
                }
                assert_eq!(h.value.mach3.motion, MotionMode::Enhanced);
                h.next();
                assert_eq!(h.value.mach3.motion, MotionMode::Original);
                h.focus("Scene movement", "Original", mode);
                let output = h.next();
                assert_eq!(h.value.mach3.motion, MotionMode::Reforged);
                assert!(Harness::text(&output, "Player craft movement").is_empty());
                h.next();
                assert_eq!(
                    h.value.mach3.motion,
                    MotionMode::Original,
                    "restore the legacy craft choice"
                );
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
                            RowContext {
                                base: &value,
                                previous: &value,
                                scope: 0,
                                overrides: Overrides::default(),
                                cli: &Default::default(),
                            }
                            .state("backend", &value),
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
