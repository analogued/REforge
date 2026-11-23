// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
// Split Inspector layout with the shared theme selector.
use reforge_sdl::ui::ButtonActivation;
use reforge_sdl::{
    egui::{self, Align, CornerRadius, FontId, Layout, Pos2, RichText, Sense, Vec2},
    settings::Gui,
    ui::primitives,
};

use super::super::{
    catalog::ResolvedAppearance,
    model::{EntryStatus, GameEntry, State},
    switchers::{self, AnchorPlacement},
    widgets::{
        UiAction, draw_launch_options_rows, draw_palette, draw_preview_screen,
        draw_provenance_inline, paint_palette_at, paint_title, primary_button, quiet_text_button,
        secondary_button, section_rule,
    },
};

pub fn draw(
    ctx: &egui::Context,
    state: &mut State,
    ap: &ResolvedAppearance,
) -> (Option<UiAction>, Option<AnchorPlacement>) {
    let mut action = None;
    let mut anchor = None;
    let screen_w = ctx.screen_rect().width();
    let compact = screen_w < 600.0;

    let sidebar_w = if compact { 172.0 } else { 218.0 };

    egui::SidePanel::left("split_sidebar")
        .resizable(false)
        .exact_width(sidebar_w)
        .show_separator_line(true)
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.panel)
                .inner_margin(egui::Margin::symmetric(12, 12)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("REforge")
                        .size(17.0)
                        .strong()
                        .color(ap.tokens.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if secondary_button(ui, ap, "Add…", Vec2::new(56.0, 24.0))
                        .on_hover_text("Add an original DOS game folder or executable")
                        .activated()
                    {
                        action = Some(UiAction::Add);
                    }
                });
            });

            ui.add_space(10.0);

            if state.games.len() >= 5 {
                Gui::text_edit(
                    ui.add(
                        egui::TextEdit::singleline(&mut state.filter_query)
                            .id(egui::Id::new("launcher_filter"))
                            .hint_text("Filter…")
                            .desired_width(f32::INFINITY),
                    ),
                );
                ui.add_space(6.0);
            }

            let footer_reserve = 38.0;
            let list_h = (ui.available_height() - footer_reserve).max(68.0);
            let query = state.filter_query.trim().to_ascii_lowercase();

            egui::ScrollArea::vertical()
                .id_salt("split_list")
                .max_height(list_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for game in &state.games {
                        if !query.is_empty() && !game.title.to_ascii_lowercase().contains(&query) {
                            continue;
                        }
                        let is_selected = state.selected.as_deref() == Some(game.entry_id.as_str());
                        let resp = draw_sidebar_row(ui, ap, game, is_selected, compact);
                        if resp.activated() || resp.has_focus() {
                            state.selected = Some(game.entry_id.clone());
                        }
                    }
                });

            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                let (foot_act, foot_anchor) =
                    switchers::draw_sidebar_footer(ui, state, ap, compact);
                if foot_act.is_some() {
                    action = foot_act;
                }
                if foot_anchor.is_some() {
                    anchor = foot_anchor;
                }
            });
        });

    egui::TopBottomPanel::bottom("split_provenance")
        .show_separator_line(false)
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.canvas)
                .inner_margin(egui::Margin::symmetric(if compact { 12 } else { 20 }, 9)),
        )
        .show(ctx, |ui| {
            let Some(idx) = state.selected_index() else {
                return;
            };

            primitives::separator(ui, &ap.theme);
            ui.add_space(6.0);

            let game = &state.games[idx];
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 25.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if quiet_text_button(ui, ap, "Remove")
                        .on_hover_text("Remove entry from library; original files are kept")
                        .activated()
                    {
                        action = Some(UiAction::Remove(game.entry_id.clone()));
                    }
                    if quiet_text_button(ui, ap, "Relocate…")
                        .on_hover_text("Choose the new location of this game’s original files")
                        .activated()
                    {
                        action = Some(UiAction::Locate(game.entry_id.clone()));
                    }
                    ui.add_space(4.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        draw_provenance_inline(ui, ap, game);
                    });
                },
            );
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.canvas)
                .inner_margin(egui::Margin {
                    left: if compact { 12 } else { 20 },
                    right: if compact { 12 } else { 20 },
                    top: 16,
                    bottom: 4,
                }),
        )
        .show(ctx, |ui| {
            state.content_bottom = ui.max_rect().bottom();
            egui::ScrollArea::vertical()
                .id_salt("split_stage")
                .auto_shrink([false, false])
                .show_viewport(ui, |ui, viewport| {
                    ui.set_width(viewport.width());
                    ui.set_max_width(viewport.width());

                    let Some(idx) = state.selected_index() else {
                        return;
                    };
                    let snapshot = state.games[idx].clone();
                    let stage_w = ui.available_width();

                    if stage_w >= 260.0 {
                        ui.horizontal_top(|ui| {
                            let size = preview_size(stage_w, state.split_preview_height, &snapshot);
                            let preview = draw_preview_screen(ui, ap, size, &snapshot, true);
                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                ui.set_max_width(ui.available_width());
                                if let Some(act) = draw_hero_details(
                                    ui, ap, &snapshot, compact, Some(preview.bottom()),
                                ) {
                                    action = Some(act);
                                }
                            });
                        });
                    } else if let Some(act) = draw_hero_details(ui, ap, &snapshot, compact, None) {
                        action = Some(act);
                    }

                    if let EntryStatus::Missing(ref reason) = snapshot.status {
                        ui.add_space(10.0);
                        egui::Frame::NONE
                            .fill(ap.missing.detail_background)
                            .corner_radius(CornerRadius::same(ap.tokens.card_radius))
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("Some required game files could not be found. Use Locate Files… to select their location.")
                                        .size(12.0)
                                        .color(ap.missing.foreground),
                                    )
                                    .wrap(),
                                ).on_hover_text(reason);
                            });
                    }

                    ui.add_space(8.0);
                    section_rule(ui, ap);
                    ui.label(
                        RichText::new("Launch options")
                            .size(12.5)
                            .strong()
                            .color(ap.tokens.text_primary),
                    );
                    ui.add_space(5.0);
                    draw_launch_options_rows(ui, ap, &mut state.games[idx]);
                });
        });

    (action, anchor)
}

