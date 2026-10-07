// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Guest clock samples and the verified player/ordinary-alley timing gates.
//!
//! These mechanisms contain no host clock, rendering, or CPU-cost calibration.
//! Callers advance virtual time by the costs of their selected execution profile.
//! The default clock assumes an already-running channel 0 in binary mode 3 with
//! divisor 65536, immediate BIOS tick delivery, and an ordinary non-midnight BIOS
//! result (AX=0). Interrupt latency and the BIOS 24-hour reset are not modeled.
//! A supplied [`BiosSample`] can preserve other observed INT 1Ah results.

use crate::alley::Machine;
use std::time::Duration;

/// DOSBox-X's IBM PIT input frequency, rather than an inferred CPU frequency.
/// See upstream `include/timer.h` (`PIT_TICK_RATE_IBM`).
pub const PIT_HZ: u64 = 1_193_182;
pub const BIOS_TICK_PIT_CLOCKS: u64 = 65_536;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BiosSample {
    /// DX from INT 1Ah/AH=00h. Low-word subtraction wraps as in the original.
    pub tick_low: u16,
    /// AX returned by that BIOS call, retained on the unchanged-tick path.
    pub ax: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSample {
    pub bios: BiosSample,
    /// Channel-0 count latched by original helper 13b7, not its output bit.
    pub pit_counter: u16,
    /// Port 03DAh bit 3 sampled by original helper 13d8.
    pub retrace: bool,
}

/// A display's bit-3 pulse, measured in virtual PIT input clocks.
///
/// Both endpoints are inclusive, matching DOSBox-X's port-status comparison;
/// a start greater than end describes a pulse across the frame boundary.
/// Phase is independent of host presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetraceProfile {
    period_pit_ticks: u64,
    start_pit_ticks: u64,
    end_pit_ticks: u64,
    phase_pit_ticks: u64,
}

impl RetraceProfile {
    pub fn new(
        period_pit_ticks: u64,
        start_pit_ticks: u64,
        end_pit_ticks: u64,
        phase_pit_ticks: u64,
    ) -> Result<Self, &'static str> {
        if period_pit_ticks == 0 {
            return Err("Retrace period must be nonzero");
        }
        if start_pit_ticks >= period_pit_ticks || end_pit_ticks > period_pit_ticks {
            return Err("Retrace boundaries must fit within the period");
        }
        if start_pit_ticks == end_pit_ticks {
            return Err("Retrace pulse must have nonzero width");
        }
        Ok(Self {
            period_pit_ticks,
            start_pit_ticks,
            end_pit_ticks,
            phase_pit_ticks: phase_pit_ticks % period_pit_ticks,
        })
    }

    /// Both sampled original startup paths select BIOS video mode 4 (CS:0032).
    /// The selected DOSBox-X reference gives this mode the same timing on both
    /// machines; countdowns and palette handling still differ.
    pub fn for_machine(_machine: Machine) -> Self {
        Self::default()
    }

    pub fn with_phase(mut self, phase_pit_ticks: u64) -> Self {
        self.phase_pit_ticks = phase_pit_ticks % self.period_pit_ticks;
        self
    }

    pub fn period_pit_ticks(self) -> u64 {
        self.period_pit_ticks
    }

    pub fn is_set(self, elapsed_pit_ticks: u128) -> bool {
        self.is_set_scaled(
            (elapsed_pit_ticks % u128::from(self.period_pit_ticks)) * NANOS_PER_SECOND,
        )
    }

    fn is_set_scaled(self, elapsed_scaled: u128) -> bool {
        let period = u128::from(self.period_pit_ticks) * NANOS_PER_SECOND;
        let phase = (elapsed_scaled % period + u128::from(self.phase_pit_ticks) * NANOS_PER_SECOND)
            % period;
        let start = u128::from(self.start_pit_ticks) * NANOS_PER_SECOND;
        let end = u128::from(self.end_pit_ticks) * NANOS_PER_SECOND;
        if start < end {
            phase >= start && phase <= end
        } else {
            phase >= start || phase <= end
        }
    }
}

