// Copyright (c) 2026 analogued. Project license has not been selected.
//! Received scan transitions on Mach 3's pause-adjusted scheduling timeline.

#[cfg(feature = "dev-tools")]
use crate::gameplay::SessionState;
use reforge_sdl::{EventClockSample, HostPause, sdl3::event::Event};
#[cfg(feature = "dev-tools")]
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy)]
pub(super) struct EventTime {
    pub host: Instant,
    pub timeline: Instant,
    #[cfg(feature = "dev-tools")]
    pub now: Instant,
}

impl EventTime {
    pub fn sample(
        event: &Event,
        clock: EventClockSample,
        pause: &HostPause,
        received: Instant,
    ) -> Self {
        let timestamp = event.get_timestamp();
        // Use the middle of the measured correlation bracket. Unknown, future,
        // or unrepresentable timestamps take effect at receipt, without waiting.
        let host = if timestamp == 0 || timestamp > clock.sdl_ns {
            received
        } else {
            (clock.host + clock.span / 2)
                .checked_sub(std::time::Duration::from_nanos(clock.sdl_ns - timestamp))
                .unwrap_or(received)
                .min(received)
        };
        Self {
            host,
            timeline: pause.now(host),
            #[cfg(feature = "dev-tools")]
            now: pause.now(received),
        }
    }
}

/// Reject SDL events queued before an authoritative host reset.
#[derive(Default)]
pub(super) struct InputEpoch {
    epoch: Option<Instant>,
}

impl InputEpoch {
    pub fn accepts(&self, host: Instant) -> bool {
        self.epoch.is_none_or(|epoch| host >= epoch)
    }

    pub fn reset(&mut self, host: Instant) {
        self.epoch = Some(self.epoch.map_or(host, |old| old.max(host)));
    }
}

#[cfg(feature = "dev-tools")]
#[derive(Default)]
pub(super) struct InputHistory {
    pending: VecDeque<(Instant, Change)>,
    consumed: Option<Instant>,
    ordered: Option<Instant>,
    epoch: InputEpoch,
}

#[cfg(feature = "dev-tools")]
enum Change {
    Scan(u8),
    Joystick(u8),
}

#[cfg(feature = "dev-tools")]
impl InputHistory {
    pub fn queued_at(&self) -> Option<Instant> {
        self.ordered
    }

    pub fn accepts(&self, host: Instant) -> bool {
        self.epoch.accepts(host)
    }

    pub fn push(&mut self, at: Instant, scan: u8) {
        self.push_change(at, Change::Scan(scan));
    }

    pub fn push_joystick(&mut self, at: Instant, mask: u8) {
        self.push_change(at, Change::Joystick(mask & 31));
    }

    fn push_change(&mut self, at: Instant, change: Change) {
        // Never rewrite executed polls or reorder delivered make/break bytes.
        // Equal/nonmonotonic timestamps retain receipt order. A late-delivered
        // transition becomes eligible at the next unexecuted poll.
        let at = self
            .consumed
            .into_iter()
            .chain(self.ordered)
            .fold(at, Instant::max);
        self.pending.push_back((at, change));
        self.ordered = Some(at);
    }

    pub fn apply_until(&mut self, session: &mut SessionState, at: Instant) {
        while self.pending.front().is_some_and(|(time, _)| *time <= at) {
            match self.pending.pop_front().unwrap().1 {
                Change::Scan(scan) => session.keyboard.apply_scan(scan),
                Change::Joystick(mask) => session.joystick = mask,
            }
        }
        self.consumed = Some(self.consumed.map_or(at, |old| old.max(at)));
    }

