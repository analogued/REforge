// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Shared library arrangement with independently selected appearance.
pub mod compact_deck;
pub mod gallery_shelf;
pub mod split_inspector;
use super::{
    catalog::ResolvedAppearance,
    model::{LayoutVariant, State},
    switchers::{self, AnchorPlacement},
    widgets::{self, UiAction},
};
use reforge_sdl::{
    egui::{self, Align, Layout, RichText, Vec2},
    ui::ButtonActivation,
};

pub fn draw(ctx: &egui::Context, state: &mut State) {
    let ap = ResolvedAppearance::resolve(state.effective_theme());
    ap.apply(ctx);
    let (action, anchor) = if state.games.is_empty() {
        empty(ctx, state, &ap)
    } else {
        match state.layout {
            LayoutVariant::SplitInspector => split_inspector::draw(ctx, state, &ap),
            LayoutVariant::GalleryShelf => gallery_shelf::draw(ctx, state, &ap),
            LayoutVariant::CompactDeck => compact_deck::draw(ctx, state, &ap),
        }
    };
    if !state.busy && state.pending_remove.is_none() {
        if let Some(action) = action {
            match action {
                UiAction::Play(id) => state.trigger_play(id),
                UiAction::Add => state.trigger_add(),
                UiAction::Locate(id) => state.trigger_locate(id),
                UiAction::Remove(id) => state.request_remove(id),
                UiAction::OpenSettings => state.open_settings(),
            }
        }
    }
    switchers::draw_overlays(ctx, state, &ap, anchor);
    switchers::draw_remove_confirmation(ctx, state, &ap);
    if let Some(error) = state.games.iter_mut().find_map(|g| g.error.take()) {
        state.notify(error, true);
    }
    state.expire_notice();
    if let Some((message, error)) = &state.notice {
        let bottom = (state.content_bottom - 12.0).max(120.0);
        egui::Area::new(egui::Id::new("launcher_notice"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(egui::pos2(ctx.screen_rect().center().x, bottom.max(120.0)))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(if *error {
                        ap.tokens.err_banner_bg
                    } else {
                        ap.theme.palette.window
                    })
                    .corner_radius(ap.tokens.card_radius)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_max_width((ctx.screen_rect().width() - 80.0).clamp(200.0, 600.0));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(message).color(if *error {
                                ap.theme.palette.notice_error
                            } else {
                                ap.tokens.text_primary
                            }));
                            if widgets::quiet_text_button(ui, &ap, "Dismiss").activated() {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(egui::Id::new("dismiss_notice"), true)
                                });
                            }
                        });
                    });
            });
        if ctx
            .data_mut(|d| d.remove_temp::<bool>(egui::Id::new("dismiss_notice")))
            .unwrap_or(false)
        {
            state.notice = None;
        }
    }
}
fn empty(
    ctx: &egui::Context,
    state: &mut State,
    ap: &ResolvedAppearance,
) -> (Option<UiAction>, Option<AnchorPlacement>) {
    let mut action = None;
    let mut anchor = None;
    egui::TopBottomPanel::top("empty_header")
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
                        if widgets::secondary_button(ui, ap, "Add…", Vec2::new(66.0, 25.0))
                            .activated()
                        {
                            action = Some(UiAction::Add);
                        }
                        if widgets::secondary_button(ui, ap, "Settings…", Vec2::new(84.0, 25.0))
                            .activated()
                        {
                            action = Some(UiAction::OpenSettings);
                        }
                        anchor = switchers::draw_header_theme_control(ui, state, ap);
                    });
                },
            );
        });
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ap.theme.palette.canvas)
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            state.content_bottom = ui.max_rect().bottom();
            if let Some(a) = widgets::draw_empty_onboarding(ui, ap) {
                action = Some(a);
            }
        });
    (action, anchor)
}
