// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! One original interrupting score, rendered by the selected original sound device.
use crate::assets::{Assets, Result};
use crate::audio::{Music as Adlib, SoundDevice};
use crate::speaker_audio::Music as Speaker;

pub enum Music {
    Adlib(Adlib),
    Speaker(Speaker),
}
impl Music {
    pub fn song(assets: &Assets, id: u8, device: SoundDevice) -> Result<Self> {
        match device {
            SoundDevice::Adlib => Adlib::song(assets, id).map(Self::Adlib),
            SoundDevice::PcSpeaker => Speaker::song(assets, id).map(Self::Speaker),
        }
    }
    #[cfg(test)]
    pub fn from_sequencer(sequence: crate::music::Sequencer) -> Result<Self> {
        Adlib::from_sequencer(sequence).map(Self::Adlib)
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn play_data(&mut self, song: Vec<u8>) -> Result<()> {
        match self {
            Self::Adlib(music) => music.play_data(song),
            Self::Speaker(music) => music.play_data(song),
        }
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn stop(&mut self) -> Result<()> {
        match self {
            Self::Adlib(music) => music.stop(),
            Self::Speaker(music) => music.stop(),
        }
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn is_playing(&self) -> bool {
        match self {
            Self::Adlib(music) => music.is_playing(),
            Self::Speaker(music) => music.is_playing(),
        }
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn position_frames(&self) -> u64 {
        match self {
            Self::Adlib(music) => music.position_frames(),
            Self::Speaker(music) => music.position_frames(),
        }
    }
    #[cfg(any(feature = "sdl", test))]
    pub fn finished_at(&self) -> Option<u64> {
        match self {
            Self::Adlib(music) => music.finished_at(),
            Self::Speaker(music) => music.finished_at(),
        }
    }
    pub fn render(&mut self, output: &mut [i16]) -> Result<()> {
        match self {
            Self::Adlib(music) => music.render(output),
            Self::Speaker(music) => music.render(output),
        }
    }
}
