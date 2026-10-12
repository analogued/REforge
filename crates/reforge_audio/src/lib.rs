// Copyright (c) 2026 analogued. Project license has not been selected.
//! Deterministic device execution, independent of synthesis and host playback.
#![forbid(unsafe_code)]

pub mod pit;
pub mod speaker;

/// Explicit nominal PC PIT clock frequency. Device execution uses integer clocks.
pub const PIT_HZ: u64 = 1_193_182;
