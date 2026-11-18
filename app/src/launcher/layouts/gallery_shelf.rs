// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Gallery Shelf layout with the shared theme selector.
use std::collections::BTreeMap;

use reforge_sdl::ui::ButtonActivation;
use reforge_sdl::{
    egui::{self, Align, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2},
    settings::Gui,
};
use reforge_settings::launcher::GalleryPreviewAspect;

use super::super::{
    catalog::ResolvedAppearance,
    model::{GameEntry, State},
    switchers::{self, AnchorPlacement},
    widgets::{
        ChevronDir, UiAction, draw_options_panel, draw_provenance_inline, options_toggle_button,
        paint_palette_at, paint_preview_in_rect, paint_title, palette_size, primary_button,
        quiet_text_button, secondary_button,
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
                            action = Some(UiAction::Locate(snapshot.entry_id.clone()));
                        }
                    } else if primary_button(ui, ap, "Play", Vec2::new(104.0, 29.0), false)
                        .activated()
                    {
                        action = Some(UiAction::Play(snapshot.entry_id.clone()));
                    }
                    if quiet_text_button(ui, ap, "Remove").activated() {
                        action = Some(UiAction::Remove(snapshot.entry_id.clone()));
                    }
                    if quiet_text_button(ui, ap, "Relocate…")
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
                        ui.spacing_mut().item_spacing.x = 8.0;
                        // Keep room for the status marker/word before allowing
                        // the title and original path to consume the remainder.
                        let status = if snapshot.status.is_missing() {
                            "Missing"
                        } else {
                            "Verified"
                        };
                        let status_width = ui
                            .painter()
                            .layout_no_wrap(
                                format!("{status} · …"),
                                FontId::proportional(11.5),
                                ap.tokens.text_muted,
                            )
                            .size()
                            .x
                            + 13.0;
                        let font = FontId::proportional(15.0);
                        let natural = ui
                            .painter()
                            .layout_no_wrap(
                                snapshot.title.clone(),
                                font.clone(),
                                ap.tokens.text_primary,
                            )
                            .size();
                        let width = natural.x.min(
                            (ui.available_width() - status_width - ui.spacing().item_spacing.x)
                                .max(0.0),
                        );
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, natural.y), Sense::hover());
                        paint_title(
                            ui.painter(),
                            rect.min,
                            width,
                            &snapshot.title,
                            font,
                            ap.tokens.text_primary,
                        );
                        draw_provenance_inline(ui, ap, &snapshot);
                    });
                });
            });
        });

    // The options drawer changes the visible viewport, not the gallery's layout
    // reference. Keep card sizes, column count and centring stable as it toggles.
    let gallery_frame = egui::Frame::NONE
        .fill(ap.theme.palette.canvas)
        .inner_margin(egui::Margin::symmetric(18, 12));
    let gallery_height =
        (ctx.available_rect().height() - gallery_frame.inner_margin.sum().y).max(0.0);

    if state.options_open
        && let Some(idx) = state.selected_index()
    {
        draw_options_panel(ctx, ap, &mut state.games[idx], "gallery_deck_options");
    }

    egui::CentralPanel::default()
        .frame(gallery_frame)
        .show(ctx, |ui| {
            state.content_bottom = ui.max_rect().bottom();
            egui::ScrollArea::vertical()
                .id_salt("gallery_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let grid =
                        GalleryGrid::fit(state.games.len(), Vec2::new(avail_w, gallery_height));
                    let aspects = row_aspects(
                        state.games.iter().map(preview_aspect),
                        grid.columns,
                        state.gallery_preview_aspect,
                    );
                    let height = gallery_height_for(grid.card_width, aspects.iter().copied());
                    let left_pad = ((avail_w - grid.width()) * 0.5).max(0.0);
                    let top_pad = ((gallery_height - height) * 0.5).max(0.0);
                    ui.add_space(top_pad);
                    ui.spacing_mut().item_spacing.y = GALLERY_GAP;

                    let selected_id = state.selected.clone();
                    let mut next_selected = selected_id.clone();

                    for (chunk, aspect) in state.games.chunks(grid.columns).zip(aspects) {
                        ui.horizontal(|ui| {
                            if left_pad > 0.0 {
                                ui.add_space(left_pad);
                            }
                            ui.spacing_mut().item_spacing.x = GALLERY_GAP;
                            for game in chunk {
                                let is_sel = selected_id.as_deref() == Some(game.entry_id.as_str());
                                let resp =
                                    draw_game_card(ui, ap, game, grid.card_width, aspect, is_sel);
                                if resp.activated() || resp.has_focus() {
                                    next_selected = Some(game.entry_id.clone());
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
const CARD_PADDING: f32 = 9.0;
const CARD_CHROME_HEIGHT: f32 = CARD_PADDING + 7.0 + 46.0;
type Aspect = (u32, u32);
const FOUR_THREE: Aspect = (4, 3);

fn preview_aspect(game: &GameEntry) -> Aspect {
    game.previews
        .get(game.preview_variant())
        .map(|source| {
            source
                .frame
                .geometry()
                .ratio(reforge_sdl::AspectRatio::Original)
        })
        .unwrap_or(FOUR_THREE)
}

fn most_common_aspect(aspects: impl IntoIterator<Item = Aspect>) -> Aspect {
    let mut votes = BTreeMap::new();
    for aspect in aspects {
        *votes.entry(aspect).or_insert(0usize) += 1;
    }
    let most_votes = votes.values().copied().max().unwrap_or(0);
    let mut leaders = votes.into_iter().filter(|&(_, count)| count == most_votes);
    match (leaders.next(), leaders.next()) {
        (Some((aspect, _)), None) => aspect,
        _ => FOUR_THREE,
    }
}

fn row_aspects(
    aspects: impl ExactSizeIterator<Item = Aspect>,
    columns: usize,
    policy: GalleryPreviewAspect,
) -> Vec<Aspect> {
    let rows = aspects.len().div_ceil(columns);
    match policy {
        GalleryPreviewAspect::FourThree => vec![FOUR_THREE; rows],
        GalleryPreviewAspect::WholeGallery => vec![most_common_aspect(aspects); rows],
        GalleryPreviewAspect::PerRow => aspects
            .collect::<Vec<_>>()
            .chunks(columns)
            .map(|row| most_common_aspect(row.iter().copied()))
            .collect(),
    }
}

fn screen_width(card_width: f32) -> f32 {
    (card_width - 2.0 * CARD_PADDING).max(1.0)
}

fn screen_height(card_width: f32, (width, height): Aspect) -> f32 {
    (screen_width(card_width) * (height as f32 / width as f32))
        .round()
        .max(1.0)
}

fn gallery_height_for(card_width: f32, aspects: impl ExactSizeIterator<Item = Aspect>) -> f32 {
    let gaps = aspects.len().saturating_sub(1) as f32 * GALLERY_GAP;
    aspects
        .map(|aspect| CARD_CHROME_HEIGHT + screen_height(card_width, aspect))
        .sum::<f32>()
        + gaps
}

struct GalleryGrid {
    columns: usize,
    rows: usize,
    card_width: f32,
}
impl GalleryGrid {
    fn width(&self) -> f32 {
        self.columns as f32 * self.card_width + self.columns.saturating_sub(1) as f32 * GALLERY_GAP
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
            // Fit the 4:3 reference so switching the aspect setting or loading
            // a wide preview only changes row heights. Search the actual
            // rounded dimensions so a rounding plateau cannot hide a fit.
            let mut low = min_width.ceil() as usize;
            let mut high = width_for_columns(columns).floor() as usize;
            let mut fitting_width = None;
            while low <= high {
                let width = low + (high - low) / 2;
                let height = rows as f32 * card_height(width as f32)
                    + rows.saturating_sub(1) as f32 * GALLERY_GAP;
                if height <= available.y {
                    fitting_width = Some(width as f32);
                    low = width + 1;
                } else {
                    high = width - 1;
                }
            }
            let Some(card_width) = fitting_width else {
                continue;
            };
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
        // Continue at the readable minimum when the last fitting layout starts
        // to overflow, rather than jumping back to maximum-width cards. Use the
        // fewest possible rows, then avoid unnecessary empty positions, matching
        // the fitting path's tie-break at the minimum-width boundary.
        best.unwrap_or_else(|| {
            let rows = count.div_ceil(max_columns);
            Self {
                columns: count.div_ceil(rows),
                rows,
                card_width: min_width,
            }
        })
    }
}

fn card_height(card_w: f32) -> f32 {
    CARD_CHROME_HEIGHT + screen_height(card_w, FOUR_THREE)
}

fn draw_game_card(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &GameEntry,
    card_w: f32,
    aspect: Aspect,
    selected: bool,
) -> egui::Response {
    let pad = CARD_PADDING;
    let screen_w = screen_width(card_w);
    let screen_h = screen_height(card_w, aspect);
    let card_h = CARD_CHROME_HEIGHT + screen_h;

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

    let palette = game.gallery_palette_display();
    let dot_size = 6.0;
    let size = palette_size(&palette, dot_size);
    let dots_w = size.x;
    let shares_metadata_line = palette.rows > 1;
    let title_right = rect.right() - pad - dots_w - 8.0;
    paint_title(
        ui.painter(),
        Pos2::new(rect.left() + pad, text_top),
        title_right - (rect.left() + pad),
        &game.title,
        FontId::proportional(14.5),
        title_col,
    );

    let sub = if game.status.is_missing() {
        "Missing files".to_string()
    } else {
        game.library_line()
    };
    let sub_position = Pos2::new(rect.left() + pad, text_top + 19.0);
    let sub_right = if shares_metadata_line {
        title_right
    } else {
        rect.right() - pad
    };
    let sub_rect = if shares_metadata_line {
        // Multi-row palettes also share the developer/status line. Keep long
        // labels and the missing-files marker out of its colour cells.
        let font = FontId::proportional(11.5);
        let height = ui
            .painter()
            .layout_no_wrap(sub.clone(), font.clone(), sub_col)
            .size()
            .y;
        let width =
            (sub_right - sub_position.x - if game.status.is_missing() { 12.0 } else { 0.0 })
                .max(0.0);
        paint_title(ui.painter(), sub_position, width, &sub, font, sub_col);
        Rect::from_min_size(sub_position, Vec2::new(width, height))
    } else {
        ui.painter().text(
            sub_position,
            egui::Align2::LEFT_TOP,
            sub,
            FontId::proportional(11.5),
            sub_col,
        )
    };

    if game.status.is_missing() {
        ui.painter().circle_filled(
            Pos2::new(sub_right - 3.2, sub_rect.center().y),
            3.2,
            ap.missing.foreground,
        );
    }

    paint_palette_at(
        ui.painter(),
        ap,
        // Thirteen rows occupy 38 points: the existing 46-point metadata
        // area leaves eight points below, absorbing the extra point in bottom
        // padding without changing card height or gallery grid fitting.
        Pos2::new(
            rect.right() - pad - dots_w,
            if shares_metadata_line {
                text_top
            } else {
                text_top + 7.5 - size.y * 0.5
            },
        ),
        &palette,
        dot_size,
    );

    response
}

#[cfg(test)]
mod tests;
