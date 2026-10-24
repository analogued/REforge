// Copyright (c) 2026 analogued. Project license has not yet been selected.

use super::*;
use crate::timing::BIOS_TICK_PIT_CLOCKS;

#[test]
fn measured_lethal_prefix_replaces_generic_scan_costs_only_for_covered_work() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut costs = ReferenceCosts::ibm_xt(machine);
        costs.underwater_contact_gate = 900_000;
        costs.underwater_contact_test = 800_000;
        let blocking = costs.blocking.as_mut().unwrap();
        blocking.eel_scan_start = 30;
        blocking.eel_scan_slot = 10;
        blocking.eel_scan_miss = 5;
        blocking.eel_scan_collect = 20;
        blocking.eel_failure_draw = 100;
        costs.swimming_restore_setup = 40;
        costs.swimming_restore_row = 0;
        costs.swimming_restore_byte = 0;
        costs.underwater_actor_restore = 50;
        let run = Runtime::with_costs(machine, costs);
        let mut work = crate::fishbowl::ContactStep {
            visited: 13,
            tested: 1,
            hazard_contacts: 1,
            failure: true,
            ..Default::default()
        };
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 260);
        work.tested = 3;
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 270);
        work.visited = 24;
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 380);
        work.contacts = 1;
        assert!(costs.blocking.unwrap().lethal_scan(work).is_none());
        work.collected = 1;
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 435);
        work.restored_fish = 1;
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 485);
        work.contacts = 2;
        work.collected = 2;
        work.restored_fish = 2;
        assert_eq!(run.swimming_contact_cost(work, 0x0c03), 550);
        work.failure = false;
        assert!(costs.blocking.unwrap().lethal_scan(work).is_none());
        let legacy = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
        work.contacts = 0;
        work.collected = 0;
        work.restored_fish = 0;
        work.failure = true;
        assert_eq!(
            legacy.swimming_contact_cost(work, 0x0c03),
            legacy.costs.underwater_contact_gate + 3 * legacy.costs.underwater_contact_test
        );
    }
}

#[test]
fn audible_contact_work_is_charged_once_on_both_scan_routes() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut costs = ReferenceCosts::ibm_xt(machine);
        costs.underwater_contact_sound = 123;
        let run = Runtime::with_costs(machine, costs);
        let mut contact = crate::fishbowl::ContactStep {
            visited: 13,
            tested: 3,
            contacts: 2,
            collected: 2,
            hazard_contacts: 1,
            failure: true,
            ..Default::default()
        };
        for failure in [true, false] {
            contact.failure = failure;
            contact.audible_requests = 0;
            let quiet = run.swimming_contact_cost(contact, 0x0c03);
            contact.audible_requests = 2;
            assert_eq!(run.swimming_contact_cost(contact, 0x0c03), quiet + 246);
            let legacy = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
            let audible = legacy.swimming_contact_cost(contact, 0x0c03);
            contact.audible_requests = 0;
            assert_eq!(legacy.swimming_contact_cost(contact, 0x0c03), audible);
        }
    }
}

#[test]
fn lethal_swimming_stops_scan_pricing_and_omits_the_post_return_redraw() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut costs = ReferenceCosts::ibm_xt(machine);
        costs.swimming_player = 0;
        costs.swimming_draw_setup = 100;
        costs.swimming_draw_row = 0;
        costs.swimming_draw_byte = 0;
        costs.swimming_restore_setup = 0;
        costs.swimming_restore_row = 0;
        costs.swimming_restore_byte = 0;
        costs.underwater_contact_gate = 24_000;
        costs.underwater_contact_test = 1_000;
        costs.underwater_contact_each = 2_000;
        let blocking = costs.blocking.as_mut().unwrap();
        blocking.eel_failure_draw = 10_000;
        blocking.eel_lethal_test = 100;
        let work = crate::swimming::SwimmingWork {
            previous_dimensions: 0x0c03,
            dimensions: 0x0c03,
            bubble: false,
            surface_wait: false,
            contacts: crate::fishbowl::ContactStep {
                visited: 13,
                tested: 1,
                contacts: 1,
                failure: true,
                ..Default::default()
            },
        };
        let mut run = Runtime::with_costs(machine, costs);
        run.charge_swimming(work, 0);
        assert_eq!(run.clock.elapsed_nanos(), 26_200);
        let mut full = work;
        full.contacts.visited = 24;
        let mut run = Runtime::with_costs(machine, costs);
        run.charge_swimming(full, 0);
        assert_eq!(run.clock.elapsed_nanos(), 37_200);
        full.contacts.failure = false;
        let mut run = Runtime::with_costs(machine, costs);
        run.charge_swimming(full, 0);
        assert_eq!(run.clock.elapsed_nanos(), 27_200);

        let mut legacy = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
        legacy.charge_swimming_before_damage(&mut Alley::synthetic(), work);
        assert_eq!(legacy.clock.elapsed_nanos(), 0);
    }
}

