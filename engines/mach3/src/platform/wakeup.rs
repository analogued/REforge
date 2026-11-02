// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host polling only: device deadlines retain their existing simulation times.

use reforge_audio::PIT_HZ;
use std::time::{Duration, Instant};

const MAX_SLEEP: Duration = Duration::from_millis(2);

/// Sleep no later than an already scheduled simulation event, with the existing
/// input/audio polling bound as a ceiling. `now` uses HostPause's frozen timeline.
/// Round tick conversion upward so a fractional nanosecond cannot cause a spin
/// just before the corresponding simulation budget reaches that tick.
pub(super) fn delay(epoch: Instant, now: Instant, next_tick: Option<u64>) -> Duration {
    let Some(tick) = next_tick else {
        return MAX_SLEEP;
    };
    let deadline_ns = (u128::from(tick) * 1_000_000_000).div_ceil(u128::from(PIT_HZ));
    let elapsed_ns = now.saturating_duration_since(epoch).as_nanos();
    Duration::from_nanos(
        deadline_ns
            .saturating_sub(elapsed_ns)
            .min(MAX_SLEEP.as_nanos()) as u64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imminent_and_overdue_events_shorten_the_polling_ceiling() {
        let epoch = Instant::now();
        assert_eq!(delay(epoch, epoch, None), MAX_SLEEP);
        assert_eq!(delay(epoch, epoch, Some(PIT_HZ)), MAX_SLEEP);
        let deadline = Duration::from_nanos(1_000_000_000_u64.div_ceil(PIT_HZ));
        assert_eq!(delay(epoch, epoch, Some(1)), deadline);
        assert_eq!(delay(epoch, epoch + deadline, Some(1)), Duration::ZERO);
        assert_eq!(delay(epoch, epoch + MAX_SLEEP, Some(1)), Duration::ZERO);
        assert_eq!(delay(epoch, epoch, Some(u64::MAX)), MAX_SLEEP);
        assert_eq!(delay(epoch, epoch, Some(0)), Duration::ZERO);
    }

    #[test]
    fn rendering_time_and_host_pause_do_not_shift_the_deadline() {
        let epoch = Instant::now();
        let next = 2_000;
        let first = delay(epoch, epoch, Some(next));
        let work = Duration::from_micros(700);
        assert_eq!(delay(epoch, epoch + work, Some(next)), first - work);

        let mut pause = reforge_sdl::HostPause::default();
        pause.toggle(epoch + work);
        let resumed = epoch + Duration::from_secs(60);
        assert_eq!(delay(epoch, pause.now(resumed), Some(next)), first - work);
        pause.toggle(resumed);
        assert_eq!(delay(epoch, pause.now(resumed), Some(next)), first - work);
    }
}
