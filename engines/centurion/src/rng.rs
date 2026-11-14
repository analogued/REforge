// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#[derive(Clone, Copy)]
pub struct OriginalRng {
    pub first: u16,
    pub second: u16,
}

impl OriginalRng {
    pub fn bounded(&mut self, bound: u16) -> u16 {
        // Private prototype diagnostic for paired read-only DOS procedure traces.
        static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *TRACE.get_or_init(|| std::env::var_os("CENTURION_TRACE_RNG").is_some()) {
            eprintln!("rng {} {} {}", bound, self.first, self.second);
        }
        #[cfg(feature = "dev-tools")]
        let (first, second) = (self.first, self.second);
        self.first = self.first.wrapping_mul(0x7abd).wrapping_add(0x1b0f);
        self.second = self.second.wrapping_mul(0x1b0f).wrapping_add(0x7abd);
        let result = (((self.first ^ self.second) as u32 * bound as u32) >> 16) as u16;
        #[cfg(feature = "dev-tools")]
        crate::record::trace_rng(bound, first, second, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bound_still_advances_state() {
        let mut rng = OriginalRng {
            first: 0,
            second: 0,
        };
        assert_eq!(rng.bounded(0), 0);
        assert_eq!((rng.first, rng.second), (0x1b0f, 0x7abd));
        assert_eq!(rng.bounded(1), 0);
    }
}
