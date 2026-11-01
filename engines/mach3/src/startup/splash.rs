// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The startup DOS-clock wait at 5df0, before installing game timer handlers.

use reforge_audio::PIT_HZ;

/// Channel zero is reprogrammed directly, without the game's counter helper.
#[cfg(feature = "sdl")]
pub(crate) const SPLASH_DIVISOR: u16 = 0x2a30;
const BIOS_DAY_TICKS: u32 = 0x1800b0;

/// DOS seconds derived from BIOS ticks, not seconds on the accelerated PIT.
/// The initial BIOS phase is explicit; ordinary playback pins it to zero.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SplashWait {
    bios_ticks: u32,
    first_second: u8,
    complete: bool,
}

impl Default for SplashWait {
    fn default() -> Self {
        Self::new(0)
    }
}

impl SplashWait {
    pub(crate) fn new(bios_ticks: u32) -> Self {
        let bios_ticks = bios_ticks % BIOS_DAY_TICKS;
        Self {
            bios_ticks,
            first_second: dos_second(bios_ticks),
            complete: false,
        }
    }

    /// The original BIOS handler advances time once per rising channel-zero edge.
    pub(crate) fn interrupt(&mut self) -> bool {
        self.bios_ticks = (self.bios_ticks + 1) % BIOS_DAY_TICKS;
        let second = dos_second(self.bios_ticks);
        let elapsed = if second < self.first_second {
            second + 60 - self.first_second
        } else {
            second - self.first_second
        };
        self.complete |= elapsed >= 12;
        self.complete
    }
}

fn dos_second(bios_ticks: u32) -> u8 {
    ((u64::from(bios_ticks) * 65_536 / PIT_HZ) % 60) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(mut wait: SplashWait) -> u32 {
        (1..=219).find(|_| wait.interrupt()).unwrap()
    }

    #[test]
    fn zero_phase_wait_uses_219_bios_interrupts() {
        let mut wait = SplashWait::default();
        for _ in 0..218 {
            assert!(!wait.interrupt());
        }
        assert!(wait.interrupt());
    }

    #[test]
    fn second_phase_and_minute_rollover_change_the_tick_count() {
        // Authored clock phases: the integer DOS second is already underway.
        assert_eq!(count(SplashWait::new(9)), 210);
        assert_eq!(dos_second(1001), 54);
        assert_eq!(count(SplashWait::new(1001)), 201);
    }

    #[test]
    fn bios_midnight_wrap_keeps_the_seconds_comparison() {
        assert_eq!(dos_second(BIOS_DAY_TICKS - 1), 59);
        let mut wait = SplashWait::new(BIOS_DAY_TICKS - 1);
        assert!(!wait.interrupt());
        assert_eq!(wait.bios_ticks, 0);
        assert_eq!(count(wait), 201);
    }
}
