// Copyright (c) 2026 analogued. Project license has not yet been selected.
// Shared launcher UI helpers routed through `ResolvedAppearance` and `reforge_sdl::ui::primitives`.
use reforge_sdl::ui::ButtonActivation;

use reforge_sdl::{
    egui::{
        self, Align, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke,
        StrokeKind, Vec2,
    },
    settings::Gui,
    ui::{SelectionState, primitives},
};

use super::{
    catalog::{ActionRole, PaletteShape, ResolvedAppearance},
    model::{EntryStatus, GameEntry},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Play(i64),
    Add,
    Locate(i64),
    Remove(i64),
    OpenSettings,
}

#[derive(Clone, Copy)]
pub enum ChevronDir {
    Up,
    Down,
}

pub fn paint_chevron(painter: &egui::Painter, center: Pos2, dir: ChevronDir, color: Color32) {
    let pts = match dir {
        ChevronDir::Up => vec![
            center + Vec2::new(-3.5, 1.8),
            center + Vec2::new(0.0, -1.8),
            center + Vec2::new(3.5, 1.8),
        ],
        ChevronDir::Down => vec![
            center + Vec2::new(-3.5, -1.8),
            center + Vec2::new(0.0, 1.8),
            center + Vec2::new(3.5, -1.8),
        ],
    };
    painter.add(egui::Shape::line(pts, Stroke::new(1.3_f32, color)));
}

pub fn section_rule(ui: &mut egui::Ui, ap: &ResolvedAppearance) {
    ui.add_space(6.0);
    primitives::separator(ui, &ap.theme);
    ui.add_space(6.0);
}

pub fn primary_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    label: &str,
    min_size: Vec2,
    warning: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_at_least(min_size, Sense::click_and_drag());
    let response = Gui::control(response);
    ap.paint_action_frame(
        ui,
        &response,
        if warning {
            ActionRole::Warning
        } else {
            ActionRole::Primary
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    response
}

pub fn secondary_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    label: &str,
    min_size: Vec2,
) -> egui::Response {
    themed_action_button(
        ui,
        ap,
        label,
        min_size,
        ActionRole::Standard,
        13.0,
        ap.tokens.text_primary,
    )
}

pub fn quiet_text_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    label: &str,
) -> egui::Response {
    themed_action_button(
        ui,
        ap,
        label,
        Vec2::ZERO,
        ActionRole::Quiet,
        12.5,
        ap.tokens.text_secondary,
    )
}

fn themed_action_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    label: &str,
    min_size: Vec2,
    role: ActionRole,
    font_size: f32,
    text_colour: Color32,
) -> egui::Response {
    // Keep egui's button sizing, identity and interaction; replace its frame
    // through the same theme policy used by custom-painted controls.
    let frame = ui.painter().add(egui::Shape::Noop);
    let response = Gui::control(
        ui.add(
            egui::Button::new(RichText::new(label).size(font_size).color(text_colour))
                .sense(Sense::click_and_drag())
                .min_size(min_size)
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE),
        ),
    );
    let v = ap.action_visuals(ui, &response, role);
    ui.painter().set(
        frame,
        egui::epaint::RectShape::new(
            response.rect.expand(v.expansion),
            v.corner_radius,
            v.weak_bg_fill,
            v.bg_stroke,
            StrokeKind::Inside,
        ),
    );
    response
}

pub fn options_toggle_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    label: &str,
    open: bool,
    dir: ChevronDir,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(12.5),
        ap.tokens.text_primary,
    );
    let pad_x = 8.5;
    let w = pad_x * 2.0 + galley.size().x + 14.0;
    let h = 26.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    let resp = Gui::control(resp);

    let activated = resp.activated();
    let shown_open = open ^ activated;
    ap.paint_action_frame(
        ui,
        &resp,
        if shown_open {
            ActionRole::Primary
        } else {
            ActionRole::Standard
        },
    );
    let fg = if shown_open {
        ap.theme.palette.selection.foreground
    } else {
        ap.tokens.text_primary
    };
    let text_galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), FontId::proportional(12.5), fg);
    ui.painter().galley(
        Pos2::new(
            rect.left() + pad_x,
            rect.center().y - text_galley.size().y * 0.5,
        ),
        text_galley,
        fg,
    );
    paint_chevron(
        ui.painter(),
        Pos2::new(rect.right() - pad_x - 3.5, rect.center().y + 0.5),
        if activated {
            match dir {
                ChevronDir::Up => ChevronDir::Down,
                ChevronDir::Down => ChevronDir::Up,
            }
        } else {
            dir
        },
        fg,
    );
    resp
}

pub fn draw_palette_dots(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    swatches: &[[u8; 3]],
    size: f32,
) {
    let count = swatches.len();
    let gap = 3.0;
    let total_w = count as f32 * size + (count.saturating_sub(1)) as f32 * gap;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, size), Sense::hover());
    paint_palette_dots_at(ui.painter(), ap, rect.min, swatches, size);
}

