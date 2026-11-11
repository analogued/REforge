// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Alley Cat controller templates. Menu buttons and native movement stay separate.

use reforge_sdl::gamepad::{Actions, FaceButtons};
use reforge_sdl::settings::{AlleyCatSettings, ControllerMovement};

/// Hearts has no bottom floor; swimming and entry/fall states are not grounded.
pub(super) fn on_solid_floor(player: &crate::player::Player, scene: u16) -> bool {
    matches!(scene, 0 | 1 | 3..=6)
        && player.y == 180
        && player.vertical_direction == 0
        && player.support_state == 0
        && player.entry_state == 0
}

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
    /// player changes stick direction or lands. It is not a tap buffer or a physics rule.
    pub fn map(&mut self, pad: Actions, faces: FaceButtons, on_floor: bool) -> Actions {
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
            self.dropping = pad.down && !on_floor;
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
        assert_eq!(controller.map(pad, faces(FaceButton::South), false), pad);
        for button in FaceButton::ALL {
            controller.settings.controller_action = button;
            let mapped = controller.map(pad, faces(button), false);
            assert!(mapped.up && mapped.right && mapped.action);
            if button != FaceButton::South {
                assert!(!controller.map(pad, faces(FaceButton::South), false).action);
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
                false,
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
                false,
            );
            assert!(mapped.up && mapped.left && !mapped.down);
            controller.map(Actions::default(), FaceButtons::default(), false);
            let mapped = controller.map(
                Actions {
                    down: true,
                    ..Actions::default()
                },
                held,
                false,
            );
            assert!(mapped.down && !mapped.up);
            // Releasing Down does not turn this held drop into a jump.
            assert!(controller.map(Actions::default(), held, false).down);
            let mapped = controller.map(Actions::default(), faces(action), false);
            assert!(mapped.action && !mapped.up && !mapped.down);
            assert_eq!(
                controller.map(Actions::default(), FaceButtons::default(), false),
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
                FaceButtons::default(),
                false,
            ),
            Actions::default()
        );
        assert_eq!(
            controller.map(
                Actions {
                    up: true,
                    ..Actions::default()
                },
                FaceButtons::default(),
                false,
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
                    jump,
                    false,
                )
                .down
        );
        controller.reset();
        assert!(controller.map(Actions::default(), jump, false).up);
    }

    #[test]
    fn down_and_jump_uses_solid_floors_without_changing_a_held_press() {
        for template in ControllerButtons::ALL {
            let mut controller = Controller::default();
            controller.settings.controller_movement = ControllerMovement::Button;
            controller.settings.controller_buttons = template;
            let (jump, _) = template.binding();
            let down = Actions {
                down: true,
                right: true,
                ..Default::default()
            };
            for scene in [0, 1, 3, 4, 5, 6] {
                controller.reset();
                let mut player = crate::player::Player::at_alley_floor(100);
                let mapped = controller.map(down, faces(jump), on_solid_floor(&player, scene));
                assert!(mapped.up && !mapped.down && mapped.right);
                let mut keys = crate::control_history::Keys::default();
                crate::platform::set_controller_keys(
                    &mut keys,
                    mapped,
                    down,
                    ControllerMovement::Button,
                );
                let (controls, _) = keys.controls(false);
                // Raw directions still win if the simulation enters swimming.
                assert_eq!(keys.controls(true).0.y, 1);
                let mut data = vec![0; crate::assets::DATA_LENGTH];
                crate::assets::set_word(&mut data, 4, scene);
                player.update(controls, &data).unwrap();
                assert_eq!((player.vertical_direction, player.vertical_step), (255, 8));
                // Taking off does not turn the held jump into a drop.
                player.y = 160;
                player.vertical_direction = 255;
                let mapped = controller.map(down, faces(jump), on_solid_floor(&player, scene));
                assert!(mapped.up && !mapped.down);

                controller.map(down, FaceButtons::default(), false);
                player.y = 96;
                player.vertical_direction = 0;
                let mapped = controller.map(down, faces(jump), on_solid_floor(&player, scene));
                assert!(mapped.down && !mapped.up);
                // Landing with Drop still held must not auto-jump.
                player = crate::player::Player::at_alley_floor(100);
                let mapped = controller.map(down, faces(jump), on_solid_floor(&player, scene));
                assert!(mapped.down && !mapped.up);
            }
        }
    }

    #[test]
    fn floor_fallback_excludes_water_hearts_and_non_grounded_poses() {
        let floor = crate::player::Player::at_alley_floor(100);
        for scene in [2, 7] {
            assert!(!on_solid_floor(&floor, scene));
        }
        for scene in [0, 1, 3, 4, 5, 6] {
            for (y, vertical, support, entry) in [
                (179, 0, 0, 0),
                (180, 1, 0, 0),
                (180, 255, 0, 0),
                (180, 0, 1, 0),
                (180, 0, 0, 1),
            ] {
                let mut player = floor.clone();
                player.y = y;
                player.vertical_direction = vertical;
                player.support_state = support;
                player.entry_state = entry;
                assert!(!on_solid_floor(&player, scene));
            }
        }
    }
}
