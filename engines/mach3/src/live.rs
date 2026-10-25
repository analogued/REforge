// Copyright (c) 2026 analogued. Project license has not been selected.
//! Presentation bridge for budgeted gameplay on the caller's device timeline.
//!
//! The caller owns the clock across startup playback, gameplay and endings, and
//! converts its host simulation budget to PIT ticks. This bridge retains the last
//! published framebuffer while the foreground waits; audio observation remains
//! on the same clock and cannot advance gameplay independently.
//!
//! Expanding scene changes suspend the foreground and publish each region on
//! the same device timeline. Their fixed intervals normalize the original
//! CPU-dependent retrace polling; device and keyboard interrupts continue.

#[cfg(test)]
mod tests;

use crate::{
    Error,
    frame::Frame,
    gameplay::{
        Gameplay, SessionState,
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
    visible_palette: u8,
}

impl GameplayRuntime {
    #[cfg(feature = "viewer")]
    pub(crate) fn next_transition_deadline(&self) -> Option<u64> {
        self.task.next_transition_deadline()
    }

    pub fn begin<O: FnMut(Stamp, ClockEvent)>(
        game: Gameplay,
        state: GameplayState,
        game_cs: u16,
        pause_poll_period: u64,
        clock: &Clock<O>,
    ) -> Result<Self, Error> {
        let visible = game.frame().clone();
        let visible_palette = game.palette_family();
        let task = GameplayTask::begin(game, state, game_cs, pause_poll_period, clock)?
            .with_transition_period(reforge_audio::PIT_HZ / 60)?;
        Ok(Self {
            task,
            visible,
            visible_palette,
        })
    }

    pub fn game(&self) -> &Gameplay {
        self.task.game()
    }

    pub fn state(&self) -> &GameplayState {
        self.task.state()
    }

    pub fn visible_frame(&self) -> &Frame {
        &self.visible
    }

    pub fn palette_family(&self) -> u8 {
        self.visible_palette
    }

    /// Copy persistent controls back without changing session random state or
    /// retained world records, which belong to the caller's ending lifecycle.
    pub fn synchronize_session(&self, session: &mut SessionState) {
        let state = self.state();
        session.controls = state.controls.clone();
        session.keyboard = state.keyboard.clone();
        session.pause = state.pause.clone();
        session.joystick = state.joystick;
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

    pub fn schedule_joystick<O: FnMut(Stamp, ClockEvent)>(
        &mut self,
        tick: u64,
        mask: u8,
        clock: &Clock<O>,
    ) -> Result<(), Error> {
        self.task.schedule_joystick(tick, mask, clock)
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
        let progress = self.task.advance_until(target, resources, clock, observe);
        self.task.discard_transitions();
        let progress = progress?;
        if matches!(progress, GameplayProgress::Published { .. }) {
            self.visible = self.task.game().pass_frame().clone();
            self.visible_palette = self.task.game().palette_family();
        } else if matches!(progress, GameplayProgress::TransitionPublished { .. }) {
            let frame = self.task.transition_frame().expect("published transition");
            self.visible = frame.frame.clone();
            self.visible_palette = frame.palette_family;
        } else {
            // Function-key changes remain visible during pending native pause.
            self.visible_palette = self.task.game().palette_family();
        }
        Ok(progress)
    }

    pub fn into_parts(self) -> Result<(Gameplay, GameplayState), Error> {
        self.task.into_parts()
    }
    pub fn take_scheduled_joysticks(&mut self) -> std::collections::VecDeque<(u64, u8)> {
        self.task.take_scheduled_joysticks()
    }
}