#[test]
fn quiet_cost_replaces_the_gate_and_leaves_active_and_legacy_work_alone() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut costs = ReferenceCosts::ibm_xt(machine);
        costs.room_actor_gate = 999_000;
        let mut r = Runtime::with_costs(machine, costs);
        r.charge_room_gate(Some(crate::room::QuietWork::SpiderTick));
        assert_eq!(r.clock.elapsed_nanos(), 84_650);
        r.charge_room_gate(None);
        assert_eq!(r.clock.elapsed_nanos(), 84_650 + 999_000);
        let costs = ReferenceCosts::dosbox_x_240(machine);
        let mut r = Runtime::with_costs(machine, costs);
        r.charge_room_gate(Some(crate::room::QuietWork::SpiderTick));
        assert_eq!(r.clock.elapsed_nanos(), u128::from(costs.room_actor_gate));
    }
}

#[test]
fn whole_bird_cost_replaces_the_legacy_gate_and_drawing_allowances() {
    let step = crate::birdcage::BirdStep {
        drew: true,
        work: Some(crate::birdcage::BirdWork {
            gate: crate::birdcage::BirdGate::Motion,
            random_draws: 3,
            axes: 2,
            ..Default::default()
        }),
        ..Default::default()
    };
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut costs = ReferenceCosts::ibm_xt(machine);
        costs.room_actor_gate = 9_000_000;
        costs.room_bird_draw = 10_000_000;
        let mut runtime = Runtime::with_costs(machine, costs);
        runtime.charge_bird(step);
        assert_eq!(runtime.clock.elapsed_nanos(), 1_438_000);
        let legacy = ReferenceCosts::dosbox_x_240(machine);
        let mut runtime = Runtime::with_costs(machine, legacy);
        runtime.charge_bird(step);
        assert_eq!(
            runtime.clock.elapsed_nanos(),
            u128::from(legacy.room_actor_gate + legacy.room_bird_draw)
        );
    }
}

#[test]
fn bookkeeping_is_additional_to_graphics_and_absent_from_the_older_profile() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut runtime = Runtime::new(machine);
        let work = crate::presentation::Work {
            bookkeeping: crate::presentation::Bookkeeping {
                progress: true,
                award: Some(crate::presentation::AwardWork {
                    bonus: 2700,
                    multiplier: None,
                }),
                life: Some(false),
            },
            ..Default::default()
        };
        let before = runtime.clock.elapsed_nanos();
        runtime.charge_presentation(work, 0);
        assert_eq!(runtime.clock.elapsed_nanos() - before, 2_005_100);
        let after = runtime.clock.elapsed_nanos();
        runtime.charge_presentation(Default::default(), 0);
        assert_eq!(runtime.clock.elapsed_nanos(), after);
        let mut older = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
        older.charge_presentation(work, 0);
        assert_eq!(older.clock.elapsed_nanos(), 0);
    }
}

