// Copyright (c) 2026 analogued. Project license has not been selected.
//! Shared continuations for immediate stepping and timer-driven execution.

use super::{Opening, Steering, enemy::EnemyUpdate, player::PlayerUpdate};
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
    Compose,
    Publish,
    Tail,
    Returned,
}

#[derive(Debug)]
pub(super) struct PassTask {
    pass: u8,
    stage: Stage,
}

pub(super) enum PassProgress {
    Action(GameplayAction),
    Publish { wait_required: bool },
    Returned,
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
                    game.finish_world(assets, self.pass == 1, effects, sound)?;
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
                    game.finish_pass(assets, sound)?;
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
