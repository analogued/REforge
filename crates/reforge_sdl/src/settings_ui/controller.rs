// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Alley Cat's paired movement/button templates. The diagram is explanatory;
//! the two carousels participate in the normal settings navigation and draft.

use super::{RowContext, RowState, section};
use crate::{
    controller_art as art,
    gamepad::ControllerStyle,
    settings_navigation::{self, Kind},
};
use egui::{Align2, Color32, Key, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use reforge_settings::{
    CONTROLLER_ACTION, CONTROLLER_BUTTONS, CONTROLLER_MOVEMENT, ControllerButtons,
    ControllerMovement, FaceButton, RuntimeSettings,
};

pub(super) fn controls(
    ui: &mut Ui,
    value: &mut RuntimeSettings,
    rows: RowContext<'_>,
    style: ControllerStyle,
) {
    section(ui, "Controller layout");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(art::model_name(style)).size(12.0));
    });
    ui.columns(2, |columns| {
        let (left_column, right_column) = columns.split_at_mut(1);
        let left = &mut left_column[0];
        left.set_min_height(104.0);
        heading(left, "Movement", rows.state(CONTROLLER_MOVEMENT, value));
        let movement = &mut value.alleycat.controller_movement;
        carousel(
            left,
            CONTROLLER_MOVEMENT,
            movement,
            &ControllerMovement::ALL,
            |current| current.label().into(),
            |ui, rect, current, colour| {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    current.label(),
                    egui::TextStyle::Button.resolve(ui.style()),
                    colour,
                );
            },
        );
        detail(
            left,
            if *movement == ControllerMovement::Directional {
                "Up / down / left / right"
            } else {
                "Left / right"
            },
        );

        let right = &mut right_column[0];
        right.set_min_height(104.0);
        let directional = value.alleycat.controller_movement == ControllerMovement::Directional;
        let key = if directional {
            CONTROLLER_ACTION
        } else {
            CONTROLLER_BUTTONS
        };
        heading(right, "Button layout", rows.state(key, value));
        if directional {
            let action = &mut value.alleycat.controller_action;
            carousel(
                right,
                key,
                action,
                &FaceButton::ALL,
                |button| format!("{} Action", art::button_name(style, button)),
                |ui, rect, button, colour| {
                    art::glyph(
                        ui,
                        Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                        style,
                        button,
                        colour,
                    );
                },
            );
            detail(
                right,
                &format!("{} Action", art::button_name(style, *action)),
            );
        } else {
            let buttons = &mut value.alleycat.controller_buttons;
            carousel(
                right,
                key,
                buttons,
                &ControllerButtons::ALL,
                |layout| {
                    let (jump, action) = layout.binding();
                    format!(
                        "{} Jump / Drop, {} Action",
                        art::button_name(style, jump),
                        art::button_name(style, action)
                    )
                },
                |ui, rect, layout, colour| {
                    let (jump, action) = layout.binding();
                    let other = if jump == FaceButton::South {
                        action
                    } else {
                        jump
                    };
                    for (button, dx) in [(other, -17.0), (FaceButton::South, 17.0)] {
                        art::glyph(
                            ui,
                            Rect::from_center_size(
                                rect.center() + egui::vec2(dx, 0.0),
                                Vec2::splat(20.0),
                            ),
                            style,
                            button,
                            colour,
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "·",
                        egui::TextStyle::Button.resolve(ui.style()),
                        colour,
                    );
                },
            );
            let (jump, action) = buttons.binding();
            let other = if jump == FaceButton::South {
                action
            } else {
                jump
            };
            for button in [other, FaceButton::South] {
                detail(
                    right,
                    &format!(
                        "{} {}",
                        art::button_name(style, button),
                        if button == jump {
                            "Jump / Drop"
                        } else {
                            "Action"
                        }
                    ),
                );
            }
        }
    });
    let settings = value.alleycat;
    let (jump, action) = if settings.controller_movement == ControllerMovement::Directional {
        (None, settings.controller_action)
    } else {
        let (jump, action) = settings.controller_buttons.binding();
        (Some(jump), action)
    };
    let height = (ui.clip_rect().height() - 220.0).clamp(110.0, 340.0);
    art::drawing(
        ui,
        style,
        settings.controller_movement,
        action,
        jump,
        height,
    );
    detail(
        ui,
        if jump.is_some() {
            "Hold Down and press Jump to drop."
        } else {
            "Up to jump. Down to drop."
        },
    );
    detail(ui, "Swimming always uses all four directions.");
}

fn detail(ui: &mut Ui, text: &str) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0)).wrap());
    });
}

fn heading(ui: &mut Ui, text: &str, state: RowState) {
    let palette = crate::ui::current_palette(ui.ctx());
    let colour = if state.overridden {
        palette.override_text
    } else {
        ui.visuals().text_color()
    };
    let response = ui.add_sized(
        [ui.available_width(), 20.0],
        egui::Label::new(egui::RichText::new(text).color(colour)),
    );
    if state.dirty {
        ui.painter().circle_filled(
            response.rect.right_center() - egui::vec2(5.0, 0.0),
            3.0,
            palette.pending,
        );
    }
    response.on_hover_text(state.tooltip);
}

