// Copyright (c) 2026 analogued. Project license has not been selected.
//! Shared continuations for immediate stepping and timer-driven execution.

use super::{Opening, Steering, enemy::EnemyUpdate, player::PlayerUpdate, special::SpecialContact};
use crate::{
    Error,
    assets::GraphicsAssets,
    sound::{self, Effect, Foreground, GameplayAction},
};

#[derive(Debug)]
enum Stage {
    Player,
    DeathSecondWait,
    DeathReturn,
    World,
    Enemies(EnemyUpdate),
    FinishWorld(WorldCompletion),
    Compose,
    Publish,
    Tail,
    Commands(CommandTask),
    FinishPass,
    Returned,
}

#[derive(Debug)]
pub(super) struct PassTask {
    pass: u8,
    stage: Stage,
}

#[derive(Debug)]
pub(super) enum PassProgress {
    /// A seven-region transition has been queued; resume its source caller afterward.
    Transition,
    Action(GameplayAction),
    Publish {
        wait_required: bool,
    },
    Returned,
}

/// Collision contacts are captured once, before any progression edge clears records.
#[derive(Debug)]
pub(super) struct WorldCompletion {
    pub(super) contacts: [Option<SpecialContact>; 5],
    pub(super) next_contact: usize,
    pub(super) follow_goal: bool,
}

impl WorldCompletion {
    pub(super) fn new(contacts: [Option<SpecialContact>; 5]) -> Self {
        Self {
            contacts,
            next_contact: 0,
            follow_goal: false,
        }
    }
}

/// Negative-delay parsing can span several transition returns in one invocation.
#[derive(Debug, Default)]
pub(super) struct CommandTask {
    pub(super) visited: std::collections::HashSet<(u16, u16, u16)>,
}

impl PassTask {
    pub(super) fn begin(game: &mut Opening) -> Self {
        let pass = if game.at_body_boundary() {
            1
        } else {
            game.render_pass + 1
        };
        if pass == 1 {
            game.body = game.body.wrapping_add(1);
        }
        game.updating_before_render = true;
        Self {
            pass,
            stage: Stage::Player,
        }
    }

