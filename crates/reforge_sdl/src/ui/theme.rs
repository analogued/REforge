// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Complete theme compositions, with an independent choice for each primitive.
use super::Palette;
use egui::{Context, Id, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionState {
    #[default]
    Unselected,
    Inherited,
    Explicit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonShape {
    #[default]
    Native,
    Flat,
    Rounded,
    Sharp,
    Pill,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChoiceShape {
    #[default]
    Buttons,
    Flat,
    Sharp,
    Segmented,
    Pill,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    #[default]
    Line,
    Space,
    None,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Surface {
    #[default]
    Transparent,
    Canvas,
    Panel,
    Group,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionIndicator {
    #[default]
    Fill,
    Outline,
    LeftBar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonAppearance {
    pub shape: ButtonShape,
    pub radius: u8,
    pub padding: Vec2,
    pub outline: bool,
}
impl Default for ButtonAppearance {
    fn default() -> Self {
        Self {
            shape: ButtonShape::Native,
            radius: 3,
            padding: Vec2::new(8.0, 3.0),
            outline: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChoiceAppearance {
    pub shape: ChoiceShape,
    pub radius: u8,
    pub padding: Vec2,
    pub gap: f32,
}
impl Default for ChoiceAppearance {
    fn default() -> Self {
        Self {
            shape: ChoiceShape::Buttons,
            radius: 3,
            padding: Vec2::new(8.0, 3.0),
            gap: 6.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionAppearance {
    pub surface: Surface,
    pub separator: Separator,
    pub padding: Vec2,
    pub radius: u8,
    pub gap: f32,
    pub border: bool,
}
impl Default for SectionAppearance {
    fn default() -> Self {
        Self {
            surface: Surface::Transparent,
            separator: Separator::Line,
            padding: Vec2::ZERO,
            radius: 3,
            gap: 8.0,
            border: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionAppearance {
    pub indicator: SelectionIndicator,
    pub radius: u8,
    pub bar_width: f32,
}
impl Default for SelectionAppearance {
    fn default() -> Self {
        Self {
            indicator: SelectionIndicator::Fill,
            radius: 3,
            bar_width: 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Components {
    pub button: ButtonAppearance,
    pub choices: ChoiceAppearance,
    pub section: SectionAppearance,
    pub selection: SelectionAppearance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Theme {
    pub palette: Palette,
    pub components: Components,
}
impl Theme {
    /// A future variant can override any fields without mutating its base.
    /// Each base theme remains free to choose a completely different composition.
    pub fn with_overrides(mut self, edit: impl FnOnce(&mut Self)) -> Self {
        edit(&mut self);
        self
    }
    /// Install colours and expose this composition to shared primitives. Existing
    /// egui spacing, radii, stroke widths, focus and application state are retained.
    /// Settings uses only this colour path; primitives read their own geometry.
    pub fn apply_palette(self, context: &Context) {
        context.style_mut(|style| self.palette.apply(&mut style.visuals));
        context.data_mut(|data| data.insert_temp(key(), self));
    }
}
fn key() -> Id {
    Id::new("reforge_ui_theme")
}
pub fn current_theme(context: &Context) -> Theme {
    context
        .data(|data| data.get_temp(key()))
        .unwrap_or_default()
}
pub fn current_palette(context: &Context) -> Palette {
    current_theme(context).palette
}
