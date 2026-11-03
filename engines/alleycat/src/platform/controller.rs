// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Alley Cat controller templates. Menu buttons and native movement stay separate.

use reforge_sdl::gamepad::{Actions, FaceButtons};
use reforge_sdl::settings::{AlleyCatSettings, ControllerMovement};

#[derive(Default)]
pub(super) struct Controller {
    pub settings: AlleyCatSettings,
    jump_held: bool,
    dropping: bool,
}

impl Controller {
    pub fn new(settings: AlleyCatSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.jump_held = false;
        self.dropping = false;
    }

    /// Keep the jump/drop decision fixed for this press, including while the
    /// player changes stick direction. It is not a tap buffer or a physics rule.
    pub fn map(&mut self, pad: Actions, faces: FaceButtons) -> Actions {
        if self.settings.controller_movement == ControllerMovement::Directional {
            self.reset();
            return Actions {
                action: faces.held(self.settings.controller_action),
                ..pad
            };
        }
        let (jump, action) = self.settings.controller_buttons.binding();
        let held = faces.held(jump);
        if held && !self.jump_held {
            self.dropping = pad.down;
        }
        self.jump_held = held;
        Actions {
            up: held && !self.dropping,
            down: held && self.dropping,
            left: pad.left,
            right: pad.right,
            action: faces.held(action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reforge_sdl::settings::{ControllerButtons, FaceButton};

    fn faces(button: FaceButton) -> FaceButtons {
        FaceButtons {
            south: button == FaceButton::South,
            east: button == FaceButton::East,
            west: button == FaceButton::West,
            north: button == FaceButton::North,
        }
    }

    #[test]
    fn directional_templates_keep_all_directions_and_bind_only_action() {
        let mut controller = Controller::default();
        let pad = Actions {
            up: true,
            right: true,
            action: true,
            ..Actions::default()
        };
        assert_eq!(controller.map(pad, faces(FaceButton::South)), pad);
        for button in FaceButton::ALL {
            controller.settings.controller_action = button;
            let mapped = controller.map(pad, faces(button));
            assert!(mapped.up && mapped.right && mapped.action);
            if button != FaceButton::South {
                assert!(!controller.map(pad, faces(FaceButton::South)).action);
            }
        }
    }

    #[test]
    fn every_button_pair_maps_jump_drop_action_and_preserves_diagonal_movement() {
        let mut controller = Controller::default();
        controller.settings.controller_movement = ControllerMovement::Button;
        for template in ControllerButtons::ALL {
            controller.settings.controller_buttons = template;
            controller.reset();
            let (jump, action) = template.binding();
            let held = faces(jump);
            let mapped = controller.map(
                Actions {
                    right: true,
                    ..Actions::default()
                },
                held,
            );
            assert!(mapped.up && mapped.right && !mapped.down && !mapped.action);
            // Moving down during a held jump must not turn it into a drop.
            let mapped = controller.map(
                Actions {
                    down: true,
                    left: true,
                    ..Actions::default()
                },
                held,
            );
            assert!(mapped.up && mapped.left && !mapped.down);
            controller.map(Actions::default(), FaceButtons::default());
            let mapped = controller.map(
                Actions {
                    down: true,
                    ..Actions::default()
                },
                held,
            );
            assert!(mapped.down && !mapped.up);
            // Releasing Down does not turn this held drop into a jump.
            assert!(controller.map(Actions::default(), held).down);
            let mapped = controller.map(Actions::default(), faces(action));
            assert!(mapped.action && !mapped.up && !mapped.down);
            assert_eq!(
                controller.map(Actions::default(), FaceButtons::default()),
                Actions::default()
            );
        }
    }

    #[test]
    fn stick_y_alone_never_jumps_or_drops_in_button_mode_and_reset_clears_latch() {
        let mut controller = Controller::default();
        controller.settings.controller_movement = ControllerMovement::Button;
        assert_eq!(
            controller.map(
                Actions {
                    down: true,
                    ..Actions::default()
                },
                FaceButtons::default()
            ),
            Actions::default()
        );
        assert_eq!(
            controller.map(
                Actions {
                    up: true,
                    ..Actions::default()
                },
                FaceButtons::default()
            ),
            Actions::default()
        );
        let jump = faces(FaceButton::West);
        assert!(
            controller
                .map(
                    Actions {
                        down: true,
                        ..Actions::default()
                    },
                    jump
                )
                .down
        );
        controller.reset();
        assert!(controller.map(Actions::default(), jump).up);
    }
}
