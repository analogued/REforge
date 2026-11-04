// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Original alley gates over a deterministic reference clock.

use crate::alley::{Alley, DamagePoll, Machine};
use crate::player::{Controls, Draw};
use crate::timing::{
    ClockSample, ControlsGate, FenceAction, FenceEffect, OuterLoopDivider, PlayerAcceptance,
    PlayerGate, VirtualClock, keyboard_wait_complete,
};

/// Bounded stock-XT costs for milk work; retrace waits remain phase-dependent.
#[derive(Clone, Copy, Debug)]
pub struct MilkCosts {
    pub drink_gate: u64,
    pub drink_dead: u64,
    pub drink_stop: u64,
    pub drink_search: u64,
    pub drink_align: u64,
    pub drink_prepare: u64,
    pub drink_begin: u64,
    pub drink_restore: u64,
    pub drink_draw: u64,
    pub drink_finish: u64,
    pub bowl_draw: u64,
    pub bowl_empty: u64,
    pub attention_gate: u64,
    pub attention_begin: u64,
    pub attention_dead: u64,
    pub attention_slot: u64,
    pub attention_active: u64,
    pub eye_prepare: u64,
    pub eye_draw: u64,
    pub overlap_finish: u64,
    pub pursuit_entry: u64,
}

impl MilkCosts {
    fn ibm_xt() -> Self {
        Self {
            drink_gate: 100_000,
            drink_dead: 10_060,
            drink_stop: 60_000,
            drink_search: 238_000,
            drink_align: 340_000,
            drink_prepare: 475_000,
            drink_begin: 56_000,
            drink_restore: 679_800,
            drink_draw: 1_325_500,
            drink_finish: 30_000,
            bowl_draw: 677_000,
            bowl_empty: 63_000,
            attention_gate: 95_200,
            attention_begin: 24_000,
            attention_dead: 16_500,
            attention_slot: 24_000,
            attention_active: 24_000,
            eye_prepare: 160_000,
            eye_draw: 113_880,
            overlap_finish: 90_000,
            pursuit_entry: 9_850_000,
        }
    }
}

/// Scoped Hearts call costs fitted to stock-XT captures. Unsampled paths
/// combine these allowances; they do not claim instruction-exact timing.
#[derive(Clone, Copy, Debug)]
pub struct HeartsCosts {
    pub gate: u64,
    pub cat_idle: u64,
    pub cat_move_extra: u64,
    pub cat_restore: u64,
    pub cat_hidden: u64,
    pub cat_respawn_extra: u64,
    pub cat_pursuit_extra: u64,
    pub cat_recoil_reduction: u64,
    pub cat_block_start: u64,
    pub cat_block_moved: u64,
    pub cat_contact: u64,
    pub goal_contact: u64,
    pub cat_contact_moved_extra: u64,
    pub cat_active_gift: u64,
    pub cat_consume_extra: u64,
    pub goal_consume_extra: u64,
    pub cat_consume_arrow_extra: u64,
    pub arrow_spawn: u64,
    pub arrow_move: u64,
    pub arrow_toggle_extra: u64,
    pub arrow_bottom: u64,
    pub arrow_edge: u64,
    pub arrow_contact_start: u64,
    pub arrow_contact_moved: u64,
    pub pickup_held: u64,
    pub pickup_slot: u64,
    pub pickup_active: u64,
    pub pickup_hit_extra: u64,
    pub pickup_latch_reduction: u64,
    pub arrow_overlay_extra: u64,
    pub place_empty: u64,
    pub place_action_blocked: u64,
    pub place_overlap: u64,
    pub place_hit: u64,
    pub place_consume_extra: u64,
}

impl HeartsCosts {
    fn ibm_xt() -> Self {
        Self {
            gate: 80_000,
            cat_idle: 1_917_000,
            cat_move_extra: 83_000,
            cat_restore: 500_000,
            cat_hidden: 341_000,
            cat_respawn_extra: 116_000,
            cat_pursuit_extra: 108_000,
            cat_recoil_reduction: 34_000,
            cat_block_start: 288_400,
            cat_block_moved: 671_000,
            cat_contact: 3_091_400,
            goal_contact: 1_310_700,
            cat_contact_moved_extra: 400_000,
            cat_active_gift: 67_000,
            cat_consume_extra: 4_347_000,
            goal_consume_extra: 4_277_000,
            cat_consume_arrow_extra: 2_685_300,
            arrow_spawn: 2_628_000,
            arrow_move: 3_058_000,
            arrow_toggle_extra: 3_210_000,
            arrow_bottom: 719_300,
            arrow_edge: 755_000,
            arrow_contact_start: 3_326_700,
            arrow_contact_moved: 963_800,
            pickup_held: 105_500,
            pickup_slot: 16_210,
            pickup_active: 116_100,
            pickup_hit_extra: 3_813_500,
            pickup_latch_reduction: 26_400,
            arrow_overlay_extra: 2_561_400,
            place_empty: 9_320,
            place_action_blocked: 20_850,
            place_overlap: 459_800,
            place_hit: 5_200_500,
            place_consume_extra: 2_068_600,
        }
    }

