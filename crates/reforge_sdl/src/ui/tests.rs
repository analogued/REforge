// Copyright (c) 2026 analogued. Project license has not yet been selected.
use super::*;
use egui::{Color32, Context, Event, Id, Modifiers, PointerButton, RawInput, Rect, Vec2};

#[test]
fn applying_palette_preserves_settings_geometry_and_focus() {
    let context = Context::default();
    crate::settings_ui::style(&context);
    let before = context.style();
    let focus = Id::new("draft-control");
    context.memory_mut(|memory| memory.request_focus(focus));
    let base = Theme::default();
    let custom = base.with_overrides(|theme| {
        theme.palette = Palette::from_visuals(&egui::Visuals::light());
        theme.palette.focus = Color32::GOLD;
        theme.components.button.shape = ButtonShape::Pill;
        theme.components.choices.shape = ChoiceShape::Sharp;
        theme.components.section.surface = Surface::Group;
        theme.components.section.separator = Separator::None;
    });
    custom.apply_palette(&context);
    let after = context.style();
    assert_eq!(after.spacing, before.spacing);
    assert_eq!(after.text_styles, before.text_styles);
    assert_eq!(
        after.visuals.window_corner_radius,
        before.visuals.window_corner_radius
    );
    assert_eq!(
        after.visuals.window_stroke.width,
        before.visuals.window_stroke.width
    );
    assert_eq!(
        after.visuals.window_shadow.offset,
        before.visuals.window_shadow.offset
    );
    assert_eq!(
        after.visuals.window_shadow.blur,
        before.visuals.window_shadow.blur
    );
    for (a, b) in [
        (
            &after.visuals.widgets.inactive,
            &before.visuals.widgets.inactive,
        ),
        (
            &after.visuals.widgets.hovered,
            &before.visuals.widgets.hovered,
        ),
        (
            &after.visuals.widgets.active,
            &before.visuals.widgets.active,
        ),
        (&after.visuals.widgets.open, &before.visuals.widgets.open),
        (
            &after.visuals.widgets.noninteractive,
            &before.visuals.widgets.noninteractive,
        ),
    ] {
        assert_eq!(a.corner_radius, b.corner_radius);
        assert_eq!(a.expansion, b.expansion);
        assert_eq!(a.bg_stroke.width, b.bg_stroke.width);
        assert_eq!(a.fg_stroke.width, b.fg_stroke.width);
    }
    assert_eq!(context.memory(|memory| memory.focused()), Some(focus));
    assert_eq!(after.visuals.panel_fill, custom.palette.panel);
    assert_eq!(current_theme(&context), custom);
    assert_eq!(base, Theme::default(), "variant must not mutate its base");
    // Settings reapplies its fixed style during frame preparation.
    crate::settings_ui::style(&context);
    assert_eq!(current_theme(&context), custom);
    assert_eq!(context.style().visuals.panel_fill, custom.palette.panel);
}

fn frame(
    context: &Context,
    theme: &Theme,
    state: SelectionState,
    events: Vec<Event>,
    enabled: bool,
) -> (egui::FullOutput, Option<u8>) {
    let mut chosen = None;
    let output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(400.0, 200.0),
            )),
            events,
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    chosen =
                        primitives::choice_strip(ui, theme, &[(0, "CGA"), (1, "PCjr")], &0, state);
                });
            });
        },
    );
    (output, chosen)
}

