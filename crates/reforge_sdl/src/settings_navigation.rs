// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Host menu navigation. Controller repeat uses host UI time, never game time.

use egui::{Context, Event, Id, Key, Modifiers, Response, Ui, WidgetText};

use crate::gamepad::PadEvent;
use crate::settings_selector::InputMode;

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
    pub mode: InputMode,
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
        context.data_mut(|data| {
            data.remove::<Vec<(Id, Event)>>(selector_events_key());
        });
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
        // Value boxes remain mouse editors, not keyboard/controller stops.
        // If navigation resumes after a mouse edit, return to the row's slider.
        if input.iter().any(|event| {
            matches!(
                event,
                Event::Key {
                    key: Key::Tab
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight,
                    pressed: true,
                    ..
                }
            )
        }) {
            if let Some(owner) = targets
                .iter()
                .find(|target| Some(target.id) == focused && matches!(target.kind, Kind::Number))
                .and_then(|target| target.owner)
            {
                focused = Some(owner);
                context.memory_mut(|memory| memory.request_focus(owner));
            }
        }
        targets.retain(|target| !matches!(target.kind, Kind::Number));
        let initial_focus = focused;
        if let Some(target) = targets.iter().find(|target| Some(target.id) == focused) {
            context.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    target.id,
                    egui::EventFilter {
                        horizontal_arrows: matches!(target.kind, Kind::Slider | Kind::Selector),
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
            // Widget-owned horizontal arrows still adjust sliders and selectors.
            if arrow
                && current.is_some_and(|index| match targets[index].kind {
                    Kind::Slider | Kind::Selector => {
                        matches!(key, Key::ArrowLeft | Key::ArrowRight)
                    }
                    Kind::Number | Kind::Control => false,
                })
            {
                // Do not apply a value edit to a control first reached by an
                // earlier direction in this same input batch. Its egui focus
                // filter will be established before the next batch instead.
                if focused == initial_focus {
                    if let Some(index) =
                        current.filter(|index| matches!(targets[*index].kind, Kind::Selector))
                    {
                        // A just-restored popup owner cannot install egui's focus
                        // filter until the following frame. Deliver its edit
                        // directly, without also initiating spatial navigation.
                        context.data_mut(|data| {
                            data.get_temp_mut_or_default::<Vec<(Id, Event)>>(selector_events_key())
                                .push((targets[index].id, event))
                        });
                    } else {
                        events.push(event);
                    }
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
    pub fn controller(&mut self, pad: PadEvent, now: f64) -> bool {
        // Opposing directions cancel for menu navigation only.
        let a = pad.actions;
        let held = u8::from(a.up && !a.down)
            | (u8::from(a.down && !a.up) << 1)
            | (u8::from(a.left && !a.right) << 2)
            | (u8::from(a.right && !a.left) << 3);
        let fresh =
            held & !self.held != 0 || pad.pressed.action || pad.previous || pad.next || pad.pause;
        if fresh && self.mode != InputMode::Controller {
            self.repeat = [now + FIRST_REPEAT; 4];
        }
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
        if fresh {
            self.mode = InputMode::Controller;
        }
        fresh
    }

    pub fn set_mode(&mut self, context: &Context, mode: InputMode) {
        self.mode = mode;
        crate::settings_selector::set_mode(context, mode);
    }

    pub fn drain(&mut self, now: f64, events: &mut Vec<Event>) {
        for (index, key) in DIRECTIONS.into_iter().enumerate() {
            if self.mode == InputMode::Controller
                && self.held & (1 << index) != 0
                && now >= self.repeat[index]
            {
                // A host stall must not cause a burst of menu adjustments.
                pulse(&mut self.events, key, false, true);
                self.repeat[index] = now + REPEAT;
            }
        }
        events.append(&mut self.events);
    }
}

fn selector_events_key() -> Id {
    Id::new("reforge_selector_events")
}

pub(super) fn take_selector_events(context: &Context, id: Id) -> Vec<Event> {
    context.data_mut(|data| {
        let pending = data.get_temp_mut_or_default::<Vec<(Id, Event)>>(selector_events_key());
        let mut events = Vec::new();
        pending.retain(|(owner, event)| {
            if *owner == id {
                events.push(event.clone());
                false
            } else {
                true
            }
        });
        events
    })
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Control,
    Slider,
    /// Mouse editor, tracked for its slider owner but omitted from navigation.
    Number,
    Selector,
}
#[derive(Clone)]
struct Target {
    id: Id,
    rect: egui::Rect,
    vertical_rect: egui::Rect,
    visible: egui::Rect,
    layer: egui::LayerId,
    kind: Kind,
    group: Group,
    owner: Option<Id>,
    primary_footer: bool,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Group {
    Scope,
    Sidebar,
    Content,
    Footer,
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
    register(response, kind, Group::Content, false)
}
pub(super) fn group_control(response: Response, group: Group) -> Response {
    register(response, Kind::Control, group, false)
}
pub(super) fn primary_footer_control(response: Response) -> Response {
    register(response, Kind::Control, Group::Footer, true)
}
fn register(response: Response, kind: Kind, group: Group, primary_footer: bool) -> Response {
    if response.enabled() && response.sense.is_focusable() {
        response.ctx.data_mut(|data| {
            let targets = data.get_temp_mut_or_default::<Targets>(targets_key());
            targets.0.retain(|target| target.id != response.id);
            let owner = if matches!(kind, Kind::Number) {
                // Each numeric editor is registered immediately after its slider.
                // Keep that identity even if mouse text editing changes its rect.
                targets
                    .0
                    .iter()
                    .rfind(|target| {
                        matches!(target.kind, Kind::Slider)
                            && target.group == group
                            && target.layer == response.layer_id
                    })
                    .map(|target| target.id)
            } else {
                None
            };
            targets.0.push(Target {
                id: response.id,
                rect: response.rect,
                vertical_rect: response.rect,
                visible: response.interact_rect,
                layer: response.layer_id,
                kind,
                group,
                owner,
                primary_footer,
            });
        });
    }
    response
}
/// All controls in a labelled settings row occupy one vertical navigation
/// column, regardless of their individual text widths or radio-button position.
/// Actual hit/focus rectangles and horizontal navigation remain unchanged.
pub(super) fn row_controls(ui: &mut Ui, bounds: egui::Rect, draw: impl FnOnce(&mut Ui)) {
    let first = scroll_start(ui.ctx());
    draw(ui);
    ui.ctx().data_mut(|data| {
        let targets = data.get_temp_mut_or_default::<Targets>(targets_key());
        for target in targets.0.iter_mut().skip(first) {
            // A row can open a popup. Its separate layer retains its own layout.
            if target.layer == ui.layer_id() {
                target.vertical_rect.min.x = bounds.min.x;
                target.vertical_rect.max.x = bounds.max.x;
            }
        }
    });
}

pub(super) fn collapsing<R>(response: egui::CollapsingResponse<R>) -> egui::CollapsingResponse<R> {
    control(response.header_response.clone());
    response
}
fn directional_target(targets: &[Target], current: usize, key: Key) -> Option<usize> {
    let source = &targets[current];
    // The sidebar enters the displayed page, even when its first option lies
    // below the diagonal search cone because of notices or stacked headings.
    if source.group == Group::Sidebar && key == Key::ArrowRight {
        return [Group::Content, Group::Footer]
            .into_iter()
            .find_map(|group| {
                targets.iter().position(|target| {
                    target.layer == source.layer
                        && target.group == group
                        && !matches!(target.kind, Kind::Number)
                })
            });
    }
    let vertical = matches!(key, Key::ArrowUp | Key::ArrowDown);
    let sign = if matches!(key, Key::ArrowUp | Key::ArrowLeft) {
        -1.0
    } else {
        1.0
    };
    let next = targets
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if index == current || candidate.layer != source.layer {
                return None;
            }
            let delta = candidate.rect.center() - source.rect.center();
            // Horizontal movement within the content acts on the same row;
            // diagonal fields below it are reached with Up/Down instead.
            if !vertical
                && source.group == Group::Content
                && candidate.group == Group::Content
                && !source.rect.y_range().intersects(candidate.rect.y_range())
            {
                return None;
            }
            let (forward, cross) = if vertical {
                let a = source.vertical_rect;
                let b = candidate.vertical_rect;
                let gap = (a.left() - b.right()).max(b.left() - a.right()).max(0.0);
                (delta.y * sign, gap)
            } else {
                (delta.x * sign, delta.y.abs())
            };
            if forward <= 0.5 || cross > forward {
                return None;
            }
            let preferred = if vertical && source.group == Group::Footer && sign < 0.0 {
                Group::Content
            } else {
                source.group
            };
            Some((
                index,
                u8::from(candidate.group != preferred),
                forward + cross * 2.0,
            ))
        })
        .min_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)))
        .map(|(index, _, _)| index);
    if key == Key::ArrowDown
        && source.group == Group::Content
        && next.is_some_and(|index| targets[index].group == Group::Footer)
    {
        // Leaving the page should lead to its enabled primary action. Keep
        // ordinary row navigation, footer arrows and Tab order unchanged.
        return targets
            .iter()
            .position(|target| target.layer == source.layer && target.primary_footer)
            .or(next);
    }
    next
}

