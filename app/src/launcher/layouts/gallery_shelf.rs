// Copyright (c) 2026 analogued. Project license has not yet been selected.
// Gallery Shelf layout with the shared theme selector.
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

    egui::TopBottomPanel::top("gallery_top")
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

    egui::TopBottomPanel::bottom("gallery_deck")
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
                            action = Some(UiAction::Locate(snapshot.id));
                        }
                    } else if primary_button(ui, ap, "Play", Vec2::new(104.0, 29.0), false)
                        .activated()
                    {
                        action = Some(UiAction::Play(snapshot.id));
                    }
                    if quiet_text_button(ui, ap, "Remove").activated() {
                        action = Some(UiAction::Remove(snapshot.id));
                    }
                    if !snapshot.status.is_missing()
                        && quiet_text_button(ui, ap, "Relocate…")
                            .on_hover_text("Choose the new location of this game’s original files")
                            .activated()
                    {
                        action = Some(UiAction::Locate(snapshot.id));
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
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(
                            RichText::new(&snapshot.title)
                                .size(15.0)
                                .strong()
                                .color(ap.tokens.text_primary),
                        );
                        draw_provenance_inline(ui, ap, &snapshot);
                    });
                });
            });
        });

    if state.options_open
        && let Some(idx) = state.selected_index()
    {
        draw_options_panel(ctx, ap, &mut state.games[idx], "gallery_deck_options");
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.canvas)
                .inner_margin(egui::Margin::symmetric(18, 12)),
        )
        .show(ctx, |ui| {
            state.content_bottom = ui.max_rect().bottom();
            let avail_h = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("gallery_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let grid = GalleryGrid::fit(state.games.len(), Vec2::new(avail_w, avail_h));
                    let left_pad = ((avail_w - grid.size().x) * 0.5).max(0.0);
                    let top_pad = ((avail_h - grid.size().y) * 0.5).max(0.0);
                    ui.add_space(top_pad);
                    ui.spacing_mut().item_spacing.y = GALLERY_GAP;

                    let selected_id = state.selected;
                    let mut next_selected = selected_id;

                    for chunk in state.games.chunks(grid.columns) {
                        ui.horizontal(|ui| {
                            if left_pad > 0.0 {
                                ui.add_space(left_pad);
                            }
                            ui.spacing_mut().item_spacing.x = GALLERY_GAP;
                            for game in chunk {
                                let is_sel = selected_id == Some(game.id);
                                let resp = draw_game_card(ui, ap, game, grid.card_width, is_sel);
                                if resp.activated() || resp.has_focus() {
                                    next_selected = Some(game.id);
                                }
                            }
                        });
                    }
                    state.selected = next_selected;
                });
        });

    (action, anchor)
}

const GALLERY_GAP: f32 = 16.0;
const MAX_CARD_WIDTH: f32 = 292.0;
const MIN_CARD_WIDTH: f32 = 144.0;