    fn nanos(self, work: crate::hearts::Work) -> u64 {
        use crate::hearts::Work;
        match work {
            Work::CatGate | Work::ArrowGate | Work::PickupGate => self.gate,
            Work::CatHidden => self.cat_hidden,
            Work::CatArrow { moved } => {
                if moved {
                    self.cat_block_moved
                } else {
                    self.cat_block_start
                }
            }
            Work::CatContact {
                goal,
                restored,
                moved,
            } => {
                (if goal {
                    self.goal_contact
                } else {
                    self.cat_contact
                }) + if restored { self.cat_restore } else { 0 }
                    + if moved {
                        self.cat_contact_moved_extra
                    } else {
                        0
                    }
            }
            Work::CatDraw {
                moving,
                recoil,
                pursued,
                respawned,
                restored,
                active_gifts,
                consumed,
                arrow,
                goal,
            } => {
                self.cat_idle
                    + if moving { self.cat_move_extra } else { 0 }
                    + if pursued { self.cat_pursuit_extra } else { 0 }
                    + if respawned { self.cat_respawn_extra } else { 0 }
                    + if restored { self.cat_restore } else { 0 }
                    + u64::from(active_gifts) * self.cat_active_gift
                    + if consumed {
                        (if goal {
                            self.goal_consume_extra
                        } else {
                            self.cat_consume_extra
                        }) + if arrow {
                            self.cat_consume_arrow_extra
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                    - if recoil { self.cat_recoil_reduction } else { 0 }
            }
            Work::ArrowContact { moved } => {
                if moved {
                    self.arrow_contact_moved
                } else {
                    self.arrow_contact_start
                }
            }
            Work::ArrowRetire { bottom } => {
                if bottom {
                    self.arrow_bottom
                } else {
                    self.arrow_edge
                }
            }
            Work::ArrowDraw { spawned, toggle } => {
                (if spawned {
                    self.arrow_spawn
                } else {
                    self.arrow_move
                }) + if toggle { self.arrow_toggle_extra } else { 0 }
            }
            Work::Pickup {
                held: _,
                slots,
                active,
                hit,
                latched,
                arrow,
            } => {
                self.pickup_held
                    + u64::from(slots) * self.pickup_slot
                    + u64::from(active) * self.pickup_active
                    + if hit {
                        self.pickup_hit_extra + if arrow { self.arrow_overlay_extra } else { 0 }
                    } else {
                        0
                    }
                    - if latched {
                        self.pickup_latch_reduction
                    } else {
                        0
                    }
            }
            Work::PlaceGate { holding } => {
                if holding {
                    self.place_action_blocked
                } else {
                    self.place_empty
                }
            }
            Work::Place {
                blocked,
                active,
                arrow,
                consumed,
            } => {
                (if blocked {
                    self.place_overlap
                } else {
                    self.place_hit
                }) + u64::from(active) * self.pickup_active
                    + if arrow { self.arrow_overlay_extra } else { 0 }
                    + if consumed {
                        self.place_consume_extra
                    } else {
                        0
                    }
            }
        }
    }
}

/// Scoped alley-window work on the XT baseline. Quiet countdown work is
/// already represented in the grouped-call allowance.
#[derive(Clone, Copy, Debug)]
pub struct WindowCosts {
    pub countdown: u64,
    pub retrace: u64,
    pub player_below: u64,
    pub projectile: u64,
    pub selection: u64,
    pub player_hold: u64,
    pub mouse_hold: u64,
    pub holding: u64,
    pub open_row: u64,
    pub copy_row: u64,
    pub new_window: u64,
    pub selection_retry: u64,
    pub band_check: u64,
    pub hold_started: u64,
    pub hold_expired: u64,
    pub launch: u64,
}

impl WindowCosts {
    fn ibm_xt() -> Self {
        Self {
            countdown: 8_600,
            retrace: 39_700,
            player_below: 72_800,
            projectile: 190_000,
            selection: 140_800,
            player_hold: 330_300,
            mouse_hold: 286_100,
            holding: 358_800,
            open_row: 409_400,
            copy_row: 435_000,
            new_window: 210_700,
            selection_retry: 203_450,
            band_check: 37_500,
            hold_started: 74_500,
            hold_expired: 85_350,
            launch: 189_500,
        }
    }

    fn nanos(self, work: crate::windows::Work) -> u64 {
        use crate::windows::Path;
        let base = match work.path {
            Path::Countdown => self.countdown,
            Path::Retrace => self.retrace,
            Path::PlayerBelow => self.player_below,
            Path::Projectile => self.projectile,
            Path::Selection => self.selection,
            Path::PlayerHold => self.player_hold,
            Path::MouseHold => self.mouse_hold,
            Path::Holding => self.holding,
            Path::OpenRow => self.open_row,
            Path::CopyRow => self.copy_row,
        };
        base + if work.selections != 0 && work.path != Path::Selection {
            self.new_window
        } else {
            0
        } + u64::from(work.selections.saturating_sub(1)) * self.selection_retry
            + u64::from(work.band_check) * self.band_check
            + u64::from(work.hold_started) * self.hold_started
            + u64::from(work.hold_expired) * self.hold_expired
            + u64::from(work.launched) * self.launch
    }
}

/// Accepted alley-entry bodies on the agreed XT baseline. The twelve-call
/// hidden countdown and player acceptance gates remain separate game rules.
#[derive(Clone, Copy, Debug)]
pub struct EntryCosts {
    pub waiting: u64,
    pub first_crop: u64,
    pub second_crop: u64,
    pub finish_idle: u64,
    pub finish_walk: u64,
    pub finish_jump: u64,
}

impl EntryCosts {
    fn ibm_xt() -> Self {
        Self {
            waiting: 121_500,
            first_crop: 1_711_000,
            second_crop: 2_669_000,
            finish_idle: 2_550_000,
            finish_walk: 2_726_000,
            finish_jump: 402_400,
        }
    }

    fn nanos(self, draw: Draw, waiting: bool) -> Option<u64> {
        match draw {
            Draw::Keep => Some(if waiting {
                self.waiting
            } else {
                self.finish_jump
            }),
            Draw::StridedSprite { sprite, .. } => match sprite.dimensions {
                0x0b01 => Some(self.first_crop),
                0x0b02 => Some(self.second_crop),
                _ => None,
            },
            Draw::Idle => Some(self.finish_idle),
            Draw::Sprite(_) => Some(self.finish_walk),
            Draw::Erase => None,
        }
    }
}

/// Completion graphics on the agreed XT baseline. Row and byte terms are
/// fitted to these fixed native drawing routines, not general CGA bus costs.
#[derive(Clone, Copy, Debug)]
pub struct CompletionCosts {
    pub wipe_setup: u64,
    pub wipe_row: u64,
    pub wipe_byte: u64,
    pub wipe_initial: u64,
    pub wipe_growth: u64,
    pub wipe_clip: u64,
    pub face_prepare: u64,
    pub score_row: u64,
    pub score_grid_extra: u64,
}

impl CompletionCosts {
    fn ibm_xt() -> Self {
        Self {
            // Shared cumulative drawing-work curve, checked across entries
            // and returns. Each step adds overhead + rows + packed bytes.
            wipe_setup: 149_501,
            wipe_row: 51_169,
            wipe_byte: 2_722,
            wipe_initial: 0,
            wipe_growth: 0,
            wipe_clip: 0,
            face_prepare: 373_267,
            score_row: 2_985_200,
            score_grid_extra: 678_200,
        }
    }

    fn nanos(self, graphics: crate::presentation::Graphics) -> u64 {
        let wipe = graphics.wipe.map_or(0, |work| {
            self.wipe_setup
                + u64::from(work.rows) * self.wipe_row
                + work.bytes as u64 * self.wipe_byte
                + if work.first { self.wipe_initial } else { 0 }
                + if work.growing { self.wipe_growth } else { 0 }
                + u64::from(work.clips) * self.wipe_clip
        });
        wipe + u64::from(graphics.face_preparations) * self.face_prepare
            + u64::from(graphics.score_rows) * self.score_row
            + if graphics.score_grid {
                self.score_grid_extra
            } else {
                0
            }
    }
}

/// Callable Hearts presentation helpers on the shared XT baseline. These
/// fitted values exclude deliberate BIOS/retrace waits and unmeasured glue.
/// Pairing frames retain state work but omit simulated sprite-drawing latency.
#[derive(Clone, Copy, Debug)]
pub struct HeartsPresentationCosts {
    pub sprites: [u64; 6],
    pub pair_item: u64,
    pub increasing_axis: u64,
    pub decreasing_axis: u64,
    pub upper_clamp: u64,
    pub lower_clamp: u64,
    pub dance_frame: u64,
    pub dance_copy: u64,
    pub approach_init: u64,
    pub approach_drawing: u64,
    pub approach_finished: u64,
    pub pair_init: u64,
    pub reward: u64,
    pub reward_life: u64,
    pub reward_difficulty: u64,
    pub dance_init: u64,
    pub dance_skipped: u64,
}

impl HeartsPresentationCosts {
    fn ibm_xt() -> Self {
        Self {
            sprites: [
                24_877_500, 931_500, 2_000_100, 7_347_500, 771_000, 1_472_100,
            ],
            pair_item: 33_400,
            increasing_axis: 21_300,
            decreasing_axis: 18_100,
            upper_clamp: 12_300,
            lower_clamp: 5_400,
            dance_frame: 33_800,
            dance_copy: 2_579_850,
            approach_init: 70_000,
            approach_drawing: 116_300,
            approach_finished: 38_100,
            pair_init: 567_600,
            reward: 106_200,
            reward_life: 4_000,
            reward_difficulty: 5_300,
            dance_init: 120_400,
            dance_skipped: 10_950,
        }
    }

    fn nanos(self, work: crate::hearts_completion::PresentationWork) -> u64 {
        // Pairing is paced by its native tick gates, not historical drawing
        // throughput. Keep sprite metadata intact so these bytes cannot receive
        // a fallback drawing charge. Other uses of the same sprites retain
        // their measured costs, including the large approach cat.
        let drawing = if work.pair_items == 0 {
            work.sprites
                .into_iter()
                .zip(self.sprites)
                .map(|(count, cost)| u64::from(count) * cost)
                .sum::<u64>()
        } else {
            0
        };
        drawing
            + u64::from(work.pair_items) * self.pair_item
            + u64::from(work.increasing_axes) * self.increasing_axis
            + u64::from(work.decreasing_axes) * self.decreasing_axis
            + u64::from(work.upper_clamps) * self.upper_clamp
            + u64::from(work.lower_clamps) * self.lower_clamp
            + u64::from(work.dance_frames) * self.dance_frame
            + u64::from(work.dance_copies) * self.dance_copy
            + work.bookkeeping.map_or(0, |stage| {
                use crate::hearts_completion::Bookkeeping;
                match stage {
                    Bookkeeping::ApproachInit => self.approach_init,
                    Bookkeeping::Approach { drawing: true } => self.approach_drawing,
                    Bookkeeping::Approach { drawing: false } => self.approach_finished,
                    Bookkeeping::PairInit => self.pair_init,
                    Bookkeeping::Reward { life, difficulty } => {
                        self.reward
                            + u64::from(life) * self.reward_life
                            + u64::from(difficulty) * self.reward_difficulty
                    }
                    Bookkeeping::DanceInit { active: true } => self.dance_init,
                    Bookkeeping::DanceInit { active: false } => self.dance_skipped,
                }
            })
    }
}

/// Recurring title helpers on the shared XT baseline. Static construction and
/// the enclosing title-loop/music/key bookkeeping are outside these fits.
#[derive(Clone, Copy, Debug)]
pub struct TitleCosts {
    pub left_edge: u64,
    pub right_edge: u64,
    pub recent_direction: u64,
    pub random_moving: u64,
    pub random_idle: u64,
    pub copyright: u64,
    pub player_call: u64,
    pub entry_wait: u64,
    pub entry_first_crop: u64,
    pub entry_second_crop: u64,
    pub entry_finish: u64,
    pub walk: u64,
}

impl TitleCosts {
    fn ibm_xt() -> Self {
        Self {
            left_edge: 41_100,
            right_edge: 51_600,
            recent_direction: 130_850,
            random_moving: 193_000,
            random_idle: 183_300,
            copyright: 1_449_500,
            // Provisional extra MOV/CALL work on the wrapper's retrace-set path.
            player_call: 16_000,
            // Whole native player calls, including their acceptance gate.
            entry_wait: 240_850,
            entry_first_crop: 1_836_300,
            entry_second_crop: 2_794_300,
            entry_finish: 3_031_700,
            walk: 2_953_100,
        }
    }

    fn nanos(self, work: crate::splash::TitleWork, fallback: &ReferenceCosts) -> u64 {
        use crate::splash::Steering;
        let steering = match work.steering {
            Some(Steering::LeftEdge) => self.left_edge,
            Some(Steering::RightEdge) => self.right_edge,
            Some(Steering::Recent) => self.recent_direction,
            Some(Steering::RandomMoving) => self.random_moving,
            Some(Steering::RandomIdle) => self.random_idle,
            None => 0,
        };
        let player = match work.draw {
            Some(Draw::Keep) if work.entering => self.entry_wait,
            Some(Draw::StridedSprite { sprite, .. }) if sprite.dimensions == 0x0b01 => {
                self.entry_first_crop
            }
            Some(Draw::StridedSprite { sprite, .. }) if sprite.dimensions == 0x0b02 => {
                self.entry_second_crop
            }
            Some(Draw::Sprite(_)) if work.entering => self.entry_finish,
            Some(Draw::Sprite(_)) => self.walk,
            Some(Draw::Idle) => fallback.player_gate + fallback.player_idle,
            Some(_) => fallback.player_gate + fallback.player_keep,
            None if work.player_called => {
                fallback.player_gate
                    + if work.player_countdown_zero {
                        fallback.player_zero
                    } else {
                        fallback.player_countdown
                    }
            }
            None => 0,
        };
        steering
            + player
            + if work.copyright { self.copyright } else { 0 }
            + if work.steering.is_some() && work.player_called {
                self.player_call
            } else {
                0
            }
    }
}

/// Individual completion polling passes, excluding their intended tick waits.
#[derive(Clone, Copy, Debug)]
pub struct CompletionPollCosts {
    pub melody_muted: u64,
    pub melody_sound: u64,
    pub melody_tone: u64,
    pub reveal: u64,
    pub dance_muted: u64,
    pub dance_sound: u64,
    pub dance_tone: u64,
}

impl CompletionPollCosts {
    fn ibm_xt() -> Self {
        Self {
            melody_muted: 108_100,
            melody_sound: 188_200,
            melody_tone: 83_300,
            reveal: 83_000,
            dance_muted: 105_600,
            dance_sound: 184_100,
            dance_tone: 65_800,
        }
    }

    fn poll(self, kind: crate::presentation::Poll, sound: bool) -> u64 {
        use crate::presentation::Poll;
        match (kind, sound) {
            (Poll::Melody, false) => self.melody_muted,
            (Poll::Melody, true) => self.melody_sound,
            (Poll::Reveal, _) => self.reveal,
            (Poll::Dance, false) => self.dance_muted,
            (Poll::Dance, true) => self.dance_sound,
        }
    }

    fn tone(self, work: crate::presentation::PollWork) -> u64 {
        use crate::presentation::Poll;
        if !work.tone {
            return 0;
        }
        match work.kind {
            Poll::Melody => self.melody_tone,
            Poll::Dance => self.dance_tone,
            Poll::Reveal => 0,
        }
    }
}

/// Failure presentation helpers on the shared XT baseline.
#[derive(Clone, Copy, Debug)]
pub struct FailureCosts {
    pub message: u64,
    pub dog: u64,
    pub header: u64,
    pub detail: u64,
    pub message_muted_poll: u64,
    pub message_sound_poll: u64,
    pub dog_muted_poll: u64,
    pub dog_sound_poll: u64,
    pub tone: u64,
    pub message_exit: u64,
    pub dog_exit: u64,
}

impl FailureCosts {
    fn ibm_xt() -> Self {
        Self {
            message: 2_843_340,
            dog: 4_831_570,
            header: 1_144_310,
            detail: 1_817_150,
            message_muted_poll: 102_580,
            message_sound_poll: 186_060,
            dog_muted_poll: 105_600,
            dog_sound_poll: 191_090,
            tone: 75_430,
            message_exit: 57_710,
            dog_exit: 61_180,
        }
    }

    fn poll(self, poll: crate::failure::Poll, sound: bool) -> u64 {
        match (poll, sound) {
            (crate::failure::Poll::Message, false) => self.message_muted_poll,
            (crate::failure::Poll::Message, true) => self.message_sound_poll,
            (crate::failure::Poll::Dog, false) => self.dog_muted_poll,
            (crate::failure::Poll::Dog, true) => self.dog_sound_poll,
        }
    }

    fn nanos(self, work: crate::failure::PresentationWork) -> u64 {
        u64::from(work.message) * self.message
            + u64::from(work.dog) * self.dog
            + u64::from(work.header) * self.header
            + u64::from(work.detail) * self.detail
            + if work.tone { self.tone } else { 0 }
            + if work.terminal {
                match work.poll {
                    Some(crate::failure::Poll::Message) => self.message_exit,
                    Some(crate::failure::Poll::Dog) => self.dog_exit,
                    None => 0,
                }
            } else {
                0
            }
    }
}

/// Scoped XT blocking-effect work; tick deadlines remain separate.
#[derive(Clone, Copy, Debug)]
pub struct BlockingCosts {
    pub spider_entry: u64,
    pub spider_exit: u64,
    pub eel_entry: u64,
    pub eel_scan_start: u64,
    pub eel_scan_slot: u64,
    pub eel_scan_miss: u64,
    pub eel_scan_collect: u64,
    pub eel_lethal_test: u64,
    pub eel_failure_draw: u64,
    pub effect_stop: u64,
    pub mouse_sound_before: u64,
    pub mouse_sound_after: u64,
    pub mouse_tone: u64,
    pub mouse_cleanup: u64,
    pub mouse_overlay: u64,
    pub cage_poll: u64,
    pub cage_draw: u64,
    pub cage_release: u64,
    pub cage_tone: u64,
}

impl BlockingCosts {
    fn ibm_xt() -> Self {
        Self {
            // Bounded entry/exit slices, excluding the blocking polls and
            // contact images. Both software modes use the same XT baseline.
            spider_entry: 106_200,
            spider_exit: 5_550,
            eel_entry: 121_940,
            // Full native prefixes to the first harmful eel, before its image.
            eel_scan_start: 127_000,
            eel_scan_slot: 37_500,
            eel_scan_miss: 88_100,
            // Collected fish above an inactive slot; image restoration is separate.
            eel_scan_collect: 215_000,
            // Lethal decision work above an active miss, then CS:3506..3538.
            eel_lethal_test: 38_600,
            eel_failure_draw: 1_525_600,
            effect_stop: 20_110,
            // Steady audible polls have two BIOS samples, about 475 and
            // 389 cycles apart. Muted polls use ReferenceCosts directly.
            mouse_sound_before: 99_520,
            mouse_sound_after: 81_510,
            mouse_tone: 75_430,
            mouse_cleanup: 742_030,
            mouse_overlay: 5_409_380,
            // Mean quiet cage poll is 460.8 cycles. Restore/draw increments
            // exclude that poll; release includes the larger broken image.
            cage_poll: 96_550,
            cage_draw: 3_210_000,
            cage_release: 3_893_270,
            cage_tone: 26_400,
        }
    }

    fn lethal_scan(self, contact: crate::fishbowl::ContactStep) -> Option<u64> {
        // Captures cover inactive slots, active misses and fish collections
        // before one lethal eel. Global protection prevents a harmless eel
        // from preceding a lethal one in the same native scan.
        (contact.failure
            && contact.contacts == contact.collected
            && contact.hazard_contacts == 1
            && contact.restored_eels == 0)
            .then(|| {
                self.eel_scan_start
                    + u64::from(contact.visited) * self.eel_scan_slot
                    + u64::from(contact.tested.saturating_sub(contact.collected + 1))
                        * self.eel_scan_miss
                    + u64::from(contact.collected) * self.eel_scan_collect
            })
    }

    fn cage_work(self, fall: crate::birdcage::FallStep, sound: bool) -> u64 {
        if !fall.drew {
            return 0;
        }
        (if fall.released {
            self.cage_release
        } else {
            self.cage_draw
        }) + if sound { self.cage_tone } else { 0 }
    }
}

/// Post-scenery setup suffixes on the shared XT baseline. These whole-body
/// allowances already include player entry, actor resets and any HUD work.
/// They exclude scenery construction and deliberate transition waits.
#[derive(Clone, Copy, Debug)]
pub struct SetupCosts {
    pub ground_alley: u64,
    pub saved_alley: u64,
    pub rooms: [u64; 7],
}

impl SetupCosts {
    fn ibm_xt() -> Self {
        Self {
            ground_alley: 8_266_600,
            saved_alley: 8_353_900,
            rooms: [
                1_590_000, 5_330_200, 1_667_100, 2_924_000, 3_764_000, 1_600_000, 3_149_500,
            ],
        }
    }

    fn nanos(self, scene: u16, saved_alley: bool) -> u64 {
        match scene {
            0 if saved_alley => self.saved_alley,
            0 => self.ground_alley,
            1..=7 => self.rooms[usize::from(scene - 1)],
            _ => 0,
        }
    }
}

/// Bounded stock-XT fits for transition arithmetic, excluding drawing/waits.
/// Decimal conversion depends on set bits; Hearts also repeats decimal adds.
#[derive(Clone, Copy, Debug)]
pub struct BookkeepingCosts {
    pub progress: u64,
    pub ordinary: u64,
    pub hearts: u64,
    pub bonus_bit: u64,
    pub multiplier: u64,
    pub life_nonzero: u64,
    pub life_zero: u64,
    pub selection: u64,
    pub selection_attempt: u64,
    pub selection_draw: u64,
    pub selection_rejection: u64,
}

impl BookkeepingCosts {
    fn ibm_xt() -> Self {
        Self {
            progress: 13_700,
            ordinary: 837_000,
            hearts: 610_000,
            bonus_bit: 224_000,
            multiplier: 238_000,
            life_nonzero: 38_700,
            life_zero: 34_400,
            selection: 132_000,
            selection_attempt: 91_500,
            selection_draw: 35_300,
            selection_rejection: 2_200,
        }
    }

    fn nanos(self, work: crate::presentation::Bookkeeping) -> u64 {
        u64::from(work.progress) * self.progress
            + work.award.map_or(0, |award| {
                u64::from(award.bonus.count_ones()) * self.bonus_bit
                    + award.multiplier.map_or(self.ordinary, |count| {
                        self.hearts + u64::from(count) * self.multiplier
                    })
            })
            + work.life.map_or(0, |had_life| {
                if had_life {
                    self.life_nonzero
                } else {
                    self.life_zero
                }
            })
    }

    fn selection_nanos(self, work: crate::windows::SelectionWork) -> u64 {
        self.selection
            + u64::from(work.attempts) * self.selection_attempt
            + u64::from(work.extra_draws) * self.selection_draw
            + u64::from(work.history_rejections) * self.selection_rejection
    }
}

/// Whole bird scheduler calls on the shared XT baseline. The active allowance
/// includes normal first drawing; restore and decision increments are fitted
/// from bounded calls, not independent instruction timings.
#[derive(Clone, Copy, Debug)]
pub struct BirdCosts {
    pub same_tick: u64,
    pub not_released: u64,
    pub broom: u64,
    pub active: u64,
    pub random_draw: u64,
    pub separation: u64,
    pub waypoint: u64,
    pub released_waypoint: u64,
    pub turned: u64,
    pub tone: u64,
    pub axis: u64,
    pub restore: u64,
}

impl BirdCosts {
    fn ibm_xt() -> Self {
        Self {
            same_tick: 79_000,
            not_released: 111_000,
            broom: 279_000,
            active: 1_332_000,
            random_draw: 30_000,
            separation: 114_200,
            waypoint: 43_800,
            released_waypoint: 45_500,
            turned: 23_000,
            tone: 166_100,
            axis: 8_000,
            restore: 277_500,
        }
    }

    fn nanos(self, work: crate::birdcage::BirdWork) -> u64 {
        use crate::birdcage::BirdGate;
        match work.gate {
            BirdGate::SameTick => self.same_tick,
            BirdGate::NotReleased => self.not_released,
            BirdGate::Broom => self.broom,
            BirdGate::Motion => {
                self.active
                    + u64::from(work.random_draws) * self.random_draw
                    + u64::from(work.separation) * self.separation
                    + u64::from(work.waypoint) * self.waypoint
                    + u64::from(work.released_waypoint) * self.released_waypoint
                    + u64::from(work.turned) * self.turned
                    + u64::from(work.tone) * self.tone
                    + u64::from(work.axes) * self.axis
                    + u64::from(work.restored) * self.restore
            }
        }
    }
}

/// Quiet room-helper returns on the shared XT baseline. Active paths retain
/// their existing gate and drawing fit; these complete quiet costs replace it.
#[derive(Clone, Copy, Debug)]
pub struct QuietCosts {
    pub cage_tick: u64,
    pub cage_released: u64,
    pub cage_idle: u64,
    pub spider_tick: u64,
    pub spider_blocked: u64,
    pub spider_rollback: u64,
    pub plant_tick: u64,
    pub plant_miss: u64,
    pub mouse_tick: u64,
    pub mouse_collected: u64,
    pub mouse_blocked: u64,
    pub transfer_inactive: u64,
    pub transfer_tick: u64,
    pub transfer_cooldown: u64,
}

impl QuietCosts {
    fn ibm_xt() -> Self {
        Self {
            cage_tick: 78_470,
            cage_released: 95_650,
            cage_idle: 212_560,
            spider_tick: 84_650,
            spider_blocked: 183_330,
            spider_rollback: 429_730,
            plant_tick: 79_620,
            plant_miss: 359_120,
            mouse_tick: 79_620,
            mouse_collected: 131_480,
            mouse_blocked: 306_320,
            transfer_inactive: 33_310,
            transfer_tick: 88_000,
            transfer_cooldown: 180_190,
        }
    }

    fn nanos(self, work: crate::room::QuietWork) -> u64 {
        use crate::room::QuietWork;
        match work {
            QuietWork::CageTick => self.cage_tick,
            QuietWork::CageReleased => self.cage_released,
            QuietWork::CageIdle => self.cage_idle,
            QuietWork::SpiderTick => self.spider_tick,
            QuietWork::SpiderBlocked => self.spider_blocked,
            QuietWork::SpiderRollback => self.spider_rollback,
            QuietWork::PlantTick => self.plant_tick,
            QuietWork::PlantMiss => self.plant_miss,
            QuietWork::MouseTick => self.mouse_tick,
            QuietWork::MouseCollected => self.mouse_collected,
            QuietWork::MouseBlocked => self.mouse_blocked,
            QuietWork::TransferInactive => self.transfer_inactive,
            QuietWork::TransferTick => self.transfer_tick,
            QuietWork::TransferCooldown => self.transfer_cooldown,
        }
    }
}

/// Scoped execution costs for a selected reference CPU profile.
/// Values are virtual nanoseconds.
/// These are not physical 8088 cycle counts. Base call allowances and active
/// drawing costs approximate execution time; actor state and tick gates are
/// implemented separately. Uncalibrated active costs are marked below.
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
    pub entry: Option<EntryCosts>,
    pub ground_dog: u64,
    pub ground_dog_draw: u64,
    /// Complete contact/entry with eleven-row player and fifteen-row dog images.
    /// Alley (with edge reset), then the shared ordinary-room path.
    pub pursuit_contact: Option<[u64; 2]>,
    pub projectile_draw: u64,
    pub actor_group: u64,
    pub clothes_shift: u64,
    pub clothes_new_tile: u64,
    pub window_work: u64,
    pub window: Option<WindowCosts>,
    pub room_actor_gate: u64,
    pub room_cage_draw: u64,
    pub room_cage_push: u64,
    pub room_bird_draw: u64,
    pub room_broom_gate: u64,
    pub room_broom_blocked: u64,
    pub room_broom_draw: u64,
    pub room_broom_restore: u64,
    pub room_effect_poll: u64,
    pub spider_effect_poll: u64,
    pub eel_effect_poll: u64,
    pub eel_palette_poll: u64,
    pub eel_restart_poll: u64,
    pub completion_draw_byte: u64,
    pub completion: Option<CompletionCosts>,
    pub completion_poll: Option<CompletionPollCosts>,
    pub hearts_presentation: Option<HeartsPresentationCosts>,
    pub title: Option<TitleCosts>,
    pub hearts_actor_draw: u64,
    pub hearts: Option<HeartsCosts>,
    pub cheese_mouse_draw: u64,
    pub cheese_mouse_restore: u64,
    pub cheese_mouse_collect: u64,
    pub cheese_transfer_draw: u64,
    pub cheese_transfer_edge: u64,
    pub cheese_transfer_broom_draw: u64,
    pub bookcase_spider_draw: u64,
    pub bookcase_spider_restore: u64,
    pub bookcase_spider_effect_draw: u64,
    pub bookcase_target_draw: u64,
    pub swimming_player: u64,
    pub swimming_draw_setup: u64,
    pub swimming_draw_row: u64,
    pub swimming_draw_byte: u64,
    pub swimming_restore_setup: u64,
    pub swimming_restore_row: u64,
    pub swimming_restore_byte: u64,
    pub swimming_bubble: u64,
    pub swimming_surface_delay: u64,
    pub underwater_contact_gate: u64,
    pub underwater_contact_test: u64,
    pub underwater_contact_each: u64,
    pub underwater_contact_sound: u64,
    pub underwater_actor_gate: u64,
    pub underwater_actor_step: u64,
    pub underwater_actor_draw: u64,
    pub underwater_actor_restore: u64,
    pub underwater_eel_draw: u64,
    pub underwater_eel_restore: u64,
    pub water_surface_gate: u64,
    pub water_surface_step: u64,
    pub water_surface_draw: u64,
    pub fishbowl_gate: u64,
    pub fishbowl_draw: u64,
    pub milk_drink_draw: u64,
    pub milk_attention_draw: u64,
    pub milk: Option<MilkCosts>,
    pub mouse_redraw: u64,
    pub mouse_catch: u64,
    pub bin_push_sound: u64,
    pub forced_release: Option<ForcedReleaseCosts>,
    pub lower_support: Option<LowerSupportCosts>,
    pub floor: Option<FloorCosts>,
    pub upper_support: Option<UpperSupportCosts>,
    pub upper_landing: Option<UpperLandingCosts>,
    /// Complete unsupported descent bodies for the retained twelve-row jump image.
    /// Indexed by native scene; only 1,3..=6 are used.
    pub room_descent: Option<[u64; 8]>,
    pub room_surface: Option<RoomSurfaceCosts>,
    pub hearts_player: Option<HeartsPlayerCosts>,
    pub bin_frame: u64,
    pub bin_restore_row: u64,
    pub bin_draw_row: u64,
    pub bin_start: u64,
    pub loop_return: u64,
    /// Complete retained-image fence fall/reset body, outside the fence effect.
    pub fence_player: Option<u64>,
    pub fence_setup: u64,
    pub fence_bios: u64,
    pub fence_initial_wait: u64,
    pub fence_countdown: u64,
    pub fence_random: u64,
    pub fence_finish: u64,
    pub mouse_effect_poll: u64,
    pub blocking: Option<BlockingCosts>,
    pub failure: Option<FailureCosts>,
    pub setup: Option<SetupCosts>,
    pub bookkeeping: Option<BookkeepingCosts>,
    pub bird: Option<BirdCosts>,
    pub quiet: Option<QuietCosts>,
}

/// Complete bin-forced downward transitions, excluding committed tone loops.
#[derive(Clone, Copy, Debug)]
pub struct ForcedReleaseCosts {
    pub body: u64,
    pub attach: u64,
    pub first_contact: u64,
    pub beep_overhead: u64,
}

/// Ordinary lower-list landing/takeoff work, excluding committed tone loops.
#[derive(Clone, Copy, Debug)]
pub struct LowerSupportCosts {
    pub landing: u64,
    pub takeoff: u64,
    pub attach: u64,
    pub beep_overhead: u64,
}

/// Bounded floor movement; room landings cover the twelve-row jump sprite.
#[derive(Clone, Copy, Debug)]
pub struct FloorCosts {
    pub landing: u64,
    pub draw_row: u64,
    pub takeoff: u64,
    /// Room landing bodies indexed by native scene; XT uses the shared twelve-row cost.
    pub room_landing: [u64; 8],
    pub room_walk: u64,
    pub room_mark: u64,
}

/// Already-supported bodies, excluding the initial landing sound wait.
#[derive(Clone, Copy, Debug)]
pub struct UpperSupportCosts {
    pub pose: u64,
    pub hold: u64,
    pub jump: u64,
    pub drop: u64,
}

/// Initial cling landing, split around the separately timed sound wait.
#[derive(Clone, Copy, Debug)]
pub struct UpperLandingCosts {
    pub alley_before: u64,
    pub bookcase_before: u64,
    /// Other ordinary-room table searches, before the separate sound wait.
    pub room_before: u64,
    pub room_entry: u64,
    pub after: u64,
    /// Combined restore/draw work per retained image row above ten.
    pub extra_row: u64,
    /// Prefix reduction for the sampled stationary thirteen-row image.
    pub stationary_discount: u64,
}

/// Ordinary state-zero furniture work, excluding attached cling and its sound wait.
#[derive(Clone, Copy, Debug)]
pub struct RoomSurfaceCosts {
    /// Complete eleven-row walking bodies, indexed by native scene 1,3..=6.
    pub walk: [u64; 8],
    /// Complete upward-transition bodies that retain the previous image.
    pub jump: [u64; 8],
    /// Complete ten-row landing reference, indexed by native scene 1,3..=6.
    pub landing_base: [u64; 8],
    /// Restore/draw increment per retained image row above ten.
    pub landing_row: u64,
    /// Bookcase support miss that starts a vertical fall without drawing.
    pub bookcase_fall: u64,
    /// Additional setup above the ordinary ground-idle allowance.
    pub idle_search_setup: u64,
    /// Approximate work per visited native support entry, including the hit.
    pub idle_search_entry: u64,
}

/// Hearts common-player allowances, without actor work. Ordinary actions share
/// the XT policy; clipping retains separate bounded fits.
#[derive(Clone, Copy, Debug)]
pub struct HeartsPlayerCosts {
    pub walk: u64,
    pub idle: u64,
    pub keep: u64,
    pub descent: u64,
    pub landing: u64,
    /// Ten-row draw clipped from a retained twelve-row image at the lower edge.
    pub bottom_clip: u64,
    pub clipping: HeartsClippingCosts,
}

/// Clipped drawing/restoration work, fitted to valid retained Hearts images.
#[derive(Clone, Copy, Debug)]
pub struct HeartsClippingCosts {
    pub bottom: u64,
    pub top: u64,
    pub hidden: u64,
    pub restore_row: u64,
    pub draw_row: u64,
}

impl HeartsClippingCosts {
    fn nanos(self, work: crate::player::HeartsClippingWork) -> u64 {
        use crate::player::HeartsClipKind;
        let setup = match work.kind {
            HeartsClipKind::Bottom => self.bottom,
            HeartsClipKind::Top => self.top,
            HeartsClipKind::Hidden => self.hidden,
        };
        setup
            + u64::from(work.restored_rows) * self.restore_row
            + u64::from(work.drawn_rows) * self.draw_row
    }
}

impl ReferenceCosts {
    /// XT-based profile with normalized ordinary player actions.
    /// Movement gates and deliberate waits remain native; comparable ordinary
    /// actions omit room-search overhead. This complete profile supplies its own
    /// bounded capture fits and explicit provisional defaults, independently of
    /// the legacy DOSBox comparison profile. See docs/timing.md.
    pub fn ibm_xt(machine: Machine) -> Self {
        // One allowance per comparable action, independent of room/search depth.
        let walk = 2_720_000;
        let idle = 2_480_000;
        let keep = 360_000;
        let descent = 3_240_000;
        let jump = 300_000;
        let landing_base = 950_000;
        let landing_row = 190_000;
        let cling_prefix = 780_000;
        Self {
            prelude: 45_000,
            control_bios: 50_000,
            control_hold: 33_810,
            control_decode: 178_000,
            pit_read: 30_000,
            pit_start_setup: 8_000,
            pit_repeat: 25_000,
            control_return: 15_324,
            player_gate: 70_000,
            player_candidate: 55_000,
            player_countdown: 28_476,
            player_zero: 20_200,
            player_retrace: 25_000,
            player_retrace_return: 3_000,
            player_keep: keep,
            player_idle: idle,
            player_walk: walk,
            player_airborne: 2_870_000,
            player_descending: descent,
            // Include the support beep once, as in the captured whole bodies.
            player_landing: 26_513_562,
            player_support_release: 23_923_219,
            ground_dog: 93_867,
            ground_dog_draw: 8_395_409,
            pursuit_contact: Some([3_120_000, 2_065_000]),
            // Quiet grouped-call work, fitted against consecutive alley passes.
            actor_group: 800_000,
            projectile_draw: 2_643_667,
            clothes_shift: 7_803_715,
            clothes_new_tile: 8_770_353,
            mouse_redraw: 1_361_800,
            // Native catch helper above the inactive-contact allowance.
            mouse_catch: 7_408_100,
            // Whole trios, including their counted sound loops once. The
            // original PCjr software branch is measured on the same XT CPU.
            bin_push_sound: if machine == Machine::Pcjr {
                35_107_600
            } else {
                69_985_143
            },
            forced_release: Some(ForcedReleaseCosts {
                body: 687_100,
                attach: 50_000,
                first_contact: 88_000,
                // Small fitted programming/caller work above each busy loop;
                // shared XT CPU for both software machine branches.
                beep_overhead: 55_000,
            }),
            lower_support: Some(LowerSupportCosts {
                landing: 3_179_000,
                takeoff: 641_000,
                attach: 50_000,
                beep_overhead: 55_000,
            }),
            upper_support: Some(UpperSupportCosts {
                pose: 2_925_000,
                hold: 115_000,
                jump: 281_000,
                drop: 266_000,
            }),
            upper_landing: Some(UpperLandingCosts {
                alley_before: cling_prefix,
                bookcase_before: cling_prefix,
                room_before: cling_prefix,
                room_entry: 0,
                after: 2_265_000,
                extra_row: 187_500,
                stationary_discount: 20_000,
            }),
            floor: Some(FloorCosts {
                landing: landing_base,
                draw_row: landing_row,
                takeoff: jump,
                room_landing: [landing_base + 12 * landing_row; 8],
                room_walk: walk,
                room_mark: 0,
            }),
            room_descent: Some([descent; 8]),
            room_surface: Some(RoomSurfaceCosts {
                walk: [walk; 8],
                jump: [jump; 8],
                landing_base: [landing_base + 10 * landing_row; 8],
                landing_row,
                bookcase_fall: keep,
                idle_search_setup: 0,
                idle_search_entry: 0,
            }),
            hearts_player: Some(HeartsPlayerCosts {
                walk,
                idle,
                keep,
                descent,
                landing: landing_base + 12 * landing_row,
                bottom_clip: 2_659_000,
                clipping: HeartsClippingCosts {
                    bottom: 650_000,
                    top: 2_000_000,
                    hidden: 1_893_000,
                    restore_row: 60_000,
                    draw_row: 128_000,
                },
            }),
            // Valid saved-background frames, less the quiet grouped baseline.
            // Fixed sprite content makes these fitted row coefficients.
            bin_frame: 399_200,
            bin_restore_row: 39_380,
            bin_draw_row: 268_200,
            bin_start: 104_600,
            // Cage increments are above the provisional room actor gate;
            // the bird entry measures the motion body without its scheduler.
            room_cage_draw: 3_378_400,
            room_cage_push: 2_488_400,
            room_bird_draw: 1_333_410,
            // Broom CS:3150: quiet tick check, blocked work above that check,
            // first drawing, and optional restoration of its previous image.
            room_broom_gate: 97_430,
            room_broom_blocked: 200_000,
            room_broom_draw: 3_709_620,
            room_broom_restore: 1_706_780,
            // Native bookcase helper totals above the provisional room gate.
            bookcase_spider_draw: 5_460_700,
            bookcase_spider_restore: 890_000,
            bookcase_target_draw: 8_929_000,
            // Native cheese helpers above the provisional room gate. Restore-
            // only relocation uses the conservative mouse drawing allowance.
            cheese_mouse_draw: 1_767_100,
            cheese_mouse_restore: 690_000,
            cheese_mouse_collect: 3_878_000,
            cheese_transfer_draw: 1_011_600,
            cheese_transfer_edge: 1_036_200,
            cheese_transfer_broom_draw: 3_020_000,
            // Bowl, fish/eel and surface near-calls on the same XT baseline.
            // Drawing increments exclude their own quiet/accepted call work.
            fishbowl_gate: 84_960,
            fishbowl_draw: 668_000,
            underwater_actor_gate: 78_360,
            underwater_actor_step: 66_840,
            underwater_actor_draw: 562_780,
            underwater_actor_restore: 336_080,
            underwater_eel_draw: 392_440,
            underwater_eel_restore: 156_510,
            water_surface_gate: 85_070,
            water_surface_step: 53_840,
            water_surface_draw: 228_750,
            // Accepted swimming prelude, variable native player rectangles,
            // and nonblocking contact scans. Row/byte values are fitted costs,
            // not universal CGA-memory access times.
            swimming_player: 700_000,
            swimming_draw_setup: 58_400,
            swimming_draw_row: 35_450,
            swimming_draw_byte: 15_050,
            swimming_restore_setup: 89_800,
            swimming_restore_row: 33_350,
            swimming_restore_byte: 4_620,
            swimming_bubble: 535_000,
            // Native FD near-surface clear/set wait is phase-dependent. This
            // prices the counted delay after the final retrace read only.
            swimming_surface_delay: 2_637_000,
            underwater_contact_gate: 978_270,
            underwater_contact_test: 88_370,
            // Nonblocking bookkeeping; active tests and images are separate.
            underwater_contact_each: 200_000,
            // Extra execution above the muted CS:593b call already in contact
            // work. This is PIT programming/BIOS sampling, not tone duration.
            underwater_contact_sound: 126_000,
            // Whole native loop intervals, fitted to the stock XT capture.
            spider_effect_poll: 151_810,
            mouse_effect_poll: 105_600,
            blocking: Some(BlockingCosts::ibm_xt()),
            failure: Some(FailureCosts::ibm_xt()),
            // Pure image helper; contact/scheduler work remains separate.
            bookcase_spider_effect_draw: 14_240_800,
            eel_effect_poll: 84_530,
            eel_palette_poll: 224_130,
            eel_restart_poll: 148_570,
            // Fence pulse spacing is about 462 cycles per counted check plus
            // 380 per random/reload cycle. Their internal split is unmeasured.
            fence_bios: 96_800,
            fence_initial_wait: 0,
            fence_countdown: 0,
            fence_random: 79_620,
            // Complete twelve-row fence falls, both return sides. Port anchors
            // leave a small setup/return overlap within this rounded body fit.
            fence_player: Some(2_400_000),
            // CS:59cb..59df and the final shared stop call, respectively.
            // Player reset/background work remains in its enclosing allowance.
            fence_setup: 92_710,
            fence_finish: 20_110,
            milk: Some(MilkCosts::ibm_xt()),
            hearts: Some(HeartsCosts::ibm_xt()),
            completion: Some(CompletionCosts::ibm_xt()),
            completion_poll: Some(CompletionPollCosts::ibm_xt()),
            hearts_presentation: Some(HeartsPresentationCosts::ibm_xt()),
            title: Some(TitleCosts::ibm_xt()),
            entry: Some(EntryCosts::ibm_xt()),
            window: Some(WindowCosts::ibm_xt()),
            setup: Some(SetupCosts::ibm_xt()),
            bookkeeping: Some(BookkeepingCosts::ibm_xt()),
            bird: Some(BirdCosts::ibm_xt()),
            quiet: Some(QuietCosts::ibm_xt()),
            // Provisional caller/polling allowances, not independent XT
            // measurements. Active helper increments already account for this
            // shared room baseline; changing it requires reviewing those fits.
            room_actor_gate: 200_000,
            room_effect_poll: 50_000,
            // Return work is already included in the enclosing allowances.
            loop_return: 0,
            // Explicit fallback drawing defaults. The component models above
            // supersede these on their covered paths. Keep the established
            // values for unsupported shapes, diagnostic overrides and score
            // text whose firmware-dependent drawing slowdown is not modeled.
            player_entry: 2_875_000,
            completion_draw_byte: 4_000,
            window_work: 480_000,
            hearts_actor_draw: 1_500_000,
            milk_drink_draw: 1_500_000,
            milk_attention_draw: 1_000_000,
        }
    }