#[test]
fn every_choice_primitive_returns_clicks_including_active_values() {
    for shape in [
        ChoiceShape::Buttons,
        ChoiceShape::Flat,
        ChoiceShape::Sharp,
        ChoiceShape::Segmented,
        ChoiceShape::Pill,
    ] {
        for state in [SelectionState::Inherited, SelectionState::Explicit] {
            for (label, expected, enabled) in [
                ("CGA", Some(0), true),
                ("PCjr", Some(1), true),
                ("CGA", None, false),
            ] {
                let context = Context::default();
                let theme =
                    Theme::default().with_overrides(|theme| theme.components.choices.shape = shape);
                theme.apply_palette(&context);
                let (output, _) = frame(&context, &theme, state, vec![], enabled);
                if enabled {
                    let fill = match state {
                        SelectionState::Inherited => theme.palette.inherited.background,
                        _ => theme.palette.selection.background,
                    };
                    assert!(
                        output.shapes.iter().any(
                            |s| matches!(&s.shape, egui::epaint::Shape::Rect(r) if r.fill == fill)
                        ),
                        "selection colour: {shape:?} {state:?}"
                    );
                }
                let pos = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.visual_bounding_rect().center())
                        }
                        _ => None,
                    })
                    .unwrap();
                let event = |pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                frame(
                    &context,
                    &theme,
                    state,
                    vec![Event::PointerMoved(pos), event(true)],
                    enabled,
                );
                let (_, chosen) = frame(&context, &theme, state, vec![event(false)], enabled);
                assert_eq!(
                    chosen, expected,
                    "{shape:?} {state:?} {label} enabled={enabled}"
                );
                if enabled {
                    // Keyboard traversal establishes focus independently of
                    // pointer clicks (egui does not focus buttons on click).
                    for _ in 0..if label == "CGA" { 1 } else { 2 } {
                        for pressed in [true, false] {
                            frame(
                                &context,
                                &theme,
                                state,
                                vec![Event::Key {
                                    key: egui::Key::Tab,
                                    physical_key: None,
                                    pressed,
                                    repeat: false,
                                    modifiers: Modifiers::NONE,
                                }],
                                true,
                            );
                        }
                    }
                    let (_, keyboard) = frame(
                        &context,
                        &theme,
                        state,
                        vec![Event::Key {
                            key: egui::Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                        true,
                    );
                    assert_eq!(
                        keyboard, expected,
                        "keyboard activation: {shape:?} {state:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn independent_section_and_selection_primitives_render_theme_roles() {
    let theme = Theme::default().with_overrides(|theme| {
        theme.components.button.shape = ButtonShape::Pill;
        theme.components.choices.shape = ChoiceShape::Sharp;
        theme.components.section.surface = Surface::Group;
        theme.components.section.separator = Separator::None;
        theme.components.selection.indicator = SelectionIndicator::LeftBar;
        theme.palette.group = Color32::from_rgb(12, 34, 56);
        theme.palette.inherited.border = Color32::from_rgb(65, 43, 21);
    });
    let context = Context::default();
    theme.apply_palette(&context);
    let output = context.run(RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            primitives::section(ui, &theme, "Preview", |ui| {
                ui.add(primitives::Button::new(&theme, "Play"));
                primitives::choice_strip(
                    ui,
                    &theme,
                    &[(0, "CGA"), (1, "PCjr")],
                    &0,
                    SelectionState::Inherited,
                );
            });
            primitives::selection_marker(
                ui.painter(),
                &theme,
                Rect::from_min_size(egui::pos2(20.0, 120.0), egui::vec2(100.0, 30.0)),
                SelectionState::Inherited,
            );
        });
    });
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape, egui::epaint::Shape::Rect(r) if r.fill == theme.palette.group)
    ));
    assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::epaint::Shape::Rect(r) if r.fill == theme.palette.inherited.border && r.rect.width() == theme.components.selection.bar_width)));
}

#[test]
fn button_identity_and_focus_survive_pin_and_appearance_changes() {
    let context = Context::default();
    let mut first = None;
    for shape in [
        ButtonShape::Native,
        ButtonShape::Flat,
        ButtonShape::Sharp,
        ButtonShape::Rounded,
        ButtonShape::Pill,
    ] {
        for state in [
            SelectionState::Inherited,
            SelectionState::Explicit,
            SelectionState::Unselected,
        ] {
            let theme =
                Theme::default().with_overrides(|theme| theme.components.button.shape = shape);
            theme.apply_palette(&context);
            let _ = context.run(RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let before = ui.style().clone();
                    let response = ui.add(primitives::Button::new(&theme, "CGA").selection(state));
                    let initial = first.is_none();
                    let original = *first.get_or_insert(response.id);
                    assert_eq!(response.id, original, "{shape:?} {state:?}");
                    if initial {
                        response.request_focus();
                    } else {
                        assert!(response.has_focus(), "focus must follow the same control");
                    }
                    assert_eq!(
                        *ui.style(),
                        before,
                        "style must not leak to following controls"
                    );
                });
            });
            assert_eq!(context.memory(|memory| memory.focused()), first);
        }
    }
}