struct GalleryGrid {
    columns: usize,
    rows: usize,
    card_width: f32,
}
impl GalleryGrid {
    fn size(&self) -> Vec2 {
        Vec2::new(
            self.columns as f32 * self.card_width
                + self.columns.saturating_sub(1) as f32 * GALLERY_GAP,
            self.rows as f32 * card_height(self.card_width)
                + self.rows.saturating_sub(1) as f32 * GALLERY_GAP,
        )
    }
    fn fit(count: usize, available: Vec2) -> Self {
        let count = count.max(1);
        let min_width = MIN_CARD_WIDTH.min(available.x).max(1.0);
        let max_columns = (((available.x + GALLERY_GAP) / (min_width + GALLERY_GAP)) as usize)
            .max(1)
            .min(count);
        let width_for_columns = |cols| {
            ((available.x - (cols - 1) as f32 * GALLERY_GAP) / cols as f32)
                .clamp(1.0, MAX_CARD_WIDTH)
        };
        let mut best: Option<Self> = None;
        for columns in 1..=max_columns {
            let rows = count.div_ceil(columns);
            let row_height =
                (available.y - rows.saturating_sub(1) as f32 * GALLERY_GAP) / rows as f32;
            let card_width = width_for_columns(columns)
                .min((row_height - 62.0) / 0.65 + 18.0)
                .floor();
            if card_width < min_width
                || rows as f32 * card_height(card_width)
                    + rows.saturating_sub(1) as f32 * GALLERY_GAP
                    > available.y
            {
                continue;
            }
            let candidate = Self {
                columns,
                rows,
                card_width,
            };
            if best.as_ref().is_none_or(|old| {
                card_width > old.card_width
                    || (card_width == old.card_width
                        && (columns * rows - count, rows)
                            < (old.columns * old.rows - count, old.rows))
            }) {
                best = Some(candidate);
            }
        }
        // Large libraries scroll instead of shrinking cards below the readable
        // minimum. The same cap and centring apply to every library size.
        best.unwrap_or_else(|| {
            let columns = (((available.x + GALLERY_GAP) / (MAX_CARD_WIDTH + GALLERY_GAP)) as usize)
                .max(1)
                .min(count);
            Self {
                columns,
                rows: count.div_ceil(columns),
                card_width: width_for_columns(columns),
            }
        })
    }
}

fn card_height(card_w: f32) -> f32 {
    let pad = 9.0;
    let screen_w = (card_w - pad * 2.0).max(96.0);
    let screen_h = (screen_w * 0.65).round();
    pad + screen_h + 7.0 + 46.0
}

fn draw_game_card(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &GameEntry,
    card_w: f32,
    selected: bool,
) -> egui::Response {
    let pad = 9.0;
    let screen_w = (card_w - pad * 2.0).max(96.0);
    let screen_h = (screen_w * 0.65).round();
    let card_h = card_height(card_w);

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click_and_drag());
    let response = Gui::control(response);

    ap.paint_game_frame(ui, &response, selected, ap.tokens.card_radius);

    let screen_rect = Rect::from_min_size(
        Pos2::new(rect.left() + pad, rect.top() + pad),
        Vec2::new(screen_w, screen_h),
    );
    paint_preview_in_rect(ui.painter(), ap, screen_rect, game, true);

    let text_top = screen_rect.bottom() + 7.0;
    let (_, bar_title, bar_sub) = ap.selected_row_colors();
    let title_col = if selected {
        bar_title
    } else {
        ap.tokens.text_primary
    };
    let sub_col = if game.status.is_missing() {
        ap.missing.foreground
    } else if selected {
        bar_sub
    } else {
        ap.tokens.text_secondary
    };
    let (title_col, sub_col) = ap.game_text_colours(&response, selected, title_col, sub_col);
    let sub_col = if game.status.is_missing() {
        ap.missing.foreground
    } else {
        sub_col
    };

    ui.painter().text(
        Pos2::new(rect.left() + pad, text_top),
        egui::Align2::LEFT_TOP,
        &game.title,
        FontId::proportional(14.5),
        title_col,
    );

    let sub = if game.status.is_missing() {
        "Missing files".to_string()
    } else if let Some(note) = game.edition_note {
        format!("{} · {note}", game.year)
    } else {
        format!("{} · {}", game.year, game.developer)
    };
    let sub_rect = ui.painter().text(
        Pos2::new(rect.left() + pad, text_top + 19.0),
        egui::Align2::LEFT_TOP,
        sub,
        FontId::proportional(11.5),
        sub_col,
    );

    if game.status.is_missing() {
        ui.painter().circle_filled(
            Pos2::new(rect.right() - pad - 3.2, sub_rect.center().y),
            3.2,
            ap.missing.foreground,
        );
    }

    let swatches = game.palette_swatches();
    let dot_size = 6.0;
    let dots_w = swatches.len() as f32 * dot_size + (swatches.len().saturating_sub(1)) as f32 * 3.0;
    paint_palette_dots_at(
        ui.painter(),
        ap,
        Pos2::new(rect.right() - pad - dots_w, text_top + 4.5),
        swatches,
        dot_size,
    );

    response
}
