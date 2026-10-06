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
/// Clotheslines and nonlethal actors have separate active-work costs.
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
    pub window_work: u64,
    pub room_actor_gate: u64,
    pub room_cage_draw: u64,
    pub room_bird_draw: u64,
    pub room_broom_draw: u64,
    pub room_effect_poll: u64,
    pub completion_draw_byte: u64,
    pub hearts_actor_draw: u64,
    pub cheese_mouse_draw: u64,
    pub cheese_transfer_draw: u64,
    pub bookcase_spider_draw: u64,
    pub bookcase_target_draw: u64,
    pub mouse_redraw: u64,
    pub mouse_catch: u64,
    pub bin_push_sound: u64,
    pub bin_frame: u64,
    pub bin_restore_row: u64,
    pub bin_draw_row: u64,
    pub bin_start: u64,
    pub loop_return: u64,
    pub fence_setup: u64,
    pub fence_bios: u64,
    pub fence_initial_wait: u64,
    pub fence_countdown: u64,
    pub fence_random: u64,
    pub fence_finish: u64,
    pub mouse_effect_poll: u64,
}

impl ReferenceCosts {
    pub fn dosbox_x_240(machine: Machine) -> Self {
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
            // Original window-row helpers measured about 0.5ms at fixed240.
            // New-window and contact/hold paths still use this bounded fit.
            window_work: 480_000,
            // Provisional room allowances; not independently CPU-calibrated.
            // Original tick gates, physics and blocking waits remain separate.
            room_actor_gate: 200_000,
            room_cage_draw: 2_000_000,
            room_bird_draw: 600_000,
            room_broom_draw: 2_500_000,
            room_effect_poll: 50_000,
            // Provisional drawing allowance for wipes and cat-face stamps.
            // Original presentation tick/retrace waits are modeled separately.
            completion_draw_byte: 4_000,
            // Provisional scene-seven drawing work above its tick gates.
            hearts_actor_draw: 1_500_000,
            // Provisional scene-four costs above its original tick gates.
            cheese_mouse_draw: 600_000,
            cheese_transfer_draw: 2_000_000,
            // Provisional scene-three work above the original tick gates.
            bookcase_spider_draw: 1_500_000,
            bookcase_target_draw: 1_500_000,
            // Full original helpers: normal draw about 1.6ms, catch about 8ms.
            // Add only work above the ordinary gated group allowance.
            mouse_redraw: 1_550_000,
            mouse_catch: 7_600_000,
            // Native CALL/RET helper measurements, including its three tones.
            bin_push_sound: if machine == Machine::Pcjr {
                40_900_000
            } else {
                81_500_000
            },
            // Bounded full-helper fit, less its ordinary gated allowance.
            // Row coefficients are a fit, not physical per-row CPU costs.
            bin_frame: 390_000,
            bin_restore_row: 26_000,
            bin_draw_row: 399_000,
            bin_start: 120_000,
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
            // BIOS tick read plus the short mouse-effect loop; approximate.
            mouse_effect_poll: 50_000,
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
    pub mouse_wait: bool,
    pub room_wait: bool,
    pub scene: u16,
    pub scene_changed: bool,
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
    mouse_pause: Option<u16>,
    cage_falling: bool,
    scene: u16,
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
        assert!(
            costs.mouse_effect_poll > 0,
            "Mouse-effect polling must advance virtual time"
        );
        assert!(
            costs.room_effect_poll > 0,
            "Room-effect polling must advance virtual time"
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
            mouse_pause: None,
            cage_falling: false,
            scene: 0,
        }
    }

    /// Execute a loop attempt, including attempts which reject player movement.
    /// Host input changes held keys; only the original input gate latches them.
    pub fn advance_pass(
        &mut self,
        alley: &mut Alley,
        held: Controls,
    ) -> Result<PassOutcome, String> {
        self.advance_pass_with_action(alley, held, false)
    }

