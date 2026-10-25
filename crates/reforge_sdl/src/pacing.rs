// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Presentation ceiling only. Engine clocks continue to measure elapsed wall time.
use reforge_settings::FrameLimit;
use std::time::{Duration, Instant};
#[derive(Default)]
pub(super) struct FramePacer {
    next_start: Option<(Instant, Duration)>,
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
        self.next_start
            .filter(|(_, scheduled_period)| *scheduled_period == period)
            .map(|(deadline, _)| deadline.saturating_duration_since(now))
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
    fn record_start(&mut self, now: Instant, period: Duration) {
        let scheduled = self
            .next_start
            .filter(|(_, scheduled_period)| *scheduled_period == period)
            .map_or(now, |(deadline, _)| deadline);
        let next = scheduled + period;
        // Keep ordinary polling/render-preparation lateness from accumulating.
        // If a whole additional interval was missed, drop presentation debt:
        // recovery must not produce a burst of stale frames.
        self.next_start = Some((if next > now { next } else { now + period }, period));
    }
    pub(super) fn wait(&mut self, limit: FrameLimit, refresh: Option<f32>, synchronized: bool) {
        let Some(delay) = self.delay(Instant::now(), limit, refresh, synchronized) else {
            self.next_start = None;
            return;
        };
        // Count rendering and VSync waits toward the interval. Starting the next
        // interval after Present would cap a synchronized display at half speed.
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        self.record_start(Instant::now(), interval(limit, refresh).unwrap());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn armed(start: Instant, limit: FrameLimit, refresh: Option<f32>) -> FramePacer {
        let mut pacer = FramePacer::default();
        if let Some(period) = interval(limit, refresh) {
            pacer.record_start(start, period);
        }
        pacer
    }
    #[test]
    fn readiness_keeps_average_cadence_without_blocking_host_work() {
        let start = Instant::now();
        for (limit, refresh, frames) in [
            (FrameLimit::Fps60, None, 60),
            (FrameLimit::Fps120, None, 120),
            (FrameLimit::Display, Some(144.0), 144),
            (FrameLimit::Unlimited, None, 1000),
        ] {
            let mut pacer = armed(start, limit, refresh);
            let mut presented = 0;
            let mut guest_passes = 0;
            for ms in 1..=1000 {
                guest_passes += 1;
                let now = start + Duration::from_millis(ms);
                let due = pacer.ready(now, limit, refresh, false);
                assert_eq!(pacer.ready(now, limit, refresh, false), due);
                if due {
                    presented += 1;
                    if let Some(period) = interval(limit, refresh) {
                        pacer.record_start(now, period);
                    }
                }
            }
            assert_eq!(guest_passes, 1000);
            // Rounded nanosecond periods can put the final deadline just
            // beyond the last poll (60 Hz by 20 ns after one second).
            assert!((frames - 1..=frames).contains(&presented));
        }
    }
    #[test]
    fn polling_and_preparation_lateness_do_not_lower_the_average_limit() {
        let start = Instant::now();
        for poll_ms in [1, 2] {
            for limit in [FrameLimit::Fps60, FrameLimit::Fps120] {
                let period = interval(limit, None).unwrap();
                let mut pacer = armed(start, limit, None);
                let mut now = start;
                let mut frames = 0;
                while now < start + Duration::from_secs(10) {
                    if pacer.ready(now, limit, None, false) {
                        // Engines can generate/upload pixels after ready() and
                        // before wait(). This delay must not reset the cadence.
                        now += Duration::from_millis(3);
                        pacer.record_start(now, period);
                        frames += 1;
                    }
                    now += Duration::from_millis(poll_ms);
                }
                let target = if limit == FrameLimit::Fps60 {
                    600
                } else {
                    1200
                };
                assert!(
                    (target - 1..=target + 1).contains(&frames),
                    "{limit:?} poll={poll_ms}: {frames}"
                );
            }
        }
    }
    #[test]
    fn readiness_respects_vsync_stalls_and_live_limits() {
        let start = Instant::now();
        let pacer = armed(start, FrameLimit::Fps60, None);
        let now = start + Duration::from_millis(9);
        assert!(!pacer.ready(now, FrameLimit::Fps60, Some(144.0), false));
        // A changed limit starts afresh, independently of the old deadline.
        assert!(pacer.ready(start, FrameLimit::Fps120, Some(144.0), false));
        assert!(pacer.ready(now, FrameLimit::Unlimited, None, false));
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
    fn render_work_counts_once_and_stalls_discard_burst_debt() {
        let start = Instant::now();
        let period = interval(FrameLimit::Fps60, None).unwrap();
        let mut pacer = armed(start, FrameLimit::Fps60, None);
        assert_eq!(
            pacer.remaining(start + Duration::from_millis(10), period),
            period - Duration::from_millis(10)
        );
        assert_eq!(pacer.remaining(start + period, period), Duration::ZERO);
        let late = start + period + Duration::from_millis(2);
        pacer.record_start(late, period);
        assert_eq!(pacer.next_start, Some((start + period * 2, period)));
        let stalled = start + Duration::from_secs(2);
        assert_eq!(pacer.remaining(stalled, period), Duration::ZERO);
        pacer.record_start(stalled, period);
        assert_eq!(pacer.remaining(stalled, period), period);
        assert!(!pacer.ready(stalled, FrameLimit::Fps60, None, false));
        pacer.record_start(stalled + period, period);
        assert_eq!(pacer.next_start, Some((stalled + period * 2, period)));
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
        assert!(pacer.next_start.is_none());
        pacer.wait(FrameLimit::Fps120, Some(60.0), true);
        assert!(pacer.next_start.is_none());
        pacer.wait(FrameLimit::Unlimited, None, false);
        assert!(pacer.next_start.is_none());
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
