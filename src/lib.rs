// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Alley rendering and accepted player updates, independent of host presentation.

pub mod alley;
pub mod assets;
mod bin_cat;
mod birdcage;
mod bookcase;
pub mod cga;
mod cheese;
mod clothes;
mod completion;
mod decoration;
#[cfg(feature = "dev-tools")]
pub mod dev_tools;
mod dog;
mod failure;
mod fishbowl;
mod font;
mod hearts;
mod hearts_completion;
mod mice;
mod milk;
#[cfg(feature = "sdl")]
pub mod platform;
pub mod player;
mod projectiles;
mod room;
pub mod runtime;
pub mod splash;
mod swimming;
pub mod timing;
mod windows;
