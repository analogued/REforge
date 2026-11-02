// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reusable egui controls, surfaces and separators. No game or save policy.
use super::{
    ButtonAppearance, ButtonShape, ChoiceShape, SectionAppearance, SelectionIndicator,
    SelectionState, Separator, Surface, Theme,
};
use egui::{
    Color32, CornerRadius, InnerResponse, Rect, Response, Stroke, StrokeKind, Ui, Vec2, WidgetText,
    emath::GuiRounding,
};

/// Release-inside activation for buttons using `Sense::click_and_drag()`.
/// Keeps normal keyboard/accessibility clicks and pointer capture when a held
/// pointer moves beyond egui's click-distance or click-duration threshold.
pub trait ButtonActivation {
    fn activated(&self) -> bool;
}
impl ButtonActivation for Response {
    fn activated(&self) -> bool {
        self.enabled()
            && (self.clicked()
                || (self.drag_stopped_by(egui::PointerButton::Primary) && self.contains_pointer()))
    }
}

/// Shared button widget; its appearance comes from the theme's composition.
#[must_use]
pub struct Button<'a> {
    theme: &'a Theme,
    label: WidgetText,
    state: SelectionState,
    min_size: Vec2,
}
impl<'a> Button<'a> {
    pub fn new(theme: &'a Theme, label: impl Into<WidgetText>) -> Self {
        Self {
            theme,
            label: label.into(),
            state: SelectionState::Unselected,
            min_size: Vec2::ZERO,
        }
    }
    pub fn selection(mut self, state: SelectionState) -> Self {
        self.state = state;
        self
    }
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }
}
impl egui::Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        draw_button(
            ui,
            self.theme,
            self.theme.components.button,
            self.label,
            self.state,
            self.min_size,
        )
    }
}

/// Ordinary themed button. The caller registers its navigation group, as with
/// egui::Button; choice_strip registers its own individual choice controls.
/// Use [`ButtonActivation::activated`] to accept moving presses released inside.
pub fn button(
    ui: &mut Ui,
    theme: &Theme,
    label: impl Into<WidgetText>,
    state: SelectionState,
) -> Response {
    draw_button(
        ui,
        theme,
        theme.components.button,
        label.into(),
        state,
        Vec2::ZERO,
    )
}
fn draw_button(
    ui: &mut Ui,
    theme: &Theme,
    appearance: ButtonAppearance,
    label: WidgetText,
    state: SelectionState,
    min_size: Vec2,
) -> Response {
    // Keep the existing native widget path and identity for the current UI.
    if appearance.shape == ButtonShape::Native
        && state != SelectionState::Inherited
        && appearance.padding == ui.spacing().button_padding
        && appearance.outline
    {
        return ui.add(
            egui::Button::new(label)
                .sense(egui::Sense::click_and_drag())
                .selected(state == SelectionState::Explicit)
                .min_size(min_size),
        );
    }
    // Keep widget identity stable when a preference changes from inherited to
    // explicit, or the theme changes its primitive. A nested scope changes IDs.
    let previous = ui.style().clone();
    ui.spacing_mut().button_padding = appearance.padding;
    let mut widget = egui::Button::new(label)
        .sense(egui::Sense::click_and_drag())
        .selected(state == SelectionState::Explicit)
        .min_size(min_size);
    let corner = match appearance.shape {
        ButtonShape::Native => None,
        ButtonShape::Sharp => Some(CornerRadius::ZERO),
        ButtonShape::Pill => Some(CornerRadius::same(u8::MAX)),
        ButtonShape::Flat | ButtonShape::Rounded => Some(CornerRadius::same(appearance.radius)),
    };
    if let Some(corner) = corner {
        widget = widget.corner_radius(corner);
    }
    if state == SelectionState::Inherited {
        let p = theme.palette.inherited;
        widget = widget
            .fill(p.background)
            .stroke(Stroke::new(1.0_f32, p.border));
        // WidgetText applies this only to text without an explicit colour.
        ui.visuals_mut().override_text_color = Some(p.foreground);
    } else if appearance.shape != ButtonShape::Native {
        if state == SelectionState::Explicit {
            widget = widget.fill(theme.palette.selection.background);
            ui.visuals_mut().override_text_color = Some(theme.palette.selection.foreground);
        } else {
            widget = widget.fill(theme.palette.inactive.weak_background);
        }
    }
    if !appearance.outline {
        widget = widget.stroke(Stroke::NONE);
    }
    let response = ui.add(widget);
    ui.set_style(previous);
    response
}

