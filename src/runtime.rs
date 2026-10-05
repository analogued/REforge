// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Original alley gates over a deterministic reference clock.

use crate::alley::{Alley, Machine};
use crate::player::{Controls, Draw};
use crate::timing::{
    ControlsGate, FenceAction, FenceEffect, OuterLoopDivider, PlayerAcceptance, PlayerGate,
    VirtualClock, keyboard_wait_complete,
};

/// Scoped execution costs for the DOSBox-X normal-core, fixed-240 reference.
/// Values are virtual nanoseconds.
/// These are not physical 8088 cycle counts. Omitted actors consume a measured
/// time allowance here; their game state and individual gates are not simulated.
/// Clothesline state is implemented and active scrolling has a separate cost.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceCosts {
    pub prelude: u64,
    pub control_bios: u64,
    pub control_hold: u64,
    pub control_decode: u64,
    pub pit_read: u64,
    pub pit_start_setup: u64,
    pub pit_repeat: u64,
    pub control_return: u64,
    pub player_gate: u64,
    pub player_candidate: u64,
    pub player_countdown: u64,
    pub player_zero: u64,
    pub player_retrace: u64,
    pub player_retrace_return: u64,
    pub player_keep: u64,
    pub player_idle: u64,
    pub player_walk: u64,
    pub player_airborne: u64,
    pub player_descending: u64,
    pub player_landing: u64,
    pub player_support_release: u64,
    pub player_entry: u64,
    pub ground_dog: u64,
    pub actor_group: u64,
    pub clothes_shift: u64,
    pub clothes_new_tile: u64,
    pub loop_return: u64,
    pub fence_setup: u64,
    pub fence_bios: u64,
    pub fence_initial_wait: u64,
    pub fence_countdown: u64,
    pub fence_random: u64,
    pub fence_finish: u64,
}

