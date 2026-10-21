// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Alley rendering and accepted player updates, independent of host presentation.

extern crate self as alleycat;

pub mod alley;
pub mod assets;
mod bin_cat;
mod birdcage;
mod bookcase;
pub mod cga;
mod cheese;
pub mod cli;
mod clothes;
mod completion;
#[cfg(any(feature = "sdl", test))]
mod control_history;
mod decoration;
#[cfg(feature = "dev-tools")]
pub mod dev_tools;
mod dog;
mod failure;
mod fishbowl;
mod font;
mod hearts;
mod hearts_completion;
pub mod menu;
mod mice;
mod milk;
#[cfg(feature = "sdl")]
pub mod platform;
pub mod player;
mod projectiles;
mod room;
pub mod runtime;
pub mod sound;
pub mod splash;
mod swimming;
pub mod timing;
mod transition;
mod windows;
