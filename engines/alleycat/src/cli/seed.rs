// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host startup policy; native seed construction and the shared PRNG stay in assets.

use super::Options;
use crate::{
    assets,
    timing::{RetraceProfile, VirtualClock},
};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn resolve(options: &Options) -> Result<u16, String> {
    #[cfg(feature = "dev-tools")]
    let diagnostic =
        options.dev.headless || options.dev.scene_selected || options.dev.step_hz.is_some();
    #[cfg(not(feature = "dev-tools"))]
    let diagnostic = false;
    choose(options.seed, diagnostic, host_counter)
}

fn host_counter() -> Result<u16, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Cannot sample the startup clock: {error}"))?;
    // DOS's physical PIT registers are not available on a modern host.
    // Use wall time as the phase of the existing mode-3, divisor-65536 counter.
    // This clock is sampled once for seeding, never installed as gameplay time.
    let mut clock = VirtualClock::new(RetraceProfile::default());
    clock.advance(elapsed);
    Ok(clock.pit_counter())
}

fn choose(
    explicit: Option<u16>,
    diagnostic: bool,
    mut counter: impl FnMut() -> Result<u16, String>,
) -> Result<u16, String> {
    if let Some(seed) = explicit {
        assets::validate_startup_seed(seed)?;
        return Ok(seed);
    }
    if diagnostic {
        return Ok(0xfa59);
    }
    loop {
        let seed = assets::startup_seed_from_pit(counter()?);
        // Native seed 1 reaches the absorbing zero state and hangs scenery.
        // Preserve the existing correction by sampling again, not changing the
        // native byte swap, zero fallback or subsequent random sequence.
        if seed != 1 {
            return Ok(seed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_counter_byte_order_and_zero_fallback_are_preserved() {
        for (count, seed) in [
            (0, 0xfa59),
            (0x1234, 0x3412),
            (0x0100, 1),
            (0x0001, 0x0100),
            (0xffff, 0xffff),
        ] {
            assert_eq!(assets::startup_seed_from_pit(count), seed);
        }
        // Every nonzero timer word has exactly its two source bytes exchanged.
        for count in 1..=u16::MAX {
            let seed = assets::startup_seed_from_pit(count);
            assert_eq!(seed.to_le_bytes(), count.to_be_bytes());
        }
    }

    #[test]
    fn explicit_and_diagnostic_seeds_never_sample_host_time() {
        for seed in [2, 0x1234, 0xfa59, 65535] {
            assert_eq!(
                choose(Some(seed), false, || panic!("explicit seed read the clock")).unwrap(),
                seed
            );
            assert_eq!(
                choose(Some(seed), true, || panic!("explicit seed read the clock")).unwrap(),
                seed
            );
        }
        for seed in [0, 1] {
            assert!(choose(Some(seed), false, || panic!("invalid seed read the clock")).is_err());
        }
        assert_eq!(
            choose(None, true, || panic!("diagnostic read the clock")).unwrap(),
            0xfa59
        );
    }

    #[test]
    fn automatic_seed_uses_the_sample_and_retries_only_the_known_bad_seed() {
        let mut samples = [0x0100, 0x0200].into_iter();
        assert_eq!(
            choose(None, false, || Ok(samples.next().unwrap())).unwrap(),
            2
        );
        assert!(samples.next().is_none());
        assert_eq!(choose(None, false, || Ok(0)).unwrap(), 0xfa59);
        assert_eq!(choose(None, false, || Ok(0x1234)).unwrap(), 0x3412);
        assert_eq!(
            choose(None, false, || Err("sample failed".into())).unwrap_err(),
            "sample failed"
        );
    }
}
