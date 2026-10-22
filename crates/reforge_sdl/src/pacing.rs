// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Presentation ceiling only. Engine clocks continue to measure elapsed wall time.
use reforge_settings::FrameLimit;
use std::time::{Duration, Instant};
#[derive(Default)]
pub(super) struct FramePacer {
    previous_start: Option<Instant>,
}
fn interval(limit: FrameLimit, refresh: Option<f32>) -> Option<Duration> {
    let fps = match limit {
        FrameLimit::Unlimited => return None,
        FrameLimit::Fps60 => 60.0,
        FrameLimit::Fps120 => 120.0,
        FrameLimit::Display => refresh
            .filter(|v| v.is_finite() && *v >= 1.0)
            .unwrap_or(60.0) as f64,
    };
    Some(Duration::from_secs_f64(1.0 / fps))
}
impl FramePacer {
    fn remaining(&self, now: Instant, period: Duration) -> Duration {
        self.previous_start
            .map(|start| period.saturating_sub(now.saturating_duration_since(start)))
            .unwrap_or_default()
    }
    fn delay(
        &self,
        now: Instant,
        limit: FrameLimit,
        refresh: Option<f32>,
        synchronized: bool,
    ) -> Option<Duration> {
        // Avoid a redundant sleep beside VSync when its refresh already meets
        // the requested ceiling. Timer jitter must not miss the next vblank.
        let sync_covers_limit = match limit {
            FrameLimit::Display | FrameLimit::Unlimited => true,
            FrameLimit::Fps60 => refresh.is_some_and(|hz| hz <= 60.5),
            FrameLimit::Fps120 => refresh.is_some_and(|hz| hz <= 120.5),
        };
        if synchronized && sync_covers_limit {
            return None;
        }
        interval(limit, refresh).map(|period| self.remaining(now, period))
    }
    pub(super) fn ready(
        &self,
        now: Instant,
        limit: FrameLimit,
        refresh: Option<f32>,
        synchronized: bool,
    ) -> bool {
        self.delay(now, limit, refresh, synchronized)
            .is_none_or(|delay| delay.is_zero())
    }
    pub(super) fn wait(&mut self, limit: FrameLimit, refresh: Option<f32>, synchronized: bool) {
        let Some(delay) = self.delay(Instant::now(), limit, refresh, synchronized) else {
            self.previous_start = None;
            return;
        };
        // Count rendering and VSync waits toward the interval. Starting the next
        // interval after Present would cap a synchronized display at half speed.
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        // Stalls do not accumulate a burst of presentation debt.
        self.previous_start = Some(Instant::now());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_uses_selected_ceiling_without_blocking_host_work() {
        let start = Instant::now();
        for (limit, refresh, frames) in [
            (FrameLimit::Fps60, None, 58),
            (FrameLimit::Fps120, None, 111),
            (FrameLimit::Display, Some(144.0), 142),
            (FrameLimit::Unlimited, None, 1000),
        ] {
            let mut pacer = FramePacer {
                previous_start: Some(start),
            };
            let mut presented = 0;
            let mut guest_passes = 0;
            // An event/simulation pass every millisecond must remain possible
            // even when the next presentation is not due. Readiness itself
            // never consumes an opportunity or mutates the presentation clock.
            for ms in 1..=1000 {
                guest_passes += 1;
                let now = start + Duration::from_millis(ms);
                let due = pacer.ready(now, limit, refresh, false);
                assert_eq!(pacer.ready(now, limit, refresh, false), due);
                if due {
                    presented += 1;
                    // Model the start recorded by wait() on a successful frame.
                    pacer.previous_start = Some(now);
                }
            }
            assert_eq!(guest_passes, 1000);
            assert_eq!(presented, frames);
        }
    }
    #[test]
    fn readiness_respects_vsync_stalls_and_live_limits() {
        let start = Instant::now();
        let pacer = FramePacer {
            previous_start: Some(start),
        };
        let now = start + Duration::from_millis(9);
        assert!(!pacer.ready(now, FrameLimit::Fps60, Some(144.0), false));
        assert!(pacer.ready(now, FrameLimit::Fps120, Some(144.0), false));
        assert!(pacer.ready(now, FrameLimit::Unlimited, None, false));
        // VSync alone owns the ceiling when the refresh is slow enough.
        assert!(pacer.ready(start, FrameLimit::Fps120, Some(60.0), true));
        assert!(pacer.ready(start, FrameLimit::Display, Some(144.0), true));
        assert!(!pacer.ready(start, FrameLimit::Fps60, Some(144.0), true));
        assert!(pacer.ready(
            start + Duration::from_secs(2),
            FrameLimit::Fps60,
            None,
            false
        ));
        assert!(FramePacer::default().ready(start, FrameLimit::Fps60, None, false));
    }
    #[test]
    fn vsync_and_stalls_count_once_without_burst_debt() {
        let start = Instant::now();
        let mut p = FramePacer {
            previous_start: Some(start),
        };
        let period = interval(FrameLimit::Fps60, None).unwrap();
        assert_eq!(
            p.remaining(start + Duration::from_millis(10), period),
            period - Duration::from_millis(10)
        );
        assert_eq!(p.remaining(start + period, period), Duration::ZERO);
        assert_eq!(
            p.remaining(start + Duration::from_secs(2), period),
            Duration::ZERO
        );
        p.previous_start = Some(start + Duration::from_secs(2));
        assert_eq!(p.remaining(start + Duration::from_secs(2), period), period);
    }
    #[test]
    fn cap_waits_without_vsync_and_sync_does_not_add_timer_debt() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        for _ in 0..4 {
            pacer.wait(FrameLimit::Fps120, Some(60.0), false);
        }
        assert!(start.elapsed() >= Duration::from_millis(25));
        pacer.wait(FrameLimit::Display, Some(60.0), true);
        assert!(pacer.previous_start.is_none());
        pacer.wait(FrameLimit::Fps120, Some(60.0), true);
        assert!(pacer.previous_start.is_none());
        pacer.wait(FrameLimit::Unlimited, None, false);
        assert!(pacer.previous_start.is_none());
    }
    #[test]
    fn refresh_fractional_unknown_and_unlimited() {
        assert_eq!(
            interval(FrameLimit::Display, None),
            interval(FrameLimit::Fps60, None)
        );
        assert_eq!(
            interval(FrameLimit::Display, Some(f32::NAN)),
            interval(FrameLimit::Fps60, None)
        );
        assert!(
            interval(FrameLimit::Display, Some(59.94)).unwrap()
                > interval(FrameLimit::Fps60, None).unwrap()
        );
        assert_eq!(interval(FrameLimit::Unlimited, None), None);
    }
}
