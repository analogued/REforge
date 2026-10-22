// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Host menu navigation. Controller repeat uses host UI time, never game time.

use egui::{Context, Event, Id, Key, Modifiers, Response, Ui, WidgetText};

use crate::gamepad::PadEvent;

const DIRECTIONS: [Key; 4] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
];
const FIRST_REPEAT: f64 = 0.35;
const REPEAT: f64 = 0.08;

#[derive(Default)]
pub(super) struct Navigation {
    held: u8,
    repeat: [f64; 4],
    events: Vec<Event>,
    deferred: Vec<Event>,
    pub focus_first: bool,
}

fn pulse(events: &mut Vec<Event>, key: Key, shift: bool, repeat: bool) {
    for pressed in [true, false] {
        events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: repeat && pressed,
            modifiers: Modifiers {
                shift,
                ..Modifiers::NONE
            },
        });
    }
}

impl Navigation {
    pub fn navigate(&mut self, context: &Context, events: &mut Vec<Event>) {
        if context.memory(|memory| memory.any_popup_open()) {
            if events.iter().any(|event| {
                matches!(
                    event,
                    Event::MouseWheel { .. } | Event::PointerButton { pressed: true, .. }
                )
            }) {
                context.data_mut(|data| data.insert_temp(reveal_key(), false));
            }
            return;
        }
        let mut targets = context
            .data(|data| data.get_temp::<Targets>(targets_key()).unwrap_or_default())
            .0;
        if let Some(root) = targets.first().map(|target| target.layer) {
            targets.retain(|target| target.layer == root);
        }
        let mut input = std::mem::take(&mut self.deferred);
        input.append(events);
        let mut focused = context.memory(|memory| memory.focused());
        let initial_focus = focused;
        if let Some(target) = targets.iter().find(|target| Some(target.id) == focused) {
            context.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    target.id,
                    egui::EventFilter {
                        horizontal_arrows: matches!(target.kind, Kind::Slider | Kind::Number),
                        vertical_arrows: matches!(target.kind, Kind::Number),
                        ..Default::default()
                    },
                )
            });
        }
        for event in input {
            if matches!(
                &event,
                Event::MouseWheel { .. } | Event::PointerButton { pressed: true, .. }
            ) {
                context.data_mut(|data| data.insert_temp(reveal_key(), false));
            }
            let Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } = &event
            else {
                events.push(event);
                continue;
            };
            let arrow = matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            );
            if !arrow && *key != Key::Tab {
                events.push(event);
                continue;
            }
            context.data_mut(|data| data.insert_temp(reveal_key(), true));
            if targets.is_empty() {
                self.deferred.push(event);
                continue;
            }
            let current = targets.iter().position(|target| Some(target.id) == focused);
            // Widget-owned arrows still adjust sliders, values and text carets.
            if arrow
                && current.is_some_and(|index| match targets[index].kind {
                    Kind::Slider => matches!(key, Key::ArrowLeft | Key::ArrowRight),
                    Kind::Number => true,
                    Kind::Control => false,
                })
            {
                // Do not apply a value edit to a control first reached by an
                // earlier direction in this same input batch. Its egui focus
                // filter will be established before the next batch instead.
                if focused == initial_focus {
                    events.push(event);
                }
                continue;
            }
            if !pressed {
                continue;
            }
            let next = if *key == Key::Tab {
                let step = if modifiers.shift { -1 } else { 1 };
                let index = current.unwrap_or(if modifiers.shift {
                    0
                } else {
                    targets.len() - 1
                });
                Some((index as isize + step).rem_euclid(targets.len() as isize) as usize)
            } else if let Some(index) = current {
                directional_target(&targets, index, *key).or(Some(index))
            } else {
                Some(0)
            };
            if let Some(index) = next {
                focused = Some(targets[index].id);
                context.memory_mut(|memory| memory.request_focus(targets[index].id));
            }
        }
    }
    pub fn take_events(&mut self, events: &mut Vec<Event>) {
        events.append(&mut self.events);
    }
    pub fn controller(&mut self, pad: PadEvent, now: f64) {
        // Opposing directions cancel for menu navigation only.
        let a = pad.actions;
        let held = u8::from(a.up && !a.down)
            | (u8::from(a.down && !a.up) << 1)
            | (u8::from(a.left && !a.right) << 2)
            | (u8::from(a.right && !a.left) << 3);
        for (index, key) in DIRECTIONS.into_iter().enumerate() {
            if held & !self.held & (1 << index) != 0 {
                pulse(&mut self.events, key, false, false);
                self.repeat[index] = now + FIRST_REPEAT;
            }
        }
        self.held = held;
        if pad.pressed.action {
            pulse(&mut self.events, Key::Enter, false, false);
        }
        if pad.previous != pad.next {
            pulse(&mut self.events, Key::Tab, pad.previous, false);
        }
    }

    pub fn drain(&mut self, now: f64, events: &mut Vec<Event>) {
        for (index, key) in DIRECTIONS.into_iter().enumerate() {
            if self.held & (1 << index) != 0 && now >= self.repeat[index] {
                // A host stall must not cause a burst of menu adjustments.
                pulse(&mut self.events, key, false, true);
                self.repeat[index] = now + REPEAT;
            }
        }
        events.append(&mut self.events);
    }
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Control,
    Slider,
    Number,
}
#[derive(Clone)]
struct Target {
    id: Id,
    rect: egui::Rect,
    visible: egui::Rect,
    layer: egui::LayerId,
    kind: Kind,
}
#[derive(Clone, Default)]
struct Targets(Vec<Target>);
fn targets_key() -> Id {
    Id::new("reforge_settings_controls")
}
pub(super) fn begin_frame(context: &Context) {
    context.data_mut(|data| data.insert_temp(targets_key(), Targets::default()));
}
pub(super) fn control(response: Response) -> Response {
    value_control(response, Kind::Control)
}
pub(super) fn value_control(response: Response, kind: Kind) -> Response {
    if response.enabled() && response.sense.is_focusable() {
        response.ctx.data_mut(|data| {
            let targets = data.get_temp_mut_or_default::<Targets>(targets_key());
            targets.0.retain(|target| target.id != response.id);
            targets.0.push(Target {
                id: response.id,
                rect: response.rect,
                visible: response.interact_rect,
                layer: response.layer_id,
                kind,
            });
        });
    }
    response
}
pub(super) fn combo<R>(response: egui::InnerResponse<Option<R>>) -> egui::InnerResponse<Option<R>> {
    control(response.response.clone());
    response
}
pub(super) fn collapsing<R>(response: egui::CollapsingResponse<R>) -> egui::CollapsingResponse<R> {
    control(response.header_response.clone());
    response
}
fn directional_target(targets: &[Target], current: usize, key: Key) -> Option<usize> {
    let source = &targets[current];
    let vertical = matches!(key, Key::ArrowUp | Key::ArrowDown);
    let sign = if matches!(key, Key::ArrowUp | Key::ArrowLeft) {
        -1.0
    } else {
        1.0
    };
    targets
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if index == current || candidate.layer != source.layer {
                return None;
            }
            let delta = candidate.rect.center() - source.rect.center();
            let (forward, cross) = if vertical {
                (delta.y * sign, delta.x.abs())
            } else {
                (delta.x * sign, delta.y.abs())
            };
            if forward <= 0.5 || cross > forward {
                return None;
            }
            Some((index, forward + cross * 2.0))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

#[derive(Clone, Default)]
struct Popup {
    items: Vec<Id>,
    anchor: Option<Id>,
}

fn anchor_key() -> Id {
    Id::new("reforge_settings_popup_anchor")
}

fn active_popup_key() -> Id {
    Id::new("reforge_settings_active_popup")
}

/// Resolve popup navigation before egui's spatial focus search. Otherwise its
/// end-of-frame arrow handling can move focus out of the list after our choice.
pub(super) fn filter_popup_input(context: &Context, events: &mut Vec<Event>) {
    if !context.memory(|memory| memory.any_popup_open()) {
        return;
    }
    let Some(state) = context.data(|data| data.get_temp::<Popup>(active_popup_key())) else {
        return;
    };
    let focused = context.memory(|memory| memory.focused());
    let Some(mut index) = state.items.iter().position(|id| Some(*id) == focused) else {
        return;
    };
    events.retain(|event| {
        let Event::Key {
            key,
            pressed,
            modifiers,
            ..
        } = event
        else {
            return true;
        };
        let step = match key {
            Key::ArrowDown | Key::ArrowRight => 1,
            Key::ArrowUp | Key::ArrowLeft => -1,
            Key::Tab => {
                if modifiers.shift {
                    -1
                } else {
                    1
                }
            }
            Key::Home | Key::End => 0,
            _ => return true,
        };
        if *pressed {
            context.data_mut(|data| data.insert_temp(reveal_key(), true));
            index = match key {
                Key::Home => 0,
                Key::End => state.items.len() - 1,
                _ => (index as isize + step).rem_euclid(state.items.len() as isize) as usize,
            };
        }
        false
    });
    if Some(state.items[index]) != focused {
        context.memory_mut(|memory| memory.request_focus(state.items[index]));
    }
}

pub(super) fn close_popup(context: &Context) {
    context.memory_mut(|memory| memory.close_popup());
    if let Some(anchor) = context.data(|data| data.get_temp::<Id>(anchor_key())) {
        context.memory_mut(|memory| memory.request_focus(anchor));
    }
}

/// Add keyboard focus and selection semantics to egui's mouse-oriented combos.
/// The wrapper still uses the ordinary egui choices and paints no extra widgets.
pub(super) struct Choices<'a> {
    ui: &'a mut Ui,
    items: Vec<(Id, bool)>,
    chosen: bool,
}