fn draw_sidebar_row(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &GameEntry,
    selected: bool,
    compact: bool,
) -> egui::Response {
    let row_h = if compact { 38.0 } else { 42.0 };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), row_h),
        Sense::click_and_drag(),
    );
    let response = Gui::control(response);

    let (_, sel_title, sel_sub) = ap.selected_row_colors();
    ap.paint_game_frame(ui, &response, selected, ap.tokens.row_radius);

    let text_x = rect.left() + if ap.uses_left_bar() { 12.0 } else { 10.0 };
    let title_col = if selected {
        sel_title
    } else {
        ap.tokens.text_primary
    };
    let sub_col = if game.status.is_missing() {
        ap.missing.foreground
    } else if selected {
        sel_sub
    } else {
        ap.tokens.text_muted
    };
    let (title_col, sub_col) = ap.game_text_colours(&response, selected, title_col, sub_col);
    let sub_col = if game.status.is_missing() {
        ap.missing.foreground
    } else {
        sub_col
    };

    paint_title(
        ui.painter(),
        Pos2::new(text_x, rect.top() + 6.0),
        rect.right() - 10.0 - text_x - if game.status.is_missing() { 7.2 } else { 0.0 },
        &game.title,
        FontId::proportional(13.5),
        title_col,
    );

    let sub = if game.status.is_missing() {
        "Missing files".to_string()
    } else {
        game.library_line()
    };
    ui.painter().text(
        Pos2::new(text_x, rect.bottom() - 16.0),
        egui::Align2::LEFT_TOP,
        sub,
        FontId::proportional(11.0),
        sub_col,
    );

    if game.status.is_missing() {
        ui.painter().circle_filled(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            3.2,
            ap.missing.foreground,
        );
    }

    response
}

fn preview_size(stage_width: f32, preferred_height: u16, game: &GameEntry) -> Vec2 {
    let (width, height) = game
        .previews
        .get(game.preview_variant())
        .map(|source| {
            source
                .frame
                .geometry()
                .display_size(reforge_sdl::AspectRatio::Original)
        })
        .unwrap_or((4, 3));
    let aspect = width as f32 / height as f32;
    // Share a widescreen envelope so switching between 4:3 and 16:9 games
    // preserves height even in a narrow window. Wider frames still fit.
    let max_width = (stage_width - 10.0 - 200.0).max(1.0);
    let height = f32::from(preferred_height).min(max_width / aspect.max(16.0 / 9.0));
    Vec2::new(height * aspect, height)
}

