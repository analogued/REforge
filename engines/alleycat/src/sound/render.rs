// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared PIT/speaker and tone-PSG execution with band-limited PCM.
//! Game sequencing remains in the engine sound programs.
use super::{Category, Write};
use reforge_audio::{
    PIT_HZ,
    category::{CategoryVolumes, SoundCategory},
    pit::CountOnePolicy,
    psg::TonePsg,
    render::{CategorizedRenderer, Config},
    speaker::PcSpeaker,
};

pub const RATE: u32 = 48_000;
struct SpeakerRenderer {
    speaker: PcSpeaker,
    renderer: CategorizedRenderer,
    category: SoundCategory,
}
impl SpeakerRenderer {
    pub fn new(volumes: CategoryVolumes) -> Result<Self, String> {
        let mut renderer = CategorizedRenderer::new(
            Config {
                clock_hz: PIT_HZ as u32,
                sample_rate: RATE,
                chunk_ticks: (PIT_HZ / 100) as u32,
            },
            0,
        )
        .map_err(|e| e.to_string())?;
        renderer.set_volumes(volumes).map_err(|e| e.to_string())?;
        Ok(Self {
            speaker: PcSpeaker::with_mode3_count_one(0, [true; 3], CountOnePolicy::DosBoxWrap),
            renderer,
            category: SoundCategory::Effects,
        })
    }
    pub fn set_volumes(&mut self, volumes: CategoryVolumes) -> Result<(), String> {
        self.renderer
            .set_volumes(volumes)
            .map_err(|e| e.to_string())
    }
    fn advance(&mut self, target: u64, pcm: &mut Vec<i16>) -> Result<(), String> {
        while self.speaker.pit().tick() < target {
            let events = self.speaker.advance_to(target).map_err(|e| e.to_string())?;
            if let Some(edge) = events.drive {
                self.renderer
                    .level_at(
                        edge.tick,
                        self.category,
                        if edge.high { 8192 } else { 0 },
                        &mut |block| {
                            pcm.extend_from_slice(block);
                            Ok(())
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        writes: impl IntoIterator<Item = Write>,
        until: u64,
    ) -> Result<Vec<i16>, String> {
        let mut pcm = Vec::new();
        for write in writes {
            if write.tick > until || write.tick < self.speaker.pit().tick() {
                return Err("speaker event outside committed interval".into());
            }
            self.advance(write.tick, &mut pcm)?;
            self.category = match write.category {
                Category::Music => SoundCategory::Music,
                Category::Effects => SoundCategory::Effects,
            };
            let events = self
                .speaker
                .write_port(write.port, write.value)
                .map_err(|e| e.to_string())?;
            self.renderer
                .level_at(
                    write.tick,
                    self.category,
                    if self.speaker.drive() { 8192 } else { 0 },
                    &mut |block| {
                        pcm.extend_from_slice(block);
                        Ok(())
                    },
                )
                .map_err(|e| e.to_string())?;
            debug_assert!(events.tick == write.tick);
        }
        self.advance(until, &mut pcm)?;
        self.renderer
            .finish_until(until, &mut |block| {
                pcm.extend_from_slice(block);
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(pcm)
    }
}

struct PsgRenderer {
    device: TonePsg,
    renderer: CategorizedRenderer,
}

impl PsgRenderer {
    fn new(volumes: CategoryVolumes) -> Result<Self, String> {
        let mut renderer = CategorizedRenderer::new(
            Config {
                clock_hz: PIT_HZ as u32,
                sample_rate: RATE,
                chunk_ticks: (PIT_HZ / 100) as u32,
            },
            0,
        )
        .map_err(|e| e.to_string())?;
        renderer.set_volumes(volumes).map_err(|e| e.to_string())?;
        Ok(Self {
            device: TonePsg::new(),
            renderer,
        })
    }

    fn advance(&mut self, target: u64, pcm: &mut Vec<i16>) -> Result<(), String> {
        let master_target = target.checked_mul(3).ok_or("PSG clock overflow")?;
        while self.device.tick() < master_target {
            if let Some(edge) = self.device.advance_to(master_target)? {
                // The chip has three input clocks per PIT clock. Retain that
                // exact counter phase, quantizing only presentation timestamps
                // down to the shared renderer's integer PIT-clock resolution.
                self.renderer
                    .level_at(
                        edge.tick / 3,
                        SoundCategory::Music,
                        edge.level,
                        &mut |block| {
                            pcm.extend_from_slice(block);
                            Ok(())
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn render(&mut self, writes: &[Write], until: u64) -> Result<Vec<i16>, String> {
        let mut pcm = Vec::new();
        for write in writes {
            if write.tick > until || write.tick < self.device.tick().div_ceil(3) {
                return Err("PSG event outside committed interval".into());
            }
            self.advance(write.tick, &mut pcm)?;
            self.device.write(write.value)?;
            self.renderer
                .level_at(
                    write.tick,
                    SoundCategory::Music,
                    self.device.level(),
                    &mut |block| {
                        pcm.extend_from_slice(block);
                        Ok(())
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        self.advance(until, &mut pcm)?;
        self.renderer
            .finish_until(until, &mut |block| {
                pcm.extend_from_slice(block);
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(pcm)
    }
}

/// Independent deterministic speaker and three-tone paths mixed after semantic
/// category gain. Silent PSG output leaves the legacy speaker PCM unchanged.
pub struct Renderer {
    speaker: SpeakerRenderer,
    psg: PsgRenderer,
}

impl Renderer {
    pub fn new(volumes: CategoryVolumes) -> Result<Self, String> {
        Ok(Self {
            speaker: SpeakerRenderer::new(volumes)?,
            psg: PsgRenderer::new(volumes)?,
        })
    }

    pub fn set_volumes(&mut self, volumes: CategoryVolumes) -> Result<(), String> {
        self.speaker.set_volumes(volumes)?;
        self.psg
            .renderer
            .set_volumes(volumes)
            .map_err(|e| e.to_string())
    }

    pub fn render(
        &mut self,
        writes: impl IntoIterator<Item = Write>,
        until: u64,
    ) -> Result<Vec<i16>, String> {
        let (psg, speaker): (Vec<_>, Vec<_>) =
            writes.into_iter().partition(|write| write.port == 0xc0);
        let mut pcm = self.speaker.render(speaker, until)?;
        let music = self.psg.render(&psg, until)?;
        if pcm.len() != music.len() {
            return Err("Speaker/PSG synthesis sample positions diverged".into());
        }
        for (sample, music) in pcm.iter_mut().zip(music) {
            *sample = sample.saturating_add(music);
        }
        Ok(pcm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::set_word;
    use crate::sound::Sound;
    #[test]
    fn psg_music_and_speaker_effects_mix_independently_and_keep_chunk_phase() {
        let mut sound = Sound::default();
        sound.record(true);
        sound.tone(1200, Category::Effects);
        for value in [0x93, 0x88, 0x12, 0xb3, 0xa0, 0x18, 0xd3, 0xc0, 0x20] {
            sound.write(0xc0, value, Category::Music);
        }
        let events: Vec<_> = sound.drain().collect();
        let until = PIT_HZ / 3;
        let render = |volumes| {
            Renderer::new(CategoryVolumes(volumes))
                .unwrap()
                .render(events.clone(), until)
                .unwrap()
        };
        let normal = render([100; 3]);
        let music = render([100, 100, 0]);
        let effects = render([0, 100, 100]);
        assert!(music.iter().any(|v| v.abs() > 100));
        assert!(effects.iter().any(|v| v.abs() > 100));
        assert_eq!(normal.len(), music.len());
        assert!(render([0, 100, 0]).iter().all(|v| *v == 0));
        let expected_effects = SpeakerRenderer::new(CategoryVolumes::default())
            .unwrap()
            .render(events.iter().copied().filter(|w| w.port != 0xc0), until)
            .unwrap();
        assert_eq!(effects, expected_effects);
        let mut split = Renderer::new(CategoryVolumes::default()).unwrap();
        let mut actual = split.render(events, PIT_HZ / 7).unwrap();
        actual.extend(split.render([], until).unwrap());
        assert_eq!(normal, actual);
    }

    #[test]
    fn background_music_and_movement_effects_obey_separate_volume_controls() {
        for (scene, running, broom, falling, tail, category) in [
            (0, 0, 0, 0, false, SoundCategory::Music),
            (4, 0, 0, 0, false, SoundCategory::Music),
            (0, 0, 0, 0, true, SoundCategory::Music),
            (4, 0, 0, 0, true, SoundCategory::Music),
            (0, 1, 0, 0, false, SoundCategory::Effects),
            (4, 1, 0, 0, false, SoundCategory::Effects),
            (0, 0, 1, 0, false, SoundCategory::Effects),
            (0, 0, 0, 2, false, SoundCategory::Effects),
        ] {
            let mut data = vec![0; crate::assets::DATA_LENGTH];
            data[0] = 255;
            set_word(&mut data, 4, scene);
            data[0x584] = running;
            data[0x1cbf] = broom;
            data[0x1673] = falling;
            data[0x5927] = 0x80;
            data[0x592c] = 255;
            set_word(&mut data, 0x592a, 0x500);
            set_word(&mut data, 0x5a44, 1000);
            for address in [0x5a02, 0x5a04, 0x5a06, 0x5a08] {
                set_word(&mut data, address, 0x2000);
            }
            if tail {
                data[0x592d] = 1;
            } else {
                data[0x59c2] = 0xff;
            }
            let mut sound = Sound::default();
            sound.record(true);
            sound.ambient(&mut data, 1);
            let events: Vec<_> = sound.drain().collect();
            let end = sound.tick() + PIT_HZ / 20;
            let render = |levels| {
                Renderer::new(CategoryVolumes(levels))
                    .unwrap()
                    .render(events.clone(), end)
                    .unwrap()
            };
            let normal = render([100; 3]);
            assert!(
                normal.iter().any(|v| v.abs() > 100),
                "silent fixture: scene={scene}, running={running}, broom={broom}, falling={falling}, tail={tail}"
            );
            let mut muted = [100; 3];
            muted[category.index()] = 0;
            assert!(render(muted).iter().all(|v| *v == 0));
            muted = [100; 3];
            let other = if category == SoundCategory::Music {
                SoundCategory::Effects
            } else {
                SoundCategory::Music
            };
            muted[other.index()] = 0;
            assert_eq!(normal, render(muted));
        }
    }
    #[test]
    fn category_mute_changes_pcm_without_changing_the_port_program_or_sample_count() {
        let mut sound = Sound::default();
        sound.record(true);
        sound.tone(1000, Category::Music);
        sound.at(PIT_HZ / 10);
        sound.stop();
        sound.at(PIT_HZ / 5);
        sound.tone(1500, Category::Effects);
        sound.at(PIT_HZ * 3 / 10);
        sound.stop();
        let events: Vec<_> = sound.drain().collect();
        let end = PIT_HZ * 4 / 10;
        let normal = Renderer::new(CategoryVolumes::default())
            .unwrap()
            .render(events.clone(), end)
            .unwrap();
        let muted = Renderer::new(CategoryVolumes([0, 100, 100]))
            .unwrap()
            .render(events.clone(), end)
            .unwrap();
        assert_eq!(normal.len(), muted.len());
        assert!(normal[1000..4000].iter().any(|v| v.abs() > 100));
        assert!(muted[1000..4000].iter().all(|v| *v == 0));
        assert!(muted[11000..14000].iter().any(|v| v.abs() > 100));
        let mut split = Renderer::new(CategoryVolumes::default()).unwrap();
        let cutoff = PIT_HZ / 10;
        let mut pcm = split
            .render(events.iter().copied().filter(|e| e.tick <= cutoff), cutoff)
            .unwrap();
        pcm.extend(
            split
                .render(events.iter().copied().filter(|e| e.tick > cutoff), end)
                .unwrap(),
        );
        assert_eq!(normal, pcm);
    }
}