#[derive(Clone, Default)]
struct Popup {
    items: Vec<Id>,
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

pub(super) fn popup<R>(ui: &mut Ui, anchor: Id, contents: impl FnOnce(&mut Choices<'_>) -> R) -> R {
    let context = ui.ctx().clone();
    let key = ui.layer_id().id.with("reforge_navigation");
    let mut state = context.data_mut(|data| data.get_temp::<Popup>(key).unwrap_or_default());
    let focused = context.memory(|memory| memory.focused());
    let inside = focused.is_some_and(|id| state.items.contains(&id));
    // The owning selector knows its identity. Inferring it from focus can
    // capture another control during egui's layout/discard passes.
    context.data_mut(|data| data.insert_temp(anchor_key(), anchor));
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
                egui::Stroke::new(1.5_f32, crate::ui::current_palette(context).focus),
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
    fn sidebar_right_enters_content_below_notices_and_falls_back_to_actions() {
        let target = |name: &str, group, x, y| {
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(100.0, 22.0));
            Target {
                id: Id::new(name),
                rect,
                vertical_rect: rect,
                visible: rect,
                layer: egui::LayerId::new(egui::Order::Middle, Id::new("menu")),
                kind: Kind::Control,
                group,
                owner: None,
                primary_footer: false,
            }
        };
        let mut targets = vec![
            target("Global defaults", Group::Scope, 150.0, 0.0),
            target("Game overrides", Group::Scope, 270.0, 0.0),
            target("Display", Group::Sidebar, 0.0, 45.0),
            target("Windowed", Group::Content, 180.0, 230.0),
            target("Reset section", Group::Footer, 0.0, 400.0),
        ];
        assert_eq!(directional_target(&targets, 2, Key::ArrowRight), Some(3));
        targets.remove(3); // Every page option may be locked or disabled.
        assert_eq!(directional_target(&targets, 2, Key::ArrowRight), Some(3));
        targets.remove(3);
        assert_eq!(directional_target(&targets, 2, Key::ArrowRight), None);
    }

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
    fn audio_navigation_follows_content_rows_and_skips_numeric_editors() {
        const NAME: &str = "settings_navigation::tests::audio_navigation_follows_content_rows_and_skips_numeric_editors";
        if std::env::var("REFORGE_AUDIO_NAV_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_AUDIO_NAV_TEST", NAME)
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
        use reforge_settings::{SoundCategory, SoundRenderer};
        use sdl3::{
            event::Event as SdlEvent,
            keyboard::{Keycode, Mod, Scancode},
        };
        let display = crate::Window::new("Audio row navigation", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        for mach3 in [false, true] {
            for scope in [0, 1] {
                for (size, density) in [
                    ((640, 480), 1.0),
                    ((960, 720), 1.5),
                    ((1280, 960), 2.25),
                    ((2560, 1920), 2.25),
                ] {
                    for controller in [false, true] {
                        creator.session.borrow_mut().game =
                            if mach3 { "mach-3" } else { "alley-cat" }.into();
                        let metrics = UiMetrics {
                            drawable: size,
                            window: size,
                            pixels_per_point: density,
                        };
                        let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default())
                            .with_audio_renderers(&[SoundRenderer::Speaker])
                            .with_audio_categories(if mach3 {
                                &SoundCategory::ALL
                            } else {
                                &[SoundCategory::Music, SoundCategory::Effects]
                            });
                        menu = if mach3 {
                            menu.with_mach3_options(None)
                        } else {
                            menu.with_alleycat_options()
                        };
                        menu.toggle();
                        menu.scope = scope;
                        menu.page = 2;
                        menu.prepare(metrics, display.settings());
                        menu.game_editing[2] = true;
                        for _ in 0..4 {
                            menu.prepare(metrics, display.settings());
                        }
                        let targets = menu
                            .context
                            .data(|data| data.get_temp::<Targets>(targets_key()).unwrap().0);
                        let content: Vec<_> = targets
                            .iter()
                            .filter(|target| {
                                target.group == Group::Content
                                    && !matches!(target.kind, Kind::Number)
                            })
                            .collect();
                        let master = content
                            .iter()
                            .position(|target| matches!(target.kind, Kind::Slider))
                            .unwrap();
                        assert!(master > 0);
                        let volumes = if scope == 0 || mach3 { 4 } else { 3 };
                        assert!(
                            content[master..master + volumes]
                                .iter()
                                .all(|target| matches!(target.kind, Kind::Slider))
                        );
                        let send = |menu: &mut SettingsMenu<'_>, key: Key| {
                            if controller {
                                menu.handle_gamepad(PadEvent {
                                    next: key == Key::Tab,
                                    actions: Actions {
                                        up: key == Key::ArrowUp,
                                        down: key == Key::ArrowDown,
                                        right: key == Key::ArrowRight,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                });
                            } else {
                                let (scan, code) = match key {
                                    Key::ArrowUp => (Scancode::Up, Keycode::Up),
                                    Key::ArrowDown => (Scancode::Down, Keycode::Down),
                                    Key::ArrowRight => (Scancode::Right, Keycode::Right),
                                    Key::Tab => (Scancode::Tab, Keycode::Tab),
                                    _ => unreachable!(),
                                };
                                for pressed in [true, false] {
                                    let event = if pressed {
                                        SdlEvent::KeyDown {
                                            timestamp: 0,
                                            window_id: 0,
                                            which: 0,
                                            raw: 0,
                                            scancode: Some(scan),
                                            keycode: Some(code),
                                            keymod: Mod::NOMOD,
                                            repeat: false,
                                        }
                                    } else {
                                        SdlEvent::KeyUp {
                                            timestamp: 0,
                                            window_id: 0,
                                            which: 0,
                                            raw: 0,
                                            scancode: Some(scan),
                                            keycode: Some(code),
                                            keymod: Mod::NOMOD,
                                            repeat: false,
                                        }
                                    };
                                    menu.handle_event(&event);
                                }
                            }
                            menu.prepare(metrics, display.settings());
                            menu.handle_gamepad(PadEvent::default());
                            menu.prepare(metrics, display.settings());
                        };
                        // Enable sound must not jump diagonally to a value box.
                        let enable = content[master - 1].id;
                        menu.context.memory_mut(|m| m.request_focus(enable));
                        menu.prepare(metrics, display.settings());
                        send(&mut menu, Key::ArrowRight);
                        assert_eq!(menu.context.memory(|m| m.focused()), Some(enable));
                        // Walk every enabled Audio row, including options below
                        // the scroll viewport, before visiting the footer.
                        for pair in content.windows(2) {
                            for (source, key, expected) in [
                                (pair[0].id, Key::ArrowDown, pair[1].id),
                                (pair[1].id, Key::ArrowUp, pair[0].id),
                            ] {
                                menu.context.memory_mut(|m| m.request_focus(source));
                                menu.prepare(metrics, display.settings());
                                send(&mut menu, key);
                                assert_eq!(
                                    menu.context.memory(|m| m.focused()),
                                    Some(expected),
                                    "mach3={mach3} scope={scope} size={size:?} density={density} controller={controller} key={key:?}"
                                );
                                let visible = menu.context.data(|data| {
                                    data.get_temp::<Targets>(targets_key())
                                        .unwrap()
                                        .0
                                        .into_iter()
                                        .find(|t| t.id == expected)
                                        .unwrap()
                                        .visible
                                });
                                assert!(
                                    visible.is_positive(),
                                    "focused Audio control must scroll into view"
                                );
                            }
                        }
                        let numbers: Vec<_> = targets
                            .iter()
                            .filter(|t| matches!(t.kind, Kind::Number))
                            .map(|t| t.id)
                            .collect();
                        assert_eq!(numbers.len(), volumes);
                        for _ in 0..targets.len() + 1 {
                            send(&mut menu, Key::Tab);
                            assert!(
                                !numbers.contains(&menu.context.memory(|m| m.focused()).unwrap())
                            );
                        }
                        // Mouse-entered numeric editing is retained until an
                        // actual navigation key returns to the owning slider.
                        menu.context.memory_mut(|m| m.request_focus(numbers[0]));
                        menu.prepare(metrics, display.settings());
                        assert_eq!(menu.context.memory(|m| m.focused()), Some(numbers[0]));
                        send(&mut menu, Key::ArrowDown);
                        assert_eq!(
                            menu.context.memory(|m| m.focused()),
                            Some(content[master + 1].id),
                            "number exit: mach3={mach3} scope={scope} size={size:?} density={density} controller={controller}"
                        );
                        // Down leaves each page through Apply when a draft is
                        // pending. Clean drafts retain the usual Reset target;
                        // Tab continues to follow the footer's visual order.
                        let clean = menu.draft.clone();
                        for changed in [false, true] {
                            menu.draft = clean.clone();
                            if changed {
                                menu.draft.as_mut().unwrap().edit(
                                    scope == 1,
                                    "master_volume",
                                    "79".into(),
                                );
                            }
                            for page in 0..4 {
                                menu.page = page;
                                menu.game_editing[page] = true;
                                for _ in 0..3 {
                                    menu.prepare(metrics, display.settings());
                                }
                                let targets = menu.context.data(|data| {
                                    data.get_temp::<Targets>(targets_key()).unwrap().0
                                });
                                let last = targets
                                    .iter()
                                    .rfind(|t| {
                                        t.group == Group::Content && !matches!(t.kind, Kind::Number)
                                    })
                                    .unwrap()
                                    .id;
                                let reset = targets
                                    .iter()
                                    .find(|t| t.group == Group::Footer)
                                    .unwrap()
                                    .id;
                                let expected = if changed {
                                    targets
                                        .iter()
                                        .find(|t| t.rect == menu.apply_rect.unwrap())
                                        .unwrap()
                                        .id
                                } else {
                                    reset
                                };
                                menu.context.memory_mut(|m| m.request_focus(last));
                                menu.prepare(metrics, display.settings());
                                send(&mut menu, Key::ArrowDown);
                                assert_eq!(
                                    menu.context.memory(|m| m.focused()),
                                    Some(expected),
                                    "page exit: mach3={mach3} scope={scope} size={size:?} density={density} controller={controller} page={page} changed={changed}"
                                );
                                send(&mut menu, Key::ArrowUp);
                                assert_eq!(menu.context.memory(|m| m.focused()), Some(last));
                                send(&mut menu, Key::Tab);
                                assert_eq!(menu.context.memory(|m| m.focused()), Some(reset));
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
            controller(&mut menu, next); // Override defaults checkbox
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

    #[test]
    fn adaptive_menu_handoff_uses_real_sdl_events_at_both_ui_densities() {
        const NAME: &str = "settings_navigation::tests::adaptive_menu_handoff_uses_real_sdl_events_at_both_ui_densities";
        if std::env::var("REFORGE_ADAPTIVE_MENU_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_ADAPTIVE_MENU_TEST", NAME)
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
        use sdl3::{
            event::Event as SdlEvent,
            keyboard::{Keycode, Mod, Scancode},
            mouse::{MouseButton, MouseState},
        };
        let display = crate::Window::new("adaptive settings", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        let key = |scancode, keycode, pressed| {
            if pressed {
                SdlEvent::KeyDown {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    raw: 0,
                    scancode: Some(scancode),
                    keycode: Some(keycode),
                    keymod: Mod::NOMOD,
                    repeat: false,
                }
            } else {
                SdlEvent::KeyUp {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    raw: 0,
                    scancode: Some(scancode),
                    keycode: Some(keycode),
                    keymod: Mod::NOMOD,
                    repeat: false,
                }
            }
        };
        let selector = |menu: &SettingsMenu<'_>| {
            menu.context.data(|data| {
                data.get_temp::<Targets>(targets_key())
                    .unwrap()
                    .0
                    .into_iter()
                    .find(|t| matches!(t.kind, Kind::Selector))
                    .unwrap()
            })
        };
        for mach3 in [true, false] {
            for density in [1.0, 2.25] {
                let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
                menu = if mach3 {
                    menu.with_mach3_options(None)
                } else {
                    menu.with_alleycat_options()
                };
                let metrics = UiMetrics {
                    drawable: (1920, 1440),
                    window: (1920, 1440),
                    pixels_per_point: density,
                };
                menu.handle_event(&key(Scancode::F10, Keycode::F10, true));
                menu.handle_event(&key(Scancode::F10, Keycode::F10, false));
                assert!(menu.is_open());
                assert!(menu.navigation.mode == InputMode::Mouse);
                // Closing with Escape must not silently choose carousels for
                // the next F10 opening either.
                menu.handle_event(&key(Scancode::Escape, Keycode::Escape, true));
                menu.handle_event(&key(Scancode::Escape, Keycode::Escape, false));
                assert!(!menu.is_open());
                assert!(menu.navigation.mode == InputMode::Mouse);
                menu.handle_event(&key(Scancode::F10, Keycode::F10, true));
                menu.handle_event(&key(Scancode::F10, Keycode::F10, false));
                assert!(menu.is_open());
                assert!(menu.navigation.mode == InputMode::Mouse);
                for _ in 0..4 {
                    menu.prepare(metrics, display.settings());
                }
                let first = selector(&menu);
                menu.context.memory_mut(|m| m.request_focus(first.id));
                menu.prepare(metrics, display.settings());
                menu.handle_event(&key(Scancode::Right, Keycode::Right, true));
                menu.handle_event(&key(Scancode::Right, Keycode::Right, false));
                menu.prepare(metrics, display.settings());
                assert!(menu.navigation.mode == InputMode::Keyboard);
                assert_eq!(
                    menu.draft.as_ref().unwrap().global_values().scaling_policy,
                    reforge_settings::ScalingPolicy::Fit
                );
                assert!(!menu.context.memory(|m| m.any_popup_open()));
                let target = selector(&menu);
                assert_eq!(target.rect.width(), 220.0);
                assert!(target.rect.height() <= 24.0);
                let position = target.visible.center() * density;
                menu.handle_event(&SdlEvent::MouseMotion {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mousestate: MouseState::from_sdl_state(0),
                    x: position.x,
                    y: position.y,
                    xrel: 1.0,
                    yrel: 1.0,
                });
                menu.prepare(metrics, display.settings());
                assert!(menu.navigation.mode == InputMode::Keyboard);
                for pressed in [true, false] {
                    let event = if pressed {
                        SdlEvent::MouseButtonDown {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: position.x,
                            y: position.y,
                        }
                    } else {
                        SdlEvent::MouseButtonUp {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: position.x,
                            y: position.y,
                        }
                    };
                    menu.handle_event(&event);
                    assert!(menu.navigation.mode == InputMode::Mouse);
                    menu.prepare(metrics, display.settings());
                }
                assert!(
                    menu.context.memory(|m| m.any_popup_open()),
                    "single click at density {density}"
                );
                menu.handle_event(&key(Scancode::Right, Keycode::Right, true));
                menu.handle_event(&key(Scancode::Right, Keycode::Right, false));
                menu.prepare(metrics, display.settings());
                assert_eq!(
                    menu.draft.as_ref().unwrap().global_values().scaling_policy,
                    reforge_settings::ScalingPolicy::Stretch
                );
                assert!(!menu.context.memory(|m| m.any_popup_open()));
                assert_eq!(
                    menu.context.memory(|m| m.focused()),
                    Some(target.id),
                    "popup handoff mach3={mach3} density={density}"
                );
                let right = PadEvent {
                    actions: Actions {
                        right: true,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                menu.handle_gamepad(right);
                menu.prepare(metrics, display.settings());
                assert!(menu.navigation.mode == InputMode::Controller);
                menu.navigation.set_mode(&menu.context, InputMode::Mouse);
                menu.handle_gamepad(right); // unchanged axis notification while held
                assert!(menu.navigation.mode == InputMode::Mouse);
                let mut repeated = Vec::new();
                menu.navigation.drain(1_000_000.0, &mut repeated);
                assert!(
                    repeated.is_empty(),
                    "mouse mode suppresses held-controller repeat"
                );
                menu.handle_gamepad(PadEvent::default());
                menu.handle_gamepad(right);
                assert!(menu.navigation.mode == InputMode::Controller);
                menu.handle_event(&SdlEvent::Window {
                    timestamp: 0,
                    window_id: 0,
                    win_event: sdl3::event::WindowEvent::FocusLost,
                });
                assert_eq!(menu.navigation.held, 0);
                menu.toggle();
                menu.navigation.set_mode(&menu.context, InputMode::Mouse);
                menu.handle_gamepad(PadEvent {
                    settings: true,
                    ..Default::default()
                });
                assert!(menu.is_open());
                assert!(menu.navigation.mode == InputMode::Controller);
            }
        }
    }

    #[test]
    fn display_arrows_visit_each_row_from_wide_and_narrow_controls() {
        const NAME: &str = "settings_navigation::tests::display_arrows_visit_each_row_from_wide_and_narrow_controls";
        if std::env::var("REFORGE_DISPLAY_ROWS_TEST").as_deref() != Ok(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_DISPLAY_ROWS_TEST", NAME)
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
        use sdl3::{
            event::Event as SdlEvent,
            keyboard::{Keycode, Mod, Scancode},
        };
        let display = crate::Window::new("settings row navigation", 320, 240, Some(4)).unwrap();
        let creator = display.texture_creator();
        for mach3 in [true, false] {
            for (size, density, auto_scale) in [
                ((1280, 960), 1.0, true),
                ((1280, 960), 1.0, false),
                ((1280, 960), 2.25, true),
                ((1280, 960), 2.25, false),
                ((2560, 1920), 2.25, true),
                ((2560, 1920), 2.25, false),
                ((1920, 1440), 2.25, true),
                ((1920, 1440), 2.25, false),
            ] {
                for controller in [false, true] {
                    creator.session.borrow_mut().game =
                        if mach3 { "mach-3" } else { "alley-cat" }.into();
                    let mut menu = SettingsMenu::new(&creator, VideoCapabilities::default());
                    menu = if mach3 {
                        menu.with_mach3_options(None)
                    } else {
                        menu.with_alleycat_options()
                    };
                    menu.toggle();
                    let metrics = UiMetrics {
                        drawable: size,
                        window: size,
                        pixels_per_point: density,
                    };
                    menu.prepare(metrics, display.settings());
                    menu.draft.as_mut().unwrap().edit(
                        false,
                        "scale",
                        if auto_scale { "auto" } else { "4" }.into(),
                    );
                    for _ in 0..4 {
                        menu.prepare(metrics, display.settings());
                    }
                    let targets = menu
                        .context
                        .data(|data| data.get_temp::<Targets>(targets_key()).unwrap().0);
                    let policy = targets
                        .iter()
                        .position(|t| matches!(t.kind, Kind::Selector))
                        .unwrap();
                    let sampling = policy + 1;
                    let aspect = policy + 2;
                    let renderer = policy + 3;
                    let top_controls: Vec<_> = targets[..policy]
                        .iter()
                        .filter(|t| matches!(t.kind, Kind::Control))
                        .collect();
                    let count = top_controls.len();
                    let windowed = top_controls[count - 3].id;
                    let fullscreen = top_controls[count - 2].id;
                    let auto = top_controls[count - 1].id;
                    let backend = targets[renderer + 1].id;
                    let vsync = targets[renderer + 2].id;
                    let limit = targets[renderer + 3].id;
                    let cancel = targets.last().unwrap().id;
                    let display_page = targets
                        .iter()
                        .find(|t| t.group == Group::Sidebar)
                        .unwrap()
                        .id;
                    let slider = targets[..renderer]
                        .iter()
                        .find(|t| matches!(t.kind, Kind::Slider))
                        .map(|t| t.id);
                    let below_scale = slider.unwrap_or(targets[policy].id);
                    let above_policy = slider.unwrap_or(auto);
                    let above_aspect = targets[sampling].id;
                    let above_renderer = targets[aspect].id;
                    for (source, key, expected, label) in [
                        (
                            windowed,
                            Key::ArrowRight,
                            fullscreen,
                            "Windowed -> Fullscreen",
                        ),
                        (
                            fullscreen,
                            Key::ArrowLeft,
                            windowed,
                            "Fullscreen -> Windowed",
                        ),
                        (windowed, Key::ArrowDown, auto, "Windowed -> Window scale"),
                        (
                            auto,
                            Key::ArrowDown,
                            below_scale,
                            "Window scale -> next row",
                        ),
                        (
                            targets[policy].id,
                            Key::ArrowUp,
                            above_policy,
                            "Scaling policy -> previous row",
                        ),
                        (
                            targets[policy].id,
                            Key::ArrowDown,
                            targets[sampling].id,
                            "Scaling policy -> Sampling",
                        ),
                        (
                            targets[sampling].id,
                            Key::ArrowDown,
                            targets[aspect].id,
                            "Sampling -> Aspect ratio",
                        ),
                        (
                            targets[aspect].id,
                            Key::ArrowUp,
                            above_aspect,
                            "Aspect ratio -> previous row",
                        ),
                        (
                            targets[aspect].id,
                            Key::ArrowDown,
                            targets[renderer].id,
                            "Aspect ratio -> Renderer",
                        ),
                        (
                            targets[renderer].id,
                            Key::ArrowUp,
                            above_renderer,
                            "Renderer -> previous row",
                        ),
                        (
                            fullscreen,
                            Key::ArrowDown,
                            auto,
                            "Fullscreen -> Window scale",
                        ),
                        (backend, Key::ArrowDown, vsync, "Backend -> VSync"),
                        (vsync, Key::ArrowDown, limit, "VSync -> Frame limit"),
                        (limit, Key::ArrowUp, vsync, "Frame limit -> VSync"),
                        (vsync, Key::ArrowUp, backend, "VSync -> Backend"),
                        (cancel, Key::ArrowUp, limit, "Cancel -> Frame limit"),
                        (
                            display_page,
                            Key::ArrowRight,
                            windowed,
                            "Display -> page controls",
                        ),
                        (
                            display_page,
                            Key::ArrowRight,
                            windowed,
                            "Display notice -> page controls",
                        ),
                    ] {
                        let notice = label == "Display notice -> page controls";
                        if notice {
                            creator
                                .session
                                .borrow_mut()
                                .cli
                                .insert("scale".into(), "4".into());
                        }
                        menu.context.memory_mut(|m| m.request_focus(source));
                        menu.prepare(metrics, display.settings());
                        let expected = if notice {
                            // CLI overrides are fixed before a real menu opens.
                            // Re-measure after adding this fixture's notice.
                            menu.prepare(metrics, display.settings());
                            menu.context.data(|data| {
                                data.get_temp::<Targets>(targets_key())
                                    .unwrap()
                                    .0
                                    .into_iter()
                                    .find(|t| {
                                        t.group == Group::Content && matches!(t.kind, Kind::Control)
                                    })
                                    .unwrap()
                                    .id
                            })
                        } else {
                            expected
                        };
                        if controller {
                            menu.handle_gamepad(PadEvent {
                                actions: Actions {
                                    up: key == Key::ArrowUp,
                                    down: key == Key::ArrowDown,
                                    left: key == Key::ArrowLeft,
                                    right: key == Key::ArrowRight,
                                    ..Default::default()
                                },
                                ..Default::default()
                            });
                        } else {
                            for pressed in [true, false] {
                                let (scancode, keycode) = match key {
                                    Key::ArrowUp => (Scancode::Up, Keycode::Up),
                                    Key::ArrowDown => (Scancode::Down, Keycode::Down),
                                    Key::ArrowLeft => (Scancode::Left, Keycode::Left),
                                    Key::ArrowRight => (Scancode::Right, Keycode::Right),
                                    _ => unreachable!(),
                                };
                                let event = if pressed {
                                    SdlEvent::KeyDown {
                                        timestamp: 0,
                                        window_id: 0,
                                        which: 0,
                                        raw: 0,
                                        scancode: Some(scancode),
                                        keycode: Some(keycode),
                                        keymod: Mod::NOMOD,
                                        repeat: false,
                                    }
                                } else {
                                    SdlEvent::KeyUp {
                                        timestamp: 0,
                                        window_id: 0,
                                        which: 0,
                                        raw: 0,
                                        scancode: Some(scancode),
                                        keycode: Some(keycode),
                                        keymod: Mod::NOMOD,
                                        repeat: false,
                                    }
                                };
                                menu.handle_event(&event);
                            }
                        }
                        menu.prepare(metrics, display.settings());
                        assert_eq!(
                            menu.context.memory(|m| m.focused()),
                            Some(expected),
                            "{label}; mach3={mach3} size={size:?} density={density} controller={controller} auto_scale={auto_scale}"
                        );
                        menu.handle_gamepad(PadEvent::default());
                        menu.prepare(metrics, display.settings());
                        let target = menu.context.data(|data| {
                            data.get_temp::<Targets>(targets_key())
                                .unwrap()
                                .0
                                .into_iter()
                                .find(|t| t.id == expected)
                                .unwrap()
                        });
                        assert!(
                            target.visible.is_positive(),
                            "focused row must be revealed: {label}"
                        );
                        if notice {
                            creator.session.borrow_mut().cli.remove("scale");
                        }
                    }
                    menu.context.memory_mut(|m| m.request_focus(vsync));
                    menu.prepare(metrics, display.settings());
                    let before = menu.draft.as_ref().unwrap().global_values().vsync;
                    menu.handle_gamepad(PadEvent {
                        pressed: Actions {
                            action: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                    menu.prepare(metrics, display.settings());
                    assert_ne!(
                        menu.draft.as_ref().unwrap().global_values().vsync,
                        before,
                        "VSync must activate once focused"
                    );
                }
            }
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
                                popup(ui, anchor, |ui| {
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