#[test]
fn setup_crosses_timer_boundaries_once_without_resetting_ongoing_controls() {
    let mut runtime = Runtime::new(Machine::Cga);
    runtime.clock.advance_nanos(54_000_000);
    let before = runtime.clock.elapsed_nanos();
    let tick = runtime.clock.sample().bios.tick_low;
    runtime.enter_scene(3, false);
    assert_eq!(runtime.clock.sample().bios.tick_low, tick.wrapping_add(1));
    assert_eq!(
        runtime.clock.elapsed_nanos() - before,
        u128::from(runtime.costs.setup.unwrap().rooms[2])
    );
    runtime.controls.x = 1;
    let after = runtime.clock.elapsed_nanos();
    runtime.enter_scene(3, false);
    assert_eq!(runtime.clock.elapsed_nanos(), after);
    assert_eq!(runtime.controls.x, 1);
}

#[test]
fn setup_distinguishes_saved_alley_return_from_ground_entry() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut runtime = Runtime::new(machine);
        runtime.enter_scene(5, false);
        let before = runtime.clock.elapsed_nanos();
        runtime.enter_scene(0, false);
        assert_eq!(
            runtime.clock.elapsed_nanos() - before,
            u128::from(runtime.costs.setup.unwrap().ground_alley)
        );
        runtime.enter_scene(5, false);
        let before = runtime.clock.elapsed_nanos();
        runtime.enter_scene(0, true);
        assert_eq!(
            runtime.clock.elapsed_nanos() - before,
            u128::from(runtime.costs.setup.unwrap().saved_alley)
        );
        let mut older = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
        older.enter_scene(5, false);
        older.enter_scene(0, true);
        assert_eq!(older.clock.elapsed_nanos(), 0);
    }
}

#[test]
fn window_retrace_work_is_charged_without_recharging_quiet_countdown() {
    let mut data = vec![0; crate::assets::DATA_LENGTH];
    let mut screen = crate::framebuffer::Framebuffer::default();
    let player = crate::player::Player::at_alley_floor(8);
    data[0x166a] = 2;
    let quiet = crate::windows::advance(
        &mut data,
        &mut screen,
        &player,
        1,
        false,
        crate::windows::HoldTiming::default(),
    )
    .unwrap();
    let rejected = crate::windows::advance(
        &mut data,
        &mut screen,
        &player,
        1,
        true,
        crate::windows::HoldTiming::default(),
    )
    .unwrap();
    assert!(!quiet.accepted && !rejected.accepted);
    assert_eq!(data[0x1665], 0);
    let mut runtime = Runtime::new(Machine::Cga);
    let before = runtime.clock.elapsed_nanos();
    runtime.charge_window(quiet);
    assert_eq!(runtime.clock.elapsed_nanos(), before);
    runtime.charge_window(rejected);
    assert!(runtime.clock.elapsed_nanos() > before);
    let mut older = Runtime::with_costs(Machine::Cga, ReferenceCosts::dosbox_x_240(Machine::Cga));
    let before = older.clock.elapsed_nanos();
    older.charge_window(quiet);
    older.charge_window(rejected);
    assert_eq!(older.clock.elapsed_nanos(), before);
}

