// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! One compact combo identity and layout, with a navigation-oriented presentation.

use egui::{Color32, Id, Key, Response, Ui};

use crate::settings_navigation::{self, Kind};

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(super) enum InputMode {
    #[default]
    Mouse,
    Keyboard,
    Controller,
}
fn mode_key() -> Id {
    Id::new("reforge_settings_input_mode")
}
pub(super) fn set_mode(context: &egui::Context, mode: InputMode) {
    if mode != InputMode::Mouse && context.memory(|m| m.any_popup_open()) {
        settings_navigation::close_popup(context);
    }
    context.data_mut(|data| data.insert_temp(mode_key(), mode));
}
fn mode(ui: &Ui) -> InputMode {
    ui.ctx()
        .data(|data| data.get_temp(mode_key()).unwrap_or_default())
}

pub(super) struct Choice<T> {
    value: T,
    label: String,
    unavailable: Option<&'static str>,
}
impl<T> Choice<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            unavailable: None,
        }
    }
    pub fn enabled(mut self, enabled: bool, reason: &'static str) -> Self {
        if !enabled {
            self.unavailable = Some(reason);
        }
        self
    }
}

pub(super) struct Selector<T> {
    salt: Id,
    choices: Vec<Choice<T>>,
    selected_text: Option<String>,
    truncate: bool,
}
impl<T: Clone + PartialEq> Selector<T> {
    pub fn new(salt: impl std::hash::Hash, choices: impl IntoIterator<Item = Choice<T>>) -> Self {
        Self {
            salt: Id::new(salt),
            choices: choices.into_iter().collect(),
            selected_text: None,
            truncate: false,
        }
    }
    pub fn selected_text(mut self, text: impl Into<String>) -> Self {
        self.selected_text = Some(text.into());
        self
    }
    pub fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }
    pub fn show(self, ui: &mut Ui, current: &mut T) -> Response {
        // ComboBox hashes its salt before making the persistent widget ID.
        let id = ui.make_persistent_id(Id::new(self.salt));
        let carousel = mode(ui) != InputMode::Mouse;
        let mut owned_events = settings_navigation::take_selector_events(ui.ctx(), id);
        let mut changed = false;
        if carousel && ui.is_enabled() && ui.memory(|m| m.has_focus(id)) {
            ui.input_mut(|input| {
                input.events.append(&mut owned_events);
                input.events.retain(|event| {
                    let egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } = event
                    else {
                        return true;
                    };
                    if modifiers.alt || modifiers.ctrl || modifiers.command {
                        return true;
                    }
                    let step = match key {
                        Key::ArrowLeft => -1,
                        Key::ArrowRight | Key::Enter | Key::Space => 1,
                        _ => return true,
                    };
                    if *pressed {
                        let available: Vec<_> = self
                            .choices
                            .iter()
                            .filter(|c| c.unavailable.is_none())
                            .collect();
                        if !available.is_empty() {
                            let selected = available.iter().position(|c| c.value == *current);
                            let next = match selected {
                                Some(index) => (index as isize + step)
                                    .rem_euclid(available.len() as isize)
                                    as usize,
                                None if step < 0 => available.len() - 1,
                                None => 0,
                            };
                            changed |= *current != available[next].value;
                            *current = available[next].value.clone();
                        }
                    }
                    false
                });
            });
        }
        let label = if changed { None } else { self.selected_text }
            .or_else(|| {
                self.choices
                    .iter()
                    .find(|c| c.value == *current)
                    .map(|c| c.label.clone())
            })
            .unwrap_or_default();
        let width = ui.spacing().combo_width.min(ui.available_width()).max(1.0);
        let mut combo = egui::ComboBox::from_id_salt(self.salt).width(width);
        if self.truncate || width < 220.0 {
            combo = combo.truncate();
        }
        // Use the same egui widget in both modes: sizing, ID, hit testing,
        // disabled appearance and the press/release gesture remain identical.
        // Hide only its text/icon, then draw the carousel inside its actual rect.
        combo = if carousel {
            combo
                .selected_text(egui::RichText::new(&label).color(Color32::TRANSPARENT))
                .icon(|_, _, _, _, _| {})
        } else {
            combo.selected_text(&label)
        };
        let mut response = combo
            .show_ui(ui, |ui| {
                settings_navigation::popup(ui, id, |ui| {
                    for choice in self.choices {
                        if let Some(reason) = choice.unavailable {
                            ui.add_enabled_ui(false, |ui| {
                                ui.selectable_value(current, choice.value, choice.label)
                                    .on_disabled_hover_text(reason);
                            });
                        } else {
                            ui.selectable_value(current, choice.value, choice.label);
                        }
                    }
                });
            })
            .response;
        if carousel {
            // A pointer-down selects mouse mode before this widget receives it.
            // Navigation activation is consumed above and cannot open a popup.
            let visuals = ui.style().interact(&response);
            let inset = ui.spacing().button_padding.x;
            let icon = ui.spacing().icon_width;
            let rect = response.rect;
            let painter = ui.painter().with_clip_rect(
                ui.clip_rect()
                    .intersect(rect.shrink2(egui::vec2(inset + icon, 0.0))),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &label,
                egui::TextStyle::Button.resolve(ui.style()),
                ui.visuals()
                    .override_text_color
                    .unwrap_or(visuals.text_color()),
            );
            for (x, direction) in [
                (rect.left() + inset + icon * 0.5, -1.0),
                (rect.right() - inset - icon * 0.5, 1.0),
            ] {
                let center = egui::pos2(x, rect.center().y);
                ui.painter().add(egui::Shape::line(
                    vec![
                        center + egui::vec2(-direction * 2.0, -3.0),
                        center + egui::vec2(direction * 2.0, 0.0),
                        center + egui::vec2(-direction * 2.0, 3.0),
                    ],
                    egui::Stroke::new(1.0_f32, visuals.text_color()),
                ));
            }
        }
        if changed {
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, response.enabled(), &label)
        });
        settings_navigation::value_control(response, Kind::Selector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Modifiers, Pos2, RawInput, Rect, Vec2};

    struct Harness {
        context: Context,
        nav: settings_navigation::Navigation,
        value: i32,
        response: Option<Response>,
        time: f64,
        disabled: bool,
    }
    impl Harness {
        fn new() -> Self {
            let context = Context::default();
            crate::settings_ui::style(&context);
            let mut h = Self {
                context,
                nav: Default::default(),
                value: 0,
                response: None,
                time: 0.0,
                disabled: false,
            };
            for _ in 0..4 {
                h.frame(vec![]);
            }
            h
        }
        fn frame(&mut self, mut events: Vec<Event>) {
            self.time += 0.016;
            self.nav.navigate(&self.context, &mut events);
            let _ = self.context.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 650.0))),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    settings_navigation::begin_frame(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(!self.disabled, |ui| {
                            self.response = Some(
                                Selector::new(
                                    "authored selector",
                                    [
                                        Choice::new(0, "Zero"),
                                        Choice::new(1, "Unavailable")
                                            .enabled(false, "Authored disabled choice"),
                                        Choice::new(2, "Two"),
                                    ],
                                )
                                .show(ui, &mut self.value),
                            );
                        });
                        settings_navigation::control(ui.button("Next control"));
                    });
                },
            );
        }
        fn mode(&mut self, mode: InputMode) {
            self.nav.set_mode(&self.context, mode);
        }
        fn focus(&self) {
            self.response.as_ref().unwrap().request_focus();
        }
        fn key(&mut self, key: Key) {
            self.mode(InputMode::Keyboard);
            self.frame(
                [true, false]
                    .into_iter()
                    .map(|pressed| Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    })
                    .collect(),
            );
        }
        fn pointer(&mut self, pos: Pos2, pressed: bool) {
            if pressed {
                self.mode(InputMode::Mouse);
            }
            self.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
    }
    #[test]
    fn compact_dimensions_and_identity_match_stock_combo_in_both_modes() {
        let mut h = Harness::new();
        let original = h.response.clone().unwrap();
        for mode in [InputMode::Keyboard, InputMode::Controller, InputMode::Mouse] {
            h.mode(mode);
            h.focus();
            h.frame(vec![]);
            let current = h.response.as_ref().unwrap();
            assert_eq!(current.id, original.id);
            assert_eq!(current.rect, original.rect);
            assert_eq!(h.context.memory(|m| m.focused()), Some(original.id));
        }
        let mut stock = None;
        let _ = h.context.run(RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                stock = Some(
                    egui::ComboBox::from_id_salt("stock")
                        .width(220.0)
                        .selected_text("Zero")
                        .show_ui(ui, |_| {})
                        .response,
                );
            });
        });
        assert_eq!(stock.unwrap().rect.size(), original.rect.size());
        assert_eq!(original.rect.width(), 220.0);
        assert!(
            original.rect.height() <= 24.0,
            "preserve Compact's 22–24 point controls"
        );
    }
    #[test]
    fn carousel_retains_the_settings_override_text_colour() {
        let context = Context::default();
        crate::settings_ui::style(&context);
        let accent = Color32::from_rgb(100, 200, 226);
        context.style_mut(|style| style.visuals.override_text_color = Some(accent));
        set_mode(&context, InputMode::Keyboard);
        let output = context.run(RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                Selector::new("coloured selector", [Choice::new(0, "Inherited override")])
                    .show(ui, &mut 0);
            });
        });
        assert!(
            output.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Inherited override" => {
                    text.galley
                        .job
                        .sections
                        .iter()
                        .any(|section| section.format.color == accent)
                }
                _ => false,
            }),
            "the carousel must retain the row's override colour"
        );
    }

    #[test]
    fn navigation_cycles_and_skips_disabled_choices_without_opening_popup() {
        let mut h = Harness::new();
        h.focus();
        for (key, expected) in [
            (Key::ArrowRight, 2),
            (Key::ArrowRight, 0),
            (Key::ArrowLeft, 2),
            (Key::Enter, 0),
            (Key::Space, 2),
        ] {
            h.key(key);
            assert_eq!(h.value, expected);
            assert!(!h.context.memory(|m| m.any_popup_open()));
            assert!(h.response.as_ref().unwrap().has_focus());
        }
        h.key(Key::Tab);
        assert_ne!(
            h.context.memory(|m| m.focused()),
            Some(h.response.as_ref().unwrap().id)
        );
    }
    #[test]
    fn single_mouse_click_switches_and_opens_dropdown_and_navigation_cancels_it() {
        let mut h = Harness::new();
        h.mode(InputMode::Controller);
        h.frame(vec![]);
        let anchor = h.response.as_ref().unwrap().clone();
        h.pointer(anchor.rect.center(), true);
        assert!(!h.context.memory(|m| m.any_popup_open()));
        h.pointer(anchor.rect.center(), false);
        assert!(h.context.memory(|m| m.any_popup_open()));
        assert_eq!(h.response.as_ref().unwrap().rect, anchor.rect);
        h.key(Key::ArrowRight);
        assert!(!h.context.memory(|m| m.any_popup_open()));
        assert_eq!(
            h.value, 2,
            "the switching key edits the owning field, not a popup item"
        );
        assert_eq!(h.context.memory(|m| m.focused()), Some(anchor.id));
        h.pointer(anchor.rect.center(), true);
        h.pointer(Pos2::new(800.0, 600.0), false);
        assert!(
            !h.context.memory(|m| m.any_popup_open()),
            "release outside must cancel"
        );
    }
    #[test]
    fn disabled_selector_cannot_change_or_open_in_either_mode() {
        let mut h = Harness::new();
        h.focus();
        h.disabled = true;
        h.key(Key::ArrowRight);
        h.key(Key::Enter);
        assert_eq!(h.value, 0);
        let pos = h.response.as_ref().unwrap().rect.center();
        h.pointer(pos, true);
        h.pointer(pos, false);
        assert!(!h.context.memory(|m| m.any_popup_open()));
        assert_eq!(h.value, 0);
        assert!(!h.response.as_ref().unwrap().enabled());
    }
}