fn carousel<T: Copy + PartialEq>(
    ui: &mut Ui,
    key: &str,
    current: &mut T,
    choices: &[T],
    label: impl Fn(T) -> String,
    paint: impl Fn(&Ui, Rect, T, Color32),
) -> Response {
    let id = ui.make_persistent_id(key);
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 24.0));
    let mut response = ui.interact(rect, id, Sense::click());
    let mut step: isize = 0;
    if response.clicked() && ui.input(|input| input.pointer.any_released()) {
        response.request_focus();
        step = if response
            .interact_pointer_pos()
            .is_some_and(|p| p.x < rect.center().x)
        {
            -1
        } else {
            1
        };
    }
    if ui.is_enabled() && response.has_focus() {
        let mut owned = settings_navigation::take_selector_events(ui.ctx(), id);
        ui.input_mut(|input| {
            input.events.append(&mut owned);
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
                let delta = match key {
                    Key::ArrowLeft => -1,
                    Key::ArrowRight | Key::Enter | Key::Space => 1,
                    _ => return true,
                };
                if *pressed {
                    step += delta;
                }
                false
            });
        });
    }
    if step != 0 && ui.is_enabled() {
        let index = choices
            .iter()
            .position(|choice| choice == current)
            .unwrap_or(0);
        *current = choices[(index as isize + step).rem_euclid(choices.len() as isize) as usize];
        response.mark_changed();
    }
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        visuals.corner_radius,
        visuals.bg_fill,
        visuals.bg_stroke,
        StrokeKind::Inside,
    );
    let colour = visuals.text_color();
    paint(ui, rect.shrink2(egui::vec2(21.0, 0.0)), *current, colour);
    for (x, direction) in [(rect.left() + 9.0, -1.0), (rect.right() - 9.0, 1.0)] {
        let center = egui::pos2(x, rect.center().y);
        ui.painter().add(egui::Shape::line(
            vec![
                center + egui::vec2(-direction * 2.0, -3.0),
                center + egui::vec2(direction * 2.0, 0.0),
                center + egui::vec2(-direction * 2.0, 3.0),
            ],
            Stroke::new(1.0_f32, colour),
        ));
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ComboBox,
            response.enabled(),
            label(*current),
        )
    });
    settings_navigation::value_control(response, Kind::Selector)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        context: egui::Context,
        navigation: settings_navigation::Navigation,
        current: usize,
        response: Option<Response>,
        enabled: bool,
    }
    impl Harness {
        fn new() -> Self {
            let context = egui::Context::default();
            super::super::style(&context);
            let mut harness = Self {
                context,
                navigation: Default::default(),
                current: 0,
                response: None,
                enabled: true,
            };
            harness.frame(vec![]);
            harness.frame(vec![]);
            harness
        }
        fn frame(&mut self, mut events: Vec<egui::Event>) {
            self.navigation.navigate(&self.context, &mut events);
            let _ = self.context.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    settings_navigation::begin_frame(context);
                    egui::CentralPanel::default().show(context, |ui| {
                        ui.add_enabled_ui(self.enabled, |ui| {
                            self.response = Some(carousel(
                                ui,
                                "template",
                                &mut self.current,
                                &[0, 1, 2, 3],
                                |v| v.to_string(),
                                |ui, rect, v, colour| {
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        v.to_string(),
                                        egui::TextStyle::Button.resolve(ui.style()),
                                        colour,
                                    );
                                },
                            ));
                        });
                    });
                },
            );
        }
        fn key(&mut self, key: Key) {
            self.navigation
                .set_mode(&self.context, crate::settings_selector::InputMode::Keyboard);
            for pressed in [true, false] {
                self.frame(vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
        }
        fn click(&mut self, right: bool) {
            let rect = self.response.as_ref().unwrap().rect;
            let pos = egui::pos2(
                if right {
                    rect.right() - 8.0
                } else {
                    rect.left() + 8.0
                },
                rect.center().y,
            );
            self.navigation
                .set_mode(&self.context, crate::settings_selector::InputMode::Mouse);
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
        }
    }

    #[test]
    fn template_carousel_changes_once_per_mouse_keyboard_or_controller_activation() {
        let mut h = Harness::new();
        let rect = h.response.as_ref().unwrap().rect;
        h.click(true);
        assert_eq!(h.current, 1);
        for (key, value) in [
            (Key::Enter, 2),
            (Key::Space, 3),
            (Key::ArrowRight, 0),
            (Key::ArrowLeft, 3),
        ] {
            h.key(key);
            assert_eq!(h.current, value, "{key:?} should move exactly one position");
            assert_eq!(h.response.as_ref().unwrap().rect, rect);
        }
        h.navigation
            .set_mode(&h.context, crate::settings_selector::InputMode::Controller);
        h.navigation.controller(
            crate::gamepad::PadEvent {
                pressed: crate::gamepad::Actions {
                    action: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            0.0,
        );
        let mut events = Vec::new();
        h.navigation.take_events(&mut events);
        h.frame(events);
        assert_eq!(h.current, 0);
        h.click(false);
        assert_eq!(h.current, 3);
        assert!(!h.context.memory(|memory| memory.any_popup_open()));
        h.enabled = false;
        h.key(Key::Enter);
        h.click(true);
        assert_eq!(h.current, 3, "disabled templates must not change");
    }
}

#[cfg(test)]
mod captures;
