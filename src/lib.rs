// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Alley rendering and accepted player updates, independent of host presentation.

pub mod alley;
pub mod assets;
pub mod cga;
#[cfg(feature = "sdl")]
pub mod platform;
pub mod player;