    /// Alt places a carried gift. Like directions, action is latched only by
    /// the original two-tick input gate; it does not interrupt blocking work.
    pub fn advance_pass_with_action(
        &mut self,
        alley: &mut Alley,
        held: Controls,
        action: bool,
    ) -> Result<PassOutcome, String> {
        let start_nanos = self.clock.elapsed_nanos();
        self.synchronize_scene(alley);
        if alley.completion_waiting() {
            self.clock.advance_nanos(self.costs.room_effect_poll);
            let sample = self.clock.sample();
            let work = alley.advance_completion(sample.bios.tick_low, sample.retrace)?;
            self.clock
                .advance_nanos(work.bytes_drawn as u64 * self.costs.completion_draw_byte);
            let changed = work.complete;
            if changed {
                self.synchronize_scene(alley);
            }
            let mut event = self.room_wait_outcome(alley, start_nanos);
            event.scene_changed = changed;
            return Ok(event);
        }
        if let Some(start_tick) = self.mouse_pause {
            let mut scene_changed = false;
            self.clock.advance_nanos(self.costs.mouse_effect_poll);
            let sample = self.clock.sample();
            alley.mouse_effect_random(sample.bios.tick_low);
            if sample.bios.tick_low.wrapping_sub(start_tick) >= 8 {
                alley.finish_mouse_effect()?;
                self.mouse_pause = None;
                // The suspended mouse CALL has returned. Resume after the
                // actor batch, without repeating its divider or actor calls.
                self.clock.advance_nanos(self.costs.loop_return);
                if alley.finish_scene(sample.bios.tick_low)? {
                    self.synchronize_scene(alley);
                    scene_changed = true;
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
                fence_wait: false,
                mouse_wait: true,
                room_wait: false,
                scene: alley.scene(),
                scene_changed,
            });
        }
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
                mouse_wait: false,
                room_wait: false,
                scene: alley.scene(),
                scene_changed: false,
            });
        }

        let resumed_fall = if self.cage_falling {
            self.clock.advance_nanos(self.costs.room_effect_poll);
            let sample = self.clock.sample();
            let fall = alley.advance_room_fall(sample.bios.tick_low)?;
            if fall.drew {
                self.clock.advance_nanos(self.costs.room_cage_draw);
            }
            if !fall.released {
                return Ok(self.room_wait_outcome(alley, start_nanos));
            }
            self.cage_falling = false;
            true
        } else {
            false
        };

        if !resumed_fall {
            self.clock.advance_nanos(self.costs.prelude);
            self.clock.advance_nanos(self.costs.control_bios);
        }
        let control_polled = !resumed_fall
            && self
                .controls_gate
                .should_poll(self.clock.sample().bios.tick_low);
        if control_polled {
            self.controls = held;
            alley.latch_action(action);
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
        } else if !resumed_fall {
            self.clock.advance_nanos(self.costs.control_hold);
        }

        if alley.scene() == 5 {
            if !resumed_fall {
                alley.advance_room_tone(self.clock.sample().bios.tick_low);
                let cage = alley.advance_room_cage(self.clock.sample().bios.tick_low)?;
                self.clock.advance_nanos(self.costs.room_actor_gate);
                if cage.drew {
                    self.clock.advance_nanos(self.costs.room_cage_draw);
                }
                if cage.falling {
                    self.cage_falling = true;
                    let mut event = self.room_wait_outcome(alley, start_nanos);
                    event.control_polled = control_polled;
                    return Ok(event);
                }
            }
            let bird = alley.advance_room_bird(self.clock.sample().bios.tick_low)?;
            self.clock.advance_nanos(self.costs.room_actor_gate);
            if bird.drew {
                self.clock.advance_nanos(self.costs.room_bird_draw);
            }
        }
        if matches!(alley.scene(), 3 | 4 | 7) {
            // These scenes expire the shared room tone before the player call.
            alley.advance_room_tone(self.clock.sample().bios.tick_low);
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
            alley.scene(),
            alley.player.horizontal_direction as i8,
            alley.player.vertical_direction as i8,
            retrace,
        );
        if acceptance.is_some() {
            let airborne = alley.player.vertical_direction != 0;
            let descending = alley.player.vertical_direction == 1;
            let entering = alley.player.entry_state != 0;
            let releasing = alley.player.support_state != 0 && self.controls.y != 0;
            let bin_sound_ready = alley.bin_contact_sound_ready();
            let outcome = alley.update(self.controls)?;
            if let Some(controls) = outcome.forced_controls {
                self.controls = controls;
            }
            let cost = if releasing || outcome.forced_controls.is_some() {
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
            if outcome.forced_controls.is_some() && bin_sound_ready {
                self.clock.advance_nanos(self.costs.bin_push_sound);
            }
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
                    mouse_wait: false,
                    room_wait: false,
                    scene: alley.scene(),
                    scene_changed: false,
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
        let before_scene = alley.scene();
        if matches!(before_scene, 3..=5 | 7) {
            if before_scene == 7 {
                for call in [
                    Alley::advance_hearts_arrow,
                    Alley::pickup_hearts_item,
                    Alley::place_hearts_item,
                    Alley::advance_hearts_cat,
                ] {
                    let step = call(alley, self.clock.sample().bios.tick_low)?;
                    self.clock.advance_nanos(self.costs.room_actor_gate);
                    if step.drew {
                        self.clock.advance_nanos(self.costs.hearts_actor_draw);
                    }
                }
            } else {
                if before_scene == 3 {
                    // Original scene-three loop: player, spider, plants, broom.
                    let spider =
                        alley.advance_bookcase_spider(self.clock.sample().bios.tick_low)?;
                    self.clock.advance_nanos(self.costs.room_actor_gate);
                    if spider.drew {
                        self.clock.advance_nanos(self.costs.bookcase_spider_draw);
                    }
                    let targets =
                        alley.advance_bookcase_targets(self.clock.sample().bios.tick_low)?;
                    self.clock.advance_nanos(self.costs.room_actor_gate);
                    if targets.drew {
                        self.clock.advance_nanos(self.costs.bookcase_target_draw);
                    }
                }
                if before_scene == 4 {
                    // Original scene-four loop: player, transfer, mice, broom,
                    // then termination. Transfers do not block room scheduling.
                    let transfer =
                        alley.advance_cheese_transfer(self.clock.sample().bios.tick_low)?;
                    self.clock.advance_nanos(self.costs.room_actor_gate);
                    if transfer.drew {
                        self.clock.advance_nanos(self.costs.cheese_transfer_draw);
                    }
                    let mice = alley.advance_cheese_mice(self.clock.sample().bios.tick_low)?;
                    self.clock.advance_nanos(self.costs.room_actor_gate);
                    if mice.drew {
                        self.clock.advance_nanos(self.costs.cheese_mouse_draw);
                    }
                }
                if let Some(broom) = alley.advance_room_broom(self.clock.sample().bios.tick_low)? {
                    if broom.drawn {
                        self.clock.advance_nanos(self.costs.room_broom_draw);
                    }
                }
            }
            self.clock.advance_nanos(self.costs.room_actor_gate);
            let sample = self.clock.sample();
            let scene_changed = alley.finish_scene(sample.bios.tick_low)?;
            if scene_changed {
                self.synchronize_scene(alley);
            }
            return Ok(PassOutcome {
                start_nanos,
                end_nanos: self.clock.elapsed_nanos(),
                tick_low: sample.bios.tick_low,
                pit_counter: sample.pit_counter,
                retrace: sample.retrace,
                control_polled,
                acceptance,
                actor_group_due: false,
                fence_wait: false,
                mouse_wait: false,
                room_wait: false,
                scene: alley.scene(),
                scene_changed,
            });
        }
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
            let sample = self.clock.sample();
            let window = alley.advance_windows(sample.bios.tick_low, sample.retrace)?;
            if window.accepted {
                self.clock.advance_nanos(self.costs.window_work);
            }
            let sample = self.clock.sample();
            if let Some(bin) = alley.advance_bin_cat(sample.bios.tick_low, sample.retrace)? {
                self.clock.advance_nanos(
                    self.costs.bin_frame
                        + u64::from(bin.restore_rows) * self.costs.bin_restore_row
                        + u64::from(bin.draw_rows) * self.costs.bin_draw_row
                        + if bin.started { self.costs.bin_start } else { 0 },
                );
            }
            let sample = self.clock.sample();
            let mouse = alley.advance_mice(sample.bios.tick_low)?;
            self.clock.advance_nanos(if mouse.caught {
                self.costs.mouse_catch
            } else if mouse.redraw {
                self.costs.mouse_redraw
            } else {
                0
            });
            self.clock.advance_nanos(self.costs.actor_group);
            if mouse.paused {
                let tick = self.clock.sample().bios.tick_low;
                self.mouse_pause = Some(tick);
                alley.start_mouse_effect_wait(tick);
            }
        }
        if self.mouse_pause.is_none() {
            self.clock.advance_nanos(self.costs.loop_return);
        }
        let sample = self.clock.sample();
        let scene_changed = if self.mouse_pause.is_none() {
            alley.finish_scene(sample.bios.tick_low)?
        } else {
            false
        };
        if scene_changed {
            self.synchronize_scene(alley);
        }
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
            mouse_wait: false,
            room_wait: false,
            scene: alley.scene(),
            scene_changed,
        })
    }

    fn synchronize_scene(&mut self, alley: &Alley) {
        if self.scene != alley.scene() {
            self.scene = alley.scene();
            self.player_gate.reset();
            self.controls = Controls::default();
            self.fence = None;
            self.mouse_pause = None;
            self.cage_falling = false;
        }
    }

    fn room_wait_outcome(&self, alley: &Alley, start_nanos: u128) -> PassOutcome {
        let sample = self.clock.sample();
        PassOutcome {
            start_nanos,
            end_nanos: self.clock.elapsed_nanos(),
            tick_low: sample.bios.tick_low,
            pit_counter: sample.pit_counter,
            retrace: sample.retrace,
            control_polled: false,
            acceptance: None,
            actor_group_due: false,
            fence_wait: false,
            mouse_wait: false,
            room_wait: true,
            scene: alley.scene(),
            scene_changed: false,
        }
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
    fn mouse_effect_blocks_input_and_resumes_after_the_actor_batch_at_tick_wrap() {
        let mut alley = Alley::synthetic();
        let mut runtime = Runtime::new(Machine::Cga);
        runtime
            .clock
            .advance_pit_ticks(BIOS_TICK_PIT_CLOCKS * u64::from(u16::MAX - 3));
        runtime.mouse_pause = Some(u16::MAX - 3);
        runtime.divider = OuterLoopDivider::new(4);
        runtime.controls = Controls { x: 1, y: 0 };
        runtime.player_gate.accepted_ax = 32;
        let position = (alley.player.x, alley.player.y);
        let first = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: -1 })
            .unwrap();
        assert!(first.mouse_wait);
        assert!(!first.control_polled && !first.actor_group_due);
        assert!(first.acceptance.is_none());
        assert!(runtime.mouse_pause.is_some());
        runtime.clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS * 8);
        let last = runtime
            .advance_pass(&mut alley, Controls { x: -1, y: -1 })
            .unwrap();
        assert!(last.mouse_wait);
        assert!(!last.control_polled && !last.actor_group_due);
        assert!(last.acceptance.is_none());
        assert!(runtime.mouse_pause.is_none());
        assert_eq!(runtime.clock.sample().bios.tick_low, 4);
        assert_eq!(runtime.controls, Controls { x: 1, y: 0 });
        assert_eq!(runtime.divider.counter, 4);
        assert_eq!(runtime.player_gate.accepted_ax, 32);
        assert_eq!((alley.player.x, alley.player.y), position);
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
