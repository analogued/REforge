// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Theme preview, header actions and removal confirmation.
use reforge_sdl::ui::ButtonActivation;

use reforge_sdl::{
    egui::{
        self, Align, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke,
        Vec2,
    },
    settings::Gui,
    ui::primitives,
};

use super::{
    catalog::{ActionRole, ResolvedAppearance, ThemeId, paint_theme_swatch},
    model::State,
    widgets::{UiAction, primary_button, quiet_text_button, secondary_button},
};

#[derive(Clone, Copy, Debug)]
pub enum AnchorPlacement {
    BelowRight(Rect),
    AboveSidebar { trigger: Rect, sidebar_inner_w: f32 },
}

pub fn draw_header_theme_control(
    ui: &mut egui::Ui,
    state: &mut State,
    ap: &ResolvedAppearance,
) -> Option<AnchorPlacement> {
    let id = state.effective_theme();
    let (response, activated) = theme_swatch_button(ui, ap, id, state.preview_open);
    let anchor = AnchorPlacement::BelowRight(response.rect);
    if activated {
        state.toggle_preview();
    }
    Some(anchor)
}

pub fn draw_sidebar_footer(
    ui: &mut egui::Ui,
    state: &mut State,
    ap: &ResolvedAppearance,
    _compact: bool,
) -> (Option<UiAction>, Option<AnchorPlacement>) {
    let mut action = None;
    let width = ui.available_width();
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let settings_width = (width - 34.0 - 6.0).max(1.0);
        if secondary_button(ui, ap, "Settings…", Vec2::new(settings_width, 26.0)).activated() {
            action = Some(UiAction::OpenSettings);
        }
        let id = state.effective_theme();
        let (_, activated) = theme_swatch_button(ui, ap, id, state.preview_open);
        if activated {
            state.toggle_preview();
        }
    });
    (
        action,
        Some(AnchorPlacement::AboveSidebar {
            trigger: row.response.rect,
            sidebar_inner_w: width,
        }),
    )
}

fn draw_vertical_theme_list(
    ui: &mut egui::Ui,
    state: &mut State,
    ap: &ResolvedAppearance,
    row_h: f32,
) {
    ui.spacing_mut().item_spacing.y = 3.0;
    for id in ThemeId::ALL {
        let selected = state.preview_draft == id;
        let (row_rect, resp) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), row_h),
            Sense::click_and_drag(),
        );
        let resp = Gui::control(resp);
        let (sel_bg, sel_fg, _) = ap.selected_row_colors();
        let row_bg = if selected {
            sel_bg
        } else if resp.hovered() || resp.has_focus() {
            ap.tokens.item_hover
        } else {
            Color32::TRANSPARENT
        };
        let r_cr = CornerRadius::same(ap.tokens.row_radius.max(2));
        if row_bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, r_cr, row_bg);
        }
        if selected && ap.uses_left_bar() {
            ap.paint_left_bar(ui.painter(), row_rect, ap.tokens.row_radius.max(2));
        }

        if resp.has_focus() {
            ui.painter().rect_stroke(
                row_rect,
                r_cr,
                Stroke::new(1.0_f32, ap.theme.palette.focus),
                egui::StrokeKind::Inside,
            );
        }
        let sw = Rect::from_min_size(
            Pos2::new(row_rect.left() + 7.0, (row_rect.center().y - 6.5).round()),
            Vec2::new(19.0, 13.0),
        );
        paint_theme_swatch(
            ui.painter(),
            sw,
            id,
            selected.then_some(ap.theme.palette.accent),
        );

        let title_col = if selected {
            sel_fg
        } else {
            ap.tokens.text_primary
        };
        let max_text_x = row_rect.right() - 6.0;
        let galley = ui.painter().layout_no_wrap(
            id.label().to_string(),
            FontId::proportional(12.0),
            title_col,
        );
        let text_x = sw.right() + 7.0;
        let clipped = ui.painter().with_clip_rect(Rect::from_min_max(
            Pos2::new(text_x, row_rect.top()),
            Pos2::new(max_text_x, row_rect.bottom()),
        ));
        clipped.galley(
            Pos2::new(text_x, row_rect.center().y - galley.size().y * 0.5),
            galley,
            title_col,
        );

        if state.preview_focus_pending && selected {
            resp.request_focus();
        }
        if resp.activated() {
            state.preview_draft = id;
        }
    }
    state.preview_focus_pending = false;
}

/// Minimal trigger. The catalog provides its swatch; appearance is not encoded here.
pub(crate) fn theme_swatch_button(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    theme_id: ThemeId,
    open: bool,
) -> (egui::Response, bool) {
    // Retain pointer capture after egui classifies an in-button movement as
    // a drag; release inside still activates this disclosure control.
    let (rect, response) = ui.allocate_exact_size(Vec2::new(34.0, 26.0), Sense::click_and_drag());
    let response = Gui::control(response);
    let activated = response.activated();
    // Paint the resulting state in the activation frame, before the caller
    // updates the popup, avoiding a one-frame neutral flash on release.
    let shown_open = open ^ activated;
    ap.paint_action_frame(
        ui,
        &response,
        if shown_open {
            ActionRole::Primary
        } else {
            ActionRole::Standard
        },
    );
    let swatch = Rect::from_center_size(rect.center(), Vec2::new(22.0, 15.0));
    paint_theme_swatch(ui.painter(), swatch, theme_id, None);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Choose theme")
    });
    let response = response.on_hover_text(if shown_open {
        format!("Keep {} and close (Ctrl+T)", theme_id.label())
    } else {
        format!("Choose theme · {} (Ctrl+T)", theme_id.label())
    });
    (response, activated)
}