impl Default for RetraceProfile {
    fn default() -> Self {
        // Mode-4 timing facts: 57 character clocks per line, character clock
        // 3/4 of PIT_HZ, 262 lines, and a fixed 16-line pulse from line 224.
        // Consequently each line takes exactly 76 PIT input clocks.
        // Primary sources: DOSBox-X int10_modes.cpp (ModeList_OTHER and
        // INT10_SetVideoMode_OTHER), vga_draw.cpp (VGA_SetupDrawing), and
        // vga_misc.cpp (vga_read_p3da). The latter includes both pulse endpoints.
        // This is an authored pulse model, not copied implementation code.
        // https://github.com/joncampbell123/dosbox-x/blob/master/src/ints/int10_modes.cpp
        Self {
            period_pit_ticks: 76 * 262,
            start_pit_ticks: 76 * 224,
            end_pit_ticks: 76 * 240,
            phase_pit_ticks: 0,
        }
    }
}

/// A monotonic virtual clock with integer rational accumulation.
///
/// Duration advances retain their fractional PIT clocks, so subdividing an
/// elapsed duration does not lose time. Direct PIT-clock advances are exact.
#[derive(Clone, Debug)]
pub struct VirtualClock {
    /// Units are 1/1e9 of a PIT input clock.
    elapsed_scaled: u128,
    initial_tick: u16,
    retrace: RetraceProfile,
}

impl VirtualClock {
    pub fn new(retrace: RetraceProfile) -> Self {
        Self::with_initial_tick(retrace, 0)
    }

    pub fn with_initial_tick(retrace: RetraceProfile, initial_tick: u16) -> Self {
        Self {
            elapsed_scaled: 0,
            initial_tick,
            retrace,
        }
    }

    pub fn advance(&mut self, duration: Duration) {
        self.elapsed_scaled += duration.as_nanos() * u128::from(PIT_HZ);
    }

    pub fn advance_nanos(&mut self, nanos: u64) {
        self.advance(Duration::from_nanos(nanos));
    }

    pub fn advance_pit_ticks(&mut self, ticks: u64) {
        self.elapsed_scaled += u128::from(ticks) * NANOS_PER_SECOND;
    }

    /// Advance to the first BIOS edge strictly after the current instant.
    /// This preserves the fractional phase for deterministic blocking waits.
    pub fn advance_to_next_bios_tick(&mut self) {
        let period = u128::from(BIOS_TICK_PIT_CLOCKS) * NANOS_PER_SECOND;
        self.elapsed_scaled += period - self.elapsed_scaled % period;
    }

    pub fn elapsed_pit_ticks(&self) -> u128 {
        self.elapsed_scaled / NANOS_PER_SECOND
    }

    pub fn elapsed_nanos(&self) -> u128 {
        self.elapsed_scaled / u128::from(PIT_HZ)
    }

    pub fn bios_sample(&self) -> BiosSample {
        let ticks = self.elapsed_pit_ticks() / u128::from(BIOS_TICK_PIT_CLOCKS);
        BiosSample {
            tick_low: self.initial_tick.wrapping_add(ticks as u16),
            ax: 0,
        }
    }

    pub fn retrace(&self) -> bool {
        self.retrace.is_set_scaled(self.elapsed_scaled)
    }

    pub fn pit_counter(&self) -> u16 {
        // DOSBox-X mode 3 interpolates the down-count, truncates to a word,
        // then clears its low bit. For this even divisor that equals rounding
        // the fractional input-clock phase up before decrementing by two.
        // Thus this latch matches the emulator's interpolation rather than a
        // physical discrete-edge count. Each half period reloads 65536 (word
        // zero), while IRQ0/BIOS rises once per full period.
        // Source: DOSBox-X timer.cpp, PIT_Block::read_counter's mode-3 behavior.
        let half_scaled = self.elapsed_scaled % (32_768 * NANOS_PER_SECOND);
        let half_phase = half_scaled.div_ceil(NANOS_PER_SECOND) as u16;
        0_u16.wrapping_sub(half_phase.wrapping_mul(2))
    }

