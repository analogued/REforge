// Copyright (c) 2026 analogued. Project license has not been selected.
//! Presentation bridge for budgeted gameplay on the caller's device timeline.
//!
//! The caller owns the clock across startup playback, gameplay and endings, and
//! converts its host simulation budget to PIT ticks. This bridge retains the last
//! published framebuffer while the foreground waits; audio observation remains
//! on the same clock and cannot advance gameplay independently.

#[cfg(test)]
mod tests;

use crate::{
    Error,
    frame::Frame,
    opening::{
        Opening, SessionState,
        execution::{
            GameplayEvent, GameplayProgress, GameplayResources, GameplayState, GameplayTask,
        },
    },
    sound::SampleMemory,
    timer::{Clock, ClockEvent, Stamp},
};

/// A live gameplay continuation and its most recently published image.
pub struct GameplayRuntime {
    task: GameplayTask,
    visible: Frame,
}

impl GameplayRuntime {
    pub fn begin<O: FnMut(Stamp, ClockEvent)>(
        game: Opening,
        state: GameplayState,
        game_cs: u16,
        pause_poll_period: u64,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        let visible = game.frame().clone();
        let task = GameplayTask::begin(game, state, game_cs, pause_poll_period, clock)?;
        Ok(Self { task, visible })
    }

    pub fn game(&self) -> &Opening {
        self.task.game()
    }

    pub fn state(&self) -> &GameplayState {
        self.task.state()
    }

    pub fn visible_frame(&self) -> &Frame {
        &self.visible
    }

    /// Copy persistent controls back without changing session random state or
    /// retained world records, which belong to the caller's ending lifecycle.
    pub fn synchronize_session(&self, session: &mut SessionState) {
        let state = self.state();
        session.controls = state.controls.clone();
        session.keyboard = state.keyboard.clone();
        session.pause = state.pause.clone();
    }

    pub fn schedule_scan<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        tick: u64,
        raw: u8,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        self.task.schedule_scan(tick, raw, clock)
    }

    pub fn clear_input<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        self.task.clear_input(clock)
    }

    /// Stop at one publication or return so the caller can present each image
    /// and retain unused budget for the next continuation.
    pub fn advance_until<O: FnMut(Stamp, ClockEvent), M: SampleMemory>(
        &mut self,
        target: u64,
        resources: &GameplayResources<'_, M>,
        clock: &mut Clock<O>,
        observe: &mut impl FnMut(u64, GameplayEvent),
    ) -> Result<GameplayProgress, Error> {
        let progress = self.task.advance_until(target, resources, clock, observe)?;
        if matches!(progress, GameplayProgress::Published { .. }) {
            self.visible = self.task.game().pass_frame().clone();
        }
        Ok(progress)
    }

    pub fn into_parts(self) -> Result<(Opening, GameplayState), Error> {
        self.task.into_parts()
    }
}