impl std::ops::Deref for Choices<'_> {
    type Target = Ui;
    fn deref(&self) -> &Ui {
        self.ui
    }
}
impl std::ops::DerefMut for Choices<'_> {
    fn deref_mut(&mut self) -> &mut Ui {
        self.ui
    }
}
impl Choices<'_> {
    pub fn selectable_value<T: PartialEq>(
        &mut self,
        current: &mut T,
        value: T,
        text: impl Into<WidgetText>,
    ) -> Response {
        let selected = *current == value;
        let response = control(self.ui.selectable_value(current, value, text));
        if response.enabled() {
            self.items.push((response.id, selected));
        }
        self.chosen |= response.clicked();
        response
    }
}

pub(super) fn popup<R>(ui: &mut Ui, contents: impl FnOnce(&mut Choices<'_>) -> R) -> R {
    let context = ui.ctx().clone();
    let key = ui.layer_id().id.with("reforge_navigation");
    let mut state = context.data_mut(|data| data.get_temp::<Popup>(key).unwrap_or_default());
    let focused = context.memory(|memory| memory.focused());
    let inside = focused.is_some_and(|id| state.items.contains(&id));
    if !inside {
        state.anchor = focused;
    }
    let mut choices = Choices {
        ui,
        items: Vec::new(),
        chosen: false,
    };
    let first = scroll_start(&context);
    let result = contents(&mut choices);
    if !inside && !choices.chosen {
        if let Some((id, _)) = choices
            .items
            .iter()
            .find(|(_, selected)| *selected)
            .or_else(|| choices.items.first())
        {
            context.memory_mut(|memory| memory.request_focus(*id));
        }
    }
    state.items = choices.items.iter().map(|(id, _)| *id).collect();
    if let Some(anchor) = state.anchor {
        context.data_mut(|data| data.insert_temp(anchor_key(), anchor));
    }
    if choices.chosen {
        close_popup(&context);
    }
    scroll_focus(choices.ui, first);
    context.data_mut(|data| {
        data.insert_temp(active_popup_key(), state.clone());
        data.insert_temp(key, state);
    });
    result
}

fn reveal_key() -> Id {
    Id::new("reforge_settings_reveal_focus")
}
pub(super) fn scroll_start(context: &Context) -> usize {
    context.data(|data| {
        data.get_temp::<Targets>(targets_key())
            .unwrap_or_default()
            .0
            .len()
    })
}
pub(super) fn scroll_focus(ui: &Ui, first: usize) {
    let focused = ui.memory(|memory| memory.focused());
    let key = ui.id().with("reforge_scrolled_focus");
    let changed = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<Option<Id>>(key).flatten();
        data.insert_temp(key, focused);
        previous != focused
    });
    // Geometry overlap does not prove membership: scrolled content can overlap
    // a header/footer in virtual coordinates. Only this area's controls count.
    let belongs = ui.ctx().data(|data| {
        data.get_temp::<Targets>(targets_key())
            .is_some_and(|targets| {
                targets
                    .0
                    .iter()
                    .skip(first)
                    .any(|target| Some(target.id) == focused)
            })
    });
    if belongs {
        let target = ui.ctx().data(|data| {
            data.get_temp::<Targets>(targets_key()).and_then(|targets| {
                targets
                    .0
                    .into_iter()
                    .find(|target| Some(target.id) == focused)
            })
        });
        if let Some(target) = target {
            let clipped = !ui.clip_rect().contains_rect(target.rect);
            let reveal = ui
                .ctx()
                .data(|data| data.get_temp::<bool>(reveal_key()).unwrap_or(false));
            if changed || (clipped && reveal) {
                ui.scroll_to_rect_animation(
                    target.rect,
                    None,
                    egui::style::ScrollAnimation::none(),
                );
                if clipped {
                    ui.ctx().request_discard("Reveal focused settings control");
                }
            }
        }
    }
}