    pub fn sample(&self) -> ClockSample {
        ClockSample {
            bios: self.bios_sample(),
            pit_counter: self.pit_counter(),
            retrace: self.retrace(),
        }
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new(RetraceProfile::default())
    }
}

/// Original keyboard busy-wait predicate at CS:1226..122b: subtract the
/// starting channel-0 count from the new count, then unsigned-compare F8EDh.
/// A mode-3 counter decreases, so this does not represent an elapsed ascending
/// tick threshold. Poll spacing and half-cycle wrap affect when it succeeds.
pub fn keyboard_wait_complete(start_counter: u16, current_counter: u16) -> bool {
    current_counter.wrapping_sub(start_counter) >= 0xf8ed
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerAcceptance {
    pub tick_low: u16,
    pub ax: u16,
}

/// Original CS:08e5..0926 acceptance gate, before movement suppression/body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerGate {
    /// DS:057d, committed only after the retrace test passes.
    pub last_tick: u16,
    /// DS:0684, a call countdown rather than a time delay.
    pub countdown: u16,
    /// DS:057f, last accepted AX, including a same-tick BIOS return value.
    pub accepted_ax: u16,
}

impl PlayerGate {
    pub fn new(last_tick: u16, countdown: u16) -> Self {
        Self {
            last_tick,
            countdown,
            accepted_ax: 0,
        }
    }

    /// Reset words cleared by alley edge initialization 070d. DS:057f survives.
    pub fn reset(&mut self) {
        self.last_tick = 0;
        self.countdown = 0;
    }

    /// Directions are the retained player state before this call, not newly
    /// polled controls. `retrace` may be sampled later than `bios` if the caller
    /// accounts for the helper's execution cost. Commit occurs even if a later
    /// scene-specific cheese/drinking predicate suppresses movement.
    pub fn attempt(
        &mut self,
        bios: BiosSample,
        machine: Machine,
        scene: u16,
        prior_horizontal: i8,
        prior_vertical: i8,
        retrace: bool,
    ) -> Option<PlayerAcceptance> {
        let ax = if bios.tick_low != self.last_tick {
            let reload = match machine {
                Machine::Cga => 32,
                Machine::Pcjr => 16,
            };
            self.countdown = reload;
            reload
        } else {
            if self.countdown == 0 {
                return None;
            }
            self.countdown -= 1;
            if self.countdown != 0 {
                return None;
            }
            bios.ax
        };
        let tests_retrace = scene == 2 || (prior_horizontal == 0 && prior_vertical == 0);
        if tests_retrace && !retrace {
            return None;
        }
        self.last_tick = bios.tick_low;
        self.accepted_ax = ax;
        Some(PlayerAcceptance {
            tick_low: bios.tick_low,
            ax,
        })
    }
}

/// Original CS:1200 control poll threshold, baseline DS:069f.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlsGate {
    pub last_tick: u16,
}

impl ControlsGate {
    pub fn new(last_tick: u16) -> Self {
        Self { last_tick }
    }

    pub fn should_poll(&mut self, tick_low: u16) -> bool {
        if tick_low.wrapping_sub(self.last_tick) < 2 {
            return false;
        }
        self.last_tick = tick_low;
        true
    }
}

/// Ordinary alley CS:0155: controls, player and dog run before this decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OuterLoopDivider {
    /// DS:040f, including byte wrap; unaffected by rejected player attempts.
    pub counter: u8,
}

impl OuterLoopDivider {
    pub fn new(counter: u8) -> Self {
        Self { counter }
    }

    /// True schedules 546d, 04a0, 1936, 184b, 2216, 237b, then 26b3.
    /// Dog death (DS:1cb8) bypasses the increment as well as the divider test.
    pub fn advance(&mut self, dog_death: bool) -> bool {
        if dog_death {
            return true;
        }
        self.counter = self.counter.wrapping_add(1);
        self.counter & 3 == 0
    }
}