    pub(super) fn resume(
        &mut self,
        game: &mut Opening,
        assets: &GraphicsAssets,
        input: Steering,
        sound: &mut Foreground<'_>,
    ) -> Result<PassProgress, Error> {
        loop {
            match &mut self.stage {
                Stage::Player => {
                    match game.player.update_begin(input, sound)? {
                        PlayerUpdate::Complete(reset) => game.finish_player(reset),
                        PlayerUpdate::Death => {
                            self.stage = Stage::DeathSecondWait;
                            return Ok(PassProgress::Action(GameplayAction::Wait {
                                site: 0x25f9,
                                units: 1,
                            }));
                        }
                    }
                    self.stage = Stage::World;
                }
                Stage::DeathSecondWait => {
                    self.stage = Stage::DeathReturn;
                    return Ok(PassProgress::Action(GameplayAction::Wait {
                        site: 0x2602,
                        units: 20,
                    }));
                }
                Stage::DeathReturn => {
                    sound::request(sound, 0x2607, Effect::Reset);
                    let reset = game.player.finish_death();
                    game.finish_player(reset);
                    self.stage = Stage::World;
                }
                Stage::World => {
                    self.stage = if self.pass == 2 {
                        Stage::Compose
                    } else {
                        Stage::Enemies(game.enemy.begin_update(
                            game.player.snapshot().coordinates,
                            game.enemy_context()?,
                        ))
                    };
                }
                Stage::Enemies(task) => {
                    if let Some(action) = game.enemy.resume_update(task, &mut game.random, sound)? {
                        return Ok(PassProgress::Action(action));
                    }
                    let effects = task.effects();
                    self.stage =
                        Stage::FinishWorld(game.begin_finish_world(assets, effects, sound)?);
                }
                Stage::FinishWorld(task) => {
                    if game.resume_finish_world(assets, self.pass == 1, task, sound)? {
                        return Ok(PassProgress::Transition);
                    }
                    self.stage = Stage::Compose;
                }
                Stage::Compose => {
                    game.player.project(assets, game.vertical_origin)?;
                    if self.pass == 1 {
                        let depths = game.world_depths();
                        game.sort_world(&depths);
                        game.sort_full_world(&depths);
                        game.hud.set_orientation(game.player.snapshot().orientation);
                        game.hud.set_progression(game.progression);
                        game.hud.set_score_multiplier(1);
                        if let Some(rules) = &game.rules {
                            game.hud
                                .set_marker_styles(assets, rules.marker_styles(game.difficulty)?)?;
                        }
                        let effects = game.hud.update_with_weapon(
                            game.body,
                            &mut game.random,
                            game.weapon.snapshot().counter,
                        )?;
                        if let Some(counter) = effects.weapon_counter {
                            game.weapon.set_counter(counter);
                        }
                        if let Some(damage) = effects.damage {
                            game.player.hit_with_sound(damage, sound);
                        }
                        game.advance_strip();
                        // Keep the established snapshot checkpoint placement.
                        // This is an atomic foreground approximation, not an
                        // instruction-by-instruction reconstruction of 2208.
                        game.advance_speed();
                    }
                    game.render_pass = self.pass;
                    game.render(assets)?;
                    self.stage = Stage::Publish;
                    return Ok(PassProgress::Publish {
                        wait_required: self.pass == 1 && game.pass_one_wait,
                    });
                }
                Stage::Publish => {
                    return Err(Error::InvalidData(
                        "publish the prepared pass before resuming it".into(),
                    ));
                }
                Stage::Tail => {
                    if self.pass == 1 {
                        sound::request(
                            sound,
                            0x067b,
                            Effect::Noise {
                                divisor: game.noise_divisor as u16,
                            },
                        );
                    }
                    let transition = game.begin_finish_pass(assets)?;
                    self.stage = if self.pass == 3 {
                        Stage::Commands(CommandTask::default())
                    } else {
                        Stage::Returned
                    };
                    if transition {
                        return Ok(PassProgress::Transition);
                    }
                }
                Stage::Commands(task) => {
                    if game.rules.is_some() && game.resume_commands(assets, task, sound)? {
                        return Ok(PassProgress::Transition);
                    }
                    self.stage = Stage::FinishPass;
                }
                Stage::FinishPass => {
                    game.finish_pass(sound)?;
                    self.stage = Stage::Returned;
                }
                Stage::Returned => return Ok(PassProgress::Returned),
            }
        }
    }

