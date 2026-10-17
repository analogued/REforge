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
    pub(super) fn wait(&mut self, limit: FrameLimit, refresh: Option<f32>, synchronized: bool) {
        // Avoid a redundant sleep beside VSync when its refresh already meets
        // the requested ceiling. Timer jitter must not miss the next vblank.
        let sync_covers_limit = match limit {
            FrameLimit::Display | FrameLimit::Unlimited => true,
            FrameLimit::Fps60 => refresh.is_some_and(|hz| hz <= 60.5),
            FrameLimit::Fps120 => refresh.is_some_and(|hz| hz <= 120.5),
        };
        if synchronized && sync_covers_limit {
            self.previous_start = None;
            return;
        }
        let Some(period) = interval(limit, refresh) else {
            self.previous_start = None;
            return;
        };
        // Count rendering and VSync waits toward the interval. Starting the next
        // interval after Present would cap a synchronized display at half speed.
        let delay = self.remaining(Instant::now(), period);
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