#[test]
fn joined_choices_keep_pixel_aligned_geometry_across_interaction_states() {
    for shape in [
        ChoiceShape::Sharp,
        ChoiceShape::Segmented,
        ChoiceShape::Pill,
    ] {
        for density in [1.0, 1.25, 1.5, 2.0, 2.25, 2.5, 3.0] {
            for offset in [0.0, 0.25, 0.5, 0.75] {
                let context = Context::default();
                let theme = Theme::default().with_overrides(|t| {
                    t.components.choices.shape = shape;
                    t.components.choices.gap = if shape == ChoiceShape::Sharp {
                        0.0
                    } else {
                        2.0
                    };
                    t.palette.extreme_background = Color32::from_rgb(1, 2, 3);
                    t.palette.selection.background = Color32::from_rgb(20, 60, 100);
                    t.palette.inherited.background = Color32::from_rgb(10, 30, 50);
                });
                let render = |events, state| {
                    let mut input = RawInput {
                        events,
                        ..Default::default()
                    };
                    input
                        .viewports
                        .entry(egui::ViewportId::ROOT)
                        .or_default()
                        .native_pixels_per_point = Some(density);
                    context.run(input, |context| {
                        theme.apply_palette(context);
                        egui::CentralPanel::default().show(context, |ui| {
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(Rect::from_min_size(
                                    egui::pos2(20.0 + offset, 20.0 + offset),
                                    Vec2::new(300.0, 60.0),
                                )),
                                |ui| {
                                    primitives::choice_strip(
                                        ui,
                                        &theme,
                                        &[(0, "CGA"), (1, "PCjr")],
                                        &0,
                                        state,
                                    );
                                },
                            );
                        });
                    })
                };
                let idle = render(vec![], SelectionState::Explicit);
                let text_pos = |out: &egui::FullOutput, label| {
                    out.shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.text() == label => {
                                Some(t.visual_bounding_rect().center())
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                let bounds = |out: &egui::FullOutput| {
                    out.shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            egui::Shape::Rect(r) if r.rect.min.x >= 19.0 && r.rect.max.y < 80.0 => {
                                Some(r.rect)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let baseline = bounds(&idle);
                for selected in [SelectionState::Explicit, SelectionState::Inherited] {
                    for label in ["CGA", "PCjr"] {
                        let pos = text_pos(&idle, label);
                        let hover = render(vec![Event::PointerMoved(pos)], selected);
                        assert_eq!(
                            bounds(&hover),
                            baseline,
                            "{shape:?} {density}: hover changed geometry"
                        );
                        let click = |pressed| Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        };
                        let held = render(vec![click(true)], selected);
                        assert_eq!(
                            bounds(&held),
                            baseline,
                            "{shape:?} {density}: press changed geometry"
                        );
                        render(vec![click(false)], selected);
                    }
                }
                let outer = baseline[0];
                for rect in baseline {
                    assert!(
                        (rect.center().y - outer.center().y).abs() < 0.001,
                        "{shape:?} {density}: asymmetric vertical insets"
                    );
                    assert!(outer.expand(0.001).contains_rect(rect));
                    for value in [rect.left(), rect.right(), rect.top(), rect.bottom()] {
                        let physical = value * density;
                        assert!(
                            (physical - physical.round()).abs() < 0.001,
                            "{shape:?} {density}: fractional painted edge {physical}"
                        );
                    }
                }
            }
        }
    }
}
