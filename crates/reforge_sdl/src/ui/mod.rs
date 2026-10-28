// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Shared appearance definitions and native egui primitives.
//!
//! A theme owns a complete palette and independently chosen component styles.
//! Screens own layout and preference semantics; appearance does not save values.

mod palette;
pub mod primitives;
mod theme;

#[cfg(test)]
mod tests;

pub use palette::{ChoiceColours, Palette, WidgetColours};
pub use primitives::ButtonActivation;
pub use theme::{
    ButtonAppearance, ButtonShape, ChoiceAppearance, ChoiceShape, Components, SectionAppearance,
    SelectionAppearance, SelectionIndicator, SelectionState, Separator, Surface, Theme,
    current_palette, current_theme,
};