/// Seven-low-tick blocking interval at CS:59cb. The caller must separately run
/// its execution-dependent sound/random work; completion alone does not do so.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FenceGate {
    pub start_tick: u16,
}

impl FenceGate {
    pub fn new(start_tick: u16) -> Self {
        Self { start_tick }
    }

    pub fn complete(self, tick_low: u16) -> bool {
        tick_low.wrapping_sub(self.start_tick) >= 7
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceAction {
    /// Fewer than two BIOS ticks elapsed; the saved CX count is untouched.
    WaitingForStart,
    /// The eligible LOOP instruction decremented CX but did not reach zero.
    Counting,
    /// Invoke shared random helper 2dfd, including when speaker output is muted.
    Random,
    /// Seven ticks elapsed; perform the final speaker cleanup and return.
    Complete,
}

/// State for the original blocking fence effect CS:59cb..5a1b.
///
/// Each check represents the BIOS-time test at 59eb..59fd and, when eligible,
/// the LOOP at 59ff. The caller supplies execution costs between checks and
/// invokes the shared PRNG exactly on [`FenceAction::Random`]. Speaker output
/// and final hardware cleanup remain the caller's responsibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FenceEffect {
    /// DS:5a40, sampled before entering the effect.
    pub baseline: u16,
    /// DS:5a42, cleared on entry and incremented by seven after each PRNG call.
    pub accumulator: u16,
    /// CX, preserved across INT 1Ah and decremented only after the tick tests.
    pub remaining: u16,
}

impl FenceEffect {
    pub fn new(baseline: u16) -> Self {
        // 59d9 clears the accumulator even if a prior invocation left a value.
        Self {
            baseline,
            accumulator: 0,
            remaining: 1,
        }
    }

    pub fn check(&mut self, tick_low: u16) -> FenceAction {
        let elapsed = tick_low.wrapping_sub(self.baseline);
        if elapsed < 2 {
            return FenceAction::WaitingForStart;
        }
        if elapsed >= 7 {
            return FenceAction::Complete;
        }
        self.remaining = self.remaining.wrapping_sub(1);
        if self.remaining != 0 {
            return FenceAction::Counting;
        }
        // 5a01 calls the PRNG before this 5a11 increment and period reload.
        // Keeping the state transition atomic lets a pure caller carry out the
        // requested random call without deriving its cadence from audio state.
        self.accumulator = self.accumulator.wrapping_add(7);
        self.remaining = (self.accumulator >> 6).max(1);
        FenceAction::Random
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bios(tick_low: u16, ax: u16) -> BiosSample {
        BiosSample { tick_low, ax }
    }

    #[test]
    fn rational_clock_preserves_fractional_progress_across_small_advances() {
        let mut split = VirtualClock::default();
        for _ in 0..1_000_000 {
            split.advance_nanos(1_000);
        }
        let mut whole = VirtualClock::default();
        whole.advance(Duration::from_secs(1));
        assert_eq!(split.elapsed_pit_ticks(), u128::from(PIT_HZ));
        assert_eq!(split.elapsed_scaled, whole.elapsed_scaled);
        assert_eq!(split.sample(), whole.sample());
        assert_eq!(split.bios_sample().tick_low, 18);
    }

    #[test]
    fn bios_boundary_and_low_word_wrap_are_exact_in_pit_clocks() {
        let mut clock = VirtualClock::with_initial_tick(RetraceProfile::default(), u16::MAX);
        clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS - 1);
        assert_eq!(clock.bios_sample(), bios(u16::MAX, 0));
        clock.advance_pit_ticks(1);
        assert_eq!(clock.bios_sample(), bios(0, 0));
        clock.advance_pit_ticks(2 * BIOS_TICK_PIT_CLOCKS);
        assert_eq!(clock.bios_sample(), bios(2, 0));
    }