pub fn paint_palette_dots_at(
    painter: &egui::Painter,
    ap: &ResolvedAppearance,
    origin: Pos2,
    swatches: &[[u8; 3]],
    size: f32,
) {
    let gap = 3.0;
    let ring = if ap.theme.palette.dark {
        Color32::from_white_alpha(34)
    } else {
        Color32::from_gray(172)
    };
    for (i, [r, g, b]) in swatches.iter().enumerate() {
        let x = (origin.x + i as f32 * (size + gap)).round();
        let sq = Rect::from_min_size(Pos2::new(x, origin.y.round()), Vec2::splat(size.round()));
        match ap.tokens.palette_shape {
            PaletteShape::Square => {
                painter.rect_filled(sq, 0.0, Color32::from_rgb(*r, *g, *b));
                painter.rect_stroke(sq, 0.0, Stroke::new(1.0_f32, ring), StrokeKind::Inside);
            }
            PaletteShape::Circle => {
                painter.circle_filled(sq.center(), sq.width() * 0.5, Color32::from_rgb(*r, *g, *b));
                painter.circle_stroke(sq.center(), sq.width() * 0.5, Stroke::new(1.0_f32, ring));
            }
        }
    }
}

pub fn draw_preview_screen(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    size: Vec2,
    game: &GameEntry,
    show_badge: bool,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_preview_in_rect(ui.painter(), ap, rect, game, show_badge);
    rect
}

pub fn paint_preview_in_rect(
    painter: &egui::Painter,
    ap: &ResolvedAppearance,
    rect: Rect,
    game: &GameEntry,
    show_badge: bool,
) {
    let cr = CornerRadius::same(ap.tokens.preview_radius);
    let inner = rect.shrink(1.0);
    paint_symbolic(painter, ap, rect, game, show_badge);

    if game.status.is_missing() {
        painter.rect_filled(rect, cr, Color32::from_black_alpha(125));
        if inner.width() >= 88.0 && inner.height() >= 42.0 {
            let pill = Rect::from_center_size(
                Pos2::new(inner.center().x, inner.bottom() - 16.0),
                Vec2::new((inner.width() - 16.0).min(110.0), 21.0),
            );
            painter.rect_filled(
                pill,
                CornerRadius::same(ap.theme.components.button.radius.min(4)),
                ap.missing.preview_background,
            );
            painter.text(
                pill.center(),
                egui::Align2::CENTER_CENTER,
                "Missing files",
                FontId::proportional(11.0),
                ap.missing.preview_foreground,
            );
        }
    }

    let frame_col = if ap.theme.palette.dark {
        Color32::from_white_alpha(22)
    } else {
        Color32::from_black_alpha(38)
    };
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0_f32, frame_col),
        StrokeKind::Inside,
    );
}

