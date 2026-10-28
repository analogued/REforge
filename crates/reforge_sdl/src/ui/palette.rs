// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Colour roles only. Applying them never replaces widget geometry or spacing.
use egui::{Color32, Visuals, style::WidgetVisuals};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetColours {
    pub background: Color32,
    pub weak_background: Color32,
    pub border: Color32,
    pub foreground: Color32,
}
impl WidgetColours {
    fn from_visuals(v: &WidgetVisuals) -> Self {
        Self {
            background: v.bg_fill,
            weak_background: v.weak_bg_fill,
            border: v.bg_stroke.color,
            foreground: v.fg_stroke.color,
        }
    }
    fn apply(self, v: &mut WidgetVisuals) {
        v.bg_fill = self.background;
        v.weak_bg_fill = self.weak_background;
        v.bg_stroke.color = self.border;
        v.fg_stroke.color = self.foreground;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChoiceColours {
    pub background: Color32,
    pub foreground: Color32,
    pub border: Color32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub canvas: Color32,
    pub panel: Color32,
    pub group: Color32,
    pub window: Color32,
    pub window_border: Color32,
    pub window_shadow: Color32,
    pub popup_shadow: Color32,
    pub extreme_background: Color32,
    pub faint_background: Color32,
    pub code_background: Color32,
    pub text_override: Option<Color32>,
    /// Heading colour, independent of egui's active-control text colour.
    pub strong_text: Color32,
    pub hyperlink: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub validation_error: Color32,
    pub notice_error: Color32,
    pub accent: Color32,
    pub focus: Color32,
    pub override_text: Color32,
    pub pending: Color32,
    pub cli_lock: Color32,
    pub cli_background: Color32,
    pub cli_text: Color32,
    pub lock_keyhole: Color32,
    pub locked_text: Color32,
    pub scrim: Color32,
    pub text_cursor: Color32,
    pub selection: ChoiceColours,
    pub inherited: ChoiceColours,
    pub inactive: WidgetColours,
    pub hovered: WidgetColours,
    pub active: WidgetColours,
    pub open: WidgetColours,
    pub noninteractive: WidgetColours,
}
impl Palette {
    /// Extract colours without importing egui's radii, borders widths or metrics.
    /// This is a construction helper, not a catalog of shipped themes.
    pub fn from_visuals(v: &Visuals) -> Self {
        let accent = v.hyperlink_color;
        let selection = ChoiceColours {
            background: v.selection.bg_fill,
            foreground: v.selection.stroke.color,
            border: v.selection.stroke.color,
        };
        Self {
            dark: v.dark_mode,
            canvas: v.panel_fill,
            panel: v.panel_fill,
            group: v.window_fill,
            window: v.window_fill,
            window_border: v.window_stroke.color,
            window_shadow: v.window_shadow.color,
            popup_shadow: v.popup_shadow.color,
            extreme_background: v.extreme_bg_color,
            faint_background: v.faint_bg_color,
            code_background: v.code_bg_color,
            text_override: v.override_text_color,
            strong_text: v.strong_text_color(),
            hyperlink: v.hyperlink_color,
            warning: v.warn_fg_color,
            error: Color32::LIGHT_RED,
            validation_error: v.error_fg_color,
            notice_error: v.error_fg_color,
            accent,
            focus: accent,
            override_text: accent,
            pending: v.warn_fg_color,
            cli_lock: accent,
            cli_background: v.extreme_bg_color,
            cli_text: v.text_color(),
            lock_keyhole: v.extreme_bg_color,
            locked_text: v.weak_text_color(),
            scrim: Color32::from_black_alpha(165),
            text_cursor: v.text_cursor.stroke.color,
            selection,
            inherited: ChoiceColours {
                background: mix(selection.background, v.panel_fill, 0.40),
                foreground: mix(accent, v.text_color(), 0.55),
                border: mix(accent, v.panel_fill, 0.45),
            },
            inactive: WidgetColours::from_visuals(&v.widgets.inactive),
            hovered: WidgetColours::from_visuals(&v.widgets.hovered),
            active: WidgetColours::from_visuals(&v.widgets.active),
            open: WidgetColours::from_visuals(&v.widgets.open),
            noninteractive: WidgetColours::from_visuals(&v.widgets.noninteractive),
        }
    }
    pub(super) fn apply(self, v: &mut Visuals) {
        v.dark_mode = self.dark;
        v.panel_fill = self.panel;
        v.window_fill = self.window;
        v.window_stroke.color = self.window_border;
        v.window_shadow.color = self.window_shadow;
        v.popup_shadow.color = self.popup_shadow;
        v.extreme_bg_color = self.extreme_background;
        v.faint_bg_color = self.faint_background;
        v.code_bg_color = self.code_background;
        v.override_text_color = self.text_override;
        v.hyperlink_color = self.hyperlink;
        v.warn_fg_color = self.warning;
        // Keep the generic egui error colour independent of custom notices.
        v.error_fg_color = self.validation_error;
        v.text_cursor.stroke.color = self.text_cursor;
        v.selection.bg_fill = self.selection.background;
        v.selection.stroke.color = self.selection.foreground;
        self.inactive.apply(&mut v.widgets.inactive);
        self.hovered.apply(&mut v.widgets.hovered);
        self.active.apply(&mut v.widgets.active);
        self.open.apply(&mut v.widgets.open);
        self.noninteractive.apply(&mut v.widgets.noninteractive);
    }
}
impl Default for Palette {
    /// Preserve REforge's current Compact colours until starting themes ship.
    fn default() -> Self {
        let mut visuals = Visuals::dark();
        visuals.selection.bg_fill = Color32::from_rgb(43, 103, 131);
        let mut p = Self::from_visuals(&visuals);
        p.accent = Color32::from_rgb(100, 200, 226);
        p.focus = p.accent;
        p.override_text = p.accent;
        p.pending = Color32::from_rgb(245, 185, 90);
        p.cli_lock = Color32::from_rgb(182, 169, 214);
        p.cli_background = Color32::from_rgb(35, 58, 72);
        p.cli_text = Color32::from_rgb(190, 214, 228);
        p.lock_keyhole = Color32::from_gray(40);
        p.locked_text = Color32::from_gray(100);
        p.notice_error = Color32::from_rgb(255, 169, 144);
        p.inherited.foreground = mix(p.accent, p.inactive.foreground, 0.55);
        p.inherited.border = mix(p.accent, p.panel, 0.45);
        p
    }
}
fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let a = a.to_array();
    let b = b.to_array();
    let channel = |i| (a[i] as f32 * amount + b[i] as f32 * (1.0 - amount)).round() as u8;
    Color32::from_rgb(channel(0), channel(1), channel(2))
}
