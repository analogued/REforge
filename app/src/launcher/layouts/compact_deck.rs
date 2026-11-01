// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Compact Deck layout with the shared theme selector.
use reforge_sdl::ui::ButtonActivation;
use reforge_sdl::{
    egui::{self, Align, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2},
    settings::Gui,
};

use super::super::{
    catalog::ResolvedAppearance,
    model::{GameEntry, State},
    switchers::{self, AnchorPlacement},
    widgets::{
        ChevronDir, UiAction, draw_options_panel, draw_provenance_inline, options_toggle_button,
        paint_palette_dots_at, paint_preview_in_rect, primary_button, quiet_text_button,
        secondary_button,
    },
};

pub fn draw(
    ctx: &egui::Context,
    state: &mut State,
    ap: &ResolvedAppearance,
) -> (Option<UiAction>, Option<AnchorPlacement>) {
    let mut action = None;
    let mut anchor = None;

    egui::TopBottomPanel::top("compact_top")
        .show_separator_line(true)
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.panel)
                .inner_margin(egui::Margin::symmetric(18, 9)),
        )
        .show(ctx, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 25.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(
                        RichText::new("REforge")
                            .size(17.5)
                            .strong()
                            .color(ap.tokens.text_primary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if secondary_button(ui, ap, "Add…", Vec2::new(66.0, 25.0)).activated() {
                            action = Some(UiAction::Add);
                        }
                        if secondary_button(ui, ap, "Settings…", Vec2::new(84.0, 25.0)).activated()
                        {
                            action = Some(UiAction::OpenSettings);
                        }
                        anchor = switchers::draw_header_theme_control(ui, state, ap);
                    });
                },
            );
        });

    egui::TopBottomPanel::bottom("compact_strip")
        .exact_height(47.0)
        .resizable(false)
        .show_separator_line(true)
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.panel)
                .inner_margin(egui::Margin::symmetric(18, 9)),
        )
        .show(ctx, |ui| {
            let Some(idx) = state.selected_index() else {
                return;
            };

            let snapshot = state.games[idx].clone();
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if snapshot.status.is_missing() {
                        if primary_button(ui, ap, "Locate Files…", Vec2::new(122.0, 29.0), true)
                            .activated()
                        {
                            action = Some(UiAction::Locate(snapshot.entry_id.clone()));
                        }
                    } else if primary_button(ui, ap, "Play", Vec2::new(104.0, 29.0), false)
                        .activated()
                    {
                        action = Some(UiAction::Play(snapshot.entry_id.clone()));
                    }
                    if quiet_text_button(ui, ap, "Remove")
                        .on_hover_text("Remove from library; keep original game files")
                        .activated()
                    {
                        action = Some(UiAction::Remove(snapshot.entry_id.clone()));
                    }
                    if !snapshot.status.is_missing()
                        && quiet_text_button(ui, ap, "Relocate…")
                            .on_hover_text("Choose the new location of this game’s original files")
                            .activated()
                    {
                        action = Some(UiAction::Locate(snapshot.entry_id.clone()));
                    }
                    let chev = if state.options_open {
                        ChevronDir::Down
                    } else {
                        ChevronDir::Up
                    };
                    if options_toggle_button(ui, ap, "Options", state.options_open, chev)
                        .on_hover_text("Show or hide per-game launch options")
                        .activated()
                    {
                        state.options_open = !state.options_open;
                    }

                    ui.add_space(6.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        draw_provenance_inline(ui, ap, &snapshot);
                    });
                });
            });
        });

    if state.options_open
        && let Some(idx) = state.selected_index()
    {
        draw_options_panel(ctx, ap, &mut state.games[idx], "compact_strip_options");
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.canvas)
                .inner_margin(egui::Margin::symmetric(18, 12)),
        )
        .show(ctx, |ui| {
            state.content_bottom = ui.max_rect().bottom();
            let selected_id = state.selected.clone();
            let mut next_selected = selected_id.clone();

            egui::ScrollArea::vertical()
                .id_salt("compact_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 5.0;
                    for game in &state.games {
                        let is_sel = selected_id.as_deref() == Some(game.entry_id.as_str());
                        let resp = draw_deck_row(ui, ap, game, is_sel);
                        if resp.activated() || resp.has_focus() {
                            next_selected = Some(game.entry_id.clone());
                        }
                    }
                });

            state.selected = next_selected;
        });

    (action, anchor)
}