/// Returns the clicked value even when it is already selected. The caller owns
/// pin/unpin policy and persistence; this works for preferences or ordinary tabs.
pub fn choice_strip<T: Clone + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    choices: &[(T, &str)],
    current: &T,
    origin: SelectionState,
) -> Option<T> {
    choice_strip_enabled(ui, theme, choices, current, origin, |_| true)
}

/// A choice strip with unavailable choices excluded from input navigation.
pub fn choice_strip_enabled<T: Clone + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    choices: &[(T, &str)],
    current: &T,
    origin: SelectionState,
    enabled: impl Fn(&T) -> bool,
) -> Option<T> {
    let style = theme.components.choices;
    let joined = matches!(
        style.shape,
        ChoiceShape::Sharp | ChoiceShape::Segmented | ChoiceShape::Pill
    );
    if joined {
        return joined_choice_strip(ui, theme, choices, current, origin, enabled);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = style.gap;
        let mut clicked = None;
        for (value, label) in choices {
            let state = if value == current {
                origin
            } else {
                SelectionState::Unselected
            };
            let appearance = ButtonAppearance {
                shape: if style.shape == ChoiceShape::Buttons {
                    ButtonShape::Native
                } else {
                    ButtonShape::Flat
                },
                radius: style.radius,
                padding: style.padding,
                outline: style.shape == ChoiceShape::Buttons,
            };
            let response = ui
                .add_enabled_ui(enabled(value), |ui| {
                    crate::settings_navigation::control(draw_button(
                        ui,
                        theme,
                        appearance,
                        (*label).into(),
                        state,
                        Vec2::ZERO,
                    ))
                })
                .inner;
            if response.activated() {
                clicked = Some(value.clone());
            }
        }
        clicked
    })
    .inner
}

fn joined_choice_strip<T: Clone + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    choices: &[(T, &str)],
    current: &T,
    origin: SelectionState,
    enabled: impl Fn(&T) -> bool,
) -> Option<T> {
    if choices.is_empty() {
        return None;
    }
    let style = theme.components.choices;
    let sharp = style.shape == ChoiceShape::Sharp;
    let scale = ui.pixels_per_point();
    // Construct the whole group in physical pixels. One shared height and
    // inset keep every segment aligned, even at fractional display scaling.
    let pixel = |points: f32| (points * scale).round();
    let border = pixel(1.0).max(1.0);
    let inset = border + if sharp { 0.0 } else { pixel(2.0) };
    let gap = if sharp {
        border
    } else {
        pixel(style.gap).max(0.0)
    };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galleys: Vec<_> = choices
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).into(), font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let widths: Vec<_> = galleys
        .iter()
        .map(|g| ((g.size().x + 2.0 * style.padding.x) * scale).ceil())
        .collect();
    let height = galleys
        .iter()
        .map(|g| g.size().y + 2.0 * style.padding.y)
        .fold(ui.spacing().interact_size.y, f32::max);
    let height = (height * scale).ceil();
    let size = Vec2::new(
        widths.iter().sum::<f32>() + gap * (choices.len() - 1) as f32 + 2.0 * inset,
        height + 2.0 * inset,
    ) / scale;
    let (allocated, group) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rect = Rect::from_min_size(allocated.min.round_to_pixels(scale), size);
    let radius = if sharp {
        0
    } else if style.shape == ChoiceShape::Pill {
        u8::MAX
    } else {
        style.radius
    };
    ui.painter().rect(
        rect,
        radius,
        theme.palette.extreme_background,
        Stroke::new(border / scale, theme.palette.noninteractive.border),
        StrokeKind::Inside,
    );
    let top = rect.top() + inset / scale;
    let mut x = rect.left() + inset / scale;
    let mut clicked = None;
    for (i, ((value, label), galley)) in choices.iter().zip(galleys).enumerate() {
        let tile = Rect::from_min_size(egui::pos2(x, top), Vec2::new(widths[i], height) / scale);
        let available = ui.is_enabled() && enabled(value);
        let response = ui
            .add_enabled_ui(available, |ui| {
                crate::settings_navigation::control(ui.interact(
                    tile,
                    group.id.with(i),
                    egui::Sense::click_and_drag(),
                ))
            })
            .inner;
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, available, *label));
        let state = if value == current {
            origin
        } else {
            SelectionState::Unselected
        };
        let p = theme.palette;
        let (base, text) = match state {
            SelectionState::Explicit => (p.selection.background, p.selection.foreground),
            SelectionState::Inherited => (p.inherited.background, p.inherited.foreground),
            SelectionState::Unselected => (Color32::TRANSPARENT, p.inactive.foreground),
        };
        // Custom painting happens outside the disabled interaction scope.
        // Use egui's normal fade for an unavailable choice, without adding
        // opacity or compounding the fade inherited from a disabled group.
        let mut painter = ui.painter().clone();
        if !available {
            painter.set_fade_to_color(Some(ui.visuals().fade_out_to_color()));
        }
        let fill = if response.is_pointer_button_down_on() && response.contains_pointer() {
            if state == SelectionState::Unselected {
                p.active.weak_background
            } else {
                base.lerp_to_gamma(Color32::BLACK, 0.12)
            }
        } else if response.hovered() {
            if state == SelectionState::Unselected {
                p.hovered.weak_background
            } else {
                base.lerp_to_gamma(
                    if p.dark {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    },
                    0.08,
                )
            }
        } else {
            base
        };
        let stroke = if response.has_focus() {
            Stroke::new(border / scale, p.focus)
        } else {
            Stroke::NONE
        };
        // Hover and selection change colour only: no expansion into adjacent
        // options, the group border, or its rounded outer corners.
        painter.rect(tile, radius, fill, stroke, StrokeKind::Inside);
        painter.galley(tile.center() - galley.size() * 0.5, galley, text);
        if response.activated() {
            clicked = Some(value.clone());
        }
        x += (widths[i] + gap) / scale;
        if sharp && i + 1 < choices.len() {
            let divider = Rect::from_min_size(
                egui::pos2(tile.right(), top),
                Vec2::new(gap, height) / scale,
            );
            ui.painter()
                .rect_filled(divider, 0, p.noninteractive.border);
        }
    }
    clicked
}