    /// Work after one title BIOS sample; the caller charges its prelude first.
    pub fn splash_nanos(&self, step: crate::splash::SplashStep) -> u64 {
        if let Some(costs) = self.title {
            costs.nanos(step.work, self)
        } else {
            self.player_gate
                + self.player_countdown
                + if step.player_accepted {
                    self.player_walk
                } else if step.redrawn {
                    self.window_work
                } else {
                    0
                }
        }
    }

    /// Legacy fixed-240 DOSBox-X profile for explicit comparison runs.
    /// Ordinary play and construction of the XT profile do not use this profile.
    pub fn dosbox_x_240(machine: Machine) -> Self {
        // Rounded guest-time stage medians from the fixed-240 reference.
        // Software-breakpoint observation and state-dependent paths limit these
        // fitted allowances; see docs/timing.md. New actor costs are provisional.
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
            forced_release: None,
            lower_support: None,
            floor: None,
            upper_support: None,
            upper_landing: None,
            room_descent: None,
            room_surface: None,
            hearts_player: None,
            // Legacy clipped-entry allowance; XT entry costs and fallbacks
            // are specified independently in ibm_xt.
            player_entry: 2_875_000,
            entry: None,
            ground_dog: 95_833,
            ground_dog_draw: 1_500_000,
            pursuit_contact: None,
            projectile_draw: 500_000,
            actor_group: 345_833,
            // Additional work above the ordinary grouped-call allowance.
            // Fixed-240 helper probes measured about 8ms for a band shift,
            // 9.5ms with tile generation, and 0.1ms for a tick-gated hold.
            clothes_shift: 7_900_000,
            clothes_new_tile: 9_400_000,
            // Original window-row helpers measured about 0.5ms at fixed240.
            // New-window and contact/hold paths still use this bounded fit.
            window_work: 480_000,
            window: None,
            // Provisional room allowances; not independently CPU-calibrated.
            // Original tick gates, physics and blocking waits remain separate.
            room_actor_gate: 200_000,
            room_cage_draw: 2_000_000,
            // The older profile did not price the cage's player redraw.
            room_cage_push: 0,
            room_bird_draw: 600_000,
            // Preserve the older profile's drawing-only allowance.
            room_broom_gate: 0,
            room_broom_blocked: 0,
            room_broom_draw: 2_500_000,
            room_broom_restore: 0,
            room_effect_poll: 50_000,
            spider_effect_poll: 50_000,
            eel_effect_poll: 50_000,
            eel_palette_poll: 50_000,
            eel_restart_poll: 50_000,
            // Provisional drawing allowance for wipes and cat-face stamps.
            // Original presentation tick/retrace waits are modeled separately.
            completion_draw_byte: 4_000,
            completion: None,
            completion_poll: None,
            hearts_presentation: None,
            title: None,
            // Provisional scene-seven drawing work above its tick gates.
            hearts_actor_draw: 1_500_000,
            hearts: None,
            // Provisional scene-four costs above its original tick gates.
            cheese_mouse_draw: 600_000,
            cheese_mouse_restore: 0,
            cheese_mouse_collect: 600_000,
            cheese_transfer_draw: 2_000_000,
            cheese_transfer_edge: 0,
            cheese_transfer_broom_draw: 0,
            // Provisional scene-three work above the original tick gates.
            bookcase_spider_draw: 1_500_000,
            bookcase_spider_restore: 0,
            // The contact image was not part of the motion timing capture.
            bookcase_spider_effect_draw: 1_500_000,
            bookcase_target_draw: 1_500_000,
            // Provisional fishbowl/swimming work, separate from BIOS gates.
            swimming_player: 3_500_000,
            swimming_draw_setup: 0,
            swimming_draw_row: 0,
            swimming_draw_byte: 0,
            swimming_restore_setup: 0,
            swimming_restore_row: 0,
            swimming_restore_byte: 0,
            swimming_bubble: 0,
            swimming_surface_delay: 0,
            underwater_contact_gate: 0,
            underwater_contact_test: 0,
            underwater_contact_each: 0,
            underwater_contact_sound: 0,
            underwater_actor_gate: 200_000,
            underwater_actor_step: 0,
            underwater_actor_draw: 250_000,
            underwater_actor_restore: 0,
            underwater_eel_draw: 250_000,
            underwater_eel_restore: 0,
            water_surface_gate: 200_000,
            water_surface_step: 0,
            water_surface_draw: 200_000,
            fishbowl_gate: 200_000,
            fishbowl_draw: 500_000,
            // Provisional milk-room drawing costs, separate from native gates.
            milk_drink_draw: 1_500_000,
            milk_attention_draw: 1_000_000,
            milk: None,
            // Full original helpers: normal draw about 1.6ms, catch about 8ms.
            // Add only work above the ordinary gated group allowance.
            mouse_redraw: 1_550_000,
            mouse_catch: 7_600_000,
            // Provisional total allowance, including the three counted loops.
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
            fence_player: None,
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
            blocking: None,
            failure: None,
            setup: None,
            bookkeeping: None,
            bird: None,
            quiet: None,
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

#[derive(Clone, Copy, PartialEq)]
enum DamageResume {
    Player,
    Projectiles,
    Spider,
}

pub struct Runtime {
    pub clock: VirtualClock,
    pub player_gate: PlayerGate,
    pub controls_gate: ControlsGate,
    pub divider: OuterLoopDivider,
    pub controls: Controls,
    /// Responsive host input refresh; native timing charges stay unchanged.
    pub responsive_controls: bool,
    /// Last host sample, kept separate from helper-generated directions.
    sampled_input: Option<(Controls, bool)>,
    costs: ReferenceCosts,
    machine: Machine,
    fence: Option<FenceEffect>,
    mouse_pause: Option<u16>,
    cage_falling: bool,
    damage_resume: Option<DamageResume>,
    scene: u16,
}

impl Runtime {
    pub fn new(machine: Machine) -> Self {
        Self::with_costs(machine, ReferenceCosts::ibm_xt(machine))
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
        assert!(
            costs.spider_effect_poll > 0
                && costs.eel_effect_poll > 0
                && costs.eel_palette_poll > 0
                && costs.eel_restart_poll > 0,
            "Damage-effect polling must advance virtual time"
        );
        if let Some(cost) = costs.blocking {
            assert!(
                cost.mouse_sound_before + cost.mouse_sound_after > 0 && cost.cage_poll > 0,
                "Blocking-effect polling must advance virtual time"
            );
        }
        if let Some(cost) = costs.failure {
            assert!(
                cost.message_muted_poll > 0
                    && cost.message_sound_poll > 0
                    && cost.dog_muted_poll > 0
                    && cost.dog_sound_poll > 0,
                "Failure polling must advance virtual time"
            );
        }
        Self {
            clock: VirtualClock::default(),
            player_gate: PlayerGate::new(u16::MAX, 0),
            controls_gate: ControlsGate::new(u16::MAX - 1),
            divider: OuterLoopDivider::new(0),
            controls: Controls::default(),
            responsive_controls: false,
            sampled_input: None,
            costs,
            machine,
            fence: None,
            mouse_pause: None,
            cage_falling: false,
            damage_resume: None,
            scene: 0,
        }
    }

    fn swimming_image_cost(&self, dimensions: u16, restore: bool) -> u64 {
        let rows = u64::from(dimensions >> 8);
        let bytes = rows * u64::from(dimensions & 255) * 2;
        if restore {
            self.costs.swimming_restore_setup
                + rows * self.costs.swimming_restore_row
                + bytes * self.costs.swimming_restore_byte
        } else {
            self.costs.swimming_draw_setup
                + rows * self.costs.swimming_draw_row
                + bytes * self.costs.swimming_draw_byte
        }
    }

    fn charge_window(&mut self, update: crate::windows::Update) {
        let nanos = if let Some(costs) = self.costs.window {
            costs.nanos(update.work).saturating_sub(costs.countdown)
        } else if update.accepted {
            self.costs.window_work
        } else {
            0
        };
        self.clock.advance_nanos(nanos);
    }

    fn player_draw_cost(
        &self,
        draw: Draw,
        entering: bool,
        waiting: bool,
        airborne: bool,
        descending: bool,
    ) -> u64 {
        if entering
            && let Some(nanos) = self
                .costs
                .entry
                .and_then(|costs| costs.nanos(draw, waiting))
        {
            return nanos;
        }
        match draw {
            Draw::Keep => self.costs.player_keep,
            Draw::Idle => self.costs.player_idle,
            Draw::Erase => self.costs.player_airborne,
            Draw::StridedSprite { .. } => self.costs.player_entry,
            Draw::Sprite(_) if entering => self.costs.player_entry,
            Draw::Sprite(_) if descending => self.costs.player_descending,
            Draw::Sprite(_) if airborne => self.costs.player_airborne,
            Draw::Sprite(_) => self.costs.player_walk,
        }
    }

    fn completion_poll_nanos(&self, kind: Option<crate::presentation::Poll>, sound: bool) -> u64 {
        match (self.costs.completion_poll, kind) {
            (Some(costs), Some(kind)) => costs.poll(kind, sound),
            _ => self.costs.room_effect_poll,
        }
    }

    fn charge_presentation(&mut self, work: crate::presentation::Work, precharged: u64) {
        let (hearts_nanos, hearts_bytes) = self.costs.hearts_presentation.map_or((0, 0), |costs| {
            (costs.nanos(work.hearts), work.hearts.bytes())
        });
        let (failure_nanos, failure_bytes) = self.costs.failure.map_or((0, 0), |costs| {
            (costs.nanos(work.failure), work.failure.bytes())
        });
        let nanos = if let Some(costs) = self.costs.completion {
            let calibrated_bytes =
                work.graphics.score_bytes + work.graphics.wipe.map_or(0, |wipe| wipe.bytes);
            let fallback_bytes = work
                .bytes_drawn
                .saturating_sub(calibrated_bytes + hearts_bytes + failure_bytes);
            let total = costs.nanos(work.graphics)
                + hearts_nanos
                + failure_nanos
                + fallback_bytes as u64 * self.costs.completion_draw_byte;
            // A wipe's measured body includes its polling/loop work. The
            // dispatcher already advanced time before sampling this step.
            if work.graphics.wipe.is_some() {
                total.saturating_sub(precharged)
            } else {
                total
            }
        } else {
            hearts_nanos
                + failure_nanos
                + work
                    .bytes_drawn
                    .saturating_sub(hearts_bytes + failure_bytes) as u64
                    * self.costs.completion_draw_byte
        };
        let bookkeeping = self
            .costs
            .bookkeeping
            .map_or(0, |costs| costs.nanos(work.bookkeeping));
        let tone = self
            .costs
            .completion_poll
            .zip(work.polling)
            .map_or(0, |(costs, work)| costs.tone(work));
        self.clock.advance_nanos(nanos + bookkeeping + tone);
    }

    fn charge_room_gate(&mut self, work: Option<crate::room::QuietWork>) {
        let nanos = match (self.costs.quiet, work) {
            (Some(costs), Some(work)) => costs.nanos(work),
            _ => self.costs.room_actor_gate,
        };
        self.clock.advance_nanos(nanos);
    }

    fn charge_bird(&mut self, step: crate::birdcage::BirdStep) {
        let nanos = match (self.costs.bird, step.work) {
            (Some(costs), Some(work)) => costs.nanos(work),
            _ => self.costs.room_actor_gate + u64::from(step.drew) * self.costs.room_bird_draw,
        };
        self.clock.advance_nanos(nanos);
    }

    fn charge_hearts(&mut self, step: crate::hearts::HeartStep) {
        let nanos = match (self.costs.hearts, step.work) {
            (Some(costs), Some(work)) => costs.nanos(work),
            _ => {
                self.costs.room_actor_gate
                    + if step.drew {
                        self.costs.hearts_actor_draw
                    } else {
                        0
                    }
            }
        };
        self.clock.advance_nanos(nanos);
    }

    fn charge_milk(&mut self, work: crate::milk::Work, sound: &mut crate::sound::Sound) -> u16 {
        use crate::milk::Work;
        let c = self
            .costs
            .milk
            .expect("Milk work requires its execution profile");
        let nanos = match work {
            Work::DrinkGate { dying } => {
                if dying {
                    c.drink_dead
                } else {
                    c.drink_gate
                }
            }
            Work::DrinkStop {
                visible,
                dimensions,
            } => {
                c.drink_stop
                    + if visible {
                        c.drink_restore + self.swimming_image_cost(dimensions, false)
                    } else {
                        0
                    }
            }
            Work::DrinkSearch { found, aligned } => {
                if !found {
                    c.drink_search
                } else if aligned {
                    c.drink_prepare
                } else {
                    c.drink_align
                }
            }
            Work::DrinkStart { dimensions } => {
                c.drink_begin + 2 * self.swimming_image_cost(dimensions, true)
            }
            Work::Retrace => {
                self.clock.advance_to_retrace_set();
                0
            }
            Work::RestoreDrink => c.drink_restore,
            Work::DrawDrink => c.drink_draw + c.drink_finish,
            Work::EmptyBowl => c.bowl_empty,
            Work::DrawBowl => c.bowl_draw,
            Work::Broom { restored } => {
                self.costs.room_broom_draw
                    + if restored {
                        self.costs.room_broom_restore
                    } else {
                        0
                    }
            }
            Work::AttentionGate { accepted, dying } => {
                c.attention_gate
                    + if !accepted {
                        0
                    } else if dying {
                        c.attention_dead
                    } else {
                        c.attention_begin
                    }
            }
            Work::AttentionSlot { active } => {
                c.attention_slot + if active { c.attention_active } else { 0 }
            }
            Work::Eyes {
                overlap,
                drinking,
                dimensions,
            } => {
                self.clock.advance_nanos(c.eye_prepare);
                if overlap {
                    self.clock.advance_to_retrace_set();
                }
                c.eye_draw
                    + if !overlap {
                        0
                    } else {
                        c.overlap_finish
                            + if drinking {
                                c.drink_restore + c.drink_draw
                            } else {
                                self.swimming_image_cost(dimensions, true)
                                    + self.swimming_image_cost(dimensions, false)
                            }
                    }
            }
            Work::PursuitEntry { player_dimensions } => {
                if player_dimensions == 0x0b03 {
                    c.pursuit_entry
                } else {
                    0
                }
            }
            Work::End => 0,
        };
        self.clock.advance_nanos(nanos);
        sound.at(self.clock.elapsed_pit_ticks() as u64);
        self.clock.bios_sample().tick_low
    }

    fn step_milk_drinking(&mut self, alley: &mut Alley) -> Result<(), String> {
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let tick = self.clock.bios_sample().tick_low;
        if self.costs.milk.is_some() {
            alley.advance_milk_drinking_timed(tick, &mut |event, sound| {
                self.charge_milk(event, sound)
            })?;
        } else {
            let drinking = alley.advance_milk_drinking(tick)?;
            self.clock.advance_nanos(self.costs.room_actor_gate);
            if drinking.drew {
                self.clock.advance_nanos(self.costs.milk_drink_draw);
            }
        }
        self.controls = alley.requested_controls();
        Ok(())
    }

    fn step_milk_attention(&mut self, alley: &mut Alley) -> Result<(), String> {
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let tick = self.clock.bios_sample().tick_low;
        if self.costs.milk.is_some() {
            alley.advance_milk_attention_timed(tick, &mut |event, sound| {
                self.charge_milk(event, sound)
            })?;
            self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
        } else {
            let attention = alley.advance_milk_attention(tick)?;
            self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
            self.clock.advance_nanos(self.costs.room_actor_gate);
            if attention.drew {
                self.clock.advance_nanos(self.costs.milk_attention_draw);
            }
        }
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        Ok(())
    }

    fn swimming_contact_cost(&self, contact: crate::fishbowl::ContactStep, dimensions: u16) -> u64 {
        let sound_work = u64::from(contact.audible_requests) * self.costs.underwater_contact_sound;
        if let Some((cost, scan)) = self
            .costs
            .blocking
            .and_then(|cost| cost.lethal_scan(contact).map(|scan| (cost, scan)))
        {
            return sound_work
                + scan
                + cost.eel_failure_draw
                + if contact.contacts != 0 {
                    self.swimming_image_cost(dimensions, true)
                } else {
                    0
                }
                + u64::from(contact.restored_fish) * self.costs.underwater_actor_restore;
        }
        // Unsupported diagnostic combinations retain the partial-scan fallback.
        let gate = if contact.failure && self.costs.blocking.is_some() {
            self.costs.underwater_contact_gate * u64::from(contact.visited) / 24
        } else {
            self.costs.underwater_contact_gate
        };
        sound_work
            + gate
            + u64::from(contact.tested) * self.costs.underwater_contact_test
            + u64::from(contact.contacts) * self.costs.underwater_contact_each
            + if contact.contacts != 0 {
                self.swimming_image_cost(dimensions, true)
            } else {
                0
            }
            + u64::from(contact.restored_fish) * self.costs.underwater_actor_restore
            + u64::from(contact.restored_eels) * self.costs.underwater_eel_restore
            + self.costs.blocking.map_or(0, |cost| {
                if contact.failure {
                    cost.eel_lethal_test + cost.eel_failure_draw
                } else {
                    0
                }
            })
    }

    fn charge_swimming(&mut self, work: crate::swimming::SwimmingWork, busy_ticks: u64) {
        let prelude = self.costs.swimming_player
            + if work.bubble {
                self.costs.swimming_bubble
            } else {
                0
            };
        let post = self.swimming_image_cost(work.previous_dimensions, true)
            + self.swimming_image_cost(work.dimensions, false)
            + self.swimming_contact_cost(work.contacts, work.dimensions)
            + if work.contacts.contacts != 0 && !work.contacts.failure {
                // CS:0ba8 redraws the player after the first contact removed it.
                self.swimming_image_cost(work.dimensions, false)
            } else {
                0
            };
        let busy_ns = busy_ticks * 1_000_000_000 / crate::timing::PIT_HZ;
        self.clock.advance_nanos(prelude.saturating_sub(busy_ns));
        self.clock.advance_pit_ticks(busy_ticks);
        if work.surface_wait && self.costs.swimming_surface_delay != 0 {
            self.clock.advance_to_next_retrace();
            self.clock.advance_nanos(self.costs.swimming_surface_delay);
        }
        self.clock
            .advance_nanos(post.saturating_sub(busy_ns.saturating_sub(prelude)));
    }

    fn pursuit_contact_cost(&self, alley: &Alley) -> u64 {
        if alley.pursuit_contact_dimensions() != Some((0x0b03, 0x0f04)) {
            return 0;
        }
        self.costs
            .pursuit_contact
            .map_or(0, |costs| costs[usize::from(alley.scene() != 0)])
    }

    fn check_player_dog(&mut self, alley: &mut Alley) -> Result<bool, String> {
        let entry = self.pursuit_contact_cost(alley);
        let started = alley.check_player_dog()?;
        if started {
            self.clock.advance_nanos(entry);
        }
        Ok(started)
    }

    fn step_ground_dog(&mut self, alley: &mut Alley) -> Result<(), String> {
        let was_dying = alley.dying();
        let entry = self.pursuit_contact_cost(alley);
        let sample = self.clock.sample();
        if alley.advance_ground_dog(sample.bios.tick_low, sample.retrace)? {
            self.clock.advance_nanos(self.costs.ground_dog_draw);
        }
        if !was_dying && alley.dying() {
            // The contact path restores images instead of drawing the normal
            // moving dog. Charge entry once; pursuit movement has its own gate.
            self.clock.advance_nanos(entry);
        }
        self.clock.advance_nanos(self.costs.ground_dog);
        Ok(())
    }

    fn step_fishbowl(&mut self, alley: &mut Alley) -> Result<(), String> {
        let bowl = alley.advance_fishbowl(self.clock.sample().bios.tick_low)?;
        self.clock.advance_nanos(self.costs.fishbowl_gate);
        if bowl.drew {
            self.clock.advance_nanos(self.costs.fishbowl_draw);
        }
        Ok(())
    }

    fn step_underwater_actors(&mut self, alley: &mut Alley) -> Result<(), String> {
        let actor = alley.advance_underwater_actors(self.clock.sample().bios.tick_low)?;
        self.clock.advance_nanos(self.costs.underwater_actor_gate);
        if actor.accepted {
            self.clock.advance_nanos(self.costs.underwater_actor_step);
        }
        if actor.drew {
            self.clock.advance_nanos(if actor.eel {
                self.costs.underwater_eel_draw
            } else {
                self.costs.underwater_actor_draw
            });
        }
        if actor.restored {
            self.clock.advance_nanos(if actor.eel {
                self.costs.underwater_eel_restore
            } else {
                self.costs.underwater_actor_restore
            });
        }
        Ok(())
    }

    fn step_water_surface(&mut self, alley: &mut Alley) -> Result<(), String> {
        let surface = alley.advance_water_surface(self.clock.sample().bios.tick_low)?;
        self.clock.advance_nanos(self.costs.water_surface_gate);
        if surface.accepted {
            self.clock.advance_nanos(self.costs.water_surface_step);
        }
        if surface.drew {
            self.clock.advance_nanos(self.costs.water_surface_draw);
        }
        Ok(())
    }

    fn step_cheese_mice(&mut self, alley: &mut Alley) -> Result<(), String> {
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let mice = alley.advance_cheese_mice(self.clock.sample().bios.tick_low)?;
        self.charge_room_gate(mice.quiet);
        if mice.collected {
            // Collection includes mouse/player background work and HUD drawing.
            self.clock.advance_nanos(self.costs.cheese_mouse_collect);
        } else if mice.drew {
            self.clock.advance_nanos(self.costs.cheese_mouse_draw);
            if mice.restored && mice.image_drawn {
                self.clock.advance_nanos(self.costs.cheese_mouse_restore);
            }
        }
        Ok(())
    }

    fn step_cheese_transfer(&mut self, alley: &mut Alley) -> Result<(), String> {
        let transfer = alley.advance_cheese_transfer(self.clock.sample().bios.tick_low)?;
        self.charge_room_gate(transfer.quiet);
        if transfer.drew {
            self.clock.advance_nanos(self.costs.cheese_transfer_draw);
            if transfer.started || transfer.finished {
                self.clock.advance_nanos(self.costs.cheese_transfer_edge);
            }
        }
        if transfer.broom_restored {
            self.clock.advance_nanos(self.costs.room_broom_restore);
        }
        if transfer.broom_drawn {
            self.clock
                .advance_nanos(self.costs.cheese_transfer_broom_draw);
        }
        Ok(())
    }

    fn step_bookcase_spider(&mut self, alley: &mut Alley) -> Result<(), String> {
        let spider = alley.advance_bookcase_spider(self.clock.sample().bios.tick_low)?;
        self.charge_room_gate(spider.quiet);
        if spider.drew {
            self.clock.advance_nanos(if spider.failure_pending {
                self.costs.bookcase_spider_effect_draw
            } else {
                self.costs.bookcase_spider_draw
            });
        }
        if spider.restored {
            self.clock.advance_nanos(self.costs.bookcase_spider_restore);
        }
        if spider.failure_pending && self.costs.blocking.is_some() {
            // CS:3dd0 samples the wait baseline after drawing the image.
            self.start_damage_effect_wait(alley);
        }
        Ok(())
    }

    fn step_bookcase_targets(&mut self, alley: &mut Alley) -> Result<(), String> {
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let targets = alley.advance_bookcase_targets(self.clock.sample().bios.tick_low)?;
        self.charge_room_gate(targets.quiet);
        if targets.drew {
            self.clock.advance_nanos(self.costs.bookcase_target_draw);
        }
        Ok(())
    }

    fn step_room_broom(&mut self, alley: &mut Alley) -> Result<(), String> {
        let broom = alley.advance_room_broom(self.clock.sample().bios.tick_low)?;
        self.clock.advance_nanos(self.costs.room_broom_gate);
        if let Some(broom) = broom {
            self.clock.advance_nanos(if broom.drawn {
                self.costs.room_broom_draw
            } else {
                self.costs.room_broom_blocked
            });
            if broom.restored {
                self.clock.advance_nanos(self.costs.room_broom_restore);
            }
        }
        Ok(())
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

    /// Alt drinks milk, uses cheese holes or places a gift. Like directions,
    /// action is latched only by the original two-tick input gate; it does not
    /// interrupt blocking work.
    pub fn advance_pass_with_action(
        &mut self,
        alley: &mut Alley,
        held: Controls,
        action: bool,
    ) -> Result<PassOutcome, String> {
        self.advance_pass_with_input(alley, |_| (held, action))
    }

    /// Sample host history at the original control gate's virtual timestamp.
    /// Responsive mode also refreshes requests between native
    /// polls, without charging extra emulated work. Blocking work never samples.
    pub fn advance_pass_with_input(
        &mut self,
        alley: &mut Alley,
        mut sample: impl FnMut(u128) -> (Controls, bool),
    ) -> Result<PassOutcome, String> {
        self.advance_pass_with_scene_input(alley, |at, _scene| sample(at))
    }

    /// Host adapters can resolve scene-dependent controls at the actual input
    /// sample, including when pending work completes a room transition first.
    pub(crate) fn advance_pass_with_scene_input(
        &mut self,
        alley: &mut Alley,
        mut sample: impl FnMut(u128, u16) -> (Controls, bool),
    ) -> Result<PassOutcome, String> {
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let mut event = self.advance_pass_inner(alley, &mut sample)?;
        self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        event.end_nanos = self.clock.elapsed_nanos();
        Ok(event)
    }

    fn charge_swimming_before_damage(
        &mut self,
        alley: &mut Alley,
        work: crate::swimming::SwimmingWork,
    ) {
        if self.costs.blocking.is_some() {
            self.charge_swimming(work, 0);
            // CS:353f follows player work, the partial contact scan and image.
            self.start_damage_effect_wait(alley);
        }
    }

    fn start_damage_effect_wait(&mut self, alley: &mut Alley) {
        if let Some(cost) = self.costs.blocking {
            match alley.damage_poll() {
                DamagePoll::Spider => {
                    self.clock.advance_nanos(cost.spider_entry);
                    alley.start_spider_effect_wait(self.clock.sample().bios.tick_low);
                }
                DamagePoll::EelRestart => {
                    self.clock.advance_nanos(cost.eel_entry);
                    alley.start_eel_effect_wait(self.clock.sample().bios.tick_low);
                }
                _ => {}
            }
        }
    }

    fn start_fence_effect(&mut self, alley: &mut Alley) {
        self.controls = Controls::default();
        self.player_gate.reset();
        self.clock.advance_nanos(self.costs.fence_setup);
        alley.sound.gate(alley.sound.control() & !1);
        self.fence = Some(FenceEffect::new(self.clock.sample().bios.tick_low));
    }

    fn stop_fence_effect(&mut self, alley: &mut Alley) {
        self.clock.advance_nanos(self.costs.fence_finish);
        alley.sound.stop();
        self.fence = None;
    }

    fn step_damage_effect(&mut self, alley: &mut Alley) -> Result<bool, String> {
        let kind = alley.damage_poll();
        let cost = match kind {
            DamagePoll::Ordinary => self.costs.room_effect_poll,
            DamagePoll::Spider => self.costs.spider_effect_poll,
            DamagePoll::Eel => self.costs.eel_effect_poll,
            DamagePoll::EelPalette => self.costs.eel_palette_poll,
            DamagePoll::EelRestart => self.costs.eel_restart_poll,
        };
        self.clock.advance_nanos(cost);
        let sample = self.clock.sample();
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let finished = alley.advance_damage_effect(
            sample.bios.tick_low,
            sample.pit_counter,
            sample.retrace,
        )?;
        if let Some(cost) = self.costs.blocking.filter(|_| finished) {
            let cleanup = match kind {
                DamagePoll::Spider => cost.spider_exit + cost.effect_stop,
                DamagePoll::Eel | DamagePoll::EelPalette | DamagePoll::EelRestart => {
                    cost.effect_stop
                }
                DamagePoll::Ordinary => 0,
            };
            self.clock.advance_nanos(cleanup);
        }
        Ok(finished)
    }

    fn step_mouse_effect(
        &mut self,
        alley: &mut Alley,
        start_tick: u16,
    ) -> Result<(ClockSample, bool), String> {
        let sound = alley.native_sound_enabled();
        self.clock.advance_nanos(self.costs.blocking.map_or(
            self.costs.mouse_effect_poll,
            |cost| {
                if sound {
                    cost.mouse_sound_before
                } else {
                    self.costs.mouse_effect_poll
                }
            },
        ));
        let tone_sample = self.clock.sample();
        if self.costs.blocking.is_some() {
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        }
        let tone = alley.mouse_effect_random(tone_sample.bios.tick_low);
        if let Some(cost) = self.costs.blocking.filter(|_| sound) {
            self.clock
                .advance_nanos(cost.mouse_sound_after + if tone { cost.mouse_tone } else { 0 });
        }
        // The exit poll follows the sound poll and can see a later tick.
        let sample = self.clock.sample();
        let finished = sample.bios.tick_low.wrapping_sub(start_tick) >= 8;
        if finished {
            alley.finish_mouse_effect()?;
            if let Some(cost) = self.costs.blocking {
                self.clock.advance_nanos(cost.mouse_cleanup);
            }
        }
        Ok((sample, finished))
    }

    fn step_cage_effect(&mut self, alley: &mut Alley) -> Result<crate::birdcage::FallStep, String> {
        self.clock.advance_nanos(
            self.costs
                .blocking
                .map_or(self.costs.room_effect_poll, |cost| cost.cage_poll),
        );
        let sample = self.clock.sample();
        alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
        let fall = alley.advance_room_fall(sample.bios.tick_low)?;
        let work = self.costs.blocking.map_or(
            if fall.drew {
                self.costs.room_cage_draw
            } else {
                0
            },
            |cost| cost.cage_work(fall, alley.native_sound_enabled()),
        );
        self.clock.advance_nanos(work);
        Ok(fall)
    }

    fn advance_pass_inner(
        &mut self,
        alley: &mut Alley,
        sample_input: &mut impl FnMut(u128, u16) -> (Controls, bool),
    ) -> Result<PassOutcome, String> {
        let start_nanos = self.clock.elapsed_nanos();
        self.synchronize_scene(alley);
        if alley.game_over() {
            self.clock.advance_nanos(self.costs.room_effect_poll);
            return Ok(self.room_wait_outcome(alley, start_nanos));
        }
        if alley.damage_waiting() {
            if !self.step_damage_effect(alley)? || alley.game_over() {
                return Ok(self.room_wait_outcome(alley, start_nanos));
            }
            let resume = self.damage_resume.take();
            return self.finish_pass_from(alley, start_nanos, false, None, resume);
        }
        if alley.presentation_active() {
            let poll = match (self.costs.failure, alley.failure_poll()) {
                (Some(cost), Some(kind)) => cost.poll(kind, alley.native_sound_enabled()),
                _ => self
                    .completion_poll_nanos(alley.presentation_poll(), alley.native_sound_enabled()),
            };
            self.clock.advance_nanos(poll);
            let sample = self.clock.sample();
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
            let work = alley.advance_presentation(sample.bios.tick_low, sample.retrace)?;
            self.charge_presentation(work, poll);
            if work.failure.message && self.costs.failure.is_some() {
                alley.finish_failure_drawing(self.clock.sample().bios.tick_low);
            }
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
            alley.finish_presentation_step(&work, self.clock.sample().bios.tick_low)?;
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
            let (sample, finished) = self.step_mouse_effect(alley, start_tick)?;
            if finished {
                self.mouse_pause = None;
                // The suspended mouse CALL has returned. Resume after the
                // actor batch, without repeating its divider or actor calls.
                self.clock.advance_nanos(self.costs.loop_return);
                let return_tick = if self.costs.blocking.is_some() {
                    self.clock.sample().bios.tick_low
                } else {
                    sample.bios.tick_low
                };
                if alley.finish_scene(return_tick)? {
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
                    alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                    alley.advance_random();
                    self.clock.advance_nanos(self.costs.fence_random);
                }
                FenceAction::Complete => {
                    self.stop_fence_effect(alley);
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
            let fall = self.step_cage_effect(alley)?;
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
        if !resumed_fall && (control_polled || self.responsive_controls) {
            let (held, action) = sample_input(self.clock.elapsed_nanos(), alley.scene());
            alley.latch_action(action);
            if alley.scene() == 6 {
                // The milk helper owns the effective direction between input
                // polls, including passes rejected by its BIOS tick gate.
                // Responsive sampling must deliver host changes immediately,
                // without repeatedly erasing an unchanged auto-approach request.
                if control_polled || self.sampled_input != Some((held, action)) {
                    alley.latch_controls(held);
                }
            } else {
                self.controls = held;
            }
            self.sampled_input = Some((held, action));
        }
        if control_polled {
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
                alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                alley.advance_room_tone(self.clock.sample().bios.tick_low);
                self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
                let cage = alley.advance_room_cage(self.clock.sample().bios.tick_low)?;
                self.charge_room_gate(cage.quiet);
                if cage.drew {
                    self.clock.advance_nanos(self.costs.room_cage_draw);
                }
                if cage.player_moved {
                    self.clock.advance_nanos(self.costs.room_cage_push);
                }
                if cage.falling {
                    self.cage_falling = true;
                    let mut event = self.room_wait_outcome(alley, start_nanos);
                    event.control_polled = control_polled;
                    return Ok(event);
                }
            }
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
            let bird = alley.advance_room_bird(self.clock.sample().bios.tick_low)?;
            self.charge_bird(bird);
        }
        if matches!(alley.scene(), 1..=4 | 6..=7) {
            // These scenes expire the shared room tone before the player call.
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
            alley.advance_room_tone(self.clock.sample().bios.tick_low);
            self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
        }
        if alley.scene() == 6 {
            // CS:02ce..02d4: drinking and sleeping-dog attention precede the
            // player call. The drink helper may replace latched directions.
            self.step_milk_drinking(alley)?;
            self.step_milk_attention(alley)?;
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
            && (alley.scene() == 2
                || (alley.player.horizontal_direction == 0
                    && alley.player.vertical_direction == 0));
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
        if let Some(accepted) = acceptance {
            if alley.scene() == 0 && alley.check_player_projectile(accepted.tick_low)? {
                if alley.damage_waiting() {
                    self.damage_resume = Some(DamageResume::Player);
                    return Ok(self.room_wait_outcome(alley, start_nanos));
                }
                return self.finish_pass(alley, start_nanos, control_polled, acceptance);
            }
            let airborne = alley.player.vertical_direction != 0;
            let descending = alley.player.vertical_direction == 1;
            let entering = alley.player.entry_state != 0;
            let retained_airborne_image = alley.player.has_retained_airborne_image();
            let fence_player_cost = self.costs.fence_player.filter(|_| {
                alley.scene() == 0
                    && descending
                    && !entering
                    && alley.player.support_state == 0
                    && matches!(alley.player.horizontal_direction, 1 | 255)
                    && alley.player.has_retained_fence_fall_image()
                    && !alley.dying()
            });
            let retained_idle_image = alley.player.has_retained_idle_image();
            let retained_walking_image = alley.player.has_retained_walking_image();
            let hearts_player_costs = self.costs.hearts_player.filter(|_| {
                alley.scene() == 7 && !entering && alley.player.support_state == 0 && !alley.dying()
            });
            let grounded_room_surface = matches!(alley.scene(), 1 | 3..=6)
                && !airborne
                && !entering
                && alley.player.y < 180
                && alley.player.support_state == 0
                && !alley.dying();
            let jumping_from_floor = matches!(alley.scene(), 0 | 1 | 3..=6)
                && !airborne
                && !entering
                && alley.player.y == 180
                && alley.player.support_state == 0
                && self.controls.y == -1;
            let walking_from_room_floor = matches!(alley.scene(), 1 | 3..=6)
                && !airborne
                && !entering
                && alley.player.y == 180
                && alley.player.support_state == 0
                && self.controls.x != 0
                && self.controls.y == 0;
            let entry_waiting = alley.player.entry_delay != 0;
            let supported = alley.player.support_state;
            let releasing = supported != 0 && self.controls.y != 0;
            let upper_support_costs = self.costs.upper_support.filter(|_| {
                matches!(alley.scene(), 0 | 1 | 3..=6) && !entering && !airborne && !alley.dying()
            });
            let bin_sound_ready = alley.bin_contact_sound_ready();
            let first_bin_contact = !alley.bin_contact_latched();
            let support_tone = alley.support_sound_latched();
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
            let (outcome, swimming) = if alley.scene() == 2 {
                let (outcome, work) =
                    alley.update_swimming(self.controls, accepted.tick_low, accepted.ax)?;
                (outcome, Some(work))
            } else {
                (
                    alley.advance_player_update_at_tick(self.controls, accepted.tick_low)?,
                    None,
                )
            };
            if alley.damage_waiting() {
                if let Some(work) = swimming {
                    self.charge_swimming_before_damage(alley, work);
                }
                self.damage_resume = Some(DamageResume::Player);
                return Ok(self.room_wait_outcome(alley, start_nanos));
            }
            if !matches!(outcome.draw, Draw::Keep | Draw::Erase)
                && self.check_player_dog(alley)?
                && alley.scene() == 0
            {
                self.player_gate.reset();
                self.controls = Controls::default();
            }
            if let Some(controls) = outcome.forced_controls {
                self.controls = controls;
            }
            // Track the lower-list hit before a transition clears its support
            // result. Native attachment can be followed by release this call.
            let transitioned = outcome.support == crate::player::Support::None
                && alley.player.vertical_direction != 0;
            let forced = outcome.forced_controls.is_some();
            let forced_costs = self.costs.forced_release.filter(|_| forced);
            let lower_attach = outcome.lower_support && !support_tone;
            if lower_attach {
                alley.support_sound(0x400, true);
            }
            let mut support_ticks = alley.sound.take_busy_ticks();
            let mut beep_count = u64::from(support_ticks != 0);
            if forced && bin_sound_ready {
                alley.bin_sound();
                let ticks = alley.sound.take_busy_ticks();
                if forced_costs.is_some() {
                    support_ticks += ticks;
                    beep_count += 3 * u64::from(ticks != 0);
                } else {
                    self.clock.advance_nanos(
                        self.costs
                            .bin_push_sound
                            .saturating_sub(ticks * 1_000_000_000 / crate::timing::PIT_HZ),
                    );
                    self.clock.advance_pit_ticks(ticks);
                }
            }
            if (transitioned && (support_tone || lower_attach) && outcome.bin_contact_checked)
                || forced
            {
                alley.support_sound(0x390, false);
            }
            if alley.scene() == 0
                && !outcome.bin_contact_checked
                && outcome.support != crate::player::Support::LowerList
            {
                // A failed lower-list search clears 127c before the ordinary
                // fall initializer; walking off a bin does not play 58f8.
                alley.clear_support_sound();
            }
            let release_ticks = alley.sound.take_busy_ticks();
            support_ticks += release_ticks;
            beep_count += u64::from(release_ticks != 0);
            let upper_landing_costs = self.costs.upper_landing.and_then(|costs| {
                let eligible = airborne
                    && descending
                    && !entering
                    && supported == 0
                    && retained_airborne_image
                    && !alley.dying()
                    && !forced
                    && outcome.landed
                    && alley.player.support_state == 1
                    && matches!(
                        (alley.scene(), outcome.support),
                        (0, crate::player::Support::UpperBitfield)
                            | (1 | 3..=6, crate::player::Support::RoomTable)
                    );
                if !eligible {
                    return None;
                }
                match outcome.draw {
                    Draw::Sprite(sprite)
                        if alley.player.is_unclipped_airborne_sprite(sprite)
                            && (matches!(alley.scene(), 0 | 3)
                                || (outcome.room_surface_entries != 0
                                    && matches!(sprite.dimensions, 0x0c03 | 0x0d03)))
                            && matches!(
                                (sprite.dimensions, alley.player.horizontal_direction),
                                (0x0a03, _) | (0x0c03, 1 | 255) | (0x0d03, 0)
                            ) =>
                    {
                        Some((costs, (sprite.dimensions >> 8) - 10))
                    }
                    _ => None,
                }
            });
            if outcome.landed && alley.player.support_state != 0 {
                let tick = if let Some((costs, extra_rows)) = upper_landing_costs {
                    // Port-anchored fits include the sound helper's setup and
                    // epilogue. Advance its prefix before sampling the BIOS
                    // tick/PIT phase, rather than adding all work after the wait.
                    let before = if alley.scene() == 0 {
                        costs.alley_before
                    } else if alley.scene() == 3 {
                        costs.bookcase_before
                    } else {
                        costs.room_before
                            + u64::from(outcome.room_surface_entries) * costs.room_entry
                    };
                    self.clock
                        .advance_nanos(before.saturating_sub(
                            u64::from(extra_rows == 3) * costs.stationary_discount,
                        ));
                    alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                    self.clock.sample().bios.tick_low
                } else {
                    accepted.tick_low
                };
                alley.upper_landing_sound(tick);
                self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
            }
            let cost = if releasing || outcome.forced_controls.is_some() {
                self.costs.player_support_release
            } else if outcome.landed {
                self.costs.player_landing
            } else {
                self.player_draw_cost(outcome.draw, entering, entry_waiting, airborne, descending)
            };
            if let Some(nanos) = fence_player_cost.filter(|_| outcome.edge_reset) {
                // Replaces the generic body; 070d's reset/background save is
                // included. The deliberate fence effect starts separately below.
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some((costs, extra_rows)) = upper_landing_costs {
                self.clock
                    .advance_nanos(costs.after + u64::from(extra_rows) * costs.extra_row);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(costs) = forced_costs {
                self.clock.advance_nanos(
                    costs.body
                        + u64::from(!support_tone) * costs.attach
                        + u64::from(first_bin_contact) * costs.first_contact
                        + beep_count * costs.beep_overhead,
                );
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(costs) = self
                .costs
                .lower_support
                .filter(|_| outcome.lower_support && (outcome.landed || transitioned))
            {
                let body = if outcome.landed {
                    costs.landing
                } else {
                    costs.takeoff
                };
                self.clock.advance_nanos(
                    body + u64::from(lower_attach) * costs.attach
                        + beep_count * costs.beep_overhead,
                );
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(nanos) = upper_support_costs.and_then(|costs| {
                match (
                    supported,
                    alley.player.support_state,
                    outcome.draw,
                    alley.player.vertical_direction,
                ) {
                    (1, 2, Draw::Sprite(sprite), 0)
                        if sprite.source == 0x9da && sprite.dimensions == 0x0e03 =>
                    {
                        Some(costs.pose)
                    }
                    (2, 2, Draw::Keep, 0) => Some(costs.hold),
                    (2, 0, Draw::Keep, 255) => Some(costs.jump),
                    (2, 0, Draw::Keep, 1) => Some(costs.drop),
                    _ => None,
                }
            }) {
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(nanos) =
                self.costs
                    .floor
                    .and_then(|costs| match (alley.scene(), outcome.draw) {
                        (0, Draw::Sprite(sprite))
                            if outcome.landed
                                && outcome.support == crate::player::Support::Floor
                                && sprite.dimensions & 255 == 3
                                && (10..=13).contains(&(sprite.dimensions >> 8)) =>
                        {
                            Some(costs.landing + u64::from(sprite.dimensions >> 8) * costs.draw_row)
                        }
                        (scene @ (1 | 3..=6), Draw::Sprite(sprite))
                            if outcome.landed
                                && outcome.support == crate::player::Support::Floor
                                && sprite.dimensions == 0x0c03 =>
                        {
                            Some(costs.room_landing[usize::from(scene)])
                        }
                        (1 | 3..=6, Draw::Sprite(sprite))
                            if walking_from_room_floor
                                && !outcome.landed
                                && outcome.support == crate::player::Support::Floor
                                && alley.player.vertical_direction == 0
                                && sprite.dimensions == 0x0b03 =>
                        {
                            Some(costs.room_walk + u64::from(outcome.floor_mark) * costs.room_mark)
                        }
                        (_, Draw::Keep)
                            if jumping_from_floor && alley.player.vertical_direction == 255 =>
                        {
                            Some(costs.takeoff)
                        }
                        _ => None,
                    })
            {
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(nanos) = hearts_player_costs.and_then(|costs| {
                let supported = outcome.support == crate::player::Support::RoomTable
                    && alley.player.vertical_direction == 0;
                match outcome.draw {
                    Draw::Idle | Draw::Keep
                        if !airborne
                            && supported
                            && retained_idle_image
                            && self.controls == Controls::default()
                            && alley.player.horizontal_direction == 0 =>
                    {
                        Some(if outcome.draw == Draw::Idle {
                            costs.idle
                        } else {
                            costs.keep
                        })
                    }
                    Draw::Sprite(sprite)
                        if !airborne
                            && supported
                            && retained_walking_image
                            && !outcome.landed
                            && sprite.dimensions == 0x0b03 =>
                    {
                        Some(costs.walk)
                    }
                    Draw::Sprite(sprite)
                        if airborne
                            && retained_airborne_image
                            && sprite.dimensions == 0x0c03
                            && alley.player.is_unclipped_airborne_sprite(sprite)
                            && outcome.airborne_support_checked =>
                    {
                        if outcome.landed && supported {
                            Some(costs.landing)
                        } else if !outcome.landed
                            && outcome.support == crate::player::Support::None
                            && alley.player.vertical_direction == 1
                        {
                            Some(costs.descent)
                        } else {
                            None
                        }
                    }
                    Draw::Sprite(sprite)
                        if airborne
                            && descending
                            && retained_airborne_image
                            && !outcome.landed
                            && !outcome.airborne_support_checked
                            && outcome.support == crate::player::Support::None
                            && alley.player.is_hearts_ten_row_clip(sprite) =>
                    {
                        Some(costs.bottom_clip)
                    }
                    _ => outcome
                        .hearts_clipping
                        .map(|work| costs.clipping.nanos(work)),
                }
            }) {
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(nanos) = self.costs.room_surface.and_then(|costs| {
                let scene = usize::from(alley.scene());
                if matches!(scene, 1 | 3..=6)
                    && airborne
                    && !entering
                    && retained_airborne_image
                    && outcome.landed
                    && outcome.support == crate::player::Support::RoomTable
                    && alley.player.support_state == 0
                    && let Draw::Sprite(sprite) = outcome.draw
                {
                    let rows = sprite.dimensions >> 8;
                    if sprite.dimensions & 255 == 3
                        && ((12..=13).contains(&rows) || (scene == 3 && rows == 10))
                        && alley.player.is_unclipped_airborne_sprite(sprite)
                    {
                        return Some(
                            costs.landing_base[scene] + u64::from(rows - 10) * costs.landing_row,
                        );
                    }
                }
                if !grounded_room_surface || outcome.landed || alley.player.support_state != 0 {
                    return None;
                }
                if outcome.room_surface_hit
                    && outcome.room_surface_entries != 0
                    && outcome.support == crate::player::Support::RoomTable
                    && retained_idle_image
                    && self.controls == Controls::default()
                    && alley.player.horizontal_direction == 0
                    && alley.player.vertical_direction == 0
                {
                    let idle = match outcome.draw {
                        Draw::Idle => Some(self.costs.player_idle),
                        Draw::Keep => Some(self.costs.player_keep),
                        _ => None,
                    };
                    if let Some(idle) = idle {
                        return Some(
                            idle + costs.idle_search_setup
                                + u64::from(outcome.room_surface_entries) * costs.idle_search_entry,
                        );
                    }
                }
                match outcome.draw {
                    Draw::Sprite(sprite)
                        if outcome.room_surface_hit
                            && outcome.support == crate::player::Support::RoomTable
                            && alley.player.vertical_direction == 0
                            && sprite.dimensions == 0x0b03 =>
                    {
                        Some(costs.walk[scene])
                    }
                    Draw::Keep
                        if self.controls.y == -1
                            && outcome.room_surface_hit
                            && alley.player.vertical_direction == 255
                            && outcome.support == crate::player::Support::None =>
                    {
                        Some(costs.jump[scene])
                    }
                    Draw::Keep
                        if scene == 3
                            && alley.player.vertical_direction == 1
                            && alley.player.horizontal_direction == 0
                            && outcome.support == crate::player::Support::None =>
                    {
                        Some(costs.bookcase_fall)
                    }
                    _ => None,
                }
            }) {
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(nanos) = self.costs.room_descent.and_then(|costs| {
                let scene = alley.scene();
                match outcome.draw {
                    Draw::Sprite(sprite)
                        if matches!(scene, 1 | 3..=6)
                            && airborne
                            && !entering
                            && outcome.airborne_support_checked
                            && !outcome.landed
                            && outcome.support == crate::player::Support::None
                            && alley.player.vertical_direction == 1
                            && sprite.dimensions == 0x0c03
                            && alley.player.is_unclipped_airborne_sprite(sprite) =>
                    {
                        // XT uses the shared falling allowance even when a room
                        // search runs or this update turns from rising to falling.
                        Some(costs[usize::from(scene)])
                    }
                    _ => None,
                }
            }) {
                self.clock.advance_nanos(nanos);
                self.clock.advance_pit_ticks(support_ticks);
            } else if let Some(work) = swimming {
                self.charge_swimming(work, support_ticks);
            } else {
                self.clock.advance_nanos(
                    cost.saturating_sub(support_ticks * 1_000_000_000 / crate::timing::PIT_HZ),
                );
                self.clock.advance_pit_ticks(support_ticks);
            }
            if outcome.edge_reset {
                // The original calls 070d before 59cb: clear requests, accepted
                // tick and countdown now. 069f, divider and accepted AX survive.
                self.start_fence_effect(alley);
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
        self.finish_pass_from(alley, start_nanos, control_polled, acceptance, None)
    }

    fn finish_pass_from(
        &mut self,
        alley: &mut Alley,
        start_nanos: u128,
        control_polled: bool,
        acceptance: Option<PlayerAcceptance>,
        resume: Option<DamageResume>,
    ) -> Result<PassOutcome, String> {
        let before_scene = alley.scene();
        if matches!(before_scene, 1..=7) {
            if before_scene == 7 {
                for call in [
                    Alley::advance_hearts_arrow,
                    Alley::pickup_hearts_item,
                    Alley::place_hearts_item,
                    Alley::advance_hearts_cat,
                ] {
                    alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                    let step = call(alley, self.clock.sample().bios.tick_low)?;
                    self.charge_hearts(step);
                }
            } else if before_scene == 2 {
                self.step_underwater_actors(alley)?;
                self.step_water_surface(alley)?;
            } else {
                if before_scene == 3 && resume != Some(DamageResume::Spider) {
                    // Original scene-three loop: player, spider, plants, broom.
                    self.step_bookcase_spider(alley)?;
                    if alley.damage_waiting() {
                        self.damage_resume = Some(DamageResume::Spider);
                        return Ok(self.room_wait_outcome(alley, start_nanos));
                    }
                }
                if before_scene == 3 {
                    self.step_bookcase_targets(alley)?;
                }
                if before_scene == 4 {
                    // Original scene-four loop: player, transfer, mice, broom,
                    // then termination. Transfers do not block room scheduling.
                    self.step_cheese_transfer(alley)?;
                    self.step_cheese_mice(alley)?;
                }
                if !(before_scene == 6 && alley.dying()) {
                    self.step_room_broom(alley)?;
                }
                if matches!(before_scene, 1 | 3..=5) || (before_scene == 6 && alley.dying()) {
                    self.step_ground_dog(alley)?;
                }
                if before_scene == 1 {
                    self.step_fishbowl(alley)?;
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
        let resumed_projectile = resume == Some(DamageResume::Projectiles);
        if !resumed_projectile {
            let was_dying = alley.dying();
            self.step_ground_dog(alley)?;
            if !was_dying && alley.dying() {
                self.player_gate.reset();
                self.controls = Controls::default();
            }
            if alley.game_over() {
                return Ok(self.room_wait_outcome(alley, start_nanos));
            }
        }
        let actor_group_due = resumed_projectile || self.divider.advance(alley.dying());
        if actor_group_due {
            if !resumed_projectile {
                alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                alley.advance_room_tone(self.clock.sample().bios.tick_low);
                self.clock.advance_pit_ticks(alley.sound.take_busy_ticks());
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
                self.charge_window(window);
                let sample = self.clock.sample();
                if alley.advance_projectiles(sample.bios.tick_low)? {
                    self.clock.advance_nanos(self.costs.projectile_draw);
                }
                if alley.damage_waiting() {
                    self.damage_resume = Some(DamageResume::Projectiles);
                    return Ok(self.room_wait_outcome(alley, start_nanos));
                }
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
            alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
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
                if let Some(cost) = self.costs.blocking {
                    self.clock.advance_nanos(cost.mouse_overlay);
                }
                let tick = self.clock.sample().bios.tick_low;
                self.mouse_pause = Some(tick);
                if self.costs.blocking.is_some() {
                    alley.sound.at(self.clock.elapsed_pit_ticks() as u64);
                }
                let tone = alley.start_mouse_effect_wait(tick);
                if let Some(cost) = self.costs.blocking.filter(|_| tone) {
                    self.clock.advance_nanos(cost.mouse_tone);
                }
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

    fn synchronize_scene(&mut self, alley: &mut Alley) {
        if alley.game_over() {
            // CS:00f3 jumps to the title before constructing another alley.
            self.scene = alley.scene();
            return;
        }
        if let Some(work) = alley.take_selection_work() {
            self.clock.advance_nanos(
                self.costs
                    .bookkeeping
                    .map_or(0, |costs| costs.selection_nanos(work)),
            );
        }
        self.enter_scene(alley.scene(), alley.saved_alley_return());
    }

    fn enter_scene(&mut self, scene: u16, saved_alley: bool) {
        if self.scene != scene {
            self.clock.advance_nanos(
                self.costs
                    .setup
                    .map_or(0, |costs| costs.nanos(scene, saved_alley)),
            );
            self.scene = scene;
            self.player_gate.reset();
            self.controls = Controls::default();
            self.sampled_input = None;
            self.fence = None;
            self.mouse_pause = None;
            self.cage_falling = false;
            self.damage_resume = None;
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
mod tests;