    /// An authoritative frontend reset discards pending scans and rejects
    /// buffered events from the preceding host epoch. The caller clears the
    /// keyboard/freshness without changing native pause or control counters.
    pub fn reset(&mut self, time: EventTime) {
        self.pending.clear();
        self.ordered = None;
        self.consumed = Some(self.consumed.map_or(time.now, |old| old.max(time.now)));
        self.epoch.reset(time.host);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dev-tools")]
    #[test]
    fn scheduled_controller_changes_preserve_keyboard_and_late_delivery_policy() {
        let start = Instant::now();
        let mut session = SessionState::default();
        let mut history = InputHistory::default();
        history.push(start, 0x4d);
        history.push_joystick(start + Duration::from_millis(20), 9);
        history.push_joystick(start + Duration::from_millis(40), 0);
        history.apply_until(&mut session, start + Duration::from_millis(10));
        assert_eq!(session.joystick, 0);
        history.apply_until(&mut session, start + Duration::from_millis(30));
        assert_eq!(session.joystick, 9);
        history.apply_until(&mut session, start + Duration::from_millis(50));
        assert_eq!(session.joystick, 0);
        assert_eq!(session.keyboard.held_keys(0).actions, 8);
        history.push_joystick(start, 16); // Arrived after the already executed poll.
        assert_eq!(session.joystick, 0);
        history.apply_until(&mut session, start + Duration::from_millis(50));
        assert_eq!(session.joystick, 16);
    }
    use super::*;
    use std::time::Duration;

    #[test]
    fn input_epoch_rejects_buffered_events_after_host_reset() {
        let start = Instant::now();
        let mut epoch = InputEpoch::default();
        assert!(epoch.accepts(start));
        let reset = start + Duration::from_secs(1);
        epoch.reset(reset);
        assert!(!epoch.accepts(start));
        assert!(epoch.accepts(reset));
        epoch.reset(start);
        assert!(!epoch.accepts(start));
    }

    #[test]
    fn clock_correlation_clamps_unknown_and_future_timestamps_to_receipt() {
        let start = Instant::now();
        let clock = EventClockSample {
            host: start + Duration::from_millis(100),
            sdl_ns: 100_000_000,
            span: Duration::from_micros(2),
        };
        let received = clock.host + clock.span;
        let pause = HostPause::default();
        let time = EventTime::sample(
            &Event::Quit {
                timestamp: 50_000_000,
            },
            clock,
            &pause,
            received,
        );
        assert_eq!(
            time.host,
            start + Duration::from_millis(50) + Duration::from_micros(1)
        );
        assert_eq!(time.timeline, time.host);
        #[cfg(feature = "dev-tools")]
        assert_eq!(time.now, received);
        for timestamp in [0, 100_000_001, u64::MAX] {
            let time = EventTime::sample(&Event::Quit { timestamp }, clock, &pause, received);
            assert_eq!((time.host, time.timeline), (received, received));
            #[cfg(feature = "dev-tools")]
            assert_eq!(time.now, received);
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn resumed_epoch_excludes_paused_events_and_preserves_existing_deadlines() {
        let start = Instant::now();
        let frozen = start + Duration::from_millis(20);
        let resumed = start + Duration::from_secs(60);
        let mut pause = HostPause::default();
        pause.toggle(frozen);
        pause.toggle(resumed);
        let mut history = InputHistory::default();
        history.reset(EventTime {
            host: resumed,
            timeline: frozen,
            now: frozen,
        });
        let clock = EventClockSample {
            host: resumed + Duration::from_millis(10),
            sdl_ns: 60_010_000_000,
            span: Duration::ZERO,
        };
        let old = EventTime::sample(
            &Event::Quit {
                timestamp: 59_999_999_999,
            },
            clock,
            &pause,
            clock.host,
        );
        assert!(!history.accepts(old.host));
        let fresh = EventTime::sample(
            &Event::Quit {
                timestamp: 60_005_000_000,
            },
            clock,
            &pause,
            clock.host,
        );
        assert!(history.accepts(fresh.host));
        assert_eq!(fresh.timeline, frozen + Duration::from_millis(5));
        history.push(fresh.timeline, 0x4d);
        let mut session = SessionState::default();
        history.apply_until(&mut session, frozen);
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        history.apply_until(&mut session, fresh.timeline);
        assert_eq!(session.keyboard.held_keys(0).actions, 8);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn receipt_order_and_engine_table_clears_preserve_dos_duplicate_filtering() {
        let start = Instant::now();
        let mut history = InputHistory::default();
        let mut session = SessionState::default();
        history.push(start, 0x4d);
        history.apply_until(&mut session, start);
        assert_eq!(session.keyboard.held_keys(0).actions, 8);
        session.finish_audio_input(0);
        history.push(start, 0x4d);
        history.apply_until(&mut session, start);
        // Playback's table clear keeps the last raw byte; this identical make
        // stays suppressed. History never restores a held-state snapshot.
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        for (delta, raw) in [(2, 0xcd), (1, 0x31), (1, 0x31), (2, 0xb1)] {
            history.push(start + Duration::from_millis(delta), raw);
        }
        history.apply_until(&mut session, start + Duration::from_millis(1));
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        history.apply_until(&mut session, start + Duration::from_millis(2));
        assert_eq!(session.keyboard.held_keys(0).actions, 0);
        assert!(history.pending.is_empty());
    }
}
