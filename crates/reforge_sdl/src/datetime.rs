// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! OS-local calendar presentation through SDL's cross-platform time conversion.

use std::time::{SystemTime, UNIX_EPOCH};

/// Format a filesystem timestamp as YYYY-MM-DD HH:MM in the system timezone,
/// including the daylight-saving offset at that instant.
pub fn format_local_timestamp(time: SystemTime) -> Option<String> {
    let ticks = match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_nanos()).ok(),
        Err(error) => i64::try_from(error.duration().as_nanos()).ok().map(|n| -n),
    }?;
    let mut local = sdl3::sys::time::SDL_DateTime::default();
    // SAFETY: initialized writable struct. SDL accepts epoch nanoseconds and
    // performs the OS-local conversion without requiring a window or context.
    if !unsafe { sdl3::sys::time::SDL_TimeToDateTime(ticks, &mut local, true) } {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.year, local.month, local.day, local.hour, local.minute
    ))
}
