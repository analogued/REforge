// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Original single-player sound requests. Scene copies share this transient bus;
//! it is deliberately absent from DOS saves and never consumes campaign RNG.
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play(u8),
    Stop,
}

#[derive(Default)]
struct Requests {
    current: Option<u8>,
    pending: Vec<Command>,
}

#[derive(Clone, Default)]
pub struct Bus(Rc<RefCell<Requests>>);
impl Bus {
    pub fn play(&self, id: u8) {
        assert!(id < 17);
        let mut requests = self.0.borrow_mut();
        // Resident 5D29 compares the requested score, even after it has ended.
        if requests.current != Some(id) {
            requests.current = Some(id);
            requests.pending.push(Command::Play(id));
            #[cfg(feature = "dev-tools")]
            crate::record::trace_sound(Some(id));
        }
    }
    pub fn stop(&self) {
        let mut requests = self.0.borrow_mut();
        requests.current = None;
        requests.pending.push(Command::Stop);
        #[cfg(feature = "dev-tools")]
        crate::record::trace_sound(None);
    }
    pub fn checkpoint(&self) -> usize {
        self.0.borrow().pending.len()
    }
    pub fn driver_cost_since(&self, checkpoint: usize, startup: &[u64; 17]) -> u64 {
        self.0.borrow().pending[checkpoint..]
            .iter()
            .map(|command| match command {
                Command::Play(id) => startup[usize::from(*id)],
                Command::Stop => 26_000,
            })
            .sum()
    }
    #[cfg(feature = "dev-tools")]
    pub fn pending(&self) -> Vec<Command> {
        self.0.borrow().pending.clone()
    }
    pub fn drain(&self) -> Vec<Command> {
        std::mem::take(&mut self.0.borrow_mut().pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_copies_share_interruptions_and_repeated_track_suppression() {
        let bus = Bus::default();
        let scene = bus.clone();
        bus.play(4);
        scene.play(4);
        scene.play(7);
        scene.stop();
        bus.play(7);
        assert_eq!(
            bus.drain(),
            [
                Command::Play(4),
                Command::Play(7),
                Command::Stop,
                Command::Play(7)
            ]
        );
        assert!(scene.drain().is_empty());
    }
}