fn option_label(ui: &mut egui::Ui, ap: &ResolvedAppearance, game: &GameEntry, field: &str) {
    let colour = if game.preferences.saved.contains_key(field) {
        ap.theme.palette.override_text
    } else {
        ap.tokens.text_secondary
    };
    ui.label(RichText::new(field).size(13.0).color(colour));
}
pub(crate) fn option_control(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &mut GameEntry,
    field: &'static str,
    choices: &[&'static str],
    short: bool,
) {
    let values = game.option_values();
    let current = values[field];
    let explicit = game.preferences.saved.contains_key(field);
    let options: Vec<_> = choices
        .iter()
        .map(|value| {
            (
                *value,
                if short {
                    game.short_label(field, value)
                } else {
                    *value
                },
            )
        })
        .collect();
    let state = if explicit {
        SelectionState::Explicit
    } else {
        SelectionState::Inherited
    };
    let clicked = if ap.id == super::catalog::ThemeId::ClassicDark {
        // Same native button and explicit corner override as production Settings.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut clicked = None;
            for (value, label) in &options {
                let selected = *value == current;
                let colour = if selected && !explicit {
                    ap.theme.palette.inherited.foreground
                } else if selected {
                    ap.theme.palette.selection.foreground
                } else {
                    ap.theme.palette.inactive.foreground
                };
                let frame = selected.then(|| ui.painter().add(egui::Shape::Noop));
                let mut button = egui::Button::new(RichText::new(*label).color(colour))
                    .sense(Sense::click_and_drag())
                    .selected(selected)
                    .corner_radius(3);
                if selected {
                    // Keep native sizing and activation, while drawing state
                    // feedback behind the text with fixed rounded geometry.
                    button = button.fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
                }
                let response = Gui::control(ui.add(button));
                if let Some(frame) = frame {
                    let p = ap.theme.palette;
                    let base = if explicit {
                        p.selection.background
                    } else {
                        p.inherited.background
                    };
                    let fill =
                        if response.is_pointer_button_down_on() && response.contains_pointer() {
                            base.lerp_to_gamma(Color32::BLACK, 0.12)
                        } else if response.hovered() {
                            base.lerp_to_gamma(Color32::WHITE, 0.08)
                        } else {
                            base
                        };
                    let border = if response.has_focus() {
                        p.focus
                    } else if explicit {
                        p.selection.foreground
                    } else {
                        p.inherited.border
                    };
                    ui.painter().set(
                        frame,
                        egui::epaint::RectShape::new(
                            response.rect,
                            3,
                            fill,
                            Stroke::new(1.0_f32, border),
                            StrokeKind::Inside,
                        ),
                    );
                }
                if response.activated() {
                    clicked = Some(*value);
                }
            }
            clicked
        })
        .inner
    } else {
        primitives::choice_strip(ui, &ap.theme, &options, &current, state)
    };
    if let Some(value) = clicked {
        game.save_option(field, value);
    }
    // The group tooltip explains inherited and explicit state without a legend.
    ui.response()
        .on_hover_text(game.preferences.tooltip(field, current));
}
pub fn draw_launch_options_rows(ui: &mut egui::Ui, ap: &ResolvedAppearance, game: &mut GameEntry) {
    primitives::surface(ui, &ap.theme, ap.theme.components.section, |ui| {
        if ap.id == super::catalog::ThemeId::SoftTonal {
            ui.set_min_width(ui.available_width());
        }
        ui.vertical(|ui| {
            for (field, choices) in game.option_choices() {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(82.0, 24.0),
                        Layout::left_to_right(Align::Center),
                        |ui| option_label(ui, ap, game, field),
                    );
                    option_control(ui, ap, game, field, &choices, false);
                });
            }
        });
    });
}
pub fn draw_launch_options_inline(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &mut GameEntry,
) {
    primitives::surface(ui, &ap.theme, ap.theme.components.section, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(18.0, 6.0);
            for (field, choices) in game.option_choices() {
                let font = FontId::proportional(14.0);
                let text_width = |text: &str| {
                    ui.fonts(|f| {
                        f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                            .size()
                            .x
                    })
                };
                let style = ap.theme.components.choices;
                let button_width: f32 = choices
                    .iter()
                    .map(|v| text_width(game.short_label(field, v)) + style.padding.x * 2.0)
                    .sum();
                let joined = matches!(
                    style.shape,
                    reforge_sdl::ui::ChoiceShape::Sharp
                        | reforge_sdl::ui::ChoiceShape::Segmented
                        | reforge_sdl::ui::ChoiceShape::Pill
                );
                let width = text_width(field)
                    + 6.0
                    + button_width
                    + style.gap * (choices.len().saturating_sub(1)) as f32
                    + if joined { 8.0 } else { 4.0 };
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 32.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        option_label(ui, ap, game, field);
                        option_control(ui, ap, game, field, &choices, true);
                    },
                );
            }
        });
    });
}

pub fn draw_options_panel(
    ctx: &egui::Context,
    ap: &ResolvedAppearance,
    game: &mut GameEntry,
    id: &str,
) {
    let width = (ctx.available_rect().width() - 36.0).max(1.0);
    let mut measuring = egui::Ui::new(
        ctx.clone(),
        egui::Id::new((id, "measure")),
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 10000.0)))
            .layout(Layout::top_down(Align::Min))
            // Keep normal layout constraints: egui's sizing pass discards
            // reserved row height, which underestimates the visible panel.
            .invisible(),
    );
    let mut copy = game.clone();
    draw_launch_options_inline(&mut measuring, ap, &mut copy);
    let height = measuring.min_rect().height().ceil() + 12.0;
    egui::TopBottomPanel::bottom(egui::Id::new(id))
        .exact_height(height)
        .resizable(false)
        .show_separator_line(true)
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.panel)
                .inner_margin(egui::Margin::symmetric(18, 6)),
        )
        .show(ctx, |ui| draw_launch_options_inline(ui, ap, game));
}

pub fn draw_provenance_inline(ui: &mut egui::Ui, ap: &ResolvedAppearance, game: &GameEntry) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
        let (status, colour, tooltip) = match &game.status {
            EntryStatus::Verified => ("Verified", ap.tokens.text_muted, game.original_path.clone()),
            EntryStatus::Missing(reason) => (
                "Missing",
                ap.missing.foreground,
                format!("{}\n{reason}", game.original_path),
            ),
        };
        let dot_colour = if game.status.is_missing() {
            ap.missing.foreground
        } else {
            ap.tokens.ok_fg
        };
        ui.painter().circle_filled(
            dot_rect.center(),
            if game.status.is_missing() { 3.2 } else { 3.0 },
            dot_colour,
        );
        // Label::ui automatically adds an elision tooltip. Lay out and paint the
        // same label directly so the complete path/details have one tooltip owner.
        let (position, galley, response) = egui::Label::new(
            RichText::new(format!("{status} · {}", game.original_path))
                .size(11.5)
                .color(colour),
        )
        .truncate()
        .layout_in_ui(ui);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), galley.text())
        });
        ui.painter().galley(position, galley, colour);
        response.on_hover_text(tooltip);
    });
}