pub(super) fn paint_focus(context: &Context) {
    let focused = context.memory(|memory| memory.focused());
    if let Some(target) = context.data(|data| {
        data.get_temp::<Targets>(targets_key()).and_then(|targets| {
            targets
                .0
                .into_iter()
                .find(|target| Some(target.id) == focused)
        })
    }) {
        if target.visible.is_positive() {
            context.layer_painter(target.layer).rect_stroke(
                target.visible.expand(2.0),
                3.0,
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 200, 226)),
                egui::StrokeKind::Outside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamepad::Actions;

    #[test]
    fn rapid_stick_navigation_keeps_focus_on_a_visible_enabled_control() {
        const NAME: &str = "settings_navigation::tests::rapid_stick_navigation_keeps_focus_on_a_visible_enabled_control";
        if std::env::var("REFORGE_RAPID_NAV_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_RAPID_NAV_TEST", NAME)
                .env("SDL_VIDEO_DRIVER", "dummy")
                .env("SDL_VIDEODRIVER", "dummy")
                .env("SDL_RENDER_DRIVER", "software")
                .env("SDL_AUDIO_DRIVER", "dummy")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use crate::settings::{SettingsMenu, UiMetrics, VideoCapabilities};
        let display = crate::Window::new("rapid navigation", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        for (size, density) in [((1280, 960), 1.0), ((1280, 960), 2.25)] {
            for settle in [1, 10] {
                for seed in [0, 1, 8, 12, 14] {
                    for (up, horizontal) in [(true, 0), (false, 0), (true, 1), (false, -1)] {
                        let metrics = UiMetrics {
                            drawable: size,
                            window: size,
                            pixels_per_point: density,
                        };
                        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
                            .with_mach3_options(None);
                        menu.toggle();
                        for _ in 0..settle {
                            menu.prepare(metrics, display.settings());
                        }
                        for _ in 0..seed {
                            menu.handle_gamepad(PadEvent {
                                next: true,
                                ..Default::default()
                            });
                            menu.prepare(metrics, display.settings());
                        }
                        for index in 0..4 {
                            let actions = Actions {
                                up,
                                down: !up,
                                left: horizontal < 0,
                                right: horizontal > 0,
                                ..Default::default()
                            };
                            menu.handle_gamepad(PadEvent {
                                actions,
                                ..Default::default()
                            });
                            menu.prepare(metrics, display.settings());
                            for released in [false, true] {
                                let id = menu.context.memory(|memory| memory.focused()).unwrap_or_else(|| panic!("lost focus size={size:?} settle={settle} seed={seed} up={up} step={index} released={released}"));
                                let target = menu
                                    .context
                                    .data(|data| {
                                        data.get_temp::<Targets>(targets_key())
                                            .unwrap()
                                            .0
                                            .into_iter()
                                            .find(|target| target.id == id)
                                    })
                                    .unwrap_or_else(|| panic!("unregistered focus {id:?}"));
                                if released {
                                    assert!(
                                        target.visible.is_positive(),
                                        "invisible focus size={size:?} settle={settle} seed={seed} up={up} horizontal={horizontal} step={index} rect={:?} visible={:?}",
                                        target.rect,
                                        target.visible
                                    );
                                }
                                if !released {
                                    menu.handle_gamepad(PadEvent::default());
                                    menu.prepare(metrics, display.settings());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn shared_menu_is_usable_without_a_pointer_in_both_games() {
        const NAME: &str =
            "settings_navigation::tests::shared_menu_is_usable_without_a_pointer_in_both_games";
        if std::env::var("REFORGE_MENU_NAV_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_MENU_NAV_TEST", NAME)
                .env("SDL_VIDEO_DRIVER", "dummy")
                .env("SDL_VIDEODRIVER", "dummy")
                .env("SDL_RENDER_DRIVER", "software")
                .env("SDL_AUDIO_DRIVER", "dummy")
                .env("SDL_AUDIODRIVER", "dummy")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use crate::settings::{SettingsMenu, VideoCapabilities};
        use sdl3::{
            event::Event as SdlEvent,
            keyboard::{Keycode, Mod, Scancode},
        };
        let display = crate::Window::new("authored menu navigation", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        let confirm = PadEvent {
            pressed: Actions {
                action: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let next = PadEvent {
            next: true,
            ..Default::default()
        };
        let frame =
            |menu: &mut SettingsMenu<'_>| menu.prepare(display.ui_metrics(), display.settings());
        let controller = |menu: &mut SettingsMenu<'_>, pad| {
            assert!(menu.handle_gamepad(pad));
            frame(menu)
        };
        for mach3 in [true, false] {
            let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
            menu = if mach3 {
                menu.with_mach3_options(None)
            } else {
                menu.with_alleycat_options()
            };
            assert!(
                !menu.handle_gamepad(confirm),
                "closed menus leave game input alone"
            );
            assert!(menu.handle_gamepad(PadEvent {
                settings: true,
                ..Default::default()
            }));
            frame(&mut menu);
            frame(&mut menu);
            assert!(menu.context.memory(|memory| memory.focused()).is_some());
            controller(&mut menu, next);
            controller(&mut menu, confirm);
            assert_eq!(menu.scope, 1);
            controller(&mut menu, next); // Display page button
            for page in 1..=3 {
                controller(
                    &mut menu,
                    PadEvent {
                        actions: Actions {
                            down: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                controller(&mut menu, PadEvent::default());
                controller(&mut menu, confirm);
                assert_eq!(menu.page, page);
            }
            controller(&mut menu, next); // Use defaults checkbox
            assert!(!menu.game_editing[3]);
            controller(&mut menu, confirm);
            assert!(menu.game_editing[3]);
            controller(&mut menu, next); // shared stick slider
            let before = menu.draft.as_ref().unwrap().game_values().stick_dead_zone;
            controller(
                &mut menu,
                PadEvent {
                    actions: Actions {
                        left: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            controller(&mut menu, PadEvent::default());
            assert!(menu.draft.as_ref().unwrap().game_values().stick_dead_zone < before);
            // Keyboard Tab reaches Apply, skipping disabled controls, without
            // needing to know the different number of per-game controls.
            for _ in 0..20 {
                let focus = menu.context.memory(|memory| memory.focused()).unwrap();
                if menu.context.read_response(focus).unwrap().rect == menu.apply_rect.unwrap() {
                    break;
                }
                assert!(menu.handle_event(&SdlEvent::KeyDown {
                    timestamp: 0,
                    window_id: 0,
                    keycode: Some(Keycode::Tab),
                    scancode: Some(Scancode::Tab),
                    keymod: Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }));
                assert!(menu.handle_event(&SdlEvent::KeyUp {
                    timestamp: 0,
                    window_id: 0,
                    keycode: Some(Keycode::Tab),
                    scancode: Some(Scancode::Tab),
                    keymod: Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }));
                frame(&mut menu);
            }
            let focus = menu.context.memory(|memory| memory.focused()).unwrap();
            assert_eq!(
                menu.context.read_response(focus).unwrap().rect,
                menu.apply_rect.unwrap()
            );
            assert!(
                creator.session.borrow().saved.game.is_empty(),
                "draft has not saved yet"
            );
            let request = controller(&mut menu, confirm).expect("Apply via controller");
            assert!(request.stick_dead_zone < before);
            menu.applied();
            assert!(!creator.session.borrow().saved.game.is_empty());
            menu.handle_gamepad(PadEvent {
                actions: Actions {
                    down: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            menu.handle_event(&SdlEvent::Window {
                timestamp: 0,
                window_id: 0,
                win_event: sdl3::event::WindowEvent::FocusLost,
            });
            assert_eq!(
                menu.navigation.held, 0,
                "focus loss cancels controller repeat"
            );
            menu.handle_event(&SdlEvent::Window {
                timestamp: 0,
                window_id: 0,
                win_event: sdl3::event::WindowEvent::FocusGained,
            });
            frame(&mut menu);
            // A dropdown closes before the menu; holding Escape cannot turn
            // that first dismissal into a second action or a guest quit.
            let popup_id = Id::new("authored popup dismissal");
            menu.context
                .memory_mut(|memory| memory.open_popup(popup_id));
            let escape = |repeat| SdlEvent::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::Escape),
                scancode: Some(Scancode::Escape),
                keymod: Mod::NOMOD,
                repeat,
                which: 0,
                raw: 0,
            };
            assert!(menu.handle_event(&escape(false)));
            assert!(menu.is_open());
            assert!(!menu.context.memory(|memory| memory.any_popup_open()));
            assert!(menu.handle_event(&escape(true)));
            assert!(menu.is_open());
            controller(
                &mut menu,
                PadEvent {
                    cancel: true,
                    ..Default::default()
                },
            );
            assert!(!menu.is_open());
            // Keep the two iterations independent, with no filesystem settings.
            creator.session.borrow_mut().saved = Default::default();
        }
    }

    fn keys(events: &[Event]) -> Vec<(Key, bool, bool)> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => Some((*key, modifiers.shift, *repeat)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn controller_edges_repeat_and_boundaries_use_host_time_without_bursts() {
        let mut navigation = Navigation::default();
        let down = PadEvent {
            actions: Actions {
                down: true,
                ..Default::default()
            },
            ..Default::default()
        };
        navigation.controller(down, 1.0);
        let mut events = Vec::new();
        navigation.drain(1.0, &mut events);
        assert_eq!(keys(&events), [(Key::ArrowDown, false, false)]);
        events.clear();
        navigation.controller(down, 1.2);
        navigation.drain(1.34, &mut events);
        assert!(events.is_empty());
        navigation.drain(1.35, &mut events);
        assert_eq!(keys(&events), [(Key::ArrowDown, false, true)]);
        events.clear();
        navigation.drain(20.0, &mut events);
        assert_eq!(keys(&events).len(), 1, "no catch-up burst after a UI stall");
        events.clear();
        navigation.controller(PadEvent::default(), 20.01);
        navigation.drain(30.0, &mut events);
        assert!(events.is_empty(), "release/disconnect stops repeat");
        navigation.controller(
            PadEvent {
                actions: Actions {
                    up: true,
                    down: true,
                    ..Default::default()
                },
                pressed: Actions {
                    action: true,
                    ..Default::default()
                },
                previous: true,
                ..Default::default()
            },
            31.0,
        );
        navigation.drain(31.0, &mut events);
        assert_eq!(
            keys(&events),
            [(Key::Enter, false, false), (Key::Tab, true, false)]
        );
        assert!(
            events
                .chunks_exact(2)
                .all(|pair| matches!(pair[1], Event::Key { pressed: false, .. })),
            "controller pulses never leave egui keys held"
        );
        navigation = Navigation::default();
        events.clear();
        navigation.drain(100.0, &mut events);
        assert!(events.is_empty(), "host reset must not repeat old controls");
        let context = Context::default();
        context.memory_mut(|memory| memory.open_popup(Id::new("authored scrolling popup")));
        context.data_mut(|data| data.insert_temp(reveal_key(), true));
        events.push(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: Modifiers::NONE,
        });
        navigation.navigate(&context, &mut events);
        assert_eq!(
            context.data(|data| data.get_temp::<bool>(reveal_key())),
            Some(false),
            "manual popup scrolling stops automatic focus tracking"
        );
    }

    #[test]
    fn focus_scrolls_into_view_without_fighting_later_manual_scrolling() {
        let context = Context::default();
        context.style_mut(|style| style.animation_time = 0.0);
        let draw = |focus_last: bool, manual_top: bool| {
            let mut offset = 0.0;
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    begin_frame(context);
                    egui::CentralPanel::default().show(context, |ui| {
                        let mut scroll = egui::ScrollArea::vertical().max_height(80.0);
                        if manual_top {
                            scroll = scroll.vertical_scroll_offset(0.0);
                        }
                        offset = scroll
                            .show(ui, |ui| {
                                let first = scroll_start(ui.ctx());
                                for index in 0..30 {
                                    let response =
                                        control(ui.button(format!("Authored control {index}")));
                                    if index == 29 && focus_last {
                                        response.request_focus();
                                    }
                                }
                                scroll_focus(ui, first);
                            })
                            .state
                            .offset
                            .y;
                    });
                },
            );
            offset
        };
        draw(true, false);
        assert!(draw(false, false) > 300.0, "offscreen focus is revealed");
        assert_eq!(
            draw(false, true),
            0.0,
            "mouse scrolling is not undone by retained focus"
        );
        assert_eq!(draw(false, false), 0.0);
    }

    #[test]
    fn combo_keyboard_selection_wraps_skips_disabled_and_restores_anchor() {
        let context = Context::default();
        let mut value = 1;
        let mut anchor = Id::NULL;
        let mut draw = |mut events: Vec<Event>, initial: bool, value: &mut i32| {
            filter_popup_input(&context, &mut events);
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        let response = egui::ComboBox::from_id_salt("authored choices")
                            .selected_text(value.to_string())
                            .show_ui(ui, |ui| {
                                popup(ui, |ui| {
                                    for candidate in 0..3 {
                                        ui.selectable_value(
                                            value,
                                            candidate,
                                            candidate.to_string(),
                                        );
                                    }
                                    ui.add_enabled(false, egui::Button::new("Unavailable"));
                                })
                            })
                            .response;
                        anchor = response.id;
                        if initial {
                            response.request_focus();
                        }
                        let _ = ui.button("Outside popup");
                    });
                },
            );
        };
        let press = |key| {
            let mut events = Vec::new();
            pulse(&mut events, key, false, false);
            events
        };
        draw(Vec::new(), true, &mut value);
        draw(press(Key::Enter), false, &mut value);
        assert!(context.memory(|memory| memory.any_popup_open()));
        let middle = context.memory(|memory| memory.focused()).unwrap();
        draw(press(Key::ArrowDown), false, &mut value);
        let bottom = context.memory(|memory| memory.focused()).unwrap();
        assert_ne!(middle, bottom);
        draw(press(Key::ArrowDown), false, &mut value);
        let top = context.memory(|memory| memory.focused()).unwrap();
        assert_ne!(top, bottom);
        assert_ne!(top, middle);
        assert_eq!(value, 1, "moving focus does not commit a choice");
        draw(press(Key::Enter), false, &mut value);
        assert_eq!(value, 0);
        assert!(!context.memory(|memory| memory.any_popup_open()));
        draw(press(Key::Enter), false, &mut value);
        assert!(context.memory(|memory| memory.any_popup_open()));
        close_popup(&context);
        assert!(!context.memory(|memory| memory.any_popup_open()));
        // End the closure's borrow of the measured anchor.
        drop(draw);
        assert_eq!(context.memory(|memory| memory.focused()), Some(anchor));
    }
}