impl ReferenceCosts {
    pub fn dosbox_x_240(_machine: Machine) -> Self {
        // Rounded guest-time stage medians from the fixed-240 reference.
        // Software-breakpoint observation and state-dependent paths limit these
        // fitted allowances; see docs/timing.md. Actor draw spikes are omitted.
        Self {
            prelude: 62_500,
            control_bios: 33_333,
            control_hold: 25_000,
            control_decode: 212_500,
            pit_read: 58_333,
            pit_start_setup: 12_500,
            pit_repeat: 79_167,
            control_return: 16_667,
            player_gate: 29_167,
            player_candidate: 58_333,
            player_countdown: 37_500,
            player_zero: 29_167,
            player_retrace: 37_500,
            player_retrace_return: 12_500,
            player_keep: 345_833,
            player_idle: 2_637_500,
            player_walk: 3_054_167,
            player_airborne: 3_270_833,
            player_descending: 3_783_333,
            player_landing: 30_720_833,
            player_support_release: 27_937_500,
            // Clipped re-entry is not separately cost-calibrated yet.
            player_entry: 2_875_000,
            ground_dog: 95_833,
            actor_group: 345_833,
            // Additional work above the ordinary grouped-call allowance.
            // Fixed-240 helper probes measured about 8ms for a band shift,
            // 9.5ms with tile generation, and 0.1ms for a tick-gated hold.
            clothes_shift: 7_900_000,
            clothes_new_tile: 9_400_000,
            // The observed post-player allowance includes loop return work.
            loop_return: 0,
            fence_setup: 29_167,
            fence_bios: 33_333,
            fence_initial_wait: 16_667,
            // Bounded uninterrupted helper probes give about 14 work slots
            // per counted check and 22 additional slots per random cycle.
            fence_countdown: 25_000,
            fence_random: 91_667,
            fence_finish: 54_167,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PassOutcome {
    pub start_nanos: u128,
    pub end_nanos: u128,
    pub tick_low: u16,
    pub pit_counter: u16,
    pub retrace: bool,
    pub control_polled: bool,
    pub acceptance: Option<PlayerAcceptance>,
    pub actor_group_due: bool,
    pub fence_wait: bool,
}

pub struct Runtime {
    pub clock: VirtualClock,
    pub player_gate: PlayerGate,
    pub controls_gate: ControlsGate,
    pub divider: OuterLoopDivider,
    pub controls: Controls,
    costs: ReferenceCosts,
    machine: Machine,
    fence: Option<FenceEffect>,
}

impl Runtime {
    pub fn new(machine: Machine) -> Self {
        Self::with_costs(machine, ReferenceCosts::dosbox_x_240(machine))
    }

    pub fn with_costs(machine: Machine, costs: ReferenceCosts) -> Self {
        assert!(
            costs.pit_repeat > 0,
            "PIT polling must advance virtual time"
        );
        Self {
            clock: VirtualClock::default(),
            player_gate: PlayerGate::new(u16::MAX, 0),
            controls_gate: ControlsGate::new(u16::MAX - 1),
            divider: OuterLoopDivider::new(0),
            controls: Controls::default(),
            costs,
            machine,
            fence: None,
        }
    }

    /// Execute a loop attempt, including attempts which reject player movement.
    /// Host input changes held keys; only the original input gate latches them.
    pub fn advance_pass(
        &mut self,
        alley: &mut Alley,
        held: Controls,
    ) -> Result<PassOutcome, String> {
        let start_nanos = self.clock.elapsed_nanos();
        if let Some(fence) = self.fence.as_mut() {
            self.clock.advance_nanos(self.costs.fence_bios);
            let sample = self.clock.sample();
            let action = fence.check(sample.bios.tick_low);
            match action {
                FenceAction::WaitingForStart => {
                    self.clock.advance_nanos(self.costs.fence_initial_wait);
                }
                FenceAction::Counting => {
                    self.clock.advance_nanos(self.costs.fence_countdown);
                }
                FenceAction::Random => {
                    self.clock.advance_nanos(self.costs.fence_countdown);
                    alley.advance_random();
                    self.clock.advance_nanos(self.costs.fence_random);
                }
                FenceAction::Complete => {
                    self.clock.advance_nanos(self.costs.fence_finish);
                    self.fence = None;
                    let mut event = self.finish_pass(alley, start_nanos, false, None)?;
                    event.fence_wait = true;
                    return Ok(event);
                }
            }
            return Ok(PassOutcome {
                start_nanos,
                end_nanos: self.clock.elapsed_nanos(),
                tick_low: sample.bios.tick_low,
                pit_counter: sample.pit_counter,
                retrace: sample.retrace,
                control_polled: false,
                acceptance: None,
                actor_group_due: false,
                fence_wait: true,
            });
        }

        self.clock.advance_nanos(self.costs.prelude);
        self.clock.advance_nanos(self.costs.control_bios);
        let control_polled = self
            .controls_gate
            .should_poll(self.clock.sample().bios.tick_low);
        if control_polled {
            self.controls = held;
            self.clock.advance_nanos(self.costs.control_decode);
            self.clock.advance_nanos(self.costs.pit_read);
            let start = self.clock.sample().pit_counter;
            self.clock.advance_nanos(self.costs.pit_start_setup);
            loop {
                self.clock.advance_nanos(self.costs.pit_repeat);
                if keyboard_wait_complete(start, self.clock.sample().pit_counter) {
                    break;
                }
            }
            self.clock.advance_nanos(self.costs.control_return);
        } else {
            self.clock.advance_nanos(self.costs.control_hold);
        }

        self.clock.advance_nanos(self.costs.player_gate);
        let sample = self.clock.sample();
        let candidate =
            sample.bios.tick_low != self.player_gate.last_tick || self.player_gate.countdown == 1;
        self.clock.advance_nanos(if candidate {
            self.costs.player_candidate
        } else if self.player_gate.countdown == 0 {
            self.costs.player_zero
        } else {
            self.costs.player_countdown
        });
        let tests_retrace = candidate
            && alley.player.horizontal_direction == 0
            && alley.player.vertical_direction == 0;
        if tests_retrace {
            self.clock.advance_nanos(self.costs.player_retrace);
        }
        let retrace = self.clock.sample().retrace;
        let acceptance = self.player_gate.attempt(
            sample.bios,
            self.machine,
            0,
            alley.player.horizontal_direction as i8,
            alley.player.vertical_direction as i8,
            retrace,
        );
        if acceptance.is_some() {
            let airborne = alley.player.vertical_direction != 0;
            let descending = alley.player.vertical_direction == 1;
            let entering = alley.player.entry_state != 0;
            let releasing = alley.player.support_state != 0 && self.controls.y != 0;
            let outcome = alley.update(self.controls)?;
            let cost = if releasing {
                self.costs.player_support_release
            } else if outcome.landed {
                self.costs.player_landing
            } else {
                match outcome.draw {
                    Draw::Keep => self.costs.player_keep,
                    Draw::Idle => self.costs.player_idle,
                    Draw::Erase => self.costs.player_airborne,
                    Draw::StridedSprite { .. } => self.costs.player_entry,
                    Draw::Sprite(_) if entering => self.costs.player_entry,
                    Draw::Sprite(_) if descending => self.costs.player_descending,
                    Draw::Sprite(_) if airborne => self.costs.player_airborne,
                    Draw::Sprite(_) => self.costs.player_walk,
                }
            };
            self.clock.advance_nanos(cost);
            if outcome.edge_reset {
                // The original calls 070d before 59cb: clear requests, accepted
                // tick and countdown now. 069f, divider and accepted AX survive.
                self.controls = Controls::default();
                self.player_gate.reset();
                self.clock.advance_nanos(self.costs.fence_setup);
                self.fence = Some(FenceEffect::new(self.clock.sample().bios.tick_low));
                return Ok(PassOutcome {
                    start_nanos,
                    end_nanos: self.clock.elapsed_nanos(),
                    tick_low: sample.bios.tick_low,
                    pit_counter: sample.pit_counter,
                    retrace,
                    control_polled,
                    acceptance,
                    actor_group_due: false,
                    fence_wait: false,
                });
            }
        } else if tests_retrace {
            self.clock.advance_nanos(self.costs.player_retrace_return);
        }
        let mut event = self.finish_pass(alley, start_nanos, control_polled, acceptance)?;
        event.tick_low = sample.bios.tick_low;
        event.pit_counter = sample.pit_counter;
        event.retrace = retrace;
        Ok(event)
    }

    fn finish_pass(
        &mut self,
        alley: &mut Alley,
        start_nanos: u128,
        control_polled: bool,
        acceptance: Option<PlayerAcceptance>,
    ) -> Result<PassOutcome, String> {
        self.clock.advance_nanos(self.costs.ground_dog);
        let actor_group_due = self.divider.advance(false);
        if actor_group_due {
            let sample = self.clock.sample();
            if let Some(scroll) = alley.advance_scenery(sample.bios.tick_low, sample.retrace)? {
                self.clock.advance_nanos(if scroll.generated_tile {
                    self.costs.clothes_new_tile
                } else {
                    self.costs.clothes_shift
                });
            }
            self.clock.advance_nanos(self.costs.actor_group);
        }
        self.clock.advance_nanos(self.costs.loop_return);
        let sample = self.clock.sample();
        Ok(PassOutcome {
            start_nanos,
            end_nanos: self.clock.elapsed_nanos(),
            tick_low: sample.bios.tick_low,
            pit_counter: sample.pit_counter,
            retrace: sample.retrace,
            control_polled,
            acceptance,
            actor_group_due,
            fence_wait: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::BIOS_TICK_PIT_CLOCKS;

    #[test]
    fn controls_are_latched_on_bios_ticks_even_between_player_accepts() {
        let mut alley = Alley::synthetic();
        let mut runtime = Runtime::new(Machine::Cga);
        let first = runtime
            .advance_pass(&mut alley, Controls { x: 1, y: 0 })
            .unwrap();
        assert!(first.control_polled);
        assert_eq!(runtime.controls, Controls { x: 1, y: 0 });
        let second = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: 0 })
            .unwrap();
        assert!(!second.control_polled);
        assert_eq!(runtime.controls.x, 1);
        runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS * 2);
        let later = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: 0 })
            .unwrap();
        assert!(later.control_polled);
        assert_eq!(runtime.controls.x, -1);
    }

    #[test]
    fn rejected_movement_still_advances_the_outer_loop_divider() {
        let mut alley = Alley::synthetic();
        let mut runtime = Runtime::new(Machine::Cga);
        runtime.player_gate = PlayerGate::new(0, 0);
        let mut due = Vec::new();
        for _ in 0..8 {
            let event = runtime
                .advance_pass(&mut alley, Controls::default())
                .unwrap();
            assert!(event.acceptance.is_none());
            assert!(event.end_nanos > event.start_nanos);
            due.push(event.actor_group_due);
        }
        assert_eq!(due, [false, false, false, true, false, false, false, true]);
    }

    #[test]
    fn fence_blocks_controls_and_actors_then_resumes_the_suspended_pass() {
        let mut alley = Alley::synthetic();
        let mut runtime = Runtime::new(Machine::Cga);
        runtime.fence = Some(FenceEffect::new(0));
        runtime.player_gate.accepted_ax = 32;
        let mut checks = 0;
        while runtime.fence.is_some() {
            let event = runtime
                .advance_pass(&mut alley, Controls { x: -1, y: -1 })
                .unwrap();
            assert!(event.fence_wait);
            assert!(!event.control_polled);
            assert!(event.acceptance.is_none());
            checks += 1;
            assert!(checks < 20_000, "Fence wait must finish");
            if runtime.fence.is_some() {
                assert_eq!(runtime.divider.counter, 0);
                assert_eq!(runtime.controls, Controls::default());
            }
        }
        assert_eq!(runtime.clock.sample().bios.tick_low, 7);
        assert_eq!(runtime.controls, Controls::default());
        assert_eq!(runtime.divider.counter, 1);
        assert_eq!(runtime.player_gate.accepted_ax, 32);
        assert_eq!(runtime.player_gate.countdown, 0);
    }

    #[test]
    fn fence_fall_clears_requests_before_starting_the_blocking_effect() {
        let mut alley = Alley::synthetic();
        alley.player.band_state = 2;
        alley.player.y = 108;
        alley.player.vertical_coordinate = 158;
        alley.player.vertical_direction = 1;
        let mut runtime = Runtime::new(Machine::Cga);
        let event = runtime
            .advance_pass(&mut alley, Controls { x: 1, y: 0 })
            .unwrap();
        assert!(event.acceptance.is_some());
        assert!(runtime.fence.is_some());
        assert_eq!(runtime.controls, Controls::default());
        assert_eq!(runtime.player_gate.last_tick, 0);
        assert_eq!(runtime.player_gate.countdown, 0);
        assert_eq!(runtime.player_gate.accepted_ax, 32);
        assert_eq!(runtime.divider.counter, 0);
    }
}