pub fn draw_empty_onboarding(ui: &mut egui::Ui, ap: &ResolvedAppearance) -> Option<UiAction> {
    let mut action = None;
    let avail = ui.available_size();
    let card_w = (avail.x - 32.0).clamp(280.0, 490.0);

    ui.vertical_centered(|ui| {
        let top_pad = ((avail.y - 252.0) * 0.44).max(8.0);
        ui.add_space(top_pad);

        egui::Frame::NONE
            .fill(ap.theme.palette.group)
            .corner_radius(CornerRadius::same(ap.tokens.card_radius.max(2)))
            .stroke(Stroke::new(1.0_f32, ap.theme.palette.noninteractive.border))
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.set_max_width(card_w);
                ui.vertical_centered(|ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(46.0, 34.0), Sense::hover());
                    ui.painter().rect_filled(
                        icon_rect,
                        CornerRadius::same(ap.tokens.preview_radius.max(2)),
                        ap.theme.palette.selection.background.gamma_multiply(0.28),
                    );
                    ui.painter().rect_stroke(
                        icon_rect,
                        CornerRadius::same(ap.tokens.preview_radius.max(2)),
                        Stroke::new(1.5_f32, ap.theme.palette.accent),
                        StrokeKind::Inside,
                    );
                    ui.painter().circle_filled(
                        Pos2::new(icon_rect.right() - 6.0, icon_rect.bottom() - 5.0),
                        2.0,
                        ap.tokens.ok_fg,
                    );

                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Your library is empty")
                            .size(17.5)
                            .strong()
                            .color(ap.tokens.text_primary),
                    );
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Add a folder containing your original game files. REforge will identify supported games and add them to your library. Your original files stay unchanged.",
                            )
                            .size(12.5)
                            .color(ap.tokens.text_secondary),
                        )
                        .wrap(),
                    );
                    ui.add_space(14.0);

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if primary_button(
                            ui,
                            ap,
                            "Add Game Folder…",
                            Vec2::new(152.0, 30.0),
                            false,
                        )
                        .activated()
                        {
                            action = Some(UiAction::Add);
                        }
                        if secondary_button(ui, ap, "Settings…", Vec2::new(96.0, 30.0)).activated() {
                            action = Some(UiAction::OpenSettings);
                        }
                    });
                });
            });
    });
    action
}

fn paint_symbolic(
    painter: &egui::Painter,
    ap: &ResolvedAppearance,
    rect: Rect,
    game: &GameEntry,
    show_badge: bool,
) {
    let painter = &painter.with_clip_rect(rect);
    let colours = game.palette_swatches();
    painter.rect_filled(
        rect,
        ap.tokens.preview_radius,
        Color32::from_rgb(16, 19, 24),
    );
    // Continue the strip beneath the border, using its same outer contour.
    // The border is painted last, so its translucent inner edge blends with
    // the palette instead of an inset dark gap. Retain 3.5 points inside it.
    let strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 4.5));
    if !colours.is_empty() {
        let width = strip.width() / colours.len() as f32;
        for (i, [r, g, b]) in colours.iter().enumerate() {
            let segment = Rect::from_min_size(
                Pos2::new(strip.left() + i as f32 * width, strip.top()),
                Vec2::new(width, strip.height()),
            );
            painter.with_clip_rect(segment).rect_filled(
                rect,
                ap.tokens.preview_radius,
                Color32::from_rgb(*r, *g, *b),
            );
        }
    }
    let rect = rect.shrink(1.0);
    let accent = colours
        .iter()
        .skip(1)
        .find(|c| c.iter().copied().max().unwrap_or(0) >= 160)
        .map_or(Color32::from_gray(220), |[r, g, b]| {
            Color32::from_rgb(*r, *g, *b)
        });
    let text = super::symbolic::initials(&game.title);
    painter.text(
        rect.center()
            - Vec2::new(
                0.0,
                if show_badge && rect.height() > 64.0 {
                    7.0
                } else {
                    0.0
                },
            ),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional((rect.height() * 0.28).clamp(13.0, 28.0)),
        accent,
    );
    if show_badge && rect.height() > 64.0 && !game.status.is_missing() {
        painter.text(
            Pos2::new(rect.center().x, rect.center().y + 14.0),
            egui::Align2::CENTER_CENTER,
            game.full_mode_label(),
            FontId::proportional(10.5),
            Color32::from_gray(165),
        );
    }
}
