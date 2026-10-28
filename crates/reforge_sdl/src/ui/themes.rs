// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Shipped theme colours and component compositions, independent of launcher data.
use super::{
    ButtonShape, ChoiceColours, ChoiceShape, Palette, SelectionIndicator, Separator, Surface,
    Theme, WidgetColours,
};
use crate::egui::{
    self, Color32 as C, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2, emath::GuiRounding,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeId {
    #[default]
    ClassicDark,
    SoftTonal,
    WarmAmber,
    CrispLight,
}
impl ThemeId {
    pub const ALL: [Self; 4] = [
        Self::ClassicDark,
        Self::SoftTonal,
        Self::WarmAmber,
        Self::CrispLight,
    ];
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ClassicDark => "classic-dark",
            Self::SoftTonal => "soft-tonal",
            Self::WarmAmber => "warm-amber",
            Self::CrispLight => "crisp-light",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClassicDark => "Classic Dark",
            Self::SoftTonal => "Soft Tonal",
            Self::WarmAmber => "Warm Amber",
            Self::CrispLight => "Crisp Light",
        }
    }
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "classic-dark" => Some(Self::ClassicDark),
            "soft-tonal" => Some(Self::SoftTonal),
            "warm-amber" => Some(Self::WarmAmber),
            "crisp-light" => Some(Self::CrispLight),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteShape {
    Square,
    Circle,
}
#[derive(Clone, Copy)]
pub struct ScreenTokens {
    pub item_bg: C,
    pub item_hover: C,
    pub item_pressed: C,
    pub item_pressed_text: C,
    pub item_pressed_subtext: C,
    pub card_selected_bg: C,
    pub item_border: Option<C>,
    pub cta_hover: C,
    pub text_primary: C,
    pub text_secondary: C,
    pub text_muted: C,
    pub ok_fg: C,
    pub err_banner_bg: C,
    pub notice_banner_bg: C,
    pub card_radius: u8,
    pub row_radius: u8,
    pub preview_radius: u8,
    pub palette_shape: PaletteShape,
    selected_bg: C,
    selected_text: C,
    selected_sub: C,
}
impl ScreenTokens {
    fn use_button_pressed_colours(&mut self, palette: &Palette) {
        self.item_pressed = palette.active.weak_background;
        self.item_pressed_text = palette.active.foreground;
        self.item_pressed_subtext = mix(palette.active.foreground, self.item_pressed, 0.92);
    }
}
#[derive(Clone, Copy)]
pub enum ActionRole {
    Standard,
    Quiet,
    Primary,
    Warning,
}
// Keep missing-state roles independent of generic errors and of each other:
// badge text must contrast with its badge, while regular text and dots match.
#[derive(Clone, Copy)]
pub struct MissingStyle {
    pub foreground: C,
    pub preview_foreground: C,
    pub preview_background: C,
    pub detail_background: C,
}
#[derive(Clone, Copy)]
pub struct ResolvedAppearance {
    pub id: ThemeId,
    pub theme: Theme,
    pub tokens: ScreenTokens,
    pub missing: MissingStyle,
}
fn rgb(c: [u8; 3]) -> C {
    C::from_rgb(c[0], c[1], c[2])
}
fn mix(a: C, b: C, t: f32) -> C {
    let a = a.to_array();
    let b = b.to_array();
    let f = |i| (a[i] as f32 * t + b[i] as f32 * (1.0 - t)).round() as u8;
    C::from_rgb(f(0), f(1), f(2))
}
impl ResolvedAppearance {
    pub fn resolve(id: ThemeId) -> Self {
        let mut theme = Theme::default();
        let p = &mut theme.palette;
        let mut tokens = ScreenTokens {
            item_bg: p.inactive.weak_background,
            item_hover: p.hovered.weak_background,
            item_pressed: p.active.weak_background,
            item_pressed_text: p.active.foreground,
            item_pressed_subtext: mix(p.active.foreground, p.active.weak_background, 0.92),
            card_selected_bg: p.selection.background,
            item_border: None,
            cta_hover: C::from_rgb(54, 122, 154),
            text_primary: p.inactive.foreground,
            text_secondary: p.noninteractive.foreground,
            text_muted: C::from_gray(116),
            ok_fg: C::from_rgb(118, 196, 146),
            err_banner_bg: C::from_rgb(56, 32, 30),
            notice_banner_bg: p.cli_background,
            card_radius: 3,
            row_radius: 3,
            preview_radius: 2,
            palette_shape: PaletteShape::Square,
            selected_bg: p.selection.background,
            selected_text: p.selection.foreground,
            selected_sub: C::from_rgb(200, 226, 238),
        };
        match id {
            ThemeId::ClassicDark => {
                // Native Settings colours; launcher group surfaces are independent.
                p.group = C::from_gray(40);
                theme.components.choices.shape = ChoiceShape::Buttons;
                theme.components.selection.indicator = SelectionIndicator::Outline;
            }
            ThemeId::SoftTonal => {
                // Charcoal/slate elevation, pill controls, no divider outlines.
                configure(
                    p,
                    [20, 22, 26],
                    [27, 30, 36],
                    [38, 43, 52],
                    [50, 56, 68],
                    [132, 182, 242],
                    [58, 106, 168],
                    [232, 236, 242],
                    [154, 162, 176],
                    [104, 112, 126],
                );
                tokens.use_button_pressed_colours(p);
                p.group = rgb([24, 27, 33]);
                p.noninteractive.border = C::TRANSPARENT;
                tokens.item_bg = rgb([33, 37, 45]);
                tokens.item_hover = rgb([43, 48, 58]);
                tokens.selected_bg = rgb([46, 72, 108]);
                tokens.card_selected_bg = rgb([39, 58, 86]);
                tokens.selected_text = C::WHITE;
                tokens.selected_sub = rgb([196, 214, 238]);
                tokens.card_radius = 16;
                tokens.row_radius = 14;
                tokens.preview_radius = 6;
                tokens.palette_shape = PaletteShape::Circle;
                tokens.cta_hover = rgb([72, 122, 190]);
                theme.components.button.shape = ButtonShape::Pill;
                theme.components.button.radius = 16;
                theme.components.button.outline = false;
                theme.components.choices.shape = ChoiceShape::Pill;
                theme.components.choices.radius = 16;
                theme.components.choices.gap = 4.0;
                theme.components.section.surface = Surface::Group;
                theme.components.section.separator = Separator::Space;
                theme.components.section.padding = Vec2::splat(12.0);
                theme.components.section.radius = 16;
                theme.components.selection.indicator = SelectionIndicator::Fill;
            }
            ThemeId::WarmAmber => {
                // Sharp selectors pair with a left selection marker.
                configure(
                    p,
                    [25, 23, 20],
                    [25, 23, 20],
                    [54, 48, 41],
                    [68, 60, 51],
                    [228, 170, 82],
                    [152, 98, 34],
                    [235, 229, 219],
                    [164, 154, 140],
                    [118, 110, 98],
                );
                tokens.use_button_pressed_colours(p);
                p.group = rgb([35, 32, 28]);
                p.noninteractive.border = rgb([68, 60, 50]);
                p.window_border = rgb([68, 60, 50]);
                tokens.item_bg = rgb([35, 32, 28]);
                tokens.item_hover = rgb([47, 43, 37]);
                tokens.selected_bg = rgb([49, 40, 29]);
                tokens.card_selected_bg = tokens.selected_bg;
                tokens.selected_text = rgb([250, 244, 234]);
                tokens.selected_sub = rgb([208, 186, 152]);
                tokens.card_radius = 2;
                tokens.row_radius = 1;
                tokens.preview_radius = 1;
                tokens.cta_hover = rgb([174, 114, 42]);
                theme.components.button.shape = ButtonShape::Flat;
                theme.components.button.radius = 2;
                theme.components.button.outline = false;
                theme.components.choices.shape = ChoiceShape::Sharp;
                theme.components.choices.radius = 0;
                theme.components.choices.gap = 0.0;
                theme.components.selection.indicator = SelectionIndicator::LeftBar;
            }
            ThemeId::CrispLight => {
                *p = Palette::from_visuals(&egui::Visuals::light());
                configure(
                    p,
                    [232, 235, 240],
                    [232, 235, 240],
                    [205, 212, 222],
                    [190, 199, 211],
                    [22, 98, 132],
                    [28, 104, 138],
                    [24, 29, 36],
                    [74, 83, 96],
                    [106, 115, 128],
                );
                tokens.use_button_pressed_colours(p);
                p.dark = false;
                p.window = rgb([244, 246, 250]);
                p.group = rgb([249, 250, 252]);
                // A saturated label accent stands apart from ordinary grey text.
                p.override_text = rgb([29, 78, 216]);
                p.noninteractive.border = rgb([166, 175, 188]);
                p.window_border = p.noninteractive.border;
                p.extreme_background = rgb([212, 218, 227]);
                p.window_shadow = C::from_black_alpha(30);
                p.popup_shadow = C::from_black_alpha(30);
                p.warning = rgb([172, 100, 14]);
                p.pending = p.warning;
                p.error = rgb([188, 46, 40]);
                p.notice_error = p.error;
                p.validation_error = p.error;
                tokens.item_bg = rgb([249, 250, 252]);
                tokens.item_hover = rgb([226, 232, 240]);
                tokens.selected_bg = rgb([212, 230, 242]);
                tokens.card_selected_bg = rgb([214, 231, 242]);
                tokens.item_border = Some(rgb([186, 194, 206]));
                tokens.selected_text = rgb([14, 42, 58]);
                tokens.selected_sub = rgb([48, 84, 104]);
                tokens.card_radius = 5;
                tokens.row_radius = 4;
                tokens.preview_radius = 3;
                tokens.cta_hover = rgb([21, 88, 118]);
                tokens.ok_fg = rgb([26, 128, 76]);
                tokens.err_banner_bg = rgb([248, 224, 222]);
                theme.components.button.shape = ButtonShape::Rounded;
                theme.components.button.radius = 4;
                theme.components.button.outline = false;
                theme.components.choices.shape = ChoiceShape::Segmented;
                theme.components.choices.radius = 4;
                theme.components.choices.gap = 2.0;
                theme.components.selection.indicator = SelectionIndicator::Outline;
            }
        }
        if id != ThemeId::ClassicDark {
            tokens.text_primary = theme.palette.inactive.foreground;
            tokens.text_secondary = theme.palette.noninteractive.foreground;
            tokens.text_muted = theme.palette.locked_text;
            tokens.notice_banner_bg = theme.palette.cli_background;
        }
        let missing = MissingStyle {
            foreground: theme.palette.notice_error,
            preview_foreground: rgb([255, 182, 166]),
            preview_background: rgb([62, 26, 26]),
            detail_background: tokens.err_banner_bg,
        };
        Self {
            id,
            theme,
            tokens,
            missing,
        }
    }
    /// Shared interaction treatment for launcher actions, including icon-only
    /// and disclosure buttons. Selection and emphasis affect the base fill;
    /// the theme owns hover, pressed and focus treatment.
    pub fn action_visuals(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        role: ActionRole,
    ) -> egui::style::WidgetVisuals {
        let mut v = *ui.style().interact(response);
        let p = self.theme.palette;
        let (base, hover) = match role {
            ActionRole::Standard => (p.inactive.weak_background, p.hovered.weak_background),
            ActionRole::Quiet => (
                p.inactive
                    .weak_background
                    .gamma_multiply(if p.dark { 0.86 } else { 0.92 }),
                p.hovered.weak_background,
            ),
            ActionRole::Primary => (p.selection.background, self.tokens.cta_hover),
            ActionRole::Warning => (
                p.warning.gamma_multiply(if p.dark { 0.65 } else { 0.92 }),
                p.warning.gamma_multiply(if p.dark { 0.80 } else { 1.0 }),
            ),
        };
        v.weak_bg_fill = if response.is_pointer_button_down_on() && response.contains_pointer() {
            match role {
                ActionRole::Primary | ActionRole::Warning => base.gamma_multiply(0.88),
                _ => p.active.weak_background,
            }
        } else if response.hovered() {
            hover
        } else if response.has_focus() {
            // Focus stays visible without consuming the hover state. This is
            // important for auto-focused dialog actions such as Cancel.
            base.lerp_to_gamma(hover, 0.5)
        } else {
            base
        };
        v.bg_fill = v.weak_bg_fill;
        if !self.theme.components.button.outline {
            // Borderless themes retain their filled-button recipe on focus;
            // the hover/focus fill above is their navigation indication.
            v.bg_stroke = Stroke::NONE;
            v.expansion = 0.0;
        }
        v.corner_radius = egui::CornerRadius::same(self.theme.components.button.radius);
        v
    }
    pub fn paint_action_frame(&self, ui: &egui::Ui, response: &egui::Response, role: ActionRole) {
        let v = self.action_visuals(ui, response, role);
        ui.painter().rect(
            response.rect.expand(v.expansion),
            v.corner_radius,
            v.weak_bg_fill,
            v.bg_stroke,
            StrokeKind::Inside,
        );
    }
    pub fn selected_row_colors(&self) -> (C, C, C) {
        (
            self.tokens.selected_bg,
            self.tokens.selected_text,
            self.tokens.selected_sub,
        )
    }
    pub fn uses_left_bar(&self) -> bool {
        self.theme.components.selection.indicator == SelectionIndicator::LeftBar
    }
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_visuals(if self.theme.palette.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        ctx.style_mut(|s| {
            s.animation_time = 0.0;
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = self.theme.components.button.padding;
            s.spacing.interact_size.y = 22.0;
            for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
                s.text_styles.insert(text, FontId::proportional(14.0));
            }
            if self.id != ThemeId::ClassicDark {
                for v in [
                    &mut s.visuals.widgets.inactive,
                    &mut s.visuals.widgets.hovered,
                    &mut s.visuals.widgets.active,
                    &mut s.visuals.widgets.open,
                ] {
                    v.corner_radius = egui::CornerRadius::same(self.theme.components.button.radius);
                }
            }
        });
        self.theme.apply_palette(ctx);
    }
    pub fn paint_left_bar(&self, p: &egui::Painter, r: Rect, radius: u8) {
        let inset = f32::from(radius).max(2.0);
        p.rect_filled(
            Rect::from_min_max(
                Pos2::new(r.left(), r.top() + inset),
                Pos2::new(r.left() + 3.0, r.bottom() - inset),
            ),
            1.0,
            self.theme.palette.accent,
        );
    }
    pub fn game_text_colours(
        &self,
        response: &egui::Response,
        selected: bool,
        title: C,
        subtitle: C,
    ) -> (C, C) {
        if !selected
            && self.id != ThemeId::ClassicDark
            && response.is_pointer_button_down_on()
            && response.contains_pointer()
        {
            (
                self.tokens.item_pressed_text,
                self.tokens.item_pressed_subtext,
            )
        } else {
            (title, subtitle)
        }
    }
    pub fn paint_game_frame(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        selected: bool,
        radius: u8,
    ) {
        let r = response.rect;
        let t = self.tokens;
        if self.id == ThemeId::ClassicDark {
            let v = if !selected
                && response.is_pointer_button_down_on()
                && !response.contains_pointer()
            {
                ui.visuals().widgets.inactive
            } else {
                ui.style().interact_selectable(response, selected)
            };
            ui.painter().rect(
                if selected { r } else { r.expand(v.expansion) },
                3,
                v.weak_bg_fill,
                if selected {
                    ui.visuals().selection.stroke
                } else {
                    v.bg_stroke
                },
                StrokeKind::Inside,
            );
        } else {
            let fill = if selected {
                t.selected_bg
            } else if response.is_pointer_button_down_on() && response.contains_pointer() {
                t.item_pressed
            } else if response.hovered() && response.contains_pointer() {
                t.item_hover
            } else {
                t.item_bg
            };
            ui.painter().rect_filled(r, radius, fill);
            if selected && self.uses_left_bar() {
                self.paint_left_bar(ui.painter(), r, radius);
            } else if selected
                && self.theme.components.selection.indicator == SelectionIndicator::Outline
            {
                ui.painter().rect_stroke(
                    r,
                    radius,
                    Stroke::new(1.0_f32, self.theme.palette.accent),
                    StrokeKind::Inside,
                );
            } else if let Some(c) = t.item_border {
                ui.painter()
                    .rect_stroke(r, radius, Stroke::new(1.0_f32, c), StrokeKind::Inside);
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    r,
                    radius,
                    Stroke::new(1.0_f32, self.theme.palette.focus),
                    StrokeKind::Inside,
                );
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn configure(
    p: &mut Palette,
    canvas: [u8; 3],
    panel: [u8; 3],
    button: [u8; 3],
    hover: [u8; 3],
    accent: [u8; 3],
    selection: [u8; 3],
    text: [u8; 3],
    secondary: [u8; 3],
    muted: [u8; 3],
) {
    p.strong_text = rgb(text);
    p.canvas = rgb(canvas);
    p.panel = rgb(panel);
    p.group = rgb(panel);
    p.window = rgb(panel);
    p.extreme_background = mix(p.canvas, C::BLACK, 0.8);
    p.accent = rgb(accent);
    p.focus = p.accent;
    p.override_text = p.accent;
    p.hyperlink = p.accent;
    p.selection = ChoiceColours {
        background: rgb(selection),
        foreground: C::WHITE,
        border: p.accent,
    };
    p.inactive = WidgetColours {
        background: rgb(button),
        weak_background: rgb(button),
        border: mix(rgb(button), rgb(hover), 0.5),
        foreground: rgb(text),
    };
    p.hovered = WidgetColours {
        background: rgb(hover),
        weak_background: rgb(hover),
        border: p.accent,
        foreground: rgb(text),
    };
    p.active = WidgetColours {
        background: rgb(selection),
        weak_background: rgb(selection),
        border: p.accent,
        foreground: C::WHITE,
    };
    p.open = p.hovered;
    p.noninteractive = WidgetColours {
        background: p.panel,
        weak_background: p.panel,
        border: mix(p.panel, rgb(secondary), 0.24),
        foreground: rgb(secondary),
    };
    p.locked_text = rgb(muted);
    p.cli_background = mix(p.selection.background, p.panel, 0.3);
    p.cli_text = rgb(text);
    p.cli_lock = mix(p.accent, rgb(text), 0.65);
    p.lock_keyhole = p.panel;
    p.inherited = ChoiceColours {
        background: mix(p.selection.background, p.panel, 0.4),
        foreground: mix(p.accent, rgb(secondary), 0.55),
        border: mix(p.accent, p.panel, 0.45),
    };
}
pub fn paint_theme_swatch(p: &egui::Painter, r: Rect, id: ThemeId, ring: Option<C>) {
    let theme = ResolvedAppearance::resolve(id).theme;
    // Snap the origin and size once, then reuse physical-pixel insets. Rounding
    // every absolute edge separately can give matching gaps different widths
    // at fractional display scales or positions.
    let scale = p.pixels_per_point();
    let r = Rect::from_min_size(
        r.min.round_to_pixels(scale),
        r.size().round_to_pixels(scale),
    );
    let inset = |units: f32| units.round_to_pixels(scale);
    p.rect_filled(r, 2, theme.palette.canvas);
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(
                (r.center().x - 1.0).round_to_pixels(scale),
                r.top() + inset(3.0),
            ),
            Pos2::new(r.right() - inset(2.5), r.bottom() - inset(3.0)),
        ),
        1,
        theme.palette.selection.background,
    );
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(r.left() + inset(1.5), r.top() + inset(2.0)),
            Pos2::new(r.left() + inset(4.0), r.bottom() - inset(2.0)),
        ),
        1,
        theme.palette.accent,
    );
    p.rect_stroke(
        r,
        2,
        Stroke::new(1.0_f32, ring.unwrap_or(theme.palette.window_border)),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_ids_round_trip_and_classic_preserves_native_colours() {
        assert_eq!(ThemeId::default(), ThemeId::ClassicDark);
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_slug(id.slug()), Some(id));
        }
        assert_eq!(ThemeId::from_slug("unknown-future-theme"), None);
        let classic = ResolvedAppearance::resolve(ThemeId::ClassicDark);
        let mut native = Theme::default().palette;
        // The independent launcher grouping surface is the sole colour change.
        native.group = classic.theme.palette.group;
        assert_eq!(classic.theme.palette, native);
    }

    #[test]
    fn applying_every_theme_preserves_native_settings_geometry() {
        let context = egui::Context::default();
        crate::settings_ui::style(&context);
        let baseline = context.style();
        for id in ThemeId::ALL {
            let appearance = ResolvedAppearance::resolve(id);
            appearance.theme.apply_palette(&context);
            let themed = context.style();
            assert_eq!(
                themed.spacing, baseline.spacing,
                "{id:?}: palette changed spacing"
            );
            for (actual, expected) in [
                (
                    themed.visuals.widgets.inactive,
                    baseline.visuals.widgets.inactive,
                ),
                (
                    themed.visuals.widgets.hovered,
                    baseline.visuals.widgets.hovered,
                ),
                (
                    themed.visuals.widgets.active,
                    baseline.visuals.widgets.active,
                ),
                (themed.visuals.widgets.open, baseline.visuals.widgets.open),
            ] {
                assert_eq!(actual.corner_radius, expected.corner_radius);
                assert_eq!(actual.bg_stroke.width, expected.bg_stroke.width);
                assert_eq!(actual.fg_stroke.width, expected.fg_stroke.width);
                assert_eq!(actual.expansion, expected.expansion);
            }
            assert_eq!(super::super::current_theme(&context), appearance.theme);
            assert_eq!(themed.visuals.window_fill, appearance.theme.palette.window);
        }
    }
}