    #[test]
    fn mode_three_latch_reloads_twice_per_bios_tick() {
        let mut clock = VirtualClock::default();
        assert_eq!(clock.pit_counter(), 0);
        clock.advance_pit_ticks(1);
        assert_eq!(clock.pit_counter(), 65_534);
        clock.advance_pit_ticks(32_766);
        assert_eq!(clock.pit_counter(), 2);
        clock.advance_pit_ticks(1);
        assert_eq!(clock.pit_counter(), 0);
        assert_eq!(clock.bios_sample().tick_low, 0);
        clock.advance_pit_ticks(1);
        assert_eq!(clock.pit_counter(), 65_534);
        clock.advance_pit_ticks(32_767);
        assert_eq!(clock.pit_counter(), 0);
        assert_eq!(clock.bios_sample().tick_low, 1);
    }

    #[test]
    fn emulator_interpolated_latch_and_retrace_keep_fractional_phase() {
        let mut clock = VirtualClock::default();
        clock.advance_nanos(1);
        assert_eq!(clock.elapsed_pit_ticks(), 0);
        assert_eq!(clock.pit_counter(), 65_534);
        clock.advance_pit_ticks(1);
        assert_eq!(clock.pit_counter(), 65_532);

        let mut clock = VirtualClock::default();
        clock.advance_pit_ticks(18_240);
        assert!(clock.retrace(), "DOSBox-X includes the exact ending edge");
        clock.advance_nanos(1);
        assert_eq!(clock.elapsed_pit_ticks(), 18_240);
        assert!(
            !clock.retrace(),
            "pulse ends without rounding its phase down"
        );
    }

    #[test]
    fn keyboard_wait_preserves_subtraction_order_and_overshoot_behavior() {
        assert!(!keyboard_wait_complete(1000, 1000));
        assert!(keyboard_wait_complete(1000, 998));
        assert!(keyboard_wait_complete(2, 0));
        assert!(keyboard_wait_complete(0, 65_534));
        assert!(!keyboard_wait_complete(65_534, 0));
        // A late read can skip the accepted wrapped range and must keep polling.
        assert!(!keyboard_wait_complete(4000, 2000));
        assert!(keyboard_wait_complete(4000, 2189));
        assert!(!keyboard_wait_complete(4000, 2188));
    }

    #[test]
    fn next_bios_edge_advancement_handles_fractional_and_exact_edges() {
        let mut clock = VirtualClock::default();
        clock.advance_nanos(1);
        clock.advance_to_next_bios_tick();
        assert_eq!(clock.elapsed_pit_ticks(), 65_536);
        assert_eq!(clock.bios_sample().tick_low, 1);
        clock.advance_to_next_bios_tick();
        assert_eq!(clock.elapsed_pit_ticks(), 131_072);
        assert_eq!(clock.bios_sample().tick_low, 2);
    }

    #[test]
    fn retrace_has_distinct_clear_set_phases_and_configurable_phase() {
        for machine in [Machine::Cga, Machine::Pcjr] {
            let profile = RetraceProfile::for_machine(machine);
            assert!(!profile.is_set(17_023));
            assert!(profile.is_set(17_024));
            assert!(profile.is_set(18_239));
            assert!(profile.is_set(18_240));
            assert!(!profile.is_set(18_241));
            assert!(!profile.is_set(19_912));
            assert!(profile.is_set(19_912 + 17_024));
            assert!(profile.with_phase(17_024).is_set(0));
        }
        let across_boundary = RetraceProfile::new(100, 90, 10, 0).unwrap();
        assert!(across_boundary.is_set(99));
        assert!(across_boundary.is_set(0));
        assert!(across_boundary.is_set(10));
        assert!(!across_boundary.is_set(11));
        assert!(!across_boundary.is_set(89));
        assert!(RetraceProfile::new(0, 0, 1, 0).is_err());
        assert!(RetraceProfile::new(100, 100, 10, 0).is_err());
        assert!(RetraceProfile::new(100, 20, 20, 0).is_err());
    }

