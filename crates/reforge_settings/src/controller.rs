// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Alley Cat controller templates use physical face-button positions, independent
//! of the connected controller's printed labels.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControllerMovement {
    #[default]
    Directional,
    Button,
}
impl ControllerMovement {
    pub const ALL: [Self; 2] = [Self::Directional, Self::Button];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Directional => "directional",
            Self::Button => "button",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Directional => "Directional jump",
            Self::Button => "Button jump",
        }
    }
}
impl std::str::FromStr for ControllerMovement {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "directional" => Ok(Self::Directional),
            "button" => Ok(Self::Button),
            _ => Err("controller movement must be directional or button".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FaceButton {
    #[default]
    South,
    East,
    West,
    North,
}
impl FaceButton {
    pub const ALL: [Self; 4] = [Self::South, Self::East, Self::West, Self::North];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::North => "north",
        }
    }
}
impl std::str::FromStr for FaceButton {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "south" => Ok(Self::South),
            "east" => Ok(Self::East),
            "west" => Ok(Self::West),
            "north" => Ok(Self::North),
            _ => Err("controller action must be south, east, west or north".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControllerButtons {
    #[default]
    WestJumpSouthAction,
    EastJumpSouthAction,
    WestActionSouthJump,
    EastActionSouthJump,
}
impl ControllerButtons {
    pub const ALL: [Self; 4] = [
        Self::WestJumpSouthAction,
        Self::EastJumpSouthAction,
        Self::WestActionSouthJump,
        Self::EastActionSouthJump,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WestJumpSouthAction => "west-jump-south-action",
            Self::EastJumpSouthAction => "east-jump-south-action",
            Self::WestActionSouthJump => "west-action-south-jump",
            Self::EastActionSouthJump => "east-action-south-jump",
        }
    }

    /// Return the Jump/Drop button followed by the Action button.
    pub fn binding(self) -> (FaceButton, FaceButton) {
        use FaceButton::{East, South, West};
        match self {
            Self::WestJumpSouthAction => (West, South),
            Self::EastJumpSouthAction => (East, South),
            Self::WestActionSouthJump => (South, West),
            Self::EastActionSouthJump => (South, East),
        }
    }
}
impl std::str::FromStr for ControllerButtons {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.as_str() == value)
            .ok_or_else(|| "unknown controller button layout".into())
    }
}
