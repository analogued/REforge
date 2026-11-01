// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic device execution, independent of synthesis and host playback.
#![forbid(unsafe_code)]

pub mod category;
pub mod pit;
pub mod render;
pub mod speaker;
pub mod wav;

/// Explicit nominal PC PIT clock frequency. Device execution uses integer clocks.
pub const PIT_HZ: u64 = 1_193_182;