fn draw_hero_details(
    ui: &mut egui::Ui,
    ap: &ResolvedAppearance,
    game: &GameEntry,
    compact: bool,
    preview_bottom: Option<f32>,
) -> Option<UiAction> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(&game.title)
                .size(if compact { 18.0 } else { 21.5 })
                .strong()
                .color(ap.tokens.text_primary),
        );
        if let Some(note) = game.edition_label() {
            ui.label(
                RichText::new(format!("· {note}"))
                    .size(12.5)
                    .color(ap.theme.palette.accent),
            );
        }
    });

    let credits = ui.label(
        RichText::new(game.detail_line())
            .size(12.0)
            .color(ap.tokens.text_secondary),
    );

    // Keep the original single-line layout height, even without a tagline.
    // A second line uses the existing gaps without moving the other elements.
    let font = FontId::proportional(11.5);
    let width = ui.available_width();
    let colour = ap.tokens.text_muted;
    // Use the laid-out height, including the original Label's rounding.
    let height = ui
        .painter()
        .layout_no_wrap(" ".into(), font.clone(), colour)
        .size()
        .y;
    let job = egui::text::LayoutJob::simple(game.tagline().into(), font, colour, width);
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.add_space(4.0);
    let palette = game.palette_display();
    let features = game.features_line();
    // Fixed hardware palettes declare their sizes in the choices. Resource-
    // extracted palettes can grow when another preview/mode is selected; only
    // those, or known palettes above four colours, need the compact16 reserve.
    let compact_palette_possible = game
        .options
        .iter()
        .flat_map(|option| option.choices)
        .filter_map(|choice| choice.palette.map(<[_]>::len))
        .max()
        .is_none_or(|colours| colours > 4);
    // A wrapped label normally indents its first row by the preceding swatch.
    // Keep that compact flow unless changing between four and sixteen colours
    // would add a text row. Then use full-width text below a shared-height
    // swatch row, so palette size cannot move Play or the launch options.
    let separate_specs = compact_palette_possible
        && !palette.mosaic
        && !palette.colours.is_empty()
        && palette.colours.len() <= 16
        && {
            let rows = |columns: usize| {
                let mut job = egui::text::LayoutJob::simple(
                    features.clone(),
                    FontId::proportional(11.5),
                    ap.tokens.text_muted,
                    width,
                );
                job.first_row_min_height = 16.0;
                if let Some(section) = job.sections.first_mut() {
                    section.leading_space =
                        columns as f32 * 6.5 + columns.saturating_sub(1) as f32 * 3.0 + 8.0;
                }
                ui.painter().layout_job(job).rows.len()
            };
            rows(4) != rows(8)
        };
    let specs = if separate_specs {
        ui.vertical(|ui| {
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
            paint_palette_at(ui.painter(), ap, row.min, &palette, 6.5);
            ui.label(
                RichText::new(&features)
                    .size(11.5)
                    .color(ap.tokens.text_muted),
            );
        })
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            draw_palette(ui, ap, &palette, 6.5);
            ui.label(
                RichText::new(&features)
                    .size(11.5)
                    .color(ap.tokens.text_muted),
            );
        })
    };
    let tagline_top = rect
        .top()
        .min(specs.response.rect.top() - galley.size().y - 1.0)
        .max(credits.rect.bottom() + 1.0);
    ui.painter()
        .galley(Pos2::new(rect.left(), tagline_top), galley, colour);

    let btn_size = if compact {
        Vec2::new(112.0, 28.0)
    } else {
        Vec2::new(124.0, 30.0)
    };
    if let Some(bottom) = preview_bottom {
        // Align when the text leaves room; otherwise keep a readable gap and
        // let the hero grow. Window height never squeezes the text or image.
        let button_top = (bottom - btn_size.y).max(specs.response.rect.bottom() + 12.0);
        ui.add_space((button_top - ui.cursor().top()).max(0.0));
    } else {
        ui.add_space(if compact { 8.0 } else { 12.0 });
    }
    if game.status.is_missing() {
        if primary_button(ui, ap, "Locate Files…", btn_size, true).activated() {
            action = Some(UiAction::Locate(game.entry_id.clone()));
        }
    } else if primary_button(ui, ap, "Play", btn_size, false).activated() {
        action = Some(UiAction::Play(game.entry_id.clone()));
    }

    action
}
