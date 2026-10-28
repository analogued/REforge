// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Reusable egui controls, surfaces and separators. No game or save policy.
use super::{
    ButtonAppearance, ButtonShape, ChoiceShape, SectionAppearance, SelectionIndicator,
    SelectionState, Separator, Surface, Theme,
};
use egui::{
    Color32, CornerRadius, InnerResponse, Rect, Response, Stroke, StrokeKind, Ui, Vec2, WidgetText,
};

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
            false,
            self.min_size,
        )
    }
}

/// Ordinary themed button. The caller registers its navigation group, as with
/// egui::Button; choice_strip registers its own individual choice controls.
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
        false,
        Vec2::ZERO,
    )
}
fn draw_button(
    ui: &mut Ui,
    theme: &Theme,
    appearance: ButtonAppearance,
    label: WidgetText,
    state: SelectionState,
    transparent: bool,
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
                .selected(state == SelectionState::Explicit)
                .min_size(min_size),
        );
    }
    // Keep widget identity stable when a preference changes from inherited to
    // explicit, or the theme changes its primitive. A nested scope changes IDs.
    let previous = ui.style().clone();
    ui.spacing_mut().button_padding = appearance.padding;
    let mut widget = egui::Button::new(label)
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
            widget = widget.fill(if transparent {
                Color32::TRANSPARENT
            } else {
                theme.palette.inactive.weak_background
            });
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
    let style = theme.components.choices;
    let joined = matches!(
        style.shape,
        ChoiceShape::Sharp | ChoiceShape::Segmented | ChoiceShape::Pill
    );
    let radius = if style.shape == ChoiceShape::Pill {
        u8::MAX
    } else {
        style.radius
    };
    let frame = if joined {
        egui::Frame::NONE
            .fill(theme.palette.extreme_background)
            .corner_radius(radius)
            .stroke(Stroke::new(1.0_f32, theme.palette.noninteractive.border))
            .inner_margin(if style.shape == ChoiceShape::Sharp {
                0
            } else {
                2
            })
    } else {
        egui::Frame::NONE
    };
    frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = if style.shape == ChoiceShape::Sharp {
                    0.0
                } else {
                    style.gap
                };
                let mut clicked = None;
                for (i, (value, label)) in choices.iter().enumerate() {
                    let state = if value == current {
                        origin
                    } else {
                        SelectionState::Unselected
                    };
                    let shape = match style.shape {
                        ChoiceShape::Buttons => ButtonShape::Native,
                        ChoiceShape::Flat | ChoiceShape::Segmented => ButtonShape::Flat,
                        ChoiceShape::Sharp => ButtonShape::Sharp,
                        ChoiceShape::Pill => ButtonShape::Pill,
                    };
                    let appearance = ButtonAppearance {
                        shape,
                        radius,
                        padding: style.padding,
                        outline: style.shape == ChoiceShape::Buttons,
                    };
                    let response = draw_button(
                        ui,
                        theme,
                        appearance,
                        (*label).into(),
                        state,
                        joined,
                        Vec2::ZERO,
                    );
                    if crate::settings_navigation::control(response).clicked() {
                        clicked = Some(value.clone());
                    }
                    if style.shape == ChoiceShape::Sharp && i + 1 < choices.len() {
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(1.0, ui.spacing().interact_size.y),
                            egui::Sense::hover(),
                        );
                        ui.painter().vline(
                            rect.center().x,
                            rect.y_range(),
                            Stroke::new(1.0_f32, theme.palette.noninteractive.border),
                        );
                    }
                }
                clicked
            })
            .inner
        })
        .inner
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