#[test]
fn calibrated_wipe_counts_zero_byte_work_without_double_charging_dispatch() {
    let mut wipe = crate::presentation::Wipe::new(160, 96, 0);
    let work = wipe
        .advance(
            &mut vec![0; crate::assets::DATA_LENGTH],
            &mut crate::framebuffer::Framebuffer::default(),
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
    assert_eq!(work.bytes_drawn, 0);
    let mut runtime = Runtime::new(Machine::Cga);
    let expected = runtime.costs.completion.unwrap().nanos(work.graphics);
    let poll = runtime.costs.room_effect_poll;
    let before = runtime.clock.elapsed_nanos();
    runtime.clock.advance_nanos(poll);
    runtime.charge_presentation(work, poll);
    assert_eq!(runtime.clock.elapsed_nanos() - before, u128::from(expected));
    assert!(expected > poll);

    let mut older = Runtime::with_costs(Machine::Cga, ReferenceCosts::dosbox_x_240(Machine::Cga));
    let poll = older.costs.room_effect_poll;
    let before = older.clock.elapsed_nanos();
    older.clock.advance_nanos(poll);
    older.charge_presentation(work, poll);
    assert_eq!(older.clock.elapsed_nanos() - before, u128::from(poll));
}

#[test]
fn uncalibrated_presentation_graphics_keep_the_byte_allowance() {
    let work = crate::presentation::Work {
        bytes_drawn: 384,
        ..crate::presentation::Work::default()
    };
    let mut runtime = Runtime::new(Machine::Cga);
    let before = runtime.clock.elapsed_nanos();
    runtime.charge_presentation(work, 0);
    assert_eq!(
        runtime.clock.elapsed_nanos() - before,
        384 * u128::from(runtime.costs.completion_draw_byte)
    );
}

#[test]
fn completion_poll_replaces_the_fallback_and_only_tone_work_is_added_later() {
    use crate::presentation::{Poll, PollWork, Work};
    for machine in [Machine::Cga, Machine::Pcjr] {
        for mut costs in [
            ReferenceCosts::ibm_xt(machine),
            ReferenceCosts::dosbox_x_240(machine),
        ] {
            costs.room_effect_poll = 1000;
            if let Some(c) = costs.completion_poll.as_mut() {
                c.melody_sound = 19;
                c.melody_tone = 23;
            }
            let mut r = Runtime::with_costs(machine, costs);
            assert_eq!(r.completion_poll_nanos(None, true), 1000);
            let poll = r.completion_poll_nanos(Some(Poll::Melody), true);
            assert_eq!(
                poll,
                if costs.completion_poll.is_some() {
                    19
                } else {
                    1000
                }
            );
            r.clock.advance_nanos(poll);
            r.charge_presentation(
                Work {
                    polling: Some(PollWork {
                        kind: Poll::Melody,
                        tone: true,
                    }),
                    ..Default::default()
                },
                poll,
            );
            let expected = if costs.completion_poll.is_some() {
                42
            } else {
                1000
            };
            assert_eq!(r.clock.elapsed_nanos(), expected);
            r.charge_presentation(
                Work {
                    polling: Some(PollWork {
                        kind: Poll::Melody,
                        tone: false,
                    }),
                    ..Default::default()
                },
                0,
            );
            assert_eq!(r.clock.elapsed_nanos(), expected);
        }
    }
}

#[test]
fn hearts_bookkeeping_is_separate_from_polling_drawing_and_legacy_costs() {
    let work = crate::presentation::Work {
        bytes_drawn: 384,
        hearts: crate::hearts_completion::PresentationWork {
            bookkeeping: Some(crate::hearts_completion::Bookkeeping::PairInit),
            ..Default::default()
        },
        ..Default::default()
    };
    for machine in [Machine::Cga, Machine::Pcjr] {
        for costs in [
            ReferenceCosts::ibm_xt(machine),
            ReferenceCosts::dosbox_x_240(machine),
        ] {
            let mut r = Runtime::with_costs(machine, costs);
            r.clock.advance_nanos(costs.room_effect_poll);
            r.charge_presentation(work, costs.room_effect_poll);
            let body = costs.hearts_presentation.map_or(0, |c| c.pair_init);
            let expected = costs.room_effect_poll + body + 384 * costs.completion_draw_byte;
            assert_eq!(r.clock.elapsed_nanos(), u128::from(expected));
            r.charge_presentation(Default::default(), 0);
            assert_eq!(r.clock.elapsed_nanos(), u128::from(expected));
        }
    }
}

#[test]
fn hearts_helpers_replace_only_their_own_byte_allowance() {
    let work = crate::presentation::Work {
        bytes_drawn: 280 + 384,
        hearts: crate::hearts_completion::PresentationWork {
            dance_frames: 1,
            dance_copies: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    for machine in [Machine::Cga, Machine::Pcjr] {
        let mut runtime = Runtime::new(machine);
        let body = runtime
            .costs
            .hearts_presentation
            .unwrap()
            .nanos(work.hearts);
        let before = runtime.clock.elapsed_nanos();
        runtime.charge_presentation(work, 0);
        assert_eq!(
            runtime.clock.elapsed_nanos() - before,
            u128::from(body + 384 * runtime.costs.completion_draw_byte)
        );
        let mut older = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
        let before = older.clock.elapsed_nanos();
        older.charge_presentation(work, 0);
        assert_eq!(
            older.clock.elapsed_nanos() - before,
            work.bytes_drawn as u128 * u128::from(older.costs.completion_draw_byte)
        );
    }
}

#[test]
fn pairing_omits_drawing_latency_without_losing_byte_accounting() {
    use crate::hearts_completion::{PresentationSprite, PresentationWork};

    for machine in [Machine::Cga, Machine::Pcjr] {
        for sprite in [
            PresentationSprite::PairSmall,
            PresentationSprite::PairMedium,
            PresentationSprite::ApproachCat,
        ] {
            for copies in [1, 8] {
                let mut hearts = PresentationWork {
                    pair_items: 8,
                    increasing_axes: 4,
                    decreasing_axes: 3,
                    upper_clamps: 1,
                    lower_clamps: 2,
                    ..Default::default()
                };
                hearts.sprites[sprite as usize] = copies;
                let mut runtime = Runtime::new(machine);
                let costs = runtime.costs.hearts_presentation.unwrap();
                let state = 8 * costs.pair_item
                    + 4 * costs.increasing_axis
                    + 3 * costs.decreasing_axis
                    + costs.upper_clamp
                    + 2 * costs.lower_clamp;
                let work = crate::presentation::Work {
                    bytes_drawn: hearts.bytes() + 384,
                    hearts,
                    ..Default::default()
                };
                runtime.charge_presentation(work, 0);
                assert_eq!(
                    runtime.clock.elapsed_nanos(),
                    u128::from(state + 384 * runtime.costs.completion_draw_byte)
                );

                // Outside pairing, exactly the same image work still costs
                // its measured allowance; the legacy profile uses bytes.
                let standalone = PresentationWork {
                    sprites: hearts.sprites,
                    ..Default::default()
                };
                assert_eq!(
                    costs.nanos(standalone),
                    u64::from(copies) * costs.sprites[sprite as usize]
                );
                let mut older = Runtime::with_costs(machine, ReferenceCosts::dosbox_x_240(machine));
                older.charge_presentation(work, 0);
                assert_eq!(
                    older.clock.elapsed_nanos(),
                    work.bytes_drawn as u128 * u128::from(older.costs.completion_draw_byte)
                );
            }
        }
    }
}

#[test]
fn historical_input_is_sampled_at_the_control_gate_not_pass_start() {
    use crate::control_history::{ControlHistory, HostChange, Key, Keys};
    use std::time::Duration;

    let pressed = {
        let mut keys = Keys::default();
        keys.set(Key::Right, true);
        keys
    };
    for machine in [Machine::Cga, Machine::Pcjr] {
        for (press, release, expected) in [(60, 80, 0), (100, 120, 1), (120, 140, 0)] {
            let mut history = ControlHistory::new(Keys::default());
            history.extend(
                150_000_000,
                Duration::from_millis(100),
                50_000_000,
                [
                    HostChange {
                        at_ns: press * 1_000_000,
                        keys: pressed,
                    },
                    HostChange {
                        at_ns: release * 1_000_000,
                        keys: Keys::default(),
                    },
                ],
            );
            let mut alley = Alley::synthetic();
            let mut runtime = Runtime::new(machine);
            // Deliberately make the pass begin before the 100 ms press,
            // while its actual control sample occurs at 110 ms.
            runtime.costs.prelude = 20_000_000;
            runtime
                .clock
                .advance_nanos(110_000_000 - runtime.costs.prelude - runtime.costs.control_bios);
            let mut samples = Vec::new();
            let event = runtime
                .advance_pass_with_input(&mut alley, |at| {
                    samples.push(at);
                    history.sample(at)
                })
                .unwrap();
            assert!(event.control_polled);
            assert_eq!(samples, [110_000_000]);
            assert_eq!(runtime.controls.x, expected);
        }
    }
}

#[test]
fn responsive_requests_refresh_between_native_polls_without_extra_clock_cost() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        let run = |responsive| {
            let mut alley = Alley::synthetic();
            let mut runtime = Runtime::new(machine);
            runtime.responsive_controls = responsive;
            runtime.controls_gate = ControlsGate::new(0);
            runtime.player_gate = PlayerGate::new(0, 0);
            let mut calls = 0;
            let event = runtime
                .advance_pass_with_input(&mut alley, |_| {
                    calls += 1;
                    (Controls { x: 1, y: 0 }, true)
                })
                .unwrap();
            assert!(!event.control_polled);
            (runtime.clock.elapsed_nanos(), runtime.controls, calls)
        };
        let original = run(false);
        let responsive = run(true);
        assert_eq!(original.0, responsive.0);
        assert_eq!(original.1, Controls::default());
        assert_eq!(original.2, 0);
        assert_eq!(responsive.1, Controls { x: 1, y: 0 });
        assert_eq!(responsive.2, 1);
    }
}

#[test]
fn responsive_constant_walking_keeps_movement_clock_and_frames_identical() {
    for machine in [Machine::Cga, Machine::Pcjr] {
        for direction in [-1, 0, 1] {
            let run = |responsive| {
                let mut alley = Alley::synthetic();
                let mut runtime = Runtime::new(machine);
                runtime.responsive_controls = responsive;
                let mut trace = Vec::new();
                while runtime.clock.elapsed_nanos() < 600_000_000 {
                    let event = runtime
                        .advance_pass_with_action(
                            &mut alley,
                            Controls { x: direction, y: 0 },
                            false,
                        )
                        .unwrap();
                    trace.push(format!("{event:?};{:?}", alley.player));
                }
                (
                    trace,
                    alley.framebuffer().bytes().to_vec(),
                    runtime.clock.elapsed_nanos(),
                )
            };
            assert_eq!(run(false), run(true));
        }
    }
}

#[test]
fn responsive_input_does_not_interrupt_native_blocking_effects() {
    let mut alley = Alley::synthetic();
    let mut runtime = Runtime::new(Machine::Cga);
    runtime.responsive_controls = true;
    runtime.fence = Some(FenceEffect::new(runtime.clock.sample().bios.tick_low));
    let event = runtime
        .advance_pass_with_input(&mut alley, |_| {
            panic!("blocking fence effect must not sample controls")
        })
        .unwrap();
    assert!(event.fence_wait);
}

#[test]
fn native_hearts_work_is_independent_of_host_recomposition() {
    let mut data = vec![0; crate::assets::DATA_LENGTH];
    crate::assets::set_word(&mut data, 0x45b8, 1);
    let mut step = crate::hearts::advance_cat(
        &mut data,
        &mut crate::framebuffer::Framebuffer::default(),
        1,
        &mut crate::sound::Sound::default(),
    )
    .unwrap();
    assert!(!step.accepted);
    // A host image redraw must not turn a rejected native call into an
    // active native drawing cost. The legacy profile retains its contract.
    step.drew = true;
    let mut runtime = Runtime::new(Machine::Cga);
    runtime.costs.hearts.as_mut().unwrap().gate = 12_345;
    let before = runtime.clock.elapsed_nanos();
    runtime.charge_hearts(step);
    assert_eq!(runtime.clock.elapsed_nanos() - before, 12_345);
    let costs = ReferenceCosts::dosbox_x_240(Machine::Cga);
    let expected = costs.room_actor_gate + costs.hearts_actor_draw;
    let mut older = Runtime::with_costs(Machine::Cga, costs);
    let before = older.clock.elapsed_nanos();
    older.charge_hearts(step);
    assert_eq!(older.clock.elapsed_nanos() - before, u128::from(expected));
}

#[test]
fn xt_single_support_beep_fits_the_whole_body_allowance() {
    let mut data = vec![0; crate::assets::DATA_LENGTH];
    data[0] = 0xff;
    for machine in [Machine::Cga, Machine::Pcjr] {
        data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
        let mut sound = crate::sound::Sound::default();
        sound.beep(&data, 1024, 6144);
        let nanos = sound.take_busy_ticks() * 1_000_000_000 / crate::timing::PIT_HZ;
        let costs = ReferenceCosts::ibm_xt(machine);
        assert!(nanos < costs.player_support_release);
        assert!(nanos < costs.player_landing);
    }
}

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
