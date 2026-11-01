// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Host control history mapped onto the retained gameplay timeline. This does
//! not emulate keyboard interrupts or retain taps until the game consumes them.

use crate::player::Controls;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Key {
    Up,
    Down,
    Left,
    Right,
    LeftAlt,
    RightAlt,
    PadUp,
    PadDown,
    PadLeft,
    PadRight,
    PadAction,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Keys(u16);

impl Keys {
    pub fn set(&mut self, key: Key, pressed: bool) {
        let mask = 1 << key as u8;
        if pressed {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }

    pub fn controls(self) -> (Controls, bool) {
        let down = |key| self.0 & (1 << key as u8) != 0;
        (
            Controls {
                x: if down(Key::Left) || down(Key::PadLeft) {
                    -1
                } else {
                    i8::from(down(Key::Right) || down(Key::PadRight))
                },
                y: if down(Key::Up) || down(Key::PadUp) {
                    -1
                } else {
                    i8::from(down(Key::Down) || down(Key::PadDown))
                },
            },
            down(Key::LeftAlt) || down(Key::RightAlt) || down(Key::PadAction),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HostChange {
    pub at_ns: u64,
    pub keys: Keys,
}

pub(crate) struct ControlHistory {
    held: Keys,
    pending: VecDeque<(u128, Keys)>,
}

impl ControlHistory {
    pub fn new(held: Keys) -> Self {
        Self {
            held,
            pending: VecDeque::new(),
        }
    }

    /// Retain the latest part of a host interval, matching the runner's stall
    /// clamp. Changes in discarded time collapse to the interval's start;
    /// previous catch-up debt retains its own, earlier input history.
    pub fn extend(
        &mut self,
        host_end_ns: u64,
        retained: Duration,
        guest_start_ns: u128,
        changes: impl IntoIterator<Item = HostChange>,
    ) {
        let retained_ns = retained.as_nanos().min(u128::from(host_end_ns)) as u64;
        let host_start_ns = host_end_ns - retained_ns;
        for change in changes {
            let offset = change.at_ns.saturating_sub(host_start_ns).min(retained_ns);
            self.boundary(guest_start_ns + u128::from(offset), change.keys);
        }
    }

    /// A pause/settings boundary has no elapsed guest time. Keep older debt,
    /// but never replay keys pressed during the pause across that debt.
    pub fn boundary(&mut self, guest_ns: u128, keys: Keys) {
        // Preserve SDL queue order for equal, stale or out-of-order timestamps.
        let at = self
            .pending
            .back()
            .map_or(guest_ns, |(last, _)| guest_ns.max(*last));
        let previous = self.pending.back().map_or(self.held, |(_, keys)| *keys);
        if keys != previous {
            self.pending.push_back((at, keys));
        }
    }

    pub fn sample(&mut self, guest_ns: u128) -> (Controls, bool) {
        while self.pending.front().is_some_and(|(at, _)| *at <= guest_ns) {
            self.held = self.pending.pop_front().unwrap().1;
        }
        self.held.controls()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn controller_and_keyboard_releases_remain_independent_at_scheduled_polls() {
        let mut keys = Keys::default();
        keys.set(Key::Right, true);
        keys.set(Key::PadRight, true);
        keys.set(Key::PadUp, true);
        keys.set(Key::LeftAlt, true);
        keys.set(Key::PadAction, true);
        let mut history = ControlHistory::new(keys);
        keys.set(Key::PadRight, false);
        keys.set(Key::PadAction, false);
        history.boundary(100, keys);
        assert_eq!(history.sample(99), (Controls { x: 1, y: -1 }, true));
        assert_eq!(history.sample(100), (Controls { x: 1, y: -1 }, true));
        keys.set(Key::Right, false);
        keys.set(Key::LeftAlt, false);
        history.boundary(200, keys);
        assert_eq!(history.sample(200), (Controls { x: 0, y: -1 }, false));
        keys.set(Key::PadUp, false);
        history.boundary(300, keys);
        assert_eq!(history.sample(300), (Controls::default(), false));
    }
    use super::*;

    fn right() -> Keys {
        let mut keys = Keys::default();
        keys.set(Key::Right, true);
        keys
    }

    fn ms(value: u64) -> u64 {
        value * 1_000_000
    }

    #[test]
    fn delayed_polls_observe_input_at_their_own_time() {
        for (press, release, expected) in [(60, 80, 0), (100, 120, 1), (120, 140, 0)] {
            let mut history = ControlHistory::new(Keys::default());
            history.extend(
                ms(150),
                Duration::from_millis(100),
                u128::from(ms(50)),
                [
                    HostChange {
                        at_ns: ms(press),
                        keys: right(),
                    },
                    HostChange {
                        at_ns: ms(release),
                        keys: Keys::default(),
                    },
                ],
            );
            assert_eq!(history.sample(u128::from(ms(110))).0.x, expected);
            assert_eq!(history.sample(u128::from(ms(150))).0.x, 0);
        }
    }

    #[test]
    fn new_events_do_not_change_earlier_catch_up_debt() {
        let mut history = ControlHistory::new(Keys::default());
        history.extend(
            ms(100),
            Duration::from_millis(100),
            0,
            [HostChange {
                at_ns: ms(90),
                keys: right(),
            }],
        );
        // Only 20 ms of the first batch have been executed when more input is pumped.
        assert_eq!(history.sample(u128::from(ms(20))).0.x, 0);
        history.extend(
            ms(120),
            Duration::from_millis(20),
            u128::from(ms(100)),
            [HostChange {
                at_ns: ms(110),
                keys: Keys::default(),
            }],
        );
        assert_eq!(history.sample(u128::from(ms(80))).0.x, 0);
        assert_eq!(history.sample(u128::from(ms(100))).0.x, 1);
        assert_eq!(history.sample(u128::from(ms(115))).0.x, 0);
    }

    #[test]
    fn discarded_stall_time_does_not_replay_a_complete_old_tap() {
        let mut history = ControlHistory::new(Keys::default());
        history.extend(
            ms(500),
            Duration::from_millis(100),
            u128::from(ms(20)),
            [
                HostChange {
                    at_ns: ms(100),
                    keys: right(),
                },
                HostChange {
                    at_ns: ms(200),
                    keys: Keys::default(),
                },
                HostChange {
                    at_ns: ms(450),
                    keys: right(),
                },
            ],
        );
        assert_eq!(history.sample(u128::from(ms(20))).0.x, 0);
        assert_eq!(history.sample(u128::from(ms(69))).0.x, 0);
        assert_eq!(history.sample(u128::from(ms(70))).0.x, 1);
    }

    #[test]
    fn a_key_held_across_discarded_time_is_still_held() {
        let mut history = ControlHistory::new(Keys::default());
        history.extend(
            ms(500),
            Duration::from_millis(100),
            0,
            [HostChange {
                at_ns: ms(100),
                keys: right(),
            }],
        );
        assert_eq!(history.sample(0).0.x, 1);
    }

    #[test]
    fn settings_boundary_keeps_debt_but_not_paused_taps() {
        let mut history = ControlHistory::new(Keys::default());
        history.boundary(u128::from(ms(60)), right());
        history.boundary(u128::from(ms(100)), Keys::default());
        // Resume much later at the same guest deadline; no paused event times enter it.
        history.boundary(u128::from(ms(100)), Keys::default());
        history.boundary(u128::from(ms(120)), right());
        assert_eq!(history.sample(u128::from(ms(50))).0.x, 0);
        assert_eq!(history.sample(u128::from(ms(70))).0.x, 1);
        assert_eq!(history.sample(u128::from(ms(100))).0.x, 0);
        assert_eq!(history.sample(u128::from(ms(120))).0.x, 1);
        // Manual pause/restart drops pending pacing debt, as before.
        history = ControlHistory::new(Keys::default());
        assert_eq!(history.sample(u128::from(ms(70))).0.x, 0);
    }

    #[test]
    fn opposing_directions_and_two_alt_keys_keep_existing_precedence() {
        let mut keys = right();
        for key in [Key::Left, Key::Up, Key::Down, Key::LeftAlt, Key::RightAlt] {
            keys.set(key, true);
        }
        assert_eq!(keys.controls(), (Controls { x: -1, y: -1 }, true));
        keys.set(Key::LeftAlt, false);
        assert!(keys.controls().1);
        keys.set(Key::RightAlt, false);
        assert!(!keys.controls().1);
    }

    #[test]
    fn host_pump_partition_does_not_change_guest_poll_results() {
        let changes = [
            HostChange {
                at_ns: ms(60),
                keys: right(),
            },
            HostChange {
                at_ns: ms(80),
                keys: Keys::default(),
            },
            HostChange {
                at_ns: ms(100),
                keys: right(),
            },
            HostChange {
                at_ns: ms(120),
                keys: Keys::default(),
            },
        ];
        let replay = |ends: &[u64]| {
            let mut history = ControlHistory::new(Keys::default());
            let mut start = 0;
            let mut polls = Vec::new();
            for &end in ends {
                history.extend(
                    ms(end),
                    Duration::from_millis(end - start),
                    u128::from(ms(start)),
                    changes
                        .into_iter()
                        .filter(|c| c.at_ns > ms(start) && c.at_ns <= ms(end)),
                );
                start = end;
            }
            for at in [50, 70, 90, 110, 130] {
                polls.push(history.sample(u128::from(ms(at))).0.x);
            }
            polls
        };
        assert_eq!(replay(&[50, 150]), [0, 1, 0, 1, 0]);
        assert_eq!(replay(&[30, 60, 90, 120, 150]), replay(&[50, 150]));
    }
}