    #[test]
    fn each_machine_has_one_changed_tick_and_one_expiry_acceptance() {
        for (machine, reload) in [(Machine::Cga, 32), (Machine::Pcjr, 16)] {
            let mut gate = PlayerGate::default();
            assert_eq!(
                gate.attempt(bios(1, 0x1234), machine, 0, 1, 0, false),
                Some(PlayerAcceptance {
                    tick_low: 1,
                    ax: reload
                })
            );
            for countdown in (1..reload).rev() {
                assert!(gate.attempt(bios(1, 0), machine, 0, 1, 0, false).is_none());
                assert_eq!(gate.countdown, countdown);
            }
            assert_eq!(
                gate.attempt(bios(1, 0x00a5), machine, 0, 1, 0, false),
                Some(PlayerAcceptance {
                    tick_low: 1,
                    ax: 0x00a5
                })
            );
            assert_eq!(gate.accepted_ax, 0x00a5);
            assert!(gate.attempt(bios(1, 0), machine, 0, 1, 0, true).is_none());
        }
    }

    #[test]
    fn same_tick_expiry_consumed_by_retrace_rejection() {
        let mut gate = PlayerGate::new(5, 1);
        gate.accepted_ax = 9;
        assert!(
            gate.attempt(bios(5, 0), Machine::Cga, 0, 0, 0, false)
                .is_none()
        );
        assert_eq!(
            (gate.last_tick, gate.countdown, gate.accepted_ax),
            (5, 0, 9)
        );
        assert!(
            gate.attempt(bios(5, 0), Machine::Cga, 0, 0, 0, true)
                .is_none()
        );
    }

    #[test]
    fn changed_tick_retrace_rejection_retries_without_committing() {
        let mut gate = PlayerGate::new(5, 1);
        assert!(
            gate.attempt(bios(6, 0), Machine::Pcjr, 0, 0, 0, false)
                .is_none()
        );
        assert_eq!((gate.last_tick, gate.countdown), (5, 16));
        assert!(
            gate.attempt(bios(6, 0), Machine::Pcjr, 0, 0, 0, false)
                .is_none()
        );
        assert_eq!((gate.last_tick, gate.countdown), (5, 16));
        assert_eq!(
            gate.attempt(bios(6, 0), Machine::Pcjr, 0, 0, 0, true),
            Some(PlayerAcceptance {
                tick_low: 6,
                ax: 16
            })
        );
    }

    #[test]
    fn prior_movement_bypasses_retrace_except_underwater() {
        for (scene, horizontal, vertical, accepted) in [
            (0, 0, 0, false),
            (0, -1, 0, true),
            (0, 0, -1, true),
            (2, 1, 1, false),
        ] {
            let mut gate = PlayerGate::default();
            assert_eq!(
                gate.attempt(bios(1, 0), Machine::Cga, scene, horizontal, vertical, false)
                    .is_some(),
                accepted
            );
        }
    }

    #[test]
    fn player_tick_change_handles_wrap_and_edge_reset_preserves_ax() {
        let mut gate = PlayerGate::new(u16::MAX, 0);
        assert!(
            gate.attempt(bios(0, 0), Machine::Cga, 0, 1, 0, false)
                .is_some()
        );
        assert_eq!(
            (gate.last_tick, gate.countdown, gate.accepted_ax),
            (0, 32, 32)
        );
        gate.reset();
        assert_eq!(
            (gate.last_tick, gate.countdown, gate.accepted_ax),
            (0, 0, 32)
        );
    }

    #[test]
    fn controls_poll_at_two_ticks_and_rebaseline_after_long_gaps_and_wrap() {
        let mut gate = ControlsGate::new(u16::MAX);
        assert!(!gate.should_poll(0));
        assert!(gate.should_poll(1));
        assert!(!gate.should_poll(2));
        assert!(gate.should_poll(3));
        assert!(gate.should_poll(100));
        assert_eq!(gate.last_tick, 100);
        assert!(!gate.should_poll(101));
    }

    #[test]
    fn divider_runs_on_rejected_player_passes_and_wraps_its_byte() {
        let mut divider = OuterLoopDivider::new(254);
        let mut player = PlayerGate::default();
        for expected in [false, true, false, false, false, true] {
            assert!(
                player
                    .attempt(bios(0, 0), Machine::Cga, 0, 1, 0, false)
                    .is_none()
            );
            assert_eq!(divider.advance(false), expected);
        }
        assert_eq!(divider.counter, 4);
    }