fn draw_deck_row(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &GameEntry,
    selected: bool,
) -> egui::Response {
    let row_h = 52.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_h),
        Sense::click_and_drag(),
    );
    let response = Gui::control(response);

    let (_, sel_title, sel_sub) = ap.selected_row_colors();
    ap.paint_game_frame(ui, &response, selected, ap.tokens.row_radius);

    let thumb_left = rect.left() + if ap.uses_left_bar() { 10.0 } else { 8.0 };
    let thumb_rect = Rect::from_min_size(
        Pos2::new(thumb_left, rect.top() + 5.0),
        Vec2::new(56.0, 42.0),
    );
    paint_preview_in_rect(ui.painter(), ap, thumb_rect, game, false);

    let title_col = if selected {
        sel_title
    } else {
        ap.tokens.text_primary
    };
    let sub_col = if selected {
        sel_sub
    } else {
        ap.tokens.text_secondary
    };
    let (title_col, sub_col) = ap.game_text_colours(&response, selected, title_col, sub_col);

    let text_x = thumb_rect.right() + 12.0;
    ui.painter().text(
        Pos2::new(text_x, rect.top() + 7.5),
        egui::Align2::LEFT_TOP,
        &game.title,
        FontId::proportional(14.5),
        title_col,
    );

    let mut meta = game.detail_line();
    if let Some(note) = game.edition_label() {
        if !meta.is_empty() {
            meta.push_str(" · ");
        }
        meta.push_str(note);
    }
    let max_meta_x = if rect.width() >= 420.0 {
        rect.right() - 165.0
    } else {
        rect.right() - 10.0
    };
    let meta_galley = ui
        .painter()
        .layout_no_wrap(meta, FontId::proportional(11.5), sub_col);
    let clipped = ui.painter().with_clip_rect(Rect::from_min_max(
        Pos2::new(text_x, rect.top()),
        Pos2::new(max_meta_x.max(text_x), rect.bottom()),
    ));
    clipped.galley(Pos2::new(text_x, rect.top() + 28.0), meta_galley, sub_col);

    if rect.width() >= 420.0 {
        let (label_str, label_col) = if game.status.is_missing() {
            ("Missing files".to_string(), ap.missing.foreground)
        } else {
            (game.full_mode_label(), sub_col)
        };
        let (_, label_col) = ap.game_text_colours(&response, selected, label_col, label_col);
        let label_col = if game.status.is_missing() {
            ap.missing.foreground
        } else {
            label_col
        };
        let galley = ui
            .painter()
            .layout_no_wrap(label_str, FontId::proportional(11.5), label_col);
        let label_pos = Pos2::new(
            rect.right()
                - galley.size().x
                - 14.0
                - if game.status.is_missing() { 13.4 } else { 0.0 },
            rect.center().y - galley.size().y * 0.5,
        );
        let dot_y = label_pos.y + galley.size().y * 0.5;
        ui.painter().galley(label_pos, galley, label_col);
        if game.status.is_missing() {
            ui.painter().circle_filled(
                Pos2::new(rect.right() - 14.0 - 3.2, dot_y),
                3.2,
                ap.missing.foreground,
            );
        }

        if !game.status.is_missing() && rect.width() >= 500.0 {
            let swatches = game.palette_swatches();
            let dot_size = 6.0;
            let dots_w =
                swatches.len() as f32 * dot_size + (swatches.len().saturating_sub(1)) as f32 * 3.0;
            paint_palette_dots_at(
                ui.painter(),
                ap,
                Pos2::new(
                    label_pos.x - dots_w - 10.0,
                    rect.center().y - dot_size * 0.5,
                ),
                swatches,
                dot_size,
            );
        }
    }

    response
}