    pub(super) fn publish(&mut self, game: &mut Opening) {
        assert!(matches!(self.stage, Stage::Publish));
        game.pass_frame = game.frame.clone();
        game.pass_snapshot = Some(Box::new(game.snapshot()));
        game.updating_before_render = false;
        self.stage = Stage::Tail;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opening::{SessionState, special::SpecialTransition, tests::*};

    fn game(assets: &GraphicsAssets, commands: &[u8]) -> Opening {
        let mut data = synthetic_rules(commands).data_segment().to_vec();
        // These tests exercise negative delays at difficulty three, whose HUD
        // marker family must also have distinct synthetic sprite sources.
        data[0x04b5..0x04b8].copy_from_slice(&[1, 2, 3]);
        Opening::for_play(
            assets,
            crate::opening::ScriptRules::from_data_segment(&data).unwrap(),
            SessionState::default().begin_game(),
        )
        .unwrap()
    }

    #[test]
    fn parser_returns_each_transition_before_the_next_command_or_sound() {
        let assets = synthetic_assets();
        let mut game = game(&assets, &[0xc7, 0xcb, 0xcc, 0xd5, 0xd6, 0xc4, 0x85]);
        game.difficulty = 3;
        let mut task = PassTask {
            pass: 3,
            stage: Stage::Commands(CommandTask::default()),
        };
        let mut actions = Vec::new();
        for cursor in 1..=5 {
            assert!(matches!(
                task.resume(&mut game, &assets, Steering::default(), &mut |action| {
                    actions.push(action)
                })
                .unwrap(),
                PassProgress::Transition
            ));
            assert_eq!(game.script_cursor, cursor);
            assert_eq!(game.script_delay, 0);
            assert!(!game.enemy.active());
            assert!(actions.is_empty());
            let frames = game.take_transitions();
            assert_eq!(frames.len(), 7);
            assert!(
                frames
                    .iter()
                    .all(|frame| { frame.state.as_ref().unwrap().script_cursor == cursor })
            );
        }
        assert!(matches!(
            task.resume(&mut game, &assets, Steering::default(), &mut |action| {
                actions.push(action)
            })
            .unwrap(),
            PassProgress::Returned
        ));
        assert!(matches!(
            actions[0],
            GameplayAction::Request { site: 0x1cf1, .. }
        ));
        assert_eq!(game.script_cursor, 7);
        assert!(game.enemy.active());
    }

    #[test]
    fn parser_cycle_guard_survives_transition_returns() {
        let assets = synthetic_assets();
        let mut game = game(&assets, &[0xc7]);
        game.difficulty = 3;
        let first_script = game.script_index;
        let mut task = PassTask {
            pass: 3,
            stage: Stage::Commands(CommandTask::default()),
        };
        for _ in 0..21 {
            assert!(matches!(
                task.resume(&mut game, &assets, Steering::default(), &mut |_| {})
                    .unwrap(),
                PassProgress::Transition
            ));
            assert_eq!(game.take_transitions().len(), 7);
        }
        let error = task
            .resume(&mut game, &assets, Steering::default(), &mut |_| {})
            .unwrap_err();
        assert!(error.to_string().contains("command cycle"));
        assert_eq!(game.script_index, first_script);
        assert_eq!(game.script_cursor, 0);
    }

    #[test]
    fn overlay_expiry_returns_before_parser_mutation() {
        let assets = synthetic_assets();
        let mut game = game(&assets, &[0xc7, 0xc4, 0x85]);
        game.render_pass = 3;
        game.overlay = 80;
        game.script_delay = 20;
        let mut task = PassTask {
            pass: 3,
            stage: Stage::Tail,
        };
        let mut actions = Vec::new();
        for (cursor, delay) in [(0, 20), (1, 0)] {
            assert!(matches!(
                task.resume(&mut game, &assets, Steering::default(), &mut |action| {
                    actions.push(action)
                })
                .unwrap(),
                PassProgress::Transition
            ));
            assert_eq!((game.script_cursor, game.script_delay), (cursor, delay));
            assert_eq!(game.overlay, 0);
            assert!(actions.is_empty());
            assert_eq!(game.take_transitions().len(), 7);
        }
    }

    #[test]
    fn world_transition_returns_before_goal_edge_weapon_and_composition() {
        let assets = synthetic_assets();
        let mut game = game(&assets, &[0x85]);
        let mut records = game.special.snapshots();
        records[0].active = true;
        game.special = crate::opening::special::Special::from_snapshots(records);
        game.body = 1;
        let mut task = PassTask {
            pass: 1,
            stage: Stage::FinishWorld(WorldCompletion::new([
                Some(SpecialContact::Award {
                    site: 0x372b,
                    points: 1000,
                    transition: Some(SpecialTransition::Normal),
                }),
                // Already captured contacts must survive the edge clearing records.
                Some(SpecialContact::Award {
                    site: 0x37a3,
                    points: 550,
                    transition: None,
                }),
                None,
                None,
                None,
            ])),
        };
        game.weapon.set_counter(223);
        let old_weapon = game.weapon.snapshot().counter;
        let mut actions = Vec::new();
        assert!(matches!(
            task.resume(&mut game, &assets, Steering::default(), &mut |action| {
                actions.push(action)
            })
            .unwrap(),
            PassProgress::Transition
        ));
        assert_eq!(game.progression, 1);
        assert_eq!(game.render_pass, 0);
        assert!(game.special.snapshots()[0].active);
        assert_eq!(game.weapon.snapshot().counter, old_weapon);
        assert_eq!(actions.len(), 1);
        let frames = game.take_transitions();
        assert_eq!(frames.len(), 7);
        assert!(frames.iter().all(|frame| {
            let state = frame.state.as_ref().unwrap();
            state.progression == 1 && state.render_pass == 0 && state.special[0].active
        }));
        assert!(matches!(
            task.resume(&mut game, &assets, Steering::default(), &mut |action| {
                actions.push(action)
            })
            .unwrap(),
            PassProgress::Publish { .. }
        ));
        assert_eq!(game.progression, 2);
        assert_eq!(game.render_pass, 1);
        assert!(!game.special.snapshots()[0].active);
        assert_ne!(game.weapon.snapshot().counter, old_weapon);
        assert!(matches!(
            actions[1],
            GameplayAction::Request { site: 0x37a3, .. }
        ));
        assert_eq!(game.snapshot().hud.pending_score, 1550);
    }

    #[test]
    fn resumed_passes_match_immediate_stepping_with_all_frames_retained() {
        let mut assets = synthetic_assets();
        assets.primary.fill(0x55);
        assets.alternate.fill(0xaa);
        let prepare = || {
            let mut game = game(&assets, &[0xcb, 0xcc, 0xd6, 0xc3, 0x85]);
            game.overlay = 80;
            game.difficulty = 3;
            game
        };
        let mut immediate = prepare();
        let mut resumed = prepare();
        let mut immediate_actions = Vec::new();
        let mut resumed_actions = Vec::new();
        let mut transitions = 0;
        for _ in 0..3 {
            immediate
                .advance_subframe_with_sound(&assets, Steering::default(), &mut |action| {
                    immediate_actions.push(action)
                })
                .unwrap();
            let mut task = PassTask::begin(&mut resumed);
            loop {
                match task
                    .resume(&mut resumed, &assets, Steering::default(), &mut |action| {
                        resumed_actions.push(action)
                    })
                    .unwrap()
                {
                    PassProgress::Action(action) => resumed_actions.push(action),
                    PassProgress::Transition => transitions += 1,
                    PassProgress::Publish { .. } => task.publish(&mut resumed),
                    PassProgress::Returned => break,
                }
            }
            assert_eq!(
                format!("{:?}", immediate.snapshot()),
                format!("{:?}", resumed.snapshot())
            );
            assert_eq!(immediate.frame(), resumed.frame());
            assert_eq!(immediate.pass_frame(), resumed.pass_frame());
            assert_eq!(
                format!("{:?}", immediate.pass_snapshot()),
                format!("{:?}", resumed.pass_snapshot())
            );
        }
        assert_eq!(transitions, 4);
        assert_eq!(immediate_actions, resumed_actions);
        let immediate_frames = immediate.take_transitions();
        let resumed_frames = resumed.take_transitions();
        assert_eq!(immediate_frames.len(), 28);
        assert_eq!(immediate_frames.len(), resumed_frames.len());
        for (immediate, resumed) in immediate_frames.iter().zip(&resumed_frames) {
            assert_eq!(immediate.frame, resumed.frame);
            assert_eq!(
                (
                    immediate.clip.x_bytes,
                    immediate.clip.y,
                    immediate.clip.width_bytes,
                    immediate.clip.height
                ),
                (
                    resumed.clip.x_bytes,
                    resumed.clip.y,
                    resumed.clip.width_bytes,
                    resumed.clip.height
                ),
            );
            assert_eq!(immediate.wait_count, resumed.wait_count);
            assert_eq!(immediate.before_pass, resumed.before_pass);
            assert_eq!(immediate.palette_family, resumed.palette_family);
            assert_eq!(
                format!("{:?}", immediate.state),
                format!("{:?}", resumed.state)
            );
        }
    }
}