pub fn draw_overlays(
    ctx: &egui::Context,
    state: &mut State,
    ap: &ResolvedAppearance,
    anchor: Option<AnchorPlacement>,
) {
    if state.preview_open
        && let Some(placement) = anchor
    {
        draw_theme_popover(ctx, state, ap, placement);
    }
}

fn draw_theme_popover(
    ctx: &egui::Context,
    state: &mut State,
    ap: &ResolvedAppearance,
    placement: AnchorPlacement,
) {
    let screen = ctx.screen_rect();
    let (pivot, pos, pop_w, trigger_rect) = match placement {
        AnchorPlacement::BelowRight(r) => {
            let w = 256.0;
            let x = r.right().clamp(w + 8.0, screen.right() - 8.0);
            let y = r.bottom() + 6.0;
            (egui::Align2::RIGHT_TOP, Pos2::new(x, y), w, r)
        }
        AnchorPlacement::AboveSidebar {
            trigger,
            sidebar_inner_w,
        } => {
            // Keep the popover inside the sidebar, aligned with its footer.
            let w = sidebar_inner_w.min(224.0);
            let x = trigger.left();
            let y = trigger.top() - 6.0;
            (egui::Align2::LEFT_BOTTOM, Pos2::new(x, y), w, trigger)
        }
    };

    let area_resp = egui::Area::new(egui::Id::new("theme_selector"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let shadow = egui::epaint::Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(if ap.theme.palette.dark { 115 } else { 42 }),
            };
            egui::Frame::NONE
                .fill(ap.theme.palette.window)
                .stroke(Stroke::new(1.0_f32, ap.theme.palette.window_border))
                .corner_radius(CornerRadius::same(ap.tokens.card_radius.max(3)))
                .shadow(shadow)
                .inner_margin(egui::Margin::symmetric(9, 8))
                .show(ui, |ui| {
                    ui.set_width((pop_w - 18.0).max(120.0));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding.x = 4.0;
                        ui.label(
                            RichText::new("Theme")
                                .size(11.5)
                                .strong()
                                .color(ap.tokens.text_primary),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if primary_button(ui, ap, "Keep", Vec2::new(46.0, 21.0), false)
                                .on_hover_text("Save this theme and close")
                                .activated()
                            {
                                state.commit_preview();
                            }
                            if quiet_text_button(ui, ap, "Revert")
                                .on_hover_text("Restore the saved theme and close")
                                .activated()
                            {
                                state.revert_preview();
                            }
                        });
                    });
                    ui.add_space(3.0);
                    primitives::separator(ui, &ap.theme);
                    ui.add_space(4.0);
                    draw_vertical_theme_list(ui, state, ap, 28.0);
                });
        });

    let pop_rect = area_resp.response.rect;
    let outside = ctx.input(|i| {
        i.pointer
            .primary_clicked()
            .then_some(i.pointer.interact_pos())
            .flatten()
    });
    if let Some(p) = outside
        && !pop_rect.contains(p)
        && !trigger_rect.contains(p)
    {
        state.commit_preview();
    }
}

pub fn draw_remove_confirmation(ctx: &egui::Context, state: &mut State, ap: &ResolvedAppearance) {
    let Some(id) = state.pending_remove else {
        return;
    };
    let Some(game) = state.games.iter().find(|game| game.id == id) else {
        state.pending_remove = None;
        return;
    };
    let title = game.title.clone();
    let mut confirm = false;
    let mut cancel = false;
    let response = egui::Modal::new(egui::Id::new("remove_game_confirmation"))
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.window)
                .stroke(Stroke::new(1.0_f32, ap.theme.palette.window_border))
                .corner_radius(CornerRadius::same(ap.tokens.card_radius))
                .inner_margin(18.0),
        )
        .show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 72.0).clamp(240.0, 360.0));
            ui.label(
                RichText::new(format!("Remove {title} from your library?"))
                    .strong()
                    .size(16.0),
            );
            ui.add_space(8.0);
            ui.label("Your original game files will stay on disk.");
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                confirm =
                    primary_button(ui, ap, "Remove game", Vec2::new(106.0, 28.0), true).activated();
                let response = secondary_button(ui, ap, "Cancel", Vec2::new(82.0, 28.0));
                // Default keyboard activation cancels; deletion is deliberate.
                if state.remove_focus_pending {
                    response.request_focus();
                }
                cancel = response.activated();
            });
        });
    state.remove_focus_pending = false;
    if confirm {
        state.confirm_remove();
    } else if cancel || response.should_close() {
        state.pending_remove = None;
    }
}