    #[test]
    fn dog_death_group_runs_each_pass_without_advancing_divider() {
        let mut divider = OuterLoopDivider::new(3);
        assert!(divider.advance(true));
        assert!(divider.advance(true));
        assert_eq!(divider.counter, 3);
        assert!(divider.advance(false));
        assert_eq!(divider.counter, 4);
    }

    #[test]
    fn fence_uses_elapsed_ticks_not_seven_tick_durations_from_start_phase() {
        let mut clock = VirtualClock::with_initial_tick(RetraceProfile::default(), 65_532);
        clock.advance_pit_ticks(BIOS_TICK_PIT_CLOCKS - 1);
        let fence = FenceGate::new(clock.bios_sample().tick_low);
        clock.advance_pit_ticks(6 * BIOS_TICK_PIT_CLOCKS);
        assert!(!fence.complete(clock.bios_sample().tick_low));
        clock.advance_pit_ticks(1);
        assert!(fence.complete(clock.bios_sample().tick_low));
        assert!(fence.complete(4));
    }

    #[test]
    fn fence_effect_entry_clears_accumulator_and_wait_keeps_initial_count() {
        let mut effect = FenceEffect::new(42);
        assert_eq!((effect.accumulator, effect.remaining), (0, 1));
        assert_eq!(effect.check(42), FenceAction::WaitingForStart);
        assert_eq!(effect.check(43), FenceAction::WaitingForStart);
        assert_eq!((effect.accumulator, effect.remaining), (0, 1));
        assert_eq!(effect.check(44), FenceAction::Random);
        assert_eq!((effect.accumulator, effect.remaining), (7, 1));
        // A new invocation does not retain the former accumulated period.
        effect = FenceEffect::new(44);
        assert_eq!((effect.accumulator, effect.remaining), (0, 1));
    }

    #[test]
    fn fence_effect_counts_calls_after_two_ticks_and_reloads_from_accumulator() {
        let mut effect = FenceEffect::new(42);
        for index in 1..=19 {
            assert_eq!(effect.check(44), FenceAction::Random);
            assert_eq!(effect.accumulator, index * 7);
        }
        assert_eq!((effect.accumulator, effect.remaining), (133, 2));
        assert_eq!(effect.check(44), FenceAction::Counting);
        assert_eq!((effect.accumulator, effect.remaining), (133, 1));
        assert_eq!(effect.check(44), FenceAction::Random);
        assert_eq!((effect.accumulator, effect.remaining), (140, 2));
    }

    #[test]
    fn fence_effect_seven_tick_exit_preempts_decrement_and_random() {
        let mut effect = FenceEffect::new(42);
        effect.accumulator = 126;
        let before = effect;
        assert_eq!(effect.check(49), FenceAction::Complete);
        assert_eq!(effect, before);
        assert_eq!(effect.check(50), FenceAction::Complete);
        assert_eq!(effect, before);
        assert_eq!(effect.check(48), FenceAction::Random);
    }

    #[test]
    fn fence_effect_period_accumulator_wraps_as_a_word() {
        let mut effect = FenceEffect::new(42);
        effect.accumulator = u16::MAX;
        assert_eq!(effect.check(44), FenceAction::Random);
        assert_eq!((effect.accumulator, effect.remaining), (6, 1));
        assert_eq!(effect.check(44), FenceAction::Random);
        assert_eq!((effect.accumulator, effect.remaining), (13, 1));
    }

    #[test]
    fn fence_effect_tick_boundaries_use_unsigned_low_word_wrap() {
        let mut effect = FenceEffect::new(65_534);
        assert_eq!(effect.check(65_535), FenceAction::WaitingForStart);
        assert_eq!(effect.check(0), FenceAction::Random);
        assert_eq!(effect.check(4), FenceAction::Random);
        let before = effect;
        assert_eq!(effect.check(5), FenceAction::Complete);
        assert_eq!(effect, before);
    }
}