pub fn surface(
    ui: &mut Ui,
    theme: &Theme,
    appearance: SectionAppearance,
    content: impl FnOnce(&mut Ui),
) -> InnerResponse<()> {
    let background = match appearance.surface {
        Surface::Transparent => Color32::TRANSPARENT,
        Surface::Canvas => theme.palette.canvas,
        Surface::Panel => theme.palette.panel,
        Surface::Group => theme.palette.group,
    };
    egui::Frame::NONE
        .fill(background)
        .corner_radius(appearance.radius)
        .inner_margin(egui::Margin::symmetric(
            appearance.padding.x.round() as i8,
            appearance.padding.y.round() as i8,
        ))
        .stroke(if appearance.border {
            Stroke::new(1.0_f32, theme.palette.noninteractive.border)
        } else {
            Stroke::NONE
        })
        .show(ui, content)
}

pub fn separator(ui: &mut Ui, theme: &Theme) {
    let style = theme.components.section;
    match style.separator {
        Separator::Line => {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                Stroke::new(1.0_f32, theme.palette.noninteractive.border),
            );
        }
        Separator::Space => {
            ui.add_space(style.gap);
        }
        Separator::None => {}
    }
}
pub fn section(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    content: impl FnOnce(&mut Ui),
) -> InnerResponse<()> {
    ui.add_space(theme.components.section.gap);
    surface(ui, theme, theme.components.section, |ui| {
        if !title.is_empty() {
            ui.label(
                egui::RichText::new(title)
                    .strong()
                    .color(theme.palette.strong_text),
            );
        }
        separator(ui, theme);
        content(ui);
    })
}

/// Paint selection on a screen-owned card/list rectangle without imposing its layout.
pub fn selection_marker(painter: &egui::Painter, theme: &Theme, rect: Rect, state: SelectionState) {
    if state == SelectionState::Unselected {
        return;
    }
    let style = theme.components.selection;
    let colour = if state == SelectionState::Inherited {
        theme.palette.inherited
    } else {
        theme.palette.selection
    };
    match style.indicator {
        SelectionIndicator::Fill => {
            painter.rect_filled(rect, style.radius, colour.background);
        }
        SelectionIndicator::Outline => {
            painter.rect_stroke(
                rect,
                style.radius,
                Stroke::new(1.0_f32, colour.border),
                StrokeKind::Inside,
            );
        }
        SelectionIndicator::LeftBar => {
            painter.rect_filled(
                Rect::from_min_size(
                    rect.min,
                    Vec2::new(style.bar_width.min(rect.width()), rect.height()),
                ),
                style.radius,
                colour.border,
            );
        }
    }
}
